//! Lambda closures (M3 Wave 3, M3_PLAN §6).
//!
//! A closure is a heap object whose class **is** the functional interface it
//! satisfies (e.g. `java/util/function/Function`). That makes every subtype
//! check and every interface dispatch exact with zero new machinery: the
//! `invokeinterface Function.apply` walk starts at the receiver's class,
//! which is the interface, and hits the native SAM row registered below.
//!
//! Slots (owned by this module and the natives SAM entry):
//! * slot 0 — shape (see [`SHAPE_STATIC`]..[`SHAPE_CTOR`]),
//! * slot 1 — target class (`Value::Str`),
//! * slot 2 — target method name (`Value::Str`),
//! * slot 3 — target descriptor (`Value::Str`),
//! * slot 4 — instance-slot count for constructor targets (`Value::Int`),
//! * slots 5.. — the captured values, in capture order (JLS 15.27.2:
//!   capture-by-value; a later mutation of the original local is not
//!   observed, pinned differentially by `LambdaCapture`).

use crate::desc::{parse_method_desc, TypeDesc};
use crate::heap::{self, JRef};
use crate::natives;
use crate::object::{new_obj, JObj};
use crate::{throw_new, Dispatch, Throw, Value};
use std::rc::Rc;

pub const SHAPE_STATIC: u8 = 0; // caps ++ sam_args, no receiver
pub const SHAPE_BOUND: u8 = 1; // receiver = capture 0
pub const SHAPE_UNBOUND: u8 = 2; // receiver = first SAM argument
pub const SHAPE_CTOR: u8 = 3; // construct target, call <init>

const SLOTS_BEFORE_CAPTURES: usize = 5;

/// Builds the closure object emitted at an `invokedynamic` site.
#[allow(clippy::too_many_arguments)]
pub fn make_closure(
    shape: u8,
    interface: &str,
    target_class: &str,
    target_name: &str,
    target_desc: &str,
    ctor_slots: u32,
    captures: &[Value],
) -> Result<Value, Throw> {
    let mut slots = Vec::with_capacity(SLOTS_BEFORE_CAPTURES + captures.len());
    slots.push(Value::Int(shape as i32));
    slots.push(Value::Str(Rc::new(target_class.to_string())));
    slots.push(Value::Str(Rc::new(target_name.to_string())));
    slots.push(Value::Str(Rc::new(target_desc.to_string())));
    slots.push(Value::Int(ctor_slots as i32));
    slots.extend_from_slice(captures);
    Ok(Value::Ref(Some(heap::alloc(JObj {
        jclass: Rc::from(interface),
        slots,
    }))))
}

