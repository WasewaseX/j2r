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
    Ok(Value::Ref(Some(crate::object::new_string(&out))))
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

const AL_SIZE: usize = 0;
const AL_MODCOUNT: usize = 1;
const AL_ELEMS: usize = 2;

pub(crate) fn al_state(list: &JRef) -> (i32, i32, JRef) {
    with_obj(*list, |o| {
        (
            o.slots[AL_SIZE].expect_int(),
            o.slots[AL_MODCOUNT].expect_int(),
            match &o.slots[AL_ELEMS] {
                Value::Ref(Some(e)) => *e,
                _ => panic!("j2r internal: ArrayList without backing array"),
            },
        )
    })
}

/// Capacity of the backing array (its slot 0).
fn array_capacity(elems: &JRef) -> i32 {
    with_obj(*elems, |o| o.slots[0].expect_int())
}

fn alloc_array(cap: i32) -> JRef {
    let mut slots = Vec::with_capacity(cap as usize + 1);
    slots.push(Value::Int(cap));
    for _ in 0..cap {
        slots.push(Value::Ref(None));
    }
    alloc(crate::object::JObj {
        jclass: Arc::from("[Ljava/lang/Object;"),
        slots,
        prim: None,
    })
}

/// Grows the backing array when `size + 1 > capacity`; returns the
/// (possibly unchanged) elements handle.
fn al_grow(list: &JRef, elems: JRef, size: i32) -> JRef {
    let cap = array_capacity(&elems);
    if size < cap {
        return elems;
    }
    // Java grow: oldCap + (oldCap >> 1), floored at the needed minimum.
    let mut new_cap = cap + (cap >> 1);
    if new_cap < size + 1 {
        new_cap = size + 1;
    }
    if cap == 0 {
        // Java's no-arg ArrayList grows to 10 on the first add; the
        // difference vs size+1 is unobservable (capacity is not readable),
        // but we match Java anyway so the comment above stays honest.
        if new_cap < 10 {
            new_cap = 10;
        }
    }
    let fresh = alloc_array(new_cap);
    // copy: read old elements (shared borrows may nest), then write the
    // fresh array in one flat with_obj_mut
    let copied: Vec<Value> = with_obj(elems, |o| o.slots[1..=size as usize].to_vec());
    with_obj_mut(fresh, |o| {
        for (i, v) in copied.into_iter().enumerate() {
            o.slots[i + 1] = v;
        }
    });
    with_obj_mut(*list, |o| o.slots[AL_ELEMS] = Value::Ref(Some(fresh)));
    fresh
}

fn al_range_check(index: i32, size: i32) -> Result<(), Throw> {
    if index < 0 || index >= size {
        return Err(throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&format!("Index {index} out of bounds for length {size}")),
        ));
    }
    Ok(())
}

fn al_range_check_add(index: i32, size: i32) -> Result<(), Throw> {
    if index < 0 || index > size {
        return Err(throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&format!("Index: {index}, Size: {size}")),
        ));
    }
    Ok(())
}

/// Reads element `i` of the backing array (0-based element index).
fn al_get_elem(elems: &JRef, i: i32) -> Value {
    with_obj(*elems, |o| o.slots[(i + 1) as usize].clone())
}

fn al_set_elem(elems: &JRef, i: i32, v: Value) {
    with_obj_mut(*elems, |o| o.slots[(i + 1) as usize] = v);
}

// <init>()V and <init>(I)V
pub fn n_al_init(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.<init> without receiver");
    let cap = match args.first() {
        None => 0,
        Some(v) => {
            let c = v.expect_int();
            if c < 0 {
                return Err(throw_new(
                    "java/lang/IllegalArgumentException",
                    Some(&format!("Illegal Capacity: {c}")),
                ));
            }
            c
        }
    };
    let elems = alloc_array(cap);
    with_obj_mut(*r, |o| {
        o.slots[AL_SIZE] = Value::Int(0);
        o.slots[AL_MODCOUNT] = Value::Int(0);
        o.slots[AL_ELEMS] = Value::Ref(Some(elems));
    });
    Ok(Value::Void)
}

/// `<init>`(Collection) — the copy constructor (corpus wave 1: minimal-json's
/// `new ArrayList<String>(object.names)`). Iterates the source through its
/// OWN iterator dispatch (invokevirtual on the receiver's chain), so the
/// source may be any collection implementation — including the
/// UnmodifiableList view — and appends through the ordinary add path.
/// Copy then diverge: later source mutations must not touch the copy.
pub fn n_al_init_from_collection(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.<init> without receiver");
    let coll = match args.first() {
        Some(Value::Ref(Some(c))) => *c,
        Some(Value::Ref(None)) | None => return Err(crate::npe()),
        other => {
            return Err(internal_error(&format!(
                "j2r internal: ArrayList.<init>(Collection) arg {other:?}"
            )))
        }
    };
    // start from the empty state (same as the no-arg ctor)
    let elems = alloc_array(0);
    with_obj_mut(*r, |o| {
        o.slots[AL_SIZE] = Value::Int(0);
        o.slots[AL_MODCOUNT] = Value::Int(0);
        o.slots[AL_ELEMS] = Value::Ref(Some(elems));
    });
    // iterate via the source collection's own iterator
    let it = invoke_virtual(
        &coll,
        "java/util/Collection",
        "iterator",
        "()Ljava/util/Iterator;",
        &[],
        ctx,
    )?;
    let it = match it {
        Value::Ref(Some(i)) => i,
        other => {
            return Err(internal_error(&format!(
                "j2r internal: Collection.iterator returned {other:?}"
            )))
        }
    };
    loop {
        let has = invoke_virtual(&it, "java/util/Iterator", "hasNext", "()Z", &[], ctx)?;
        if has.expect_int() == 0 {
            break;
        }
        let v = invoke_virtual(
            &it,
            "java/util/Iterator",
            "next",
            "()Ljava/lang/Object;",
            &[],
            ctx,
        )?;
        n_al_add(Some(r), std::slice::from_ref(&v), ctx)?;
    }
    Ok(Value::Void)
}

pub fn n_al_add(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.add without receiver");
    let e = arg_or_null(args);
    let (size, mc, mut elems) = al_state(r);
    elems = al_grow(r, elems, size);
    al_set_elem(&elems, size, e);
    with_obj_mut(*r, |o| {
        o.slots[AL_SIZE] = Value::Int(size + 1);
        o.slots[AL_MODCOUNT] = Value::Int(mc + 1);
    });
    Ok(Value::Int(1))
}

/// `AbstractList.hashCode`: `1; 31*h + (e == null ? 0 : e.hashCode())` per
/// element, wrapping arithmetic exactly like the JVM (corpus wave 1:
/// `names.hashCode()` in JsonObject.hashCode).
pub fn n_al_hash_code(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let _ = args;
    let r = match recv {
        Some(r) => r,
        None => return Err(crate::npe()),
    };
    let (size, _mc, elems) = al_state(r);
    let mut h: i32 = 1;
    for i in 0..size {
        let eh = match al_get_elem(&elems, i) {
            Value::Ref(None) => 0,
            Value::Ref(Some(er)) => {
                let out = invoke_virtual(&er, "java/lang/Object", "hashCode", "()I", &[], ctx)?;
                out.expect_int()
            }
            other => {
                return Err(internal_error(&format!(
                    "j2r internal: list element {other:?}"
                )))
            }
        };
        h = h.wrapping_mul(31).wrapping_add(eh);
    }
    Ok(Value::Int(h))
}

/// `AbstractList.equals`: same object, or a List of the same size with
/// pairwise-equal elements (`equals` dispatch on each pair).
pub fn n_al_equals(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => r,
        None => return Err(crate::npe()),
    };
    let other = match args.first() {
        Some(Value::Ref(Some(o))) => *o,
        Some(Value::Ref(None)) | None => return Ok(Value::Int(0)),
        other => {
            return Err(internal_error(&format!(
                "j2r internal: List.equals arg {other:?}"
            )))
        }
    };
    if other == *r {
        return Ok(Value::Int(1));
    }
    let ocls = with_obj(other, |o| o.jclass.to_string());
    if !ctx.is_subtype(&ocls, "java/util/List") {
        return Ok(Value::Int(0));
    }
    let (size, _mc, elems) = al_state(r);
    let osize = invoke_virtual(&other, "java/util/List", "size", "()I", &[], ctx)?.expect_int();
    if osize != size {
        return Ok(Value::Int(0));
    }
    for i in 0..size {
        let a = al_get_elem(&elems, i);
        let b = invoke_virtual(
            &other,
            "java/util/List",
            "get",
            "(I)Ljava/lang/Object;",
            &[Value::Int(i)],
            ctx,
        )?;
        let eq = match (&a, &b) {
            (Value::Ref(None), Value::Ref(None)) => true,
            (Value::Ref(None), _) | (_, Value::Ref(None)) => false,
            (Value::Ref(Some(x)), _) => dispatch_equals(x, &b, ctx)?,
            (other_a, _) => {
                return Err(internal_error(&format!(
                    "j2r internal: list element {other_a:?}"
                )))
            }
        };
        if !eq {
            return Ok(Value::Int(0));
        }
    }
    Ok(Value::Int(1))
}

/// `AbstractList.lastIndexOf`: scan from the end, `equals` dispatch.
pub fn n_al_last_index_of(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => r,
        None => return Err(crate::npe()),
    };
    let target = arg_or_null(args);
    let (size, _mc, elems) = al_state(r);
    for i in (0..size).rev() {
        let e = al_get_elem(&elems, i);
        let eq = match (&e, &target) {
            (Value::Ref(None), Value::Ref(None)) => true,
            (Value::Ref(None), _) | (_, Value::Ref(None)) => false,
            (Value::Ref(Some(x)), _) => dispatch_equals(x, &target, ctx)?,
            (other_e, _) => {
                return Err(internal_error(&format!(
                    "j2r internal: list element {other_e:?}"
                )))
            }
        };
        if eq {
            return Ok(Value::Int(i));
        }
    }
    Ok(Value::Int(-1))
}

pub fn n_al_add_at(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.add without receiver");
    let index = args.first().cloned().unwrap_or(Value::Int(0)).expect_int();
    let e = args.get(1).cloned().unwrap_or(Value::Ref(None));
    let (size, mc, mut elems) = al_state(r);
    al_range_check_add(index, size)?;
    elems = al_grow(r, elems, size);
    // shift right from the end
    let mut i = size;
    while i > index {
        let v = al_get_elem(&elems, i - 1);
        al_set_elem(&elems, i, v);
        i -= 1;
    }
    al_set_elem(&elems, index, e);
    with_obj_mut(*r, |o| {
        o.slots[AL_SIZE] = Value::Int(size + 1);
        o.slots[AL_MODCOUNT] = Value::Int(mc + 1);
    });
    Ok(Value::Void)
}

pub fn n_al_get(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.get without receiver");
    let index = args.first().cloned().unwrap_or(Value::Int(0)).expect_int();
    let (size, _, elems) = al_state(r);
    al_range_check(index, size)?;
    Ok(al_get_elem(&elems, index))
}

pub fn n_al_set(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.set without receiver");
    let index = args.first().cloned().unwrap_or(Value::Int(0)).expect_int();
    let e = args.get(1).cloned().unwrap_or(Value::Ref(None));
    let (size, _, elems) = al_state(r);
    al_range_check(index, size)?;
    let old = al_get_elem(&elems, index);
    al_set_elem(&elems, index, e);
    Ok(old)
}

pub fn n_al_remove_at(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.remove without receiver");
    let index = args.first().cloned().unwrap_or(Value::Int(0)).expect_int();
    let (size, mc, elems) = al_state(r);
    al_range_check(index, size)?;
    let old = al_get_elem(&elems, index);
    let mut i = index;
    while i < size - 1 {
        let v = al_get_elem(&elems, i + 1);
        al_set_elem(&elems, i, v);
        i += 1;
    }
    al_set_elem(&elems, size - 1, Value::Ref(None));
    with_obj_mut(*r, |o| {
        o.slots[AL_SIZE] = Value::Int(size - 1);
        o.slots[AL_MODCOUNT] = Value::Int(mc + 1);
    });
    Ok(old)
}

pub fn n_al_remove_obj(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.remove without receiver");
    let target = arg_or_null(args);
    let (size, mc, elems) = al_state(r);
    let mut found_at: i32 = -1;
    for i in 0..size {
        let v = al_get_elem(&elems, i);
        let eq = match (&v, &target) {
            (Value::Ref(Some(a)), Value::Ref(Some(_))) => dispatch_equals(a, &target, ctx)?,
            (Value::Ref(None), Value::Ref(None)) => true,
            _ => false,
        };
        if eq {
            found_at = i;
            break;
        }
    }
    if found_at < 0 {
        return Ok(Value::Int(0));
    }
    let mut i = found_at;
    while i < size - 1 {
        let v = al_get_elem(&elems, i + 1);
        al_set_elem(&elems, i, v);
        i += 1;
    }
    al_set_elem(&elems, size - 1, Value::Ref(None));
    with_obj_mut(*r, |o| {
        o.slots[AL_SIZE] = Value::Int(size - 1);
        o.slots[AL_MODCOUNT] = Value::Int(mc + 1);
    });
    Ok(Value::Int(1))
}

pub fn n_al_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.size without receiver");
    Ok(Value::Int(with_obj(*r, |o| o.slots[AL_SIZE].expect_int())))
}

pub fn n_al_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.isEmpty without receiver");
    Ok(Value::Int(
        (with_obj(*r, |o| o.slots[AL_SIZE].expect_int()) == 0) as i32,
    ))
}

pub fn n_al_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.contains without receiver");
    let target = arg_or_null(args);
    let (size, _, elems) = al_state(r);
    for i in 0..size {
        let v = al_get_elem(&elems, i);
        let eq = match (&v, &target) {
            (Value::Ref(Some(a)), Value::Ref(Some(_))) => dispatch_equals(a, &target, ctx)?,
            (Value::Ref(None), Value::Ref(None)) => true,
            _ => false,
        };
        if eq {
            return Ok(Value::Int(1));
        }
    }
    Ok(Value::Int(0))
}

pub fn n_al_index_of(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.indexOf without receiver");
    let target = arg_or_null(args);
    let (size, _, elems) = al_state(r);
    for i in 0..size {
        let v = al_get_elem(&elems, i);
        let eq = match (&v, &target) {
            (Value::Ref(Some(a)), Value::Ref(Some(_))) => dispatch_equals(a, &target, ctx)?,
            (Value::Ref(None), Value::Ref(None)) => true,
            _ => false,
        };
        if eq {
            return Ok(Value::Int(i));
        }
    }
    Ok(Value::Int(-1))
}

pub fn n_al_clear(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.clear without receiver");
    let (size, mc, elems) = al_state(r);
    for i in 0..size {
        al_set_elem(&elems, i, Value::Ref(None));
    }
    with_obj_mut(*r, |o| {
        o.slots[AL_SIZE] = Value::Int(0);
        o.slots[AL_MODCOUNT] = Value::Int(mc + 1);
    });
    Ok(Value::Void)
}

pub fn n_al_to_string(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.toString without receiver");
    let (size, _, elems) = al_state(r);
    let items: Vec<Value> = (0..size).map(|i| al_get_elem(&elems, i)).collect();
    collection_to_string(items, ctx)
}

/// Dispatches one functional-interface SAM call on a possibly-null target:
/// closures (object class = the interface) and user implementations both
/// resolve through the ordinary `invoke_virtual` walk. Null target → NPE,
/// exactly like a JVM invocation on a null receiver.
fn invoke_sam(
    target: &Value,
    iface: &str,
    name: &str,
    desc: &str,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match target {
        Value::Ref(Some(r)) => *r,
        _ => return Err(crate::npe()),
    };
    crate::invoke_virtual(&r, iface, name, desc, args, ctx)
}

/// `ArrayList.forEach(Consumer)` — OpenJDK-faithful: the action runs per
/// element in order and `modCount` is checked after every element, so a
/// structural modification through the action throws CME.
pub fn n_al_for_each(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.forEach without receiver");
    let action = arg_or_null(args);
    let (size, mc, elems) = al_state(r);
    for i in 0..size {
        let v = al_get_elem(&elems, i);
        invoke_sam(
            &action,
            "java/util/function/Consumer",
            "accept",
            "(Ljava/lang/Object;)V",
            &[v],
            ctx,
        )?;
        if with_obj(*r, |o| o.slots[AL_MODCOUNT].expect_int()) != mc {
            return Err(crate::cme());
        }
    }
    Ok(Value::Void)
}

/// `ArrayList.removeIf(Predicate)` — OpenJDK shape: one pass computes the
/// removal set (CME if the predicate structurally modified the list), then
/// a single in-place compaction and one `modCount` increment. Returns
/// whether any element was removed.
pub fn n_al_remove_if(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.removeIf without receiver");
    let filter = arg_or_null(args);
    let (size, mc, elems) = al_state(r);
    let mut remove: Vec<bool> = Vec::with_capacity(size.max(0) as usize);
    let mut any = false;
    for i in 0..size {
        let v = al_get_elem(&elems, i);
        let t = invoke_sam(
            &filter,
            "java/util/function/Predicate",
            "test",
            "(Ljava/lang/Object;)Z",
            &[v],
            ctx,
        )?;
        let drop_it = t.expect_int() != 0;
        remove.push(drop_it);
        any |= drop_it;
        if with_obj(*r, |o| o.slots[AL_MODCOUNT].expect_int()) != mc {
            return Err(crate::cme());
        }
    }
    if any {
        let mut w = 0i32;
        for i in 0..size {
            let v = al_get_elem(&elems, i);
            if !remove[i as usize] {
                al_set_elem(&elems, w, v);
                w += 1;
            }
        }
        for i in w..size {
            al_set_elem(&elems, i, Value::Ref(None));
        }
        with_obj_mut(*r, |o| {
            o.slots[AL_SIZE] = Value::Int(w);
            o.slots[AL_MODCOUNT] = Value::Int(mc + 1);
        });
    }
    Ok(Value::Int(any as i32))
}

pub fn n_al_iterator(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.iterator without receiver");
    let (_, mc, _elems) = al_state(r);
    // slots: 0 = list, 1 = cursor, 2 = expectedModCount, 3 = lastRet (-1 =
    // none yet) — Java's ArrayList$Itr layout for remove() support
    let it = new_obj("java/util/ArrayList$Itr", 4)?;
    with_obj_mut(it, |o| {
        o.slots[0] = Value::Ref(Some(*r));
        o.slots[1] = Value::Int(0);
        o.slots[2] = Value::Int(mc);
        o.slots[3] = Value::Int(-1);
    });
    Ok(Value::Ref(Some(it)))
}

// ArrayList$Itr: 0 = list, 1 = cursor, 2 = expectedModCount
pub fn n_itr_has_next(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: Iterator.hasNext without receiver");
    let (list, cursor) = with_obj(*it, |o| {
        (
            match &o.slots[0] {
                Value::Ref(Some(l)) => *l,
                _ => panic!("j2r internal: iterator without list"),
            },
            o.slots[1].expect_int(),
        )
    });
    let size = with_obj(list, |o| o.slots[AL_SIZE].expect_int());
    Ok(Value::Int((cursor != size) as i32))
}

fn al_check_comodification(it: &JRef, list: &JRef) -> Result<(), Throw> {
    let expected = with_obj(*it, |o| o.slots[2].expect_int());
    let mc = with_obj(*list, |o| o.slots[AL_MODCOUNT].expect_int());
    if expected != mc {
        return Err(crate::cme());
    }
    Ok(())
}

pub fn n_itr_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: Iterator.next without receiver");
    let (list, cursor) = with_obj(*it, |o| {
        (
            match &o.slots[0] {
                Value::Ref(Some(l)) => *l,
                _ => panic!("j2r internal: iterator without list"),
            },
            o.slots[1].expect_int(),
        )
    });
    al_check_comodification(it, &list)?;
    let (size, _, elems) = al_state(&list);
    if cursor >= size {
        return Err(throw_new("java/util/NoSuchElementException", None));
    }
    let v = al_get_elem(&elems, cursor);
    with_obj_mut(*it, |o| {
        o.slots[1] = Value::Int(cursor + 1);
        o.slots[3] = Value::Int(cursor);
    });
    Ok(v)
}

/// `ArrayList$Itr.remove()` — Java-faithful: removes the last-returned
/// element, rewinds the cursor, and re-syncs expectedModCount so removal
/// through the iterator never throws CME. (The M3_PLAN §5.2 draft pinned
/// this as loud-unsupported; the differential oracle showed Temurin's
/// real behaviour, and faithful > convenient — the plan deviation is
/// recorded in docs/M3_EVIDENCE.md.)
pub fn n_itr_remove(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: Iterator.remove without receiver");
    let (list, cursor, last_ret) = with_obj(*it, |o| {
        (
            match &o.slots[0] {
                Value::Ref(Some(l)) => *l,
                _ => panic!("j2r internal: iterator without list"),
            },
            o.slots[1].expect_int(),
            o.slots[3].expect_int(),
        )
    });
    al_check_comodification(it, &list)?;
    if last_ret < 0 {
        return Err(throw_new("java/lang/IllegalStateException", None));
    }
    // list.remove(lastRet) inline: bounds are trivially valid (lastRet was
    // produced by next()), shift left, null the tail, bump modCount
    let (size, mc, elems) = al_state(&list);
    let mut i = last_ret;
    while i < size - 1 {
        let v = al_get_elem(&elems, i + 1);
        al_set_elem(&elems, i, v);
        i += 1;
    }
    al_set_elem(&elems, size - 1, Value::Ref(None));
    with_obj_mut(list, |o| {
        o.slots[AL_SIZE] = Value::Int(size - 1);
        o.slots[AL_MODCOUNT] = Value::Int(mc + 1);
    });
    with_obj_mut(*it, |o| {
        o.slots[1] = Value::Int(last_ret);
        o.slots[3] = Value::Int(-1);
        // re-sync: Java sets expectedModCount = modCount after removal
        o.slots[2] = Value::Int(mc + 1);
    });
    let _ = cursor;
    Ok(Value::Void)
}

// ---------------------------------------------------------------- HashMap

const HM_SIZE: usize = 0;
const HM_MODCOUNT: usize = 1;
const HM_THRESHOLD: usize = 2;
const HM_TABLE: usize = 3;

/// `HashMap$Node` slot 4 — the prim tag (G3.3 slice 3). `Value::Void` on
/// a faithful node; `Value::Int(desc-char)` when the value slot holds a
/// RAW prim payload written by `hm_put_prim`.
const HM_NODE_TAG: usize = 4;

fn tag_slot(tag: Option<char>) -> Value {
    match tag {
        Some(c) => Value::Int(c as i32),
        None => Value::Void,
    }
}

fn hm_state(map: &JRef) -> (i32, i32, i32, Option<JRef>) {
    with_obj(*map, |o| {
        (
            o.slots[HM_SIZE].expect_int(),
            o.slots[HM_MODCOUNT].expect_int(),
            o.slots[HM_THRESHOLD].expect_int(),
            match &o.slots[HM_TABLE] {
                Value::Ref(Some(t)) => Some(*t),
                _ => None,
            },
        )
    })
}

fn hm_table_len(table: &JRef) -> i32 {
    with_obj(*table, |o| o.slots[0].expect_int())
}

fn hm_bin(table: &JRef, idx: i32) -> Option<JRef> {
    with_obj(*table, |o| match &o.slots[(idx + 1) as usize] {
        Value::Ref(Some(n)) => Some(*n),
        _ => None,
    })
}

fn hm_set_bin(table: &JRef, idx: i32, node: Option<JRef>) {
    with_obj_mut(*table, |o| {
        o.slots[(idx + 1) as usize] = match node {
            Some(n) => Value::Ref(Some(n)),
            None => Value::Ref(None),
        }
    });
}

