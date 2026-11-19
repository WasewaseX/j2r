use super::*;

pub fn n_arrays_to_string_int(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = match args.first() {
        Some(Value::Ref(Some(r))) => r,
        _ => return Ok(Value::Ref(Some(object::new_string("null")))),
    };
    let s = join_array(a, &|v| Ok(v.expect_int().to_string()))?;
    Ok(Value::Ref(Some(object::new_string(&s))))
}
pub fn n_arrays_to_string_long(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = match args.first() {
        Some(Value::Ref(Some(r))) => r,
        _ => return Ok(Value::Ref(Some(object::new_string("null")))),
    };
    let s = join_array(a, &|v| Ok(v.expect_long().to_string()))?;
    Ok(Value::Ref(Some(object::new_string(&s))))
}
pub fn n_arrays_to_string_double(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = match args.first() {
        Some(Value::Ref(Some(r))) => r,
        _ => return Ok(Value::Ref(Some(object::new_string("null")))),
    };
    // Doubles go through their exact Java toString (the subnormal quirk
    // and all) by boxing each element and dispatching Double.toString.
    let s = join_array(a, &|v| {
        let out = natives::invoke_exact(
            "java/lang/Double",
            "toString",
            "(D)Ljava/lang/String;",
            None,
            &[Value::Double(v.expect_double())],
            ctx,
        )?;
        Ok(string_of_value(&out, "Double.toString")?.to_string())
    })?;
    Ok(Value::Ref(Some(object::new_string(&s))))
}
pub fn n_arrays_to_string_bool(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = match args.first() {
        Some(Value::Ref(Some(r))) => r,
        _ => return Ok(Value::Ref(Some(object::new_string("null")))),
    };
    let s = join_array(a, &|v| {
        Ok(if v.expect_int() != 0 {
            "true".to_string()
        } else {
            "false".to_string()
        })
    })?;
    Ok(Value::Ref(Some(object::new_string(&s))))
}
pub fn n_arrays_to_string_char(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = match args.first() {
        Some(Value::Ref(Some(r))) => r,
        _ => return Ok(Value::Ref(Some(object::new_string("null")))),
    };
    let s = join_array(a, &|v| {
        Ok(char::from_u32(v.expect_int() as u32)
            .unwrap_or('\u{FFFD}')
            .to_string())
    })?;
    Ok(Value::Ref(Some(object::new_string(&s))))
}
pub fn n_arrays_to_string_obj(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = match args.first() {
        Some(Value::Ref(Some(r))) => r,
        _ => return Ok(Value::Ref(Some(object::new_string("null")))),
    };
    let s = join_array(a, &|v| v_to_string(v, ctx))?;
    Ok(Value::Ref(Some(object::new_string(&s))))
}

