//! Real-program JDK surface (post-M3 review wave): `java.util.Arrays`,
//! `java.util.Collections` (sort/reverse/swap) and `ArrayList.subList`
//! with real view semantics.
//!
//! These exist because real programs use them on nearly every page, and
//! the real-program fixture tier (BigJson/BigHuffman/BigRpg/BigLang/
//! BigCache) showed exactly where the loud-unsupported wall sat. Design
//! notes:
//!
//! * **Object sorts are STABLE** (merge sort), matching TimSort's
//!   observable guarantee: equal elements keep their relative order. A
//!   quicksort would produce the same *set* but a different *order* for
//!   equal keys, and the differential suite diffs byte-for-byte.
//! * Comparisons dispatch through the ordinary call path (generated
//!   registry + native walk), so lambda comparators (M3 SAM objects) and
//!   `compareTo` bridges just work.
//! * `subList` is a live view, not a copy: it reads/writes through the
//!   parent list, keeps its own `expectedModCount` synced to the parent's,
//!   and throws `ConcurrentModificationException` (null message, like
//!   Temurin) when the parent mutates structurally behind the view's back.
//! * Everything here is single-threaded by the M3 boundary; contention
//!   semantics arrive with the M4 threads milestone.

use crate::array;
use crate::heap::{with_obj, with_obj_mut, JRef};
use crate::natives;
use crate::object;
use crate::{collections, invoke_virtual, throw_new, Dispatch, Throw, Value};
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn arr_elems(r: &JRef) -> Result<i32, Throw> {
    array::array_length(r)
}

fn arr_load(r: &JRef, i: i32) -> Result<Value, Throw> {
    array::array_load(r, i)
}

fn arr_store(r: &JRef, i: i32, v: Value, ctx: Dispatch) -> Result<(), Throw> {
    array::array_store(r, i, v, ctx)
}

/// Strings in this runtime come back two ways: inline `Value::Str` payloads
/// and heap `java/lang/String` objects. Accept both, reject the rest loudly.
fn string_of_value(v: &Value, meth: &str) -> Result<Rc<String>, Throw> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        Value::Ref(Some(r)) => {
            let cls = with_obj(*r, |o| o.jclass.to_string());
            if cls == "java/lang/String" {
                Ok(object::string_value(r))
            } else {
                Err(crate::internal_error(&format!(
                    "j2r internal: {meth} returned a {cls}, not a String"
                )))
            }
        }
        other => Err(crate::internal_error(&format!(
            "j2r internal: {meth} returned {other:?}, not a String"
        ))),
    }
}

fn null_arg(meth: &str) -> Throw {
    crate::internal_error(&format!("j2r internal: {meth} missing argument"))
}