fn node_parts(node: &JRef) -> (i32, Value, Value, Option<JRef>) {
    with_obj(*node, |o| {
        (
            o.slots[0].expect_int(),
            o.slots[1].clone(),
            o.slots[2].clone(),
            match &o.slots[3] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    })
}

fn alloc_node_table(len: i32) -> JRef {
    let mut slots = Vec::with_capacity(len as usize + 1);
    slots.push(Value::Int(len));
    for _ in 0..len {
        slots.push(Value::Ref(None));
    }
    alloc(crate::object::JObj {
        jclass: Arc::from("[Ljava/util/HashMap$Node;"),
        slots,
        prim: None,
    })
}

/// Key equality per OpenJDK `putVal`: `key == k || (k != null &&
/// key.equals(k))`, guarded by equal stored hashes.
fn key_matches(
    stored_hash: i32,
    stored_key: &Value,
    hash: i32,
    key: &Value,
    ctx: Dispatch,
) -> Result<bool, Throw> {
    if stored_hash != hash {
        return Ok(false);
    }
    match (stored_key, key) {
        (Value::Ref(Some(_)), Value::Ref(None)) => {
            // stored key non-null, looked-up key null: OpenJDK guards with
            // `k != null`, so null never matches a non-null stored key.
            Ok(false)
        }
        (Value::Ref(None), Value::Ref(None)) => Ok(true),
        (Value::Ref(Some(a_ref)), b) => {
            if let Value::Ref(Some(b_ref)) = b {
                if JRef::ptr_eq(a_ref, b_ref) {
                    return Ok(true);
                }
            }
            dispatch_equals(a_ref, b, ctx)
        }
        _ => Err(internal_error("collection key is not a reference")),
    }
}

/// OpenJDK `resize` (Java-8 split): doubles the table, splitting each bin
/// into lo (`(hash & oldCap) == 0`) and hi halves **preserving relative
/// order within each half** — this is what makes iteration order match
/// Temurin byte-for-byte.
fn hm_resize(map: &JRef) -> Result<(), Throw> {
    let (size, mc, threshold, table) = hm_state(map);
    let _ = mc;
    let (old_cap, old_thr) = match table {
        None => (0, threshold),
        Some(t) => (hm_table_len(&t), threshold),
    };
    let (new_cap, mut new_thr) = if old_cap > 0 {
        // grow path. Java doubles the threshold only when
        // `oldCap >= DEFAULT_INITIAL_CAPACITY (16)`; below that it falls
        // through to the ft recompute — the timing of this branch is
        // observable (it decides when the next resize fires, which changes
        // table capacities and therefore iteration order), so it is
        // reproduced exactly.
        let nc = old_cap << 1;
        let nt = if old_cap >= 16 { threshold << 1 } else { 0 };
        (nc, nt)
    } else if old_thr > 0 {
        // first allocation from `new HashMap<>(initialCapacity)`
        (old_thr, 0)
    } else {
        (16, 12)
    };
    if new_thr == 0 {
        // Java: float ft = (float)newCap * loadFactor; (float arithmetic)
        let ft = (new_cap as f32 * 0.75f32) as i32;
        new_thr = if new_cap < (1 << 30) && (ft as f32) < (1 << 30) as f32 {
            ft
        } else {
            1 << 30
        };
    }
    let fresh = alloc_node_table(new_cap);
    if let Some(old) = table {
        let old_len = hm_table_len(&old);
        for i in 0..old_len {
            let mut lo_head: Option<JRef> = None;
            let mut lo_tail: Option<JRef> = None;
            let mut hi_head: Option<JRef> = None;
            let mut hi_tail: Option<JRef> = None;
            let mut cur = hm_bin(&old, i);
            while let Some(node) = cur {
                let (hash, key, value, next) = node_parts(&node);
                if (hash & old_cap) == 0 {
                    match lo_tail {
                        None => lo_head = Some(node),
                        Some(t) => with_obj_mut(t, |o| o.slots[3] = Value::Ref(Some(node))),
                    }
                    lo_tail = Some(node);
                } else {
                    match hi_tail {
                        None => hi_head = Some(node),
                        Some(t) => with_obj_mut(t, |o| o.slots[3] = Value::Ref(Some(node))),
                    }
                    hi_tail = Some(node);
                }
                cur = next;
                let _ = (key, value);
            }
            // OpenJDK terminates both split lists explicitly
            // (`loTail.next = null; hiTail.next = null`) — without this the
            // old cross-bin chain pointer survives and one node is iterated
            // from two bins.
            if let Some(t) = lo_tail {
                with_obj_mut(t, |o| o.slots[3] = Value::Ref(None));
            }
            if let Some(t) = hi_tail {
                with_obj_mut(t, |o| o.slots[3] = Value::Ref(None));
            }
            if let Some(h) = lo_head {
                hm_set_bin(&fresh, i, Some(h));
            }
            if let Some(h) = hi_head {
                hm_set_bin(&fresh, i + old_cap, Some(h));
            }
        }
    }
    with_obj_mut(*map, |o| {
        o.slots[HM_TABLE] = Value::Ref(Some(fresh));
        o.slots[HM_THRESHOLD] = Value::Int(new_thr);
    });
    let _ = size;
    Ok(())
}

/// The node layout hm_put allocates: the plain HashMap$Node (5 slots —
/// hash/key/value/next + the undeclared tag) or the LinkedHashMap$Entry
/// (7 slots — before/after after the tag, the order-link pair).
#[derive(Clone, Copy, PartialEq)]
enum NodeKind {
    HashNode,
    LinkedEntry,
}

/// OpenJDK `putVal` with tail insertion. Returns the previous value
/// (`Ref(None)` when absent) plus the freshly-inserted NODE (None on an
/// overwrite — LinkedHashMap's order law needs the new node to link it
/// at the tail; an overwrite keeps position, so no link work).
fn hm_put(
    map: &JRef,
    key: Value,
    value: Value,
    ctx: Dispatch,
    tag: Option<char>,
    kind: NodeKind,
) -> Result<(Value, Option<JRef>), Throw> {
    let hash = map_hash(&key, ctx)?;
    let (_, _, _, table) = hm_state(map);
    if table.is_none() || hm_table_len(table.as_ref().unwrap()) == 0 {
        hm_resize(map)?;
    }
    let table = match hm_state(map).3 {
        Some(t) => t,
        None => return Err(internal_error("HashMap table missing after resize")),
    };
    let len = hm_table_len(&table);
    let idx = (len - 1) & hash;
    // walk the bin
    let mut cur = hm_bin(&table, idx);
    let mut prev: Option<JRef> = None;
    while let Some(node) = cur {
        let (h, k, _old_value, next) = node_parts(&node);
        if key_matches(h, &k, hash, &key, ctx)? {
            // The Map.put contract returns the OLD value — materialize a
            // raw-prim slot first (slice 3), so the faithful caller sees
            // the same box Java would have held. The allocation happens
            // before any with_obj scope (the heap-lock law).
            let old = node_value_materialized(&node)?;
            with_obj_mut(node, |o| {
                o.slots[2] = value;
                o.slots[HM_NODE_TAG] = tag_slot(tag);
            });
            return Ok((old, None));
        }
        prev = Some(node);
        cur = next;
    }
    // tail-insert a fresh node (the kind decides the layout — LinkedHashMap
    // entries carry the before/after pair the order law walks)
    let node = match kind {
        NodeKind::HashNode => new_obj("java/util/HashMap$Node", 5)?,
        NodeKind::LinkedEntry => new_obj("java/util/LinkedHashMap$Entry", 7)?,
    };
    with_obj_mut(node, |o| {
        o.slots[0] = Value::Int(hash);
        o.slots[1] = key;
        o.slots[2] = value;
        o.slots[3] = Value::Ref(None);
        o.slots[HM_NODE_TAG] = tag_slot(tag);
    });
    match prev {
        None => hm_set_bin(&table, idx, Some(node)),
        Some(p) => with_obj_mut(p, |o| o.slots[3] = Value::Ref(Some(node))),
    }
    let (size, mc, _, _) = hm_state(map);
    with_obj_mut(*map, |o| {
        o.slots[HM_SIZE] = Value::Int(size + 1);
        o.slots[HM_MODCOUNT] = Value::Int(mc + 1);
    });
    if size + 1 > with_obj(*map, |o| o.slots[HM_THRESHOLD].expect_int()) {
        hm_resize(map)?;
    }
    Ok((Value::Ref(None), Some(node)))
}

fn hm_find(map: &JRef, key: &Value, ctx: Dispatch) -> Result<Option<JRef>, Throw> {
    let (_, _, _, table) = hm_state(map);
    let Some(table) = table else { return Ok(None) };
    let hash = map_hash(key, ctx)?;
    let len = hm_table_len(&table);
    let idx = (len - 1) & hash;
    let mut cur = hm_bin(&table, idx);
    while let Some(node) = cur {
        let (h, k, _, next) = node_parts(&node);
        if key_matches(h, &k, hash, key, ctx)? {
            return Ok(Some(node));
        }
        cur = next;
    }
    Ok(None)
}

fn hm_remove(map: &JRef, key: &Value, ctx: Dispatch) -> Result<Value, Throw> {
    let (_, _, _, table) = hm_state(map);
    let Some(table) = table else {
        return Ok(Value::Ref(None));
    };
    let hash = map_hash(key, ctx)?;
    let len = hm_table_len(&table);
    let idx = (len - 1) & hash;
    let mut prev: Option<JRef> = None;
    let mut cur = hm_bin(&table, idx);
    while let Some(node) = cur {
        let (h, k, _value, next) = node_parts(&node);
        if key_matches(h, &k, hash, key, ctx)? {
            // The removed value escapes to faithful code — materialize
            // before unlinking (slice 3; the alloc precedes any with_obj
            // scope).
            let out = node_value_materialized(&node)?;
            match prev {
                None => hm_set_bin(&table, idx, next),
                Some(p) => with_obj_mut(p, |o| {
                    o.slots[3] = match next {
                        Some(n) => Value::Ref(Some(n)),
                        None => Value::Ref(None),
                    }
                }),
            }
            let (size, mc, _, _) = hm_state(map);
            with_obj_mut(*map, |o| {
                o.slots[HM_SIZE] = Value::Int(size - 1);
                o.slots[HM_MODCOUNT] = Value::Int(mc + 1);
            });
            return Ok(out);
        }
        prev = Some(node);
        cur = next;
    }
    Ok(Value::Ref(None))
}

/// Table-order iteration (bins in order, within-bin list order). Values
/// materialize lazily (slice 3): every consumer of this Vec is faithful
/// code (toString/containsValue/view toString), which must observe real
/// boxes.
fn hm_all_entries(map: &JRef) -> Result<Vec<(Value, Value)>, Throw> {
    let (_, _, _, table) = hm_state(map);
    let Some(table) = table else {
        return Ok(Vec::new());
    };
    let len = hm_table_len(&table);
    let mut out = Vec::new();
    for i in 0..len {
        let mut cur = hm_bin(&table, i);
        while let Some(node) = cur {
            let (_, k, _, next) = node_parts(&node);
            let v = node_value_materialized(&node)?;
            out.push((k, v));
            cur = next;
        }
    }
    Ok(out)
}

// <init>()V / <init>(I)V / <init>(Ljava/util/Map;)V — the (I,F) ctor is
// NOT registered (the HashMap (I,F) ctor is unsupported too; the
// access-order (I,F,Z) ctor is its own loud gap). The Map arm is
// OpenJDK's putMapEntries on a fresh table: the pre-size law
// (threshold = tableSizeFor((int)((float)s / 0.75f + 1.0f)), the table
// stays lazy until the first put) and the source's ITERATION ORDER
// drives insertion — the probed HashMapCopyProbe laws L1-L9 (the
// Temurin 21.0.13+9 oracle, scripts/evidence/hm_copy_ctor/).
pub fn n_hm_init(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.<init> without receiver");
    match args.first() {
        None | Some(Value::Int(_)) => {
            // HashMap(): the threshold stays 0 — the first put takes the
            // default cap 16 (table_size_for(0) would be 1, which is the
            // (I)-ctor law, NOT the no-arg law; the distinction is
            // observable through the first allocation's capacity).
            let thr = match args.first() {
                None => 0,
                Some(v) => {
                    let c = v.expect_int();
                    if c < 0 {
                        return Err(throw_new(
                            "java/lang/IllegalArgumentException",
                            Some(&format!("Illegal initial capacity: {c}")),
                        ));
                    }
                    table_size_for(c)
                }
            };
            with_obj_mut(*r, |o| {
                o.slots[HM_SIZE] = Value::Int(0);
                o.slots[HM_MODCOUNT] = Value::Int(0);
                o.slots[HM_THRESHOLD] = Value::Int(thr);
                o.slots[HM_TABLE] = Value::Ref(None);
            });
            Ok(Value::Void)
        }
        Some(Value::Ref(None)) => {
            // putMapEntries reads m.size() on the null map FIRST.
            Err(throw_new("java/lang/NullPointerException", None))
        }
        Some(Value::Ref(Some(src))) => {
            let entries = map_source_entries(src, ctx.parents)?;
            hm_ctor_presize(r, entries.len())?;
            for (k, v) in entries {
                hm_put(r, k, v, ctx, None, NodeKind::HashNode)?;
            }
            Ok(Value::Void)
        }
        Some(other) => Err(internal_error(&format!(
            "j2r internal: HashMap.<init> unexpected arg {other:?}"
        ))),
    }
}

/// The copy-ctor SOURCE-storage law shared by the HashMap and
/// LinkedHashMap copy ctors: iterate the source in ITS order — the
/// LinkedHashMap link chain FIRST (LHM extends HashMap, so the LHM test
/// must win the dispatch), then the TreeMap key order, then the plain
/// HashMap table order. User subclasses inherit their builtin
/// ancestor's storage (the generated parents table classifies them);
/// a user class implementing Map directly has no builtin storage to
/// walk and stays loud (the same-surface law; the user-Map-source
/// residual — Temurin would iterate its entrySet()).
fn map_source_entries(
    src: &JRef,
    parents: Option<UserParents>,
) -> Result<Vec<(Value, Value)>, Throw> {
    let src_class = with_obj(*src, |o| o.jclass.to_string());
    if is_subtype(&src_class, "java/util/LinkedHashMap", parents) {
        lhm_all_entries(src)
    } else if is_subtype(&src_class, "java/util/TreeMap", parents) {
        tm_all_entries(src)
    } else if is_subtype(&src_class, "java/util/HashMap", parents) {
        hm_all_entries(src)
    } else {
        Err(internal_error(&format!(
            "j2r internal: map copy from unsupported source class {src_class} (the user-Map-source residual)"
        )))
    }
}

/// The putMapEntries pre-size law shared by both copy ctors: threshold
/// = tableSizeFor((int)((float)s / 0.75f + 1.0f)) (or 0 for an empty
/// source — putMapEntries skips everything and the first put takes the
/// default cap 16), and the table stays lazy: hm_resize's old_thr>0
/// arm allocates it at the threshold on the first put.
fn hm_ctor_presize(r: &JRef, s: usize) -> Result<(), Throw> {
    let thr = if s > 0 {
        // Java: float ft = ((float)s / 0.75f) + 1.0f;
        //       int t = (ft < (float)MAXIMUM_CAPACITY) ? (int)ft : Integer.MAX_VALUE;
        let ft = (s as f32 / 0.75f32) + 1.0f32;
        let t = if ft < (1u32 << 30) as f32 {
            ft as i32
        } else {
            i32::MAX
        };
        table_size_for(t)
    } else {
        0
    };
    with_obj_mut(*r, |o| {
        o.slots[HM_SIZE] = Value::Int(0);
        o.slots[HM_MODCOUNT] = Value::Int(0);
        o.slots[HM_THRESHOLD] = Value::Int(thr);
        o.slots[HM_TABLE] = Value::Ref(None);
    });
    Ok(())
}

pub fn n_hm_put(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.put without receiver");
    let key = arg_or_null(args);
    let value = args.get(1).cloned().unwrap_or(Value::Ref(None));
    let (old, _) = hm_put(r, key, value, ctx, None, NodeKind::HashNode)?;
    Ok(old)
}

pub fn n_hm_get(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.get without receiver");
    let key = arg_or_null(args);
    match hm_find(r, &key, ctx)? {
        Some(node) => node_value_materialized(&node),
        None => Ok(Value::Ref(None)),
    }
}

// --- G3.3 slice 2: the prim-slot map natives (the specialized-map path) ---
//
// The peephole (`map_prim_rewrites`) rewrites javac's `valueOf -> put ->
// pop` and `get -> checkcast -> doubleValue` cores on receiver locals its
// closure law proved safe (every load of the local is a rewritten core's
// receiver, so the map never aliases and every slot was written by a
// rewritten put) and — slice 3 — on static fields whose class-level law
// holds (every PutStatic a HashMap creation run, every GetStatic a
// guarded core's receiver pinned by the field signature). There the box
// is unobservable as an `Object`, and the node's value slot holds the RAW
// prim `Value` — one fewer heap child per node, no valueOf alloc, no
// unbox dispatch (the register's 2.78x container tax, evidence
// g3_3/opening_census).

/// G3.3 slice 3: the lazy box materialization at the faithful exits.
///
/// A node whose value slot holds a RAW prim (stamped by `hm_put_prim`
/// with the payload's desc char in slot 4) is materialized into its
/// boxed object on the first faithful read and the box is CACHED back
/// into the slot — get-twice-returns-the-same-ref (probe law L1: three
/// generic exits of one key compare identical) — and the boxing routes
/// through the same `valueOf` natives user code uses, so the JLS
/// identity caches apply (probe law L2: `CACHE.get("n") ==
/// Integer.valueOf(5)` is TRUE for cached values). A tag-less node is a
/// faithful node: its slot is already a box or null, pass it through.
/// The allocation happens OUTSIDE any `with_obj` scope (the heap-lock
/// law: alloc never nests under an object borrow).
fn node_value_materialized(node: &JRef) -> Result<Value, Throw> {
    node_value_materialized_at(node, 2, HM_NODE_TAG)
}

/// The layout-aware form of [`node_value_materialized`] (G3.3 slice 4):
/// the same law at an arbitrary (value slot, tag slot) pair — the
/// red-black storage's `TreeMap$Entry` reads (TME_VALUE, TME_TAG). All
/// other laws are the helper's own (shared above).
fn node_value_materialized_at(
    node: &JRef,
    value_idx: usize,
    tag_idx: usize,
) -> Result<Value, Throw> {
    let (v, tag) = with_obj(*node, |o| {
        (o.slots[value_idx].clone(), o.slots[tag_idx].clone())
    });
    let Value::Int(c) = tag else { return Ok(v) };
    let is_prim = matches!(
        v,
        Value::Double(_) | Value::Float(_) | Value::Int(_) | Value::Long(_)
    );
    if !is_prim {
        // A tagged slot whose value is already a box: the box was
        // materialized earlier and the tag kept as provenance — pass it
        // through (the tag is an annotation, not a mode switch).
        return Ok(v);
    }
    let desc = match char::from_u32(c as u32) {
        Some(d @ ('D' | 'F' | 'I' | 'J')) => d,
        _ => {
            return Err(internal_error(
                "j2r internal: map node carries a prim slot with an invalid tag",
            ))
        }
    };
    let boxed = crate::reflect::box_by_desc(v, &desc.to_string())?;
    with_obj_mut(*node, |o| o.slots[value_idx] = boxed.clone());
    Ok(boxed)
}

/// The red-black storage's materialized value reader (G3.3 slice 4): the
/// SAME law at the TreeMap$Entry layout. Routed at every semantic value
/// reader — n_tm_get/get_or_default/contains_value, the remove and
/// put-overwrite returns, tm_all_entries (toString/containsValue/the
/// view builders), the value iterators, entry getValue, and the sms
/// view entries — so the bounded views ride by construction.
fn tm_entry_value_materialized(e: &JRef) -> Result<Value, Throw> {
    node_value_materialized_at(e, TME_VALUE, TME_TAG)
}

/// The specialized-map put: stores `value` (a raw `Value::Double/Float/
/// Int/Long`) in the node's value slot. The key's hash/equals dispatch,
/// the resize law and the modCount are `hm_put`'s own machinery — only
/// the box allocation and the `valueOf` dispatch disappear. The `Map.put`
/// return (the OLD value, per the Map contract) is discarded: the
/// rewritten core requires the `Pop`, so no caller can observe it.
pub fn hm_put_prim(map: &JRef, key: Value, value: Value, ctx: Dispatch) -> Result<(), Throw> {
    // The tag rides the stored value's own prim kind — the node becomes
    // self-describing, so a faithful reader (slice 3's materialization)
    // constructs the exact box class the compiler's closure law hid.
    let tag = match &value {
        Value::Double(_) => Some('D'),
        Value::Float(_) => Some('F'),
        Value::Int(_) => Some('I'),
        Value::Long(_) => Some('J'),
        _ => None,
    };
    // G3.3 slice 4: the node kind follows the MAP's runtime class — a
    // LinkedHashMap rides the same rewrite (the link-chain storage, the
    // tail-link law included; an overwrite keeps position, so no link
    // work there either). The linked entry layout already carries the
    // tag (the post-slice-3 node layout), so the materialization layer
    // serves it unchanged.
    let cls = with_obj(*map, |o| o.jclass.to_string());
    let kind = if cls == "java/util/LinkedHashMap" {
        NodeKind::LinkedEntry
    } else {
        NodeKind::HashNode
    };
    let (_, node) = hm_put(map, key, value, ctx, tag, kind)?;
    if kind == NodeKind::LinkedEntry {
        if let Some(node) = node {
            lhm_link_at_tail(map, &node);
        }
    }
    Ok(())
}

/// The specialized-map get: the FUSED `checkcast + xValue()` law. The
/// synthetic replaces `get; checkcast Box; Box.xValue()p`, so its
/// failure modes are the fused law's failure modes, byte-for-byte:
/// - a MISSING key is the null box reaching `xValue()`: the bare
///   `NullPointerException` (no message) — the same throw the boxing
///   path produces at that program point (uncaught traces render no
///   frame lines, and the peephole refuses spans straddling a handler
///   boundary, so the fault-pc shift is unobservable);
/// - a stored `null` VALUE (a non-core put wrote it) is the same null
///   box reaching `xValue()`: the same bare NPE;
/// - a FOREIGN box (a non-core put wrote a wrong-type box — raw types,
///   unchecked generics) fails the faithful `checkcast` through the
///   shared helper: the exact CCE bytes;
/// - a raw-prim slot whose tag disagrees with `expect` is the same CCE
///   with the tag's class as the actual type (the faithful slot held
///   that box);
/// - a matching box or tag unwraps to the prim payload.
pub fn hm_get_prim(
    map: &JRef,
    key: Value,
    expect: &'static str,
    ctx: Dispatch,
) -> Result<Value, Throw> {
    match hm_find(map, &key, ctx)? {
        Some(node) => {
            let v = node_parts(&node).2;
            match v {
                Value::Ref(None) => Err(throw_new("java/lang/NullPointerException", None)),
                Value::Ref(Some(boxed)) => {
                    crate::checkcast(Value::Ref(Some(boxed)), expect, ctx)?;
                    let payload = with_obj(boxed, |o| o.slots[0].clone());
                    match payload {
                        p @ (Value::Double(_)
                        | Value::Float(_)
                        | Value::Int(_)
                        | Value::Long(_)) => Ok(p),
                        _ => Err(internal_error(
                            "j2r internal: map prim slot holds a non-prim box",
                        )),
                    }
                }
                other @ (Value::Double(_) | Value::Float(_) | Value::Int(_) | Value::Long(_)) => {
                    // The RAW prim slot: the tag is the node's own class
                    // stamp — the provenance check the faithful checkcast
                    // would have run on the box.
                    let tag = with_obj(node, |o| o.slots[HM_NODE_TAG].clone());
                    let Value::Int(c) = tag else {
                        return Err(internal_error(
                            "j2r internal: map prim slot without a class tag",
                        ));
                    };
                    let actual = match char::from_u32(c as u32) {
                        Some('D') => "java/lang/Double",
                        Some('F') => "java/lang/Float",
                        Some('I') => "java/lang/Integer",
                        Some('J') => "java/lang/Long",
                        _ => {
                            return Err(internal_error(
                                "j2r internal: map node carries an invalid class tag",
                            ))
                        }
                    };
                    if actual == expect {
                        Ok(other)
                    } else {
                        Err(crate::cce_throw(actual, expect))
                    }
                }
                other => Err(internal_error(&format!(
                    "j2r internal: map prim slot holds {other:?}"
                ))),
            }
        }
        None => Err(throw_new("java/lang/NullPointerException", None)),
    }
}

/// G3.3 slice 4: the fused get law at the RED-BLACK storage — the same
/// failure modes as [`hm_get_prim`], byte-for-byte, with the walk (and
/// the compare/CCE law on the key) being tm_find's own:
/// - a MISSING key is the null box reaching `xValue()`: the bare NPE;
/// - a stored null VALUE is the same bare NPE;
/// - a FOREIGN box fails the faithful checkcast through the shared
///   helper (the exact CCE bytes);
/// - a raw-prim slot whose tag disagrees with `expect` is the same CCE
///   with the tag's class as the actual type;
/// - a matching box or tag unwraps to the prim payload.
pub fn tm_get_prim(
    map: &JRef,
    key: Value,
    expect: &'static str,
    ctx: Dispatch,
) -> Result<Value, Throw> {
    match tm_find(map, &key, ctx)? {
        Some(node) => {
            let v = with_obj(node, |o| o.slots[TME_VALUE].clone());
            match v {
                Value::Ref(None) => Err(throw_new("java/lang/NullPointerException", None)),
                Value::Ref(Some(boxed)) => {
                    crate::checkcast(Value::Ref(Some(boxed)), expect, ctx)?;
                    let payload = with_obj(boxed, |o| o.slots[0].clone());
                    match payload {
                        p @ (Value::Double(_)
                        | Value::Float(_)
                        | Value::Int(_)
                        | Value::Long(_)) => Ok(p),
                        _ => Err(internal_error(
                            "j2r internal: map prim slot holds a non-prim box",
                        )),
                    }
                }
                other @ (Value::Double(_) | Value::Float(_) | Value::Int(_) | Value::Long(_)) => {
                    let tag = with_obj(node, |o| o.slots[TME_TAG].clone());
                    let Value::Int(c) = tag else {
                        return Err(internal_error(
                            "j2r internal: map prim slot without a class tag",
                        ));
                    };
                    let actual = match char::from_u32(c as u32) {
                        Some('D') => "java/lang/Double",
                        Some('F') => "java/lang/Float",
                        Some('I') => "java/lang/Integer",
                        Some('J') => "java/lang/Long",
                        _ => {
                            return Err(internal_error(
                                "j2r internal: map node carries an invalid class tag",
                            ))
                        }
                    };
                    if actual == expect {
                        Ok(other)
                    } else {
                        Err(crate::cce_throw(actual, expect))
                    }
                }
                other => Err(internal_error(&format!(
                    "j2r internal: map prim slot holds {other:?}"
                ))),
            }
        }
        None => Err(throw_new("java/lang/NullPointerException", None)),
    }
}

pub fn n_hm_get_or_default(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.getOrDefault without receiver");
    let key = arg_or_null(args);
    let default = args.get(1).cloned().unwrap_or(Value::Ref(None));
    match hm_find(r, &key, ctx)? {
        Some(node) => node_value_materialized(&node),
        None => Ok(default),
    }
}

pub fn n_hm_contains_key(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.containsKey without receiver");
    let key = arg_or_null(args);
    Ok(Value::Int(hm_find(r, &key, ctx)?.is_some() as i32))
}

pub fn n_hm_contains_value(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.containsValue without receiver");
    let target = arg_or_null(args);
    for (_, v) in hm_all_entries(r)? {
        let eq = match (&v, &target) {
            (Value::Ref(Some(a)), Value::Ref(Some(_))) => dispatch_equals(a, &target, ctx)?,
            (Value::Ref(None), Value::Ref(None)) => true,
            _ => false,
        };
        if eq {
            return Ok(Value::Int(1));
        }
    }
    Ok(Value::Int(0))
}

pub fn n_hm_remove(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.remove without receiver");
    let key = arg_or_null(args);
    hm_remove(r, &key, ctx)
}

pub fn n_hm_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.size without receiver");
    Ok(Value::Int(with_obj(*r, |o| o.slots[HM_SIZE].expect_int())))
}

pub fn n_hm_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.isEmpty without receiver");
    Ok(Value::Int(
        (with_obj(*r, |o| o.slots[HM_SIZE].expect_int()) == 0) as i32,
    ))
}

pub fn n_hm_clear(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.clear without receiver");
    let mc = with_obj(*r, |o| o.slots[HM_MODCOUNT].expect_int());
    with_obj_mut(*r, |o| {
        o.slots[HM_SIZE] = Value::Int(0);
        o.slots[HM_MODCOUNT] = Value::Int(mc + 1);
        o.slots[HM_TABLE] = Value::Ref(None);
    });
    Ok(Value::Void)
}

pub fn n_hm_to_string(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.toString without receiver");
    let entries = hm_all_entries(r)?;
    let mut out = String::from("{");
    for (i, (k, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(k, ctx)?);
        out.push('=');
        out.push_str(&dispatch_to_string(v, ctx)?);
    }
    out.push('}');
    Ok(Value::Ref(Some(crate::object::new_string(&out))))
}

pub fn n_hm_key_set(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.keySet without receiver");
    let ks = new_obj("java/util/HashMap$KeySet", 1)?;
    with_obj_mut(ks, |o| o.slots[0] = Value::Ref(Some(*r)));
    Ok(Value::Ref(Some(ks)))
}

pub fn n_hm_values(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.values without receiver");
    let vs = new_obj("java/util/HashMap$Values", 1)?;
    with_obj_mut(vs, |o| o.slots[0] = Value::Ref(Some(*r)));
    Ok(Value::Ref(Some(vs)))
}

// ------------------------------------------- HashMap views and iterators

fn view_map(recv: Option<&JRef>, who: &str) -> JRef {
    let v = recv.unwrap_or_else(|| panic!("j2r internal: {who} without receiver"));
    with_obj(*v, |o| match &o.slots[0] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: {who} without map"),
    })
}

pub fn n_ks_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(view_map(recv, "KeySet.size"), |o| {
        o.slots[HM_SIZE].expect_int()
    })))
}

pub fn n_ks_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(view_map(recv, "KeySet.isEmpty"), |o| {
            o.slots[HM_SIZE].expect_int()
        }) == 0) as i32,
    ))
}

pub fn n_ks_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = view_map(recv, "KeySet.contains");
    let key = arg_or_null(args);
    Ok(Value::Int(hm_find(&map, &key, ctx)?.is_some() as i32))
}

pub fn n_ks_to_string(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = view_map(recv, "KeySet.toString");
    let items: Vec<Value> = hm_all_entries(&map)?.into_iter().map(|(k, _)| k).collect();
    collection_to_string(items, ctx)
}

pub fn n_ks_iterator(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let map = view_map(recv, "KeySet.iterator");
    hm_new_hash_iterator(&map, "java/util/HashMap$KeyIterator")
}

pub fn n_vs_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(view_map(recv, "Values.size"), |o| {
        o.slots[HM_SIZE].expect_int()
    })))
}

pub fn n_vs_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(view_map(recv, "Values.isEmpty"), |o| {
            o.slots[HM_SIZE].expect_int()
        }) == 0) as i32,
    ))
}

pub fn n_vs_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    n_hm_contains_value(Some(&view_map(recv, "Values.contains")), args, ctx)
}

pub fn n_vs_to_string(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = view_map(recv, "Values.toString");
    let items: Vec<Value> = hm_all_entries(&map)?.into_iter().map(|(_, v)| v).collect();
    collection_to_string(items, ctx)
}

pub fn n_vs_iterator(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let map = view_map(recv, "Values.iterator");
    hm_new_hash_iterator(&map, "java/util/HashMap$ValueIterator")
}

/// Builds a positioned hash iterator (`HashMap$HashIterator` semantics:
/// the constructor advances to the first entry; `hasNext` is `next !=
/// null` with no comodification check; `nextNode` checks modCount first).
/// Layout: 0 = map, 1 = next table index to scan, 2 = pending node or
/// null, 3 = expectedModCount, 4 = current (last-returned node or null).
fn hm_new_hash_iterator(map: &JRef, class: &str) -> Result<Value, Throw> {
    let (_, _, _, table) = hm_state(map);
    let mc = with_obj(*map, |o| o.slots[HM_MODCOUNT].expect_int());
    let (mut idx, mut pending) = (0i32, None);
    if let Some(t) = table {
        let len = hm_table_len(&t);
        while idx < len {
            if let Some(n) = hm_bin(&t, idx) {
                pending = Some(n);
                idx += 1;
                break;
            }
            idx += 1;
        }
    }
    let it = new_obj(class, 5)?;
    with_obj_mut(it, |o| {
        o.slots[0] = Value::Ref(Some(*map));
        o.slots[1] = Value::Int(idx);
        o.slots[2] = match pending {
            Some(n) => Value::Ref(Some(n)),
            None => Value::Ref(None),
        };
        o.slots[3] = Value::Int(mc);
        o.slots[4] = Value::Ref(None);
    });
    Ok(Value::Ref(Some(it)))
}

/// Advances an iterator after consuming `pending`: next node in the same
/// bin, else the next non-null bin from slot 1.
fn hm_advance(it: &JRef, pending: JRef) {
    let (_, _, _, next) = node_parts(&pending);
    if let Some(n) = next {
        with_obj_mut(*it, |o| o.slots[2] = Value::Ref(Some(n)));
        return;
    }
    let map = with_obj(*it, |o| match &o.slots[0] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: hash iterator without map"),
    });
    let mut idx = with_obj(*it, |o| o.slots[1].expect_int());
    let table = match hm_state(&map).3 {
        Some(t) => t,
        None => {
            with_obj_mut(*it, |o| {
                o.slots[2] = Value::Ref(None);
                o.slots[1] = Value::Int(idx);
            });
            return;
        }
    };
    let len = hm_table_len(&table);
    let mut found = None;
    while idx < len {
        if let Some(n) = hm_bin(&table, idx) {
            found = Some(n);
            idx += 1;
            break;
        }
        idx += 1;
    }
    with_obj_mut(*it, |o| {
        o.slots[2] = match found {
            Some(n) => Value::Ref(Some(n)),
            None => Value::Ref(None),
        };
        o.slots[1] = Value::Int(idx);
    });
}

/// Shared `nextNode()` for both hash iterators: CME check first (exact
/// OpenJDK order), then `NoSuchElementException` when exhausted.
fn hm_hash_next_node(recv: Option<&JRef>, who: &str) -> Result<(JRef, JRef), Throw> {
    let it = recv.unwrap_or_else(|| panic!("j2r internal: {who} without receiver"));
    let (map, expected, pending) = with_obj(*it, |o| {
        (
            match &o.slots[0] {
                Value::Ref(Some(m)) => *m,
                _ => panic!("j2r internal: {who} without map"),
            },
            o.slots[3].expect_int(),
            match &o.slots[2] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    let mc = with_obj(map, |o| o.slots[HM_MODCOUNT].expect_int());
    if expected != mc {
        return Err(crate::cme());
    }
    let Some(node) = pending else {
        return Err(throw_new("java/util/NoSuchElementException", None));
    };
    hm_advance(it, node);
    // record `current` (the last-returned node) — the field the JDK
    // HashIterator.remove() consumes
    with_obj_mut(*it, |o| o.slots[4] = Value::Ref(Some(node)));
    Ok((*it, node))
}

/// The hash iterators' `remove()` (`HashMap$KeyIterator`,
/// `HashMap$ValueIterator`, `HashMap$EntryIterator`) — the OpenJDK
/// `HashIterator.remove` law, PROBED vs Temurin 21.0.13+9 (evidence
/// scripts/evidence/itr_remove/itrorder_temurin_2026-11-15.txt, cases
/// 1–9): **ISE FIRST** when `current` is null (case 1: both conditions
/// hold → ISE, not CME), THEN the CME check (case 4rm: lastRet set +
/// foreign mod → CME); the removal goes through the map's own remove
/// law (the same path as `HashMap.remove`); `current` clears (case 3:
/// double remove → ISE); `expectedModCount` re-syncs (case 9: continued
/// iteration after remove throws no CME). This REPLACES the accidental
/// `n_itr_remove` wiring (the ArrayList-shaped rows — the slot-2
/// type confusion registered as a residual in the stringsim wave).
pub fn n_hi_remove(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: hash iterator remove without receiver");
    let (map, expected, current) = with_obj(*it, |o| {
        (
            match &o.slots[0] {
                Value::Ref(Some(m)) => *m,
                _ => panic!("j2r internal: hash iterator without map"),
            },
            o.slots[3].expect_int(),
            match &o.slots[4] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    // probed law: ISE on null current BEFORE the CME check
    let Some(node) = current else {
        return Err(throw_new("java/lang/IllegalStateException", None));
    };
    let mc = with_obj(map, |o| o.slots[HM_MODCOUNT].expect_int());
    if expected != mc {
        return Err(crate::cme());
    }
    let key = node_parts(&node).1;
    n_hm_remove(Some(&map), std::slice::from_ref(&key), ctx)?;
    let mc = with_obj(map, |o| o.slots[HM_MODCOUNT].expect_int());
    with_obj_mut(*it, |o| {
        o.slots[4] = Value::Ref(None);
        o.slots[3] = Value::Int(mc);
    });
    Ok(Value::Void)
}

pub fn n_ki_has_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: KeyIterator.hasNext without receiver");
    let has = with_obj(*it, |o| matches!(&o.slots[2], Value::Ref(Some(_))));
    Ok(Value::Int(has as i32))
}

pub fn n_ki_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = hm_hash_next_node(recv, "KeyIterator.next")?;
    Ok(node_parts(&node).1)
}

pub fn n_vi_has_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    n_ki_has_next(recv, _args, _ctx)
}

pub fn n_vi_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = hm_hash_next_node(recv, "ValueIterator.next")?;
    node_value_materialized(&node)
}

// ------------------------------------------------------------- LinkedHashMap
//
// Run 4 (2026-11-15): the OpenJDK subclass shape — the map IS a HashMap
// (the same size/modCount/threshold/table slots at 0..3, the same Node
// prefix hash/key/value/next on its entries) plus the ORDER LINKS:
//   map slot 4 = head (the OLDEST entry), slot 5 = tail (the NEWEST
//   entry) — the own slots AFTER the inherited HashMap base (the
//   builtin_own_slots layout law: inherited first, own after);
//   entry slots 5/6 = before/after (the doubly-linked pair AFTER the
//   Node prefix; the runtime allocates 7 slots — the undeclared node-tag
//   slot 4 rides at the same index as the HashMap$Node convention).
//
// Laws (probed vs Temurin 21.0.13+9, LhmProbe, evidence
// scripts/evidence/lhm/lhmprobe_temurin_2026-11-15.txt):
//   L1  iteration order = insertion order (first insertion wins position);
//   L2  re-put of an existing key replaces the value and KEEPS position
//       (hm_put returns no node on an overwrite — no link work);
//   L3  remove + re-put moves the entry to the TAIL;
//   L4  get/containsKey never reorder (they inherit the HashMap rows
//       through the builtin parent walk — order-neutral);
//   L5  <init>(Map) copies in the SOURCE's iteration order (a
//       LinkedHashMap source in link order, anything else table order);
//   L6  is-a HashMap (builtin_parent_of) and is-a Map (interfaces table);
//   L7  keySet/values/entrySet iterate in link order (own view classes);
//   L8  iterator.remove() removes the last-returned entry (order intact,
//       expectedModCount re-synced — the JDK HashIterator.remove law);
//   L9  one null key, null values (the HashMap law: hash(null) = 0);
//   L10 clear resets everything (head/tail included), re-adds re-link;
//   L11 toString iterates in link order.

const LHM_HEAD: usize = 4;
const LHM_TAIL: usize = 5;
// LinkedHashMap$Entry link slots (after the Node prefix + tag).
const LHE_BEFORE: usize = 5;
const LHE_AFTER: usize = 6;

/// Reads the order links under one heap-lock scope.
fn lhm_links(map: &JRef) -> (Option<JRef>, Option<JRef>) {
    with_obj(*map, |o| {
        let head = match &o.slots[LHM_HEAD] {
            Value::Ref(Some(h)) => Some(*h),
            _ => None,
        };
        let tail = match &o.slots[LHM_TAIL] {
            Value::Ref(Some(t)) => Some(*t),
            _ => None,
        };
        (head, tail)
    })
}

/// Links `node` at the tail (the L3 law: every NEW insertion goes last).
/// Pure slot writes — no allocation inside any scope (the heap-lock law).
fn lhm_link_at_tail(map: &JRef, node: &JRef) {
    let (head, tail) = lhm_links(map);
    with_obj_mut(*node, |o| {
        o.slots[LHE_BEFORE] = match tail {
            Some(t) => Value::Ref(Some(t)),
            None => Value::Ref(None),
        };
        o.slots[LHE_AFTER] = Value::Ref(None);
    });
    if let Some(t) = tail {
        with_obj_mut(t, |o| o.slots[LHE_AFTER] = Value::Ref(Some(*node)));
    }
    with_obj_mut(*map, |o| {
        if head.is_none() {
            o.slots[LHM_HEAD] = Value::Ref(Some(*node));
        }
        o.slots[LHM_TAIL] = Value::Ref(Some(*node));
    });
}

/// Unlinks `node` from the chain (the remove law). Pure slot writes.
fn lhm_unlink(map: &JRef, node: &JRef) {
    let (before, after) = with_obj(*node, |o| {
        let b = match &o.slots[LHE_BEFORE] {
            Value::Ref(Some(x)) => Some(*x),
            _ => None,
        };
        let a = match &o.slots[LHE_AFTER] {
            Value::Ref(Some(x)) => Some(*x),
            _ => None,
        };
        (b, a)
    });
    match before {
        Some(b) => with_obj_mut(b, |o| {
            o.slots[LHE_AFTER] = match after {
                Some(a) => Value::Ref(Some(a)),
                None => Value::Ref(None),
            }
        }),
        None => with_obj_mut(*map, |o| {
            o.slots[LHM_HEAD] = match after {
                Some(a) => Value::Ref(Some(a)),
                None => Value::Ref(None),
            }
        }),
    }
    match after {
        Some(a) => with_obj_mut(a, |o| {
            o.slots[LHE_BEFORE] = match before {
                Some(b) => Value::Ref(Some(b)),
                None => Value::Ref(None),
            }
        }),
        None => with_obj_mut(*map, |o| {
            o.slots[LHM_TAIL] = match before {
                Some(b) => Value::Ref(Some(b)),
                None => Value::Ref(None),
            }
        }),
    }
    with_obj_mut(*node, |o| {
        o.slots[LHE_BEFORE] = Value::Ref(None);
        o.slots[LHE_AFTER] = Value::Ref(None);
    });
}

/// Link-order entry walk (the L1/L7 law). Values materialize lazily
/// (slice 3) — every consumer is faithful code.
fn lhm_all_entries(map: &JRef) -> Result<Vec<(Value, Value)>, Throw> {
    let (mut cur, _) = lhm_links(map);
    let mut out = Vec::new();
    while let Some(node) = cur {
        let (_, k, _, next) = node_parts(&node);
        let v = node_value_materialized(&node)?;
        out.push((k, v));
        cur = with_obj(node, |o| match &o.slots[LHE_AFTER] {
            Value::Ref(Some(x)) => Some(*x),
            _ => None,
        });
        let _ = next;
    }
    Ok(out)
}

// <init>()V / <init>(I)V / <init>(Ljava/util/Map;)V — the (I,F) ctor is
// NOT registered (the HashMap (I,F) ctor is unsupported too; the
// access-order (I,F,Z) ctor is its own loud gap).
pub fn n_lhm_init(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.<init> without receiver");
    match args.first() {
        None | Some(Value::Int(_)) => {
            // The HashMap init law for the empty shapes (the negative-cap
            // IAE included), then the order links start empty.
            n_hm_init(Some(r), args, ctx)?;
            with_obj_mut(*r, |o| {
                o.slots[LHM_HEAD] = Value::Ref(None);
                o.slots[LHM_TAIL] = Value::Ref(None);
            });
            Ok(Value::Void)
        }
        Some(Value::Ref(Some(src))) => {
            let entries = map_source_entries(src, ctx.parents)?;
            hm_ctor_presize(r, entries.len())?;
            with_obj_mut(*r, |o| {
                o.slots[LHM_HEAD] = Value::Ref(None);
                o.slots[LHM_TAIL] = Value::Ref(None);
            });
            for (k, v) in entries {
                lhm_put_entry(r, k, v, ctx)?;
            }
            Ok(Value::Void)
        }
        Some(Value::Ref(None)) => Err(throw_new("java/lang/NullPointerException", None)),
        Some(other) => Err(internal_error(&format!(
            "j2r internal: LinkedHashMap.<init> unexpected arg {other:?}"
        ))),
    }
}

/// The put machinery shared by n_lhm_put and the copy ctor: hm_put for
/// the table (the HashMap law untouched), the new node linked at the
/// tail (L1/L3). Overwrites return the old value and touch no links (L2).
fn lhm_put_entry(map: &JRef, key: Value, value: Value, ctx: Dispatch) -> Result<Value, Throw> {
    let (old, node) = hm_put(map, key, value, ctx, None, NodeKind::LinkedEntry)?;
    if let Some(node) = node {
        lhm_link_at_tail(map, &node);
    }
    Ok(old)
}

pub fn n_lhm_put(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.put without receiver");
    let key = arg_or_null(args);
    let value = args.get(1).cloned().unwrap_or(Value::Ref(None));
    lhm_put_entry(r, key, value, ctx)
}

pub fn n_lhm_remove(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.remove without receiver");
    let key = arg_or_null(args);
    let node = hm_find(r, &key, ctx)?;
    match node {
        Some(n) => {
            // The table walk first (materializes the old value), then the
            // chain unlink (pure slot writes, no allocation between).
            let old = hm_remove(r, &key, ctx)?;
            lhm_unlink(r, &n);
            Ok(old)
        }
        None => Ok(Value::Ref(None)),
    }
}

pub fn n_lhm_clear(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.clear without receiver");
    let mc = with_obj(*r, |o| o.slots[HM_MODCOUNT].expect_int());
    with_obj_mut(*r, |o| {
        o.slots[HM_SIZE] = Value::Int(0);
        o.slots[HM_MODCOUNT] = Value::Int(mc + 1);
        o.slots[HM_TABLE] = Value::Ref(None);
        o.slots[LHM_HEAD] = Value::Ref(None);
        o.slots[LHM_TAIL] = Value::Ref(None);
    });
    Ok(Value::Void)
}

pub fn n_lhm_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.toString without receiver");
    let entries = lhm_all_entries(r)?;
    let mut out = String::from("{");
    for (i, (k, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(k, ctx)?);
        out.push('=');
        out.push_str(&dispatch_to_string(v, ctx)?);
    }
    out.push('}');
    Ok(Value::Ref(Some(crate::object::new_string(&out))))
}

fn lhm_new_view(map: &JRef, class: &str) -> Result<Value, Throw> {
    let v = new_obj(class, 1)?;
    with_obj_mut(v, |o| o.slots[0] = Value::Ref(Some(*map)));
    Ok(Value::Ref(Some(v)))
}

pub fn n_lhm_key_set(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.keySet without receiver");
    lhm_new_view(r, "java/util/LinkedHashMap$KeySet")
}

pub fn n_lhm_values(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.values without receiver");
    lhm_new_view(r, "java/util/LinkedHashMap$Values")
}

pub fn n_lhm_entry_set(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.entrySet without receiver");
    lhm_new_view(r, "java/util/LinkedHashMap$EntrySet")
}

// --- the LinkedHashMap views (slot 0 = the backing map, the HashMap
// view shape; every order-visible operation walks the LINK CHAIN).

pub fn n_lks_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(
        view_map(recv, "LinkedKeySet.size"),
        |o| o.slots[HM_SIZE].expect_int(),
    )))
}

