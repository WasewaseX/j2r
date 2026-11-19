use super::*;

/// Boxes a primitive `Value` according to a primitive field descriptor,
/// routing through the same `valueOf` natives as user code (so the
/// -128..127 identity caches apply — oracle: `f.get(o) == Integer.valueOf(5)`
/// is true on Temurin).
pub(crate) fn box_by_desc(v: Value, desc: &str) -> Result<Value, Throw> {
    let noop_ctx = Dispatch::default();
    match desc {
        "I" => natives::n_integer_value_of(None, &[v], noop_ctx),
        "J" => natives::n_long_value_of(None, &[v], noop_ctx),
        "F" => natives::n_float_value_of(None, &[v], noop_ctx),
        "D" => natives::n_double_value_of(None, &[v], noop_ctx),
        "B" => natives::n_byte_value_of(None, &[v], noop_ctx),
        "S" => natives::n_short_value_of(None, &[v], noop_ctx),
        "Z" => natives::n_boolean_value_of(None, &[v], noop_ctx),
        "C" => natives::n_character_value_of(None, &[v], noop_ctx),
        d if d.starts_with('L') || d.starts_with('[') => Ok(v),
        _ => Err(internal_error(&format!("bad field desc {desc:?}"))),
    }
}
/// The exact box class a primitive descriptor demands.
fn expected_box_class(desc: &str) -> &'static str {
    match desc {
        "I" => "java/lang/Integer",
        "J" => "java/lang/Long",
        "F" => "java/lang/Float",
        "D" => "java/lang/Double",
        "B" => "java/lang/Byte",
        "S" => "java/lang/Short",
        "Z" => "java/lang/Boolean",
        "C" => "java/lang/Character",
        _ => "java/lang/Object",
    }
}
/// Unboxes a boxed argument for a primitive parameter. Exact box class or
/// `argument type mismatch` (Temurin's reflective-conversion failure).
pub(crate) fn unbox_by_desc(v: &Value, desc: &str) -> Result<Value, Throw> {
    let want = expected_box_class(desc);
    match v {
        Value::Ref(Some(r)) => {
            let ok = with_obj(*r, |o| &*o.jclass == want);
            if !ok {
                return Err(throw_ill_arg("argument type mismatch"));
            }
            Ok(with_obj(*r, |o| o.slots[0].clone()))
        }
        Value::Ref(None) => Err(throw_ill_arg("argument type mismatch")),
        // Direct primitives appear only from internal callers; accept them.
        prim @ (Value::Int(_) | Value::Long(_) | Value::Float(_) | Value::Double(_)) => {
            Ok(prim.clone())
        }
        other => Err(internal_error(&format!("bad unbox arg {other:?}"))),
    }
}
pub(crate) fn throw_ill_arg(msg: &str) -> Throw {
    crate::throw_new("java/lang/IllegalArgumentException", Some(msg))
}
pub(crate) fn dotted_class_of(v: Option<&JRef>) -> String {
    match v {
        Some(r) => binary_name(&with_obj(*r, |o| o.jclass.to_string())),
        None => "null".to_string(),
    }
}
pub(crate) fn throw_npe_null_recv() -> Throw {
    crate::throw_new("java/lang/NullPointerException", Some(NULL_RECV_NPE))
}

