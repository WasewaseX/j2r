use super::*;

/// `Double.valueOf(String)` — parse then box (JAMA's uncalled Matrix.read
/// parses tokens through it; the parsing core is the probed
/// parseDouble contract).
pub(crate) fn n_double_value_of_s(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = n_double_parse_double(_recv, args, ctx)?;
    let Value::Double(d) = v else {
        return Err(internal_error("j2r internal: parseDouble result"));
    };
    let r = object::new_obj("java/lang/Double", 1)?;
    object::obj_set(&r, 0, Value::Double(d));
    Ok(Value::Ref(Some(r)))
}

// ---------------------------------------------------------------- Object
pub(crate) fn n_object_init(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Void)
}
// Object monitor condition methods (M4 Wave 3, T11/T12): real
// implementations over the Wave-3 monitor (crate::monitor). All three
// `wait` shapes funnel into one implementation; argument validation and
// the ownership check live there (HotSpot order: ownership first).
pub(crate) fn n_object_wait(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.wait");
    let (millis, nanos) = match args {
        [] => (0i64, 0i32),
        [Value::Long(v)] => (*v, 0i32),
        [Value::Long(v), Value::Int(n)] => (*v, *n),
        _ => (0, 0), // unreachable through the registered shapes
    };
    crate::monitor::object_wait(r, millis, nanos)
}
pub(crate) fn n_object_notify(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.notify");
    crate::monitor::object_notify(r, false)?;
    Ok(Value::Void)
}
pub(crate) fn n_object_notify_all(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.notifyAll");
    crate::monitor::object_notify(r, true)?;
    Ok(Value::Void)
}
pub(crate) fn n_object_hash_code(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.hashCode");
    // Identity hash = the arena index: distinct for distinct live objects,
    // stable for a live object across collections (M2_PLAN §4.6).
    Ok(Value::Int(r.index() as i32))
}
pub(crate) fn n_object_clone(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.clone");
    Ok(Value::Ref(Some(object::object_shallow_clone(r)?)))
}