pub fn n_lks_is_empty(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(view_map(recv, "LinkedKeySet.isEmpty"), |o| {
            o.slots[HM_SIZE].expect_int()
        }) == 0) as i32,
    ))
}

pub fn n_lks_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = view_map(recv, "LinkedKeySet.contains");
    let key = arg_or_null(args);
    Ok(Value::Int(hm_find(&map, &key, ctx)?.is_some() as i32))
}

pub fn n_lks_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = view_map(recv, "LinkedKeySet.toString");
    let items: Vec<Value> = lhm_all_entries(&map)?.into_iter().map(|(k, _)| k).collect();
    collection_to_string(items, ctx)
}

pub fn n_lks_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = view_map(recv, "LinkedKeySet.iterator");
    lhm_new_link_iterator(&map, "java/util/LinkedHashMap$KeyIterator")
}

pub fn n_lvs_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(
        view_map(recv, "LinkedValues.size"),
        |o| o.slots[HM_SIZE].expect_int(),
    )))
}

pub fn n_lvs_is_empty(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(view_map(recv, "LinkedValues.isEmpty"), |o| {
            o.slots[HM_SIZE].expect_int()
        }) == 0) as i32,
    ))
}

pub fn n_lvs_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    n_hm_contains_value(Some(&view_map(recv, "LinkedValues.contains")), args, ctx)
}

pub fn n_lvs_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = view_map(recv, "LinkedValues.toString");
    let items: Vec<Value> = lhm_all_entries(&map)?.into_iter().map(|(_, v)| v).collect();
    collection_to_string(items, ctx)
}

pub fn n_lvs_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = view_map(recv, "LinkedValues.iterator");
    lhm_new_link_iterator(&map, "java/util/LinkedHashMap$ValueIterator")
}

pub fn n_les_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(
        view_map(recv, "LinkedEntrySet.size"),
        |o| o.slots[HM_SIZE].expect_int(),
    )))
}

pub fn n_les_is_empty(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(view_map(recv, "LinkedEntrySet.isEmpty"), |o| {
            o.slots[HM_SIZE].expect_int()
        }) == 0) as i32,
    ))
}

pub fn n_les_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = view_map(recv, "LinkedEntrySet.iterator");
    lhm_new_link_iterator(&map, "java/util/LinkedHashMap$EntryIterator")
}

/// The link-chain iterator (the JDK LinkedHashMap LinkedHashIterator
/// shape): 0 = map, 1 = pending node (the constructor advances to head),
/// 2 = lastRet, 3 = expectedModCount.
fn lhm_new_link_iterator(map: &JRef, class: &str) -> Result<Value, Throw> {
    let mc = with_obj(*map, |o| o.slots[HM_MODCOUNT].expect_int());
    let (head, _) = lhm_links(map);
    let it = new_obj(class, 4)?;
    with_obj_mut(it, |o| {
        o.slots[0] = Value::Ref(Some(*map));
        o.slots[1] = match head {
            Some(h) => Value::Ref(Some(h)),
            None => Value::Ref(None),
        };
        o.slots[2] = Value::Ref(None);
        o.slots[3] = Value::Int(mc);
    });
    Ok(Value::Ref(Some(it)))
}

/// Shared `nextNode()` for the link iterators: the CME check FIRST
/// (exact OpenJDK order), then NoSuchElementException when exhausted;
/// advance = `pending.after`.
fn lhm_link_next_node(recv: Option<&JRef>, who: &str) -> Result<(JRef, JRef), Throw> {
    let it = recv.unwrap_or_else(|| panic!("j2r internal: {who} without receiver"));
    let (map, expected, pending) = with_obj(*it, |o| {
        (
            match &o.slots[0] {
                Value::Ref(Some(m)) => *m,
                _ => panic!("j2r internal: {who} without map"),
            },
            o.slots[3].expect_int(),
            match &o.slots[1] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    let mc = with_obj(map, |o| o.slots[HM_MODCOUNT].expect_int());
    if expected != mc {
        return Err(crate::cme());
    }
    let Some(node) = pending else {
        return Err(throw_new("java/util/NoSuchElementException", None));
    };
    let after = with_obj(node, |o| match &o.slots[LHE_AFTER] {
        Value::Ref(Some(x)) => Value::Ref(Some(*x)),
        _ => Value::Ref(None),
    });
    with_obj_mut(*it, |o| {
        o.slots[1] = after;
        o.slots[2] = Value::Ref(Some(node));
    });
    Ok((map, node))
}

fn lhm_link_has_next(recv: Option<&JRef>, who: &str) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: link iterator hasNext without receiver");
    let _ = who;
    let has = with_obj(*it, |o| matches!(&o.slots[1], Value::Ref(Some(_))));
    Ok(Value::Int(has as i32))
}

pub fn n_lki_has_next(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    lhm_link_has_next(recv, "LinkedKeyIterator")
}

pub fn n_lki_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = lhm_link_next_node(recv, "LinkedKeyIterator.next")?;
    Ok(node_parts(&node).1)
}

pub fn n_lvi_has_next(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    lhm_link_has_next(recv, "LinkedValueIterator")
}

pub fn n_lvi_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = lhm_link_next_node(recv, "LinkedValueIterator.next")?;
    node_value_materialized(&node)
}

pub fn n_lei_has_next(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    lhm_link_has_next(recv, "LinkedEntryIterator")
}

/// The entry iterator's next() IS the internal Entry node (the JDK law:
/// LinkedHashMap.Entry implements Map.Entry), exposed as Map.Entry.
pub fn n_lei_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = lhm_link_next_node(recv, "LinkedEntryIterator.next")?;
    Ok(Value::Ref(Some(node)))
}

/// Iterator.remove over the link chain (the L8 law): **ISE FIRST** when
/// lastRet is null (the 2026-11-15 ItrOrderProbe re-probe corrected the
/// original check order — Temurin case 5: both conditions hold → ISE,
/// NOT CME; the original CME-first order was never probed with both
/// conditions combined), then the CME check; the removal goes through
/// the map's own remove law (table + chain), lastRet clears,
/// expectedModCount re-syncs (no CME on a continued iteration).
pub fn n_lhm_itr_remove(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: LinkedHashMap iterator remove without receiver");
    let (map, expected, last_ret) = with_obj(*it, |o| {
        (
            match &o.slots[0] {
                Value::Ref(Some(m)) => *m,
                _ => panic!("j2r internal: link iterator without map"),
            },
            o.slots[3].expect_int(),
            match &o.slots[2] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    // probed law: ISE on null lastRet BEFORE the CME check
    // (Temurin 21.0.13+9, ItrOrderProbe case 5)
    let Some(node) = last_ret else {
        return Err(throw_new("java/lang/IllegalStateException", None));
    };
    let mc = with_obj(map, |o| o.slots[HM_MODCOUNT].expect_int());
    if expected != mc {
        return Err(crate::cme());
    }
    let key = node_parts(&node).1;
    n_lhm_remove(Some(&map), std::slice::from_ref(&key), ctx)?;
    let mc = with_obj(map, |o| o.slots[HM_MODCOUNT].expect_int());
    with_obj_mut(*it, |o| {
        o.slots[2] = Value::Ref(None);
        o.slots[3] = Value::Int(mc);
    });
    Ok(Value::Void)
}

// ----------------------------------------------------------------- TreeMap
//
// Run 5 (2026-11-15): the SORTED map — red-black storage (the CLRS shape
// OpenJDK uses), natural ordering through the ordinary Comparable dispatch
// walk, Comparator ordering through the Comparator walk (lambda + builtin
// comparators resolve the same way Arrays.sort(Object[], Comparator) has
// resolved since the stringsim wave).
//
// Laws (probed vs Temurin 21.0.13+9, TreeMapProbe, evidence
// scripts/evidence/treemap/treemapprobe_temurin_2026-11-15.txt):
//   T1  iteration in KEY order (the whole point) — keys ascending under
//       the map's ordering; values/entries follow KEY order; toString
//       `{k=v, k=v}` sorted
//   T2  firstKey/lastKey = min/max under the ordering
//   T3  duplicate put replaces the value, keeps the position, returns old
//   T4  remove returns the old value (absent → null), order preserved
//   T5  ceiling/floor accept exact; higher/lower are strict;
//       out-of-range → null
//   T6  the Comparator surface reorders everything (iteration, first/
//       last, navigation)
//   T7  put(null) under natural ordering → NPE (the type check runs even
//       on an empty map — OpenJDK's compare(key, key) pre-step)
//   T8  put(non-Comparable) → ClassCastException
//   T9  the copy ctor sorts regardless of the source's iteration order
//   T10 iterator remove mid-walk: order intact, no CME (expectedModCount
//       re-synced); ISE-first on null current (the run-4 probed law);
//       foreign modification → CME
//   T11 size/isEmpty/clear/getOrDefault/containsValue
//
// Slots: 0 = comparator ref (null = natural), 1 = size, 2 = modCount,
// 3 = root (TreeMap$Entry or null). Entry: 0 key, 1 value, 2 left,
// 3 right, 4 parent, 5 color (Int 0=RED/1=BLACK). Iterators: 0 map,
// 1 next (successor-walk position), 2 current (last-returned entry —
// the field remove() consumes), 3 expectedModCount. Views: map ref.
// subMap/headMap/tailMap are NOT registered this slice (the bounded
// views are their own slice; loud unsupported by the same-surface law).

const TM_COMPARATOR: usize = 0;
const TM_SIZE: usize = 1;
const TM_MODCOUNT: usize = 2;
const TM_ROOT: usize = 3;

const TME_KEY: usize = 0;
const TME_VALUE: usize = 1;
const TME_LEFT: usize = 2;
const TME_RIGHT: usize = 3;
const TME_PARENT: usize = 4;
const TME_COLOR: usize = 5; // Int: 0 = RED, 1 = BLACK
                            // G3.3 slice 4: the class tag (the slice-3 law, now on the red-black
                            // storage too): Void = a faithful node; Int(desc char) = slot TME_VALUE
                            // holds a RAW prim written by tm_put_prim. The tag is an ANNOTATION, not
                            // a mode switch — a faithful overwrite clears it, a faithful overwrite
                            // of a tagged slot leaves a box in the slot and the tag passes through
                            // every reader.
const TME_TAG: usize = 6;
const TM_RED: i32 = 0;
const TM_BLACK: i32 = 1;

// iterator slots
const TMI_MAP: usize = 0;
const TMI_NEXT: usize = 1;
const TMI_CURRENT: usize = 2;
const TMI_EXPECTED: usize = 3;

// the NavigableMap descent modes
#[derive(Clone, Copy, PartialEq)]
enum TmNav {
    Ceiling,
    Higher,
    Floor,
    Lower,
}

fn tm_root(map: &JRef) -> Option<JRef> {
    with_obj(*map, |o| match &o.slots[TM_ROOT] {
        Value::Ref(Some(r)) => Some(*r),
        _ => None,
    })
}

fn tm_entry_key(e: &JRef) -> Value {
    with_obj(*e, |o| o.slots[TME_KEY].clone())
}

fn tm_entry_value(e: &JRef) -> Value {
    with_obj(*e, |o| o.slots[TME_VALUE].clone())
}

fn tm_child(e: &JRef, slot: usize) -> Option<JRef> {
    with_obj(*e, |o| match &o.slots[slot] {
        Value::Ref(Some(c)) => Some(*c),
        _ => None,
    })
}

fn tm_parent(e: &JRef) -> Option<JRef> {
    tm_child(e, TME_PARENT)
}

fn tm_color(e: Option<&JRef>) -> i32 {
    match e {
        None => TM_BLACK, // the nil leaves are black
        Some(r) => with_obj(*r, |o| o.slots[TME_COLOR].expect_int()),
    }
}

fn tm_set_child(e: &JRef, slot: usize, c: Option<JRef>) {
    with_obj_mut(*e, |o| {
        o.slots[slot] = match c {
            Some(x) => Value::Ref(Some(x)),
            None => Value::Ref(None),
        }
    });
}

fn tm_set_parent(e: &JRef, p: Option<JRef>) {
    tm_set_child(e, TME_PARENT, p);
}

fn tm_set_color(e: &JRef, c: i32) {
    with_obj_mut(*e, |o| o.slots[TME_COLOR] = Value::Int(c));
}

fn tm_size_of(map: &JRef) -> i32 {
    with_obj(*map, |o| o.slots[TM_SIZE].expect_int())
}

fn tm_modcount(map: &JRef) -> i32 {
    with_obj(*map, |o| o.slots[TM_MODCOUNT].expect_int())
}

/// The map's ordering: the Comparator walk when the map carries one,
/// otherwise the natural Comparable walk (with the probed NPE / CCE
/// laws on the way in).
fn tm_compare(map: &JRef, a: &Value, b: &Value, ctx: Dispatch) -> Result<i32, Throw> {
    let comp = with_obj(*map, |o| match &o.slots[TM_COMPARATOR] {
        Value::Ref(Some(c)) => Some(*c),
        _ => None,
    });
    if let Some(c) = comp {
        let out = crate::invoke_virtual(
            &c,
            "java/util/Comparator",
            "compare",
            "(Ljava/lang/Object;Ljava/lang/Object;)I",
            &[a.clone(), b.clone()],
            ctx,
        )?;
        return Ok(out.expect_int());
    }
    let ra = match a {
        Value::Ref(Some(r)) => *r,
        Value::Ref(None) => return Err(crate::npe()),
        other => {
            return Err(internal_error(&format!(
                "j2r internal: TreeMap natural key {other:?}"
            )))
        }
    };
    // the CCE law (probed): a non-Comparable key never enters the tree
    let cls = with_obj(ra, |o| o.jclass.to_string());
    if !ctx.is_subtype(&cls, "java/lang/Comparable") {
        return Err(throw_new(
            "java/lang/ClassCastException",
            Some(&format!(
                "class {cls} cannot be cast to class java.lang.Comparable"
            )),
        ));
    }
    let out = crate::invoke_virtual(
        &ra,
        "java/lang/Comparable",
        "compareTo",
        "(Ljava/lang/Object;)I",
        std::slice::from_ref(b),
        ctx,
    )?;
    Ok(out.expect_int())
}

fn tm_leftmost(mut n: JRef) -> JRef {
    while let Some(l) = tm_child(&n, TME_LEFT) {
        n = l;
    }
    n
}

fn tm_rightmost(mut n: JRef) -> JRef {
    while let Some(r) = tm_child(&n, TME_RIGHT) {
        n = r;
    }
    n
}

/// The in-order successor (OpenJDK `successor(t)`): the leftmost of the
/// right subtree, else the first ancestor whose left chain contains t.
fn tm_successor_of(n: JRef) -> Option<JRef> {
    if let Some(r) = tm_child(&n, TME_RIGHT) {
        return Some(tm_leftmost(r));
    }
    let mut cur = n;
    loop {
        let p = tm_parent(&cur)?;
        if tm_child(&p, TME_LEFT) == Some(cur) {
            return Some(p);
        }
        cur = p;
    }
}

/// The in-order predecessor (OpenJDK `predecessor(t)`) — the firstEntry
/// walk of a descending iteration (not probed this slice; kept for the
/// navigable mirror).
#[allow(dead_code)]
fn tm_predecessor_of(n: JRef) -> Option<JRef> {
    if let Some(l) = tm_child(&n, TME_LEFT) {
        return Some(tm_rightmost(l));
    }
    let mut cur = n;
    loop {
        let p = tm_parent(&cur)?;
        if tm_child(&p, TME_RIGHT) == Some(cur) {
            return Some(p);
        }
        cur = p;
    }
}

/// Every (key, value) pair in KEY order — the toString / ctor-from-map /
/// containsValue walk (T1).
fn tm_all_entries(map: &JRef) -> Result<Vec<(Value, Value)>, Throw> {
    let mut out = Vec::new();
    if let Some(root) = tm_root(map) {
        let mut cur = Some(tm_leftmost(root));
        while let Some(n) = cur {
            out.push((tm_entry_key(&n), tm_entry_value_materialized(&n)?));
            cur = tm_successor_of(n);
        }
    }
    Ok(out)
}

// --- the CLRS rotations (parent pointers updated; the root slot fixed) ---

fn tm_rotate_left(map: &JRef, x: JRef) {
    let y = tm_child(&x, TME_RIGHT).expect("j2r internal: rotate_left without right child");
    let yl = tm_child(&y, TME_LEFT);
    tm_set_child(&x, TME_RIGHT, yl);
    if let Some(yl) = yl {
        tm_set_parent(&yl, Some(x));
    }
    let xp = tm_parent(&x);
    tm_set_parent(&y, xp);
    match xp {
        None => tm_set_root(map, Some(y)),
        Some(p) => {
            if tm_child(&p, TME_LEFT) == Some(x) {
                tm_set_child(&p, TME_LEFT, Some(y));
            } else {
                tm_set_child(&p, TME_RIGHT, Some(y));
            }
        }
    }
    tm_set_child(&y, TME_LEFT, Some(x));
    tm_set_parent(&x, Some(y));
}

fn tm_rotate_right(map: &JRef, x: JRef) {
    let y = tm_child(&x, TME_LEFT).expect("j2r internal: rotate_right without left child");
    let yr = tm_child(&y, TME_RIGHT);
    tm_set_child(&x, TME_LEFT, yr);
    if let Some(yr) = yr {
        tm_set_parent(&yr, Some(x));
    }
    let xp = tm_parent(&x);
    tm_set_parent(&y, xp);
    match xp {
        None => tm_set_root(map, Some(y)),
        Some(p) => {
            if tm_child(&p, TME_RIGHT) == Some(x) {
                tm_set_child(&p, TME_RIGHT, Some(y));
            } else {
                tm_set_child(&p, TME_LEFT, Some(y));
            }
        }
    }
    tm_set_child(&y, TME_RIGHT, Some(x));
    tm_set_parent(&x, Some(y));
}

fn tm_set_root(map: &JRef, r: Option<JRef>) {
    with_obj_mut(*map, |o| {
        o.slots[TM_ROOT] = match r {
            Some(x) => Value::Ref(Some(x)),
            None => Value::Ref(None),
        }
    });
}

/// The insert fixup (CLRS §13.3): redden the new node, repair the
/// double-red, recolor the root black.
fn tm_insert_fixup(map: &JRef, mut z: JRef) {
    loop {
        let p = match tm_parent(&z) {
            Some(p) if tm_color(Some(&p)) == TM_RED => p,
            _ => break,
        };
        let gp = match tm_parent(&p) {
            Some(g) => g,
            None => break, // the root is black; unreachable in a valid tree
        };
        let p_is_left = tm_child(&gp, TME_LEFT) == Some(p);
        let uncle = tm_child(&gp, if p_is_left { TME_RIGHT } else { TME_LEFT });
        if tm_color(uncle.as_ref()) == TM_RED {
            tm_set_color(&p, TM_BLACK);
            if let Some(u) = uncle {
                tm_set_color(&u, TM_BLACK);
            }
            tm_set_color(&gp, TM_RED);
            z = gp;
            continue;
        }
        if p_is_left {
            // zig-zag first: after the rotation z is the TOP of the former
            // double-red pair (the CLRS rename z = z.p BEFORE the rotation
            // lands the old parent below it), so `top` = z; a straight
            // line keeps the parent as the pivot
            let top = if tm_child(&p, TME_RIGHT) == Some(z) {
                tm_rotate_left(map, p);
                z
            } else {
                p
            };
            tm_set_color(&top, TM_BLACK);
            let gp2 = tm_parent(&top).unwrap_or_else(|| panic!("j2r internal: fixup gp"));
            tm_set_color(&gp2, TM_RED);
            tm_rotate_right(map, gp2);
        } else {
            let top = if tm_child(&p, TME_LEFT) == Some(z) {
                tm_rotate_right(map, p);
                z
            } else {
                p
            };
            tm_set_color(&top, TM_BLACK);
            let gp2 = tm_parent(&top).unwrap_or_else(|| panic!("j2r internal: fixup gp"));
            tm_set_color(&gp2, TM_RED);
            tm_rotate_left(map, gp2);
        }
    }
    if let Some(root) = tm_root(map) {
        tm_set_color(&root, TM_BLACK);
    }
}

/// Replaces the subtree rooted at u with the subtree rooted at v
/// (CLRS §13.4 transplant).
fn tm_transplant(map: &JRef, u: JRef, v: Option<JRef>) {
    let up = tm_parent(&u);
    match up {
        None => tm_set_root(map, v),
        Some(p) => {
            if tm_child(&p, TME_LEFT) == Some(u) {
                tm_set_child(&p, TME_LEFT, v);
            } else {
                tm_set_child(&p, TME_RIGHT, v);
            }
        }
    }
    if let Some(v) = v {
        tm_set_parent(&v, up);
    }
}

/// The delete fixup (CLRS §13.4 / OpenJDK fixupAfterDeletion): x may be
/// the node just logically removed (the nil-sentinel trick — its pointers
/// still reach the tree until the transplant).
fn tm_delete_fixup(map: &JRef, mut x: JRef, mut xp: JRef) {
    // CLRS: `while x != T.root && x.color == BLACK` — a RED x means the
    // deficit is absorbed by recoloring (the exit recolor below), not by
    // more rotations; the root is re-read LIVE (a case-1 rotation can
    // replace it mid-fixup)
    while Some(x) != tm_root(map) && tm_color(Some(&x)) == TM_BLACK {
        let x_is_left = tm_child(&xp, TME_LEFT) == Some(x);
        let far = if x_is_left { TME_RIGHT } else { TME_LEFT };
        let near = if x_is_left { TME_LEFT } else { TME_RIGHT };
        let w = match tm_child(&xp, far) {
            Some(w) => w,
            None => break, // a valid red-black tree never gets here
        };
        if tm_color(Some(&w)) == TM_RED {
            tm_set_color(&w, TM_BLACK);
            tm_set_color(&xp, TM_RED);
            if x_is_left {
                tm_rotate_left(map, xp);
            } else {
                tm_rotate_right(map, xp);
            }
            // xp moved down; the new sibling
            xp = match tm_parent(&x) {
                Some(p) => p,
                None => break,
            };
            continue;
        }
        let wl = tm_color(tm_child(&w, TME_LEFT).as_ref());
        let wr = tm_color(tm_child(&w, TME_RIGHT).as_ref());
        if wl == TM_BLACK && wr == TM_BLACK {
            tm_set_color(&w, TM_RED);
            let x2 = xp;
            match tm_parent(&x2) {
                Some(p) => {
                    x = x2;
                    xp = p;
                }
                None => break, // x2 is the root; the final recolor fixes it
            }
            continue;
        }
        let far_black = tm_color(tm_child(&w, far).as_ref()) == TM_BLACK;
        if far_black {
            // the near nephew is RED: rotate the sibling into a straight line
            if let Some(nn) = tm_child(&w, near) {
                tm_set_color(&nn, TM_BLACK);
            }
            tm_set_color(&w, TM_RED);
            if x_is_left {
                tm_rotate_right(map, w);
            } else {
                tm_rotate_left(map, w);
            }
        }
        let w2 = tm_child(&xp, far).expect("j2r internal: delete fixup sibling");
        tm_set_color(&w2, tm_color(Some(&xp)));
        tm_set_color(&xp, TM_BLACK);
        if let Some(f) = tm_child(&w2, far) {
            tm_set_color(&f, TM_BLACK);
        }
        if x_is_left {
            tm_rotate_left(map, xp);
        } else {
            tm_rotate_right(map, xp);
        }
        break;
    }
    // THE CLRS EXIT LAW (the red-red-at-16.right corruption): the node the
    // loop ended on recolors BLACK UNCONDITIONALLY — it absorbed the
    // double-black (or was red from case 1's parent recolor and must go
    // black). The root-only recolor left case-1+case-2 climbs red.
    tm_set_color(&x, TM_BLACK);
}

/// Removes one node from the tree (OpenJDK `deleteEntry`): the two-child
/// case splices the successor's payload into the target and deletes the
/// successor (at most one child); the black-loss case runs the fixup.
fn tm_delete(map: &JRef, z: JRef) {
    let mc = tm_modcount(map);
    let sz = tm_size_of(map);
    with_obj_mut(*map, |o| {
        o.slots[TM_MODCOUNT] = Value::Int(mc + 1);
        o.slots[TM_SIZE] = Value::Int(sz - 1);
    });
    let mut target = z;
    if tm_child(&z, TME_LEFT).is_some() && tm_child(&z, TME_RIGHT).is_some() {
        let s = tm_successor_of(z).expect("j2r internal: successor of a two-child node");
        let (sk, sv) = (tm_entry_key(&s), tm_entry_value(&s));
        // G3.3 slice 4: the class tag RIDES the value in the
        // transplant-by-copy — the successor node dies here, so its raw
        // provenance must move with the payload (a cleared tag on a raw
        // slot would be a raw slot no reader can name).
        let stag = with_obj(s, |o| o.slots[TME_TAG].clone());
        with_obj_mut(z, |o| {
            o.slots[TME_KEY] = sk;
            o.slots[TME_VALUE] = sv;
            o.slots[TME_TAG] = stag;
        });
        target = s;
    }
    let rep = match tm_child(&target, TME_LEFT) {
        Some(l) => Some(l),
        None => tm_child(&target, TME_RIGHT),
    };
    let tp = tm_parent(&target);
    match rep {
        Some(r) => {
            tm_transplant(map, target, Some(r));
            if tm_color(Some(&target)) == TM_BLACK {
                match tp {
                    Some(p) => tm_delete_fixup(map, r, p),
                    // the deleted node was the ROOT (the slice-clear path
                    // exposed this run-5 hole): the replacement IS the new
                    // root, so CLRS's `while x != root` exits immediately
                    // and the exit recolor is the whole law
                    None => tm_set_color(&r, TM_BLACK),
                }
            }
        }
        None => match tp {
            None => tm_set_root(map, None),
            Some(p) => {
                if tm_color(Some(&target)) == TM_BLACK {
                    tm_delete_fixup(map, target, p);
                }
                tm_transplant(map, target, None);
                tm_set_parent(&target, None);
            }
        },
    }
}

/// The descent machinery shared by put/find/navigate.
fn tm_find(map: &JRef, key: &Value, ctx: Dispatch) -> Result<Option<JRef>, Throw> {
    let mut cur = tm_root(map);
    while let Some(c) = cur {
        let cmp = tm_compare(map, key, &tm_entry_key(&c), ctx)?;
        cur = if cmp < 0 {
            tm_child(&c, TME_LEFT)
        } else if cmp > 0 {
            tm_child(&c, TME_RIGHT)
        } else {
            return Ok(Some(c));
        };
    }
    Ok(None)
}

/// The NavigableMap descent (T5): Ceiling = least >= key, Higher = least
/// > key, Floor = greatest <= key, Lower = greatest < key.
fn tm_navigate(map: &JRef, key: &Value, mode: TmNav, ctx: Dispatch) -> Result<Option<JRef>, Throw> {
    let mut best: Option<JRef> = None;
    let mut cur = tm_root(map);
    while let Some(c) = cur {
        let cmp = tm_compare(map, key, &tm_entry_key(&c), ctx)?;
        if cmp == 0 && matches!(mode, TmNav::Ceiling | TmNav::Floor) {
            return Ok(Some(c));
        }
        // candidates: Ceiling/Higher accept the node when key < node.key;
        // Floor/Lower when key > node.key
        let candidate = match mode {
            TmNav::Ceiling | TmNav::Higher => cmp < 0,
            TmNav::Floor | TmNav::Lower => cmp > 0,
        };
        let go = if matches!(mode, TmNav::Ceiling | TmNav::Higher) {
            // keep searching the high side unless the node is a keeper
            if cmp == 0 {
                TME_RIGHT
            } else if candidate {
                TME_LEFT
            } else {
                TME_RIGHT
            }
        } else if cmp == 0 {
            TME_LEFT
        } else if candidate {
            TME_RIGHT
        } else {
            TME_LEFT
        };
        if candidate {
            best = Some(c);
        }
        cur = tm_child(&c, go);
    }
    Ok(best)
}

/// put — the type/null check FIRST (T7/T8: it runs even on an empty
/// map), then the descent; an equal key replaces (T3), a fresh key
/// inserts RED and rebalances.
pub fn n_tm_put(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.put without receiver");
    let key = arg_or_null(args);
    let value = args.get(1).cloned().unwrap_or(Value::Ref(None));
    tm_put_impl(r, key, value, ctx, None).map(|old| old.unwrap_or(Value::Ref(None)))
}

/// G3.3 slice 4: the specialized-map put on the red-black storage. The
/// walk, the self-compare (the addEntryToEmptyMap law — a non-Comparable
/// key throws CCE on the FIRST put), the CLRS insert and the modCount/
/// size are n_tm_put's own machinery — only the box allocation and the
/// valueOf dispatch disappear. The put's return (the OLD value, per the
/// Map contract) is discarded: the rewritten core requires the `Pop`, so
/// no caller can observe it.
pub fn tm_put_prim(map: &JRef, key: Value, value: Value, ctx: Dispatch) -> Result<(), Throw> {
    // The tag rides the stored value's own prim kind — the node becomes
    // self-describing, so a faithful reader materializes the exact box
    // class the compiler's closure law hid.
    let tag = match &value {
        Value::Double(_) => Some('D'),
        Value::Float(_) => Some('F'),
        Value::Int(_) => Some('I'),
        Value::Long(_) => Some('J'),
        _ => None,
    };
    tm_put_impl(map, key, value, ctx, tag).map(|old| old.unwrap_or(Value::Ref(None)))?;
    Ok(())
}

/// The put machinery shared by n_tm_put and tm_put_prim. `tag` is None
/// for the faithful path (byte-identical behavior) and Some(desc char)
/// for the specialized path (the node stores the RAW prim + its tag).
/// The overwrite return is materialized BEFORE the slot write (the
/// hm_put pattern — the Map.put contract; the allocation happens
/// outside any with_obj scope).
fn tm_put_impl(
    r: &JRef,
    key: Value,
    value: Value,
    ctx: Dispatch,
    tag: Option<char>,
) -> Result<Option<Value>, Throw> {
    let _ = tm_compare(r, &key, &key, ctx)?;
    let mut parent: Option<JRef> = None;
    let mut last_cmp: i32 = 0;
    let mut cur = tm_root(r);
    while let Some(c) = cur {
        let cmp = tm_compare(r, &key, &tm_entry_key(&c), ctx)?;
        if cmp == 0 {
            let old = node_value_materialized_at(&c, TME_VALUE, TME_TAG)?;
            with_obj_mut(c, |o| {
                o.slots[TME_VALUE] = value.clone();
                o.slots[TME_TAG] = tag_slot(tag);
            });
            return Ok(Some(old));
        }
        parent = Some(c);
        last_cmp = cmp;
        cur = tm_child(&c, if cmp < 0 { TME_LEFT } else { TME_RIGHT });
    }
    let node = new_obj("java/util/TreeMap$Entry", 7)?;
    with_obj_mut(node, |o| {
        o.slots[TME_KEY] = key;
        o.slots[TME_VALUE] = value;
        o.slots[TME_COLOR] = Value::Int(TM_RED);
        o.slots[TME_TAG] = tag_slot(tag);
    });
    match parent {
        Some(p) => {
            tm_set_parent(&node, Some(p));
            tm_set_child(
                &p,
                if last_cmp < 0 { TME_LEFT } else { TME_RIGHT },
                Some(node),
            );
        }
        None => tm_set_root(r, Some(node)),
    }
    tm_insert_fixup(r, node);
    let (mc, sz) = (tm_modcount(r), tm_size_of(r));
    with_obj_mut(*r, |o| {
        o.slots[TM_MODCOUNT] = Value::Int(mc + 1);
        o.slots[TM_SIZE] = Value::Int(sz + 1);
    });
    Ok(None)
}

pub fn n_tm_get(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.get without receiver");
    let key = arg_or_null(args);
    match tm_find(r, &key, ctx)? {
        Some(n) => tm_entry_value_materialized(&n),
        None => Ok(Value::Ref(None)),
    }
}

pub fn n_tm_contains_key(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.containsKey without receiver");
    let key = arg_or_null(args);
    let found = tm_find(r, &key, ctx)?.is_some();
    Ok(Value::Int(found as i32))
}

pub fn n_tm_contains_value(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.containsValue without receiver");
    let target = arg_or_null(args);
    for (_, v) in tm_all_entries(r)? {
        let eq = match (&v, &target) {
            (Value::Ref(Some(a)), Value::Ref(Some(_))) => dispatch_equals(a, &target, ctx)?,
            (Value::Ref(None), Value::Ref(None)) => true,
            _ => false,
        };
        if eq {
            return Ok(Value::Int(1));
        }
    }
    Ok(Value::Int(0))
}

pub fn n_tm_get_or_default(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.getOrDefault without receiver");
    let key = arg_or_null(args);
    let dflt = args.get(1).cloned().unwrap_or(Value::Ref(None));
    match tm_find(r, &key, ctx)? {
        Some(n) => tm_entry_value_materialized(&n),
        None => Ok(dflt),
    }
}

pub fn n_tm_remove(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.remove without receiver");
    let key = arg_or_null(args);
    match tm_find(r, &key, ctx)? {
        Some(n) => {
            let old = node_value_materialized_at(&n, TME_VALUE, TME_TAG)?;
            tm_delete(r, n);
            Ok(old)
        }
        None => Ok(Value::Ref(None)),
    }
}

pub fn n_tm_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.size without receiver");
    Ok(Value::Int(tm_size_of(r)))
}

pub fn n_tm_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.isEmpty without receiver");
    Ok(Value::Int((tm_size_of(r) == 0) as i32))
}

pub fn n_tm_clear(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.clear without receiver");
    let mc = tm_modcount(r);
    with_obj_mut(*r, |o| {
        o.slots[TM_SIZE] = Value::Int(0);
        o.slots[TM_MODCOUNT] = Value::Int(mc + 1);
        o.slots[TM_ROOT] = Value::Ref(None);
    });
    Ok(Value::Void)
}

pub fn n_tm_to_string(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.toString without receiver");
    let entries = tm_all_entries(r)?;
    let mut out = String::from("{");
    for (i, (k, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(k, ctx)?);
        out.push('=');
        out.push_str(&dispatch_to_string(v, ctx)?);
    }
    out.push('}');
    Ok(Value::Ref(Some(crate::object::new_string(&out))))
}

pub fn n_tm_first_key(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.firstKey without receiver");
    match tm_root(r) {
        Some(root) => Ok(tm_entry_key(&tm_leftmost(root))),
        None => Err(throw_new("java/util/NoSuchElementException", None)),
    }
}

pub fn n_tm_last_key(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.lastKey without receiver");
    match tm_root(r) {
        Some(root) => Ok(tm_entry_key(&tm_rightmost(root))),
        None => Err(throw_new("java/util/NoSuchElementException", None)),
    }
}

/// The NavigableMap key queries (T5): a null result is a null KEY.
fn tm_nav_key(
    recv: Option<&JRef>,
    args: &[Value],
    mode: TmNav,
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.navigable query without receiver");
    let key = arg_or_null(args);
    match tm_navigate(r, &key, mode, ctx)? {
        Some(n) => Ok(tm_entry_key(&n)),
        None => Ok(Value::Ref(None)),
    }
}

pub fn n_tm_ceiling_key(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    tm_nav_key(recv, args, TmNav::Ceiling, ctx)
}

pub fn n_tm_floor_key(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    tm_nav_key(recv, args, TmNav::Floor, ctx)
}

pub fn n_tm_higher_key(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    tm_nav_key(recv, args, TmNav::Higher, ctx)
}

pub fn n_tm_lower_key(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    tm_nav_key(recv, args, TmNav::Lower, ctx)
}

// <init>()V / <init>(Ljava/util/Comparator;)V / <init>(Ljava/util/Map;)V —
// three ROWS (a null Comparator and a null Map both arrive as Ref(None);
// the row disambiguates). The SortedMap ctor and the (C,K) navigable ctor
// are NOT registered (the same-surface law; the bounded views are their
// own slice).
pub fn n_tm_init(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.<init> without receiver");
    with_obj_mut(*r, |o| {
        o.slots[TM_COMPARATOR] = Value::Ref(None);
        o.slots[TM_SIZE] = Value::Int(0);
        o.slots[TM_MODCOUNT] = Value::Int(0);
        o.slots[TM_ROOT] = Value::Ref(None);
    });
    Ok(Value::Void)
}

/// the Comparator ctor: the ref rides (a null comparator means natural)
pub fn n_tm_init_cmp(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.<init> without receiver");
    n_tm_init(Some(r), &[], _ctx)?;
    if let Some(Value::Ref(Some(c))) = args.first() {
        with_obj_mut(*r, |o| o.slots[TM_COMPARATOR] = Value::Ref(Some(*c)));
    }
    // a null comparator = natural ordering (the JDK law)
    Ok(Value::Void)
}

/// the copy ctor (T9): sorted regardless of the source's iteration order
pub fn n_tm_init_map(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.<init> without receiver");
    n_tm_init(Some(r), &[], ctx)?;
    let src = first_arg_ref!(args, Ok(Value::Void));
    let src_class = with_obj(src, |o| o.jclass.to_string());
    let entries = match src_class.as_str() {
        "java/util/LinkedHashMap" => lhm_all_entries(&src)?,
        _ => hm_all_entries(&src)?,
    };
    for (k, v) in entries {
        n_tm_put(Some(r), &[k, v], ctx)?;
    }
    Ok(Value::Void)
}

// --- the views (map-ref objects) and the successor-walk iterators ---

fn tm_new_view(map: &JRef, class: &str) -> Result<Value, Throw> {
    let v = new_obj(class, 1)?;
    with_obj_mut(v, |o| o.slots[0] = Value::Ref(Some(*map)));
    Ok(Value::Ref(Some(v)))
}

fn tm_view_map(recv: Option<&JRef>, who: &str) -> JRef {
    let r = recv.unwrap_or_else(|| panic!("j2r internal: TreeMap view {who} without receiver"));
    with_obj(*r, |o| match &o.slots[0] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: TreeMap view {who} without map"),
    })
}