// ---------------------------------------------------------------------------
// Field.get / Field.set
// ---------------------------------------------------------------------------
/// `Field.get(Object)`. Static fields ignore the receiver; instance fields
/// null-check it (Temurin NPE message), type-check it
/// (`Can not get int field O.pubf on java.lang.String`), then read and box.
pub fn field_get(ctx: Dispatch, f: &JRef, obj: Option<&JRef>) -> Result<Value, Throw> {
    let (owner, name, desc, is_static, islot, sslot, _modifiers) = field_slots(f);
    if is_static {
        let Some(get) = ctx.static_get else {
            return Err(internal_error("Field.get: no static pool access"));
        };
        return box_by_desc(get(sslot), &desc);
    }
    let Some(o) = obj else {
        return Err(throw_npe_null_recv());
    };
    let ok = with_obj(*o, |ob| ctx.is_subtype(&ob.jclass, &owner));
    if !ok {
        return Err(throw_ill_arg(&format!(
            "Can not get {} field {}.{} on {}",
            java_type_name(&desc),
            binary_name(&owner),
            name,
            dotted_class_of(obj)
        )));
    }
    let raw = crate::object::obj_get(o, islot);
    box_by_desc(raw, &desc)
}
/// `Field.set(Object, Object)`. Messages mirror Temurin's accessor family:
/// receiver mismatch reports the *receiver* class as the `to` part (the JDK
/// passes `obj` as the value there), value mismatch the value class,
/// null-on-primitive `null value` (oracle O2/O3).
pub fn field_set(ctx: Dispatch, f: &JRef, obj: Option<&JRef>, v: &Value) -> Result<(), Throw> {
    let (owner, name, desc, is_static, islot, sslot, _modifiers) = field_slots(f);
    let set_to = |cls: String| {
        throw_ill_arg(&format!(
            "Can not set {} field {}.{} to {}",
            java_type_name(&desc),
            binary_name(&owner),
            name,
            cls
        ))
    };
    let is_ref = desc.starts_with('L') || desc.starts_with('[');
    if is_static {
        let cooked = if is_ref {
            if let Value::Ref(Some(r)) = v {
                let jc = with_obj(*r, |ob| ob.jclass.to_string());
                if !ctx.is_subtype(&jc, &desc) {
                    let cls = dotted_class_of(Some(r));
                    return Err(set_to(cls));
                }
            }
            v.clone()
        } else {
            prim_cook(v, &desc, &set_to)?
        };
        let Some(set) = ctx.static_set else {
            return Err(internal_error("Field.set: no static pool access"));
        };
        set(sslot, &cooked);
        return Ok(());
    }
    let Some(o) = obj else {
        return Err(throw_npe_null_recv());
    };
    if !with_obj(*o, |ob| ctx.is_subtype(&ob.jclass, &owner)) {
        let cls = dotted_class_of(obj);
        return Err(set_to(cls));
    }
    let cooked = if is_ref {
        match v {
            Value::Ref(None) => Value::Ref(None),
            Value::Ref(Some(r)) => {
                let jc = with_obj(*r, |ob| ob.jclass.to_string());
                if !ctx.is_subtype(&jc, &desc) {
                    let cls = dotted_class_of(Some(r));
                    return Err(set_to(cls));
                }
                v.clone()
            }
            other => return Err(internal_error(&format!("bad field value {other:?}"))),
        }
    } else {
        prim_cook(v, &desc, &set_to)?
    };
    crate::object::obj_set(o, islot, cooked);
    Ok(())
}
/// Primitive-field set: exact wrapper class required; failures carry
/// Temurin's `Can not set … to <class>` / `… to null value` wording
/// (oracle O3). `Method.invoke` keeps `argument type mismatch` instead.
fn prim_cook(v: &Value, desc: &str, set_to: &dyn Fn(String) -> Throw) -> Result<Value, Throw> {
    match v {
        Value::Ref(None) => Err(set_to("null value".to_string())),
        Value::Ref(Some(r)) => {
            let want = expected_box_class(desc);
            let jc = with_obj(*r, |ob| ob.jclass.to_string());
            if jc != want {
                let cls = dotted_class_of(Some(r));
                return Err(set_to(cls));
            }
            Ok(with_obj(*r, |o| o.slots[0].clone()))
        }
        other => Err(internal_error(&format!("bad field value {other:?}"))),
    }
}

