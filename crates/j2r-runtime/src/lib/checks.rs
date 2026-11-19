use super::*;

/// Java integer division: throws `ArithmeticException("/ by zero")` on zero
/// and wraps `MIN_VALUE / -1` exactly like the JVM.
pub fn int_div(a: i32, b: i32) -> Result<i32, Throw> {
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    Ok(a.wrapping_div(b))
}
/// Java integer remainder (sign of the dividend, as in the JVM).
pub fn int_rem(a: i32, b: i32) -> Result<i32, Throw> {
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    Ok(a.wrapping_rem(b))
}
pub fn long_div(a: i64, b: i64) -> Result<i64, Throw> {
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    Ok(a.wrapping_div(b))
}
pub fn long_rem(a: i64, b: i64) -> Result<i64, Throw> {
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    Ok(a.wrapping_rem(b))
}
/// Reference identity (`if_acmpeq`/`if_acmpne`).
pub fn ref_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Ref(x), Value::Ref(y)) => match (x, y) {
            (None, None) => true,
            (Some(x), Some(y)) => JRef::ptr_eq(x, y),
            _ => false,
        },
        _ => panic!("j2r internal: if_acmp on non-reference values"),
    }
}
/// The ClassCastException the faithful `checkcast` throws: the exact
/// message builder, shared with the specialized-map get's fused unbox
/// (G3.3 slice 3) so a foreign box or a mismatched prim tag in a node
/// slot throws the same bytes the faithful `checkcast` would.
pub(crate) fn cce_throw(actual: &str, expect: &str) -> Throw {
    throw_new(
        "java/lang/ClassCastException",
        Some(&format!(
            "class {} cannot be cast to class {}",
            actual.replace('/', "."),
            expect.replace('/', ".")
        )),
    )
}
/// `checkcast`: passes `null` through, otherwise verifies the runtime type.
pub fn checkcast(v: Value, class: &str, ctx: Dispatch) -> Result<Value, Throw> {
    match &v {
        Value::Ref(None) => Ok(v),
        Value::Ref(Some(o)) => {
            let cls = crate::heap::with_obj(*o, |obj| obj.jclass.to_string());
            if ctx.is_subtype(&cls, class) {
                Ok(v)
            } else {
                Err(cce_throw(&cls, class))
            }
        }
        _ => Err(internal_error("checkcast on a primitive value")),
    }
}
/// `instanceof`: `null` is an instance of nothing.
pub fn j2r_instance_of(v: &Value, class: &str, ctx: Dispatch) -> bool {
    match v {
        Value::Ref(None) => false,
        Value::Ref(Some(o)) => {
            let cls = crate::heap::with_obj(*o, |obj| obj.jclass.to_string());
            ctx.is_subtype(&cls, class)
        }
        _ => false,
    }
}