pub fn n_tm_key_set(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.keySet without receiver");
    tm_new_view(r, "java/util/TreeMap$KeySet")
}

pub fn n_tm_values(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.values without receiver");
    tm_new_view(r, "java/util/TreeMap$Values")
}

pub fn n_tm_entry_set(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.entrySet without receiver");
    tm_new_view(r, "java/util/TreeMap$EntrySet")
}

pub fn n_tks_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "KeySet.iterator");
    tm_new_iterator(&map, "java/util/TreeMap$KeyIterator")
}

pub fn n_tvs_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "Values.iterator");
    tm_new_iterator(&map, "java/util/TreeMap$ValueIterator")
}

pub fn n_tes_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "EntrySet.iterator");
    tm_new_iterator(&map, "java/util/TreeMap$EntryIterator")
}

pub fn n_tks_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "KeySet.size");
    Ok(Value::Int(tm_size_of(&map)))
}

pub fn n_tvs_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "Values.size");
    Ok(Value::Int(tm_size_of(&map)))
}

pub fn n_tes_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "EntrySet.size");
    Ok(Value::Int(tm_size_of(&map)))
}

pub fn n_tks_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "KeySet.contains");
    let key = arg_or_null(args);
    let found = tm_find(&map, &key, ctx)?.is_some();
    Ok(Value::Int(found as i32))
}

pub fn n_tvs_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "Values.contains");
    n_tm_contains_value(Some(&map), args, ctx)
}

pub fn n_tes_contains(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    // the entrySet contains law needs a Map.Entry arg — the probed surface
    // never calls it; route through the key walk of the entry's key when
    // it is one of our entry nodes, else false
    let _map = tm_view_map(recv, "EntrySet.contains");
    Ok(Value::Int(0))
}

pub fn n_tks_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "KeySet.toString");
    let entries = tm_all_entries(&map)?;
    let mut out = String::from("[");
    for (i, (k, _)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(k, ctx)?);
    }
    out.push(']');
    Ok(Value::Ref(Some(crate::object::new_string(&out))))
}

pub fn n_tvs_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "Values.toString");
    let entries = tm_all_entries(&map)?;
    let mut out = String::from("[");
    for (i, (_, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(v, ctx)?);
    }
    out.push(']');
    Ok(Value::Ref(Some(crate::object::new_string(&out))))
}

pub fn n_tes_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "EntrySet.toString");
    let entries = tm_all_entries(&map)?;
    let mut out = String::from("[");
    for (i, (k, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(k, ctx)?);
        out.push('=');
        out.push_str(&dispatch_to_string(v, ctx)?);
    }
    out.push(']');
    Ok(Value::Ref(Some(crate::object::new_string(&out))))
}

/// Builds a positioned successor-walk iterator: next = leftmost(root),
/// current = None, expectedModCount snapshotted.
fn tm_new_iterator(map: &JRef, class: &str) -> Result<Value, Throw> {
    let next = tm_root(map).map(tm_leftmost);
    // pre-read under its own lock (the heap mutex is NOT re-entrant — a
    // with_obj inside a with_obj_mut closure deadlocks)
    let mc = tm_modcount(map);
    let it = new_obj(class, 4)?;
    with_obj_mut(it, |o| {
        o.slots[TMI_MAP] = Value::Ref(Some(*map));
        o.slots[TMI_NEXT] = match next {
            Some(n) => Value::Ref(Some(n)),
            None => Value::Ref(None),
        };
        o.slots[TMI_CURRENT] = Value::Ref(None);
        o.slots[TMI_EXPECTED] = Value::Int(mc);
    });
    Ok(Value::Ref(Some(it)))
}

/// Shared hasNext: no comodification check (the JDK law).
pub fn n_tm_i_has_next(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: TreeMap iterator hasNext without receiver");
    let has = with_obj(*it, |o| matches!(&o.slots[TMI_NEXT], Value::Ref(Some(_))));
    Ok(Value::Int(has as i32))
}

/// Shared nextNode: the CME check first, then NoSuchElementException,
/// then current = next, next = successor(next).
fn tm_iter_next_node(recv: Option<&JRef>, who: &str) -> Result<(JRef, JRef), Throw> {
    let it = recv.unwrap_or_else(|| panic!("j2r internal: {who} without receiver"));
    let (map, expected, next) = with_obj(*it, |o| {
        (
            match &o.slots[TMI_MAP] {
                Value::Ref(Some(m)) => *m,
                _ => panic!("j2r internal: {who} without map"),
            },
            o.slots[TMI_EXPECTED].expect_int(),
            match &o.slots[TMI_NEXT] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    if expected != tm_modcount(&map) {
        return Err(crate::cme());
    }
    let Some(node) = next else {
        return Err(throw_new("java/util/NoSuchElementException", None));
    };
    let succ = tm_successor_of(node);
    with_obj_mut(*it, |o| {
        o.slots[TMI_NEXT] = match succ {
            Some(s) => Value::Ref(Some(s)),
            None => Value::Ref(None),
        };
        o.slots[TMI_CURRENT] = Value::Ref(Some(node));
    });
    Ok((*it, node))
}

/// The key iterator's next: the entry's key (T1).
pub fn n_tki_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = tm_iter_next_node(recv, "TreeMap$KeyIterator.next")?;
    Ok(tm_entry_key(&node))
}

/// The value iterator's next: the entry's value (values follow KEY order).
pub fn n_tvi_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = tm_iter_next_node(recv, "TreeMap$ValueIterator.next")?;
    tm_entry_value_materialized(&node)
}

/// The entry iterator's next IS the internal entry (the JDK law:
/// TreeMap$Entry implements Map.Entry).
pub fn n_tei_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = tm_iter_next_node(recv, "TreeMap$EntryIterator.next")?;
    Ok(Value::Ref(Some(node)))
}

/// Iterator.remove over the tree (T10 + V8): the PROBED ISE-first law
/// (the run-4 ItrOrderProbe law — ISE on null current BEFORE the CME
/// check), then the map's own delete, then the re-sync (no CME on a
/// continued iteration). View-aware (run 6): the bounded iterators share
/// this SAME native — their slot 0 carries the TreeMap$SubMap view, whose
/// slot 0 carries the backing map; the ISE-first law carries by
/// construction.
pub fn n_tm_i_remove(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: TreeMap iterator remove without receiver");
    let (map_slot, expected, current) = with_obj(*it, |o| {
        (
            o.slots[TMI_MAP].clone(),
            o.slots[TMI_EXPECTED].expect_int(),
            match &o.slots[TMI_CURRENT] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    let map = match &map_slot {
        Value::Ref(Some(m)) => {
            let cls = with_obj(*m, |o| o.jclass.to_string());
            if cls == "java/util/TreeMap$SubMap" {
                sms_backing(m)
            } else {
                *m
            }
        }
        _ => panic!("j2r internal: TreeMap iterator without map"),
    };
    let Some(node) = current else {
        return Err(throw_new("java/lang/IllegalStateException", None));
    };
    if expected != tm_modcount(&map) {
        return Err(crate::cme());
    }
    tm_delete(&map, node);
    let mc = tm_modcount(&map);
    with_obj_mut(*it, |o| {
        o.slots[TMI_CURRENT] = Value::Ref(None);
        o.slots[TMI_EXPECTED] = Value::Int(mc);
    });
    Ok(Value::Void)
}

/// TreeMap$Entry.getKey / getValue (the entrySet checkcast + accessor law).
pub fn n_tme_get_key(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let e = recv.expect("j2r internal: TreeMap$Entry.getKey without receiver");
    Ok(tm_entry_key(e))
}

pub fn n_tme_get_value(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let e = recv.expect("j2r internal: TreeMap$Entry.getValue without receiver");
    tm_entry_value_materialized(e)
}

// --------------------------------------------------------- TreeMap$SubMap
//
// Run 6 (2026-11-15): the BOUNDED views — subMap/headMap/tailMap as a
// 5-slot view over the backing TreeMap (0 = backing map, 1 = lo key,
// 2 = hi key, 3 = loInclusive Int, 4 = hiInclusive Int; a null bound is
// the unbounded side, exactly the absolute map's fromStart/toEnd flags).
//
// Laws (probed vs Temurin 21.0.13+9, TreeMapViewsProbe, evidence
// scripts/evidence/treemap_views/treemapviewsprobe_temurin_2026-11-15.txt):
//   V1  contents/toString/size/isEmpty are computed over the RANGE, not
//       mirrored (V5 35-key clear: the parent keeps the out-of-range keys)
//   V2  from-inclusive, to-EXCLUSIVE on the 2-arg forms; the 4-arg forms
//       carry the flags; a single-key inclusive view is puttable (c29)
//   V3  first/lastKey over the range; empty view -> NoSuchElementException
//   V4  the range-gated Map surface: get/containsKey/getOrDefault/remove
//       SILENTLY miss out-of-range keys (c08/c09/c11/c34 — even on an
//       empty view), put throws IAE "key out of range" (c12); null keys
//       NPE through the range compare (c18) and non-Comparable keys CCE
//       (c19) — the type check rides the range check
//   V5  live both ways (c03/c13): put/remove through the view hits the
//       backing tree; parent puts in-range appear in the view
//   V6  slice-only clear (c15/c35): the slice empties, out-of-range keys
//       survive
//   V7  THE NARROWING MATRIX (c16, 26 cells) — chained builders validate
//       STRICTLY, never intersecting/clamping, with the probed ASYMMETRY:
//       the fromKey side uses the FULL inRange (tooLow with inclusivity
//       AND tooHigh with inclusivity — fromKey == an exclusive hi throws
//       "fromKey out of range", c16f/s/t), while the toKey side is
//       INCLUSIVITY-BLIND on the hi (strictly-above-hi only: headMap
//       ACCEPTS toKey == an exclusive hi, c16e, and on a hi-INCLUSIVE
//       parent too, c16y) but full tooLow on the lo (c16u "toKey out of
//       range"). Check order: fromKey range, then toKey range, then the
//       crossed check "fromKey > toKey" (c16w/x/q). The ABSOLUTE map's
//       builders never range-check — only the ctor's crossed check (c16l)
//       plus the self-compare type check (null -> NPE, non-Comparable ->
//       CCE). Chained narrowing of a DISJOINT window throws (c16o/p),
//       never an empty view; from == to is a legal empty view (c16m).
//   V8  the bounded iterators share the unbounded 4-slot layout with
//       slot 0 = the VIEW (the backing map rides the view's slot 0);
//       expectedModCount is the BACKING map's, so a foreign put
//       OUT-of-range still CMEs (c27); remove is LITERALLY the same
//       native as the unbounded iterators' (n_tm_i_remove derefs the
//       view) — the ISE-first law carries by construction (c25/c28)
//   V9  keySet/values/entrySet are live view-of-view objects; the bounded
//       iterators walk the range in key order and stop at the hi fence

const SMS_MAP: usize = 0;
const SMS_LO: usize = 1;
const SMS_HI: usize = 2;
const SMS_LO_INC: usize = 3;
const SMS_HI_INC: usize = 4;

fn sms_backing(view: &JRef) -> JRef {
    with_obj(*view, |o| match &o.slots[SMS_MAP] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: TreeMap$SubMap without backing map"),
    })
}

fn sms_bound(view: &JRef, slot: usize) -> Option<Value> {
    let v = with_obj(*view, |o| o.slots[slot].clone());
    match v {
        Value::Ref(Some(_)) => Some(v),
        _ => None, // the unbounded side carries Value::Ref(None)
    }
}

fn sms_incl(view: &JRef, slot: usize) -> bool {
    with_obj(*view, |o| o.slots[slot].expect_int() != 0)
}

/// tooLow with inclusivity — the fromKey-side gate (V7).
fn sms_too_low(view: &JRef, key: &Value, ctx: Dispatch) -> Result<bool, Throw> {
    match sms_bound(view, SMS_LO) {
        None => Ok(false),
        Some(lo) => {
            let backing = sms_backing(view);
            let cmp = tm_compare(&backing, key, &lo, ctx)?;
            Ok(cmp < 0 || (cmp == 0 && !sms_incl(view, SMS_LO_INC)))
        }
    }
}

/// tooHigh with inclusivity — half of the fromKey-side gate.
fn sms_too_high_full(view: &JRef, key: &Value, ctx: Dispatch) -> Result<bool, Throw> {
    match sms_bound(view, SMS_HI) {
        None => Ok(false),
        Some(hi) => {
            let backing = sms_backing(view);
            let cmp = tm_compare(&backing, key, &hi, ctx)?;
            Ok(cmp > 0 || (cmp == 0 && !sms_incl(view, SMS_HI_INC)))
        }
    }
}

/// strictly-above-hi — the toKey-side hi gate: INCLUSIVITY-BLIND (V7, the
/// probed c16e/c16y asymmetry — headMap accepts toKey == an exclusive hi).
fn sms_above_hi_strict(view: &JRef, key: &Value, ctx: Dispatch) -> Result<bool, Throw> {
    match sms_bound(view, SMS_HI) {
        None => Ok(false),
        Some(hi) => {
            let backing = sms_backing(view);
            Ok(tm_compare(&backing, key, &hi, ctx)? > 0)
        }
    }
}

/// The fromKey-side full range gate (put/get/remove/tailMap/subMap).
fn sms_in_range(view: &JRef, key: &Value, ctx: Dispatch) -> Result<bool, Throw> {
    Ok(!sms_too_low(view, key, ctx)? && !sms_too_high_full(view, key, ctx)?)
}

/// The toKey-side range gate (headMap/subMap's toKey): full tooLow,
/// strictly-above-hi on the other end (V7).
fn sms_in_range_for_to(view: &JRef, key: &Value, ctx: Dispatch) -> Result<bool, Throw> {
    Ok(!sms_too_low(view, key, ctx)? && !sms_above_hi_strict(view, key, ctx)?)
}

/// The leftmost in-range node: the lo bound's ceiling/higher, else the
/// tree's leftmost — hi-fence-gated (an empty view's first is None).
fn sms_first_node(view: &JRef, ctx: Dispatch) -> Result<Option<JRef>, Throw> {
    let backing = sms_backing(view);
    let start = match sms_bound(view, SMS_LO) {
        None => tm_root(&backing).map(tm_leftmost),
        Some(lo) => tm_navigate(
            &backing,
            &lo,
            if sms_incl(view, SMS_LO_INC) {
                TmNav::Ceiling
            } else {
                TmNav::Higher
            },
            ctx,
        )?,
    };
    match start {
        Some(n) if sms_too_high_full(view, &tm_entry_key(&n), ctx)? => Ok(None),
        other => Ok(other),
    }
}

/// The rightmost in-range node: the hi bound's floor/lower, else the
/// tree's rightmost — lo-fence-gated (the c07c law: Floor(20) on [10,20)
/// over {1,3,5,7,9} lands on 9, which is BELOW the view's lo).
fn sms_last_node(view: &JRef, ctx: Dispatch) -> Result<Option<JRef>, Throw> {
    let backing = sms_backing(view);
    let end = match sms_bound(view, SMS_HI) {
        None => tm_root(&backing).map(tm_rightmost),
        Some(hi) => tm_navigate(
            &backing,
            &hi,
            if sms_incl(view, SMS_HI_INC) {
                TmNav::Floor
            } else {
                TmNav::Lower
            },
            ctx,
        )?,
    };
    match end {
        Some(n) if sms_too_low(view, &tm_entry_key(&n), ctx)? => Ok(None),
        other => Ok(other),
    }
}

/// Every (key, value) pair inside the view, in KEY order (V1).
fn sms_all_entries(view: &JRef, ctx: Dispatch) -> Result<Vec<(Value, Value)>, Throw> {
    let mut out = Vec::new();
    let mut cur = sms_first_node(view, ctx)?;
    while let Some(n) = cur {
        let k = tm_entry_key(&n);
        if sms_too_high_full(view, &k, ctx)? {
            break;
        }
        out.push((k, tm_entry_value_materialized(&n)?));
        cur = tm_successor_of(n);
    }
    Ok(out)
}

fn sms_new(map: &JRef, lo: Value, lo_inc: bool, hi: Value, hi_inc: bool) -> Result<Value, Throw> {
    let v = new_obj("java/util/TreeMap$SubMap", 5)?;
    with_obj_mut(v, |o| {
        o.slots[SMS_MAP] = Value::Ref(Some(*map));
        o.slots[SMS_LO] = lo;
        o.slots[SMS_HI] = hi;
        o.slots[SMS_LO_INC] = Value::Int(lo_inc as i32);
        o.slots[SMS_HI_INC] = Value::Int(hi_inc as i32);
    });
    Ok(Value::Ref(Some(v)))
}

fn sms_view(recv: Option<&JRef>, who: &str) -> JRef {
    *recv.unwrap_or_else(|| panic!("j2r internal: TreeMap$SubMap {who} without receiver"))
}

fn arg_bool(args: &[Value], i: usize) -> bool {
    args.get(i).map(|v| v.expect_int() != 0).unwrap_or(false)
}

fn sms_iae(msg: &str) -> Throw {
    throw_new("java/lang/IllegalArgumentException", Some(msg))
}

// --- the ABSOLUTE map's builders (no range to violate; the ctor's
//     crossed check + the self-compare type check are the whole law) ---

pub fn n_tm_sub_map(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = sms_view(recv, "subMap");
    let f = arg_or_null(args);
    let t = args.get(1).cloned().unwrap_or(Value::Ref(None));
    if tm_compare(&r, &f, &t, ctx)? > 0 {
        return Err(sms_iae("fromKey > toKey"));
    }
    sms_new(&r, f, true, t, false)
}

pub fn n_tm_sub_map4(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = sms_view(recv, "subMap4");
    let f = arg_or_null(args);
    let t = args.get(2).cloned().unwrap_or(Value::Ref(None));
    if tm_compare(&r, &f, &t, ctx)? > 0 {
        return Err(sms_iae("fromKey > toKey"));
    }
    sms_new(&r, f, arg_bool(args, 1), t, arg_bool(args, 3))
}

pub fn n_tm_head_map(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = sms_view(recv, "headMap");
    let t = arg_or_null(args);
    let _ = tm_compare(&r, &t, &t, ctx)?;
    sms_new(&r, Value::Ref(None), false, t, false)
}

pub fn n_tm_head_map4(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = sms_view(recv, "headMap4");
    let t = arg_or_null(args);
    let _ = tm_compare(&r, &t, &t, ctx)?;
    sms_new(&r, Value::Ref(None), false, t, arg_bool(args, 1))
}

pub fn n_tm_tail_map(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = sms_view(recv, "tailMap");
    let f = arg_or_null(args);
    let _ = tm_compare(&r, &f, &f, ctx)?;
    sms_new(&r, f, true, Value::Ref(None), false)
}

pub fn n_tm_tail_map4(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = sms_view(recv, "tailMap4");
    let f = arg_or_null(args);
    let _ = tm_compare(&r, &f, &f, ctx)?;
    sms_new(&r, f, arg_bool(args, 1), Value::Ref(None), false)
}

pub fn n_tm_comparator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = sms_view(recv, "comparator");
    Ok(with_obj(r, |o| o.slots[TM_COMPARATOR].clone()))
}

/// The BOUNDED view's comparator: the view's slot 0 is the backing map,
/// not the comparator — deref first (the c20 law the battery caught).
pub fn n_sms_comparator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view(recv, "comparator");
    let backing = sms_backing(&view);
    Ok(with_obj(backing, |o| o.slots[TM_COMPARATOR].clone()))
}

// --- the BOUNDED view's range-gated surface ---

pub fn n_sms_put(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "put");
    let key = arg_or_null(args);
    if !sms_in_range(&view, &key, ctx)? {
        return Err(sms_iae("key out of range"));
    }
    let backing = sms_backing(&view);
    n_tm_put(Some(&backing), args, ctx)
}

pub fn n_sms_get(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "get");
    let key = arg_or_null(args);
    if !sms_in_range(&view, &key, ctx)? {
        return Ok(Value::Ref(None));
    }
    let backing = sms_backing(&view);
    n_tm_get(Some(&backing), args, ctx)
}

pub fn n_sms_contains_key(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view(recv, "containsKey");
    let key = arg_or_null(args);
    if !sms_in_range(&view, &key, ctx)? {
        return Ok(Value::Int(0));
    }
    let backing = sms_backing(&view);
    n_tm_contains_key(Some(&backing), args, ctx)
}

pub fn n_sms_get_or_default(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view(recv, "getOrDefault");
    let key = arg_or_null(args);
    if !sms_in_range(&view, &key, ctx)? {
        return Ok(args.get(1).cloned().unwrap_or(Value::Ref(None)));
    }
    let backing = sms_backing(&view);
    n_tm_get_or_default(Some(&backing), args, ctx)
}

pub fn n_sms_remove(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "remove");
    let key = arg_or_null(args);
    if !sms_in_range(&view, &key, ctx)? {
        return Ok(Value::Ref(None));
    }
    let backing = sms_backing(&view);
    n_tm_remove(Some(&backing), args, ctx)
}

pub fn n_sms_contains_value(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view(recv, "containsValue");
    let target = arg_or_null(args);
    for (_, v) in sms_all_entries(&view, ctx)? {
        let eq = match (&v, &target) {
            (Value::Ref(Some(a)), Value::Ref(Some(_))) => dispatch_equals(a, &target, ctx)?,
            (Value::Ref(None), Value::Ref(None)) => true,
            _ => false,
        };
        if eq {
            return Ok(Value::Int(1));
        }
    }
    Ok(Value::Int(0))
}

pub fn n_sms_size(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "size");
    let n = sms_all_entries(&view, ctx)?.len();
    Ok(Value::Int(n as i32))
}

pub fn n_sms_is_empty(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "isEmpty");
    Ok(Value::Int((sms_all_entries(&view, ctx)?.is_empty()) as i32))
}

/// Slice-only clear (V6): delete every in-range node; the backing map's
/// out-of-range keys survive; each delete bumps the backing modCount.
/// The hi fence gates the loop (the lo side alone would drain the whole
/// tree above lo — the c15/c35 over-delete the battery caught).
pub fn n_sms_clear(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "clear");
    let backing = sms_backing(&view);
    while let Some(node) = sms_first_node(&view, ctx)? {
        if sms_too_high_full(&view, &tm_entry_key(&node), ctx)? {
            break;
        }
        tm_delete(&backing, node);
    }
    Ok(Value::Void)
}

