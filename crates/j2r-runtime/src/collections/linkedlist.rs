use super::*;

pub(crate) const LL_SIZE: usize = 0;
pub(crate) const LL_MODCOUNT: usize = 1;
pub(crate) const LL_ELEMS: usize = 2;
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