// ---------------------------------------------------------------------------
// Method.invoke
// ---------------------------------------------------------------------------
/// `Method.invoke(Object, Object…)`. Compiled classes only. Wraps every
/// exception the target throws in `InvocationTargetException` (message
/// `null`, like Temurin; `getCause` is outside the M3 subset).
pub fn method_invoke(
    ctx: Dispatch,
    m: &JRef,
    recv: Option<&JRef>,
    args: Option<&JRef>,
) -> Result<Value, Throw> {
    let (owner, name, desc, is_static, _modifiers) = method_slots(m);
    if ctx.class_info(&owner).is_none() {
        return Err(internal_error(
            "reflection on builtin classes is outside the M3 subset (Method.invoke)",
        ));
    }
    let Some(f) = ctx.find_method(&owner, &name, &desc) else {
        return Err(internal_error(&format!(
            "reflective target {owner}.{name}{desc} not in dispatch table"
        )));
    };
    // Arrays keep their length in slot 0; elements start at slot 1.
    let actual: Vec<Value> = match args {
        None => Vec::new(),
        Some(arr) => with_obj(*arr, |o| o.slots.iter().skip(1).cloned().collect()),
    };
    let Some((params, ret)) = parse_method_desc(&desc) else {
        return Err(internal_error(&format!("bad method desc {desc:?}")));
    };
    if actual.len() != params.len() {
        return Err(throw_ill_arg(&format!(
            "wrong number of arguments: {} expected: {}",
            actual.len(),
            params.len()
        )));
    }
    let cooked = unmarshal_args(ctx, &params, &actual)?;
    let recv_for_call: Option<&JRef> = if is_static {
        // Temurin ignores the receiver on static invokes (oracle: a non-null
        // receiver of the wrong class still succeeds).
        None
    } else {
        match recv {
            None => return Err(throw_npe_null_recv()),
            Some(r) => {
                let jc = with_obj(*r, |ob| ob.jclass.to_string());
                if !ctx.is_subtype(&jc, &owner) {
                    return Err(throw_ill_arg(
                        "object is not an instance of declaring class",
                    ));
                }
                Some(r)
            }
        }
    };
    match f(recv_for_call, &cooked) {
        Ok(v) => {
            if ret == TypeDesc::Void {
                Ok(Value::Ref(None))
            } else {
                box_by_desc(v, &ret.field_desc())
            }
        }
        Err(t) => Err(wrap_in_ite(t)),
    }
}
/// Type-check + unmarshal reflective arguments (Temurin:
/// `argument type mismatch` / the IllegalArgumentException family) — the
/// one law Method.invoke and Constructor.newInstance share.
fn unmarshal_args(
    ctx: Dispatch,
    params: &[TypeDesc],
    actual: &[Value],
) -> Result<Vec<Value>, Throw> {
    let mut cooked: Vec<Value> = Vec::with_capacity(params.len());
    for (t, a) in params.iter().zip(actual.iter()) {
        match t {
            TypeDesc::Object(internal) => match a {
                Value::Ref(None) => cooked.push(Value::Ref(None)),
                Value::Ref(Some(r)) => {
                    let jc = with_obj(*r, |ob| ob.jclass.to_string());
                    if !ctx.is_subtype(jc.as_str(), internal) {
                        return Err(throw_ill_arg("argument type mismatch"));
                    }
                    cooked.push(a.clone());
                }
                _ => return Err(throw_ill_arg("argument type mismatch")),
            },
            TypeDesc::Array(d) => match a {
                Value::Ref(None) => cooked.push(Value::Ref(None)),
                Value::Ref(Some(r)) => {
                    let jc = with_obj(*r, |ob| ob.jclass.to_string());
                    if !ctx.is_subtype(jc.as_str(), d) {
                        return Err(throw_ill_arg("argument type mismatch"));
                    }
                    cooked.push(a.clone());
                }
                _ => return Err(throw_ill_arg("argument type mismatch")),
            },
            _ => cooked.push(unbox_by_desc(a, &t.field_desc())?),
        }
    }
    Ok(cooked)
}
/// `Constructor.newInstance` — NEW-INSTANCE (registered 2026-11-13,
/// fixed 2026-11-14, probes NewProbe/NewProbe2 vs Temurin 21.0.13+9):
///   P1 the clinit runs (once) before the ctor, and the ctor sees the
///      JLS §4.12.5 defaults (x=0 before assignment) — the layout
///      defaults ride the metadata (defaults_spec, total_slots);
///   P4/P7 the clinit's EIIE propagates UNWRAPPED (bare
///      ExceptionInInitializerError with the original as cause), and
///      the errant NCDFE with the fresh-EIIE cause on later accesses —
///      the ensure throws are NOT InvocationTargetException-wrapped;
///   P5 a same-package private ctor succeeds (the caller-sensitivity
///      residual: j2r has no caller concept, so the invoke ALWAYS runs;
///      the DIFFERENT-package IllegalAccessException is the registered
///      residual);
///   P6 an abstract class throws the message-null
///      InstantiationException (NOT wrapped);
///   P9 a ctor's own throw wraps in InvocationTargetException with the
///      original as cause (exactly Method.invoke's law).
pub fn ctor_new_instance(ctx: Dispatch, ctor: &JRef, args: Option<&JRef>) -> Result<Value, Throw> {
    let (owner, desc, _modifiers) = ctor_slots(ctor);
    let Some(info) = ctx.class_info(&owner) else {
        return Err(internal_error(
            "reflection on builtin classes is outside the R2 subset (Constructor.newInstance)",
        ));
    };
    // The clinit law: the ensure's EIIE / errant NCDFE propagate UNWRAPPED
    // (probes P7/P8 — the class-init exception escapes the reflection
    // wrap).
    crate::clinit::ensure(info.id, info.clinit)?;
    if info.is_abstract {
        return Err(crate::throw_new("java/lang/InstantiationException", None));
    }
    let Some(f) = ctx.find_method(&owner, "<init>", &desc) else {
        return Err(internal_error(&format!(
            "reflective target {owner}.<init>{desc} not in dispatch table"
        )));
    };
    // Arrays keep their length in slot 0; elements start at slot 1.
    let actual: Vec<Value> = match args {
        None => Vec::new(),
        Some(arr) => with_obj(*arr, |o| o.slots.iter().skip(1).cloned().collect()),
    };
    let Some((params, _ret)) = parse_method_desc(&desc) else {
        return Err(internal_error(&format!("bad ctor desc {desc:?}")));
    };
    if actual.len() != params.len() {
        return Err(throw_ill_arg(&format!(
            "wrong number of arguments: {} expected: {}",
            actual.len(),
            params.len()
        )));
    }
    let cooked = unmarshal_args(ctx, &params, &actual)?;
    // The JLS §4.12.5 default-slot fill over the WHOLE flattened layout
    // (the ctor sees x=0 before assignment — probe P1). set_defaults
    // applies the spec from the tail: the builtin base region stays Void
    // exactly like the emitted `new` sites.
    let obj = crate::object::new_obj(&owner, info.total_slots)?;
    crate::object::set_defaults(&obj, info.defaults_spec);
    // The <init> runs ON the fresh object (the receiver rides the call,
    // exactly like the generated `new` sequence's invokespecial).
    match f(Some(&obj), &cooked) {
        Ok(_) => Ok(Value::Ref(Some(obj))),
        Err(t) => Err(wrap_in_ite(t)),
    }
}
/// Wraps a target exception in `java/lang/reflect/InvocationTargetException`
/// with a `null` message (Temurin prints the bare class name). The wrapped
/// target rides in the cause slot (audit A2 added `Throwable.cause`), so
/// `getCause()` returns it exactly like the JVM.
fn wrap_in_ite(t: Throw) -> Throw {
    let obj = crate::heap::alloc(crate::object::JObj {
        jclass: Arc::from("java/lang/reflect/InvocationTargetException"),
        slots: vec![Value::Ref(None), Value::Ref(Some(t.obj)), Value::Ref(None)],
        prim: None,
    });
    Throw { obj }
}
/// The reflection CALLER law for `Constructor.newInstance` (the
/// different-package private-ctor residual, registered 2026-11-14 and
/// closed 2026-11-16). Oracle-pinned on Temurin 21.0.13+9 (probe
/// S1-S8, scripts/evidence/private_ctor/PRIVATE_CTOR_2026-11-16.txt):
///   public               -> allowed (the unnamed module reads every
///                           classpath class; probe S5)
///   private              -> NESTMATES only (JEP 181: the host classes
///                           resolve and compare; the same-package
///                           NON-nestmate caller is REFUSED too — probe
///                           S2; the nestmate shape is allowed — probe
///                           S6). The NestHost attribute (JVMS 4.7.28)
///                           is the authoritative fact; a class with no
///                           attribute IS its own nest host.
///   protected            -> same package ONLY for newInstance (the
///                           check passes obj=null, so the protected
///                           instance-context rule refuses even the
///                           different-package SUBCLASS caller — probe
///                           S4)
///   package-private      -> same package (probe S8)
/// The refusal is `IllegalAccessException` with Temurin's exact
/// message: `class <caller-dotted> cannot access a member of class
/// <declaring-dotted> with modifiers "<Modifier.toString>"`, or the
/// tail `with package access` when the raw bits are zero (probe S8).
/// The CALLER is the EMIT-TIME class of the call site — a closed-world
/// compile knows it statically, so the emitter tags every
/// `Constructor.newInstance` invoke with the emitting class's binary
/// name and no runtime caller stack is needed.
pub fn ctor_new_instance_caller(
    ctx: Dispatch,
    ctor: &JRef,
    args: &[Value],
    caller: &str,
) -> Result<Value, Throw> {
    // The access check runs BEFORE the args unpack (the JDK checks
    // access first — a refused caller must not surface the args
    // validation's IllegalArgumentException).
    let (owner, _desc, modifiers) = ctor_slots(ctor);
    if !ctor_access_allowed(&ctx, caller, &owner, modifiers) {
        return Err(throw_illegal_access(caller, &owner, modifiers));
    }
    // The args unpack mirrors n_ctor_new_instance bit-for-bit: the
    // `Object[]` param rides as the single invoke argument (a null
    // array is legal for a no-arg ctor).
    let argv = match args.first() {
        Some(Value::Ref(r)) => r.as_ref(),
        _ => {
            return Err(internal_error(
                "Constructor.newInstance needs the args array",
            ))
        }
    };
    ctor_new_instance(ctx, ctor, argv)
}
/// The member-access law (Reflection.verifyMemberAccess reduced to the
/// ctor shapes the probe pinned). Same-class callers pass through the
/// arms below (same class = same package, or nestmate for private).
pub(crate) fn ctor_access_allowed(
    ctx: &Dispatch,
    caller: &str,
    owner: &str,
    modifiers: u16,
) -> bool {
    let is_public = (modifiers & 0x0001) != 0;
    let is_private = (modifiers & 0x0002) != 0;
    if is_public {
        return true;
    }
    if is_private {
        return nest_host_of(ctx, caller) == nest_host_of(ctx, owner);
    }
    // protected and package-private: the package branch only (probe
    // S4 refuses the different-package subclass; there is no receiver
    // instance to satisfy the protected context rule).
    package_of(caller) == package_of(owner)
}
/// The class's package — everything before the last `/` of the binary
/// name, the empty package for a bare name (nested `$` names stay
/// inside their top-level package, exactly like the JDK's derivation).
fn package_of(class: &str) -> &str {
    match class.rfind('/') {
        Some(i) => &class[..i],
        None => "",
    }
}
/// The class's resolved nest host: the NestHost attribute's class, or
/// the class itself when the attribute is absent (a top-level class IS
/// its own nest host). Builtin classes carry no nest metadata and
/// resolve to themselves.
fn nest_host_of(ctx: &Dispatch, class: &str) -> String {
    ctx.class_info(class)
        .and_then(|info| info.nest_host.map(|h| h.to_string()))
        .unwrap_or_else(|| class.to_string())
}
/// `Modifier.toString` replicated bit-for-bit (the same canonical bit
/// order the natives row uses) for the refusal message's modifier word.
fn modifiers_word(modifiers: u16) -> String {
    let mut sb = String::new();
    let push = |sb: &mut String, bit: u16, word: &str| {
        if (modifiers & bit) != 0 {
            sb.push_str(word);
            sb.push(' ');
        }
    };
    push(&mut sb, 0x0001, "public");
    push(&mut sb, 0x0004, "protected");
    push(&mut sb, 0x0002, "private");
    push(&mut sb, 0x0400, "abstract");
    push(&mut sb, 0x0008, "static");
    push(&mut sb, 0x0010, "final");
    push(&mut sb, 0x0080, "transient");
    push(&mut sb, 0x0040, "volatile");
    push(&mut sb, 0x0020, "synchronized");
    push(&mut sb, 0x0100, "native");
    push(&mut sb, 0x0800, "strictfp");
    sb.strip_suffix(' ').unwrap_or(&sb).to_string()
}
/// Temurin's exact refusal message (probe S1/S3/S4/S8): dotted binary
/// names both sides, `with modifiers "..."` for any nonzero bit set and
/// `with package access` for the zero set.
pub(crate) fn throw_illegal_access(caller: &str, owner: &str, modifiers: u16) -> Throw {
    let tail = if modifiers == 0 {
        "with package access".to_string()
    } else {
        format!("with modifiers \"{}\"", modifiers_word(modifiers))
    };
    crate::throw_new(
        "java/lang/IllegalAccessException",
        Some(&format!(
            "class {} cannot access a member of class {} {}",
            caller.replace('/', "."),
            owner.replace('/', "."),
            tail
        )),
    )
}
