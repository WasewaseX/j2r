use super::*;

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
