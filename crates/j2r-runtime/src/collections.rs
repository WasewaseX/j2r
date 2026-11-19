//! `java.util` collections — Wave 2 of M3 (`docs/M3_PLAN.md` §5).
//!
//! **Everything is heap-resident.** Every collection is an ordinary
//! [`JObj`](crate::object::JObj)
//! whose state slots hold scalars and references to further heap objects
//! (backing arrays, `HashMap$Node` bins, iterators). The mark/sweep
//! collector therefore scans collections with **zero new root machinery** —
//! the strongest possible GC story (M3_PLAN §5.4). No `Arc<RefCell<..>>` side
//! tables exist anywhere.
//!
//! Slot layouts (documented here because the natives own them):
//!
//! * `java/util/ArrayList` — slot 0 `size` (Int), slot 1 `modCount` (Int),
//!   slot 2 elements reference (a `[Ljava/lang/Object;` heap array whose
//!   slot 0 is the capacity).
//! * `java/util/ArrayList$Itr` — slot 0 list reference, slot 1 `cursor`
//!   (Int), slot 2 `expectedModCount` (Int).
//! * `java/util/HashMap` — slot 0 `size` (Int), slot 1 `modCount` (Int),
//!   slot 2 `threshold` (Int), slot 3 table reference or null (a
//!   `[Ljava/util/HashMap$Node;` array whose slot 0 is the table length).
//! * `java/util/HashMap$Node` — slot 0 `hash` (Int), slot 1 key, slot 2
//!   value, slot 3 next-node reference or null, slot 4 the prim tag
//!   (G3.3 slice 3: `Void` = a faithful node; `Int(desc-char)` = the
//!   value slot holds a RAW prim payload written by `hm_put_prim`, to be
//!   materialized into its box at the first faithful read).
//! * `java/util/HashMap$KeyIterator` — slot 0 map reference, slot 1 next
//!   table index to scan (Int), slot 2 pending node or null, slot 3
//!   `expectedModCount` (Int), slot 4 `current` (the last-returned node
//!   or null — the OpenJDK HashIterator field that remove() consumes;
//!   added with the hash-iterator remove() fix, probed vs Temurin
//!   21.0.13, evidence scripts/evidence/itr_remove/).
//! * `java/util/HashMap$KeySet` / `java/util/HashMap$Values` — slot 0 map
//!   reference.
//! * `java/util/HashSet` — slot 0 backing `HashMap` reference.
//!
//! **HashMap is algorithm-faithful to OpenJDK** (risk M3-R2): the same hash
//! spreading (`h ^ (h >>> 16)`), the same `tableSizeFor`, initial capacity
//! 16, load factor 0.75, doubling resize with the Java-8 lo/hi bin split
//! that preserves relative order, tail insertion within bins, and table
//! order iteration. With `Integer`/`String` keys (whose `hashCode` is
//! pinned by the JVM spec) the iteration order is byte-identical to
//! Temurin 21 — pinned differentially by the `HashMapOrder` fixture.
//!
//! **ConcurrentModificationException is modelled** (M3_PLAN §5.3
//! decision): `modCount` is checked by iterator `next()`, which makes
//! iterator invalidation observable in single-threaded programs exactly as
//! Temurin behaves.
//!
//! Capacity growth constants follow Java's formulas, but one unobservable
//! deviation is documented honestly: a no-arg `ArrayList` starts with a
//! 0-length backing array instead of Java's shared
//! `DEFAULTCAPACITY_EMPTY_ELEMENTDATA`, so its first growth lands on
//! `size+1` rather than Java's 10. Capacity is not readable through any
//! method in the supported set, so no differential test can observe this.
//!
//! Borrowing discipline (M4 Wave 1 lock law L3): the heap is a plain
//! `Mutex` — NOTHING nests inside a heap guard any more, shared or
//! mutable. Every helper runs flat, sequential heap accesses, and error
//! objects are built after the guard ends (M2_PLAN §4.7 as amended).

use crate::first_arg_ref;
use crate::heap::{alloc, with_obj, with_obj_mut, JRef};
use crate::object::{new_obj, string_value};
use crate::subtype::is_subtype;
use crate::Value;
use crate::{internal_error, invoke_virtual, throw_new, Dispatch, Throw, UserParents};
use std::sync::Arc;
/// The CME class path the law tests assert against.
#[cfg(test)]
const CME_CLASS: &str = "java/util/ConcurrentModificationException";
/// The null-default argument law: a missing or null argument reads as the
/// null reference (the collections surface treats absent == null).
pub(crate) fn arg_or_null(args: &[Value]) -> Value {
    args.first().cloned().unwrap_or(Value::Ref(None))
}

