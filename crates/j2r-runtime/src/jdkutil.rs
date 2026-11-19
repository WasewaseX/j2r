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
use crate::internal_error;
use crate::natives;
use crate::object;
use crate::{collections, invoke_virtual, throw_new, Dispatch, Throw, Value};
use std::sync::Arc;

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
fn string_of_value(v: &Value, meth: &str) -> Result<Arc<String>, Throw> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        Value::Ref(Some(r)) => {
            let cls = with_obj(*r, |o| o.jclass.to_string());
            if cls == "java/lang/String" {
                Ok(object::string_value(r))
            } else {
                Err(internal_error(&format!(
                    "j2r internal: {meth} returned a {cls}, not a String"
                )))
            }
        }
        other => Err(internal_error(&format!(
            "j2r internal: {meth} returned {other:?}, not a String"
        ))),
    }
}
fn null_arg(meth: &str) -> Throw {
    internal_error(&format!("j2r internal: {meth} missing argument"))
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
        other => Err(internal_error(&format!(
            "j2r internal: unexpected element {other:?}"
        ))),
    }
}

// --- scanaislop split: the items below live verbatim in the child modules ---
mod arraylist;
mod arrays;
mod sublist;
mod unmod;
pub(crate) use arraylist::*;
pub(crate) use arrays::*;
pub(crate) use sublist::*;
pub(crate) use unmod::*;