// ------------------------- java/lang/Enum (corpus wave 2, JZlib)
fn enum_slot(recv: Option<&JRef>, who: &str, idx: usize) -> Value {
    let r = this(recv, who);
    crate::heap::with_obj(*r, |o| o.slots[idx].clone())
}
pub(crate) fn n_enum_init(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Enum.<init>");
    let name = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        Some(Value::Ref(Some(o))) if is_string(o) => object::string_value(o).to_string(),
        _ => return Err(internal_error("j2r internal: Enum.<init> name argument")),
    };
    let ord = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(internal_error("j2r internal: Enum.<init> ordinal argument")),
    };
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = Value::Str(std::sync::Arc::new(name));
        o.slots[1] = Value::Int(ord);
    });
    Ok(Value::Void)
}
pub(crate) fn n_enum_name(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    match enum_slot(recv, "Enum.name", 0) {
        Value::Str(s) => Ok(Value::Ref(Some(object::new_string(&s)))),
        _ => Err(internal_error("j2r internal: Enum.name slot")),
    }
}
pub(crate) fn n_enum_ordinal(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    match enum_slot(recv, "Enum.ordinal", 1) {
        Value::Int(v) => Ok(Value::Int(v)),
        _ => Err(internal_error("j2r internal: Enum.ordinal slot")),
    }
}
/// JDK `Enum.toString()` returns the name.
pub(crate) fn n_enum_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    match enum_slot(recv, "Enum.toString", 0) {
        Value::Str(s) => Ok(Value::Ref(Some(object::new_string(&s)))),
        _ => Err(internal_error("j2r internal: Enum.toString slot")),
    }
}
/// JDK `Enum.equals` is final identity (`this == other`).
pub(crate) fn n_enum_equals(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Enum.equals");
    let same = match args.first() {
        Some(Value::Ref(Some(other))) => JRef::ptr_eq(r, other),
        _ => false,
    };
    Ok(Value::Int(same as i32))
}
/// JDK `Enum.compareTo` — the ordinal compare with the exact three-way
/// branch shape (same ordinals of DIFFERENT subclasses subtract too).
pub(crate) fn n_enum_compare_to(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Enum.compareTo");
    let other = match args.first() {
        Some(Value::Ref(Some(o))) => *o,
        _ => return Err(internal_error("j2r internal: Enum.compareTo argument")),
    };
    let mine = match enum_slot(Some(r), "Enum.compareTo", 1) {
        Value::Int(v) => v,
        _ => return Err(internal_error("j2r internal: Enum.compareTo ordinal slot")),
    };
    let theirs = match enum_slot(Some(&other), "Enum.compareTo", 1) {
        Value::Int(v) => v,
        _ => return Err(internal_error("j2r internal: Enum.compareTo ordinal slot")),
    };
    // ClassCastException parity is the call site's job (javac inserts the
    // checkcast; the bridge method carries the typed descriptor).
    if mine < theirs {
        Ok(Value::Int(-1))
    } else if mine > theirs {
        Ok(Value::Int(1))
    } else {
        Ok(Value::Int(mine.wrapping_sub(theirs)))
    }
}
/// JDK `Enum.getDeclaringClass` — the `Class` object for the enum class
/// itself (final, returns the receiver's runtime class).
pub(crate) fn n_enum_get_declaring_class(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Enum.getDeclaringClass");
    let name = crate::heap::with_obj(*r, |o| o.jclass.to_string());
    Ok(reflect::class_value(&name))
}
/// JDK `Enum.valueOf(Class, String)`. javac's synthetic `E.valueOf(String)`
/// delegates here, so one implementation covers both entry points. The
/// constant table is the class's `$VALUES` static (javac synthesizes it for
/// every enum), read through the generated crate's pool: `meta` names the
/// field, `static_get` reads the slot. The constant's name lives in the
/// Enum slot 0 (the same slot `<init>` writes).
///
/// Temurin order pinned by corpus/EnumProbe.java: the directory lookup runs
/// FIRST, then a null name throws NPE("Name is null"), then a miss throws
/// IAE(`No enum constant <binary name>.<name>`) — the binary (dotted) name,
/// so a nested enum prints `EnumProbe.Color.red`.
pub(crate) fn n_enum_value_of(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let cls = first_arg_ref!(
        args,
        Err(internal_error("j2r internal: Enum.valueOf class argument"))
    );
    let name: Option<String> = match args.get(1) {
        Some(Value::Str(s)) => Some(s.to_string()),
        Some(Value::Ref(Some(o))) if object::is_string(o) => {
            Some(object::string_value(o).to_string())
        }
        Some(Value::Ref(None)) => None,
        _ => return Err(internal_error("j2r internal: Enum.valueOf name argument")),
    };
    let cls_name = reflect::class_name_of(&cls);
    let values = match (ctx.meta, ctx.static_get) {
        (Some(meta), Some(get)) => match meta(&cls_name) {
            Some(info) => info
                .fields
                .iter()
                .find(|f| f.is_static && f.name == "$VALUES")
                .map(|f| get(f.static_slot)),
            None => None,
        },
        _ => None,
    };
    match values {
        Some(Value::Ref(Some(arr))) => {
            let len = crate::array::array_length(&arr)?;
            for i in 0..len {
                let elem = crate::array::array_load(&arr, i)?;
                if let Value::Ref(Some(obj)) = &elem {
                    if let Value::Str(s) = enum_slot(Some(obj), "Enum.valueOf", 0) {
                        if let Some(want) = &name {
                            if s.as_ref() == want.as_str() {
                                return Ok(elem);
                            }
                        }
                    }
                }
            }
        }
        // Not a compiled enum class (builtin pseudo-enum, uncompiled
        // dependency): no constant table to search — loud internal marker,
        // mirroring the serialization-marker rule.
        _ => {
            return Err(internal_error(&format!(
                "j2r internal: Enum.valueOf: no $VALUES static pool for {cls_name}"
            )))
        }
    }
    match name {
        None => Err(throw_new(
            "java/lang/NullPointerException",
            Some("Name is null"),
        )),
        Some(n) => Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some(&format!(
                "No enum constant {}.{}",
                reflect::canonical_name(&cls_name),
                n
            )),
        )),
    }
}
pub(crate) fn n_object_equals(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.equals");
    let same = match args.first() {
        Some(Value::Ref(Some(other))) => JRef::ptr_eq(r, other),
        _ => false,
    };
    Ok(Value::Int(same as i32))
}
pub(crate) fn n_object_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.toString");
    let (cls, hash) = crate::heap::with_obj(*r, |o| (dotted(&o.jclass), o_slots_identity(r)));
    Ok(Value::Ref(Some(object::new_string(&format!(
        "{cls}@{}",
        javafmt::int_to_hex_string(hash)
    )))))
}
/// The identity-hash component for `Object.toString` (`Class@hash`): the
/// arena index, stable for a live object (M2_PLAN §4.6).
fn o_slots_identity(r: &JRef) -> i32 {
    r.index() as i32
}

