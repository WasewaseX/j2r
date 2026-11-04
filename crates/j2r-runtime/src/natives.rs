//! The natives table: JDK methods implemented directly in Rust.
//!
//! The code generator validates every method reference against this table
//! (for `java/*` classes) at compile time, so an unsupported JDK method is a
//! *compile error* with a clear message — never a silent miscompile.
//!
//! Conventions:
//! * instance natives get `recv = Some(&JRef)`, statics get `None`,
//! * `java/lang/String` and `StringBuilder` store their characters as
//!   `Value::Str` in slot 0 (copy-on-write via `Arc::make_mut`),
//! * `Throwable.detailMessage` is slot 0 (`Value::Ref` to a String or null),
//! * boxed classes store their payload in slot 0,
//! * `java/io/PrintStream` stores its sink id (1 = stdout, 2 = stderr) in
//!   slot 0,
//! * string lengths, indices and hashes follow Java's UTF-16 semantics.

use crate::collections as c;
use crate::concat;
use crate::heap::with_obj;
use crate::heap::JRef;
use crate::io;
use crate::jdkutil as ju;
use crate::lambda;
use crate::langext as le;
use crate::object::{self, is_string, new_obj};
use crate::reflect;
use crate::Value;
use crate::{internal_error, javafmt, throw_new, Dispatch, NativeFn, Throw};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

fn this<'a>(recv: Option<&'a JRef>, who: &str) -> &'a JRef {
    recv.unwrap_or_else(|| panic!("j2r internal: {who} called without receiver"))
}

fn arg<'a>(args: &'a [Value], who: &str) -> &'a Value {
    args.first()
        .ok_or_else(|| internal_error(&format!("{who} without argument")))
        .unwrap_or_else(|_| panic!("j2r internal: {who} without argument"))
}

fn dotted(class: &str) -> String {
    class.replace('/', ".")
}

fn print_string(sink: i32, text: &str, newline: bool) {
    use std::io::Write;
    let mut bytes = Vec::with_capacity(text.len() + 1);
    bytes.extend_from_slice(text.as_bytes());
    if newline {
        bytes.push(b'\n');
    }
    if sink == 1 {
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        let _ = lock.write_all(&bytes);
        let _ = lock.flush();
    } else {
        let stderr = std::io::stderr();
        let mut lock = stderr.lock();
        let _ = lock.write_all(&bytes);
        let _ = lock.flush();
    }
}

fn println_print(
    recv: Option<&JRef>,
    args: &[Value],
    kind: char,
    newline: bool,
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let sink = crate::heap::with_obj(*this(recv, "PrintStream method"), |o| {
        o.slots[0].expect_int()
    });
    let text = match kind {
        'N' => String::new(),
        _ => concat::format_value(arg(args, "PrintStream"), kind, ctx)?,
    };
    print_string(sink, &text, newline);
    Ok(Value::Void)
}

macro_rules! ps {
    ($name:ident, $kind:expr, $newline:expr) => {
        fn $name(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
            println_print(recv, args, $kind, $newline, ctx)
        }
    };
}

ps!(n_ps_println_n, 'N', true);
ps!(n_ps_println_z, 'Z', true);
ps!(n_ps_println_c, 'C', true);
ps!(n_ps_println_i, 'I', true);
ps!(n_ps_println_j, 'J', true);
ps!(n_ps_println_f, 'F', true);
ps!(n_ps_println_d, 'D', true);
ps!(n_ps_println_s, 'L', true);
ps!(n_ps_println_l, 'L', true);
ps!(n_ps_print_z, 'Z', false);
ps!(n_ps_print_c, 'C', false);
ps!(n_ps_print_i, 'I', false);
ps!(n_ps_print_j, 'J', false);
ps!(n_ps_print_f, 'F', false);
ps!(n_ps_print_d, 'D', false);
ps!(n_ps_print_s, 'L', false);
ps!(n_ps_print_l, 'L', false);

// ---------------------------------------------------------------- Object

fn n_object_init(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Void)
}

// Object monitor condition methods (M3 Wave 4): loud-unsupported — the
// threading boundary of M3_PLAN §7.1 (wait/notify need a multi-threaded
// monitor). Registered so javac-compiled calls fail loudly at *runtime*
// with a named reason, never silently.
fn n_object_wait(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Err(throw_new(
        "java/lang/UnsupportedOperationException",
        Some("J2R M3: Object.wait is not supported (single-threaded runtime)"),
    ))
}

fn n_object_notify(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Err(throw_new(
        "java/lang/UnsupportedOperationException",
        Some("J2R M3: Object.notify is not supported (single-threaded runtime)"),
    ))
}

fn n_object_notify_all(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Err(throw_new(
        "java/lang/UnsupportedOperationException",
        Some("J2R M3: Object.notifyAll is not supported (single-threaded runtime)"),
    ))
}

fn n_object_hash_code(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.hashCode");
    // Identity hash = the arena index: distinct for distinct live objects,
    // stable for a live object across collections (M2_PLAN §4.6).
    Ok(Value::Int(r.index() as i32))
}

fn n_object_equals(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Object.equals");
    let same = match args.first() {
        Some(Value::Ref(Some(other))) => JRef::ptr_eq(r, other),
        _ => false,
    };
    Ok(Value::Int(same as i32))
}

fn n_object_to_string(
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

fn n_system_exit(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let code = arg(args, "System.exit").expect_int();
    std::process::exit(code);
}

fn n_system_current_time_millis(
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

fn n_system_nano_time(
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
fn n_object_input_stream_default_read_object(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Err(throw_new(
        "java/lang/UnsupportedOperationException",
        Some("J2R: java.io serialization (ObjectInputStream) is not supported"),
    ))
}

fn n_system_arraycopy(_recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    crate::array::system_arraycopy(args, ctx)
}

// float/double predicates (corpus wave 1) — exact IEEE semantics
fn n_float_is_infinite(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let f = arg(args, "Float.isInfinite").expect_float();
    Ok(Value::Int(f.is_infinite() as i32))
}

fn n_float_is_nan(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let f = arg(args, "Float.isNaN").expect_float();
    Ok(Value::Int(f.is_nan() as i32))
}

fn n_double_is_infinite(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let d = arg(args, "Double.isInfinite").expect_double();
    Ok(Value::Int(d.is_infinite() as i32))
}

fn n_double_is_nan(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let d = arg(args, "Double.isNaN").expect_double();
    Ok(Value::Int(d.is_nan() as i32))
}

// ---------------------------------------------------------------- Math
//
// Math is implemented explicitly (no macro gymnastics) so Java's NaN and
// signed-zero semantics for float/double stay obvious.

fn n_math_abs_i(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(
        arg(args, "Math.abs").expect_int().wrapping_abs(),
    ))
}

fn n_math_abs_j(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Long(
        arg(args, "Math.abs").expect_long().wrapping_abs(),
    ))
}

fn n_math_abs_f(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Float(arg(args, "Math.abs").expect_float().abs()))
}

fn n_math_abs_d(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(arg(args, "Math.abs").expect_double().abs()))
}

fn java_max_d(a: f64, b: f64) -> f64 {
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

fn java_min_d(a: f64, b: f64) -> f64 {
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

fn n_math_max_i(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.max").expect_int();
    let b = args[1].expect_int();
    Ok(Value::Int(if a >= b { a } else { b }))
}

fn n_math_max_j(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.max").expect_long();
    let b = args[1].expect_long();
    Ok(Value::Long(if a >= b { a } else { b }))
}

fn n_math_max_f(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.max").expect_float();
    let b = args[1].expect_float();
    Ok(Value::Float(java_max_d(a as f64, b as f64) as f32))
}

fn n_math_max_d(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.max").expect_double();
    let b = args[1].expect_double();
    Ok(Value::Double(java_max_d(a, b)))
}

fn n_math_min_i(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.min").expect_int();
    let b = args[1].expect_int();
    Ok(Value::Int(if a <= b { a } else { b }))
}

fn n_math_min_j(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.min").expect_long();
    let b = args[1].expect_long();
    Ok(Value::Long(if a <= b { a } else { b }))
}

fn n_math_min_f(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.min").expect_float();
    let b = args[1].expect_float();
    Ok(Value::Float(java_min_d(a as f64, b as f64) as f32))
}

fn n_math_min_d(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.min").expect_double();
    let b = args[1].expect_double();
    Ok(Value::Double(java_min_d(a, b)))
}

// ---------------------------------------------------------------- String

fn utf16_units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn n_string_init(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(String::new())));
    Ok(Value::Void)
}

/// `String(char[])` / `String(char[], off, len)` (corpus wave 1): build
/// from UTF-16 code units — the same unit convention as getChars.
fn string_from_char_range(args: &[Value], op: &str) -> Result<String, Throw> {
    let (cbuf, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(a))), Some(o), Some(l)) => (*a, o.expect_int(), l.expect_int()),
        (Some(Value::Ref(None)), _, _) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
        _ => return Err(internal_error(&format!("j2r internal: {op} args"))),
    };
    let clen = crate::array::array_length(&cbuf)?;
    if off < 0 || len < 0 || off.checked_add(len).is_none_or(|e| e > clen) {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!("offset {}, count {}, length {}", off, len, clen)),
        ));
    }
    let mut units: Vec<u16> = Vec::with_capacity(len as usize);
    for i in 0..len {
        match crate::array::array_load(&cbuf, off + i)? {
            Value::Int(c) => units.push(c as u16),
            other => {
                return Err(internal_error(&format!(
                    "j2r internal: char[] element {other:?}"
                )))
            }
        }
    }
    Ok(String::from_utf16_lossy(&units))
}

fn n_string_init_chars(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    let cbuf = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(internal_error("String.<init>([C) needs a char array")),
    };
    let len = crate::array::array_length(&cbuf)?;
    let text = string_from_char_range(
        &[Value::Ref(Some(cbuf)), Value::Int(0), Value::Int(len)],
        "String.<init>([C)",
    )?;
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}

fn n_string_init_chars_range(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    let text = string_from_char_range(args, "String.<init>([CII)")?;
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}

fn n_string_init_s(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    let o = match arg(args, "String.<init>") {
        Value::Ref(Some(o)) if is_string(o) => o,
        _ => return Err(internal_error("String.<init>(String) needs a String")),
    };
    let text = object::string_value(o).to_string();
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}

fn n_string_length(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.length");
    Ok(Value::Int(
        utf16_units(&object::string_value(r)).len() as i32
    ))
}

/// Java `String.toString()` returns `this` — registered so dispatched
/// `toString()` calls (collections, concat, `Objects.toString`) reach the
/// string itself instead of falling through to `Object.toString`.
pub fn n_string_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(*this(recv, "String.toString"))))
}

fn n_string_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.isEmpty");
    Ok(Value::Int(object::string_value(r).is_empty() as i32))
}

fn n_string_char_at(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.charAt");
    let idx = arg(args, "String.charAt").expect_int();
    let units = utf16_units(&object::string_value(r));
    let len = units.len() as i32;
    if idx < 0 || idx >= len {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!("Index {idx} out of bounds for length {len}")),
        ));
    }
    Ok(Value::Int(units[idx as usize] as i32))
}

fn n_string_equals(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.equals");
    let eq = match arg(args, "String.equals") {
        Value::Ref(Some(o)) if is_string(o) => *object::string_value(r) == *object::string_value(o),
        _ => false,
    };
    Ok(Value::Int(eq as i32))
}

/// `String.getChars(int, int, char[], int)` (corpus wave 1): copy the
/// UTF-16 code units of the range into the destination char[]. Indices are
/// UTF-16 units (the string model's documented convention); the JDK's
/// StringIndexOutOfBoundsException message is reproduced.
fn n_string_get_chars(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.getChars");
    let (begin, end, dst, dst_begin) = match (args.first(), args.get(1), args.get(2), args.get(3)) {
        (
            Some(Value::Int(b)),
            Some(Value::Int(e)),
            Some(Value::Ref(Some(d))),
            Some(Value::Int(db)),
        ) => (*b, *e, *d, *db),
        (Some(Value::Ref(None)), _, _, _) | (_, _, Some(Value::Ref(None)), _) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
        _ => return Err(internal_error("j2r internal: String.getChars args")),
    };
    let units: Vec<u16> = object::string_value(r).encode_utf16().collect();
    if begin < 0 || end > units.len() as i32 || begin > end {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!(
                "begin {}, end {}, length {}",
                begin,
                end,
                units.len()
            )),
        ));
    }
    for i in begin..end {
        crate::array::array_store(
            &dst,
            dst_begin + (i - begin),
            Value::Int(units[i as usize] as i32),
            ctx,
        )?;
    }
    Ok(Value::Void)
}

fn n_string_hash_code(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.hashCode");
    let mut h: i32 = 0;
    for u in object::string_value(r).encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(u as i32);
    }
    Ok(Value::Int(h))
}

fn n_string_value_of_i(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &arg(args, "String.valueOf").expect_int().to_string(),
    ))))
}

fn n_string_value_of_j(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &arg(args, "String.valueOf").expect_long().to_string(),
    ))))
}

fn n_string_value_of_c(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    let v = arg(args, "String.valueOf").expect_int();
    let c = char::from_u32(v as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
    Ok(Value::Ref(Some(object::new_string(&c.to_string()))))
}

fn n_string_value_of_z(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    let v = arg(args, "String.valueOf").expect_int();
    Ok(Value::Ref(Some(object::new_string(if v != 0 {
        "true"
    } else {
        "false"
    }))))
}

fn n_string_value_of_f(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &javafmt::float_to_string(arg(args, "String.valueOf").expect_float()),
    ))))
}