/// Stable merge sort over a `Vec<Value>` with a comparison closure.
/// `less(a, b)` must answer "a strictly before b". Stability: on ties the
/// element from `left` wins, which preserves the input order of equals.
fn merge_sort_stable<F>(v: &mut [Value], less: &F) -> Result<(), Throw>
where
    F: Fn(&Value, &Value) -> Result<bool, Throw>,
{
    let n = v.len();
    if n < 2 {
        return Ok(());
    }
    let mut buf: Vec<Value> = v.to_vec();
    let mut width = 1usize;
    while width < n {
        let mut i = 0usize;
        while i < n {
            let mid = (i + width).min(n);
            let end = (i + 2 * width).min(n);
            let (mut l, mut r, mut k) = (i, mid, i);
            while l < mid && r < end {
                // Strictly-less keeps the left run winning ties (stable).
                if less(&buf[r], &buf[l])? {
                    v[k] = buf[r].clone();
                    r += 1;
                } else {
                    v[k] = buf[l].clone();
                    l += 1;
                }
                k += 1;
            }
            while l < mid {
                v[k] = buf[l].clone();
                l += 1;
                k += 1;
            }
            while r < end {
                v[k] = buf[r].clone();
                r += 1;
                k += 1;
            }
            i += 2 * width;
        }
        buf.clone_from_slice(v);
        width *= 2;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Arrays.toString family
// ---------------------------------------------------------------------------

fn join_array(r: &JRef, fmt: &dyn Fn(&Value) -> Result<String, Throw>) -> Result<String, Throw> {
    let len = arr_elems(r)?;
    let mut sb = String::from("[");
    for i in 0..len {
        if i > 0 {
            sb.push_str(", ");
        }
        sb.push_str(&fmt(&arr_load(r, i)?)?);
    }
    sb.push(']');
    Ok(sb)
}

fn v_to_string(v: &Value, ctx: Dispatch) -> Result<String, Throw> {
    match v {
        Value::Str(s) => Ok(s.to_string()),
        Value::Ref(None) => Ok("null".to_string()),
        Value::Ref(Some(r)) => {
            // Object.toString via the ordinary dispatch walk (boxes have
            // native rows; user classes have compiled toString).
            let out = invoke_virtual(
                r,
                "java/lang/Object",
                "toString",
                "()Ljava/lang/String;",
                &[],
                ctx,
            )?;
            Ok(string_of_value(&out, "Object.toString")?.to_string())
        }
        other => Err(crate::internal_error(&format!(
            "j2r internal: unexpected element {other:?}"
        ))),
    }
}

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

pub fn n_arrays_sort_obj_cmp(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let (a, cmp) = match (args.first(), args.get(1)) {
        (Some(Value::Ref(Some(r))), Some(Value::Ref(Some(c)))) => (r, c),
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
fn compare_to_dispatch(a: &Value, b: &Value, ctx: Dispatch) -> Result<bool, Throw> {
    let (ra, _rb) = match (a, b) {
        (Value::Ref(Some(x)), Value::Ref(Some(y))) => (x, y),
        (Value::Ref(None), _) | (_, Value::Ref(None)) => {
            return Err(throw_new("java/lang/NullPointerException", None));
        }
        other => {
            return Err(crate::internal_error(&format!(
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
fn comparator_compare(cmp: &JRef, a: &Value, b: &Value, ctx: Dispatch) -> Result<bool, Throw> {
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

/// Reads a List's elements via `get` dispatch (the JDK's own sort does the
/// equivalent through a ListIterator).
fn list_to_vec(list: &JRef, ctx: Dispatch) -> Result<Vec<Value>, Throw> {
    let n = list_size(list, ctx)?;
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        out.push(list_get(list, i, ctx)?);
    }
    Ok(out)
}

fn list_size(list: &JRef, ctx: Dispatch) -> Result<i32, Throw> {
    let out = invoke_virtual(list, "java/util/List", "size", "()I", &[], ctx)?;
    Ok(out.expect_int())
}

fn list_get(list: &JRef, i: i32, ctx: Dispatch) -> Result<Value, Throw> {
    invoke_virtual(
        list,
        "java/util/List",
        "get",
        "(I)Ljava/lang/Object;",
        &[Value::Int(i)],
        ctx,
    )
}

fn list_set(list: &JRef, i: i32, v: &Value, ctx: Dispatch) -> Result<(), Throw> {
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
            return Err(throw_new("java/lang/NullPointerException", None));
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
            return Err(throw_new("java/lang/NullPointerException", None));
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
            return Err(throw_new("java/lang/NullPointerException", None));
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
        return Err(crate::internal_error(&format!(
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
        jclass: Rc::from("java/util/ArrayList$SubList"),
        slots: vec![
            Value::Ref(Some(*list)),
            Value::Int(from),
            Value::Int(to - from),
            Value::Int(mod_count),
        ],
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
        return Err(throw_new("java/util/ConcurrentModificationException", None));
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
                return Err(crate::internal_error(&format!(
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

// `Collections.unmodifiableList` is intentionally absent — a wrapping view
// class is easy to add once a fixture needs it; until then it stays a loud
// compile error rather than a silently mutable stand-in.