// ---------------------------------------------------------------------------
// Arrays.fill / copyOf / copyOfRange / equals
// ---------------------------------------------------------------------------
pub fn n_arrays_fill_int(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let (a, v) = match (args.first(), args.get(1)) {
        (Some(Value::Ref(Some(a))), Some(v)) => (a, v.expect_int()),
        _ => return Err(null_arg("Arrays.fill")),
    };
    let len = arr_elems(a)?;
    for i in 0..len {
        arr_store(a, i, Value::Int(v), _ctx)?;
    }
    Ok(Value::Void)
}
/// `Arrays.fill(char[], char)` (corpus wave 1: minimal-json's number
/// formatting pads char buffers).
pub fn n_arrays_fill_char(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let (a, v) = match (args.first(), args.get(1)) {
        (Some(Value::Ref(Some(a))), Some(v)) => (a, v.expect_int()),
        _ => return Err(null_arg("Arrays.fill")),
    };
    let len = arr_elems(a)?;
    for i in 0..len {
        arr_store(a, i, Value::Int(v), _ctx)?;
    }
    Ok(Value::Void)
}
pub fn n_arrays_fill_long(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let (a, v) = match (args.first(), args.get(1)) {
        (Some(Value::Ref(Some(a))), Some(v)) => (a, v.expect_long()),
        _ => return Err(null_arg("Arrays.fill")),
    };
    let len = arr_elems(a)?;
    for i in 0..len {
        arr_store(a, i, Value::Long(v), _ctx)?;
    }
    Ok(Value::Void)
}
pub fn n_arrays_fill_obj(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let (a, v) = match (args.first(), args.get(1)) {
        (Some(Value::Ref(Some(a))), Some(v)) => (a, v.clone()),
        _ => return Err(null_arg("Arrays.fill")),
    };
    let len = arr_elems(a)?;
    for i in 0..len {
        arr_store(a, i, v.clone(), ctx)?;
    }
    Ok(Value::Void)
}
/// Element descriptor of an array object (`[I` -> "I",
/// `[Ljava/lang/String;` -> "java/lang/String").
fn elem_desc(array_jclass: &str) -> &str {
    let elem = &array_jclass[1..];
    if let Some(stripped) = elem.strip_prefix('L').and_then(|s| s.strip_suffix(';')) {
        stripped
    } else {
        elem
    }
}
pub fn n_arrays_copy_of_int(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    copy_of(args, "[I", ctx)
}
pub fn n_arrays_copy_of_long(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    copy_of(args, "[J", ctx)
}
pub fn n_arrays_copy_of_double(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    copy_of(args, "[D", ctx)
}
pub fn n_arrays_copy_of_obj(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    // Both the 2-arg Object[] form and javac's generic 3-arg
    // `copyOf(U[], int, Class<? extends T[]>)` land here; the extra class
    // argument is ignored (the result uses the source's component type).
    copy_of(args, "[Ljava/lang/Object;", ctx)
}
fn copy_of(args: &[Value], _expected: &str, ctx: Dispatch) -> Result<Value, Throw> {
    let src = match args.first() {
        Some(Value::Ref(Some(r))) => r,
        _ => return Err(null_arg("Arrays.copyOf")),
    };
    let new_len = match args.get(1) {
        Some(v) => v.expect_int(),
        None => return Err(null_arg("Arrays.copyOf")),
    };
    if new_len < 0 {
        return Err(throw_new(
            "java/lang/NegativeArraySizeException",
            Some(&new_len.to_string()),
        ));
    }
    let (len, jclass) = (arr_elems(src)?, with_obj(*src, |o| o.jclass.to_string()));
    let desc = elem_desc(&jclass);
    let out = if jclass.starts_with("[L") {
        array::array_new_ref(desc, new_len)?
    } else {
        array::array_new_prim(&jclass, new_len)?
    };
    let common = len.min(new_len);
    for i in 0..common {
        let v = arr_load(src, i)?;
        arr_store(&out, i, v, ctx)?;
    }
    Ok(Value::Ref(Some(out)))
}
pub fn n_arrays_copy_of_range_int(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    copy_of_range(args, ctx)
}
pub fn n_arrays_copy_of_range_obj(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    copy_of_range(args, ctx)
}
fn copy_of_range(args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let src = match args.first() {
        Some(Value::Ref(Some(r))) => r,
        _ => return Err(null_arg("Arrays.copyOfRange")),
    };
    let (from, to) = match (args.get(1), args.get(2)) {
        (Some(f), Some(t)) => (f.expect_int(), t.expect_int()),
        _ => return Err(null_arg("Arrays.copyOfRange")),
    };
    let (len, jclass) = (arr_elems(src)?, with_obj(*src, |o| o.jclass.to_string()));
    if from > to {
        return Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some(&format!("{from} > {to}")),
        ));
    }
    let new_len = to - from;
    let desc = elem_desc(&jclass);
    let out = if jclass.starts_with("[L") {
        array::array_new_ref(desc, new_len)?
    } else {
        array::array_new_prim(&jclass, new_len)?
    };
    // Source positions beyond `len` contribute default values (JDK spec).
    for i in 0..new_len {
        let pos = from + i;
        let v = if pos < len {
            arr_load(src, pos)?
        } else {
            match &jclass as &str {
                "[I" | "[J" | "[Z" | "[C" | "[B" | "[S" => Value::Int(0),
                "[F" => Value::Float(0.0),
                "[D" => Value::Double(0.0),
                _ => Value::Ref(None),
            }
        };
        arr_store(&out, i, v, ctx)?;
    }
    Ok(Value::Ref(Some(out)))
}
pub fn n_arrays_equals_int(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    arrays_equals(args, |a, b| Ok(a.expect_int() == b.expect_int()))
}
pub fn n_arrays_equals_long(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    arrays_equals(args, |a, b| Ok(a.expect_long() == b.expect_long()))
}
pub fn n_arrays_equals_obj(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    arrays_equals(args, |a, b| {
        // Element-wise Object.equals dispatch; null == null is true.
        match (a, b) {
            (Value::Ref(None), Value::Ref(None)) => Ok(true),
            (Value::Ref(Some(x)), Value::Ref(Some(y))) => {
                if x == y {
                    return Ok(true);
                }
                let out = invoke_virtual(
                    y,
                    "java/lang/Object",
                    "equals",
                    "(Ljava/lang/Object;)Z",
                    std::slice::from_ref(a),
                    ctx,
                )?;
                Ok(out.expect_int() != 0)
            }
            _ => Ok(false),
        }
    })
}
fn arrays_equals<F>(args: &[Value], elem_eq: F) -> Result<Value, Throw>
where
    F: Fn(&Value, &Value) -> Result<bool, Throw>,
{
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(Value::Ref(Some(a))), Some(Value::Ref(Some(b)))) => (a, b),
        // equals(null, null) is true; one null is false.
        (Some(Value::Ref(None)), Some(Value::Ref(None))) => return Ok(Value::Int(1)),
        (Some(Value::Ref(_)), Some(Value::Ref(_))) => return Ok(Value::Int(0)),
        _ => return Err(null_arg("Arrays.equals")),
    };
    let (la, lb) = (arr_elems(a)?, arr_elems(b)?);
    if la != lb {
        return Ok(Value::Int(0));
    }
    for i in 0..la {
        if !elem_eq(&arr_load(a, i)?, &arr_load(b, i)?)? {
            return Ok(Value::Int(0));
        }
    }
    Ok(Value::Int(1))
}