fn n_string_value_of_d(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &javafmt::double_to_string(arg(args, "String.valueOf").expect_double()),
    ))))
}

fn n_string_value_of_l(_r: Option<&JRef>, args: &[Value], c: Dispatch) -> Result<Value, Throw> {
    let text = concat::format_value(arg(args, "String.valueOf"), 'L', c)?;
    Ok(Value::Ref(Some(object::new_string(&text))))
}

// ---------------------------------------------------------------- StringBuilder

fn sb_text(r: &JRef) -> Arc<String> {
    crate::heap::with_obj(*r, |b| match &b.slots[0] {
        Value::Str(s) => s.clone(),
        other => panic!("j2r internal: StringBuilder slot0 is {other:?}"),
    })
}

fn sb_push_str(r: &JRef, text: &str) {
    crate::heap::with_obj_mut(*r, |b| match &mut b.slots[0] {
        Value::Str(s) => Arc::make_mut(s).push_str(text),
        other => panic!("j2r internal: StringBuilder slot0 is {other:?}"),
    })
}

fn n_sb_init(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.<init>");
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(String::new())));
    Ok(Value::Void)
}

fn n_sb_init_s(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.<init>");
    let o = match arg(args, "StringBuilder.<init>") {
        Value::Ref(Some(o)) if is_string(o) => o,
        _ => {
            return Err(internal_error(
                "StringBuilder.<init>(String) needs a String",
            ))
        }
    };
    let text = object::string_value(o).to_string();
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}

macro_rules! sb_append {
    ($name:ident, $kind:expr) => {
        fn $name(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
            let r = this(recv, "StringBuilder.append");
            let text = concat::format_value(arg(args, "StringBuilder.append"), $kind, ctx)?;
            sb_push_str(r, &text);
            Ok(Value::Ref(Some(r.clone())))
        }
    };
}

sb_append!(n_sb_append_z, 'Z');
sb_append!(n_sb_append_c, 'C');
sb_append!(n_sb_append_i, 'I');
sb_append!(n_sb_append_j, 'J');
sb_append!(n_sb_append_f, 'F');
sb_append!(n_sb_append_d, 'D');
sb_append!(n_sb_append_s, 'L');
sb_append!(n_sb_append_l, 'L');

/// `StringBuilder.append(char[], int, int)` / `append(char[])` (corpus
/// wave 1): copy UTF-16 units from the array into the builder.
fn sb_append_char_range(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.append");
    let (cbuf, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(a))), Some(o), Some(l)) => (*a, o.expect_int(), l.expect_int()),
        (Some(Value::Ref(None)), _, _) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
        _ => return Err(internal_error("j2r internal: StringBuilder.append args")),
    };
    let clen = crate::array::array_length(&cbuf)?;
    if off < 0 || len < 0 || off.checked_add(len).is_none_or(|e| e > clen) {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!("offset {}, count {}, length {}", off, len, clen)),
        ));
    }
    let mut units: Vec<u16> = Vec::with_capacity(len as usize);
    for i in 0..len {
        match crate::array::array_load(&cbuf, off + i)? {
            Value::Int(c) => units.push(c as u16),
            other => {
                return Err(internal_error(&format!(
                    "j2r internal: char[] element {other:?}"
                )))
            }
        }
    }
    sb_push_str(r, &String::from_utf16_lossy(&units));
    Ok(Value::Ref(Some(*r)))
}

fn sb_append_char_all(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let cbuf = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(internal_error("j2r internal: StringBuilder.append args")),
    };
    let len = crate::array::array_length(&cbuf)?;
    sb_append_char_range(
        recv,
        &[Value::Ref(Some(cbuf)), Value::Int(0), Value::Int(len)],
        Dispatch::default(),
    )
}

fn n_sb_to_string(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.toString");
    Ok(Value::Ref(Some(object::string_from_rc(sb_text(r)))))
}

fn n_sb_length(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.length");
    Ok(Value::Int(utf16_units(&sb_text(r)).len() as i32))
}

// ---------------------------------------------------------------- Throwable
//
// Heap layout (audit A2 — suppressed-exception machinery): slot 0
// `detailMessage`, slot 1 `cause`, slot 2 the suppressed list as a Ref to a
// `java/lang/Throwable[]` snapshot in insertion order (null until the first
// `addSuppressed`). `getSuppressed` returns a fresh copy of the snapshot —
// the JVM returns a clone, so mutating the returned array must not affect
// the throwable. Natives registered on `Throwable` resolve for every
// builtin exception family through the parent-chain dispatch, which is what
// makes javac's try-with-resources lowering (JLS 14.20.3 emits
// `Throwable.addSuppressed`) run unmodified.
//
// Exception-message bytes probed on Temurin 21.0.13+9 (see
// docs/AUDIT_VERIFICATION.md, 2026-11-03): "Self-suppression not permitted"
// (IllegalArgumentException, checked before the null case, as in OpenJDK)
// and "Cannot suppress a null exception." (NullPointerException, with the
// trailing period).

fn n_throwable_init(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.<init>");
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = Value::Ref(None);
        o.slots[1] = Value::Ref(None);
        o.slots[2] = Value::Ref(None);
    });
    Ok(Value::Void)
}

fn n_throwable_init_s(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
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
fn n_throwable_init_msg_cause(
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
fn n_throwable_init_cause(
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

fn n_throwable_get_message(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.getMessage");
    Ok(crate::heap::with_obj(*r, |o| o.slots[0].clone()))
}

fn n_throwable_get_cause(
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
fn n_throwable_get_suppressed(
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
fn n_throwable_add_suppressed(
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

fn n_throwable_to_string(
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

thread_local! {
    /// (class, payload-as-i64) -> cached boxed object. Only classes with a
    /// mandatory cache are inserted; Float/Double never consult this.
    static BOXED_CACHE: RefCell<HashMap<(&'static str, i64), JRef>> =
        RefCell::new(HashMap::new());
}

/// Returns the shared boxed object for `(class, v)` when `v` is inside the
/// class's mandatory identity range, allocating it on first use.
fn boxed_cached(class: &'static str, v: i64, lo: i64, hi: i64) -> Option<JRef> {
    if v < lo || v > hi {
        return None;
    }
    BOXED_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(existing) = cache.get(&(class, v)) {
            return Some(*existing);
        }
        let slot = if class == "java/lang/Long" {
            Value::Long(v)
        } else {
            Value::Int(v as i32)
        };
        let r = new_obj(class, 1).ok()?;
        object::obj_set(&r, 0, slot);
        cache.insert((class, v), r);
        Some(r)
    })
}

/// Canonicalizes an f32 the way `Float.floatToIntBits` does: all NaNs become
/// the canonical 0x7fc00000 pattern (negative zero is preserved).
fn float_to_int_bits(v: f32) -> i32 {
    if v.is_nan() {
        return 0x7fc00000u32 as i32;
    }
    v.to_bits() as i32
}

/// Canonicalizes an f64 the way `Double.doubleToLongBits` does.
fn double_to_long_bits(v: f64) -> i64 {
    if v.is_nan() {
        return 0x7ff8000000000000u64 as i64;
    }
    v.to_bits() as i64
}

macro_rules! boxed_class {
    (
        $class:expr, $prim:expr,
        value_of: $value_of:ident, slot_get: $slot_get:ident,
        unbox: $unbox:ident, $slot_ret:ident,
        to_string: $to_string:ident, $kind:expr
        $(, cache: $lo:expr, $hi:expr)?
    ) => {
        // pub(crate): reflect.rs routes Field.get / Method.invoke returns
        // through the same valueOf natives so the identity caches apply.
        pub(crate) fn $value_of(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let v = arg(args, "valueOf").$slot_get();
            $(
                if let Some(r) = boxed_cached($class, v as i64, $lo, $hi) {
                    return Ok(Value::Ref(Some(r)));
                }
            )?
            let r = object::new_obj($class, 1)?;
            object::obj_set(&r, 0, Value::$slot_ret(v));
            Ok(Value::Ref(Some(r)))
        }
        fn $unbox(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let r = this(recv, "unboxing");
            Ok(Value::$slot_ret(
                crate::heap::with_obj(*r, |o| o.slots[0].clone()).$slot_get(),
            ))
        }
        fn $to_string(
            recv: Option<&JRef>,
            _args: &[Value],
            _ctx: Dispatch,
        ) -> Result<Value, Throw> {
            let r = this(recv, "boxed toString");
            let v = crate::heap::with_obj(*r, |o| o.slots[0].clone());
            let text = concat::format_value(&v, $kind, Dispatch::default())?;
            Ok(Value::Ref(Some(object::new_string(&text))))
        }
    };
}

/// `equals` for a boxed class: the argument must be an object of exactly the
/// same class with the same payload (`Integer(1).equals(Long(1L))` is false,
/// like the JDK, because the classes differ).
macro_rules! boxed_equals {
    ($name:ident, $class:expr, $pat:pat => $conv:expr) => {
        fn $name(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let r = this(recv, "boxed equals");
            let mine = crate::heap::with_obj(*r, |o| o.slots[0].clone());
            let other = match &arg(args, "equals") {
                Value::Ref(Some(o)) if crate::heap::with_obj(*o, |obj| &*obj.jclass == $class) => {
                    crate::heap::with_obj(*o, |obj| obj.slots[0].clone())
                }
                _ => return Ok(Value::Int(0)),
            };
            let a: i64 = match mine {
                $pat => $conv,
                _ => return Ok(Value::Int(0)),
            };
            let b: i64 = match other {
                $pat => $conv,
                _ => return Ok(Value::Int(0)),
            };
            Ok(Value::Int((a == b) as i32))
        }
    };
}

boxed_equals!(n_integer_equals, "java/lang/Integer", Value::Int(v) => v as i64);
boxed_equals!(n_long_equals, "java/lang/Long", Value::Long(v) => v);
boxed_equals!(n_byte_equals, "java/lang/Byte", Value::Int(v) => v as i64);
boxed_equals!(n_short_equals, "java/lang/Short", Value::Int(v) => v as i64);
boxed_equals!(n_boolean_equals, "java/lang/Boolean", Value::Int(v) => v as i64);
boxed_equals!(n_character_equals, "java/lang/Character", Value::Int(v) => v as i64);

fn n_float_equals(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Float.equals");
    let mine = crate::heap::with_obj(*r, |o| o.slots[0].clone());
    let other = match &arg(args, "equals") {
        Value::Ref(Some(o))
            if crate::heap::with_obj(*o, |obj| &*obj.jclass == "java/lang/Float") =>
        {
            crate::heap::with_obj(*o, |obj| obj.slots[0].clone())
        }
        _ => return Ok(Value::Int(0)),
    };
    let (a, b) = match (mine, other) {
        (Value::Float(a), Value::Float(b)) => (a, b),
        _ => return Ok(Value::Int(0)),
    };
    // Float.equals compares floatToIntBits: NaN==NaN, +0.0 != -0.0
    Ok(Value::Int(
        (float_to_int_bits(a) == float_to_int_bits(b)) as i32,
    ))
}

fn n_double_equals(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Double.equals");
    let mine = crate::heap::with_obj(*r, |o| o.slots[0].clone());
    let other = match &arg(args, "equals") {
        Value::Ref(Some(o))
            if crate::heap::with_obj(*o, |obj| &*obj.jclass == "java/lang/Double") =>
        {
            crate::heap::with_obj(*o, |obj| obj.slots[0].clone())
        }
        _ => return Ok(Value::Int(0)),
    };
    let (a, b) = match (mine, other) {
        (Value::Double(a), Value::Double(b)) => (a, b),
        _ => return Ok(Value::Int(0)),
    };
    Ok(Value::Int(
        (double_to_long_bits(a) == double_to_long_bits(b)) as i32,
    ))
}

/// `hashCode` for a boxed class holding an Int payload (Integer, Short,
/// Byte, Boolean, Character): the value itself, except Boolean (1231/1237).
macro_rules! boxed_int_hash_code {
    ($name:ident) => {
        fn $name(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let r = this(recv, "boxed hashCode");
            Ok(Value::Int(
                crate::heap::with_obj(*r, |o| o.slots[0].clone()).expect_int(),
            ))
        }
    };
}

boxed_int_hash_code!(n_integer_hash_code);
boxed_int_hash_code!(n_byte_hash_code);
boxed_int_hash_code!(n_short_hash_code);
boxed_int_hash_code!(n_character_hash_code);

fn n_boolean_hash_code(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Boolean.hashCode");
    Ok(Value::Int(
        if crate::heap::with_obj(*r, |o| o.slots[0].clone()).expect_int() != 0 {
            1231
        } else {
            1237
        },
    ))
}

fn n_long_hash_code(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Long.hashCode");
    let v = crate::heap::with_obj(*r, |o| o.slots[0].clone()).expect_long();
    Ok(Value::Int((v ^ (v >> 32)) as i32))
}

fn n_float_hash_code(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Float.hashCode");
    Ok(Value::Int(float_to_int_bits(
        crate::heap::with_obj(*r, |o| o.slots[0].clone()).expect_float(),
    )))
}

fn n_double_hash_code(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Double.hashCode");
    let bits =
        double_to_long_bits(crate::heap::with_obj(*r, |o| o.slots[0].clone()).expect_double());
    Ok(Value::Int((bits ^ (bits >> 32)) as i32))
}

boxed_class! {
    "java/lang/Integer", "I",
    value_of: n_integer_value_of, slot_get: expect_int,
    unbox: n_integer_int_value, Int,
    to_string: n_integer_to_string, 'I', cache: -128, 127
}
boxed_class! {
    "java/lang/Long", "J",
    value_of: n_long_value_of, slot_get: expect_long,
    unbox: n_long_long_value, Long,
    to_string: n_long_to_string, 'J', cache: -128, 127
}
boxed_class! {
    "java/lang/Float", "F",
    value_of: n_float_value_of, slot_get: expect_float,
    unbox: n_float_float_value, Float,
    to_string: n_float_to_string, 'F'
}
boxed_class! {
    "java/lang/Double", "D",
    value_of: n_double_value_of, slot_get: expect_double,
    unbox: n_double_double_value, Double,
    to_string: n_double_to_string, 'D'
}
boxed_class! {
    "java/lang/Byte", "B",
    value_of: n_byte_value_of, slot_get: expect_int,
    unbox: n_byte_byte_value, Int,
    to_string: n_byte_to_string, 'I', cache: -128, 127
}
boxed_class! {
    "java/lang/Short", "S",
    value_of: n_short_value_of, slot_get: expect_int,
    unbox: n_short_short_value, Int,
    to_string: n_short_to_string, 'I', cache: -128, 127
}
boxed_class! {
    "java/lang/Boolean", "Z",
    value_of: n_boolean_value_of, slot_get: expect_int,
    unbox: n_boolean_boolean_value, Int,
    to_string: n_boolean_to_string, 'Z', cache: 0, 1
}
boxed_class! {
    "java/lang/Character", "C",
    value_of: n_character_value_of, slot_get: expect_int,
    unbox: n_character_char_value, Int,
    to_string: n_character_to_string, 'C', cache: 0, 127
}

fn n_integer_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &arg(args, "Integer.toString").expect_int().to_string(),
    ))))
}

/// `Integer.toString(int, int)` — radix form (corpus wave 1). JDK rule:
/// a radix outside 2..=36 falls back to base 10, digits are lowercase.
fn n_integer_to_string_radix(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let i = arg(args, "Integer.toString").expect_int();
    let radix = match args.get(1) {
        Some(v) => v.expect_int(),
        None => {
            return Err(crate::internal_error(
                "j2r internal: Integer.toString radix",
            ))
        }
    };
    let radix = if (2..=36).contains(&radix) { radix } else { 10 };
    Ok(Value::Ref(Some(object::new_string(&int_to_radix(
        i, radix,
    )))))
}

fn int_to_radix(i: i32, radix: i32) -> String {
    if i == 0 {
        return "0".to_string();
    }
    let mut digits = Vec::new();
    let mut v = i as i64;
    if v < 0 {
        // i32::MIN negated safely via i64
        v = -v;
    }
    while v > 0 {
        digits.push(std::char::from_digit((v % radix as i64) as u32, radix as u32).unwrap());
        v /= radix as i64;
    }
    if i < 0 {
        digits.push('-');
    }
    digits.into_iter().rev().collect()
}

fn n_long_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &arg(args, "Long.toString").expect_long().to_string(),
    ))))
}

