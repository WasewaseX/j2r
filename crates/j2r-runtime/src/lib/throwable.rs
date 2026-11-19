use super::*;

/// Renders an uncaught throwable the way the JVM does:
/// `Exception in thread "main" <toString>`.
pub fn throwable_display(t: &Throw, ctx: Dispatch) -> String {
    // main_run is not a generated method, so nothing else roots the thrown
    // object while `toString` dispatches into (possibly user) code — scope
    // it explicitly (M2_PLAN §4.4, native→Java re-entry).
    let _nr = crate::frame::NativeRoots::push(&[t.obj]);
    match invoke_virtual(
        &t.obj,
        "java/lang/Object",
        "toString",
        "()Ljava/lang/String;",
        &[],
        ctx,
    ) {
        Ok(Value::Ref(Some(s))) => object::string_value(&s).to_string(),
        _ => t.java_class(),
    }
}
/// Renders the enclosed trace sections of a thrown throwable — the
/// `Suppressed: ...` sections and the `Caused by: ...` chain — in exactly
/// the JVM's `printStackTrace` order (audit A2): every suppressed section
/// of an exception prints before that exception's cause chain; suppressed
/// sections indent one tab deeper per nesting level while cause captions
/// stay at the enclosing prefix; section titles go through real virtual
/// `toString` dispatch so user overrides win, like the JVM.
///
/// Frame lines are not rendered (no `LineNumberTable` support yet); the
/// differential harness normalizes JVM frames away at any tab depth, so
/// the compared bytes are the caption lines only.
pub fn throwable_enclosed_trace(t: &Throw, ctx: Dispatch) -> String {
    let mut out = String::new();
    enclosed_sections(&t.obj, "", ctx, &mut out);
    out
}
/// `toString` for one trace section through real virtual dispatch; falls
/// back to the dotted class name when dispatch fails (same fallback rule
/// as [`throwable_display`]).
fn enclosed_title(obj: &JRef, ctx: Dispatch) -> String {
    let _nr = crate::frame::NativeRoots::push(std::slice::from_ref(obj));
    match invoke_virtual(
        obj,
        "java/lang/Object",
        "toString",
        "()Ljava/lang/String;",
        &[],
        ctx,
    ) {
        Ok(Value::Ref(Some(s))) => object::string_value(&s).to_string(),
        _ => crate::heap::with_obj(*obj, |o| o.jclass.replace('/', ".")),
    }
}
fn throwable_slot(obj: &JRef, slot: usize) -> Option<JRef> {
    crate::heap::with_obj(*obj, |o| match &o.slots[slot] {
        Value::Ref(r) => *r,
        _ => None,
    })
}
fn enclosed_sections(obj: &JRef, prefix: &str, ctx: Dispatch, out: &mut String) {
    // Root everything held across the `toString` dispatches below: the
    // dispatch re-enters generated code, which can reach a safepoint and
    // collect (M2_PLAN §4.4 native→Java rule).
    let sup = throwable_slot(obj, 2);
    let cause = throwable_slot(obj, 1);
    let mut roots: Vec<JRef> = vec![*obj];
    if let Some(a) = &sup {
        roots.push(*a);
    }
    if let Some(c) = &cause {
        roots.push(*c);
    }
    let _nr = crate::frame::NativeRoots::push(&roots);
    if let Some(sup) = sup {
        if let Ok(len) = crate::array::array_length(&sup) {
            for i in 0..len {
                let Ok(v) = crate::array::array_load(&sup, i) else {
                    continue;
                };
                if let Value::Ref(Some(se)) = v {
                    let _gr = crate::frame::NativeRoots::push(&[se]);
                    let title = enclosed_title(&se, ctx);
                    out.push_str(prefix);
                    out.push_str("\tSuppressed: ");
                    out.push_str(&title);
                    out.push('\n');
                    enclosed_sections(&se, &format!("{prefix}\t"), ctx, out);
                }
            }
        }
    }
    if let Some(cause) = cause {
        let title = enclosed_title(&cause, ctx);
        out.push_str(prefix);
        out.push_str("Caused by: ");
        out.push_str(&title);
        out.push('\n');
        enclosed_sections(&cause, prefix, ctx, out);
    }
}

/// The one control-flow macro generated code needs.
///
/// Evaluates `$e` (a `Result<T, Throw>`). On `Ok(v)` yields `v`; on `Err(t)`
/// records the pending throw with the faulting instruction's pc and continues
/// the enclosing loop so the exception table can be consulted.
///
/// The caller passes its own `thrown` / `fault_pc` bindings explicitly —
/// `macro_rules!` hygiene would otherwise give the macro's local-variable
/// references a fresh, unresolvable scope. The `'run` label is passed
/// explicitly for the same hygiene reason: a label defined at the call
/// site is *not* referenceable inside the macro body.
#[macro_export]
macro_rules! j2r_try {
    ($run:lifetime, $thrown:ident, $fault_pc:ident, $fault:expr, $e:expr) => {
        match $e {
            ::core::result::Result::Ok(v) => v,
            ::core::result::Result::Err(t) => {
                $thrown = ::core::option::Option::Some(t);
                $fault_pc = $fault;
                // Labeled: the lifted body (Wave-4 `--emit-cfg`) wraps arms
                // in labeled *blocks*, and an unlabeled `continue` that
                // diverges through a labeled block is E0695.
                continue $run;
            }
        }
    };
}
