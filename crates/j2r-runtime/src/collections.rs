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
//!   value, slot 3 next-node reference or null.
//! * `java/util/HashMap$KeyIterator` — slot 0 map reference, slot 1 next
//!   table index to scan (Int), slot 2 pending node or null, slot 3
//!   `expectedModCount` (Int).
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

use crate::heap::{alloc, with_obj, with_obj_mut, JRef};
use crate::object::{new_obj, string_value};
use crate::Value;
use crate::{internal_error, invoke_virtual, throw_new, Dispatch, Throw};
use std::sync::Arc;

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
        Some(Value::Ref(None)) | None => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
        other => {
            return Err(crate::internal_error(&format!(
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
            return Err(crate::internal_error(&format!(
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
    let e = args.first().cloned().unwrap_or(Value::Ref(None));
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
        None => return Err(throw_new("java/lang/NullPointerException", None)),
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
                return Err(crate::internal_error(&format!(
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
        None => return Err(throw_new("java/lang/NullPointerException", None)),
    };
    let other = match args.first() {
        Some(Value::Ref(Some(o))) => *o,
        Some(Value::Ref(None)) | None => return Ok(Value::Int(0)),
        other => {
            return Err(crate::internal_error(&format!(
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
                return Err(crate::internal_error(&format!(
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
        None => return Err(throw_new("java/lang/NullPointerException", None)),
    };
    let target = args.first().cloned().unwrap_or(Value::Ref(None));
    let (size, _mc, elems) = al_state(r);
    for i in (0..size).rev() {
        let e = al_get_elem(&elems, i);
        let eq = match (&e, &target) {
            (Value::Ref(None), Value::Ref(None)) => true,
            (Value::Ref(None), _) | (_, Value::Ref(None)) => false,
            (Value::Ref(Some(x)), _) => dispatch_equals(x, &target, ctx)?,
            (other_e, _) => {
                return Err(crate::internal_error(&format!(
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
    let target = args.first().cloned().unwrap_or(Value::Ref(None));
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
    let target = args.first().cloned().unwrap_or(Value::Ref(None));
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
    let target = args.first().cloned().unwrap_or(Value::Ref(None));
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
        _ => return Err(throw_new("java/lang/NullPointerException", None)),
    };
    crate::invoke_virtual(&r, iface, name, desc, args, ctx)
}

/// `ArrayList.forEach(Consumer)` — OpenJDK-faithful: the action runs per
/// element in order and `modCount` is checked after every element, so a
/// structural modification through the action throws CME.
pub fn n_al_for_each(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: ArrayList.forEach without receiver");
    let action = args.first().cloned().unwrap_or(Value::Ref(None));
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
            return Err(throw_new("java/util/ConcurrentModificationException", None));
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
    let filter = args.first().cloned().unwrap_or(Value::Ref(None));
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
            return Err(throw_new("java/util/ConcurrentModificationException", None));
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
        return Err(throw_new("java/util/ConcurrentModificationException", None));
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

/// OpenJDK `putVal` with tail insertion. Returns the previous value
/// (`Ref(None)` when absent).
fn hm_put(map: &JRef, key: Value, value: Value, ctx: Dispatch) -> Result<Value, Throw> {
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
        let (h, k, old_value, next) = node_parts(&node);
        if key_matches(h, &k, hash, &key, ctx)? {
            with_obj_mut(node, |o| o.slots[2] = value);
            return Ok(old_value);
        }
        prev = Some(node);
        cur = next;
    }
    // tail-insert a fresh node
    let node = new_obj("java/util/HashMap$Node", 4)?;
    with_obj_mut(node, |o| {
        o.slots[0] = Value::Int(hash);
        o.slots[1] = key;
        o.slots[2] = value;
        o.slots[3] = Value::Ref(None);
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
    Ok(Value::Ref(None))
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
        let (h, k, value, next) = node_parts(&node);
        if key_matches(h, &k, hash, key, ctx)? {
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
            return Ok(value);
        }
        prev = Some(node);
        cur = next;
    }
    Ok(Value::Ref(None))
}

/// Table-order iteration (bins in order, within-bin list order).
fn hm_all_entries(map: &JRef) -> Vec<(Value, Value)> {
    let (_, _, _, table) = hm_state(map);
    let Some(table) = table else {
        return Vec::new();
    };
    let len = hm_table_len(&table);
    let mut out = Vec::new();
    for i in 0..len {
        let mut cur = hm_bin(&table, i);
        while let Some(node) = cur {
            let (_, k, v, next) = node_parts(&node);
            out.push((k, v));
            cur = next;
        }
    }
    out
}

// <init>()V / <init>(I)V
pub fn n_hm_init(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.<init> without receiver");
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

pub fn n_hm_put(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.put without receiver");
    let key = args.first().cloned().unwrap_or(Value::Ref(None));
    let value = args.get(1).cloned().unwrap_or(Value::Ref(None));
    hm_put(r, key, value, ctx)
}

pub fn n_hm_get(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.get without receiver");
    let key = args.first().cloned().unwrap_or(Value::Ref(None));
    match hm_find(r, &key, ctx)? {
        Some(node) => Ok(node_parts(&node).2),
        None => Ok(Value::Ref(None)),
    }
}

pub fn n_hm_get_or_default(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.getOrDefault without receiver");
    let key = args.first().cloned().unwrap_or(Value::Ref(None));
    let default = args.get(1).cloned().unwrap_or(Value::Ref(None));
    match hm_find(r, &key, ctx)? {
        Some(node) => Ok(node_parts(&node).2),
        None => Ok(default),
    }
}

pub fn n_hm_contains_key(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.containsKey without receiver");
    let key = args.first().cloned().unwrap_or(Value::Ref(None));
    Ok(Value::Int(hm_find(r, &key, ctx)?.is_some() as i32))
}

pub fn n_hm_contains_value(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.containsValue without receiver");
    let target = args.first().cloned().unwrap_or(Value::Ref(None));
    for (_, v) in hm_all_entries(r) {
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
    let key = args.first().cloned().unwrap_or(Value::Ref(None));
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
    let entries = hm_all_entries(r);
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
    let key = args.first().cloned().unwrap_or(Value::Ref(None));
    Ok(Value::Int(hm_find(&map, &key, ctx)?.is_some() as i32))
}

pub fn n_ks_to_string(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = view_map(recv, "KeySet.toString");
    let items: Vec<Value> = hm_all_entries(&map).into_iter().map(|(k, _)| k).collect();
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
    let items: Vec<Value> = hm_all_entries(&map).into_iter().map(|(_, v)| v).collect();
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
/// null, 3 = expectedModCount.
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
    let it = new_obj(class, 4)?;
    with_obj_mut(it, |o| {
        o.slots[0] = Value::Ref(Some(*map));
        o.slots[1] = Value::Int(idx);
        o.slots[2] = match pending {
            Some(n) => Value::Ref(Some(n)),
            None => Value::Ref(None),
        };
        o.slots[3] = Value::Int(mc);
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
        return Err(throw_new("java/util/ConcurrentModificationException", None));
    }
    let Some(node) = pending else {
        return Err(throw_new("java/util/NoSuchElementException", None));
    };
    hm_advance(it, node);
    Ok((*it, node))
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
    Ok(node_parts(&node).2)
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
    let e = args.first().cloned().unwrap_or(Value::Ref(None));
    // Java: map.put(e, PRESENT) == null — PRESENT is a shared dummy whose
    // only requirement is being a live heap object (roots scan it through
    // the map's node slots).
    let present = new_obj("java/lang/Object", 0)?;
    let old = hm_put(&map, e, Value::Ref(Some(present)), ctx)?;
    Ok(Value::Int(old.is_null() as i32))
}

pub fn n_hs_remove(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = hs_map(recv, "remove");
    let e = args.first().cloned().unwrap_or(Value::Ref(None));
    let old = hm_remove(&map, &e, ctx)?;
    Ok(Value::Int((!old.is_null()) as i32))
}

/// `Set.addAll(Collection)` — the stringsim corpus wave (Jaccard's union).
/// Iterates through Collection dispatch so any modelled collection works.
pub fn n_hs_add_all(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let set = recv.expect("j2r internal: Set.addAll without receiver");
    let coll = match args.first() {
        Some(Value::Ref(Some(c))) => *c,
        Some(Value::Ref(None)) | None => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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
    let e = args.first().cloned().unwrap_or(Value::Ref(None));
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
    let items: Vec<Value> = hm_all_entries(&map).into_iter().map(|(k, _)| k).collect();
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
    Ok(node_parts(n).2)
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
        _ => return Err(throw_new("java/lang/NullPointerException", None)),
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
    let key = args.first().cloned().unwrap_or(Value::Ref(None));
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
    let v = args.first().cloned().unwrap_or(Value::Ref(None));
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
    let a = args.first().cloned().unwrap_or(Value::Ref(None));
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
    let a = args.first().cloned().unwrap_or(Value::Ref(None));
    Ok(Value::Int(dispatch_hash_code(&a, ctx)?))
}

pub fn n_objects_require_non_null(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = args.first().cloned().unwrap_or(Value::Ref(None));
    if a.is_null() {
        return Err(throw_new("java/lang/NullPointerException", None));
    }
    Ok(a)
}

pub fn n_objects_require_non_null_msg(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = args.first().cloned().unwrap_or(Value::Ref(None));
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
    let a = args.first().cloned().unwrap_or(Value::Ref(None));
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
            _ => return Err(throw_new("java/lang/NullPointerException", None)),
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
    let a = args.first().cloned().unwrap_or(Value::Ref(None));
    let s = dispatch_to_string(&a, ctx)?;
    Ok(Value::Ref(Some(crate::object::new_string(&s))))
}

pub fn n_objects_is_null(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = args.first().cloned().unwrap_or(Value::Ref(None));
    Ok(Value::Int(a.is_null() as i32))
}

pub fn n_objects_non_null(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = args.first().cloned().unwrap_or(Value::Ref(None));
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

    // ---- tableSizeFor (OpenJDK edge behaviour) ----

    #[test]
    fn table_size_for_matches_openjdk() {
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
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
        assert_eq!(
            err.java_class(),
            "java/util/ConcurrentModificationException"
        );
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
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
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
        assert_eq!(
            err.java_class(),
            "java/util/ConcurrentModificationException"
        );
    }

    #[test]
    fn hashmap_null_key_and_value() {
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
        let m = new_obj("java/util/HashMap", 4).unwrap();
        let err = n_hm_init(Some(&m), &[Value::Int(-3)], ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/IllegalArgumentException");
        let msg = with_obj(err.obj, |o| o.slots[0].clone());
        assert_eq!(s(&msg), "Illegal initial capacity: -3");
    }

    // ---- HashSet ----

    #[test]
    fn hashset_add_contains_remove_tostring() {
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
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
        let _rt = crate::test_support::runtime();
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
}