// ---------------------------------------------------------------------------
// Comparator helper statics
// ---------------------------------------------------------------------------
/// `Character.compare(a, b) == a - b` per the JDK spec (chars are
/// 0..=0xFFFF, so the difference cannot overflow).
pub fn n_char_compare(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let _ = recv;
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.expect_int(), b.expect_int()),
        _ => return Err(null_arg("Character.compare")),
    };
    Ok(Value::Int(a - b))
}
/// `Double.compare`: the same total order the sort bridge uses
/// (NaN > everything, -0.0 < +0.0).
pub fn n_double_compare(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.expect_double(), b.expect_double()),
        _ => return Err(null_arg("Double.compare")),
    };
    Ok(Value::Int(crate::natives::double_total_cmp(a, b)))
}

// ---------------------------------------------------------------------------
// Arrays.sort family
// ---------------------------------------------------------------------------
pub fn n_arrays_sort_int(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = match args.first() {
        Some(Value::Ref(Some(r))) => r,
        _ => return Err(null_arg("Arrays.sort")),
    };
    let len = arr_elems(a)?;
    let mut vals: Vec<i32> = Vec::with_capacity(len.max(0) as usize);
    for i in 0..len {
        vals.push(arr_load(a, i)?.expect_int());
    }
    vals.sort_unstable(); // total order on i32: output equals the JVM's
    for (i, v) in vals.iter().enumerate() {
        arr_store(a, i as i32, Value::Int(*v), ctx)?;
    }
    Ok(Value::Void)
}
pub fn n_arrays_sort_long(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = match args.first() {
        Some(Value::Ref(Some(r))) => r,
        _ => return Err(null_arg("Arrays.sort")),
    };
    let len = arr_elems(a)?;
    let mut vals: Vec<i64> = Vec::with_capacity(len.max(0) as usize);
    for i in 0..len {
        vals.push(arr_load(a, i)?.expect_long());
    }
    vals.sort_unstable();
    for (i, v) in vals.iter().enumerate() {
        arr_store(a, i as i32, Value::Long(*v), ctx)?;
    }
    Ok(Value::Void)
}
pub fn n_arrays_sort_double(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = match args.first() {
        Some(Value::Ref(Some(r))) => r,
        _ => return Err(null_arg("Arrays.sort")),
    };
    let len = arr_elems(a)?;
    let mut vals: Vec<f64> = Vec::with_capacity(len.max(0) as usize);
    for i in 0..len {
        vals.push(arr_load(a, i)?.expect_double());
    }
    // total_cmp: IEEE total order — -0.0 < +0.0, NaN last — which is
    // exactly Java's double sort order.
    vals.sort_by(f64::total_cmp);
    for (i, v) in vals.iter().enumerate() {
        arr_store(a, i as i32, Value::Double(*v), ctx)?;
    }
    Ok(Value::Void)
}
/// Natural-ordering sort over `Object[]` — stable, via `compareTo`
/// dispatch (javac's bridge methods make the erased
/// `(Ljava/lang/Object;)I` shape resolvable on user classes; builtin
/// boxes carry bridge rows registered alongside this module).
pub fn n_arrays_sort_obj(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = match args.first() {
        Some(Value::Ref(Some(r))) => r,
        _ => return Err(null_arg("Arrays.sort")),
    };
    let len = arr_elems(a)?;
    let mut vals: Vec<Value> = Vec::with_capacity(len.max(0) as usize);
    for i in 0..len {
        vals.push(arr_load(a, i)?);
    }
    merge_sort_stable(&mut vals, &|x, y| compare_to_dispatch(x, y, ctx))?;
    for (i, v) in vals.iter().enumerate() {
        arr_store(a, i as i32, v.clone(), ctx)?;
    }
    Ok(Value::Void)
}
/// `Arrays.sort(Object[], Comparator)` — comparator ordering; a null
/// comparator delegates to the natural-ordering form (the JDK source's
/// first branch, `if (c == null) { sort(a); }` — same law the range row
/// implements).
pub fn n_arrays_sort_obj_cmp(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let cmp = match args.get(1) {
        Some(Value::Ref(Some(c))) => c,
        _ => return n_arrays_sort_obj(recv, args, ctx),
    };
    let a = match args.first() {
        Some(Value::Ref(Some(r))) => r,
        _ => return Err(null_arg("Arrays.sort")),
    };
    let len = arr_elems(a)?;
    let mut vals: Vec<Value> = Vec::with_capacity(len.max(0) as usize);
    for i in 0..len {
        vals.push(arr_load(a, i)?);
    }
    merge_sort_stable(&mut vals, &|x, y| comparator_compare(cmp, x, y, ctx))?;
    for (i, v) in vals.iter().enumerate() {
        arr_store(a, i as i32, v.clone(), ctx)?;
    }
    Ok(Value::Void)
}
/// `a.compareTo(b)` through the ordinary dispatch walk. NPEs exactly like
/// the JDK when an element is null (natural ordering has no null tolerance).
pub(crate) fn compare_to_dispatch(a: &Value, b: &Value, ctx: Dispatch) -> Result<bool, Throw> {
    let (ra, _rb) = match (a, b) {
        (Value::Ref(Some(x)), Value::Ref(Some(y))) => (x, y),
        (Value::Ref(None), _) | (_, Value::Ref(None)) => {
            return Err(crate::npe());
        }
        other => {
            return Err(internal_error(&format!(
                "j2r internal: compareTo on {other:?}"
            )))
        }
    };
    let out = invoke_virtual(
        ra,
        "java/lang/Comparable",
        "compareTo",
        "(Ljava/lang/Object;)I",
        std::slice::from_ref(b),
        ctx,
    )?;
    Ok(out.expect_int() < 0)
}
/// `cmp.compare(a, b)` on a M3 lambda/builtin Comparator object.
pub(crate) fn comparator_compare(
    cmp: &JRef,
    a: &Value,
    b: &Value,
    ctx: Dispatch,
) -> Result<bool, Throw> {
    let out = invoke_virtual(
        cmp,
        "java/util/Comparator",
        "compare",
        "(Ljava/lang/Object;Ljava/lang/Object;)I",
        &[(a).clone(), (b).clone()],
        ctx,
    )?;
    Ok(out.expect_int() < 0)
}

