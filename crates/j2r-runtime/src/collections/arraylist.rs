use super::*;

pub(crate) const AL_SIZE: usize = 0;
pub(crate) const AL_MODCOUNT: usize = 1;
pub(crate) const AL_ELEMS: usize = 2;
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
