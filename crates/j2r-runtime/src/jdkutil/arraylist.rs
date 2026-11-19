use super::*;

/// Reads a List's elements via `get` dispatch (the JDK's own sort does the
/// equivalent through a ListIterator).
pub(crate) fn list_to_vec(list: &JRef, ctx: Dispatch) -> Result<Vec<Value>, Throw> {
    let n = list_size(list, ctx)?;
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        out.push(list_get(list, i, ctx)?);
    }
    Ok(out)
}
pub(crate) fn list_size(list: &JRef, ctx: Dispatch) -> Result<i32, Throw> {
    let out = invoke_virtual(list, "java/util/List", "size", "()I", &[], ctx)?;
    Ok(out.expect_int())
}
pub(crate) fn list_get(list: &JRef, i: i32, ctx: Dispatch) -> Result<Value, Throw> {
    invoke_virtual(
        list,
        "java/util/List",
        "get",
        "(I)Ljava/lang/Object;",
        &[Value::Int(i)],
        ctx,
    )
}
pub(crate) fn list_set(list: &JRef, i: i32, v: &Value, ctx: Dispatch) -> Result<(), Throw> {
    invoke_virtual(
        list,
        "java/util/List",
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        &[Value::Int(i), v.clone()],
        ctx,
    )?;
    Ok(())
}
pub fn n_collections_sort_cmp(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let (list, cmp) = match (args.first(), args.get(1)) {
        (Some(Value::Ref(Some(l))), Some(Value::Ref(Some(c)))) => (l, c),
        (Some(Value::Ref(None)), _) => {
            return Err(crate::npe());
        }
        _ => return Err(null_arg("Collections.sort")),
    };
    let mut vals = list_to_vec(list, ctx)?;
    merge_sort_stable(&mut vals, &|x, y| comparator_compare(cmp, x, y, ctx))?;
    for (i, v) in vals.iter().enumerate() {
        list_set(list, i as i32, v, ctx)?;
    }
    Ok(Value::Void)
}
pub fn n_collections_sort_natural(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let list = match args.first() {
        Some(Value::Ref(Some(l))) => l,
        Some(Value::Ref(None)) => {
            return Err(crate::npe());
        }
        _ => return Err(null_arg("Collections.sort")),
    };
    let mut vals = list_to_vec(list, ctx)?;
    merge_sort_stable(&mut vals, &|x, y| compare_to_dispatch(x, y, ctx))?;
    for (i, v) in vals.iter().enumerate() {
        list_set(list, i as i32, v, ctx)?;
    }
    Ok(Value::Void)
}
pub fn n_collections_reverse(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let list = match args.first() {
        Some(Value::Ref(Some(l))) => l,
        _ => return Err(null_arg("Collections.reverse")),
    };
    let n = list_size(list, ctx)?;
    let mut i = 0;
    let mut j = n - 1;
    while i < j {
        let vi = list_get(list, i, ctx)?;
        let vj = list_get(list, j, ctx)?;
        list_set(list, i, &vj, ctx)?;
        list_set(list, j, &vi, ctx)?;
        i += 1;
        j -= 1;
    }
    Ok(Value::Void)
}
/// `Collections.addAll(Collection, T...)` — javac lowers the varargs to a
/// plain `Object[]`, so this is a loop of `add` dispatches over the array.
pub fn n_collections_add_all(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let (coll, arr) = match (args.first(), args.get(1)) {
        (Some(Value::Ref(Some(c))), Some(Value::Ref(Some(a)))) => (c, a),
        (Some(Value::Ref(None)), _) | (_, Some(Value::Ref(None))) => {
            return Err(crate::npe());
        }
        _ => return Err(null_arg("Collections.addAll")),
    };
    let len = arr_elems(arr)?;
    for i in 0..len {
        let v = arr_load(arr, i)?;
        invoke_virtual(
            coll,
            "java/util/Collection",
            "add",
            "(Ljava/lang/Object;)Z",
            std::slice::from_ref(&v),
            ctx,
        )?;
    }
    Ok(Value::Int(1))
}
pub fn n_collections_swap(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let (list, i, j) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(l))), Some(i), Some(j)) => (l, i.expect_int(), j.expect_int()),
        _ => return Err(null_arg("Collections.swap")),
    };
    let vi = list_get(list, i, ctx)?;
    let vj = list_get(list, j, ctx)?;
    list_set(list, i, &vj, ctx)?;
    list_set(list, j, &vi, ctx)?;
    Ok(Value::Void)
}