// ---------------------------------------------------------------------------
// Collections (sort / reverse / swap) — dispatch-based, works for any List
// ---------------------------------------------------------------------------
/// `Arrays.sort(Object[], int fromIndex, int toIndex)` — natural ordering
/// on the slice, stable merge (the same law as the whole-array form).
pub fn n_arrays_sort_obj_range(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let a = match args.first() {
        Some(Value::Ref(Some(r))) => r,
        _ => return Err(null_arg("Arrays.sort")),
    };
    let (from, to) = match (args.get(1), args.get(2)) {
        (Some(f), Some(t)) => (f.expect_int(), t.expect_int()),
        _ => return Err(null_arg("Arrays.sort")),
    };
    let len = arr_elems(a)?;
    arrays_range_check(len, from, to)?;
    let mut vals: Vec<Value> = Vec::with_capacity((to - from).max(0) as usize);
    for i in from..to {
        vals.push(arr_load(a, i)?);
    }
    merge_sort_stable(&mut vals, &|x, y| compare_to_dispatch(x, y, ctx))?;
    for (i, v) in vals.iter().enumerate() {
        arr_store(a, from + i as i32, v.clone(), ctx)?;
    }
    Ok(Value::Void)
}
/// `Arrays.sort(Object[], int, int, Comparator)` — comparator ordering;
/// a null comparator delegates to the natural-ordering form (JDK-exact).
pub fn n_arrays_sort_obj_range_cmp(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    match args.get(3) {
        Some(Value::Ref(Some(cmp))) => {
            let a = match args.first() {
                Some(Value::Ref(Some(r))) => r,
                _ => return Err(null_arg("Arrays.sort")),
            };
            let (from, to) = match (args.get(1), args.get(2)) {
                (Some(f), Some(t)) => (f.expect_int(), t.expect_int()),
                _ => return Err(null_arg("Arrays.sort")),
            };
            let len = arr_elems(a)?;
            arrays_range_check(len, from, to)?;
            let mut vals: Vec<Value> = Vec::with_capacity((to - from).max(0) as usize);
            for i in from..to {
                vals.push(arr_load(a, i)?);
            }
            merge_sort_stable(&mut vals, &|x, y| comparator_compare(cmp, x, y, ctx))?;
            for (i, v) in vals.iter().enumerate() {
                arr_store(a, from + i as i32, v.clone(), ctx)?;
            }
            Ok(Value::Void)
        }
        // null comparator: natural ordering (the JDK's exact fallback)
        _ => n_arrays_sort_obj_range(recv, args, ctx),
    }
}
/// `Class.desiredAssertionStatus()` — assertions are not a j2r feature,
/// so the JDK default (assertions disabled: false) is the honest answer.
pub fn n_class_desired_assertion_status(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(0))
}
/// `Comparator.reverseOrder()` — a ReverseComparator builtin object
/// (OpenJDK java.util.Comparators.ReverseComparator: compare(a,b) is
/// `((Comparable<U>) b).compareTo(a)` — the arguments swap, the NPE law
/// follows compareTo's).
pub fn n_comparator_reverse_order(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = object::new_builtin("java/util/Comparators$ReverseComparator")?;
    Ok(Value::Ref(Some(r)))
}
/// ReverseComparator.compare(a, b) — b.compareTo(a) through the ordinary
/// dispatch walk (compare_to_dispatch already pins the null law).
pub fn n_reverse_comparator_compare(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = match args.first() {
        Some(v) => v,
        None => return Err(null_arg("ReverseComparator.compare")),
    };
    let b = match args.get(1) {
        Some(v) => v,
        None => return Err(null_arg("ReverseComparator.compare")),
    };
    // b.compareTo(a): dispatch compareTo on b with a as the argument
    if let (Value::Ref(None), _) | (_, Value::Ref(None)) = (b, a) {
        return Err(crate::npe());
    }
    let out = invoke_virtual(
        match b {
            Value::Ref(Some(x)) => x,
            other => {
                return Err(internal_error(&format!(
                    "j2r internal: ReverseComparator on {other:?}"
                )))
            }
        },
        "java/lang/Comparable",
        "compareTo",
        "(Ljava/lang/Object;)I",
        std::slice::from_ref(a),
        ctx,
    )?;
    Ok(Value::Int(out.expect_int()))
}
