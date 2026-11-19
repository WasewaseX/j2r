use super::*;

pub(crate) fn n_character_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, "Character.toString").expect_int();
    let c = char::from_u32(v as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
    Ok(Value::Ref(Some(object::new_string(&c.to_string()))))
}
/// Java-exact integer parsing shared by `Integer.parseInt`/`Long.parseLong`.
fn java_parse_i64(s: Option<&str>, lo_inclusive: i64, hi_inclusive: i64) -> Result<i64, Throw> {
    let s = match s {
        Some(s) => s,
        None => {
            return Err(throw_new(
                "java/lang/NumberFormatException",
                Some("Cannot parse null string"),
            ))
        }
    };
    let fail = || {
        throw_new(
            "java/lang/NumberFormatException",
            Some(&format!("For input string: \"{s}\"")),
        )
    };
    let (negative, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => match s.strip_prefix('+') {
            Some(rest) => (false, rest),
            None => (false, s),
        },
    };
    if digits.is_empty() {
        return Err(fail());
    }
    let mut acc: i128 = 0;
    for c in digits.chars() {
        let d = match c.to_digit(10) {
            Some(d) => d as i128,
            None => return Err(fail()),
        };
        acc = acc * 10 + d;
        if acc > -(lo_inclusive as i128) {
            return Err(fail());
        }
    }
    let v = if negative {
        -acc
    } else {
        if acc > hi_inclusive as i128 {
            return Err(fail());
        }
        acc
    };
    if v < lo_inclusive as i128 || v > hi_inclusive as i128 {
        return Err(fail());
    }
    Ok(v as i64)
}
/// Radix form of [`java_parse_i64`] with Temurin-verified messages (see
/// `n_long_parse_long_radix`). Negative accumulation keeps MIN parseable.
fn java_parse_i64_radix(
    s: Option<&str>,
    radix: i32,
    lo_inclusive: i64,
    hi_inclusive: i64,
) -> Result<i64, Throw> {
    if radix < 2 {
        return Err(throw_new(
            "java/lang/NumberFormatException",
            Some(&format!("radix {radix} less than Character.MIN_RADIX")),
        ));
    }
    if radix > 36 {
        return Err(throw_new(
            "java/lang/NumberFormatException",
            Some(&format!("radix {radix} greater than Character.MAX_RADIX")),
        ));
    }
    let s = match s {
        Some(s) => s,
        None => {
            return Err(throw_new(
                "java/lang/NumberFormatException",
                Some("Cannot parse null string"),
            ))
        }
    };
    let fail = || {
        let msg = if radix == 10 {
            format!("For input string: \"{s}\"")
        } else {
            format!("For input string: \"{s}\" under radix {radix}")
        };
        throw_new("java/lang/NumberFormatException", Some(&msg))
    };
    let (negative, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => match s.strip_prefix('+') {
            Some(rest) => (false, rest),
            None => (false, s),
        },
    };
    if digits.is_empty() {
        return Err(fail());
    }
    let r = radix as i128;
    let mut acc: i128 = 0;
    for c in digits.chars() {
        let d = match c.to_digit(radix as u32) {
            Some(d) => d as i128,
            None => return Err(fail()),
        };
        acc = acc * r - d;
        if acc < lo_inclusive as i128 {
            return Err(fail());
        }
    }
    let v: i128 = if negative {
        acc
    } else {
        if -acc > hi_inclusive as i128 {
            return Err(fail());
        }
        -acc
    };
    Ok(v as i64)
}
pub(crate) fn n_integer_parse_int(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    let s = match arg(args, "Integer.parseInt") {
        Value::Ref(Some(o)) if is_string(o) => Some(object::string_value(o).to_string()),
        Value::Ref(None) => None,
        _ => return Err(internal_error("Integer.parseInt needs a String")),
    };
    let v = java_parse_i64(s.as_deref(), i32::MIN as i64, i32::MAX as i64)?;
    Ok(Value::Int(v as i32))
}
// -- float/double string parsing (corpus wave 1) ---------------------------
// Semantics verified against Temurin 21.0.13 (corpus/FPProbe.java):
// null -> NPE naming String.trim(); leading/trailing whitespace trimmed;
// "empty String" for the trimmed-empty input; exact spellings Infinity /
// +/-Infinity / NaN (case-sensitive - "inf" is a NumberFormatException); one
// trailing f/F/d/D suffix accepted; correctly-rounded f32/f64 parsing
// (Rust's parse is correctly rounded, matching HotSpot's strict parsing).
// Hexadecimal forms (0x1.8p1) route to the dedicated hexfloat module
// (corpus gap F4, closed 2026-11-05; grammar + rounding pinned with
// corpus/HexFloatProbe.java).
pub(crate) fn n_float_parse_float(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    let s = match arg(args, "Float.parseFloat") {
        Value::Ref(Some(o)) if is_string(o) => Some(object::string_value(o).to_string()),
        Value::Ref(None) => None,
        _ => return Err(internal_error("Float.parseFloat needs a String")),
    };
    // f32 directly (never through f64: double rounding is observable)
    let t = s.as_deref().unwrap_or("").trim();
    let fail = || {
        throw_new(
            "java/lang/NumberFormatException",
            Some(&if t.is_empty() {
                "empty String".to_string()
            } else {
                format!("For input string: \"{t}\"")
            }),
        )
    };
    let body = match t.strip_suffix(['f', 'F', 'd', 'D']) {
        Some(rest) if !rest.is_empty() => rest,
        _ => t,
    };
    // hex forms first (F4): exact bit patterns, correctly rounded
    if let Some(bits) = crate::hexfloat::parse_bits(body, true) {
        return Ok(Value::Float(f32::from_bits(bits as u32)));
    }
    let f = match body {
        "Infinity" | "+Infinity" => f32::INFINITY,
        "-Infinity" => f32::NEG_INFINITY,
        "NaN" => f32::NAN,
        _ => {
            // reject the Rust-only spellings Java rejects
            let low = body.to_ascii_lowercase();
            let low = low.trim_start_matches('+');
            if low == "infinity" || low == "inf" || low == "nan" {
                return Err(fail());
            }
            body.parse::<f32>().map_err(|_| fail())?
        }
    };
    Ok(Value::Float(f))
}
pub(crate) fn n_double_parse_double(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    let s = match arg(args, "Double.parseDouble") {
        Value::Ref(Some(o)) if is_string(o) => Some(object::string_value(o).to_string()),
        Value::Ref(None) => None,
        _ => return Err(internal_error("Double.parseDouble needs a String")),
    };
    let t = s.as_deref().unwrap_or("").trim();
    let fail = || {
        throw_new(
            "java/lang/NumberFormatException",
            Some(&if t.is_empty() {
                "empty String".to_string()
            } else {
                format!("For input string: \"{t}\"")
            }),
        )
    };
    let body = match t.strip_suffix(['f', 'F', 'd', 'D']) {
        Some(rest) if !rest.is_empty() => rest,
        _ => t,
    };
    // hex forms first (F4): exact bit patterns, correctly rounded
    if let Some(bits) = crate::hexfloat::parse_bits(body, false) {
        return Ok(Value::Double(f64::from_bits(bits)));
    }
    let d = match body {
        "Infinity" | "+Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        "NaN" => f64::NAN,
        _ => {
            let low = body.to_ascii_lowercase();
            let low = low.trim_start_matches('+');
            if low == "infinity" || low == "inf" || low == "nan" {
                return Err(fail());
            }
            body.parse::<f64>().map_err(|_| fail())?
        }
    };
    Ok(Value::Double(d))
}
pub(crate) fn n_long_parse_long(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    let s = match arg(args, "Long.parseLong") {
        Value::Ref(Some(o)) if is_string(o) => Some(object::string_value(o).to_string()),
        Value::Ref(None) => None,
        _ => return Err(internal_error("Long.parseLong needs a String")),
    };
    let v = java_parse_i64(s.as_deref(), i64::MIN, i64::MAX)?;
    Ok(Value::Long(v))
}
/// `Long.parseLong(String, int)` — radix form (corpus wave 1). Messages
/// verified against Temurin 21.0.13 (corpus/ParseProbe.java): radix bounds
/// get the MIN_RADIX/MAX_RADIX wording, the base-10 failure keeps the
/// classic message, every other radix appends "under radix N". Parsing
/// accumulates NEGATIVELY so `Long.MIN_VALUE` parses exactly.
pub(crate) fn n_long_parse_long_radix(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    let s = match arg(args, "Long.parseLong") {
        Value::Ref(Some(o)) if is_string(o) => Some(object::string_value(o).to_string()),
        Value::Ref(None) => None,
        _ => return Err(internal_error("Long.parseLong needs a String")),
    };
    let radix = match args.get(1) {
        Some(v) => v.expect_int(),
        None => return Err(internal_error("j2r internal: Long.parseLong radix")),
    };
    let v = java_parse_i64_radix(s.as_deref(), radix, i64::MIN, i64::MAX)?;
    Ok(Value::Long(v))
}