/// The single native implementation behind every M3 functional-interface
/// SAM row: reassembles the recorded call, performs the SAM↔impl type
/// adaptation LambdaMetafactory would do at link time (unbox boxed SAM
/// arguments to primitive impl parameters, box primitive impl results to
/// the erased `Object` SAM return), and dispatches through the ordinary
/// registry/native walk.
pub fn n_sam(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: lambda SAM without receiver");
    let (shape, iface, tcls, tname, tdesc, ctor_slots, caps) = heap::with_obj(*r, |o| {
        let string_of = |i: usize| match &o.slots[i] {
            Value::Str(s) => s.as_str().to_string(),
            other => panic!("j2r internal: lambda slot {i} is {other:?}, expected Str"),
        };
        let caps: Vec<Value> = o.slots[SLOTS_BEFORE_CAPTURES..].to_vec();
        (
            o.slots[0].expect_int() as u8,
            o.jclass.to_string(),
            string_of(1),
            string_of(2),
            string_of(3),
            o.slots[4].expect_int() as u32,
            caps,
        )
    });
    let (impl_params, impl_ret) = match parse_method_desc(&tdesc) {
        Some(pair) => pair,
        None => {
            return Err(crate::internal_error(&format!(
                "j2r internal: bad closure target descriptor {tdesc:?}"
            )))
        }
    };
    match shape {
        SHAPE_STATIC => {
            let n_caps = caps.len();
            let mut full = caps;
            full.extend_from_slice(args);
            adapt_args(&mut full, &impl_params, n_caps, ctx)?;
            let out = call_target(ctx, &tcls, &tname, &tdesc, None, &full)?;
            adapt_result(out, &iface, &impl_ret, ctx)
        }
        SHAPE_BOUND => {
            let recv_ref = receiver_ref(&caps[0])?;
            let mut full = caps[1..].to_vec();
            full.extend_from_slice(args);
            // The impl descriptor holds exactly the (non-receiver) SAM
            // arguments for bound shapes — validated at compile time — so
            // every entry adapts.
            adapt_args(&mut full, &impl_params, 0, ctx)?;
            let out = call_target(ctx, &tcls, &tname, &tdesc, Some(&recv_ref), &full)?;
            adapt_result(out, &iface, &impl_ret, ctx)
        }
        SHAPE_UNBOUND => {
            let first = args.first().cloned().unwrap_or(Value::Ref(None));
            let recv_ref = receiver_ref(&first)?;
            let mut full = args[1..].to_vec();
            // impl params beyond the receiver align 1:1 with the remaining
            // SAM arguments (compile-time arity rule).
            let skip_recv = impl_params.len().saturating_sub(full.len());
            adapt_args(
                &mut full,
                &impl_params[skip_recv.min(impl_params.len())..],
                0,
                ctx,
            )?;
            let out = call_target(ctx, &tcls, &tname, &tdesc, Some(&recv_ref), &full)?;
            adapt_result(out, &iface, &impl_ret, ctx)
        }
        SHAPE_CTOR => {
            let obj = new_obj(&tcls, ctor_slots as usize)?;
            let n_caps = caps.len();
            let mut full = caps;
            full.extend_from_slice(args);
            adapt_args(&mut full, &impl_params, n_caps, ctx)?;
            call_target(ctx, &tcls, "<init>", &tdesc, Some(&obj), &full)?;
            Ok(Value::Ref(Some(obj)))
        }
        other => Err(crate::internal_error(&format!(
            "j2r internal: unknown closure shape {other}"
        ))),
    }
}

/// Unboxes every value whose impl parameter is a primitive while the value
/// arrives boxed (SAM arguments of the generic interfaces are always
/// erased `Object`). Values already in raw form (primitive captures) pass
/// through unchanged.
fn adapt_args(
    full: &mut [Value],
    impl_params: &[TypeDesc],
    from: usize,
    ctx: Dispatch,
) -> Result<(), Throw> {
    let n = full.len().min(impl_params.len());
    for i in from..n {
        let prim = match &impl_params[i] {
            TypeDesc::Int => Some('I'),
            TypeDesc::Long => Some('J'),
            TypeDesc::Float => Some('F'),
            TypeDesc::Double => Some('D'),
            TypeDesc::Char => Some('C'),
            TypeDesc::Boolean => Some('Z'),
            TypeDesc::Byte => Some('B'),
            TypeDesc::Short => Some('S'),
            _ => None,
        };
        if let Some(p) = prim {
            if matches!(full[i], Value::Ref(_)) {
                full[i] = unbox(&full[i], p, ctx)?;
            }
        }
    }
    Ok(())
}

/// Boxes a primitive impl result when the SAM's erased return is `Object`
/// (the generic Function family). `Predicate` (Z), `Consumer` (V) and
/// `Comparator` (I) SAMs return primitives at the call site and need no
/// adaptation.
fn adapt_result(
    out: Value,
    iface: &str,
    impl_ret: &TypeDesc,
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let object_sam = !matches!(
        iface,
        "java/util/function/Consumer"
            | "java/util/function/BiConsumer"
            | "java/util/function/Predicate"
            | "java/util/function/BiPredicate"
            | "java/util/Comparator"
    );
    if !object_sam {
        return Ok(out);
    }
    let prim = match impl_ret {
        TypeDesc::Int => 'I',
        TypeDesc::Long => 'J',
        TypeDesc::Float => 'F',
        TypeDesc::Double => 'D',
        TypeDesc::Char => 'C',
        TypeDesc::Boolean => 'Z',
        TypeDesc::Byte => 'B',
        TypeDesc::Short => 'S',
        _ => return Ok(out),
    };
    box_value(out, prim)
}