// ---------------------------------------------------------------- System
pub(crate) fn n_system_exit(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let code = arg(args, "System.exit").expect_int();
    std::process::exit(code);
}
pub(crate) fn n_system_current_time_millis(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Ok(Value::Long(ms))
}
pub(crate) fn n_system_nano_time(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Long(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0),
    ))
}
/// Java serialization does not exist in J2R (no stream format, no
/// serialization machinery). Registered so javac-compiled calls — like
/// minimal-json's readObject hook — fail LOUDLY at runtime with a named
/// reason, never silently. Same pattern as the M3 Object.wait rows.
pub(crate) fn n_object_input_stream_default_read_object(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Err(throw_new(
        "java/lang/UnsupportedOperationException",
        Some("J2R: java.io serialization (ObjectInputStream) is not supported"),
    ))
}
pub(crate) fn n_system_arraycopy(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::array::system_arraycopy(args, ctx)
}
/// `System.loadLibrary(String)` (T13 13c — the loading story,
/// docs/M5_JNI_HARNESS.md §7): resolve the platform library filename
/// (`lib<name>.so` on this ABI), dlopen it, and run its `JNI_OnLoad` with
/// the VM table pointer through audited primitive 12. `JNI_OnLoad` then
/// `GetEnv`s the env table (VM slot 6, real since 13c) and binds its
/// natives through slot 215 — the SAME mechanism Temurin serves, which is
/// what makes the ONE harness `.so` work on both runtimes. No `JNI_OnLoad`
/// symbol is success (the spec's default-version case); a post-1.8
/// version answer is `UnsatisfiedLinkError` (the oracle's family); a
/// dlopen miss aborts loudly inside the primitive (v1: never a silent
/// fake success; the UnsatisfiedLinkError-on-load-failure transport is a
/// registered v1 gap, unit-pinned via the primitive's own loudness).
pub(crate) fn n_system_load_library(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let name = match arg(args, "System.loadLibrary") {
        Value::Ref(Some(o)) if is_string(o) => object::string_value(o).to_string(),
        Value::Ref(None) => {
            return Err(crate::throw_new("java/lang/NullPointerException", None));
        }
        _ => {
            panic!("j2r internal: System.loadLibrary needs a String")
        }
    };
    // v1: this ABI's loader filename. A non-Linux platform is a loud
    // unsupported (the standing law; CI and the parity runner are Linux).
    if !cfg!(target_os = "linux") {
        panic!("j2r JNI bridge: System.loadLibrary is Linux-only in v1 (loud unsupported)");
    }
    let path = format!("lib{name}.so");
    let Ok(cpath) = std::ffi::CString::new(path.clone()) else {
        return Err(crate::throw_new(
            "java/lang/UnsatisfiedLinkError",
            Some(&format!("{path}: embedded NUL in the library name")),
        ));
    };
    let vm = crate::jni::tables::java_vm() as usize as *mut std::os::raw::c_void;
    let ver = j2r_jni_ffi::ffi::load_foreign_jni_library(cpath.as_ptr(), vm);
    let ok = ver == 0
        || matches!(
            ver,
            crate::jni::JNI_VERSION_1_1
                | crate::jni::JNI_VERSION_1_2
                | crate::jni::JNI_VERSION_1_4
                | crate::jni::JNI_VERSION_1_6
                | crate::jni::JNI_VERSION_1_8
                | crate::jni::JNI_VERSION_9
                | crate::jni::JNI_VERSION_10
        );
    if !ok {
        return Err(crate::throw_new(
            "java/lang/UnsatisfiedLinkError",
            Some(&format!("unsupported JNI version 0x{ver:08x}")),
        ));
    }
    Ok(Value::Ref(None))
}
// float/double predicates (corpus wave 1) — exact IEEE semantics
pub(crate) fn n_float_is_infinite(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let f = arg(args, "Float.isInfinite").expect_float();
    Ok(Value::Int(f.is_infinite() as i32))
}
pub(crate) fn n_float_is_nan(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let f = arg(args, "Float.isNaN").expect_float();
    Ok(Value::Int(f.is_nan() as i32))
}
pub(crate) fn n_double_is_infinite(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let d = arg(args, "Double.isInfinite").expect_double();
    Ok(Value::Int(d.is_infinite() as i32))
}
pub(crate) fn n_double_is_nan(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let d = arg(args, "Double.isNaN").expect_double();
    Ok(Value::Int(d.is_nan() as i32))
}