// ------------------------------------------------- M3 Wave 5: reflection
// The reflection subset (M3_PLAN §8). Every failure message below is the
// exact string Temurin 21 printed during the oracle capture (O/O2/O3.java,
// recorded in M3_EVIDENCE.md wave-5).
pub(crate) fn n_object_get_class(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.getClass");
    let name = crate::heap::with_obj(*r, |o| o.jclass.to_string());
    Ok(crate::reflect::class_value(&name))
}
/// A `Class.forName` binary name → the internal form the runtime keys on
/// (dots → slashes; the array-descriptor form keeps its brackets and
/// converts only inside `L...;`). Temurin accepts both `java.lang.String`
/// and `[Ljava.lang.String;` (oracle capture, R2 slice 1 evidence).
fn for_name_internal(name: &str) -> String {
    if let Some(rest) = name.strip_prefix('[') {
        format!("[{}", for_name_internal(rest))
    } else if let Some(comp) = name.strip_prefix('L').and_then(|s| s.strip_suffix(';')) {
        format!("L{};", comp.replace('.', "/"))
    } else {
        name.replace('.', "/")
    }
}
/// Is `internal` an array whose component the closed world knows?
/// Primitive components (`[I`) are always known; `L...;` components must
/// resolve (recursively for nested descriptors).
fn for_name_known(internal: &str, ctx: Dispatch) -> bool {
    if let Some(rest) = internal.strip_prefix('[') {
        return match rest.strip_prefix('L').and_then(|s| s.strip_suffix(';')) {
            Some(comp) => for_name_known(comp, ctx),
            // primitive component (or nested weirdness the verifier would
            // have rejected) — primitives are always resolvable
            None => true,
        };
    }
    // compiled registry first (a compiled class shadows nothing — the
    // closed world is disjoint — but the registry is authoritative for
    // user classes), then the builtin tables.
    ctx.class_info(internal).is_some() || crate::builtin::builtin_class_known(internal)
}
/// `Class.forName(String)` — the R2 slice-1 opening: runtime string
/// lookups resolve against the closed world (the generated crate's
/// metadata registry + the builtin tables). Oracle capture
/// (Temurin 21.0.13, this session): unknown name →
/// `ClassNotFoundException` whose message IS the input name; null name →
/// `NullPointerException: null`; `int` is NOT resolvable (primitives are
/// not classes by name); `[I` and `[Ljava.lang.String;` resolve; the
/// message carries the BINARY name verbatim.
///
/// FORNAME-INIT (2026-11-14, probed not assumed — ForNameProbe vs
/// Temurin 21.0.13): `forName(name)` equals `forName(name, true,
/// loader)` — a first-use through forName RUNS the `<clinit>` through
/// the same JLS 12.4.2 machine the bytecode ensure sites use, and a
/// throwing clinit surfaces the EIIIE law (Errors as-is, anything else
/// wrapped message-null, later first-uses the errant NCDFE with the
/// fresh-EIIE cause). Builtin classes and array descriptors have no
/// compiled metadata — nothing to ensure; in particular
/// `forName("[LC;")` resolves WITHOUT initializing C (JLS 12.4.1, probe
/// F6).
pub(crate) fn for_name_resolve(
    args: &[Value],
    ctx: Dispatch,
    ensure_init: bool,
) -> Result<Value, Throw> {
    let name = match args.first() {
        Some(Value::Ref(None)) => {
            return Err(crate::throw_new(
                "java/lang/NullPointerException",
                Some("null"),
            ))
        }
        Some(Value::Ref(Some(r))) => crate::object::string_value(r),
        other => {
            return Err(internal_error(&format!(
                "Class.forName: expected string arg, found {other:?}"
            )))
        }
    };
    let internal = for_name_internal(&name);
    if for_name_known(&internal, ctx) {
        if ensure_init {
            if let Some(info) = ctx.class_info(&internal) {
                crate::clinit::ensure(info.id, info.clinit)?;
            }
        }
        Ok(crate::reflect::class_value(&internal))
    } else {
        Err(crate::throw_new(
            "java/lang/ClassNotFoundException",
            Some(&name),
        ))
    }
}