/// `Long.toString(long, int)` — radix form (corpus wave 1), same JDK
/// fallback rule as the Integer variant.
fn n_long_to_string_radix(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let l = arg(args, "Long.toString").expect_long();
    let radix = match args.get(1) {
        Some(v) => v.expect_int(),
        None => return Err(crate::internal_error("j2r internal: Long.toString radix")),
    };
    let radix = if (2..=36).contains(&radix) { radix } else { 10 };
    if l == 0 {
        return Ok(Value::Ref(Some(object::new_string("0"))));
    }
    let mut digits = Vec::new();
    let mut v: i128 = l as i128;
    if v < 0 {
        v = -v;
    }
    while v > 0 {
        digits.push(std::char::from_digit((v % radix as i128) as u32, radix as u32).unwrap());
        v /= radix as i128;
    }
    if l < 0 {
        digits.push('-');
    }
    digits.reverse();
    let s: String = digits.into_iter().collect();
    Ok(Value::Ref(Some(object::new_string(&s))))
}

fn n_double_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &javafmt::double_to_string(arg(args, "Double.toString").expect_double()),
    ))))
}

fn n_float_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &javafmt::float_to_string(arg(args, "Float.toString").expect_float()),
    ))))
}

fn n_boolean_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        if arg(args, "Boolean.toString").expect_int() != 0 {
            "true"
        } else {
            "false"
        },
    ))))
}