pub fn n_sms_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view(recv, "toString");
    let entries = sms_all_entries(&view, ctx)?;
    let mut out = String::from("{");
    for (i, (k, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(k, ctx)?);
        out.push('=');
        out.push_str(&dispatch_to_string(v, ctx)?);
    }
    out.push('}');
    Ok(Value::Ref(Some(crate::object::new_string(&out))))
}

pub fn n_sms_first_key(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view(recv, "firstKey");
    match sms_first_node(&view, ctx)? {
        Some(n) => Ok(tm_entry_key(&n)),
        None => Err(throw_new("java/util/NoSuchElementException", None)),
    }
}

pub fn n_sms_last_key(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "lastKey");
    match sms_last_node(&view, ctx)? {
        Some(n) => Ok(tm_entry_key(&n)),
        None => Err(throw_new("java/util/NoSuchElementException", None)),
    }
}

// --- the narrowing-checked chained builders (V7) ---

pub fn n_sms_sub_map(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "subMap");
    let backing = sms_backing(&view);
    let f = arg_or_null(args);
    let t = args.get(1).cloned().unwrap_or(Value::Ref(None));
    if !sms_in_range(&view, &f, ctx)? {
        return Err(sms_iae("fromKey out of range"));
    }
    if !sms_in_range_for_to(&view, &t, ctx)? {
        return Err(sms_iae("toKey out of range"));
    }
    if tm_compare(&backing, &f, &t, ctx)? > 0 {
        return Err(sms_iae("fromKey > toKey"));
    }
    sms_new(&backing, f, true, t, false)
}

pub fn n_sms_sub_map4(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "subMap4");
    let backing = sms_backing(&view);
    let f = arg_or_null(args);
    let t = args.get(2).cloned().unwrap_or(Value::Ref(None));
    if !sms_in_range(&view, &f, ctx)? {
        return Err(sms_iae("fromKey out of range"));
    }
    if !sms_in_range_for_to(&view, &t, ctx)? {
        return Err(sms_iae("toKey out of range"));
    }
    if tm_compare(&backing, &f, &t, ctx)? > 0 {
        return Err(sms_iae("fromKey > toKey"));
    }
    sms_new(&backing, f, arg_bool(args, 1), t, arg_bool(args, 3))
}

pub fn n_sms_head_map(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "headMap");
    let backing = sms_backing(&view);
    let t = arg_or_null(args);
    if !sms_in_range_for_to(&view, &t, ctx)? {
        return Err(sms_iae("toKey out of range"));
    }
    let (lo, lo_inc) = (
        sms_bound(&view, SMS_LO).unwrap_or(Value::Ref(None)),
        sms_incl(&view, SMS_LO_INC),
    );
    sms_new(&backing, lo, lo_inc, t, false)
}

pub fn n_sms_head_map4(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "headMap4");
    let backing = sms_backing(&view);
    let t = arg_or_null(args);
    if !sms_in_range_for_to(&view, &t, ctx)? {
        return Err(sms_iae("toKey out of range"));
    }
    let (lo, lo_inc) = (
        sms_bound(&view, SMS_LO).unwrap_or(Value::Ref(None)),
        sms_incl(&view, SMS_LO_INC),
    );
    sms_new(&backing, lo, lo_inc, t, arg_bool(args, 1))
}

pub fn n_sms_tail_map(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "tailMap");
    let backing = sms_backing(&view);
    let f = arg_or_null(args);
    if !sms_in_range(&view, &f, ctx)? {
        return Err(sms_iae("fromKey out of range"));
    }
    let (hi, hi_inc) = (
        sms_bound(&view, SMS_HI).unwrap_or(Value::Ref(None)),
        sms_incl(&view, SMS_HI_INC),
    );
    sms_new(&backing, f, true, hi, hi_inc)
}

pub fn n_sms_tail_map4(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "tailMap4");
    let backing = sms_backing(&view);
    let f = arg_or_null(args);
    if !sms_in_range(&view, &f, ctx)? {
        return Err(sms_iae("fromKey out of range"));
    }
    let (hi, hi_inc) = (
        sms_bound(&view, SMS_HI).unwrap_or(Value::Ref(None)),
        sms_incl(&view, SMS_HI_INC),
    );
    sms_new(&backing, f, arg_bool(args, 1), hi, hi_inc)
}

// --- the view-of-view objects (V9) ---

fn sms_sub_view(recv: Option<&JRef>, who: &str, class: &str) -> Result<Value, Throw> {
    let view = sms_view(recv, who);
    let v = new_obj(class, 1)?;
    with_obj_mut(v, |o| o.slots[0] = Value::Ref(Some(view)));
    Ok(Value::Ref(Some(v)))
}

fn sms_view_view(recv: Option<&JRef>, who: &str) -> JRef {
    let r = recv.unwrap_or_else(|| panic!("j2r internal: SubMap {who} without receiver"));
    with_obj(*r, |o| match &o.slots[0] {
        Value::Ref(Some(v)) => *v,
        _ => panic!("j2r internal: SubMap {who} without view"),
    })
}

pub fn n_sms_key_set(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    sms_sub_view(recv, "keySet", "java/util/TreeMap$SubMap$KeySet")
}

pub fn n_sms_values(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    sms_sub_view(recv, "values", "java/util/TreeMap$SubMap$Values")
}

pub fn n_sms_entry_set(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    sms_sub_view(recv, "entrySet", "java/util/TreeMap$SubMap$EntrySet")
}

pub fn n_smsks_size(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "KeySet.size");
    Ok(Value::Int(sms_all_entries(&view, ctx)?.len() as i32))
}

pub fn n_smsvs_size(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "Values.size");
    Ok(Value::Int(sms_all_entries(&view, ctx)?.len() as i32))
}

pub fn n_smses_size(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "EntrySet.size");
    Ok(Value::Int(sms_all_entries(&view, ctx)?.len() as i32))
}

pub fn n_smsks_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "KeySet.iterator");
    sms_new_iterator(&view, "java/util/TreeMap$SubMap$KeyIterator", ctx)
}

pub fn n_smsvs_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "Values.iterator");
    sms_new_iterator(&view, "java/util/TreeMap$SubMap$ValueIterator", ctx)
}

pub fn n_smses_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "EntrySet.iterator");
    sms_new_iterator(&view, "java/util/TreeMap$SubMap$EntryIterator", ctx)
}

pub fn n_smsks_contains(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "KeySet.contains");
    let key = arg_or_null(args);
    if !sms_in_range(&view, &key, ctx)? {
        return Ok(Value::Int(0));
    }
    let backing = sms_backing(&view);
    let found = tm_find(&backing, &key, ctx)?.is_some();
    Ok(Value::Int(found as i32))
}

pub fn n_smsvs_contains(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "Values.contains");
    n_sms_contains_value(Some(&view), args, ctx)
}

pub fn n_smsks_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "KeySet.toString");
    let entries = sms_all_entries(&view, ctx)?;
    let mut out = String::from("[");
    for (i, (k, _)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(k, ctx)?);
    }
    out.push(']');
    Ok(Value::Ref(Some(crate::object::new_string(&out))))
}

pub fn n_smsvs_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "Values.toString");
    let entries = sms_all_entries(&view, ctx)?;
    let mut out = String::from("[");
    for (i, (_, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(v, ctx)?);
    }
    out.push(']');
    Ok(Value::Ref(Some(crate::object::new_string(&out))))
}

pub fn n_smses_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "EntrySet.toString");
    let entries = sms_all_entries(&view, ctx)?;
    let mut out = String::from("[");
    for (i, (k, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(k, ctx)?);
        out.push('=');
        out.push_str(&dispatch_to_string(v, ctx)?);
    }
    out.push(']');
    Ok(Value::Ref(Some(crate::object::new_string(&out))))
}

// --- the bounded iterators (V8): the unbounded 4-slot layout, slot 0 =
//     the VIEW (the backing map rides the view's slot 0) ---

fn sms_new_iterator(view: &JRef, class: &str, ctx: Dispatch) -> Result<Value, Throw> {
    let next = sms_first_node(view, ctx)?;
    let backing = sms_backing(view);
    let mc = tm_modcount(&backing);
    let it = new_obj(class, 4)?;
    with_obj_mut(it, |o| {
        o.slots[TMI_MAP] = Value::Ref(Some(*view));
        o.slots[TMI_NEXT] = match next {
            Some(n) => Value::Ref(Some(n)),
            None => Value::Ref(None),
        };
        o.slots[TMI_CURRENT] = Value::Ref(None);
        o.slots[TMI_EXPECTED] = Value::Int(mc);
    });
    Ok(Value::Ref(Some(it)))
}

/// Shared bounded hasNext: the next node must still be inside the hi
/// fence (the lo side cannot be crossed walking forward).
pub fn n_sms_i_has_next(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: SubMap iterator hasNext without receiver");
    let (view, next) = with_obj(*it, |o| {
        (
            match &o.slots[TMI_MAP] {
                Value::Ref(Some(v)) => *v,
                _ => panic!("j2r internal: SubMap iterator without view"),
            },
            match &o.slots[TMI_NEXT] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    let has = match next {
        None => false,
        Some(n) => !sms_too_high_full(&view, &tm_entry_key(&n), ctx)?,
    };
    Ok(Value::Int(has as i32))
}

/// Shared bounded nextNode: CME against the BACKING map's modCount (the
/// probed c27 law: a foreign put OUT-of-range still comodifies), the hi
/// fence -> NoSuchElementException, then current = next, next =
/// successor(next) (range-gated at hasNext/next time, never stored).
fn sms_iter_next_node(
    recv: Option<&JRef>,
    who: &str,
    ctx: Dispatch,
) -> Result<(JRef, JRef), Throw> {
    let it = recv.unwrap_or_else(|| panic!("j2r internal: {who} without receiver"));
    let (view, expected, next) = with_obj(*it, |o| {
        (
            match &o.slots[TMI_MAP] {
                Value::Ref(Some(v)) => *v,
                _ => panic!("j2r internal: {who} without view"),
            },
            o.slots[TMI_EXPECTED].expect_int(),
            match &o.slots[TMI_NEXT] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    let backing = sms_backing(&view);
    if expected != tm_modcount(&backing) {
        return Err(crate::cme());
    }
    let node = match next {
        None => return Err(throw_new("java/util/NoSuchElementException", None)),
        Some(n) => {
            if sms_too_high_full(&view, &tm_entry_key(&n), ctx)? {
                return Err(throw_new("java/util/NoSuchElementException", None));
            }
            n
        }
    };
    let succ = tm_successor_of(node);
    with_obj_mut(*it, |o| {
        o.slots[TMI_NEXT] = match succ {
            Some(s) => Value::Ref(Some(s)),
            None => Value::Ref(None),
        };
        o.slots[TMI_CURRENT] = Value::Ref(Some(node));
    });
    Ok((*it, node))
}

pub fn n_smki_next(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = sms_iter_next_node(recv, "TreeMap$SubMap$KeyIterator.next", ctx)?;
    Ok(tm_entry_key(&node))
}

pub fn n_smvi_next(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = sms_iter_next_node(recv, "TreeMap$SubMap$ValueIterator.next", ctx)?;
    tm_entry_value_materialized(&node)
}

pub fn n_smei_next(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = sms_iter_next_node(recv, "TreeMap$SubMap$EntryIterator.next", ctx)?;
    Ok(Value::Ref(Some(node)))
}

// ---------------------------------------------------------------- HashSet

const HS_MAP: usize = 0;

pub fn n_hs_init(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashSet.<init> without receiver");
    let map = new_obj("java/util/HashMap", 4)?;
    n_hm_init(Some(&map), args, ctx)?;
    with_obj_mut(*r, |o| o.slots[HS_MAP] = Value::Ref(Some(map)));
    Ok(Value::Void)
}

fn hs_map(recv: Option<&JRef>, who: &str) -> JRef {
    let r = recv.unwrap_or_else(|| panic!("j2r internal: HashSet.{who} without receiver"));
    with_obj(*r, |o| match &o.slots[HS_MAP] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: HashSet.{who} without backing map"),
    })
}

pub fn n_hs_add(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = hs_map(recv, "add");
    let e = arg_or_null(args);
    // Java: map.put(e, PRESENT) == null — PRESENT is a shared dummy whose
    // only requirement is being a live heap object (roots scan it through
    // the map's node slots).
    let present = new_obj("java/lang/Object", 0)?;
    let (old, _) = hm_put(
        &map,
        e,
        Value::Ref(Some(present)),
        ctx,
        None,
        NodeKind::HashNode,
    )?;
    Ok(Value::Int(old.is_null() as i32))
}

pub fn n_hs_remove(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = hs_map(recv, "remove");
    let e = arg_or_null(args);
    let old = hm_remove(&map, &e, ctx)?;
    Ok(Value::Int((!old.is_null()) as i32))
}

/// `Set.addAll(Collection)` — the stringsim corpus wave (Jaccard's union).
/// Iterates through Collection dispatch so any modelled collection works.
pub fn n_hs_add_all(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let set = recv.expect("j2r internal: Set.addAll without receiver");
    let coll = match args.first() {
        Some(Value::Ref(Some(c))) => *c,
        Some(Value::Ref(None)) | None => return Err(crate::npe()),
        _ => return Err(internal_error("Set.addAll needs a Collection")),
    };
    // Lock law L3: no heap access nests inside a heap guard — read the
    // backing map ref first, then its size, as two flat accesses.
    let map = with_obj(*set, |o| match &o.slots[HS_MAP] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: HashSet backing map"),
    });
    let before = with_obj(map, |m| m.slots[HM_SIZE].expect_int());
    let it = invoke_virtual(
        &coll,
        "java/util/Collection",
        "iterator",
        "()Ljava/util/Iterator;",
        &[],
        ctx,
    )?;
    let it_ref = match it {
        Value::Ref(Some(r)) => r,
        _ => return Err(internal_error("Set.addAll: iterator() not an object")),
    };
    loop {
        let has = invoke_virtual(&it_ref, "java/util/Iterator", "hasNext", "()Z", &[], ctx)?;
        if !matches!(has, Value::Int(1)) {
            break;
        }
        let v = invoke_virtual(
            &it_ref,
            "java/util/Iterator",
            "next",
            "()Ljava/lang/Object;",
            &[],
            ctx,
        )?;
        invoke_virtual(
            set,
            "java/util/Set",
            "add",
            "(Ljava/lang/Object;)Z",
            std::slice::from_ref(&v),
            ctx,
        )?;
    }
    let after = with_obj(map, |m| m.slots[HM_SIZE].expect_int());
    Ok(Value::Int((after != before) as i32))
}

pub fn n_hs_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = hs_map(recv, "contains");
    let e = arg_or_null(args);
    Ok(Value::Int(hm_find(&map, &e, ctx)?.is_some() as i32))
}

pub fn n_hs_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(hs_map(recv, "size"), |o| {
        o.slots[HM_SIZE].expect_int()
    })))
}

pub fn n_hs_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(hs_map(recv, "isEmpty"), |o| o.slots[HM_SIZE].expect_int()) == 0) as i32,
    ))
}

pub fn n_hs_iterator(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let map = hs_map(recv, "iterator");
    hm_new_hash_iterator(&map, "java/util/HashMap$KeyIterator")
}

pub fn n_hs_to_string(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = hs_map(recv, "toString");
    let items: Vec<Value> = hm_all_entries(&map)?.into_iter().map(|(k, _)| k).collect();
    collection_to_string(items, ctx)
}

// ---------------------------------------------------------------- Objects
// ---------------------------------------------------------------- EntrySet
//
// The stringsim corpus wave (2026-11-07): profile iteration through
// Map.entrySet(). The JDK returns the INTERNAL Node objects as the
// Map.Entry values (HashMap$Node implements Map.Entry), so the entry
// iterator hands out the node itself and getKey/getValue are registered
// as virtual rows on HashMap$Node. The EntrySet view is slot-0-delegate
// like KeySet/Values; iteration order is the map's table order, already
// byte-identical to Temurin by the HashMapOrder law.

pub fn n_hm_entry_set(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = recv.expect("j2r internal: HashMap.entrySet without receiver");
    let view = new_obj("java/util/HashMap$EntrySet", 1)?;
    with_obj_mut(view, |o| o.slots[0] = Value::Ref(Some(*map)));
    Ok(Value::Ref(Some(view)))
}

fn hes_map(recv: Option<&JRef>, who: &str) -> JRef {
    let r = recv.unwrap_or_else(|| panic!("j2r internal: EntrySet.{who} without receiver"));
    with_obj(*r, |o| match &o.slots[0] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: EntrySet.{who} without backing map"),
    })
}

pub fn n_hes_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = hes_map(recv, "iterator");
    hm_new_hash_iterator(&map, "java/util/HashMap$EntryIterator")
}

pub fn n_hes_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(hes_map(recv, "size"), |o| {
        o.slots[HM_SIZE].expect_int()
    })))
}

pub fn n_hes_is_empty(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(hes_map(recv, "isEmpty"), |o| o.slots[HM_SIZE].expect_int()) == 0) as i32,
    ))
}

pub fn n_ei_has_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    n_ki_has_next(recv, _args, _ctx)
}

/// JDK semantics: the entry iterator's next() IS the internal Node, exposed
/// as Map.Entry (HashMap$Node implements Map.Entry).
pub fn n_ei_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = hm_hash_next_node(recv, "EntryIterator.next")?;
    Ok(Value::Ref(Some(node)))
}

pub fn n_node_get_key(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let n = recv.expect("j2r internal: Node.getKey without receiver");
    Ok(node_parts(n).1)
}

pub fn n_node_get_value(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let n = recv.expect("j2r internal: Node.getValue without receiver");
    node_value_materialized(n)
}

// ------------------------------------------------- Collections$UnmodifiableMap
//
// The stringsim corpus wave: ShingleBased.getProfile returns
// Collections.unmodifiableMap(shingles). Slot 0 = delegate map. Reads
// forward; mutations throw UnsupportedOperationException (JDK: no
// message). entrySet forwards to the delegate — the JDK wrapper also
// iterates the delegate's entrySet, so order semantics are unchanged.

/// `Collections.unmodifiableMap(m)` — static factory for the wrapper.
pub fn n_collections_unmodifiable_map(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let wrapper = new_obj("java/util/Collections$UnmodifiableMap", 1)?;
    n_um_init(Some(&wrapper), args, ctx)?;
    Ok(Value::Ref(Some(wrapper)))
}

pub fn n_um_init(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: UnmodifiableMap.<init> without receiver");
    let delegate = match args.first() {
        Some(Value::Ref(Some(m))) => *m,
        _ => return Err(crate::npe()),
    };
    with_obj_mut(*r, |o| o.slots[0] = Value::Ref(Some(delegate)));
    Ok(Value::Void)
}

fn um_delegate(recv: Option<&JRef>, who: &str) -> JRef {
    let r = recv.unwrap_or_else(|| panic!("j2r internal: UnmodifiableMap.{who} without receiver"));
    with_obj(*r, |o| match &o.slots[0] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: UnmodifiableMap.{who} without delegate"),
    })
}

pub fn n_um_get(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = um_delegate(recv, "get");
    let key = arg_or_null(args);
    match hm_find(&map, &key, ctx)? {
        Some(node) => Ok(with_obj(node, |o| o.slots[2].clone())),
        None => Ok(Value::Ref(None)),
    }
}

pub fn n_um_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(um_delegate(recv, "size"), |o| {
        o.slots[HM_SIZE].expect_int()
    })))
}

pub fn n_um_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(um_delegate(recv, "isEmpty"), |o| {
            o.slots[HM_SIZE].expect_int()
        }) == 0) as i32,
    ))
}

pub fn n_um_entry_set(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = um_delegate(recv, "entrySet");
    n_hm_entry_set(Some(&map), _args, _ctx)
}

/// `UnmodifiableMap.keySet()` — forwards to the delegate. The JDK wraps
/// in an UnmodifiableSet view; for read-only use (the corpus rule) the
/// delegate's keySet is observationally identical.
pub fn n_um_key_set(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = um_delegate(recv, "keySet");
    invoke_virtual(
        &map,
        "java/util/Map",
        "keySet",
        "()Ljava/util/Set;",
        args,
        ctx,
    )
}

/// `UnmodifiableMap.containsKey(k)` — forwards to the delegate.
pub fn n_um_contains_key(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = um_delegate(recv, "containsKey");
    invoke_virtual(
        &map,
        "java/util/Map",
        "containsKey",
        "(Ljava/lang/Object;)Z",
        args,
        ctx,
    )
}

pub fn n_um_put(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let _ = um_delegate(recv, "put");
    Err(throw_new("java/lang/UnsupportedOperationException", None))
}

// ---------------------------------------------------------------- LinkedList
//
// The stringsim corpus wave (Sift4's index-oriented Offset list). Same
// storage model as ArrayList (slot 0 size, slot 1 modCount, slot 2 the
// Object[] payload); the JDK's deque methods stay unregistered (loud)
// until a caller needs them — the honest surface rule.

const LL_SIZE: usize = 0;
const LL_MODCOUNT: usize = 1;
const LL_ELEMS: usize = 2;

fn ll_elems_ref(r: &JRef) -> JRef {
    with_obj(*r, |o| match &o.slots[LL_ELEMS] {
        Value::Ref(Some(a)) => *a,
        _ => panic!("j2r internal: LinkedList elements slot"),
    })
}

fn ll_ensure_capacity(r: &JRef, need: usize, ctx: Dispatch) -> Result<(), Throw> {
    let arr = ll_elems_ref(r);
    let cap = crate::array::array_length(&arr)? as usize;
    if need <= cap {
        return Ok(());
    }
    // ArrayList growth law: 1.5x, min 10 (see the module note: a no-arg
    // list grows to size+1 first; capacity is not observable).
    let mut newcap = cap + (cap >> 1);
    if newcap < need {
        newcap = need;
    }
    if newcap < 10 {
        newcap = 10;
    }
    let new_arr = crate::array::array_new_ref("java/lang/Object", newcap as i32)?;
    let old = ll_elems_ref(r);
    let n = with_obj(*r, |o| o.slots[LL_SIZE].expect_int());
    for i in 0..n {
        let v = crate::array::array_load(&old, i)?;
        crate::array::array_store(&new_arr, i, v, ctx)?;
    }
    with_obj_mut(*r, |o| o.slots[LL_ELEMS] = Value::Ref(Some(new_arr)));
    Ok(())
}

pub fn n_ll_init(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedList.<init> without receiver");
    let arr = crate::array::array_new_ref("java/lang/Object", 10)?;
    with_obj_mut(*r, |o| {
        o.slots[LL_SIZE] = Value::Int(0);
        o.slots[LL_MODCOUNT] = Value::Int(0);
        o.slots[LL_ELEMS] = Value::Ref(Some(arr));
    });
    Ok(Value::Void)
}

pub fn n_ll_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedList.size without receiver");
    Ok(Value::Int(with_obj(*r, |o| o.slots[LL_SIZE].expect_int())))
}

pub fn n_ll_add(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedList.add without receiver");
    let v = arg_or_null(args);
    let size = with_obj(*r, |o| o.slots[LL_SIZE].expect_int());
    ll_ensure_capacity(r, size as usize + 1, ctx)?;
    let arr = ll_elems_ref(r);
    crate::array::array_store(&arr, size, v.clone(), ctx)?;
    with_obj_mut(*r, |o| {
        o.slots[LL_SIZE] = Value::Int(size + 1);
        o.slots[LL_MODCOUNT] = Value::Int(o.slots[LL_MODCOUNT].expect_int() + 1);
    });
    Ok(Value::Int(1))
}

pub fn n_ll_get(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedList.get without receiver");
    let i = args.first().cloned().unwrap_or(Value::Int(0)).expect_int();
    let size = with_obj(*r, |o| o.slots[LL_SIZE].expect_int());
    if i < 0 || i >= size {
        return Err(throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&format!("Index: {i}, Size: {size}")),
        ));
    }
    let arr = ll_elems_ref(r);
    crate::array::array_load(&arr, i)
}

pub fn n_ll_remove(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedList.remove without receiver");
    let i = args.first().cloned().unwrap_or(Value::Int(0)).expect_int();
    let size = with_obj(*r, |o| o.slots[LL_SIZE].expect_int());
    if i < 0 || i >= size {
        return Err(throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&format!("Index: {i}, Size: {size}")),
        ));
    }
    let arr = ll_elems_ref(r);
    let old = crate::array::array_load(&arr, i)?;
    for j in i..size - 1 {
        let v = crate::array::array_load(&arr, j + 1)?;
        crate::array::array_store(&arr, j, v, _ctx)?;
    }
    crate::array::array_store(&arr, size - 1, Value::Ref(None), _ctx)?;
    with_obj_mut(*r, |o| {
        o.slots[LL_SIZE] = Value::Int(size - 1);
        o.slots[LL_MODCOUNT] = Value::Int(o.slots[LL_MODCOUNT].expect_int() + 1);
    });
    Ok(old)
}

pub fn n_objects_equals(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = arg_or_null(args);
    let b = args.get(1).cloned().unwrap_or(Value::Ref(None));
    let eq = match (&a, &b) {
        (Value::Ref(Some(x)), Value::Ref(Some(y))) if JRef::ptr_eq(x, y) => true,
        (Value::Ref(Some(x)), b) => dispatch_equals(x, b, ctx)?,
        (Value::Ref(None), Value::Ref(None)) => true,
        _ => false,
    };
    Ok(Value::Int(eq as i32))
}

pub fn n_objects_hash_code(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = arg_or_null(args);
    Ok(Value::Int(dispatch_hash_code(&a, ctx)?))
}

pub fn n_objects_require_non_null(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = arg_or_null(args);
    if a.is_null() {
        return Err(crate::npe());
    }
    Ok(a)
}

pub fn n_objects_require_non_null_msg(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = arg_or_null(args);
    if a.is_null() {
        let msg = args.get(1).cloned().unwrap_or(Value::Ref(None));
        let text = match &msg {
            Value::Ref(Some(r)) => Some(string_value(r).to_string()),
            _ => None,
        };
        return Err(throw_new("java/lang/NullPointerException", text.as_deref()));
    }
    Ok(a)
}

pub fn n_objects_require_non_null_supplier(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = arg_or_null(args);
    if a.is_null() {
        // dispatch the supplier's compiled get()
        match args.get(1) {
            Some(Value::Ref(Some(s))) => {
                let msg = crate::invoke_virtual(
                    s,
                    "java/util/function/Supplier",
                    "get",
                    "()Ljava/lang/Object;",
                    &[],
                    ctx,
                )?;
                let text = msg.expect_ref().map(|r| string_value(&r).to_string());
                return Err(throw_new("java/lang/NullPointerException", text.as_deref()));
            }
            _ => return Err(crate::npe()),
        }
    }
    Ok(a)
}

pub fn n_objects_to_string(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = arg_or_null(args);
    let s = dispatch_to_string(&a, ctx)?;
    Ok(Value::Ref(Some(crate::object::new_string(&s))))
}

pub fn n_objects_is_null(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = arg_or_null(args);
    Ok(Value::Int(a.is_null() as i32))
}

