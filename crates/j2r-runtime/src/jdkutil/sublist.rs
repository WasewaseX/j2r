use super::*;

const SL_PARENT: usize = 0;
const SL_OFFSET: usize = 1;
const SL_SIZE: usize = 2;
const SL_EXPECTED: usize = 3;
/// The JDK's `IndexOutOfBoundsException` messages for subList bounds:
/// `fromIndex(5) > toIndex(2)` for crossed bounds, `toIndex = 12` /
/// `fromIndex = -1` for out-of-range.
fn check_sub_range(from: i32, to: i32, size: i32) -> Result<(), Throw> {
    if from < 0 {
        return Err(throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&format!("fromIndex = {from}")),
        ));
    }
    if to > size {
        return Err(throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&format!("toIndex = {to}")),
        ));
    }
    if from > to {
        // Crossed bounds are an IllegalArgumentException in the JDK
        // (AbstractList.subListRangeCheck), unlike out-of-range bounds.
        return Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some(&format!("fromIndex({from}) > toIndex({to})")),
        ));
    }
    Ok(())
}
pub fn n_al_sub_list(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let list = match recv {
        Some(r) => r,
        None => return Err(null_arg("ArrayList.subList")),
    };
    // Chained views (`list.subList(..).subList(..)`) are not supported
    // yet — refuse loudly instead of misreading the SubList layout.
    let jclass = with_obj(*list, |o| o.jclass.to_string());
    if jclass != "java/util/ArrayList" {
        return Err(internal_error(&format!(
            "subList on {jclass} is outside the supported surface (chained views are a documented loud error)"
        )));
    }
    let (from, to) = match (args.first(), args.get(1)) {
        (Some(f), Some(t)) => (f.expect_int(), t.expect_int()),
        _ => return Err(null_arg("ArrayList.subList")),
    };
    let (size, mod_count, _elems) = collections::al_state(list);
    check_sub_range(from, to, size)?;
    let view = crate::heap::alloc(crate::object::JObj {
        jclass: Arc::from("java/util/ArrayList$SubList"),
        slots: vec![
            Value::Ref(Some(*list)),
            Value::Int(from),
            Value::Int(to - from),
            Value::Int(mod_count),
        ],
        prim: None,
    });
    Ok(Value::Ref(Some(view)))
}
/// The parent ArrayList reference + view fields, all at once.
fn sl_state(view: &JRef) -> (JRef, i32, i32, i32) {
    with_obj(*view, |o| {
        let parent = match &o.slots[SL_PARENT] {
            Value::Ref(Some(p)) => *p,
            other => panic!("j2r internal: SubList without parent: {other:?}"),
        };
        (
            parent,
            o.slots[SL_OFFSET].expect_int(),
            o.slots[SL_SIZE].expect_int(),
            o.slots[SL_EXPECTED].expect_int(),
        )
    })
}
/// Temurin throws a message-less CME when the parent's structure changed
/// behind the view (AbstractList$SubList.checkForComodification).
fn sl_check_comod(parent: &JRef, expected: i32) -> Result<(), Throw> {
    let (_, mod_count, _) = collections::al_state(parent);
    if mod_count != expected {
        return Err(crate::cme());
    }
    Ok(())
}
/// After the view itself structurally mutates the parent, both the parent's
/// modCount and the view's expected copy move in lockstep.
fn sl_sync(view: &JRef, parent: &JRef) {
    let (_, mod_count, _) = collections::al_state(parent);
    with_obj_mut(*view, |o| o.slots[SL_EXPECTED] = Value::Int(mod_count));
}
fn sl_abs_index(view: &JRef, idx: i32) -> Result<(JRef, i32, i32, i32), Throw> {
    let (parent, offset, size, expected) = sl_state(view);
    sl_check_comod(&parent, expected)?;
    if idx < 0 || idx >= size {
        return Err(throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&format!("Index {idx} out of bounds for length {size}")),
        ));
    }
    Ok((parent, offset, idx, expected))
}
pub fn n_sl_get(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = match recv {
        Some(r) => r,
        None => return Err(null_arg("SubList.get")),
    };
    let idx = match args.first() {
        Some(v) => v.expect_int(),
        None => return Err(null_arg("SubList.get")),
    };
    let (parent, offset, idx, _) = sl_abs_index(view, idx)?;
    list_get(&parent, offset + idx, ctx)
}
pub fn n_sl_set(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = match recv {
        Some(r) => r,
        None => return Err(null_arg("SubList.set")),
    };
    let idx = match args.first() {
        Some(v) => v.expect_int(),
        None => return Err(null_arg("SubList.set")),
    };
    let v = match args.get(1) {
        Some(v) => v.clone(),
        None => return Err(null_arg("SubList.set")),
    };
    let (parent, offset, idx, _) = sl_abs_index(view, idx)?;
    // set is not structural: no modCount bump, JDK-faithful.
    invoke_virtual(
        &parent,
        "java/util/List",
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        &[Value::Int(offset + idx), v],
        ctx,
    )
}
pub fn n_sl_add_at(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (view, idx, v) = match (recv, args.first(), args.get(1)) {
        (Some(r), Some(i), Some(v)) => (r, i.expect_int(), v.clone()),
        _ => return Err(null_arg("SubList.add")),
    };
    sl_add_at_core(view, idx, &v, ctx)?;
    Ok(Value::Void)
}
/// `AbstractList.add(Object)` on the view appends at `size` and returns
/// true — inherited by JDK SubList, so the fixture's `tail.add(x)` lands
/// here.
pub fn n_sl_add_append(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (view, v) = match (recv, args.first()) {
        (Some(r), Some(v)) => (r, v.clone()),
        _ => return Err(null_arg("SubList.add")),
    };
    let size = sl_state(view).2;
    sl_add_at_core(view, size, &v, ctx)?;
    Ok(Value::Int(1))
}
fn sl_add_at_core(view: &JRef, idx: i32, v: &Value, ctx: Dispatch) -> Result<(), Throw> {
    let (parent, offset, idx, _) = {
        let (parent, offset, size, expected) = sl_state(view);
        sl_check_comod(&parent, expected)?;
        if idx < 0 || idx > size {
            return Err(throw_new(
                "java/lang/IndexOutOfBoundsException",
                Some(&format!("Index {idx} out of bounds for length {size}")),
            ));
        }
        (parent, offset, idx, expected)
    };
    invoke_virtual(
        &parent,
        "java/util/List",
        "add",
        "(ILjava/lang/Object;)V",
        &[Value::Int(offset + idx), v.clone()],
        ctx,
    )?;
    with_obj_mut(*view, |o| {
        o.slots[SL_SIZE] = Value::Int(o.slots[SL_SIZE].expect_int() + 1);
    });
    sl_sync(view, &parent);
    Ok(())
}
pub fn n_sl_remove_at(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = match recv {
        Some(r) => r,
        None => return Err(null_arg("SubList.remove")),
    };
    let idx = match args.first() {
        Some(v) => v.expect_int(),
        None => return Err(null_arg("SubList.remove")),
    };
    let (parent, offset, idx, _) = sl_abs_index(view, idx)?;
    let old = invoke_virtual(
        &parent,
        "java/util/List",
        "remove",
        "(I)Ljava/lang/Object;",
        &[Value::Int(offset + idx)],
        ctx,
    )?;
    with_obj_mut(*view, |o| {
        o.slots[SL_SIZE] = Value::Int(o.slots[SL_SIZE].expect_int() - 1);
    });
    sl_sync(view, &parent);
    Ok(old)
}
pub fn n_sl_size(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let _ = args;
    let view = match recv {
        Some(r) => r,
        None => return Err(null_arg("SubList.size")),
    };
    let (_, _, size, _) = sl_state(view);
    Ok(Value::Int(size))
}
/// `AbstractList.toString`-style collection printing for the view —
/// `[a, b, c]`, CME-checked like every other view operation (println
/// dispatches toString through the receiver's chain).
pub fn n_sl_to_string(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let _ = args;
    let view = match recv {
        Some(r) => r,
        None => return Err(null_arg("SubList.toString")),
    };
    let (parent, offset, size, expected) = sl_state(view);
    sl_check_comod(&parent, expected)?;
    let mut sb = String::from("[");
    for i in 0..size {
        if i > 0 {
            sb.push_str(", ");
        }
        let v = list_get(&parent, offset + i, ctx)?;
        match v {
            Value::Str(s) => sb.push_str(&s),
            Value::Ref(None) => sb.push_str("null"),
            Value::Ref(Some(r)) => {
                let out = invoke_virtual(
                    &r,
                    "java/lang/Object",
                    "toString",
                    "()Ljava/lang/String;",
                    &[],
                    ctx,
                )?;
                let s = string_of_value(&out, "SubList.toString")?;
                sb.push_str(&s);
            }
            other => {
                return Err(internal_error(&format!(
                    "j2r internal: SubList element {other:?}"
                )))
            }
        }
    }
    sb.push(']');
    Ok(Value::Ref(Some(object::new_string(&sb))))
}
pub fn n_sl_is_empty(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let _ = args;
    let view = match recv {
        Some(r) => r,
        None => return Err(null_arg("SubList.isEmpty")),
    };
    let (_, _, size, _) = sl_state(view);
    Ok(Value::Int((size == 0) as i32))
}

// ---- Collections.unmodifiableList (corpus wave 1: minimal-json) ----
//
// The note that used to live here said the wrapping view would land when a
// real fixture needed it. `JsonObject`'s unmodifiable copy ctor needs it
// (the compile-time surface walk must resolve `Collections.unmodifiableList`
// even though the driver never executes that path). The view is a builtin
// object with one slot — the backing list. Reads delegate to the backing
// list's own natives (live view semantics, matching the JDK wrapper);
// writes throw `UnsupportedOperationException`, which is the real JDK
// behavior of the wrapper — a documented boundary, not an approximation.
