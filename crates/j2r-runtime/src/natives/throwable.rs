use super::*;

pub(crate) fn n_throwable_init(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.<init>");
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = Value::Ref(None);
        o.slots[1] = Value::Ref(None);
        o.slots[2] = Value::Ref(None);
    });
    Ok(Value::Void)
}
pub(crate) fn n_throwable_init_s(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.<init>");
    let msg = match arg(args, "Throwable.<init>") {
        Value::Ref(None) => Value::Ref(None),
        Value::Ref(Some(o)) if is_string(o) => Value::Ref(Some(*o)),
        other => return Err(internal_error(&format!("Throwable.<init> arg {other:?}"))),
    };
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = msg;
        o.slots[1] = Value::Ref(None);
        o.slots[2] = Value::Ref(None);
    });
    Ok(Value::Void)
}
/// `Throwable(String message, Throwable cause)` — the message is used
/// verbatim (unlike the `(Throwable)` constructor, which derives one).
pub(crate) fn n_throwable_init_msg_cause(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.<init>");
    if args.len() != 2 {
        return Err(internal_error("Throwable.<init> arity"));
    }
    let msg = match &args[0] {
        Value::Ref(None) => Value::Ref(None),
        Value::Ref(Some(o)) if is_string(o) => Value::Ref(Some(*o)),
        other => return Err(internal_error(&format!("Throwable.<init> arg {other:?}"))),
    };
    let cause = match &args[1] {
        Value::Ref(c) => *c,
        other => return Err(internal_error(&format!("Throwable.<init> arg {other:?}"))),
    };
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = msg;
        o.slots[1] = Value::Ref(cause);
        o.slots[2] = Value::Ref(None);
    });
    Ok(Value::Void)
}
/// `Throwable(Throwable cause)` — JVM semantics: when `cause != null` the
/// detail message is `cause.toString()`, computed through real virtual
/// dispatch so a user `toString` override wins (OpenJDK Throwable.java).
pub(crate) fn n_throwable_init_cause(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.<init>");
    let cause = match arg(args, "Throwable.<init>") {
        Value::Ref(c) => *c,
        other => return Err(internal_error(&format!("Throwable.<init> arg {other:?}"))),
    };
    let msg: Value = match cause {
        None => Value::Ref(None),
        Some(c) => {
            let _nr = crate::frame::NativeRoots::push(&[c]);
            match crate::invoke_virtual(
                &c,
                "java/lang/Object",
                "toString",
                "()Ljava/lang/String;",
                &[],
                ctx,
            ) {
                Ok(Value::Ref(Some(s))) => Value::Ref(Some(s)),
                _ => Value::Ref(None),
            }
        }
    };
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = msg;
        o.slots[1] = Value::Ref(cause);
        o.slots[2] = Value::Ref(None);
    });
    Ok(Value::Void)
}
pub(crate) fn n_throwable_get_message(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.getMessage");
    Ok(crate::heap::with_obj(*r, |o| o.slots[0].clone()))
}