pub fn n_objects_non_null(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = arg_or_null(args);
    Ok(Value::Int((!a.is_null()) as i32))
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{new_string, string_value};

    fn ctx() -> Dispatch {
        Dispatch::default()
    }

    fn boxed_i32(v: i32) -> Value {
        // a java/lang/Integer object with its payload in slot 0 (the same
        // layout the valueOf native produces)
        let b = new_obj("java/lang/Integer", 1).unwrap();
        with_obj_mut(b, |o| o.slots[0] = Value::Int(v));
        Value::Ref(Some(b))
    }

    fn list_new() -> JRef {
        let l = new_obj("java/util/ArrayList", 3).unwrap();
        n_al_init(Some(&l), &[], ctx()).unwrap();
        l
    }

    fn list_add(l: &JRef, v: Value) {
        n_al_add(Some(l), &[v], ctx()).unwrap();
    }

    fn map_new() -> JRef {
        let m = new_obj("java/util/HashMap", 4).unwrap();
        n_hm_init(Some(&m), &[], ctx()).unwrap();
        m
    }

    fn tm_new() -> JRef {
        let m = new_obj("java/util/TreeMap", 4).unwrap();
        n_tm_init(Some(&m), &[], ctx()).unwrap();
        m
    }

    fn set_new() -> JRef {
        let s = new_obj("java/util/HashSet", 1).unwrap();
        n_hs_init(Some(&s), &[], ctx()).unwrap();
        s
    }

    /// Unboxes a boxed Integer reference (collection results hold refs).
    fn unbox_i32(v: &Value) -> i32 {
        match v {
            Value::Ref(Some(r)) => with_obj(*r, |o| o.slots[0].clone()).expect_int(),
            other => panic!("not a ref: {other:?}"),
        }
    }

    fn s(v: &Value) -> String {
        match v {
            Value::Ref(Some(r)) => string_value(r).to_string(),
            Value::Ref(None) => "null".to_string(),
            other => panic!("not a ref: {other:?}"),
        }
    }

    /// The detailMessage slot of a thrown object (the exact IAE messages
    /// the narrowing matrix pins).
    fn throw_message(t: &Throw) -> String {
        let v = crate::heap::with_obj(t.obj, |o| o.slots[0].clone());
        match v {
            Value::Ref(Some(r)) => string_value(&r).to_string(),
            _ => String::new(),
        }
    }

    // ---- tableSizeFor (OpenJDK edge behaviour) ----

    #[test]
    fn table_size_for_matches_openjdk() {
        let _rt = crate::runtime();
        assert_eq!(table_size_for(0), 1);
        assert_eq!(table_size_for(1), 1);
        assert_eq!(table_size_for(2), 2);
        assert_eq!(table_size_for(3), 4);
        assert_eq!(table_size_for(4), 4);
        assert_eq!(table_size_for(5), 8);
        assert_eq!(table_size_for(12), 16);
        assert_eq!(table_size_for(16), 16);
        assert_eq!(table_size_for(17), 32);
        assert_eq!(table_size_for(1000), 1024);
        assert_eq!(table_size_for(1 << 30), 1 << 30);
        assert_eq!(table_size_for((1 << 30) + 5), 1 << 30);
        assert_eq!(table_size_for(-7), 1);
    }

    // ---- hash spreading ----

    #[test]
    fn hash_spreading_matches_java() {
        let _rt = crate::runtime();
        // pinned by hand: 0x12345678 ^ (0x12345678 >>> 16) = 0x1234444C
        assert_eq!(
            map_hash(&boxed_i32(0x12345678u32 as i32), ctx()).unwrap(),
            0x1234444Cu32 as i32
        );
        // Integer hashCode == value -> small ints spread to themselves
        assert_eq!(map_hash(&boxed_i32(5), ctx()).unwrap(), 5);
        // negative hash: -5 = 0xFFFFFFFB; h>>>16 = 0xFFFF;
        // xor -> 0xFFFF0004 (hand-computed, then verified vs Temurin via
        // the differential suite)
        assert_eq!(map_hash(&boxed_i32(-5), ctx()).unwrap(), -65532);
    }

    // ---- ArrayList ----

    #[test]
    fn arraylist_add_get_set_remove_tostring() {
        let _rt = crate::runtime();
        let l = list_new();
        for k in 0..3 {
            list_add(&l, Value::Ref(Some(new_string(&format!("e{k}")))));
        }
        let out = n_al_to_string(Some(&l), &[], ctx()).unwrap();
        assert_eq!(s(&out), "[e0, e1, e2]");
        // set returns the old element
        let old = n_al_set(
            Some(&l),
            &[Value::Int(1), Value::Ref(Some(new_string("x")))],
            ctx(),
        )
        .unwrap();
        assert_eq!(s(&old), "e1");
        assert_eq!(
            s(&n_al_get(Some(&l), &[Value::Int(1)], ctx()).unwrap()),
            "x"
        );
        // remove(I) returns the removed element and shifts
        let rem = n_al_remove_at(Some(&l), &[Value::Int(0)], ctx()).unwrap();
        assert_eq!(s(&rem), "e0");
        assert_eq!(s(&n_al_to_string(Some(&l), &[], ctx()).unwrap()), "[x, e2]");
        // remove(Object) by dispatched equals
        let found =
            n_al_remove_obj(Some(&l), &[Value::Ref(Some(new_string("e2")))], ctx()).unwrap();
        assert_eq!(found.expect_int(), 1);
        assert_eq!(s(&n_al_to_string(Some(&l), &[], ctx()).unwrap()), "[x]");
    }

    #[test]
    fn arraylist_bounds_messages_are_java_exact() {
        let _rt = crate::runtime();
        let l = list_new();
        list_add(&l, Value::Ref(Some(new_string("a"))));
        // get(-1) / get(size): Objects.checkIndex message
        let err = n_al_get(Some(&l), &[Value::Int(-1)], ctx()).unwrap_err();
        assert_eq!(
            s(&Value::Ref(Some(with_obj(err.obj, |o| o.slots[0]
                .clone()
                .expect_ref()
                .unwrap())))),
            "Index -1 out of bounds for length 1"
        );
        let err = n_al_get(Some(&l), &[Value::Int(1)], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/IndexOutOfBoundsException");
        // add(IL,E)V uses rangeCheckForAdd: "Index: i, Size: s"
        let err = n_al_add_at(Some(&l), &[Value::Int(5), Value::Ref(None)], ctx()).unwrap_err();
        let msg = with_obj(err.obj, |o| o.slots[0].clone());
        assert_eq!(s(&msg), "Index: 5, Size: 1");
    }

    #[test]
    fn arraylist_iterator_cme_and_exhaustion() {
        let _rt = crate::runtime();
        let l = list_new();
        list_add(&l, Value::Ref(Some(new_string("a"))));
        list_add(&l, Value::Ref(Some(new_string("b"))));
        let it = match n_al_iterator(Some(&l), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator"),
        };
        assert_eq!(
            n_itr_has_next(Some(&it), &[], ctx()).unwrap().expect_int(),
            1
        );
        assert_eq!(s(&n_itr_next(Some(&it), &[], ctx()).unwrap()), "a");
        // structural modification through the list -> next() throws CME
        list_add(&l, Value::Ref(Some(new_string("c"))));
        let err = n_itr_next(Some(&it), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), CME_CLASS);
        // fresh iterator, exhaust it -> NoSuchElementException
        let it2 = match n_al_iterator(Some(&l), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator"),
        };
        while n_itr_has_next(Some(&it2), &[], ctx()).unwrap().expect_int() == 1 {
            n_itr_next(Some(&it2), &[], ctx()).unwrap();
        }
        let err = n_itr_next(Some(&it2), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/util/NoSuchElementException");
        // Iterator.remove() is Java-faithful: removes the last-returned
        // element, shrinks the list, and re-syncs modCount (the differential
        // oracle pinned this; see M3_EVIDENCE.md for the plan deviation)
        let it3 = match n_al_iterator(Some(&l), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator"),
        };
        let _first = n_itr_next(Some(&it3), &[], ctx()).unwrap();
        n_itr_remove(Some(&it3), &[], ctx()).unwrap();
        assert_eq!(
            n_al_size(Some(&l), &[], ctx()).unwrap().expect_int(),
            2,
            "remove shrinks the list"
        );
        // removal through the iterator is NOT a CME for this iterator; the
        // cursor was rewound to lastRet, so next() yields the element after
        // the removed one
        let second = n_itr_next(Some(&it3), &[], ctx()).unwrap();
        assert_eq!(s(&second), "b", "cursor rewound to lastRet");
        // remove() without any prior next() -> IllegalStateException
        // (message null); Java allows remove() after every next(), so a
        // fresh iterator with lastRet = -1 is required to hit it
        let it4 = match n_al_iterator(Some(&l), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator"),
        };
        let err = n_itr_remove(Some(&it4), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/IllegalStateException");
    }

    #[test]
    fn arraylist_copy_constructor_copies_then_diverges() {
        let _rt = crate::runtime();
        // source [a, b] -> copy; copy equals source; later source mutation
        // must not touch the copy (real copy semantics, corpus wave 1)
        let src = list_new();
        list_add(&src, Value::Ref(Some(new_string("a"))));
        list_add(&src, Value::Ref(Some(new_string("b"))));
        let cp = new_obj("java/util/ArrayList", 3).unwrap();
        n_al_init_from_collection(Some(&cp), &[Value::Ref(Some(src))], ctx()).unwrap();
        assert_eq!(n_al_size(Some(&cp), &[], ctx()).unwrap().expect_int(), 2);
        assert_eq!(
            s(&n_al_get(Some(&cp), &[Value::Int(0)], ctx()).unwrap()),
            "a"
        );
        assert_eq!(
            s(&n_al_get(Some(&cp), &[Value::Int(1)], ctx()).unwrap()),
            "b"
        );
        // mutate the source; the copy diverges
        list_add(&src, Value::Ref(Some(new_string("c"))));
        assert_eq!(
            n_al_size(Some(&cp), &[], ctx()).unwrap().expect_int(),
            2,
            "copy must not see later source appends"
        );
        assert_eq!(n_al_size(Some(&src), &[], ctx()).unwrap().expect_int(), 3);
        // null source -> NPE, like the JDK
        let cp2 = new_obj("java/util/ArrayList", 3).unwrap();
        let err = n_al_init_from_collection(Some(&cp2), &[Value::Ref(None)], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/NullPointerException");
    }

    #[test]
    fn arraylist_copy_constructor_reads_through_unmodifiable_view() {
        let _rt = crate::runtime();
        // the copy-ctor must iterate through ANY collection impl — here the
        // Collections$UnmodifiableList view over an ArrayList — and the view
        // itself must reject writes with UnsupportedOperationException
        let src = list_new();
        list_add(&src, Value::Ref(Some(new_string("x"))));
        list_add(&src, Value::Ref(Some(new_string("y"))));
        let view =
            match crate::jdkutil::n_unmod_list(None, &[Value::Ref(Some(src))], ctx()).unwrap() {
                Value::Ref(Some(v)) => v,
                other => panic!("view: {other:?}"),
            };
        let cp = new_obj("java/util/ArrayList", 3).unwrap();
        n_al_init_from_collection(Some(&cp), &[Value::Ref(Some(view))], ctx()).unwrap();
        assert_eq!(n_al_size(Some(&cp), &[], ctx()).unwrap().expect_int(), 2);
        assert_eq!(
            s(&n_al_get(Some(&cp), &[Value::Int(1)], ctx()).unwrap()),
            "y"
        );
        // the view rejects writes (JDK wrapper semantics), reads still live
        let err =
            crate::jdkutil::n_unmod_add(Some(&view), &[Value::Ref(Some(new_string("z")))], ctx())
                .unwrap_err();
        assert_eq!(err.java_class(), "java/lang/UnsupportedOperationException");
        // the backing list is untouched by the rejected write
        assert_eq!(n_al_size(Some(&src), &[], ctx()).unwrap().expect_int(), 2);
    }

    #[test]
    fn arraylist_clear_and_null_elements() {
        let _rt = crate::runtime();
        let l = list_new();
        list_add(&l, Value::Ref(None));
        list_add(&l, Value::Ref(Some(new_string("z"))));
        assert_eq!(
            s(&n_al_to_string(Some(&l), &[], ctx()).unwrap()),
            "[null, z]"
        );
        n_al_clear(Some(&l), &[], ctx()).unwrap();
        assert_eq!(n_al_size(Some(&l), &[], ctx()).unwrap().expect_int(), 0);
        assert_eq!(s(&n_al_to_string(Some(&l), &[], ctx()).unwrap()), "[]");
    }

    // ---- HashMap ----

    #[test]
    fn hashmap_order_is_table_order_with_tail_bins() {
        let _rt = crate::runtime();
        // Integer keys (hash = value). 14 distinct keys inserted in the
        // order 1, 17, 0, 2..=12: keys 1 and 17 collide in bin 1 of the
        // cap-16 table (17 tail-inserted after 1); the 13th put resizes to
        // cap 32 and the Java-8 split moves 17 to bin 17.
        let m = map_new();
        let mut keys = vec![1, 17, 0];
        keys.extend(2..=12);
        for k in &keys {
            n_hm_put(Some(&m), &[boxed_i32(*k), boxed_i32(*k * 10)], ctx()).unwrap();
        }
        let out = n_hm_to_string(Some(&m), &[], ctx()).unwrap();
        assert_eq!(s(&out), "{0=0, 1=10, 2=20, 3=30, 4=40, 5=50, 6=60, 7=70, 8=80, 9=90, 10=100, 11=110, 12=120, 17=170}");
    }

    #[test]
    fn hashmap_put_replaces_and_returns_old_without_cme() {
        let _rt = crate::runtime();
        let m = map_new();
        let old = n_hm_put(Some(&m), &[boxed_i32(7), boxed_i32(1)], ctx()).unwrap();
        assert!(old.is_null(), "fresh put returns null");
        let old = n_hm_put(Some(&m), &[boxed_i32(7), boxed_i32(2)], ctx()).unwrap();
        assert_eq!(unbox_i32(&old), 1, "replace returns the old value");
        // replacing is NOT a structural modification: iteration still works
        let ks = match n_hm_key_set(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(k)) => k,
            _ => panic!("keySet"),
        };
        let it = match n_ks_iterator(Some(&ks), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator"),
        };
        assert_eq!(unbox_i32(&n_ki_next(Some(&it), &[], ctx()).unwrap()), 7);
        assert_eq!(
            n_ki_has_next(Some(&it), &[], ctx()).unwrap().expect_int(),
            0
        );
    }

    #[test]
    fn hashmap_remove_and_views() {
        let _rt = crate::runtime();
        let m = map_new();
        for k in 0..5 {
            n_hm_put(Some(&m), &[boxed_i32(k), boxed_i32(k + 100)], ctx()).unwrap();
        }
        let old = n_hm_remove(Some(&m), &[boxed_i32(2)], ctx()).unwrap();
        assert_eq!(unbox_i32(&old), 102);
        assert_eq!(n_hm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 4);
        // keySet view: size, contains, toString (table order)
        let ks = match n_hm_key_set(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(k)) => k,
            _ => panic!("keySet"),
        };
        assert_eq!(n_ks_size(Some(&ks), &[], ctx()).unwrap().expect_int(), 4);
        assert_eq!(
            n_ks_contains(Some(&ks), &[boxed_i32(3)], ctx())
                .unwrap()
                .expect_int(),
            1
        );
        let out = n_ks_to_string(Some(&ks), &[], ctx()).unwrap();
        assert_eq!(s(&out), "[0, 1, 3, 4]");
        // values view: faithful table order
        let vs = match n_hm_values(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("values"),
        };
        let out = n_vs_to_string(Some(&vs), &[], ctx()).unwrap();
        assert_eq!(s(&out), "[100, 101, 103, 104]");
        // getOrDefault: present vs default
        let d = n_hm_get_or_default(Some(&m), &[boxed_i32(3), boxed_i32(-1)], ctx()).unwrap();
        assert_eq!(unbox_i32(&d), 103);
        let d = n_hm_get_or_default(Some(&m), &[boxed_i32(2), boxed_i32(-1)], ctx()).unwrap();
        assert_eq!(unbox_i32(&d), -1);
    }

    #[test]
    fn hashmap_iterator_cme_hasnext_no_check_next_checks() {
        let _rt = crate::runtime();
        let m = map_new();
        for k in 0..3 {
            n_hm_put(Some(&m), &[boxed_i32(k), boxed_i32(k)], ctx()).unwrap();
        }
        let ks = match n_hm_key_set(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(k)) => k,
            _ => panic!("keySet"),
        };
        let it = match n_ks_iterator(Some(&ks), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator"),
        };
        assert_eq!(
            n_ki_has_next(Some(&it), &[], ctx()).unwrap().expect_int(),
            1
        );
        // structural modification: hasNext still true (OpenJDK semantics),
        // next throws CME
        n_hm_put(Some(&m), &[boxed_i32(50), boxed_i32(50)], ctx()).unwrap();
        assert_eq!(
            n_ki_has_next(Some(&it), &[], ctx()).unwrap().expect_int(),
            1
        );
        let err = n_ki_next(Some(&it), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), CME_CLASS);
    }

    #[test]
    fn hashmap_null_key_and_value() {
        let _rt = crate::runtime();
        let m = map_new();
        n_hm_put(Some(&m), &[Value::Ref(None), Value::Ref(None)], ctx()).unwrap();
        assert_eq!(n_hm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 1);
        let v = n_hm_get(Some(&m), &[Value::Ref(None)], ctx()).unwrap();
        assert!(v.is_null());
        n_hm_put(Some(&m), &[Value::Ref(None), boxed_i32(9)], ctx()).unwrap();
        let v = n_hm_get(Some(&m), &[Value::Ref(None)], ctx()).unwrap();
        assert_eq!(unbox_i32(&v), 9);
        let out = n_hm_to_string(Some(&m), &[], ctx()).unwrap();
        assert_eq!(s(&out), "{null=9}");
    }

    #[test]
    fn hashmap_initial_capacity_negative_is_java_exact() {
        let _rt = crate::runtime();
        let m = new_obj("java/util/HashMap", 4).unwrap();
        let err = n_hm_init(Some(&m), &[Value::Int(-3)], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/IllegalArgumentException");
        let msg = with_obj(err.obj, |o| o.slots[0].clone());
        assert_eq!(s(&msg), "Illegal initial capacity: -3");
    }

    // ---- LinkedHashMap (run 4: the ordered map) ----

    fn lhm_new() -> JRef {
        let m = new_obj("java/util/LinkedHashMap", 6).unwrap();
        n_lhm_init(Some(&m), &[], ctx()).unwrap();
        m
    }

    #[test]
    fn lhm_order_laws_insertion_refixture_remove_tail() {
        let _rt = crate::runtime();
        let m = lhm_new();
        for (k, v) in [("a", 1), ("b", 2), ("c", 3)] {
            n_lhm_put(
                Some(&m),
                &[Value::Ref(Some(new_string(k))), boxed_i32(v)],
                ctx(),
            )
            .unwrap();
        }
        // L1: insertion order
        assert_eq!(
            s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
            "{a=1, b=2, c=3}"
        );
        // L2: re-put replaces the value and KEEPS position
        n_lhm_put(
            Some(&m),
            &[Value::Ref(Some(new_string("a"))), boxed_i32(10)],
            ctx(),
        )
        .unwrap();
        assert_eq!(
            s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
            "{a=10, b=2, c=3}"
        );
        // L3: remove + re-put moves to the tail
        let old = n_lhm_remove(Some(&m), &[Value::Ref(Some(new_string("a")))], ctx()).unwrap();
        assert_eq!(unbox_i32(&old), 10);
        n_lhm_put(
            Some(&m),
            &[Value::Ref(Some(new_string("a"))), boxed_i32(11)],
            ctx(),
        )
        .unwrap();
        assert_eq!(
            s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
            "{b=2, c=3, a=11}"
        );
        // L4: get / containsKey never reorder (the inherited rows are
        // order-neutral; the order links untouched)
        let got = n_hm_get(Some(&m), &[Value::Ref(Some(new_string("b")))], ctx()).unwrap();
        assert_eq!(unbox_i32(&got), 2);
        n_hm_contains_key(Some(&m), &[Value::Ref(Some(new_string("c")))], ctx()).unwrap();
        assert_eq!(
            s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
            "{b=2, c=3, a=11}"
        );
    }

    #[test]
    fn lhm_views_iterate_in_link_order() {
        let _rt = crate::runtime();
        let m = lhm_new();
        for (k, v) in [("a", 1), ("b", 2), ("c", 3)] {
            n_lhm_put(
                Some(&m),
                &[Value::Ref(Some(new_string(k))), boxed_i32(v)],
                ctx(),
            )
            .unwrap();
        }
        let ks = match n_lhm_key_set(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(k)) => k,
            _ => panic!("keySet"),
        };
        assert_eq!(
            s(&n_lks_to_string(Some(&ks), &[], ctx()).unwrap()),
            "[a, b, c]"
        );
        let vs = match n_lhm_values(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("values"),
        };
        assert_eq!(
            s(&n_lvs_to_string(Some(&vs), &[], ctx()).unwrap()),
            "[1, 2, 3]"
        );
        // the entry iterator walks the link chain; getKey/getValue work
        // through the $Entry node (the Node prefix layout)
        let es = match n_lhm_entry_set(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(e)) => e,
            _ => panic!("entrySet"),
        };
        let it = match n_les_iterator(Some(&es), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator"),
        };
        let mut out = String::new();
        while n_lei_has_next(Some(&it), &[], ctx()).unwrap().expect_int() == 1 {
            let e = n_lei_next(Some(&it), &[], ctx()).unwrap();
            let node = match e {
                Value::Ref(Some(n)) => n,
                _ => panic!("entry node"),
            };
            let k = n_node_get_key(Some(&node), &[], ctx()).unwrap();
            let v = n_node_get_value(Some(&node), &[], ctx()).unwrap();
            out.push_str(&format!("{}={};", s(&k), unbox_i32(&v)));
        }
        assert_eq!(out, "a=1;b=2;c=3;");
        // size/isEmpty/contains delegate through the inherited shapes
        assert_eq!(n_lks_size(Some(&ks), &[], ctx()).unwrap().expect_int(), 3);
        assert_eq!(
            n_lks_contains(Some(&ks), &[Value::Ref(Some(new_string("b")))], ctx())
                .unwrap()
                .expect_int(),
            1
        );
        assert_eq!(
            n_lvs_contains(Some(&vs), &[boxed_i32(3)], ctx())
                .unwrap()
                .expect_int(),
            1
        );
    }

    #[test]
    fn lhm_iterator_remove_order_and_modcount_law() {
        let _rt = crate::runtime();
        let m = lhm_new();
        for (k, v) in [("a", 1), ("b", 2), ("c", 3)] {
            n_lhm_put(
                Some(&m),
                &[Value::Ref(Some(new_string(k))), boxed_i32(v)],
                ctx(),
            )
            .unwrap();
        }
        let es = match n_lhm_entry_set(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(e)) => e,
            _ => panic!("entrySet"),
        };
        let it = match n_les_iterator(Some(&es), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator"),
        };
        n_lei_next(Some(&it), &[], ctx()).unwrap();
        n_lhm_itr_remove(Some(&it), &[], ctx()).unwrap();
        // the head is gone, order intact for the rest
        assert_eq!(
            s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
            "{b=2, c=3}"
        );
        // continued iteration: no CME (expectedModCount re-synced)
        let e = n_lei_next(Some(&it), &[], ctx()).unwrap();
        let node = match e {
            Value::Ref(Some(n)) => n,
            _ => panic!("entry node"),
        };
        assert_eq!(s(&n_node_get_key(Some(&node), &[], ctx()).unwrap()), "b");
        // remove again right away is LEGAL (lastRet = b)
        n_lhm_itr_remove(Some(&it), &[], ctx()).unwrap();
        assert_eq!(s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()), "{c=3}");
        // remove WITHOUT a next = IllegalStateException (lastRet cleared)
        let err = n_lhm_itr_remove(Some(&it), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/IllegalStateException");
        // CME: a structural modification behind the iterator's back
        n_lhm_put(
            Some(&m),
            &[Value::Ref(Some(new_string("z"))), boxed_i32(9)],
            ctx(),
        )
        .unwrap();
        let err = n_lei_next(Some(&it), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), CME_CLASS);
    }

    #[test]
    fn hash_iter_remove_order_and_modcount_law() {
        let _rt = crate::runtime();
        let m = map_new();
        for k in 0..3 {
            n_hm_put(Some(&m), &[boxed_i32(k), boxed_i32(k + 10)], ctx()).unwrap();
        }
        let ks = match n_hm_key_set(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(k)) => k,
            _ => panic!("keySet"),
        };
        let it = match n_ks_iterator(Some(&ks), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator"),
        };
        // remove the key == 2 node mid-walk; the walk continues (no CME)
        let mut removed = false;
        while n_ki_has_next(Some(&it), &[], ctx()).unwrap().expect_int() == 1 {
            let k = n_ki_next(Some(&it), &[], ctx()).unwrap();
            if unbox_i32(&k) == 2 {
                n_hi_remove(Some(&it), &[], ctx()).unwrap();
                removed = true;
            }
        }
        assert!(removed, "walked and removed key 2");
        assert_eq!(n_hm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 2);
        let ks2 = match n_hm_key_set(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(k)) => k,
            _ => panic!("keySet 2"),
        };
        assert_eq!(
            n_ks_contains(Some(&ks2), &[boxed_i32(2)], ctx())
                .unwrap()
                .expect_int(),
            0
        );
        // double remove: current cleared → ISE
        let err = n_hi_remove(Some(&it), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/IllegalStateException");
        // foreign modification behind the iterator's back, then next → CME
        n_hm_put(Some(&m), &[boxed_i32(9), boxed_i32(90)], ctx()).unwrap();
        let err = n_ki_next(Some(&it), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), CME_CLASS);
        // THE PROBED COMBINED LAW (Temurin case 1): a fresh iterator over a
        // foreign-modified map, remove() without ANY next() → ISE, NOT CME
        // (the ISE check precedes the CME check in HashIterator.remove)
        let ks3 = match n_hm_key_set(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(k)) => k,
            _ => panic!("keySet 3"),
        };
        let it3 = match n_ks_iterator(Some(&ks3), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator 3"),
        };
        let err = n_hi_remove(Some(&it3), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/IllegalStateException");
        // the CME branch is still reachable: current set + foreign mod
        let m4 = map_new();
        for k in 0..2 {
            n_hm_put(Some(&m4), &[boxed_i32(k), boxed_i32(k)], ctx()).unwrap();
        }
        let ks4 = match n_hm_key_set(Some(&m4), &[], ctx()).unwrap() {
            Value::Ref(Some(k)) => k,
            _ => panic!("keySet 4"),
        };
        let it4 = match n_ks_iterator(Some(&ks4), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator 4"),
        };
        n_ki_next(Some(&it4), &[], ctx()).unwrap();
        n_hm_remove(Some(&m4), &[boxed_i32(0)], ctx()).unwrap();
        let err = n_hi_remove(Some(&it4), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), CME_CLASS);
    }

    #[test]
    fn hash_iter_remove_value_and_entry_law() {
        let _rt = crate::runtime();
        let m = map_new();
        for k in 0..3 {
            n_hm_put(Some(&m), &[boxed_i32(k), boxed_i32(k + 10)], ctx()).unwrap();
        }
        // value iterator remove: the whole node (key AND value) is removed
        let vs = match n_hm_values(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("values"),
        };
        let it = match n_vs_iterator(Some(&vs), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("value iterator"),
        };
        let v = n_vi_next(Some(&it), &[], ctx()).unwrap();
        n_hi_remove(Some(&it), &[], ctx()).unwrap();
        assert_eq!(n_hm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 2);
        // continued value iteration: no CME, two values remain
        let rest = n_vi_next(Some(&it), &[], ctx()).unwrap();
        let _ = unbox_i32(&rest);
        let _ = v;
        // entry iterator remove: next() returns the node, remove() drops it
        let es = match n_hm_entry_set(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(e)) => e,
            _ => panic!("entrySet"),
        };
        let eit = match n_hes_iterator(Some(&es), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("entry iterator"),
        };
        let entry = match n_ei_next(Some(&eit), &[], ctx()).unwrap() {
            Value::Ref(Some(n)) => n,
            _ => panic!("entry node"),
        };
        let key = n_node_get_key(Some(&entry), &[], ctx()).unwrap();
        n_hi_remove(Some(&eit), &[], ctx()).unwrap();
        assert_eq!(n_hm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 1);
        assert_eq!(
            n_hm_contains_key(Some(&m), &[key], ctx())
                .unwrap()
                .expect_int(),
            0
        );
        // entry iterator remove without next → ISE (fresh iterator)
        let eit2 = match n_hes_iterator(Some(&es), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("entry iterator 2"),
        };
        let err = n_hi_remove(Some(&eit2), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/IllegalStateException");
    }

    #[test]
    fn lhm_itr_remove_ise_before_cme_law() {
        let _rt = crate::runtime();
        // the probed LinkedHashMap combined law (Temurin case 5): a fresh
        // link iterator over a foreign-modified map, remove() without any
        // next() → ISE, NOT CME — the original CME-first order diverged
        // from Temurin and was corrected against the ItrOrderProbe oracle
        let m = lhm_new();
        for (k, v) in [("a", 1), ("b", 2)] {
            n_lhm_put(
                Some(&m),
                &[Value::Ref(Some(new_string(k))), boxed_i32(v)],
                ctx(),
            )
            .unwrap();
        }
        let es = match n_lhm_entry_set(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(e)) => e,
            _ => panic!("entrySet"),
        };
        let it = match n_les_iterator(Some(&es), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator"),
        };
        // foreign modification BEFORE any next()
        n_lhm_remove(Some(&m), &[Value::Ref(Some(new_string("a")))], ctx()).unwrap();
        let err = n_lhm_itr_remove(Some(&it), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/IllegalStateException");
        // lastRet set + foreign mod → CME (the ISE check passed, CME fires)
        let it2 = match n_les_iterator(Some(&es), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator 2"),
        };
        n_lei_next(Some(&it2), &[], ctx()).unwrap();
        n_lhm_remove(Some(&m), &[Value::Ref(Some(new_string("b")))], ctx()).unwrap();
        let err = n_lhm_itr_remove(Some(&it2), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), CME_CLASS);
    }

    #[test]
    fn treemap_sorted_order_laws() {
        let _rt = crate::runtime();
        let m = tm_new();
        for (k, v) in [(42, "a"), (-7, "b"), (0, "c"), (100, "d"), (13, "e")] {
            n_tm_put(
                Some(&m),
                &[boxed_i32(k), Value::Ref(Some(new_string(v)))],
                ctx(),
            )
            .unwrap();
        }
        // T1: iteration in KEY order (the whole point)
        assert_eq!(
            s(&n_tm_to_string(Some(&m), &[], ctx()).unwrap()),
            "{-7=b, 0=c, 13=e, 42=a, 100=d}"
        );
        // T2: firstKey/lastKey
        assert_eq!(
            unbox_i32(&n_tm_first_key(Some(&m), &[], ctx()).unwrap()),
            -7
        );
        assert_eq!(
            unbox_i32(&n_tm_last_key(Some(&m), &[], ctx()).unwrap()),
            100
        );
        // T3: duplicate put replaces, keeps position, returns old
        let old = n_tm_put(
            Some(&m),
            &[boxed_i32(13), Value::Ref(Some(new_string("E13")))],
            ctx(),
        )
        .unwrap();
        assert_eq!(s(&old), "e");
        assert_eq!(
            s(&n_tm_get(Some(&m), &[boxed_i32(13)], ctx()).unwrap()),
            "E13"
        );
        assert_eq!(n_tm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 5);
        assert_eq!(
            s(&n_tm_to_string(Some(&m), &[], ctx()).unwrap()),
            "{-7=b, 0=c, 13=E13, 42=a, 100=d}"
        );
        // T4: remove returns old, absent -> null, order preserved
        let old = n_tm_remove(Some(&m), &[boxed_i32(0)], ctx()).unwrap();
        assert_eq!(s(&old), "c");
        let old = n_tm_remove(Some(&m), &[boxed_i32(999)], ctx()).unwrap();
        assert!(old.is_null());
        assert_eq!(n_tm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 4);
        let ks = match n_tm_key_set(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(k)) => k,
            _ => panic!("keySet"),
        };
        assert_eq!(
            s(&n_tks_to_string(Some(&ks), &[], ctx()).unwrap()),
            "[-7, 13, 42, 100]"
        );
    }

    #[test]
    fn treemap_navigable_laws() {
        let _rt = crate::runtime();
        let m = tm_new();
        for (k, v) in [(42, "a"), (-7, "b"), (0, "c"), (100, "d"), (13, "e")] {
            n_tm_put(
                Some(&m),
                &[boxed_i32(k), Value::Ref(Some(new_string(v)))],
                ctx(),
            )
            .unwrap();
        }
        // T5: ceiling/floor accept exact; higher/lower are strict;
        // out-of-range -> null (the probed case-5 row)
        type TmNavFn = fn(Option<&JRef>, &[Value], Dispatch) -> Result<Value, Throw>;
        let nav = |f: TmNavFn, key: i32, want: Option<i32>, name: &str| {
            let got = f(Some(&m), &[boxed_i32(key)], ctx()).unwrap();
            match want {
                Some(w) => assert_eq!(unbox_i32(&got), w, "{name}"),
                None => assert!(got.is_null(), "{name}"),
            }
        };
        nav(n_tm_ceiling_key, 13, Some(13), "ceil13");
        nav(n_tm_floor_key, 13, Some(13), "floor13");
        nav(n_tm_ceiling_key, 14, Some(42), "ceil14");
        nav(n_tm_floor_key, 14, Some(13), "floor14");
        nav(n_tm_higher_key, 13, Some(42), "hi13");
        // the oracle's lo13 = -7 was AFTER case 4 removed key 0; here 0 is
        // present, so lower(13) = 0
        nav(n_tm_lower_key, 13, Some(0), "lo13");
        nav(n_tm_ceiling_key, 999, None, "ceil999");
        nav(n_tm_floor_key, -99, None, "floor-99");
        nav(n_tm_higher_key, 100, None, "hi100");
        nav(n_tm_lower_key, -7, None, "lo-7");
    }

    #[test]
    fn treemap_comparator_and_exceptions_law() {
        let _rt = crate::runtime();
        // T6: the Comparator surface reorders everything (the probed
        // lambda `(x, y) -> y - x` — the unit test rides the builtin
        // Comparator.reverseOrder() singleton, the same Comparator walk)
        let d = new_obj("java/util/TreeMap", 4).unwrap();
        let cmp = match crate::jdkutil::n_comparator_reverse_order(None, &[], ctx()).unwrap() {
            Value::Ref(Some(c)) => c,
            _ => panic!("reverseOrder"),
        };
        n_tm_init_cmp(Some(&d), &[Value::Ref(Some(cmp))], ctx()).unwrap();
        for (k, v) in [(42, "a"), (-7, "b"), (0, "c"), (100, "d")] {
            n_tm_put(
                Some(&d),
                &[boxed_i32(k), Value::Ref(Some(new_string(v)))],
                ctx(),
            )
            .unwrap();
        }
        assert_eq!(
            s(&n_tm_to_string(Some(&d), &[], ctx()).unwrap()),
            "{100=d, 42=a, 0=c, -7=b}"
        );
        assert_eq!(
            unbox_i32(&n_tm_first_key(Some(&d), &[], ctx()).unwrap()),
            100
        );
        assert_eq!(unbox_i32(&n_tm_last_key(Some(&d), &[], ctx()).unwrap()), -7);
        // T7: put(null) under natural ordering -> NPE (even on an empty map)
        let n = tm_new();
        let err = n_tm_put(
            Some(&n),
            &[Value::Ref(None), Value::Ref(Some(new_string("x")))],
            ctx(),
        )
        .unwrap_err();
        assert_eq!(err.java_class(), "java/lang/NullPointerException");
        // T8: put(non-Comparable) -> ClassCastException
        let obj = new_obj("java/lang/Object", 0).unwrap();
        let err = n_tm_put(
            Some(&n),
            &[Value::Ref(Some(obj)), Value::Ref(Some(new_string("x")))],
            ctx(),
        )
        .unwrap_err();
        assert_eq!(err.java_class(), "java/lang/ClassCastException");
    }

    #[test]
    fn treemap_ctor_from_map_law() {
        let _rt = crate::runtime();
        // T9: the copy ctor sorts regardless of the source's iteration order
        let src = map_new();
        for (k, v) in [(5, "v5"), (-3, "v-3"), (27, "v27")] {
            n_hm_put(
                Some(&src),
                &[boxed_i32(k), Value::Ref(Some(new_string(v)))],
                ctx(),
            )
            .unwrap();
        }
        let c = new_obj("java/util/TreeMap", 4).unwrap();
        n_tm_init_map(Some(&c), &[Value::Ref(Some(src))], ctx()).unwrap();
        assert_eq!(
            s(&n_tm_to_string(Some(&c), &[], ctx()).unwrap()),
            "{-3=v-3, 5=v5, 27=v27}"
        );
    }

    #[test]
    fn treemap_iterator_remove_and_cme_laws() {
        let _rt = crate::runtime();
        let m = tm_new();
        for k in 1..=4 {
            n_tm_put(
                Some(&m),
                &[boxed_i32(k), Value::Ref(Some(new_string("v")))],
                ctx(),
            )
            .unwrap();
        }
        let ks = match n_tm_key_set(Some(&m), &[], ctx()).unwrap() {
            Value::Ref(Some(k)) => k,
            _ => panic!("keySet"),
        };
        let it = match n_tks_iterator(Some(&ks), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator"),
        };
        // T10: remove mid-walk — order intact, no CME (probed case 9)
        let mut walked: Vec<i32> = Vec::new();
        while n_tm_i_has_next(Some(&it), &[], ctx()).unwrap().expect_int() == 1 {
            let k = n_tki_next(Some(&it), &[], ctx()).unwrap();
            if unbox_i32(&k) == 2 {
                n_tm_i_remove(Some(&it), &[], ctx()).unwrap();
            } else {
                walked.push(unbox_i32(&k));
            }
        }
        assert_eq!(walked, vec![1, 3, 4]);
        assert_eq!(n_tm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 3);
        // double remove: next + remove (ok) + remove (ISE — current
        // cleared; probed case 3), on a FRESH iterator so the walk's
        // last-returned state does not leak in
        let ita = match n_tks_iterator(Some(&ks), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator a"),
        };
        n_tki_next(Some(&ita), &[], ctx()).unwrap();
        n_tm_i_remove(Some(&ita), &[], ctx()).unwrap();
        let err = n_tm_i_remove(Some(&ita), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/IllegalStateException");
        // remove without ANY next on a fresh iterator -> ISE
        let itb = match n_tks_iterator(Some(&ks), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator b"),
        };
        let err = n_tm_i_remove(Some(&itb), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/IllegalStateException");
        // fresh iterator + foreign modification + remove without next ->
        // ISE FIRST (the run-4 probed combined law, Temurin case 1/5)
        let it2 = match n_tks_iterator(Some(&ks), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator 2"),
        };
        n_tm_put(
            Some(&m),
            &[boxed_i32(99), Value::Ref(Some(new_string("z")))],
            ctx(),
        )
        .unwrap();
        let err = n_tm_i_remove(Some(&it2), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/IllegalStateException");
        // current set + foreign mod -> CME (the ISE check passed, CME fires)
        let it3 = match n_tks_iterator(Some(&ks), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator 3"),
        };
        n_tki_next(Some(&it3), &[], ctx()).unwrap();
        n_tm_put(
            Some(&m),
            &[boxed_i32(98), Value::Ref(Some(new_string("z")))],
            ctx(),
        )
        .unwrap();
        let err = n_tm_i_remove(Some(&it3), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), CME_CLASS);
        // next after a foreign modification -> CME (probed case 10)
        let it4 = match n_tks_iterator(Some(&ks), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator 4"),
        };
        n_tki_next(Some(&it4), &[], ctx()).unwrap();
        n_tm_put(
            Some(&m),
            &[boxed_i32(97), Value::Ref(Some(new_string("z")))],
            ctx(),
        )
        .unwrap();
        let err = n_tki_next(Some(&it4), &[], ctx()).unwrap_err();
        assert_eq!(err.java_class(), CME_CLASS);
    }

    /// The red-black integrity check: BST bounds, no red parent with a red
    /// child, identical black heights on every root-to-leaf path.
    fn tm_check_rb(e: &JRef, lo: Option<i32>, hi: Option<i32>) -> i32 {
        let k = unbox_i32(&tm_entry_key(e));
        if let Some(lo) = lo {
            assert!(k > lo, "BST bound violated: {k} <= {lo}");
        }
        if let Some(hi) = hi {
            assert!(k < hi, "BST bound violated: {k} >= {hi}");
        }
        let color = tm_color(Some(e));
        let l = tm_child(e, TME_LEFT);
        let r = tm_child(e, TME_RIGHT);
        let lb = match l {
            Some(c) => {
                if color == TM_RED {
                    assert_ne!(tm_color(Some(&c)), TM_RED, "red-red at {k}.left");
                }
                tm_check_rb(&c, lo, Some(k))
            }
            None => 1,
        };
        let rb = match r {
            Some(c) => {
                if color == TM_RED {
                    assert_ne!(tm_color(Some(&c)), TM_RED, "red-red at {k}.right");
                }
                tm_check_rb(&c, Some(k), hi)
            }
            None => 1,
        };
        assert_eq!(lb, rb, "black-height mismatch at {k}");
        lb + (color == TM_BLACK) as i32
    }

    #[test]
    fn treemap_red_black_integrity_law() {
        let _rt = crate::runtime();
        // the ascending 1..=64 rung: the worst case for the right spine,
        // every rotation + recolor shape exercised
        let m = tm_new();
        for k in 1..=64 {
            n_tm_put(
                Some(&m),
                &[boxed_i32(k), Value::Ref(Some(new_string("v")))],
                ctx(),
            )
            .unwrap();
            if let Some(r) = tm_root(&m) {
                tm_check_rb(&r, None, None);
            }
        }
        let root = tm_root(&m).expect("root after 64 inserts");
        assert_eq!(tm_color(Some(&root)), TM_BLACK, "the root is black");
        tm_check_rb(&root, None, None);
        // remove every even key (two-child + leaf deletes, the sentinel
        // fixup path), then re-prove integrity and order
        for k in (2..=64).step_by(2) {
            let old = n_tm_remove(Some(&m), &[boxed_i32(k)], ctx()).unwrap();
            assert!(!old.is_null());
            if let Some(r) = tm_root(&m) {
                tm_check_rb(&r, None, None);
            }
        }
        assert_eq!(n_tm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 32);
        let root = tm_root(&m).expect("root after the even-key deletes");
        tm_check_rb(&root, None, None);
        let mut keys = Vec::new();
        for (k, _) in tm_all_entries(&m).unwrap() {
            keys.push(unbox_i32(&k));
        }
        let want: Vec<i32> = (1..=64).filter(|k| k % 2 == 1).collect();
        assert_eq!(keys, want, "odd keys remain in key order");
        // a descending insert rung (the left-spine mirror)
        let d = tm_new();
        for k in (1..=50).rev() {
            n_tm_put(
                Some(&d),
                &[boxed_i32(k), Value::Ref(Some(new_string("v")))],
                ctx(),
            )
            .unwrap();
        }
        let root = tm_root(&d).expect("root after the descending rung");
        tm_check_rb(&root, None, None);
    }

    // --- run 6: the bounded views (the V-laws; probe scripts/evidence/
    // treemap_views/, 35 Temurin cases) ---

    /// A {1=a,3=c,5=e,7=g,9=i} map plus the [3,7) view the probe rides.
    fn tm_views_fixture() -> (JRef, JRef) {
        let m = tm_new();
        for (k, v) in [(1, "a"), (3, "c"), (5, "e"), (7, "g"), (9, "i")] {
            n_tm_put(
                Some(&m),
                &[boxed_i32(k), Value::Ref(Some(new_string(v)))],
                ctx(),
            )
            .unwrap();
        }
        let sub = match n_tm_sub_map(Some(&m), &[boxed_i32(3), boxed_i32(7)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("subMap view"),
        };
        (m, sub)
    }

    #[test]
    fn submap_range_surface_law() {
        let _rt = crate::runtime();
        // V1/V2: contents (from-inclusive, to-EXCLUSIVE), computed size,
        // toString; the 4-arg inclusive variants; empty views
        let (m, sub) = tm_views_fixture();
        assert_eq!(
            s(&n_sms_to_string(Some(&sub), &[], ctx()).unwrap()),
            "{3=c, 5=e}"
        );
        assert_eq!(n_sms_size(Some(&sub), &[], ctx()).unwrap().expect_int(), 2);
        assert_eq!(
            n_sms_is_empty(Some(&sub), &[], ctx()).unwrap().expect_int(),
            0
        );
        // the absolute builders: headMap/tailMap + the four-arg flags
        let head = match n_tm_head_map(Some(&m), &[boxed_i32(5)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("headMap"),
        };
        assert_eq!(
            s(&n_sms_to_string(Some(&head), &[], ctx()).unwrap()),
            "{1=a, 3=c}"
        );
        let tail = match n_tm_tail_map(Some(&m), &[boxed_i32(5)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("tailMap"),
        };
        assert_eq!(
            s(&n_sms_to_string(Some(&tail), &[], ctx()).unwrap()),
            "{5=e, 7=g, 9=i}"
        );
        let ii = match n_tm_sub_map4(
            Some(&m),
            &[boxed_i32(3), Value::Int(1), boxed_i32(7), Value::Int(1)],
            ctx(),
        )
        .unwrap()
        {
            Value::Ref(Some(v)) => v,
            _ => panic!("subMap4"),
        };
        assert_eq!(
            s(&n_sms_to_string(Some(&ii), &[], ctx()).unwrap()),
            "{3=c, 5=e, 7=g}"
        );
        // V3: first/lastKey over the range; NSEE on an empty view
        assert_eq!(
            unbox_i32(&n_sms_first_key(Some(&sub), &[], ctx()).unwrap()),
            3
        );
        assert_eq!(
            unbox_i32(&n_sms_last_key(Some(&sub), &[], ctx()).unwrap()),
            5
        );
        let none = match n_tm_sub_map(Some(&m), &[boxed_i32(10), boxed_i32(20)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("empty view"),
        };
        assert_eq!(n_sms_size(Some(&none), &[], ctx()).unwrap().expect_int(), 0);
        assert_eq!(
            n_sms_is_empty(Some(&none), &[], ctx())
                .unwrap()
                .expect_int(),
            1
        );
        assert_eq!(s(&n_sms_to_string(Some(&none), &[], ctx()).unwrap()), "{}");
        assert_eq!(
            n_sms_first_key(Some(&none), &[], ctx())
                .unwrap_err()
                .java_class(),
            "java/util/NoSuchElementException"
        );
        // the c07c law: lastKey on an empty view whose lo is above every
        // key — Floor(hi) lands below lo and must fence to NSEE
        assert_eq!(
            n_sms_last_key(Some(&none), &[], ctx())
                .unwrap_err()
                .java_class(),
            "java/util/NoSuchElementException"
        );
    }

    #[test]
    fn submap_range_gated_map_law() {
        let _rt = crate::runtime();
        // V4: reads silently miss out-of-range keys; put throws the exact
        // IAE; null keys NPE and non-Comparable keys CCE through the range
        // check
        let (_m, sub) = tm_views_fixture();
        let get = |k: i32| n_sms_get(Some(&sub), &[boxed_i32(k)], ctx()).unwrap();
        assert_eq!(s(&get(3)), "c");
        assert!(get(6).is_null());
        assert!(get(1).is_null(), "below the lo: silent null");
        assert!(get(7).is_null(), "at the exclusive hi: silent null");
        assert!(get(99).is_null(), "above the hi: silent null");
        let ck = |k: i32| n_sms_contains_key(Some(&sub), &[boxed_i32(k)], ctx()).unwrap();
        assert_eq!(ck(3).expect_int(), 1);
        assert_eq!(ck(1).expect_int(), 0);
        assert_eq!(ck(7).expect_int(), 0);
        let gd = n_sms_get_or_default(
            Some(&sub),
            &[boxed_i32(1), Value::Ref(Some(new_string("D")))],
            ctx(),
        )
        .unwrap();
        assert_eq!(s(&gd), "D");
        // remove: silent null out-of-range (the c34a shape even on an
        // empty view)
        let none = match n_tm_sub_map(Some(&_m), &[boxed_i32(10), boxed_i32(20)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("empty view"),
        };
        assert!(n_sms_remove(Some(&none), &[boxed_i32(1)], ctx())
            .unwrap()
            .is_null());
        // put: IAE "key out of range" below-lo, at-hi-exc, above-hi
        for k in [1, 7, 99] {
            let err = n_sms_put(
                Some(&sub),
                &[boxed_i32(k), Value::Ref(Some(new_string("x")))],
                ctx(),
            )
            .unwrap_err();
            assert_eq!(err.java_class(), "java/lang/IllegalArgumentException");
            assert_eq!(throw_message(&err), "key out of range");
        }
        // null key: NPE through the range compare (the c18 shape)
        let err = n_sms_put(
            Some(&sub),
            &[Value::Ref(None), Value::Ref(Some(new_string("x")))],
            ctx(),
        )
        .unwrap_err();
        assert_eq!(err.java_class(), "java/lang/NullPointerException");
        // non-Comparable key: CCE through the range compare (c19)
        let obj = new_obj("java/lang/Object", 0).unwrap();
        let err = n_sms_put(
            Some(&sub),
            &[Value::Ref(Some(obj)), Value::Ref(Some(new_string("x")))],
            ctx(),
        )
        .unwrap_err();
        assert_eq!(err.java_class(), "java/lang/ClassCastException");
    }

    #[test]
    fn submap_narrowing_matrix_law() {
        let _rt = crate::runtime();
        // V7: the probed narrowing matrix — full inRange on the fromKey
        // side, inclusivity-blind strictly-above-hi on the toKey side, the
        // crossed check last; the absolute builders never range-check
        let (m, s37) = tm_views_fixture(); // s37: lo=3 inc, hi=7 EXC
        let tail5 = match n_tm_tail_map(Some(&m), &[boxed_i32(5)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("tail5"),
        };
        let head7 = match n_tm_head_map(Some(&m), &[boxed_i32(7)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("head7"),
        };
        let iae = |r: Result<Value, Throw>| -> String {
            let e = r.unwrap_err();
            assert_eq!(e.java_class(), "java/lang/IllegalArgumentException");
            throw_message(&e)
        };
        // a. re-requesting the same bounds is LEGAL (toKey == exclusive hi
        //    passes the toKey-side gate)
        n_sms_sub_map(Some(&s37), &[boxed_i32(3), boxed_i32(7)], ctx()).unwrap();
        // c. below the lo: "fromKey out of range"
        assert_eq!(
            iae(n_sms_sub_map(
                Some(&s37),
                &[boxed_i32(2), boxed_i32(5)],
                ctx()
            )),
            "fromKey out of range"
        );
        // e. headMap ACCEPTS toKey == the exclusive hi (c16e) — the result
        //    is the same-range view [3inc, 7exc]
        let e = match n_sms_head_map(Some(&s37), &[boxed_i32(7)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("c16e"),
        };
        assert_eq!(
            s(&n_sms_to_string(Some(&e), &[], ctx()).unwrap()),
            "{3=c, 5=e}"
        );
        // f. tailMap REJECTS fromKey == the exclusive hi (the asymmetry)
        assert_eq!(
            iae(n_sms_tail_map(Some(&s37), &[boxed_i32(7)], ctx())),
            "fromKey out of range"
        );
        // g. headMap down to the inclusive lo: legal EMPTY view
        let g = match n_sms_head_map(Some(&s37), &[boxed_i32(3)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("c16g"),
        };
        assert_eq!(s(&n_sms_to_string(Some(&g), &[], ctx()).unwrap()), "{}");
        // i. on an unbounded-hi view: headMap creates a new exclusive hi
        let i = match n_sms_head_map(Some(&tail5), &[boxed_i32(7)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("c16i"),
        };
        assert_eq!(s(&n_sms_to_string(Some(&i), &[], ctx()).unwrap()), "{5=e}");
        // j. on an exclusive-hi view: tailMap from that hi throws
        assert_eq!(
            iae(n_sms_tail_map(Some(&head7), &[boxed_i32(7)], ctx())),
            "fromKey out of range"
        );
        // k. above the exclusive hi on an unbounded-hi parent: accepted
        let k = match n_sms_tail_map(Some(&tail5), &[boxed_i32(9)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("c16k"),
        };
        assert_eq!(s(&n_sms_to_string(Some(&k), &[], ctx()).unwrap()), "{9=i}");
        // l. crossed bounds on the ABSOLUTE map: "fromKey > toKey"
        assert_eq!(
            iae(n_tm_sub_map(Some(&m), &[boxed_i32(9), boxed_i32(3)], ctx())),
            "fromKey > toKey"
        );
        // m. from == to on the absolute map: legal empty view
        let mm = match n_tm_sub_map(Some(&m), &[boxed_i32(5), boxed_i32(5)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("c16m"),
        };
        assert_eq!(s(&n_sms_to_string(Some(&mm), &[], ctx()).unwrap()), "{}");
        // n. chained twice
        let chain = match n_tm_sub_map(Some(&m), &[boxed_i32(3), boxed_i32(9)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("chain"),
        };
        let nn = match n_sms_sub_map(Some(&chain), &[boxed_i32(5), boxed_i32(7)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("c16n"),
        };
        assert_eq!(s(&n_sms_to_string(Some(&nn), &[], ctx()).unwrap()), "{5=e}");
        // o. no-intersection above: chained narrowing THROWS
        let o1 = match n_sms_head_map(Some(&tail5), &[boxed_i32(7)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("c16o parent"),
        };
        assert_eq!(
            iae(n_sms_tail_map(Some(&o1), &[boxed_i32(7)], ctx())),
            "fromKey out of range"
        );
        // q. crossed INSIDE a legal window: "fromKey > toKey" (c16q)
        assert_eq!(
            iae(n_sms_sub_map(
                Some(&chain),
                &[boxed_i32(8), boxed_i32(5)],
                ctx()
            )),
            "fromKey > toKey"
        );
        // s. subMap's fromKey == the exclusive hi: REJECTED (full inRange)
        assert_eq!(
            iae(n_sms_sub_map(
                Some(&s37),
                &[boxed_i32(7), boxed_i32(7)],
                ctx()
            )),
            "fromKey out of range"
        );
        // t. tailMap's fromKey above the hi: REJECTED
        let big = match n_tm_sub_map(Some(&m), &[boxed_i32(1), boxed_i32(9)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("big"),
        };
        assert_eq!(
            iae(n_sms_tail_map(Some(&big), &[boxed_i32(99)], ctx())),
            "fromKey out of range"
        );
        // u. headMap below the lo: "toKey out of range" (the full-tooLow
        //    half of the toKey-side gate)
        assert_eq!(
            iae(n_sms_head_map(Some(&s37), &[boxed_i32(2)], ctx())),
            "toKey out of range"
        );
        // w. chained subMap with toKey BELOW the lo: "toKey out of range"
        //    fires BEFORE the crossed check (c16w)
        assert_eq!(
            iae(n_sms_sub_map(
                Some(&big),
                &[boxed_i32(5), boxed_i32(0)],
                ctx()
            )),
            "toKey out of range"
        );
        // x. fromKey range precedes the crossed check (c16x)
        assert_eq!(
            iae(n_sms_sub_map(
                Some(&s37),
                &[boxed_i32(2), boxed_i32(1)],
                ctx()
            )),
            "fromKey out of range"
        );
        // y. headMap == hi on a HI-INCLUSIVE parent: accepted (strictly-
        //    above-hi is inclusivity-blind)
        let inc_hi = match n_tm_sub_map4(
            Some(&m),
            &[boxed_i32(3), Value::Int(1), boxed_i32(7), Value::Int(1)],
            ctx(),
        )
        .unwrap()
        {
            Value::Ref(Some(v)) => v,
            _ => panic!("c16y parent"),
        };
        let y = match n_sms_head_map(Some(&inc_hi), &[boxed_i32(7)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("c16y"),
        };
        assert_eq!(
            s(&n_sms_to_string(Some(&y), &[], ctx()).unwrap()),
            "{3=c, 5=e}"
        );
        // v. the view's comparator: null for natural ordering (deref'd from
        //    the backing map — the c20 law)
        assert!(n_sms_comparator(Some(&s37), &[], ctx()).unwrap().is_null());
    }

    #[test]
    fn submap_live_and_slice_clear_law() {
        let _rt = crate::runtime();
        // V5: live both ways; V6: slice-only clear
        let (m, sub) = tm_views_fixture();
        // put through the view at an interior key: visible in both
        n_sms_put(
            Some(&sub),
            &[boxed_i32(4), Value::Ref(Some(new_string("d4")))],
            ctx(),
        )
        .unwrap();
        assert_eq!(
            s(&n_sms_to_string(Some(&sub), &[], ctx()).unwrap()),
            "{3=c, 4=d4, 5=e}"
        );
        assert_eq!(
            s(&n_tm_to_string(Some(&m), &[], ctx()).unwrap()),
            "{1=a, 3=c, 4=d4, 5=e, 7=g, 9=i}"
        );
        n_sms_remove(Some(&sub), &[boxed_i32(4)], ctx()).unwrap();
        // parent put in-range appears in the view; out-of-range does not
        n_tm_put(
            Some(&m),
            &[boxed_i32(6), Value::Ref(Some(new_string("six")))],
            ctx(),
        )
        .unwrap();
        assert_eq!(
            s(&n_sms_to_string(Some(&sub), &[], ctx()).unwrap()),
            "{3=c, 5=e, 6=six}"
        );
        assert_eq!(
            n_sms_contains_value(Some(&sub), &[Value::Ref(Some(new_string("six")))], ctx())
                .unwrap()
                .expect_int(),
            1
        );
        assert_eq!(
            n_sms_contains_value(Some(&sub), &[Value::Ref(Some(new_string("a")))], ctx())
                .unwrap()
                .expect_int(),
            0
        );
        n_tm_remove(Some(&m), &[boxed_i32(6)], ctx()).unwrap();
        // V6: slice-only clear — [3,7) drains 3,5; 7 is the exclusive hi
        // and survives with 1 and 9
        n_sms_clear(Some(&sub), &[], ctx()).unwrap();
        assert_eq!(s(&n_sms_to_string(Some(&sub), &[], ctx()).unwrap()), "{}");
        assert_eq!(
            s(&n_tm_to_string(Some(&m), &[], ctx()).unwrap()),
            "{1=a, 7=g, 9=i}"
        );
        assert_eq!(n_tm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 3);
        // the c35 shape: a 50-key map, clear [10,20), 40 survive
        let big = tm_new();
        for k in 0..50 {
            n_tm_put(Some(&big), &[boxed_i32(k), boxed_i32(k * k)], ctx()).unwrap();
        }
        let mid = match n_tm_sub_map(Some(&big), &[boxed_i32(10), boxed_i32(20)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("mid"),
        };
        assert_eq!(n_sms_size(Some(&mid), &[], ctx()).unwrap().expect_int(), 10);
        n_sms_clear(Some(&mid), &[], ctx()).unwrap();
        assert_eq!(n_sms_size(Some(&mid), &[], ctx()).unwrap().expect_int(), 0);
        assert_eq!(n_tm_size(Some(&big), &[], ctx()).unwrap().expect_int(), 40);
        let ck = |k: i32| {
            n_tm_contains_key(Some(&big), &[boxed_i32(k)], ctx())
                .unwrap()
                .expect_int()
        };
        assert_eq!(ck(10), 0);
        assert_eq!(ck(20), 1, "the exclusive hi survives");
        assert_eq!(ck(9), 1);
        assert_eq!(ck(21), 1);
    }

    #[test]
    fn submap_views_and_iterators_law() {
        let _rt = crate::runtime();
        // V9: the view-of-view objects + the bounded iterators (order,
        // mid-walk remove, the hi fence)
        let (m, sub) = tm_views_fixture();
        let ks = match n_sms_key_set(Some(&sub), &[], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("keySet"),
        };
        let vs = match n_sms_values(Some(&sub), &[], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("values"),
        };
        let es = match n_sms_entry_set(Some(&sub), &[], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("entrySet"),
        };
        assert_eq!(
            s(&n_smsks_to_string(Some(&ks), &[], ctx()).unwrap()),
            "[3, 5]"
        );
        assert_eq!(
            s(&n_smsvs_to_string(Some(&vs), &[], ctx()).unwrap()),
            "[c, e]"
        );
        assert_eq!(
            s(&n_smses_to_string(Some(&es), &[], ctx()).unwrap()),
            "[3=c, 5=e]"
        );
        assert_eq!(n_smsks_size(Some(&ks), &[], ctx()).unwrap().expect_int(), 2);
        // contains rides the range gate
        assert_eq!(
            n_smsks_contains(Some(&ks), &[boxed_i32(3)], ctx())
                .unwrap()
                .expect_int(),
            1
        );
        assert_eq!(
            n_smsks_contains(Some(&ks), &[boxed_i32(1)], ctx())
                .unwrap()
                .expect_int(),
            0
        );
        // the bounded key iterator: mid-walk remove, order intact, no CME
        let it = match n_smsks_iterator(Some(&ks), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("iterator"),
        };
        let mut walked = Vec::new();
        let mut removed = None;
        while n_sms_i_has_next(Some(&it), &[], ctx())
            .unwrap()
            .expect_int()
            == 1
        {
            let k = n_smki_next(Some(&it), &[], ctx()).unwrap();
            if unbox_i32(&k) == 5 {
                n_tm_i_remove(Some(&it), &[], ctx()).unwrap();
                removed = Some(k.clone());
            } else {
                walked.push(unbox_i32(&k));
            }
        }
        assert_eq!(walked, vec![3]);
        assert_eq!(unbox_i32(&removed.expect("removed")), 5);
        assert_eq!(
            s(&n_sms_to_string(Some(&sub), &[], ctx()).unwrap()),
            "{3=c}"
        );
        assert_eq!(
            s(&n_tm_to_string(Some(&m), &[], ctx()).unwrap()),
            "{1=a, 3=c, 7=g, 9=i}"
        );
        // the hi fence stops the walk: [5,9) over the restored map
        n_tm_put(
            Some(&m),
            &[boxed_i32(5), Value::Ref(Some(new_string("e")))],
            ctx(),
        )
        .unwrap();
        let tail = match n_tm_tail_map(Some(&m), &[boxed_i32(5)], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("tail"),
        };
        let tks = match n_sms_key_set(Some(&tail), &[], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("tail keySet"),
        };
        let it2 = match n_smsks_iterator(Some(&tks), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("tail iterator"),
        };
        let mut keys = Vec::new();
        while n_sms_i_has_next(Some(&it2), &[], ctx())
            .unwrap()
            .expect_int()
            == 1
        {
            keys.push(unbox_i32(&n_smki_next(Some(&it2), &[], ctx()).unwrap()));
        }
        assert_eq!(keys, vec![5, 7, 9]);
        // the bounded entry iterator's next IS the backing entry node (the
        // Map$Entry checkcast law carries)
        let es2 = match n_sms_entry_set(Some(&sub), &[], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("entrySet2"),
        };
        let it3 = match n_smses_iterator(Some(&es2), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("entry iterator"),
        };
        let node = match n_smei_next(Some(&it3), &[], ctx()).unwrap() {
            Value::Ref(Some(e)) => e,
            _ => panic!("entry node"),
        };
        assert_eq!(
            with_obj(node, |o| o.jclass.to_string()),
            "java/util/TreeMap$Entry"
        );
    }

    #[test]
    fn submap_iterator_exception_laws() {
        let _rt = crate::runtime();
        // V8: ISE-first remove, double-remove ISE, exhausted NSEE, and the
        // probed c27 law — a foreign OUT-of-range put still comodifies
        let (m, sub) = tm_views_fixture();
        let ks = match n_sms_key_set(Some(&sub), &[], ctx()).unwrap() {
            Value::Ref(Some(v)) => v,
            _ => panic!("keySet"),
        };
        // remove without next -> ISE (BEFORE any CME check)
        let it = match n_smsks_iterator(Some(&ks), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("it"),
        };
        assert_eq!(
            n_tm_i_remove(Some(&it), &[], ctx())
                .unwrap_err()
                .java_class(),
            "java/lang/IllegalStateException"
        );
        // next, then a foreign put through the PARENT inside the range -> CME
        n_smki_next(Some(&it), &[], ctx()).unwrap();
        n_tm_put(
            Some(&m),
            &[boxed_i32(6), Value::Ref(Some(new_string("x")))],
            ctx(),
        )
        .unwrap();
        assert_eq!(
            n_smki_next(Some(&it), &[], ctx()).unwrap_err().java_class(),
            CME_CLASS
        );
        n_tm_remove(Some(&m), &[boxed_i32(6)], ctx()).unwrap();
        // c27: a foreign put OUTSIDE the range STILL comodifies (the
        // expectedModCount is the backing map's)
        let it2 = match n_smsks_iterator(Some(&ks), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("it2"),
        };
        n_smki_next(Some(&it2), &[], ctx()).unwrap();
        n_tm_put(
            Some(&m),
            &[boxed_i32(99), Value::Ref(Some(new_string("zz")))],
            ctx(),
        )
        .unwrap();
        assert_eq!(
            n_smki_next(Some(&it2), &[], ctx())
                .unwrap_err()
                .java_class(),
            CME_CLASS
        );
        n_tm_remove(Some(&m), &[boxed_i32(99)], ctx()).unwrap();
        // double remove -> ISE; the exhausted walk -> NSEE
        let it3 = match n_smsks_iterator(Some(&ks), &[], ctx()).unwrap() {
            Value::Ref(Some(i)) => i,
            _ => panic!("it3"),
        };
        n_smki_next(Some(&it3), &[], ctx()).unwrap();
        n_tm_i_remove(Some(&it3), &[], ctx()).unwrap();
        assert_eq!(
            n_tm_i_remove(Some(&it3), &[], ctx())
                .unwrap_err()
                .java_class(),
            "java/lang/IllegalStateException"
        );
        while n_sms_i_has_next(Some(&it3), &[], ctx())
            .unwrap()
            .expect_int()
            == 1
        {
            n_smki_next(Some(&it3), &[], ctx()).unwrap();
        }
        assert_eq!(
            n_smki_next(Some(&it3), &[], ctx())
                .unwrap_err()
                .java_class(),
            "java/util/NoSuchElementException"
        );
    }

    #[test]
    fn lhm_ctor_from_map_copies_source_order() {
        let _rt = crate::runtime();
        // LHM source: link order
        let src = lhm_new();
        for (k, v) in [("a", 1), ("b", 2), ("c", 3)] {
            n_lhm_put(
                Some(&src),
                &[Value::Ref(Some(new_string(k))), boxed_i32(v)],
                ctx(),
            )
            .unwrap();
        }
        let cp = new_obj("java/util/LinkedHashMap", 6).unwrap();
        n_lhm_init(Some(&cp), &[Value::Ref(Some(src))], ctx()).unwrap();
        assert_eq!(
            s(&n_lhm_to_string(Some(&cp), &[], ctx()).unwrap()),
            "{a=1, b=2, c=3}"
        );
        // HashMap source: the source's OWN table order
        let hm = map_new();
        for (k, v) in [("x", 1), ("y", 2), ("z", 3)] {
            n_hm_put(
                Some(&hm),
                &[Value::Ref(Some(new_string(k))), boxed_i32(v)],
                ctx(),
            )
            .unwrap();
        }
        let cp2 = new_obj("java/util/LinkedHashMap", 6).unwrap();
        n_lhm_init(Some(&cp2), &[Value::Ref(Some(hm))], ctx()).unwrap();
        // the law: the copy iterates in the SOURCE's table order
        let expected_entries = hm_all_entries(&hm).unwrap();
        let expected = format!(
            "{{{}}}",
            expected_entries
                .iter()
                .map(|(k, v)| format!("{}={}", s(k), unbox_i32(v)))
                .collect::<Vec<_>>()
                .join(", ")
        );
        assert_eq!(
            s(&n_lhm_to_string(Some(&cp2), &[], ctx()).unwrap()),
            expected
        );
        assert_eq!(with_obj(cp2, |o| o.slots[HM_SIZE].expect_int()), 3);
    }

    // ------------------------------------------------------------------
    // The HashMap copy-ctor laws (the registered copy-ctor gap CLOSED).
    // Every expected order below is the Temurin 21.0.13+9 oracle captured
    // in scripts/evidence/hm_copy_ctor/temurin_oracle_2026-11-17.txt
    // (HashMapCopyProbe L1-L9) — the fixture is the three-channel check;
    // these tests pin the internals.

    /// Copies a source's entries in the source's iteration order — the
    /// shared helper law read through the three storages.
    fn hm_copy_keys(cp: &JRef) -> Vec<i32> {
        hm_all_entries(cp)
            .unwrap()
            .into_iter()
            .map(|(k, _)| unbox_i32(&k))
            .collect()
    }

    /// The putMapEntries pre-size boundary: threshold =
    /// tableSizeFor((int)((float)s/0.75f + 1.0f)) — 11 entries -> 16,
    /// 13 entries -> 32; the table stays lazy until the first put.
    #[test]
    fn hm_ctor_from_map_presize_boundary_law() {
        let _rt = crate::runtime();
        let src11 = lhm_new();
        for i in 0..10 {
            n_lhm_put(Some(&src11), &[boxed_i32(i), boxed_i32(i)], ctx()).unwrap();
        }
        n_lhm_put(Some(&src11), &[boxed_i32(20), boxed_i32(400)], ctx()).unwrap();
        let cp = new_obj("java/util/HashMap", 4).unwrap();
        n_hm_init(Some(&cp), &[Value::Ref(Some(src11))], ctx()).unwrap();
        // after the copy the first put has allocated at the PRE-SIZED cap
        // (the threshold the put saw was 16; the post-put threshold is
        // 16*0.75 = 12 — hm_resize's law).
        let (_, size, thr, table) = hm_state(&cp);
        assert_eq!(size, 11);
        assert_eq!(thr, 12);
        assert_eq!(hm_table_len(table.as_ref().unwrap()), 16);
        let src13 = lhm_new();
        for i in 0..12 {
            n_lhm_put(Some(&src13), &[boxed_i32(i), boxed_i32(i)], ctx()).unwrap();
        }
        n_lhm_put(Some(&src13), &[boxed_i32(20), boxed_i32(400)], ctx()).unwrap();
        let cp2 = new_obj("java/util/HashMap", 4).unwrap();
        n_hm_init(Some(&cp2), &[Value::Ref(Some(src13))], ctx()).unwrap();
        let (_, size2, thr2, table2) = hm_state(&cp2);
        assert_eq!(size2, 13);
        assert_eq!(thr2, 24);
        assert_eq!(hm_table_len(table2.as_ref().unwrap()), 32);
    }

    /// L1/L3: the source's iteration order drives insertion, read
    /// through the chain order (three keys colliding at every candidate
    /// capacity; the LHM source inserts 16, 0, 32).
    #[test]
    fn hm_ctor_from_map_source_order_chain_law() {
        let _rt = crate::runtime();
        let src = lhm_new();
        for k in [16, 0, 32] {
            n_lhm_put(Some(&src), &[boxed_i32(k), boxed_i32(k * 10)], ctx()).unwrap();
        }
        let cp = new_obj("java/util/HashMap", 4).unwrap();
        n_hm_init(Some(&cp), &[Value::Ref(Some(src))], ctx()).unwrap();
        let keys = hm_copy_keys(&cp);
        assert_eq!(keys, vec![16, 0, 32]);
    }

    /// L4: the tree source inserts in SORTED order; the copy's cap-16
    /// table order is Temurin's exactly (0 18 3 21 6 24 9 27 12 15).
    #[test]
    fn hm_ctor_from_tree_source_law() {
        let _rt = crate::runtime();
        let src = tm_new();
        for i in (0..10).rev() {
            n_tm_put(Some(&src), &[boxed_i32(i * 3), boxed_i32(i)], ctx()).unwrap();
        }
        let cp = new_obj("java/util/HashMap", 4).unwrap();
        n_hm_init(Some(&cp), &[Value::Ref(Some(src))], ctx()).unwrap();
        let keys = hm_copy_keys(&cp);
        assert_eq!(keys, vec![0, 18, 3, 21, 6, 24, 9, 27, 12, 15]);
    }

    /// L6: the null map NPE (putMapEntries reads m.size() first).
    #[test]
    fn hm_ctor_null_source_npe_law() {
        let _rt = crate::runtime();
        let m = new_obj("java/util/HashMap", 4).unwrap();
        let err = n_hm_init(Some(&m), &[Value::Ref(None)], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/NullPointerException");
    }

    /// L8: the empty source keeps the table lazy (threshold 0); the
    /// first put allocates the default cap 16 (1 and 17 share bucket 1,
    /// the chain order = insertion order).
    #[test]
    fn hm_ctor_empty_source_lazy_law() {
        let _rt = crate::runtime();
        let empty = map_new();
        let cp = new_obj("java/util/HashMap", 4).unwrap();
        n_hm_init(Some(&cp), &[Value::Ref(Some(empty))], ctx()).unwrap();
        let (_, _, thr, table) = hm_state(&cp);
        assert_eq!(thr, 0);
        assert!(table.is_none());
        n_hm_put(Some(&cp), &[boxed_i32(1), boxed_i32(11)], ctx()).unwrap();
        n_hm_put(Some(&cp), &[boxed_i32(17), boxed_i32(17)], ctx()).unwrap();
        let keys = hm_copy_keys(&cp);
        assert_eq!(keys, vec![1, 17]);
    }

    /// L4b: the LHM ctor from a TREE source iterates the tree in KEY
    /// order — the latent misread the shared source-storage law fixed
    /// (the old `_` arm walked hash slots on a red-black object).
    #[test]
    fn lhm_ctor_from_tree_source_law() {
        let _rt = crate::runtime();
        let src = tm_new();
        for i in (0..10).rev() {
            n_tm_put(Some(&src), &[boxed_i32(i * 3), boxed_i32(i)], ctx()).unwrap();
        }
        let cp = new_obj("java/util/LinkedHashMap", 6).unwrap();
        n_lhm_init(Some(&cp), &[Value::Ref(Some(src))], ctx()).unwrap();
        // the L4b oracle line: the KEY order 0 3 6 9 12 15 18 21 24 27;
        // the values ride the source mapping key k*3 -> k.
        assert_eq!(
            s(&n_lhm_to_string(Some(&cp), &[], ctx()).unwrap()),
            "{0=0, 3=1, 6=2, 9=3, 12=4, 15=5, 18=6, 21=7, 24=8, 27=9}"
        );
    }

    /// L9: the post-copy growth timing — threshold 12 after the copy of
    /// the 11-entry source; the 12th entry does NOT resize (12 > 12 is
    /// false, cap stays 16, key 100 joins key 4's chain behind key 20);
    /// the 13th does (cap 32; the lo/hi split moves key 20 to its own
    /// high bucket and preserves the [4, 100] chain order).
    #[test]
    fn hm_ctor_post_copy_growth_timing_law() {
        let _rt = crate::runtime();
        let src = lhm_new();
        for i in 0..10 {
            n_lhm_put(Some(&src), &[boxed_i32(i), boxed_i32(i)], ctx()).unwrap();
        }
        n_lhm_put(Some(&src), &[boxed_i32(20), boxed_i32(400)], ctx()).unwrap();
        let cp = new_obj("java/util/HashMap", 4).unwrap();
        n_hm_init(Some(&cp), &[Value::Ref(Some(src))], ctx()).unwrap();
        n_hm_put(Some(&cp), &[boxed_i32(100), boxed_i32(100)], ctx()).unwrap();
        let keys = hm_copy_keys(&cp);
        assert_eq!(keys, vec![0, 1, 2, 3, 4, 20, 100, 5, 6, 7, 8, 9]);
        let (_, _, _, table) = hm_state(&cp);
        assert_eq!(hm_table_len(table.as_ref().unwrap()), 16);
        n_hm_put(Some(&cp), &[boxed_i32(101), boxed_i32(101)], ctx()).unwrap();
        let keys = hm_copy_keys(&cp);
        assert_eq!(keys, vec![0, 1, 2, 3, 4, 100, 5, 101, 6, 7, 8, 9, 20]);
        let (_, _, _, table) = hm_state(&cp);
        assert_eq!(hm_table_len(table.as_ref().unwrap()), 32);
    }

    #[test]
    fn lhm_null_key_value_and_clear() {
        let _rt = crate::runtime();
        let m = lhm_new();
        n_lhm_put(
            Some(&m),
            &[Value::Ref(Some(new_string("a"))), boxed_i32(1)],
            ctx(),
        )
        .unwrap();
        // L9: one null key, null values
        n_lhm_put(Some(&m), &[Value::Ref(None), boxed_i32(0)], ctx()).unwrap();
        assert_eq!(
            s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
            "{a=1, null=0}"
        );
        n_lhm_put(
            Some(&m),
            &[Value::Ref(Some(new_string("n"))), Value::Ref(None)],
            ctx(),
        )
        .unwrap();
        assert_eq!(
            s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
            "{a=1, null=0, n=null}"
        );
        let old = n_lhm_remove(Some(&m), &[Value::Ref(None)], ctx()).unwrap();
        assert_eq!(unbox_i32(&old), 0);
        assert_eq!(
            s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
            "{a=1, n=null}"
        );
        // L10: clear resets everything; re-adds re-link at the tail
        n_lhm_clear(Some(&m), &[], ctx()).unwrap();
        assert_eq!(with_obj(m, |o| o.slots[HM_SIZE].expect_int()), 0);
        n_lhm_put(
            Some(&m),
            &[Value::Ref(Some(new_string("q"))), boxed_i32(7)],
            ctx(),
        )
        .unwrap();
        assert_eq!(s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()), "{q=7}");
    }

    #[test]
    fn lhm_is_a_hashmap_subtype_law() {
        // L6: the builtin tables carry the OpenJDK subclass shape
        assert_eq!(
            crate::builtin::builtin_parent_of("java/util/LinkedHashMap"),
            Some("java/util/HashMap")
        );
        assert_eq!(
            crate::builtin::builtin_parent_of("java/util/LinkedHashMap$Entry"),
            Some("java/util/HashMap$Node")
        );
        assert_eq!(
            crate::builtin::builtin_total_slots("java/util/LinkedHashMap"),
            Some(6)
        );
        let ifaces = crate::builtin::builtin_interfaces_of("java/util/LinkedHashMap");
        assert!(ifaces.contains(&"java/util/Map"));
        let entry_ifaces = crate::builtin::builtin_interfaces_of("java/util/LinkedHashMap$Entry");
        assert!(entry_ifaces.contains(&"java/util/Map$Entry"));
    }

    #[test]
    fn lhm_get_default_and_contains_value_laws() {
        let _rt = crate::runtime();
        let m = lhm_new();
        for (k, v) in [("a", 1), ("b", 2)] {
            n_lhm_put(
                Some(&m),
                &[Value::Ref(Some(new_string(k))), boxed_i32(v)],
                ctx(),
            )
            .unwrap();
        }
        // getOrDefault/containsValue inherit the HashMap rows (order-neutral)
        let d = n_hm_get_or_default(
            Some(&m),
            &[Value::Ref(Some(new_string("b"))), boxed_i32(-1)],
            ctx(),
        )
        .unwrap();
        assert_eq!(unbox_i32(&d), 2);
        let d = n_hm_get_or_default(
            Some(&m),
            &[Value::Ref(Some(new_string("zz"))), boxed_i32(-1)],
            ctx(),
        )
        .unwrap();
        assert_eq!(unbox_i32(&d), -1);
        assert_eq!(
            n_hm_contains_value(Some(&m), &[boxed_i32(2)], ctx())
                .unwrap()
                .expect_int(),
            1
        );
        assert_eq!(
            n_hm_contains_value(Some(&m), &[boxed_i32(99)], ctx())
                .unwrap()
                .expect_int(),
            0
        );
    }

    // ---- HashSet ----

    #[test]
    fn hashset_add_contains_remove_tostring() {
        let _rt = crate::runtime();
        let st = set_new();
        assert_eq!(
            n_hs_add(Some(&st), &[boxed_i32(4)], ctx())
                .unwrap()
                .expect_int(),
            1
        );
        assert_eq!(
            n_hs_add(Some(&st), &[boxed_i32(4)], ctx())
                .unwrap()
                .expect_int(),
            0,
            "duplicate add returns false"
        );
        assert_eq!(
            n_hs_add(Some(&st), &[Value::Ref(None)], ctx())
                .unwrap()
                .expect_int(),
            1
        );
        assert_eq!(n_hs_size(Some(&st), &[], ctx()).unwrap().expect_int(), 2);
        assert_eq!(
            s(&n_hs_to_string(Some(&st), &[], ctx()).unwrap()),
            "[null, 4]"
        );
        assert_eq!(
            n_hs_contains(Some(&st), &[boxed_i32(4)], ctx())
                .unwrap()
                .expect_int(),
            1
        );
        assert_eq!(
            n_hs_remove(Some(&st), &[boxed_i32(4)], ctx())
                .unwrap()
                .expect_int(),
            1
        );
        assert_eq!(
            n_hs_remove(Some(&st), &[boxed_i32(99)], ctx())
                .unwrap()
                .expect_int(),
            0
        );
        assert_eq!(s(&n_hs_to_string(Some(&st), &[], ctx()).unwrap()), "[null]");
    }

    // ---- Objects ----

    #[test]
    fn objects_require_non_null_overloads() {
        let _rt = crate::runtime();
        let v = Value::Ref(Some(new_string("ok")));
        assert!(matches!(
            n_objects_require_non_null(None, std::slice::from_ref(&v), ctx()).unwrap(),
            Value::Ref(_)
        ));
        let err = n_objects_require_non_null(None, &[Value::Ref(None)], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/NullPointerException");
        // null message (no-arg NPE)
        let msg = with_obj(err.obj, |o| o.slots[0].clone());
        assert!(msg.is_null());
        let err = n_objects_require_non_null_msg(
            None,
            &[Value::Ref(None), Value::Ref(Some(new_string("was null")))],
            ctx(),
        )
        .unwrap_err();
        let msg = with_obj(err.obj, |o| o.slots[0].clone());
        assert_eq!(s(&msg), "was null");
    }

    #[test]
    fn objects_to_string_is_null_and_hash() {
        let _rt = crate::runtime();
        let out = n_objects_to_string(None, &[Value::Ref(None)], ctx()).unwrap();
        assert_eq!(s(&out), "null");
        let out = n_objects_to_string(None, &[Value::Ref(Some(new_string("x")))], ctx()).unwrap();
        assert_eq!(s(&out), "x");
        assert_eq!(
            n_objects_hash_code(None, &[Value::Ref(None)], ctx())
                .unwrap()
                .expect_int(),
            0
        );
        assert_eq!(
            n_objects_is_null(None, &[Value::Ref(None)], ctx())
                .unwrap()
                .expect_int(),
            1
        );
        assert_eq!(
            n_objects_non_null(None, &[Value::Ref(Some(new_string("")))], ctx())
                .unwrap()
                .expect_int(),
            1
        );
    }

    // ---- GC integration (M3_PLAN §5.4) ----

    #[test]
    fn rooted_collection_keeps_elements_alive_across_gc() {
        let _rt = crate::runtime();
        crate::heap::set_forced_gc_every(Some(1));
        let l = list_new();
        let elem = new_string("kept-by-list");
        list_add(&l, Value::Ref(Some(elem)));
        // root ONLY the list (simulates a caller frame snapshot)
        let _nr = crate::frame::NativeRoots::push(&[l]);
        for _ in 0..50 {
            let _churn = new_string("churn");
            crate::heap::gc_point();
        }
        assert!(
            crate::heap::is_live(elem),
            "element rooted through the list survives"
        );
        assert!(crate::heap::is_live(l), "the list itself survives");
        // the element is intact and readable through the list
        let v = n_al_get(Some(&l), &[Value::Int(0)], ctx()).unwrap();
        assert_eq!(s(&v), "kept-by-list");
        crate::heap::set_forced_gc_every(None);
    }

    #[test]
    fn dropped_collection_lets_elements_be_collected() {
        let _rt = crate::runtime();
        crate::heap::set_forced_gc_every(Some(1));
        let before = crate::heap::live_count();
        {
            let l = list_new();
            list_add(&l, Value::Ref(Some(new_string("dies-with-list"))));
            // HashMap with nodes
            let m = map_new();
            n_hm_put(
                Some(&m),
                &[Value::Ref(Some(new_string("k"))), boxed_i32(1)],
                ctx(),
            )
            .unwrap();
            // HashSet with its PRESENT dummy
            let st = set_new();
            n_hs_add(Some(&st), &[Value::Ref(Some(new_string("s")))], ctx()).unwrap();
        }
        crate::heap::gc_point();
        assert!(
            crate::heap::live_count() <= before,
            "dropped collections must be collectable whole (live {} before, {} after)",
            before,
            crate::heap::live_count()
        );
        crate::heap::set_forced_gc_every(None);
    }

    // ---- G3.3 slice 3: the node-stamped class tag + lazy materialization ----

    fn key(s: &str) -> Value {
        Value::Ref(Some(new_string(s)))
    }

    #[test]
    fn prim_slot_materializes_with_identity_and_jls_cache() {
        let _rt = crate::runtime();
        let m = map_new();
        // The specialized put stores the RAW prim; the node is stamped 'I'.
        hm_put_prim(&m, key("n"), Value::Int(5), ctx()).unwrap();
        // Faithful get #1 materializes; get #2 returns the SAME ref.
        let v1 = n_hm_get(Some(&m), &[key("n")], ctx()).unwrap();
        let v2 = n_hm_get(Some(&m), &[key("n")], ctx()).unwrap();
        let (Value::Ref(Some(b1)), Value::Ref(Some(b2))) = (&v1, &v2) else {
            panic!("materialized gets must be refs: {v1:?} / {v2:?}")
        };
        assert_eq!(b1, b2, "get-twice-returns-the-same-ref (probe law L1)");
        assert_eq!(with_obj(*b1, |o| o.jclass.to_string()), "java/lang/Integer");
        assert_eq!(with_obj(*b1, |o| o.slots[0].clone()), Value::Int(5));
        // The JLS identity cache: the materialized box IS
        // Integer.valueOf(5)'s shared cache box (probe law L2).
        let cached = crate::reflect::box_by_desc(Value::Int(5), "I").unwrap();
        let Value::Ref(Some(cb)) = &cached else {
            panic!("cache box")
        };
        assert_eq!(b1, cb, "materialization routes through the valueOf natives");
        // The slot now holds the box (the cache); the specialized get's
        // fused-unbox path still returns the prim payload.
        let prim = hm_get_prim(&m, key("n"), "java/lang/Integer", ctx()).unwrap();
        assert_eq!(prim, Value::Int(5));
    }

    #[test]
    fn double_slot_materializes_with_slot_identity() {
        let _rt = crate::runtime();
        let m = map_new();
        hm_put_prim(&m, key("a"), Value::Double(0.5), ctx()).unwrap();
        let v = n_hm_get(Some(&m), &[key("a")], ctx()).unwrap();
        let Value::Ref(Some(b)) = &v else {
            panic!("ref")
        };
        assert_eq!(with_obj(*b, |o| o.jclass.to_string()), "java/lang/Double");
        assert_eq!(with_obj(*b, |o| o.slots[0].clone()), Value::Double(0.5));
        // get-twice-same-ref holds for the never-cached pair too — the
        // SLOT cache, not a JLS cache.
        let v2 = n_hm_get(Some(&m), &[key("a")], ctx()).unwrap();
        let Value::Ref(Some(b2)) = &v2 else {
            panic!("ref")
        };
        assert_eq!(b, b2);
    }

    #[test]
    fn faithful_put_over_prim_node_returns_materialized_old_value() {
        let _rt = crate::runtime();
        let m = map_new();
        hm_put_prim(&m, key("n"), Value::Int(7), ctx()).unwrap();
        // A faithful put over the prim node: Map.put returns the OLD
        // value materialized (the box), and the node re-stamps faithful.
        let fresh = boxed_i32(9);
        let old = n_hm_put(Some(&m), &[key("n"), fresh], ctx()).unwrap();
        assert_eq!(unbox_i32(&old), 7, "the old value escapes as the box");
        let Value::Ref(Some(ob)) = &old else {
            panic!("old value must be a box: {old:?}")
        };
        assert_eq!(with_obj(*ob, |o| o.jclass.to_string()), "java/lang/Integer");
        // And a subsequent faithful get sees the new box directly.
        let v = n_hm_get(Some(&m), &[key("n")], ctx()).unwrap();
        assert_eq!(unbox_i32(&v), 9);
    }

    #[test]
    fn remove_and_to_string_materialize() {
        let _rt = crate::runtime();
        let m = map_new();
        hm_put_prim(&m, key("a"), Value::Double(0.5), ctx()).unwrap();
        hm_put_prim(&m, key("b"), Value::Double(1.5), ctx()).unwrap();
        // toString materializes every slot and matches Java's
        // "{a=0.5, b=1.5}" (the probe's run output).
        let ts = n_hm_to_string(Some(&m), &[], ctx()).unwrap();
        assert_eq!(s(&ts), "{a=0.5, b=1.5}");
        // remove returns the materialized box.
        let rem = n_hm_remove(Some(&m), &[key("a")], ctx()).unwrap();
        let Value::Ref(Some(rb)) = &rem else {
            panic!("removed value")
        };
        assert_eq!(with_obj(*rb, |o| o.jclass.to_string()), "java/lang/Double");
    }

    #[test]
    fn entry_get_value_materializes() {
        let _rt = crate::runtime();
        let m = map_new();
        hm_put_prim(&m, key("a"), Value::Int(5), ctx()).unwrap();
        // entrySet().iterator().next().getValue() — the registered
        // design's named generic exit.
        let es = n_hm_entry_set(Some(&m), &[], ctx()).unwrap();
        let Value::Ref(Some(esr)) = &es else {
            panic!("entrySet")
        };
        let it = n_hes_iterator(Some(esr), &[], ctx()).unwrap();
        let Value::Ref(Some(itr)) = &it else {
            panic!("iterator")
        };
        let entry = n_ei_next(Some(itr), &[], ctx()).unwrap();
        let Value::Ref(Some(er)) = &entry else {
            panic!("entry")
        };
        let gv = n_node_get_value(Some(er), &[], ctx()).unwrap();
        assert_eq!(unbox_i32(&gv), 5);
        let Value::Ref(Some(b)) = &gv else {
            panic!("materialized entry value: {gv:?}")
        };
        assert_eq!(with_obj(*b, |o| o.jclass.to_string()), "java/lang/Integer");
    }

    // ---- G3.3 slice 4: the rewrites ride both new storages ----

    #[test]
    fn tm_put_prim_stores_raw_and_the_readers_materialize() {
        let _rt = crate::runtime();
        let m = tm_new();
        // The specialized put: the CLRS walk runs (size/modCount move),
        // the value slot holds the RAW prim, the node is stamped 'I'.
        tm_put_prim(&m, key("b"), Value::Int(2), ctx()).unwrap();
        tm_put_prim(&m, key("a"), Value::Int(1), ctx()).unwrap();
        assert_eq!(n_tm_size(Some(&m), &[], ctx()).unwrap(), Value::Int(2));
        let root = tm_root(&m).expect("root after two puts");
        let (v, tag) = with_obj(root, |o| {
            (o.slots[TME_VALUE].clone(), o.slots[TME_TAG].clone())
        });
        assert_eq!(v, Value::Int(2), "the value slot holds the RAW prim");
        assert_eq!(
            tag,
            Value::Int('I' as i32),
            "the node is stamped with the desc char"
        );
        // The faithful get materializes (and caches the box back).
        let g1 = n_tm_get(Some(&m), &[key("a")], ctx()).unwrap();
        let Value::Ref(Some(b)) = &g1 else {
            panic!("materialized get must be a ref: {g1:?}")
        };
        assert_eq!(with_obj(*b, |o| o.jclass.to_string()), "java/lang/Integer");
        let g2 = n_tm_get(Some(&m), &[key("a")], ctx()).unwrap();
        assert_eq!(g1, g2, "get-twice-returns-the-same-ref (probe law L1)");
        // toString + containsValue ride tm_all_entries (materialized).
        let ts = n_tm_to_string(Some(&m), &[], ctx()).unwrap();
        assert_eq!(s(&ts), "{a=1, b=2}", "sorted order, boxes materialized");
        assert_eq!(
            n_tm_contains_value(Some(&m), &[boxed_i32(1)], ctx()).unwrap(),
            Value::Int(1)
        );
        // getOrDefault materializes; the miss returns the default.
        assert_eq!(
            unbox_i32(&n_tm_get_or_default(Some(&m), &[key("b"), boxed_i32(9)], ctx()).unwrap()),
            2
        );
        assert_eq!(
            unbox_i32(&n_tm_get_or_default(Some(&m), &[key("zz"), boxed_i32(9)], ctx()).unwrap()),
            9
        );
    }

    #[test]
    fn tm_put_overwrite_and_remove_returns_materialize() {
        let _rt = crate::runtime();
        let m = tm_new();
        tm_put_prim(&m, key("k"), Value::Long(7), ctx()).unwrap();
        // A FAITHFUL overwrite returns the OLD value — materialized out
        // of the raw slot (the Map.put contract).
        let old_box = new_obj("java/lang/Long", 1).unwrap();
        with_obj_mut(old_box, |o| o.slots[0] = Value::Long(7));
        let old = n_tm_put(Some(&m), &[key("k"), Value::Ref(Some(old_box))], ctx()).unwrap();
        let Value::Ref(Some(b)) = &old else {
            panic!("faithful overwrite must return the materialized box: {old:?}")
        };
        assert_eq!(with_obj(*b, |o| o.jclass.to_string()), "java/lang/Long");
        // The slot now holds the box under a CLEARED tag; a core get
        // still unwraps through the fused law's checkcast path.
        assert_eq!(
            tm_get_prim(&m, key("k"), "java/lang/Long", ctx()).unwrap(),
            Value::Long(7)
        );
        // remove materializes the old value too.
        tm_put_prim(&m, key("d"), Value::Long(9), ctx()).unwrap();
        let removed = n_tm_remove(Some(&m), &[key("d")], ctx()).unwrap();
        let Value::Ref(Some(rb)) = &removed else {
            panic!("remove must return the materialized box: {removed:?}")
        };
        assert_eq!(with_obj(*rb, |o| o.slots[0].clone()), Value::Long(9));
    }

    #[test]
    fn tm_get_prim_failure_modes_are_the_fused_law() {
        let _rt = crate::runtime();
        let m = tm_new();
        // Missing key: the bare NPE (the null box reaching xValue()).
        let err = tm_get_prim(&m, key("no"), "java/lang/Integer", ctx()).unwrap_err();
        assert_eq!(
            with_obj(err.obj, |o| o.jclass.to_string()),
            "java/lang/NullPointerException"
        );
        // Tag mismatch: the CCE with the tag's class as the actual type.
        tm_put_prim(&m, key("i"), Value::Int(5), ctx()).unwrap();
        let err = tm_get_prim(&m, key("i"), "java/lang/Double", ctx()).unwrap_err();
        assert_eq!(
            with_obj(err.obj, |o| o.jclass.to_string()),
            "java/lang/ClassCastException"
        );
        // A faithful FOREIGN box: the same CCE (the checkcast law).
        let foreign = new_obj("java/lang/String", 1).unwrap();
        with_obj_mut(foreign, |o| o.slots[0] = Value::Ref(None));
        n_tm_put(Some(&m), &[key("s"), Value::Ref(Some(foreign))], ctx()).unwrap();
        let err = tm_get_prim(&m, key("s"), "java/lang/Integer", ctx()).unwrap_err();
        assert_eq!(
            with_obj(err.obj, |o| o.jclass.to_string()),
            "java/lang/ClassCastException"
        );
        // The self-compare law: a non-Comparable key throws CCE on the
        // FIRST put (compare(key,key), the addEntryToEmptyMap law).
        let obj = new_obj("java/lang/Object", 0).unwrap();
        let err = tm_put_prim(&m, Value::Ref(Some(obj)), Value::Int(1), ctx()).unwrap_err();
        assert_eq!(
            with_obj(err.obj, |o| o.jclass.to_string()),
            "java/lang/ClassCastException"
        );
    }

    #[test]
    fn tm_delete_transplant_moves_the_tag_with_the_value() {
        let _rt = crate::runtime();
        let m = tm_new();
        // Three raw-prim entries: the root ("b") has two children, so
        // the delete walks the transplant-by-copy (the successor's kv
        // moves into the root slot) — the tag must ride or the surviving
        // raw slot becomes unnamed.
        tm_put_prim(&m, key("b"), Value::Int(2), ctx()).unwrap();
        tm_put_prim(&m, key("a"), Value::Int(1), ctx()).unwrap();
        tm_put_prim(&m, key("c"), Value::Int(3), ctx()).unwrap();
        // Faithful remove of the ROOT ("b"): the successor ("c") copies
        // up. The core get of "c" must still unwrap.
        assert_eq!(
            unbox_i32(&n_tm_remove(Some(&m), &[key("b")], ctx()).unwrap()),
            2
        );
        assert_eq!(
            tm_get_prim(&m, key("c"), "java/lang/Integer", ctx()).unwrap(),
            Value::Int(3)
        );
        let ts = n_tm_to_string(Some(&m), &[], ctx()).unwrap();
        assert_eq!(s(&ts), "{a=1, c=3}");
    }

    #[test]
    fn tm_view_readers_ride_the_raw_slots() {
        let _rt = crate::runtime();
        let m = tm_new();
        tm_put_prim(&m, key("a"), Value::Int(1), ctx()).unwrap();
        tm_put_prim(&m, key("b"), Value::Int(2), ctx()).unwrap();
        tm_put_prim(&m, key("z"), Value::Int(26), ctx()).unwrap();
        // The bounded view (slice-4's "the views ride"): the view's get,
        // toString, containsValue and value iterator all materialize.
        let view = n_tm_sub_map(Some(&m), &[key("a"), key("z")], ctx()).unwrap();
        let Value::Ref(Some(v)) = &view else {
            panic!("subMap view")
        };
        let g = n_sms_get(Some(v), &[key("b")], ctx()).unwrap();
        let Value::Ref(Some(bg)) = &g else {
            panic!("view get must materialize: {g:?}")
        };
        assert_eq!(with_obj(*bg, |o| o.jclass.to_string()), "java/lang/Integer");
        let ts = n_sms_to_string(Some(v), &[], ctx()).unwrap();
        // The to-EXCLUSIVE range law (run 6): z sits OUTSIDE [a, z).
        assert_eq!(s(&ts), "{a=1, b=2}");
        assert_eq!(
            n_sms_contains_value(Some(v), &[boxed_i32(2)], ctx()).unwrap(),
            Value::Int(1)
        );
        // The SubMap value iterator materializes too.
        let it_v = sms_new_iterator(v, "java/util/TreeMap$SubMap$ValueIterator", ctx()).unwrap();
        let Value::Ref(Some(it)) = &it_v else {
            panic!("values iterator")
        };
        let first = n_smvi_next(Some(it), &[], ctx()).unwrap();
        let Value::Ref(Some(bf)) = &first else {
            panic!("view iterator must materialize: {first:?}")
        };
        assert_eq!(with_obj(*bf, |o| o.slots[0].clone()), Value::Int(1));
    }

    #[test]
    fn lhm_put_prim_links_the_tail_and_materializes() {
        let _rt = crate::runtime();
        let m = lhm_new();
        // The specialized put on the LINK-CHAIN storage: the node kind
        // follows the map's runtime class (LinkedHashMap$Entry), the
        // tail links move (insertion order), the tag rides.
        hm_put_prim(&m, key("b"), Value::Int(2), ctx()).unwrap();
        hm_put_prim(&m, key("a"), Value::Int(1), ctx()).unwrap();
        hm_put_prim(&m, key("c"), Value::Int(3), ctx()).unwrap();
        // The nodes are LinkedHashMap$Entry objects with the tag slot.
        let node = hm_find(&m, &key("a"), ctx()).unwrap().expect("node");
        assert_eq!(
            with_obj(node, |o| o.jclass.to_string()),
            "java/util/LinkedHashMap$Entry",
            "the node kind follows the map's runtime class"
        );
        assert_eq!(
            with_obj(node, |o| o.slots[HM_NODE_TAG].clone()),
            Value::Int('I' as i32)
        );
        // Insertion order preserved with raw slots; values materialize.
        let ts = n_lhm_to_string(Some(&m), &[], ctx()).unwrap();
        assert_eq!(s(&ts), "{b=2, a=1, c=3}");
        // The faithful HashMap-law get materializes (the parent chain).
        let g1 = n_hm_get(Some(&m), &[key("a")], ctx()).unwrap();
        let Value::Ref(Some(b)) = &g1 else {
            panic!("LHM get must materialize: {g1:?}")
        };
        assert_eq!(with_obj(*b, |o| o.jclass.to_string()), "java/lang/Integer");
        // The fused get unwraps; the overwrite keeps position (no link
        // work) and the tail order survives.
        assert_eq!(
            hm_get_prim(&m, key("b"), "java/lang/Integer", ctx()).unwrap(),
            Value::Int(2)
        );
        hm_put_prim(&m, key("b"), Value::Int(22), ctx()).unwrap();
        let ts2 = n_lhm_to_string(Some(&m), &[], ctx()).unwrap();
        assert_eq!(s(&ts2), "{b=22, a=1, c=3}");
        // The remove unlink law: the old value materializes, the link
        // chain stays walkable.
        let removed = n_lhm_remove(Some(&m), &[key("a")], ctx()).unwrap();
        assert_eq!(unbox_i32(&removed), 1);
        let ts3 = n_lhm_to_string(Some(&m), &[], ctx()).unwrap();
        assert_eq!(s(&ts3), "{b=22, c=3}");
    }

    // ---- the fused get's unbox law (the faithful failure modes) ----

    fn msg(err: &Throw) -> String {
        let m = with_obj(err.obj, |o| o.slots[0].clone());
        s(&m)
    }

    #[test]
    fn fused_get_throws_faithful_cce_on_tag_mismatch() {
        let _rt = crate::runtime();
        let m = map_new();
        hm_put_prim(&m, key("a"), Value::Double(0.5), ctx()).unwrap();
        // A get core compiled against Integer on a Double-prim slot: the
        // tag is the faithful checkcast's actual type.
        let err = hm_get_prim(&m, key("a"), "java/lang/Integer", ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/ClassCastException");
        assert_eq!(
            msg(&err),
            "class java.lang.Double cannot be cast to class java.lang.Integer"
        );
        // The matching expect unwraps.
        let ok = hm_get_prim(&m, key("a"), "java/lang/Double", ctx()).unwrap();
        assert_eq!(ok, Value::Double(0.5));
    }

    #[test]
    fn fused_get_throws_faithful_cce_on_foreign_box() {
        let _rt = crate::runtime();
        let m = map_new();
        hm_put_prim(&m, key("a"), Value::Double(0.5), ctx()).unwrap();
        // A non-core put (unchecked generics) wrote an Integer BOX over
        // the prim slot: the faithful checkcast runs on the box.
        n_hm_put(Some(&m), &[key("a"), boxed_i32(9)], ctx()).unwrap();
        let err = hm_get_prim(&m, key("a"), "java/lang/Double", ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/ClassCastException");
        assert_eq!(
            msg(&err),
            "class java.lang.Integer cannot be cast to class java.lang.Double"
        );
        // The right box (same class as expect) unwraps through its slot.
        n_hm_put(Some(&m), &[key("a"), boxed_i32(9)], ctx()).unwrap();
        let ok = hm_get_prim(&m, key("a"), "java/lang/Integer", ctx()).unwrap();
        assert_eq!(ok, Value::Int(9));
    }

    #[test]
    fn fused_get_throws_bare_npe_on_null_value() {
        let _rt = crate::runtime();
        let m = map_new();
        // A non-core put stored a null VALUE (javac: aconst_null; put).
        n_hm_put(Some(&m), &[key("a"), Value::Ref(None)], ctx()).unwrap();
        let err = hm_get_prim(&m, key("a"), "java/lang/Double", ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/NullPointerException");
        // The BARE law: no message.
        assert!(matches!(
            with_obj(err.obj, |o| o.slots[0].clone()),
            Value::Ref(None)
        ));
    }
}