fn n_character_to_string_static(
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

fn n_integer_parse_int(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
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
// KNOWN LIMIT (loud, documented): hexadecimal float literals (0x1.8p1) are
// valid Java but parse as NumberFormatException here - registered as a
// corpus gap until a dedicated hex-float parser lands.
fn n_float_parse_float(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
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

fn n_double_parse_double(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
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

fn n_long_parse_long(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
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
fn n_long_parse_long_radix(
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

fn n_object_get_class(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.getClass");
    let name = crate::heap::with_obj(*r, |o| o.jclass.to_string());
    Ok(crate::reflect::class_value(&name))
}

fn n_class_get_name(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Class.getName");
    let name = reflect::class_name_of(r);
    Ok(Value::Ref(Some(object::new_string(&reflect::binary_name(
        &name,
    )))))
}

fn n_class_get_simple_name(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getSimpleName");
    let name = reflect::class_name_of(r);
    Ok(Value::Ref(Some(object::new_string(&reflect::simple_name(
        &name, ctx,
    )))))
}

fn n_class_to_string(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Class.toString");
    let name = reflect::class_name_of(r);
    Ok(Value::Ref(Some(object::new_string(
        &reflect::to_string_of(&name, ctx),
    ))))
}

fn n_class_is_instance(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Class.isInstance");
    let name = reflect::class_name_of(r);
    let ok = match &arg(args, "Class.isInstance") {
        Value::Ref(None) => false,
        Value::Ref(Some(o)) => {
            let jc = crate::heap::with_obj(*o, |ob| ob.jclass.to_string());
            ctx.is_subtype(&jc, &name)
        }
        _ => return Err(internal_error("Class.isInstance needs a reference")),
    };
    Ok(Value::Int(ok as i32))
}

fn n_class_is_assignable_from(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.isAssignableFrom");
    let name = reflect::class_name_of(r);
    let argref = match &arg(args, "Class.isAssignableFrom") {
        Value::Ref(Some(o)) => o,
        // Temurin: isAssignableFrom(null) is a plain NPE.
        Value::Ref(None) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(internal_error("Class.isAssignableFrom needs a Class")),
    };
    if &*crate::heap::with_obj(*argref, |o| o.jclass.clone()) != "java/lang/Class" {
        // Temurin casts; a non-Class arg is an internal dispatch bug here.
        return Err(internal_error("isAssignableFrom argument is not a Class"));
    }
    let other = reflect::class_name_of(argref);
    Ok(Value::Int(ctx.is_subtype(&other, &name) as i32))
}

fn n_class_is_interface(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.isInterface");
    let name = reflect::class_name_of(r);
    Ok(Value::Int(ctx.is_interface(&name) as i32))
}

fn n_class_is_array(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Class.isArray");
    Ok(Value::Int(reflect::class_name_of(r).starts_with('[') as i32))
}

fn n_class_is_primitive(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.isPrimitive");
    let name = reflect::class_name_of(r);
    // Temurin 21 oracle: void.class.isPrimitive() is TRUE (V.java capture).
    Ok(Value::Int(reflect::prim_display(&name).is_some() as i32))
}

fn n_class_get_superclass(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getSuperclass");
    let name = reflect::class_name_of(r);
    Ok(match reflect::superclass_of(&name, ctx) {
        Some(sup) => reflect::class_value(&sup),
        None => Value::Ref(None),
    })
}

fn n_class_get_component_type(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getComponentType");
    let name = reflect::class_name_of(r);
    Ok(match reflect::component_type_of(&name) {
        Some(elem) => reflect::class_value(&elem),
        None => Value::Ref(None),
    })
}

fn n_class_get_declared_field(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getDeclaredField");
    let owner = reflect::class_name_of(r);
    reflect::get_declared_field(ctx, &owner, args)
}

fn n_class_get_declared_method(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getDeclaredMethod");
    let owner = reflect::class_name_of(r);
    reflect::get_declared_method(ctx, &owner, args)
}

fn n_field_get(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let f = this(recv, "Field.get");
    let obj = match args.first() {
        Some(Value::Ref(r)) => r.as_ref(),
        _ => return Err(internal_error("Field.get needs a receiver argument")),
    };
    reflect::field_get(ctx, f, obj)
}

fn n_field_set(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let f = this(recv, "Field.set");
    let obj = match args.first() {
        Some(Value::Ref(r)) => r.as_ref(),
        _ => return Err(internal_error("Field.set needs a receiver argument")),
    };
    let value = args.get(1).cloned().unwrap_or(Value::Ref(None));
    reflect::field_set(ctx, f, obj, &value)?;
    Ok(Value::Void)
}

fn n_method_invoke(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let m = this(recv, "Method.invoke");
    let target = match args.first() {
        Some(Value::Ref(r)) => r.as_ref(),
        _ => return Err(internal_error("Method.invoke needs a receiver argument")),
    };
    let argv = match args.get(1) {
        Some(Value::Ref(r)) => r.as_ref(),
        _ => return Err(internal_error("Method.invoke needs an args array")),
    };
    reflect::method_invoke(ctx, m, target, argv)
}

fn n_field_get_name(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let f = this(recv, "Field.getName");
    let (_, name, _, _, _, _) = reflect::field_slots(f);
    Ok(Value::Ref(Some(object::new_string(&name))))
}

/// `Field.getType`: map the stored field descriptor to the interned
/// `Class` object. Primitives are keyed by their bare descriptor (`I` ->
/// `int.class`), object descriptors drop the `L...;` wrapper, and array
/// descriptors (`[I`, `[Ljava/lang/String;`) intern as-is — exactly the
/// keys `component_type_of` hands back, so `int[].class.getComponentType()
/// == int.class` holds end to end (BigCache oracle).
fn n_field_get_type(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let f = this(recv, "Field.getType");
    let (_, _, desc, _, _, _) = reflect::field_slots(f);
    let d = desc.as_str();
    let key: &str = if d.starts_with('[') {
        d
    } else if d.starts_with('L') && d.len() >= 2 && d.ends_with(';') {
        &d[1..d.len() - 1]
    } else {
        d
    };
    let r = reflect::class_ref(key);
    Ok(Value::Ref(Some(r)))
}

fn n_method_get_name(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let m = this(recv, "Method.getName");
    let (_, name, _, _) = reflect::method_slots(m);
    Ok(Value::Ref(Some(object::new_string(&name))))
}

/// `Method.equals`: structural (same declaring class, name and descriptor),
/// not identity — Temurin hands back fresh Method objects per lookup
/// (fixture oracle: `m1 == m2` false but `m1.equals(m2)` true).
fn n_method_equals(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let m = this(recv, "Method.equals");
    let same = match &arg(args, "Method.equals") {
        Value::Ref(Some(other)) => {
            crate::heap::with_obj(*other, |o| o.jclass.as_ref() == "java/lang/reflect/Method") && {
                let (o0, o1, o2, _) = reflect::method_slots(other);
                let (s0, s1, s2, _) = reflect::method_slots(m);
                o0 == s0 && o1 == s1 && o2 == s2
            }
        }
        _ => false,
    };
    Ok(Value::Int(same as i32))
}

fn n_system_gc(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    crate::heap::gc_now();
    Ok(Value::Void)
}

// ---------------------------------------------------------------- Table

/// The natives table: `(class, method, descriptor, implementation)`.
/// Classes use internal form (`a/b/C`). Keep sorted-ish by class for review.
pub static NATIVES: &[(&str, &str, &str, NativeFn)] = &[
    // java/lang/Object
    ("java/lang/Object", "<init>", "()V", n_object_init),
    ("java/lang/Object", "wait", "()V", n_object_wait),
    ("java/lang/Object", "wait", "(J)V", n_object_wait),
    ("java/lang/Object", "wait", "(JI)V", n_object_wait),
    ("java/lang/Object", "notify", "()V", n_object_notify),
    ("java/lang/Object", "notifyAll", "()V", n_object_notify_all),
    (
        "java/lang/Object",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_object_equals,
    ),
    ("java/lang/Object", "hashCode", "()I", n_object_hash_code),
    (
        "java/lang/Object",
        "toString",
        "()Ljava/lang/String;",
        n_object_to_string,
    ),
    // java/lang/System
    ("java/lang/System", "exit", "(I)V", n_system_exit),
    (
        "java/lang/System",
        "currentTimeMillis",
        "()J",
        n_system_current_time_millis,
    ),
    ("java/lang/System", "nanoTime", "()J", n_system_nano_time),
    // System.arraycopy (corpus wave 1) — implementation in array.rs, JDK
    // messages verified against Temurin 21.0.13 (see its doc comment).
    (
        "java/lang/System",
        "arraycopy",
        "(Ljava/lang/Object;ILjava/lang/Object;II)V",
        n_system_arraycopy,
    ),
    // java/lang/Math
    ("java/lang/Math", "abs", "(I)I", n_math_abs_i),
    ("java/lang/Math", "abs", "(J)J", n_math_abs_j),
    ("java/lang/Math", "abs", "(F)F", n_math_abs_f),
    ("java/lang/Math", "abs", "(D)D", n_math_abs_d),
    ("java/lang/Math", "max", "(II)I", n_math_max_i),
    ("java/lang/Math", "max", "(JJ)J", n_math_max_j),
    ("java/lang/Math", "max", "(FF)F", n_math_max_f),
    ("java/lang/Math", "max", "(DD)D", n_math_max_d),
    ("java/lang/Math", "min", "(II)I", n_math_min_i),
    ("java/lang/Math", "min", "(JJ)J", n_math_min_j),
    ("java/lang/Math", "min", "(FF)F", n_math_min_f),
    ("java/lang/Math", "min", "(DD)D", n_math_min_d),
    // java/lang/String
    ("java/lang/String", "<init>", "()V", n_string_init),
    (
        "java/lang/String",
        "<init>",
        "(Ljava/lang/String;)V",
        n_string_init_s,
    ),
    // char[] ctors (corpus wave 1)
    ("java/lang/String", "<init>", "([C)V", n_string_init_chars),
    (
        "java/lang/String",
        "<init>",
        "([CII)V",
        n_string_init_chars_range,
    ),
    ("java/lang/String", "length", "()I", n_string_length),
    (
        "java/lang/String",
        "toString",
        "()Ljava/lang/String;",
        n_string_to_string,
    ),
    ("java/lang/String", "isEmpty", "()Z", n_string_is_empty),
    ("java/lang/String", "charAt", "(I)C", n_string_char_at),
    (
        "java/lang/String",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_string_equals,
    ),
    ("java/lang/String", "hashCode", "()I", n_string_hash_code),
    // getChars (corpus wave 1)
    (
        "java/lang/String",
        "getChars",
        "(II[CI)V",
        n_string_get_chars,
    ),
    (
        "java/lang/String",
        "valueOf",
        "(I)Ljava/lang/String;",
        n_string_value_of_i,
    ),
    (
        "java/lang/String",
        "valueOf",
        "(J)Ljava/lang/String;",
        n_string_value_of_j,
    ),
    (
        "java/lang/String",
        "valueOf",
        "(C)Ljava/lang/String;",
        n_string_value_of_c,
    ),
    (
        "java/lang/String",
        "valueOf",
        "(Z)Ljava/lang/String;",
        n_string_value_of_z,
    ),
    (
        "java/lang/String",
        "valueOf",
        "(F)Ljava/lang/String;",
        n_string_value_of_f,
    ),
    (
        "java/lang/String",
        "valueOf",
        "(D)Ljava/lang/String;",
        n_string_value_of_d,
    ),
    (
        "java/lang/String",
        "valueOf",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        n_string_value_of_l,
    ),
    // java/lang/StringBuilder
    ("java/lang/StringBuilder", "<init>", "()V", n_sb_init),
    (
        "java/lang/StringBuilder",
        "<init>",
        "(Ljava/lang/String;)V",
        n_sb_init_s,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(Z)Ljava/lang/StringBuilder;",
        n_sb_append_z,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(C)Ljava/lang/StringBuilder;",
        n_sb_append_c,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(I)Ljava/lang/StringBuilder;",
        n_sb_append_i,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(J)Ljava/lang/StringBuilder;",
        n_sb_append_j,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(F)Ljava/lang/StringBuilder;",
        n_sb_append_f,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(D)Ljava/lang/StringBuilder;",
        n_sb_append_d,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(Ljava/lang/String;)Ljava/lang/StringBuilder;",
        n_sb_append_s,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(Ljava/lang/Object;)Ljava/lang/StringBuilder;",
        n_sb_append_l,
    ),
    // append(char[]) overloads (corpus wave 1)
    (
        "java/lang/StringBuilder",
        "append",
        "([CII)Ljava/lang/StringBuilder;",
        sb_append_char_range,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "([C)Ljava/lang/StringBuilder;",
        sb_append_char_all,
    ),
    (
        "java/lang/StringBuilder",
        "toString",
        "()Ljava/lang/String;",
        n_sb_to_string,
    ),
    ("java/lang/StringBuilder", "length", "()I", n_sb_length),
    // java/lang/Throwable (audit A2: cause + suppressed machinery; the
    // natives resolve for every builtin exception family through the
    // parent-chain dispatch, which is what runs javac's JLS 14.20.3
    // try-with-resources lowering unmodified)
    ("java/lang/Throwable", "<init>", "()V", n_throwable_init),
    (
        "java/lang/Throwable",
        "<init>",
        "(Ljava/lang/String;)V",
        n_throwable_init_s,
    ),
    (
        "java/lang/Throwable",
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Throwable;)V",
        n_throwable_init_msg_cause,
    ),
    (
        "java/lang/Throwable",
        "<init>",
        "(Ljava/lang/Throwable;)V",
        n_throwable_init_cause,
    ),
    (
        "java/lang/Throwable",
        "getMessage",
        "()Ljava/lang/String;",
        n_throwable_get_message,
    ),
    (
        "java/lang/Throwable",
        "getCause",
        "()Ljava/lang/Throwable;",
        n_throwable_get_cause,
    ),
    (
        "java/lang/Throwable",
        "addSuppressed",
        "(Ljava/lang/Throwable;)V",
        n_throwable_add_suppressed,
    ),
    (
        "java/lang/Throwable",
        "getSuppressed",
        "()[Ljava/lang/Throwable;",
        n_throwable_get_suppressed,
    ),
    (
        "java/lang/Throwable",
        "toString",
        "()Ljava/lang/String;",
        n_throwable_to_string,
    ),
    // java/io/PrintStream
    ("java/io/PrintStream", "println", "()V", n_ps_println_n),
    ("java/io/PrintStream", "println", "(Z)V", n_ps_println_z),
    ("java/io/PrintStream", "println", "(C)V", n_ps_println_c),
    ("java/io/PrintStream", "println", "(I)V", n_ps_println_i),
    ("java/io/PrintStream", "println", "(J)V", n_ps_println_j),
    ("java/io/PrintStream", "println", "(F)V", n_ps_println_f),
    ("java/io/PrintStream", "println", "(D)V", n_ps_println_d),
    (
        "java/io/PrintStream",
        "println",
        "(Ljava/lang/String;)V",
        n_ps_println_s,
    ),
    (
        "java/io/PrintStream",
        "println",
        "(Ljava/lang/Object;)V",
        n_ps_println_l,
    ),
    ("java/io/PrintStream", "print", "(Z)V", n_ps_print_z),
    ("java/io/PrintStream", "print", "(C)V", n_ps_print_c),
    ("java/io/PrintStream", "print", "(I)V", n_ps_print_i),
    ("java/io/PrintStream", "print", "(J)V", n_ps_print_j),
    ("java/io/PrintStream", "print", "(F)V", n_ps_print_f),
    ("java/io/PrintStream", "print", "(D)V", n_ps_print_d),
    (
        "java/io/PrintStream",
        "print",
        "(Ljava/lang/String;)V",
        n_ps_print_s,
    ),
    (
        "java/io/PrintStream",
        "print",
        "(Ljava/lang/Object;)V",
        n_ps_print_l,
    ),
    // boxed classes
    (
        "java/lang/Integer",
        "valueOf",
        "(I)Ljava/lang/Integer;",
        n_integer_value_of,
    ),
    ("java/lang/Integer", "intValue", "()I", n_integer_int_value),
    (
        "java/lang/Integer",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_integer_equals,
    ),
    ("java/lang/Integer", "hashCode", "()I", n_integer_hash_code),
    (
        "java/lang/Integer",
        "toString",
        "()Ljava/lang/String;",
        n_integer_to_string,
    ),
    (
        "java/lang/Integer",
        "toString",
        "(I)Ljava/lang/String;",
        n_integer_to_string_static,
    ),
    // radix form (corpus wave 1)
    (
        "java/lang/Integer",
        "toString",
        "(II)Ljava/lang/String;",
        n_integer_to_string_radix,
    ),
    (
        "java/lang/Integer",
        "parseInt",
        "(Ljava/lang/String;)I",
        n_integer_parse_int,
    ),
    (
        "java/lang/Long",
        "valueOf",
        "(J)Ljava/lang/Long;",
        n_long_value_of,
    ),
    ("java/lang/Long", "longValue", "()J", n_long_long_value),
    (
        "java/lang/Long",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_long_equals,
    ),
    ("java/lang/Long", "hashCode", "()I", n_long_hash_code),
    (
        "java/lang/Long",
        "toString",
        "()Ljava/lang/String;",
        n_long_to_string,
    ),
    (
        "java/lang/Long",
        "toString",
        "(J)Ljava/lang/String;",
        n_long_to_string_static,
    ),
    // radix form (corpus wave 1)
    (
        "java/lang/Long",
        "toString",
        "(JI)Ljava/lang/String;",
        n_long_to_string_radix,
    ),
    (
        "java/lang/Long",
        "parseLong",
        "(Ljava/lang/String;)J",
        n_long_parse_long,
    ),
    // radix form (corpus wave 1)
    (
        "java/lang/Long",
        "parseLong",
        "(Ljava/lang/String;I)J",
        n_long_parse_long_radix,
    ),
    // StringReader (corpus wave 1): remaining-string payload model
    (
        "java/io/StringReader",
        "<init>",
        "(Ljava/lang/String;)V",
        io::n_sr_init,
    ),
    ("java/io/StringReader", "read", "()I", io::n_sr_read),
    (
        "java/io/StringReader",
        "read",
        "([CII)I",
        io::n_sr_read_bulk,
    ),
    ("java/io/StringReader", "close", "()V", io::n_sr_close),
    // Reader abstract-declaration markers (compile-time surface walk):
    // every concrete Reader overrides read([CII)I and read()I
    ("java/io/Reader", "read", "()I", io::n_sr_read_marker),
    (
        "java/io/Reader",
        "read",
        "([CII)I",
        io::n_sr_read_bulk_marker,
    ),
    // serialization marker (corpus wave 1): loud-unsupported
    (
        "java/io/ObjectInputStream",
        "defaultReadObject",
        "()V",
        n_object_input_stream_default_read_object,
    ),
    // parseFloat/parseDouble (corpus wave 1)
    (
        "java/lang/Float",
        "parseFloat",
        "(Ljava/lang/String;)F",
        n_float_parse_float,
    ),
    (
        "java/lang/Double",
        "parseDouble",
        "(Ljava/lang/String;)D",
        n_double_parse_double,
    ),
    (
        "java/lang/Float",
        "valueOf",
        "(F)Ljava/lang/Float;",
        n_float_value_of,
    ),
    ("java/lang/Float", "floatValue", "()F", n_float_float_value),
    (
        "java/lang/Float",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_float_equals,
    ),
    ("java/lang/Float", "hashCode", "()I", n_float_hash_code),
    (
        "java/lang/Float",
        "toString",
        "()Ljava/lang/String;",
        n_float_to_string,
    ),
    (
        "java/lang/Float",
        "toString",
        "(F)Ljava/lang/String;",
        n_float_to_string_static,
    ),
    // float predicates (corpus wave 1: minimal-json number formatting)
    ("java/lang/Float", "isInfinite", "(F)Z", n_float_is_infinite),
    ("java/lang/Float", "isNaN", "(F)Z", n_float_is_nan),
    (
        "java/lang/Double",
        "valueOf",
        "(D)Ljava/lang/Double;",
        n_double_value_of,
    ),
    (
        "java/lang/Double",
        "doubleValue",
        "()D",
        n_double_double_value,
    ),
    (
        "java/lang/Double",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_double_equals,
    ),
    ("java/lang/Double", "hashCode", "()I", n_double_hash_code),
    // double predicates (corpus wave 1)
    (
        "java/lang/Double",
        "isInfinite",
        "(D)Z",
        n_double_is_infinite,
    ),
    ("java/lang/Double", "isNaN", "(D)Z", n_double_is_nan),
    (
        "java/lang/Double",
        "toString",
        "()Ljava/lang/String;",
        n_double_to_string,
    ),
    (
        "java/lang/Double",
        "toString",
        "(D)Ljava/lang/String;",
        n_double_to_string_static,
    ),
    (
        "java/lang/Byte",
        "valueOf",
        "(B)Ljava/lang/Byte;",
        n_byte_value_of,
    ),
    ("java/lang/Byte", "byteValue", "()B", n_byte_byte_value),
    (
        "java/lang/Byte",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_byte_equals,
    ),
    ("java/lang/Byte", "hashCode", "()I", n_byte_hash_code),
    (
        "java/lang/Byte",
        "toString",
        "()Ljava/lang/String;",
        n_byte_to_string,
    ),
    (
        "java/lang/Short",
        "valueOf",
        "(S)Ljava/lang/Short;",
        n_short_value_of,
    ),
    ("java/lang/Short", "shortValue", "()S", n_short_short_value),
    (
        "java/lang/Short",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_short_equals,
    ),
    ("java/lang/Short", "hashCode", "()I", n_short_hash_code),
    (
        "java/lang/Short",
        "toString",
        "()Ljava/lang/String;",
        n_short_to_string,
    ),
    (
        "java/lang/Boolean",
        "valueOf",
        "(Z)Ljava/lang/Boolean;",
        n_boolean_value_of,
    ),
    (
        "java/lang/Boolean",
        "booleanValue",
        "()Z",
        n_boolean_boolean_value,
    ),
    (
        "java/lang/Boolean",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_boolean_equals,
    ),
    ("java/lang/Boolean", "hashCode", "()I", n_boolean_hash_code),
    (
        "java/lang/Boolean",
        "toString",
        "()Ljava/lang/String;",
        n_boolean_to_string,
    ),
    (
        "java/lang/Boolean",
        "toString",
        "(Z)Ljava/lang/String;",
        n_boolean_to_string_static,
    ),
    (
        "java/lang/Character",
        "valueOf",
        "(C)Ljava/lang/Character;",
        n_character_value_of,
    ),
    (
        "java/lang/Character",
        "charValue",
        "()C",
        n_character_char_value,
    ),
    (
        "java/lang/Character",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_character_equals,
    ),
    (
        "java/lang/Character",
        "hashCode",
        "()I",
        n_character_hash_code,
    ),
    (
        "java/lang/Character",
        "toString",
        "()Ljava/lang/String;",
        n_character_to_string,
    ),
    (
        "java/lang/Character",
        "toString",
        "(C)Ljava/lang/String;",
        n_character_to_string_static,
    ),
    // ------------------------------------------ corpus wave 1: java.io (F3)
    // Implementations live in `io.rs`. StringWriter real behavior; the
    // Writer rows are abstract-declaration markers for the compile-time
    // surface walk and must never dispatch.
    ("java/io/StringWriter", "<init>", "()V", io::n_sw_init),
    ("java/io/StringWriter", "write", "(I)V", io::n_sw_write_int),
    (
        "java/io/StringWriter",
        "write",
        "([CII)V",
        io::n_sw_write_chars,
    ),
    (
        "java/io/StringWriter",
        "write",
        "([C)V",
        io::n_sw_write_chars_all,
    ),
    (
        "java/io/StringWriter",
        "write",
        "(Ljava/lang/String;)V",
        io::n_sw_write_str,
    ),
    (
        "java/io/StringWriter",
        "write",
        "(Ljava/lang/String;II)V",
        io::n_sw_write_str_range,
    ),
    (
        "java/io/StringWriter",
        "toString",
        "()Ljava/lang/String;",
        io::n_sw_to_string,
    ),
    ("java/io/StringWriter", "close", "()V", io::n_sw_close),
    ("java/io/StringWriter", "flush", "()V", io::n_sw_flush),
    (
        "java/io/Writer",
        "write",
        "(I)V",
        io::n_writer_write_int_marker,
    ),
    (
        "java/io/Writer",
        "write",
        "([CII)V",
        io::n_writer_write_chars_marker,
    ),
    (
        "java/io/Writer",
        "write",
        "([C)V",
        io::n_writer_write_chars_all,
    ),
    (
        "java/io/Writer",
        "write",
        "(Ljava/lang/String;)V",
        io::n_writer_write_str,
    ),
    (
        "java/io/Writer",
        "write",
        "(Ljava/lang/String;II)V",
        io::n_writer_write_str_range_marker,
    ),
    // ------------------------------------------------ M3 Wave 2: collections
    // (implementations live in `collections.rs`; heap-resident state, see
    // the slot-layout table in its module header)
    // java/util/ArrayList
    ("java/util/ArrayList", "<init>", "()V", c::n_al_init),
    ("java/util/ArrayList", "<init>", "(I)V", c::n_al_init),
    // <init>(Collection) — the copy constructor (corpus wave 1)
    (
        "java/util/ArrayList",
        "<init>",
        "(Ljava/util/Collection;)V",
        c::n_al_init_from_collection,
    ),
    (
        "java/util/ArrayList",
        "add",
        "(Ljava/lang/Object;)Z",
        c::n_al_add,
    ),
    (
        "java/util/ArrayList",
        "add",
        "(ILjava/lang/Object;)V",
        c::n_al_add_at,
    ),
    (
        "java/util/ArrayList",
        "get",
        "(I)Ljava/lang/Object;",
        c::n_al_get,
    ),
    (
        "java/util/ArrayList",
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        c::n_al_set,
    ),
    (
        "java/util/ArrayList",
        "remove",
        "(I)Ljava/lang/Object;",
        c::n_al_remove_at,
    ),
    (
        "java/util/ArrayList",
        "remove",
        "(Ljava/lang/Object;)Z",
        c::n_al_remove_obj,
    ),
    ("java/util/ArrayList", "size", "()I", c::n_al_size),
    ("java/util/ArrayList", "isEmpty", "()Z", c::n_al_is_empty),
    // AbstractList hashCode/equals/lastIndexOf (corpus wave 1: JsonObject
    // hash/equals/indexOfByName go through the List-declared static type;
    // the List rows serve the compile-time walk, runtime dispatch lands on
    // the receiver's own row).
    ("java/util/ArrayList", "hashCode", "()I", c::n_al_hash_code),
    ("java/util/List", "hashCode", "()I", c::n_al_hash_code),
    (
        "java/util/ArrayList",
        "equals",
        "(Ljava/lang/Object;)Z",
        c::n_al_equals,
    ),
    (
        "java/util/List",
        "equals",
        "(Ljava/lang/Object;)Z",
        c::n_al_equals,
    ),
    (
        "java/util/ArrayList",
        "lastIndexOf",
        "(Ljava/lang/Object;)I",
        c::n_al_last_index_of,
    ),
    (
        "java/util/List",
        "lastIndexOf",
        "(Ljava/lang/Object;)I",
        c::n_al_last_index_of,
    ),
    (
        "java/util/ArrayList",
        "contains",
        "(Ljava/lang/Object;)Z",
        c::n_al_contains,
    ),
    (
        "java/util/ArrayList",
        "indexOf",
        "(Ljava/lang/Object;)I",
        c::n_al_index_of,
    ),
    ("java/util/ArrayList", "clear", "()V", c::n_al_clear),
    (
        "java/util/ArrayList",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_al_iterator,
    ),
    (
        "java/util/ArrayList",
        "toString",
        "()Ljava/lang/String;",
        c::n_al_to_string,
    ),
    (
        "java/util/ArrayList",
        "forEach",
        "(Ljava/util/function/Consumer;)V",
        c::n_al_for_each,
    ),
    (
        "java/util/ArrayList",
        "removeIf",
        "(Ljava/util/function/Predicate;)Z",
        c::n_al_remove_if,
    ),
    // Mirror rows under the `java/util/List` interface so calls on
    // interface-typed receivers (the same pattern as the Wave-2 List rows
    // above) reach the ArrayList implementations.
    (
        "java/util/List",
        "forEach",
        "(Ljava/util/function/Consumer;)V",
        c::n_al_for_each,
    ),
    (
        "java/util/List",
        "removeIf",
        "(Ljava/util/function/Predicate;)Z",
        c::n_al_remove_if,
    ),
    // java/util/Iterator (the ArrayList$Itr runtime class dispatches here)
    (
        "java/util/ArrayList$Itr",
        "hasNext",
        "()Z",
        c::n_itr_has_next,
    ),
    (
        "java/util/ArrayList$Itr",
        "next",
        "()Ljava/lang/Object;",
        c::n_itr_next,
    ),
    ("java/util/ArrayList$Itr", "remove", "()V", c::n_itr_remove),
    // interface declarations so `Iterator i = ...` / `List l = ...`
    // receiver types resolve (validate_virtual walks to the builtin
    // frontier and needs the method present there)
    ("java/util/Iterator", "hasNext", "()Z", c::n_itr_has_next),
    (
        "java/util/Iterator",
        "next",
        "()Ljava/lang/Object;",
        c::n_itr_next,
    ),
    ("java/util/Iterator", "remove", "()V", c::n_itr_remove),
    // java/util/HashMap
    ("java/util/HashMap", "<init>", "()V", c::n_hm_init),
    ("java/util/HashMap", "<init>", "(I)V", c::n_hm_init),
    (
        "java/util/HashMap",
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_put,
    ),
    (
        "java/util/HashMap",
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_get,
    ),
    (
        "java/util/HashMap",
        "getOrDefault",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_get_or_default,
    ),
    (
        "java/util/HashMap",
        "containsKey",
        "(Ljava/lang/Object;)Z",
        c::n_hm_contains_key,
    ),
    (
        "java/util/HashMap",
        "containsValue",
        "(Ljava/lang/Object;)Z",
        c::n_hm_contains_value,
    ),
    (
        "java/util/HashMap",
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_remove,
    ),
    ("java/util/HashMap", "size", "()I", c::n_hm_size),
    ("java/util/HashMap", "isEmpty", "()Z", c::n_hm_is_empty),
    ("java/util/HashMap", "clear", "()V", c::n_hm_clear),
    (
        "java/util/HashMap",
        "keySet",
        "()Ljava/util/Set;",
        c::n_hm_key_set,
    ),
    (
        "java/util/HashMap",
        "values",
        "()Ljava/util/Collection;",
        c::n_hm_values,
    ),
    (
        "java/util/HashMap",
        "toString",
        "()Ljava/lang/String;",
        c::n_hm_to_string,
    ),
    // java/util/Map interface declarations
    (
        "java/util/Map",
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_put,
    ),
    (
        "java/util/Map",
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_get,
    ),
    (
        "java/util/Map",
        "getOrDefault",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_get_or_default,
    ),
    (
        "java/util/Map",
        "containsKey",
        "(Ljava/lang/Object;)Z",
        c::n_hm_contains_key,
    ),
    (
        "java/util/Map",
        "containsValue",
        "(Ljava/lang/Object;)Z",
        c::n_hm_contains_value,
    ),
    (
        "java/util/Map",
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_remove,
    ),
    ("java/util/Map", "size", "()I", c::n_hm_size),
    ("java/util/Map", "isEmpty", "()Z", c::n_hm_is_empty),
    ("java/util/Map", "clear", "()V", c::n_hm_clear),
    (
        "java/util/Map",
        "keySet",
        "()Ljava/util/Set;",
        c::n_hm_key_set,
    ),
    (
        "java/util/Map",
        "values",
        "()Ljava/util/Collection;",
        c::n_hm_values,
    ),
    // views: KeySet (Set semantics over the map's keys)
    (
        "java/util/HashMap$KeySet",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_ks_iterator,
    ),
    ("java/util/HashMap$KeySet", "size", "()I", c::n_ks_size),
    (
        "java/util/HashMap$KeySet",
        "isEmpty",
        "()Z",
        c::n_ks_is_empty,
    ),
    (
        "java/util/HashMap$KeySet",
        "contains",
        "(Ljava/lang/Object;)Z",
        c::n_ks_contains,
    ),
    (
        "java/util/HashMap$KeySet",
        "toString",
        "()Ljava/lang/String;",
        c::n_ks_to_string,
    ),
    // views: Values (Collection semantics over the map's values)
    (
        "java/util/HashMap$Values",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_vs_iterator,
    ),
    ("java/util/HashMap$Values", "size", "()I", c::n_vs_size),
    (
        "java/util/HashMap$Values",
        "isEmpty",
        "()Z",
        c::n_vs_is_empty,
    ),
    (
        "java/util/HashMap$Values",
        "contains",
        "(Ljava/lang/Object;)Z",
        c::n_vs_contains,
    ),
    (
        "java/util/HashMap$Values",
        "toString",
        "()Ljava/lang/String;",
        c::n_vs_to_string,
    ),
    // hash iterators (KeyIterator over keys, ValueIterator over values)
    (
        "java/util/HashMap$KeyIterator",
        "hasNext",
        "()Z",
        c::n_ki_has_next,
    ),
    (
        "java/util/HashMap$KeyIterator",
        "next",
        "()Ljava/lang/Object;",
        c::n_ki_next,
    ),
    (
        "java/util/HashMap$KeyIterator",
        "remove",
        "()V",
        c::n_itr_remove,
    ),
    (
        "java/util/HashMap$ValueIterator",
        "hasNext",
        "()Z",
        c::n_vi_has_next,
    ),
    (
        "java/util/HashMap$ValueIterator",
        "next",
        "()Ljava/lang/Object;",
        c::n_vi_next,
    ),
    (
        "java/util/HashMap$ValueIterator",
        "remove",
        "()V",
        c::n_itr_remove,
    ),
    // java/util/HashSet (backed by a private HashMap, Java-faithful)
    ("java/util/HashSet", "<init>", "()V", c::n_hs_init),
    ("java/util/HashSet", "<init>", "(I)V", c::n_hs_init),
    (
        "java/util/HashSet",
        "add",
        "(Ljava/lang/Object;)Z",
        c::n_hs_add,
    ),
    (
        "java/util/HashSet",
        "remove",
        "(Ljava/lang/Object;)Z",
        c::n_hs_remove,
    ),
    (
        "java/util/HashSet",
        "contains",
        "(Ljava/lang/Object;)Z",
        c::n_hs_contains,
    ),
    ("java/util/HashSet", "size", "()I", c::n_hs_size),
    ("java/util/HashSet", "isEmpty", "()Z", c::n_hs_is_empty),
    (
        "java/util/HashSet",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_hs_iterator,
    ),
    (
        "java/util/HashSet",
        "toString",
        "()Ljava/lang/String;",
        c::n_hs_to_string,
    ),
    // java/util/Set and java/util/Collection interface declarations
    ("java/util/Set", "add", "(Ljava/lang/Object;)Z", c::n_hs_add),
    (
        "java/util/Set",
        "remove",
        "(Ljava/lang/Object;)Z",
        c::n_hs_remove,
    ),
    (
        "java/util/Set",
        "contains",
        "(Ljava/lang/Object;)Z",
        c::n_hs_contains,
    ),
    ("java/util/Set", "size", "()I", c::n_hs_size),
    ("java/util/Set", "isEmpty", "()Z", c::n_hs_is_empty),
    (
        "java/util/Set",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_hs_iterator,
    ),
    (
        "java/util/List",
        "add",
        "(Ljava/lang/Object;)Z",
        c::n_al_add,
    ),
    (
        "java/util/List",
        "add",
        "(ILjava/lang/Object;)V",
        c::n_al_add_at,
    ),
    (
        "java/util/List",
        "get",
        "(I)Ljava/lang/Object;",
        c::n_al_get,
    ),
    (
        "java/util/List",
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        c::n_al_set,
    ),
    (
        "java/util/List",
        "remove",
        "(I)Ljava/lang/Object;",
        c::n_al_remove_at,
    ),
    (
        "java/util/List",
        "remove",
        "(Ljava/lang/Object;)Z",
        c::n_al_remove_obj,
    ),
    ("java/util/List", "size", "()I", c::n_al_size),
    ("java/util/List", "isEmpty", "()Z", c::n_al_is_empty),
    (
        "java/util/List",
        "contains",
        "(Ljava/lang/Object;)Z",
        c::n_al_contains,
    ),
    (
        "java/util/List",
        "indexOf",
        "(Ljava/lang/Object;)I",
        c::n_al_index_of,
    ),
    ("java/util/List", "clear", "()V", c::n_al_clear),
    (
        "java/util/List",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_al_iterator,
    ),
    (
        "java/util/Collection",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_al_iterator,
    ),
    (
        "java/lang/Iterable",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_al_iterator,
    ),
    // java/util/Objects
    (
        "java/util/Objects",
        "equals",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        c::n_objects_equals,
    ),
    (
        "java/util/Objects",
        "hashCode",
        "(Ljava/lang/Object;)I",
        c::n_objects_hash_code,
    ),
    (
        "java/util/Objects",
        "requireNonNull",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_objects_require_non_null,
    ),
    (
        "java/util/Objects",
        "requireNonNull",
        "(Ljava/lang/Object;Ljava/lang/String;)Ljava/lang/Object;",
        c::n_objects_require_non_null_msg,
    ),
    (
        "java/util/Objects",
        "requireNonNull",
        "(Ljava/lang/Object;Ljava/util/function/Supplier;)Ljava/lang/Object;",
        c::n_objects_require_non_null_supplier,
    ),
    (
        "java/util/Objects",
        "toString",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        c::n_objects_to_string,
    ),
    (
        "java/util/Objects",
        "isNull",
        "(Ljava/lang/Object;)Z",
        c::n_objects_is_null,
    ),
    (
        "java/util/Objects",
        "nonNull",
        "(Ljava/lang/Object;)Z",
        c::n_objects_non_null,
    ),
    // Supplier SAM row (M3 Wave 3): lambda closures dispatch here through
    // `lambda::n_sam` (Wave 2's loud placeholder became the real
    // implementation when closures arrived); user classes with a compiled
    // `get()` dispatch into their code via the registry first.
    (
        "java/util/function/Supplier",
        "get",
        "()Ljava/lang/Object;",
        lambda::n_sam,
    ),
    // ------------------------------------------------ M3 Wave 2: java.lang
    // extensions (implementations in `langext.rs`)
    (
        "java/lang/String",
        "substring",
        "(I)Ljava/lang/String;",
        le::n_str_substring_one,
    ),
    (
        "java/lang/String",
        "substring",
        "(II)Ljava/lang/String;",
        le::n_str_substring_two,
    ),
    (
        "java/lang/String",
        "indexOf",
        "(Ljava/lang/String;)I",
        le::n_str_index_of_str,
    ),
    (
        "java/lang/String",
        "indexOf",
        "(Ljava/lang/String;I)I",
        le::n_str_index_of_str,
    ),
    ("java/lang/String", "indexOf", "(I)I", le::n_str_index_of_ch),
    (
        "java/lang/String",
        "lastIndexOf",
        "(I)I",
        le::n_str_last_index_of_ch,
    ),
    (
        "java/lang/String",
        "concat",
        "(Ljava/lang/String;)Ljava/lang/String;",
        le::n_str_concat,
    ),
    (
        "java/lang/String",
        "replace",
        "(CC)Ljava/lang/String;",
        le::n_str_replace_cc,
    ),
    (
        "java/lang/String",
        "toUpperCase",
        "()Ljava/lang/String;",
        le::n_str_to_upper,
    ),
    (
        "java/lang/String",
        "toLowerCase",
        "()Ljava/lang/String;",
        le::n_str_to_lower,
    ),
    (
        "java/lang/String",
        "trim",
        "()Ljava/lang/String;",
        le::n_str_trim,
    ),
    (
        "java/lang/String",
        "compareTo",
        "(Ljava/lang/String;)I",
        le::n_str_compare_to,
    ),
    (
        "java/lang/String",
        "compareToIgnoreCase",
        "(Ljava/lang/String;)I",
        le::n_str_compare_to_ignore_case,
    ),
    (
        "java/lang/String",
        "startsWith",
        "(Ljava/lang/String;)Z",
        le::n_str_starts_with,
    ),
    (
        "java/lang/String",
        "endsWith",
        "(Ljava/lang/String;)Z",
        le::n_str_ends_with,
    ),
    (
        "java/lang/String",
        "contains",
        "(Ljava/lang/CharSequence;)Z",
        le::n_str_contains,
    ),
    (
        "java/lang/String",
        "equalsIgnoreCase",
        "(Ljava/lang/String;)Z",
        le::n_str_equals_ignore_case,
    ),
    (
        "java/lang/String",
        "toCharArray",
        "()[C",
        le::n_str_to_char_array,
    ),
    (
        "java/lang/String",
        "repeat",
        "(I)Ljava/lang/String;",
        le::n_str_repeat,
    ),
    (
        "java/lang/StringBuilder",
        "<init>",
        "(I)V",
        le::n_sb_init_capacity,
    ),
    (
        "java/lang/StringBuilder",
        "reverse",
        "()Ljava/lang/StringBuilder;",
        le::n_sb_reverse,
    ),
    (
        "java/lang/StringBuilder",
        "insert",
        "(ILjava/lang/String;)Ljava/lang/StringBuilder;",
        le::n_sb_insert_str,
    ),
    (
        "java/lang/StringBuilder",
        "insert",
        "(II)Ljava/lang/StringBuilder;",
        le::n_sb_insert_i,
    ),
    (
        "java/lang/StringBuilder",
        "insert",
        "(IC)Ljava/lang/StringBuilder;",
        le::n_sb_insert_c,
    ),
    (
        "java/lang/StringBuilder",
        "insert",
        "(IJ)Ljava/lang/StringBuilder;",
        le::n_sb_insert_j,
    ),
    (
        "java/lang/StringBuilder",
        "deleteCharAt",
        "(I)Ljava/lang/StringBuilder;",
        le::n_sb_delete_char_at,
    ),
    (
        "java/lang/StringBuilder",
        "delete",
        "(II)Ljava/lang/StringBuilder;",
        le::n_sb_delete,
    ),
    (
        "java/lang/StringBuilder",
        "replace",
        "(IILjava/lang/String;)Ljava/lang/StringBuilder;",
        le::n_sb_replace,
    ),
    (
        "java/lang/StringBuilder",
        "setLength",
        "(I)V",
        le::n_sb_set_length,
    ),
    (
        "java/lang/StringBuilder",
        "capacity",
        "()I",
        le::n_sb_capacity,
    ),
    (
        "java/lang/StringBuilder",
        "charAt",
        "(I)C",
        le::n_sb_char_at,
    ),
    (
        "java/lang/StringBuilder",
        "indexOf",
        "(Ljava/lang/String;)I",
        le::n_sb_index_of,
    ),
    (
        "java/lang/Math",
        "floorDiv",
        "(II)I",
        le::n_math_floor_div_i,
    ),
    (
        "java/lang/Math",
        "floorDiv",
        "(JJ)J",
        le::n_math_floor_div_j,
    ),
    (
        "java/lang/Math",
        "floorMod",
        "(II)I",
        le::n_math_floor_mod_i,
    ),
    (
        "java/lang/Math",
        "floorMod",
        "(JJ)J",
        le::n_math_floor_mod_j,
    ),
    ("java/lang/Math", "round", "(D)J", le::n_math_round_d),
    ("java/lang/Math", "round", "(F)I", le::n_math_round_f),
    ("java/lang/Math", "sqrt", "(D)D", le::n_math_sqrt),
    ("java/lang/Math", "cbrt", "(D)D", le::n_math_cbrt),
    ("java/lang/Math", "pow", "(DD)D", le::n_math_pow),
    ("java/lang/Math", "floor", "(D)D", le::n_math_floor),
    ("java/lang/Math", "ceil", "(D)D", le::n_math_ceil),
    ("java/lang/Math", "signum", "(D)D", le::n_math_signum_d),
    ("java/lang/Math", "signum", "(F)F", le::n_math_signum_f),
    (
        "java/lang/Math",
        "toIntExact",
        "(J)I",
        le::n_math_to_int_exact,
    ),
    (
        "java/lang/Math",
        "addExact",
        "(II)I",
        le::n_math_add_exact_i,
    ),
    (
        "java/lang/Math",
        "subtractExact",
        "(II)I",
        le::n_math_subtract_exact_i,
    ),
    (
        "java/lang/Math",
        "multiplyExact",
        "(II)I",
        le::n_math_multiply_exact_i,
    ),
    (
        "java/lang/Math",
        "addExact",
        "(JJ)J",
        le::n_math_add_exact_j,
    ),
    (
        "java/lang/Math",
        "subtractExact",
        "(JJ)J",
        le::n_math_subtract_exact_j,
    ),
    (
        "java/lang/Math",
        "multiplyExact",
        "(JJ)J",
        le::n_math_multiply_exact_j,
    ),
    (
        "java/lang/Math",
        "addExact",
        "(JI)J",
        le::n_math_add_exact_ji,
    ),
    (
        "java/lang/Math",
        "subtractExact",
        "(JI)J",
        le::n_math_subtract_exact_ji,
    ),
    (
        "java/lang/Math",
        "multiplyExact",
        "(JI)J",
        le::n_math_multiply_exact_ji,
    ),
    (
        "java/lang/Integer",
        "toBinaryString",
        "(I)Ljava/lang/String;",
        le::n_int_to_binary,
    ),
    (
        "java/lang/Integer",
        "toOctalString",
        "(I)Ljava/lang/String;",
        le::n_int_to_octal,
    ),
    (
        "java/lang/Integer",
        "toHexString",
        "(I)Ljava/lang/String;",
        le::n_int_to_hex,
    ),
    ("java/lang/Integer", "compare", "(II)I", le::n_int_compare),
    ("java/lang/Integer", "max", "(II)I", le::n_int_max),
    ("java/lang/Integer", "min", "(II)I", le::n_int_min),
    ("java/lang/Integer", "sum", "(II)I", le::n_int_sum),
    ("java/lang/Integer", "bitCount", "(I)I", le::n_int_bit_count),
    (
        "java/lang/Integer",
        "numberOfLeadingZeros",
        "(I)I",
        le::n_int_nlz,
    ),
    (
        "java/lang/Integer",
        "numberOfTrailingZeros",
        "(I)I",
        le::n_int_ntz,
    ),
    (
        "java/lang/Integer",
        "highestOneBit",
        "(I)I",
        le::n_int_highest_one_bit,
    ),
    (
        "java/lang/Integer",
        "lowestOneBit",
        "(I)I",
        le::n_int_lowest_one_bit,
    ),
    (
        "java/lang/Integer",
        "rotateLeft",
        "(II)I",
        le::n_int_rotate_left,
    ),
    (
        "java/lang/Integer",
        "rotateRight",
        "(II)I",
        le::n_int_rotate_right,
    ),
    ("java/lang/Integer", "reverse", "(I)I", le::n_int_reverse),
    (
        "java/lang/Integer",
        "parseInt",
        "(Ljava/lang/String;I)I",
        le::n_int_parse_int_radix,
    ),
    (
        "java/lang/Long",
        "toBinaryString",
        "(J)Ljava/lang/String;",
        le::n_long_to_binary,
    ),
    (
        "java/lang/Long",
        "toOctalString",
        "(J)Ljava/lang/String;",
        le::n_long_to_octal,
    ),
    (
        "java/lang/Long",
        "toHexString",
        "(J)Ljava/lang/String;",
        le::n_long_to_hex,
    ),
    ("java/lang/Long", "compare", "(JJ)I", le::n_long_compare),
    ("java/lang/Long", "max", "(JJ)J", le::n_long_max),
    ("java/lang/Long", "min", "(JJ)J", le::n_long_min),
    ("java/lang/Long", "sum", "(JJ)J", le::n_long_sum),
    ("java/lang/Long", "bitCount", "(J)I", le::n_long_bit_count),
    (
        "java/lang/Long",
        "numberOfLeadingZeros",
        "(J)I",
        le::n_long_nlz,
    ),
    (
        "java/lang/Long",
        "numberOfTrailingZeros",
        "(J)I",
        le::n_long_ntz,
    ),
    (
        "java/lang/Long",
        "highestOneBit",
        "(J)J",
        le::n_long_highest_one_bit,
    ),
    (
        "java/lang/Long",
        "lowestOneBit",
        "(J)J",
        le::n_long_lowest_one_bit,
    ),
    (
        "java/lang/Long",
        "rotateLeft",
        "(JI)J",
        le::n_long_rotate_left,
    ),
    (
        "java/lang/Long",
        "rotateRight",
        "(JI)J",
        le::n_long_rotate_right,
    ),
    ("java/lang/Long", "reverse", "(J)J", le::n_long_reverse),
    (
        "java/lang/Character",
        "isDigit",
        "(C)Z",
        le::n_char_is_digit,
    ),
    (
        "java/lang/Character",
        "isLetter",
        "(C)Z",
        le::n_char_is_letter,
    ),
    (
        "java/lang/Character",
        "isLetterOrDigit",
        "(C)Z",
        le::n_char_is_letter_or_digit,
    ),
    (
        "java/lang/Character",
        "isWhitespace",
        "(C)Z",
        le::n_char_is_whitespace,
    ),
    (
        "java/lang/Character",
        "isUpperCase",
        "(C)Z",
        le::n_char_is_upper_case,
    ),
    (
        "java/lang/Character",
        "isLowerCase",
        "(C)Z",
        le::n_char_is_lower_case,
    ),
    (
        "java/lang/Character",
        "toUpperCase",
        "(C)C",
        le::n_char_to_upper_case,
    ),
    (
        "java/lang/Character",
        "toLowerCase",
        "(C)C",
        le::n_char_to_lower_case,
    ),
    (
        "java/lang/Character",
        "getNumericValue",
        "(C)I",
        le::n_char_get_numeric_value,
    ),
    (
        "java/lang/Character",
        "compare",
        "(CC)I",
        ju::n_char_compare,
    ),
    ("java/lang/Double", "compare", "(DD)I", ju::n_double_compare),
    // ------------------------------- Real-program JDK surface (jdkutil.rs)
    // Arrays: toString/fill/copyOf/copyOfRange/equals/sort. Static natives —
    // the receiver is always None. Object sorts dispatch comparisons
    // (compareTo / Comparator) so user classes, bridges and lambda
    // comparators all resolve through the ordinary walk.
    (
        "java/util/Arrays",
        "toString",
        "([I)Ljava/lang/String;",
        ju::n_arrays_to_string_int,
    ),
    (
        "java/util/Arrays",
        "toString",
        "([J)Ljava/lang/String;",
        ju::n_arrays_to_string_long,
    ),
    (
        "java/util/Arrays",
        "toString",
        "([D)Ljava/lang/String;",
        ju::n_arrays_to_string_double,
    ),
    (
        "java/util/Arrays",
        "toString",
        "([Z)Ljava/lang/String;",
        ju::n_arrays_to_string_bool,
    ),
    (
        "java/util/Arrays",
        "toString",
        "([C)Ljava/lang/String;",
        ju::n_arrays_to_string_char,
    ),
    (
        "java/util/Arrays",
        "toString",
        "([Ljava/lang/Object;)Ljava/lang/String;",
        ju::n_arrays_to_string_obj,
    ),
    ("java/util/Arrays", "fill", "([II)V", ju::n_arrays_fill_int),
    ("java/util/Arrays", "fill", "([JJ)V", ju::n_arrays_fill_long),
    // char[] fill (corpus wave 1: minimal-json number formatting)
    ("java/util/Arrays", "fill", "([CC)V", ju::n_arrays_fill_char),
    (
        "java/util/Arrays",
        "fill",
        "([Ljava/lang/Object;Ljava/lang/Object;)V",
        ju::n_arrays_fill_obj,
    ),
    (
        "java/util/Arrays",
        "copyOf",
        "([II)[I",
        ju::n_arrays_copy_of_int,
    ),
    (
        "java/util/Arrays",
        "copyOf",
        "([JI)[J",
        ju::n_arrays_copy_of_long,
    ),
    (
        "java/util/Arrays",
        "copyOf",
        "([DI)[D",
        ju::n_arrays_copy_of_double,
    ),
    (
        "java/util/Arrays",
        "copyOf",
        "([Ljava/lang/Object;I)[Ljava/lang/Object;",
        ju::n_arrays_copy_of_obj,
    ),
    (
        "java/util/Arrays",
        "copyOf",
        "([Ljava/lang/Object;ILjava/lang/Class;)[Ljava/lang/Object;",
        ju::n_arrays_copy_of_obj,
    ),
    (
        "java/util/Arrays",
        "copyOfRange",
        "([III)[I",
        ju::n_arrays_copy_of_range_int,
    ),
    (
        "java/util/Arrays",
        "copyOfRange",
        "([Ljava/lang/Object;II)[Ljava/lang/Object;",
        ju::n_arrays_copy_of_range_obj,
    ),
    (
        "java/util/Arrays",
        "equals",
        "([I[I)Z",
        ju::n_arrays_equals_int,
    ),
    (
        "java/util/Arrays",
        "equals",
        "([J[J)Z",
        ju::n_arrays_equals_long,
    ),
    (
        "java/util/Arrays",
        "equals",
        "([Ljava/lang/Object;[Ljava/lang/Object;)Z",
        ju::n_arrays_equals_obj,
    ),
    ("java/util/Arrays", "sort", "([I)V", ju::n_arrays_sort_int),
    ("java/util/Arrays", "sort", "([J)V", ju::n_arrays_sort_long),
    (
        "java/util/Arrays",
        "sort",
        "([D)V",
        ju::n_arrays_sort_double,
    ),
    (
        "java/util/Arrays",
        "sort",
        "([Ljava/lang/Object;)V",
        ju::n_arrays_sort_obj,
    ),
    (
        "java/util/Arrays",
        "sort",
        "([Ljava/lang/Object;Ljava/util/Comparator;)V",
        ju::n_arrays_sort_obj_cmp,
    ),
    // Collections: sort (comparator + natural), reverse, swap — dispatch
    // through List.get/set so any List implementation works.
    (
        "java/util/Collections",
        "sort",
        "(Ljava/util/List;Ljava/util/Comparator;)V",
        ju::n_collections_sort_cmp,
    ),
    (
        "java/util/Collections",
        "sort",
        "(Ljava/util/List;)V",
        ju::n_collections_sort_natural,
    ),
    (
        "java/util/Collections",
        "reverse",
        "(Ljava/util/List;)V",
        ju::n_collections_reverse,
    ),
    (
        "java/util/Collections",
        "swap",
        "(Ljava/util/List;II)V",
        ju::n_collections_swap,
    ),
    (
        "java/util/Collections",
        "addAll",
        "(Ljava/util/Collection;[Ljava/lang/Object;)Z",
        ju::n_collections_add_all,
    ),
    // ArrayList.subList + the live view's own surface. The view delegates
    // through List dispatch and enforces parent modCount discipline.
    (
        "java/util/ArrayList",
        "subList",
        "(II)Ljava/util/List;",
        ju::n_al_sub_list,
    ),
    // List-declared row: javac emits invokeinterface List.subList when the
    // static type is List — compile-time validation walks the DECLARED
    // hierarchy, runtime dispatch walks the receiver's (ArrayList) chain.
    (
        "java/util/List",
        "subList",
        "(II)Ljava/util/List;",
        ju::n_al_sub_list,
    ),
    (
        "java/util/ArrayList$SubList",
        "get",
        "(I)Ljava/lang/Object;",
        ju::n_sl_get,
    ),
    (
        "java/util/ArrayList$SubList",
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        ju::n_sl_set,
    ),
    (
        "java/util/ArrayList$SubList",
        "add",
        "(ILjava/lang/Object;)V",
        ju::n_sl_add_at,
    ),
    (
        "java/util/ArrayList$SubList",
        "add",
        "(Ljava/lang/Object;)Z",
        ju::n_sl_add_append,
    ),
    (
        "java/util/ArrayList$SubList",
        "remove",
        "(I)Ljava/lang/Object;",
        ju::n_sl_remove_at,
    ),
    ("java/util/ArrayList$SubList", "size", "()I", ju::n_sl_size),
    (
        "java/util/ArrayList$SubList",
        "isEmpty",
        "()Z",
        ju::n_sl_is_empty,
    ),
    (
        "java/util/ArrayList$SubList",
        "toString",
        "()Ljava/lang/String;",
        ju::n_sl_to_string,
    ),
    // Collections.unmodifiableList + the view's surface (corpus wave 1:
    // minimal-json). Reads delegate to the backing list; writes throw
    // UnsupportedOperationException — JDK wrapper semantics.
    (
        "java/util/Collections",
        "unmodifiableList",
        "(Ljava/util/List;)Ljava/util/List;",
        ju::n_unmod_list,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "get",
        "(I)Ljava/lang/Object;",
        ju::n_unmod_get,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        ju::n_unmod_set,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "add",
        "(Ljava/lang/Object;)Z",
        ju::n_unmod_add,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "add",
        "(ILjava/lang/Object;)V",
        ju::n_unmod_add_at,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "remove",
        "(I)Ljava/lang/Object;",
        ju::n_unmod_remove_at,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "remove",
        "(Ljava/lang/Object;)Z",
        ju::n_unmod_remove_obj,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "clear",
        "()V",
        ju::n_unmod_clear,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "size",
        "()I",
        ju::n_unmod_size,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "isEmpty",
        "()Z",
        ju::n_unmod_is_empty,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "contains",
        "(Ljava/lang/Object;)Z",
        ju::n_unmod_contains,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "indexOf",
        "(Ljava/lang/Object;)I",
        ju::n_unmod_index_of,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "iterator",
        "()Ljava/util/Iterator;",
        ju::n_unmod_iterator,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "toString",
        "()Ljava/lang/String;",
        ju::n_unmod_to_string,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "hashCode",
        "()I",
        ju::n_unmod_hash_code,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "equals",
        "(Ljava/lang/Object;)Z",
        ju::n_unmod_equals,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "lastIndexOf",
        "(Ljava/lang/Object;)I",
        ju::n_unmod_last_index_of,
    ),
    (
        "java/lang/String",
        "compareTo",
        "(Ljava/lang/Object;)I",
        n_str_compare_to_bridge,
    ),
    (
        "java/lang/Integer",
        "compareTo",
        "(Ljava/lang/Object;)I",
        n_integer_compare_to_bridge,
    ),
    (
        "java/lang/Long",
        "compareTo",
        "(Ljava/lang/Object;)I",
        n_long_compare_to_bridge,
    ),
    (
        "java/lang/Double",
        "compareTo",
        "(Ljava/lang/Object;)I",
        n_double_compare_to_bridge,
    ),
    // ------------------------------------------------ M3 Wave 3: lambdas
    // One SAM row per interface in the M3 set (M3_PLAN §6.1). Closure
    // objects carry their interface as the object class, so the dispatch
    // walk reaches these rows directly; the shared implementation
    // reassembles the recorded target call (see `lambda.rs`).
    (
        "java/util/function/Function",
        "apply",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        lambda::n_sam,
    ),
    (
        "java/util/function/BiFunction",
        "apply",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        lambda::n_sam,
    ),
    (
        "java/util/function/Consumer",
        "accept",
        "(Ljava/lang/Object;)V",
        lambda::n_sam,
    ),
    (
        "java/util/function/BiConsumer",
        "accept",
        "(Ljava/lang/Object;Ljava/lang/Object;)V",
        lambda::n_sam,
    ),
    (
        "java/util/Comparator",
        "compare",
        "(Ljava/lang/Object;Ljava/lang/Object;)I",
        lambda::n_sam,
    ),
    (
        "java/util/function/Predicate",
        "test",
        "(Ljava/lang/Object;)Z",
        lambda::n_sam,
    ),
    (
        "java/util/function/BiPredicate",
        "test",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        lambda::n_sam,
    ),
    (
        "java/util/function/UnaryOperator",
        "apply",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        lambda::n_sam,
    ),
    (
        "java/util/function/BinaryOperator",
        "apply",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        lambda::n_sam,
    ),
    // M3 Wave 5: reflection subset (M3_PLAN §8)
    (
        "java/lang/Object",
        "getClass",
        "()Ljava/lang/Class;",
        n_object_get_class,
    ),
    (
        "java/lang/Class",
        "getName",
        "()Ljava/lang/String;",
        n_class_get_name,
    ),
    (
        "java/lang/Class",
        "getSimpleName",
        "()Ljava/lang/String;",
        n_class_get_simple_name,
    ),
    (
        "java/lang/Class",
        "toString",
        "()Ljava/lang/String;",
        n_class_to_string,
    ),
    (
        "java/lang/Class",
        "isInstance",
        "(Ljava/lang/Object;)Z",
        n_class_is_instance,
    ),
    (
        "java/lang/Class",
        "isAssignableFrom",
        "(Ljava/lang/Class;)Z",
        n_class_is_assignable_from,
    ),
    (
        "java/lang/Class",
        "isInterface",
        "()Z",
        n_class_is_interface,
    ),
    ("java/lang/Class", "isArray", "()Z", n_class_is_array),
    (
        "java/lang/Class",
        "isPrimitive",
        "()Z",
        n_class_is_primitive,
    ),
    (
        "java/lang/Class",
        "getSuperclass",
        "()Ljava/lang/Class;",
        n_class_get_superclass,
    ),
    (
        "java/lang/Class",
        "getComponentType",
        "()Ljava/lang/Class;",
        n_class_get_component_type,
    ),
    (
        "java/lang/Class",
        "getDeclaredField",
        "(Ljava/lang/String;)Ljava/lang/reflect/Field;",
        n_class_get_declared_field,
    ),
    (
        "java/lang/Class",
        "getDeclaredMethod",
        "(Ljava/lang/String;[Ljava/lang/Class;)Ljava/lang/reflect/Method;",
        n_class_get_declared_method,
    ),
    (
        "java/lang/reflect/Field",
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        n_field_get,
    ),
    (
        "java/lang/reflect/Field",
        "set",
        "(Ljava/lang/Object;Ljava/lang/Object;)V",
        n_field_set,
    ),
    (
        "java/lang/reflect/Field",
        "getName",
        "()Ljava/lang/String;",
        n_field_get_name,
    ),
    (
        "java/lang/reflect/Field",
        "getType",
        "()Ljava/lang/Class;",
        n_field_get_type,
    ),
    (
        "java/lang/reflect/Method",
        "getName",
        "()Ljava/lang/String;",
        n_method_get_name,
    ),
    (
        "java/lang/reflect/Method",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_method_equals,
    ),
    (
        "java/lang/reflect/Method",
        "invoke",
        "(Ljava/lang/Object;[Ljava/lang/Object;)Ljava/lang/Object;",
        n_method_invoke,
    ),
    ("java/lang/System", "gc", "()V", n_system_gc),
];

/// Exact lookup: `(class, name, descriptor)`.
pub fn lookup_exact(class: &str, name: &str, desc: &str) -> Option<NativeFn> {
    NATIVES
        .iter()
        .find(|(c, n, d, _)| *c == class && *n == name && *d == desc)
        .map(|(_, _, _, f)| *f)
}

/// Calls an exact native entry (used by statically-resolved
/// `invokestatic`/`invokespecial` emission). Missing entries are an internal
/// error because the code generator validated them at compile time.
pub fn invoke_exact(
    class: &str,
    name: &str,
    desc: &str,
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    match lookup_exact(class, name, desc) {
        Some(f) => f(recv, args, ctx),
        None => Err(internal_error(&format!(
            "native method {class}.{name}{desc} not found"
        ))),
    }
}

/// `java/lang/System.out` as a `Value`.
pub fn system_out() -> Value {
    Value::Ref(Some(object::system_out_ref()))
}

/// `java/lang/System.err` as a `Value`.
pub fn system_err() -> Value {
    Value::Ref(Some(object::system_err_ref()))
}

/// Collector hook: pushes the indices of every object held by the runtime's
/// own permanent structures. The JLS 5.1.7 boxed-identity cache entries are
/// permanent roots — `Integer.valueOf(127)` must return the same object for
/// the whole process lifetime, across any number of collections.
pub fn scan_runtime_roots(out: &mut Vec<u32>) {
    BOXED_CACHE.with(|c| {
        for r in c.borrow().values() {
            out.push(r.index());
        }
    });
}

/// Test-only access to the boxed cache (heap unit tests use it to prove the
/// cache is scanned as a root set).
#[cfg(test)]
pub fn boxed_cached_for_test(v: i64) -> Option<crate::heap::JRef> {
    boxed_cached("java/lang/Integer", v, -128, 127)
}

// --- Real-program surface: natural-ordering bridge rows --------------------
// `Collections.sort`/`Arrays.sort(Object[])` dispatch `compareTo` with the
// erased `(Ljava/lang/Object;)I` bridge descriptor. User classes carry
// javac's compiled bridges already; the builtin boxes and String need these
// rows (the dispatch walk matches exact descriptors only). Instance methods:
// the receiver is the boxed self, args[0] is the boxed other — unbox both
// from their slot-0 payloads.

fn n_str_compare_to_bridge(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    le::n_str_compare_to(recv, args, ctx)
}

fn n_integer_compare_to_bridge(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = box_payload_int(recv, "Integer.compareTo");
    let b = match args.first() {
        Some(Value::Ref(Some(r))) => box_payload_int(Some(r), "Integer.compareTo arg"),
        _ => return Err(throw_new("java/lang/NullPointerException", None)),
    };
    Ok(Value::Int(a.cmp(&b) as i32))
}

fn n_long_compare_to_bridge(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = box_payload_long(recv, "Long.compareTo");
    let b = match args.first() {
        Some(Value::Ref(Some(r))) => box_payload_long(Some(r), "Long.compareTo arg"),
        _ => return Err(throw_new("java/lang/NullPointerException", None)),
    };
    Ok(Value::Int(a.cmp(&b) as i32))
}

fn n_double_compare_to_bridge(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = box_payload_double(recv, "Double.compareTo");
    let b = match args.first() {
        Some(Value::Ref(Some(r))) => box_payload_double(Some(r), "Double.compareTo arg"),
        _ => return Err(throw_new("java/lang/NullPointerException", None)),
    };
    Ok(Value::Int(double_total_cmp(a, b)))
}

/// Java's total order on doubles: -0.0 < +0.0 (partial_cmp calls them
/// equal — the oracle sorts [-0.0, 0.0] differently), NaN > everything,
/// NaN == NaN.
pub fn double_total_cmp(a: f64, b: f64) -> i32 {
    if a == 0.0 && b == 0.0 {
        return match (a.is_sign_negative(), b.is_sign_negative()) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };
    }
    match a.partial_cmp(&b) {
        Some(ord) => ord as i32,
        None => {
            let a_nan = a.is_nan();
            let b_nan = b.is_nan();
            if a_nan && b_nan {
                0
            } else if a_nan {
                1
            } else {
                -1
            }
        }
    }
}

fn box_payload_int(r: Option<&JRef>, meth: &str) -> i32 {
    match r {
        Some(r) => with_obj(*r, |o| o.slots[0].expect_int()),
        None => panic!("j2r internal: {meth} without receiver"),
    }
}

fn box_payload_long(r: Option<&JRef>, meth: &str) -> i64 {
    match r {
        Some(r) => with_obj(*r, |o| o.slots[0].expect_long()),
        None => panic!("j2r internal: {meth} without receiver"),
    }
}

fn box_payload_double(r: Option<&JRef>, meth: &str) -> f64 {
    match r {
        Some(r) => with_obj(*r, |o| o.slots[0].expect_double()),
        None => panic!("j2r internal: {meth} without receiver"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_ctx() -> Dispatch {
        Dispatch::default()
    }

    fn boxed(class: &str, v: Value) -> JRef {
        let r = new_obj(class, 1).unwrap();
        object::obj_set(&r, 0, v);
        r
    }

    #[test]
    fn integer_value_of_caches_minus128_to_127() {
        let f = |args: &[Value]| match n_integer_value_of(None, args, no_ctx()).unwrap() {
            Value::Ref(Some(r)) => r,
            other => panic!("expected a reference, got {other:?}"),
        };
        let a = f(&[Value::Int(127)]);
        let b = f(&[Value::Int(127)]);
        assert!(crate::ref_eq(&Value::Ref(Some(a)), &Value::Ref(Some(b))));
        let c = f(&[Value::Int(128)]);
        let d = f(&[Value::Int(128)]);
        assert!(!crate::ref_eq(&Value::Ref(Some(c)), &Value::Ref(Some(d))));
        let neg = f(&[Value::Int(-128)]);
        let neg2 = f(&[Value::Int(-128)]);
        assert!(crate::ref_eq(
            &Value::Ref(Some(neg)),
            &Value::Ref(Some(neg2))
        ));
    }

    #[test]
    fn boxed_equals_compares_payload_and_type() {
        let i1 = boxed("java/lang/Integer", Value::Int(1000));
        let i2 = boxed("java/lang/Integer", Value::Int(1000));
        let r = n_integer_equals(Some(&i1), &[Value::Ref(Some(i2))], no_ctx()).unwrap();
        assert_eq!(r.expect_int(), 1);
        let l = boxed("java/lang/Long", Value::Long(1000));
        let r = n_integer_equals(Some(&i1), &[Value::Ref(Some(l))], no_ctx()).unwrap();
        // Integer(1000).equals(Long(1000)) is false in the JDK
        assert_eq!(r.expect_int(), 0);
        let i3 = boxed("java/lang/Integer", Value::Int(7));
        let r = n_integer_equals(Some(&i1), &[Value::Ref(Some(i3))], no_ctx()).unwrap();
        assert_eq!(r.expect_int(), 0);
    }

    #[test]
    fn float_equals_uses_bits_nan_is_nan() {
        let nan: f32 = f32::NAN;
        let a = boxed("java/lang/Float", Value::Float(nan));
        let b = boxed("java/lang/Float", Value::Float(nan));
        let r = n_float_equals(Some(&a), &[Value::Ref(Some(b))], no_ctx()).unwrap();
        // Float.equals: NaN equals NaN
        assert_eq!(r.expect_int(), 1);
        let z = boxed("java/lang/Float", Value::Float(0.0));
        let nz = boxed("java/lang/Float", Value::Float(-0.0));
        let r = n_float_equals(Some(&z), &[Value::Ref(Some(nz))], no_ctx()).unwrap();
        // +0.0 and -0.0 are NOT equal under Float.equals
        assert_eq!(r.expect_int(), 0);
    }

    #[test]
    fn boxed_hash_codes_match_jdk_contract() {
        let i = boxed("java/lang/Integer", Value::Int(42));
        assert_eq!(
            n_integer_hash_code(Some(&i), &[], no_ctx())
                .unwrap()
                .expect_int(),
            42
        );
        let l = boxed("java/lang/Long", Value::Long(0x1_0000_0042));
        // Long.hashCode: (int)(v ^ (v >>> 32))
        let v: i64 = 0x1_0000_0042;
        assert_eq!(
            n_long_hash_code(Some(&l), &[], no_ctx())
                .unwrap()
                .expect_int(),
            (v ^ (v >> 32)) as i32
        );
        let t = boxed("java/lang/Boolean", Value::Int(1));
        assert_eq!(
            n_boolean_hash_code(Some(&t), &[], no_ctx())
                .unwrap()
                .expect_int(),
            1231
        );
        let f = boxed("java/lang/Boolean", Value::Int(0));
        assert_eq!(
            n_boolean_hash_code(Some(&f), &[], no_ctx())
                .unwrap()
                .expect_int(),
            1237
        );
    }

    #[test]
    fn lookup_finds_exact_entries() {
        assert!(lookup_exact("java/lang/Math", "max", "(II)I").is_some());
        assert!(lookup_exact("java/lang/Math", "max", "(IJ)I").is_none());
        assert!(lookup_exact("java/io/PrintStream", "println", "(Ljava/lang/String;)V").is_some());
    }

    #[test]
    fn string_hash_code_matches_java() {
        let s = object::new_string("hello");
        let r = n_string_hash_code(Some(&s), &[], no_ctx()).unwrap();
        assert_eq!(r.expect_int(), 99162322);
        let e = object::new_string("");
        assert_eq!(
            n_string_hash_code(Some(&e), &[], no_ctx())
                .unwrap()
                .expect_int(),
            0
        );
    }

    #[test]
    fn utf16_length_semantics() {
        // U+1F600 is one code point, two UTF-16 units — Java length() == 2
        let s = object::new_string("\u{1F600}");
        assert_eq!(
            n_string_length(Some(&s), &[], no_ctx())
                .unwrap()
                .expect_int(),
            2
        );
    }

    #[test]
    fn parse_int_java_semantics() {
        let s = object::new_string("42");
        assert_eq!(
            n_integer_parse_int(None, &[Value::Ref(Some(s))], no_ctx())
                .unwrap()
                .expect_int(),
            42
        );
        let neg = object::new_string("-2147483648");
        assert_eq!(
            n_integer_parse_int(None, &[Value::Ref(Some(neg))], no_ctx())
                .unwrap()
                .expect_int(),
            i32::MIN
        );
        let over = object::new_string("2147483648");
        let err = n_integer_parse_int(None, &[Value::Ref(Some(over))], no_ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/NumberFormatException");
        let empty = object::new_string("");
        assert!(n_integer_parse_int(None, &[Value::Ref(Some(empty))], no_ctx()).is_err());
        let plus = object::new_string("+7");
        assert_eq!(
            n_integer_parse_int(None, &[Value::Ref(Some(plus))], no_ctx())
                .unwrap()
                .expect_int(),
            7
        );
    }

    #[test]
    fn math_max_nan_and_zero_signs() {
        let nan = f64::NAN;
        assert!(java_max_d(nan, 1.0).is_nan());
        assert_eq!(java_max_d(0.0, -0.0), 0.0f64.copysign(1.0));
        assert_eq!(java_min_d(0.0, -0.0).to_bits(), (-0.0f64).to_bits());
    }

    #[test]
    fn boxed_roundtrip() {
        let b = n_integer_value_of(None, &[Value::Int(5)], no_ctx()).unwrap();
        let r = b.expect_ref().unwrap();
        assert_eq!(
            crate::heap::with_obj(r, |o| o.jclass.to_string()),
            "java/lang/Integer"
        );
        assert_eq!(
            n_integer_int_value(Some(&r), &[], no_ctx())
                .unwrap()
                .expect_int(),
            5
        );
    }

    #[test]
    fn sb_append_and_to_string() {
        let sb = object::new_builtin("java/lang/StringBuilder").unwrap();
        n_sb_init(Some(&sb), &[], no_ctx()).unwrap();
        n_sb_append_i(Some(&sb), &[Value::Int(12)], no_ctx()).unwrap();
        n_sb_append_s(
            Some(&sb),
            &[Value::Ref(Some(object::new_string(" x")))],
            no_ctx(),
        )
        .unwrap();
        let out = n_sb_to_string(Some(&sb), &[], no_ctx()).unwrap();
        assert_eq!(&*object::string_value(&out.expect_ref().unwrap()), "12 x");
    }

    #[test]
    fn println_writes_to_stdout() {
        let ps = object::system_out_ref();
        n_ps_println_s(
            Some(&ps),
            &[Value::Ref(Some(object::new_string(
                "natives println works",
            )))],
            no_ctx(),
        )
        .unwrap();
    }

    #[test]
    fn throwable_to_string_format() {
        let t = throw_new("pkg/MyError", Some("boom"));
        let out = n_throwable_to_string(Some(&t.obj), &[], no_ctx()).unwrap();
        assert_eq!(
            &*object::string_value(&out.expect_ref().unwrap()),
            "pkg.MyError: boom"
        );
    }
}