// ---------------------------------------------------------------- helpers
/// `Objects.hashCode`-style dispatch: `null -> 0`, otherwise the receiver's
/// `hashCode()` (compiled user code first, then natives — `invoke_virtual`
/// walks the receiver's class chain).
pub fn dispatch_hash_code(v: &Value, ctx: Dispatch) -> Result<i32, Throw> {
    match v {
        Value::Ref(Some(r)) => {
            let h = crate::invoke_virtual(r, "java/lang/Object", "hashCode", "()I", &[], ctx)?;
            Ok(h.expect_int())
        }
        Value::Ref(None) => Ok(0),
        other => Err(internal_error(&format!(
            "collection key/value is not a reference: {other:?}"
        ))),
    }
}
/// Dispatched `equals`: `a.equals(b)` with `a` a live object.
pub fn dispatch_equals(a: &JRef, b: &Value, ctx: Dispatch) -> Result<bool, Throw> {
    let r = crate::invoke_virtual(
        a,
        "java/lang/Object",
        "equals",
        "(Ljava/lang/Object;)Z",
        std::slice::from_ref(b),
        ctx,
    )?;
    Ok(r.expect_int() != 0)
}
/// Dispatched `String.valueOf(element)`: `null -> "null"`, otherwise the
/// element's `toString()` (compiled user code first).
pub fn dispatch_to_string(v: &Value, ctx: Dispatch) -> Result<String, Throw> {
    match v {
        Value::Ref(Some(r)) => {
            let s = crate::invoke_virtual(
                r,
                "java/lang/Object",
                "toString",
                "()Ljava/lang/String;",
                &[],
                ctx,
            )?;
            match s.expect_ref() {
                Some(sr) => Ok(string_value(&sr).to_string()),
                None => Ok("null".to_string()),
            }
        }
        Value::Ref(None) => Ok("null".to_string()),
        other => Err(internal_error(&format!(
            "collection element is not a reference: {other:?}"
        ))),
    }
}
/// `AbstractCollection.toString` shape: `[a, b, c]`, `[]`, `[null]`.
fn collection_to_string(items: Vec<Value>, ctx: Dispatch) -> Result<Value, Throw> {
    let mut out = String::from("[");
    for (i, v) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(v, ctx)?);
    }
    out.push(']');
    Ok(crate::object::string_out(&out))
}
/// OpenJDK `HashMap.hash`: spreads the high bits down.
/// `h ^ (h >>> 16)` on the dispatched `hashCode()` (null key: hash 0).
fn map_hash(key: &Value, ctx: Dispatch) -> Result<i32, Throw> {
    let h = dispatch_hash_code(key, ctx)?;
    Ok(h ^ ((h as u32) >> 16) as i32)
}
/// OpenJDK `tableSizeFor`: the smallest power of two `>= cap`
/// (capped at `MAXIMUM_CAPACITY = 1 << 30`). Faithful including the
/// `cap <= 1 -> 1` edge (Java's `-1 >>> nlz(cap-1)` underflow produces 1).
fn table_size_for(cap: i32) -> i32 {
    const MAX: i32 = 1 << 30;
    if cap <= 1 {
        return 1;
    }
    if cap >= MAX {
        return MAX;
    }
    // Java: n = -1 >>> numberOfLeadingZeros(cap - 1)  (logical shift!)
    let n = ((-1i32) as u32).wrapping_shr((cap - 1).leading_zeros()) as i32;
    if n < 0 {
        1
    } else if n >= MAX {
        MAX
    } else {
        n + 1
    }
}

// ---------------------------------------------------------------- ArrayList
fn arg_bool(args: &[Value], i: usize) -> bool {
    args.get(i).map(|v| v.expect_int() != 0).unwrap_or(false)
}

// --- scanaislop split: the items below live verbatim in the child modules ---
mod arraylist;
mod hashmap;
mod hashset;
mod linkedhashmap;
mod linkedlist;
mod objects;
#[cfg(test)]
mod tests;
mod treemap;
mod treeviews;
mod unmod;
pub use arraylist::*;
pub use hashmap::*;
pub use hashset::*;
pub use linkedhashmap::*;
pub use linkedlist::*;
pub use objects::*;
pub use treemap::*;
pub use treeviews::*;
pub use unmod::*;