// EXTERNAL-B7 (jbox2d port, 2026-11-13): AssertionError's primitive and
// Object constructors. OpenJDK AssertionError.java: every constructor
// delegates `super(String.valueOf(detailMessage))` — so the message is the
// String.valueOf rendering of the argument (float/double through
// Float/Double.toString, Object through real virtual toString dispatch).
// The (Object) form: Temurin 21.0.13 pins `new
// AssertionError((Object) null).toString()` = "java.lang.AssertionError:
// null" — the old reset-the-message-to-null branch is gone upstream, so a
// null argument stores the MESSAGE "null" (getMessage() is the string
// "null", not Java null). The primitive forms can never produce a null
// message. (() and (String) already resolve through the Throwable rows
// above via the parent-chain dispatch; (String, Throwable) rides
// n_throwable_init_msg_cause the same way.)
/// String.valueOf rendering for the primitive constructor arguments
/// (int/long/float/double share one native, matched by value shape).
/// Booleans and chars arrive as Value::Int, so they get their own natives
/// below — the descriptor is the only thing that tells them apart.
fn assertion_error_message(v: &Value, ctx: Dispatch) -> Result<Value, Throw> {
    let text = match v {
        Value::Int(i) => i.to_string(),
        Value::Long(l) => l.to_string(),
        Value::Float(f) => crate::javafmt::float_to_string(*f),
        Value::Double(d) => crate::javafmt::double_to_string(*d),
        // String.valueOf law: null -> "null"; anything else renders through
        // real toString dispatch so a user override wins (the same law
        // n_throwable_init_cause states). Temurin 21.0.13 pinned (the
        // Externals fixture): AssertionError((Object) null).toString() is
        // "java.lang.AssertionError: null" — the old
        // reset-detailMessage-to-null branch is gone upstream, so a null
        // argument leaves the MESSAGE "null", unlike Throwable(String).
        Value::Ref(_) => {
            if let Value::Ref(Some(o)) = v {
                let _nr = crate::frame::NativeRoots::push(&[*o]);
            }
            concat::format_value(v, 'L', ctx)?
        }
        other => {
            return Err(internal_error(&format!(
                "AssertionError.<init> arg {other:?}"
            )))
        }
    };
    Ok(Value::Ref(Some(object::new_string(&text))))
}
/// Stores the message + the two null Throwable slots (the same shape
/// n_throwable_init_s writes).
fn set_throwable_message(r: &JRef, msg: Value) {
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = msg;
        o.slots[1] = Value::Ref(None);
        o.slots[2] = Value::Ref(None);
    });
}
/// `AssertionError(int|long|float|double)` — String.valueOf rendering.
pub(crate) fn n_assertion_error_init(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "AssertionError.<init>");
    let msg = assertion_error_message(arg(args, "AssertionError.<init>"), ctx)?;
    set_throwable_message(r, msg);
    Ok(Value::Void)
}
/// `AssertionError(boolean)` — String.valueOf(Z): "true"/"false".
pub(crate) fn n_assertion_error_init_z(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "AssertionError.<init>");
    let b = arg(args, "AssertionError.<init>").expect_int();
    let msg = Value::Ref(Some(object::new_string(if b != 0 {
        "true"
    } else {
        "false"
    })));
    set_throwable_message(r, msg);
    Ok(Value::Void)
}
/// `AssertionError(char)` — String.valueOf(C): the one-character string.
pub(crate) fn n_assertion_error_init_c(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "AssertionError.<init>");
    let i = arg(args, "AssertionError.<init>").expect_int();
    let ch = char::from_u32(i as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
    let msg = Value::Ref(Some(object::new_string(&ch.to_string())));
    set_throwable_message(r, msg);
    Ok(Value::Void)
}
/// `AssertionError(Object)` — String.valueOf rendering; a null argument
/// stores the MESSAGE "null" (Temurin 21.0.13-pinned; see the
/// assertion_error_message note), so it shares the generic native.
pub(crate) fn n_assertion_error_init_obj(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    n_assertion_error_init(recv, args, ctx)
}
pub(crate) fn n_throwable_get_cause(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.getCause");
    Ok(crate::heap::with_obj(*r, |o| o.slots[1].clone()))
}
/// Snapshot of the suppressed list as a fresh `Throwable[]` (empty when
/// nothing was suppressed — `getSuppressed` never returns null, probed on
/// Temurin: empty-len=0).
pub(crate) fn n_throwable_get_suppressed(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.getSuppressed");
    let cur = crate::heap::with_obj(*r, |o| match &o.slots[2] {
        Value::Ref(x) => *x,
        other => panic!("j2r internal: Throwable slot 2 is {other:?}"),
    });
    let out = match cur {
        None => crate::array::array_new_ref("java/lang/Throwable", 0)?,
        Some(a) => {
            let len = crate::array::array_length(&a)?;
            let out = crate::array::array_new_ref("java/lang/Throwable", len)?;
            for i in 0..len {
                let v = crate::array::array_load(&a, i)?;
                crate::array::array_store(&out, i, v, ctx)?;
            }
            out
        }
    };
    Ok(Value::Ref(Some(out)))
}
/// `addSuppressed`: append to the snapshot list, growing a fresh
/// `Throwable[]` each time (no GC can run mid-native, so the read-copy-
/// write is race-free and keeps every intermediate array reachable from
/// slot 2 until the final store).
pub(crate) fn n_throwable_add_suppressed(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.addSuppressed");
    let exc = match arg(args, "Throwable.addSuppressed") {
        Value::Ref(x) => *x,
        other => {
            return Err(internal_error(&format!(
                "Throwable.addSuppressed arg {other:?}"
            )))
        }
    };
    // OpenJDK order: the self check fires before the null check.
    match exc {
        Some(e) if *r == e => {
            return Err(throw_new(
                "java/lang/IllegalArgumentException",
                Some("Self-suppression not permitted"),
            ))
        }
        None => {
            return Err(throw_new(
                "java/lang/NullPointerException",
                Some("Cannot suppress a null exception."),
            ))
        }
        Some(_) => {}
    }
    let old = crate::heap::with_obj(*r, |o| match &o.slots[2] {
        Value::Ref(x) => *x,
        other => panic!("j2r internal: Throwable slot 2 is {other:?}"),
    });
    let len = match old {
        Some(a) => crate::array::array_length(&a)?,
        None => 0,
    };
    let out = crate::array::array_new_ref("java/lang/Throwable", len + 1)?;
    if let Some(a) = old {
        for i in 0..len {
            let v = crate::array::array_load(&a, i)?;
            crate::array::array_store(&out, i, v, ctx)?;
        }
    }
    crate::array::array_store(&out, len, Value::Ref(exc), ctx)?;
    crate::heap::with_obj_mut(*r, |o| o.slots[2] = Value::Ref(Some(out)));
    Ok(Value::Void)
}
pub(crate) fn n_throwable_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.toString");
    let cls = dotted(&crate::heap::with_obj(*r, |o| o.jclass.clone()));
    let msg = match &crate::heap::with_obj(*r, |o| o.slots[0].clone()) {
        Value::Ref(Some(o)) if is_string(o) => Some(object::string_value(o).to_string()),
        _ => None,
    };
    let text = match msg {
        Some(m) => format!("{cls}: {m}"),
        None => cls,
    };
    Ok(Value::Ref(Some(object::new_string(&text))))
}

// ---------------------------------------------------------------- Boxed types
//
// Every boxed class stores its payload in slot 0. valueOf boxes, xValue
// unboxes, toString renders with Java's formatting rules.
//
// JLS 7 §5.1.7 requires `valueOf` to return **cached, identical** objects
// for Integer/Long/Short/Byte in [-128, 127], Character in [0, 127] and
// Boolean (TRUE/FALSE); `==` on boxed values inside those ranges must be
// true. Float and Double are exempt (fresh object each time). Boxed
// `equals` compares payloads and rejects mixed types; `hashCode` follows
// the JDK contract (Long folds high/low, Boolean 1231/1237, Float/Double
// use canonicalized IEEE bits).