// ---------------------------------------------------------------------------
// ArrayList.subList — live view semantics
// ---------------------------------------------------------------------------
/// `ArrayList.toArray()` — a fresh `Object[]` of exactly `size` slots.
pub fn n_al_to_array(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let list = match recv {
        Some(r) => r,
        None => return Err(null_arg("ArrayList.toArray")),
    };
    let (size, _mc, elems) = collections::al_state(list);
    let out = array::array_new_ref("java/lang/Object", size)?;
    for i in 0..size {
        let v = arr_load(&elems, i)?;
        arr_store(&out, i, v, ctx)?;
    }
    Ok(Value::Ref(Some(out)))
}
/// Component class of a ref-array descriptor ("[Ljava/lang/String;" ->
/// "java/lang/String"; "[I" -> "I" — a primitive component that will
/// fail the aastore law on the first store, exactly like the JDK).
fn array_component(desc: &str) -> String {
    if let Some(rest) = desc.strip_prefix("[L") {
        rest.trim_end_matches(';').to_string()
    } else {
        desc[1..].to_string()
    }
}
/// `ArrayList.toArray(T[])` — the two-branch contract above.
pub fn n_al_to_array_typed(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let list = match recv {
        Some(r) => r,
        None => return Err(null_arg("ArrayList.toArray")),
    };
    let a = match args.first() {
        Some(Value::Ref(Some(r))) => *r,
        // JDK: `a` null is an ordinary NPE at the first dereference
        Some(Value::Ref(None)) | None => return Err(crate::npe()),
        other => {
            return Err(internal_error(&format!(
                "j2r internal: ArrayList.toArray arg {other:?}"
            )))
        }
    };
    let (size, _mc, elems) = collections::al_state(list);
    let (alen, adesc) = with_obj(a, |o| {
        (o.slots[0].clone().expect_int(), o.jclass.to_string())
    });
    if alen < size {
        let out = array::array_new_ref(&array_component(&adesc), size)?;
        for i in 0..size {
            let v = arr_load(&elems, i)?;
            arr_store(&out, i, v, ctx)?;
        }
        return Ok(Value::Ref(Some(out)));
    }
    for i in 0..size {
        let v = arr_load(&elems, i)?;
        arr_store(&a, i, v, ctx)?;
    }
    if alen > size {
        arr_store(&a, size, Value::Ref(None), ctx)?;
    }
    Ok(Value::Ref(Some(a)))
}

// ---------------------------------------------------------------------------
// EXTERNAL-B12 (jbox2d port, 2026-11-13): Arrays.sort range forms + the
// assertion-status static. The JDK's rangeCheck with exact messages:
//   fromIndex > toIndex  -> IllegalArgumentException("fromIndex(5) > toIndex(2)")
//   fromIndex < 0        -> ArrayIndexOutOfBoundsException("Array index out of range: -1")
//   toIndex > length     -> ArrayIndexOutOfBoundsException("Array index out of range: 12")
// and a null comparator FALLS BACK to natural ordering (OpenJDK's
// sort(T[], int, int, Comparator) first branch) rather than throwing.
/// The JDK's private `rangeCheck(arrayLength, fromIndex, toIndex)`.
pub(crate) fn arrays_range_check(len: i32, from: i32, to: i32) -> Result<(), Throw> {
    if from > to {
        return Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some(&format!("fromIndex({from}) > toIndex({to})")),
        ));
    }
    if from < 0 {
        return Err(throw_new(
            crate::AIOOBE_CLASS,
            Some(&format!("Array index out of range: {from}")),
        ));
    }
    if to > len {
        return Err(throw_new(
            crate::AIOOBE_CLASS,
            Some(&format!("Array index out of range: {to}")),
        ));
    }
    Ok(())
}