/// Unbox helpers: the SAM argument is a boxed value; the impl wants the
/// primitive.
fn unbox(v: &Value, prim: char, ctx: Dispatch) -> Result<Value, Throw> {
    let r = match v {
        Value::Ref(Some(r)) => *r,
        Value::Ref(None) => return Err(throw_new("java/lang/NullPointerException", None)),
        other => {
            return Err(crate::internal_error(&format!(
                "j2r internal: unboxing non-reference {other:?} as {prim}"
            )))
        }
    };
    let (cls, m, d): (&str, &str, &str) = match prim {
        'I' => ("java/lang/Integer", "intValue", "()I"),
        'J' => ("java/lang/Long", "longValue", "()J"),
        'F' => ("java/lang/Float", "floatValue", "()F"),
        'D' => ("java/lang/Double", "doubleValue", "()D"),
        'C' => ("java/lang/Character", "charValue", "()C"),
        'Z' => ("java/lang/Boolean", "booleanValue", "()Z"),
        'B' => ("java/lang/Byte", "byteValue", "()B"),
        'S' => ("java/lang/Short", "shortValue", "()S"),
        _ => unreachable!("primitive kind {prim} has no unbox native"),
    };
    let _ = ctx;
    natives::invoke_exact(cls, m, d, Some(&r), &[], Dispatch::default())
}

/// Box helper: the impl returned a raw primitive; the erased SAM return is
/// `Object`, so the boxed form is required (with the M1.6 identity cache).
fn box_value(v: Value, prim: char) -> Result<Value, Throw> {
    let (cls, m, d): (&str, &str, &str) = match prim {
        'I' => ("java/lang/Integer", "valueOf", "(I)Ljava/lang/Integer;"),
        'J' => ("java/lang/Long", "valueOf", "(J)Ljava/lang/Long;"),
        'F' => ("java/lang/Float", "valueOf", "(F)Ljava/lang/Float;"),
        'D' => ("java/lang/Double", "valueOf", "(D)Ljava/lang/Double;"),
        'C' => ("java/lang/Character", "valueOf", "(C)Ljava/lang/Character;"),
        'Z' => ("java/lang/Boolean", "valueOf", "(Z)Ljava/lang/Boolean;"),
        'B' => ("java/lang/Byte", "valueOf", "(B)Ljava/lang/Byte;"),
        'S' => ("java/lang/Short", "valueOf", "(S)Ljava/lang/Short;"),
        _ => unreachable!("primitive kind {prim} has no box native"),
    };
    natives::invoke_exact(cls, m, d, None, &[v], Dispatch::default())
}

/// Extracts the receiver `JRef` from a value, throwing NPE on null exactly
/// like a JVM method invocation on a null receiver.
fn receiver_ref(v: &Value) -> Result<JRef, Throw> {
    match v {
        Value::Ref(Some(r)) => Ok(*r),
        _ => Err(throw_new("java/lang/NullPointerException", None)),
    }
}

/// Calls a resolved target: compiled methods through the generated registry,
/// everything else through the natives table (validated at compile time by
/// `lambda::resolve_lambda`).
fn call_target(
    ctx: Dispatch,
    cls: &str,
    name: &str,
    desc: &str,
    recv: Option<&JRef>,
    args: &[Value],
) -> Result<Value, Throw> {
    if let Some(f) = ctx.find_method(cls, name, desc) {
        return f(recv, args);
    }
    natives::invoke_exact(cls, name, desc, recv, args, ctx)
}