// ---------------------------------------------------------------- Math
//
// Math is implemented explicitly (no macro gymnastics) so Java's NaN and
// signed-zero semantics for float/double stay obvious.
pub(crate) fn n_math_abs_i(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(
        arg(args, "Math.abs").expect_int().wrapping_abs(),
    ))
}
pub(crate) fn n_math_abs_j(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Long(
        arg(args, "Math.abs").expect_long().wrapping_abs(),
    ))
}
pub(crate) fn n_math_abs_f(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Float(arg(args, "Math.abs").expect_float().abs()))
}
pub(crate) fn n_math_abs_d(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Double(arg(args, "Math.abs").expect_double().abs()))
}
pub(crate) fn java_max_d(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        // +0.0 wins over -0.0
        if a.is_sign_positive() {
            a
        } else {
            b
        }
    } else if a > b {
        a
    } else {
        b
    }
}
pub(crate) fn java_min_d(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        // -0.0 wins over +0.0
        if a.is_sign_negative() {
            a
        } else {
            b
        }
    } else if a < b {
        a
    } else {
        b
    }
}
pub(crate) fn n_math_max_i(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, "Math.max").expect_int();
    let b = args[1].expect_int();
    Ok(Value::Int(if a >= b { a } else { b }))
}
pub(crate) fn n_math_max_j(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, "Math.max").expect_long();
    let b = args[1].expect_long();
    Ok(Value::Long(if a >= b { a } else { b }))
}
pub(crate) fn n_math_max_f(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, "Math.max").expect_float();
    let b = args[1].expect_float();
    Ok(Value::Float(java_max_d(a as f64, b as f64) as f32))
}
pub(crate) fn n_math_max_d(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, "Math.max").expect_double();
    let b = args[1].expect_double();
    Ok(Value::Double(java_max_d(a, b)))
}
pub(crate) fn n_math_min_i(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, "Math.min").expect_int();
    let b = args[1].expect_int();
    Ok(Value::Int(if a <= b { a } else { b }))
}
pub(crate) fn n_math_min_j(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, "Math.min").expect_long();
    let b = args[1].expect_long();
    Ok(Value::Long(if a <= b { a } else { b }))
}
pub(crate) fn n_math_min_f(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, "Math.min").expect_float();
    let b = args[1].expect_float();
    Ok(Value::Float(java_min_d(a as f64, b as f64) as f32))
}
pub(crate) fn n_math_min_d(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, "Math.min").expect_double();
    let b = args[1].expect_double();
    Ok(Value::Double(java_min_d(a, b)))
}

// ---------------------------------------------------------------- String
pub(crate) fn utf16_units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
