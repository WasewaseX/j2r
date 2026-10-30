//! The natives table: JDK methods implemented directly in Rust.
//!
//! The code generator validates every method reference against this table
//! (for `java/*` classes) at compile time, so an unsupported JDK method is a
//! *compile error* with a clear message — never a silent miscompile.
//!
//! Conventions:
//! * instance natives get `recv = Some(&JRef)`, statics get `None`,
//! * `java/lang/String` and `StringBuilder` store their characters as
//!   `Value::Str` in slot 0 (copy-on-write via `Rc::make_mut`),
//! * `Throwable.detailMessage` is slot 0 (`Value::Ref` to a String or null),
//! * boxed classes store their payload in slot 0,
//! * `java/io/PrintStream` stores its sink id (1 = stdout, 2 = stderr) in
//!   slot 0,
//! * string lengths, indices and hashes follow Java's UTF-16 semantics.

use crate::concat;
use crate::object::{self, is_string, JRef};
use crate::Value;
use crate::{internal_error, javafmt, throw_new, Dispatch, NativeFn, Throw};
use std::rc::Rc;

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
    let sink = this(recv, "PrintStream method").borrow().slots[0].expect_int();
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

fn n_object_hash_code(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.hashCode");
    Ok(Value::Int(Rc::as_ptr(r) as *const u8 as usize as i32))
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
    let cls = dotted(&r.borrow().jclass);
    let hash = Rc::as_ptr(r) as *const u8 as usize as i32;
    Ok(Value::Ref(Some(object::new_string(&format!(
        "{cls}@{}",
        javafmt::int_to_hex_string(hash)
    )))))
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
    r.borrow_mut().slots[0] = Value::Str(Rc::new(String::new()));
    Ok(Value::Void)
}

fn n_string_init_s(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    let o = match arg(args, "String.<init>") {
        Value::Ref(Some(o)) if is_string(o) => o,
        _ => return Err(internal_error("String.<init>(String) needs a String")),
    };
    let text = object::string_value(o).to_string();
    r.borrow_mut().slots[0] = Value::Str(Rc::new(text));
    Ok(Value::Void)
}

fn n_string_length(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.length");
    Ok(Value::Int(
        utf16_units(&object::string_value(r)).len() as i32
    ))
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

fn sb_text(r: &JRef) -> Rc<String> {
    let b = r.borrow();
    match &b.slots[0] {
        Value::Str(s) => s.clone(),
        other => panic!("j2r internal: StringBuilder slot0 is {other:?}"),
    }
}

fn sb_push_str(r: &JRef, text: &str) {
    let mut b = r.borrow_mut();
    match &mut b.slots[0] {
        Value::Str(s) => Rc::make_mut(s).push_str(text),
        other => panic!("j2r internal: StringBuilder slot0 is {other:?}"),
    }
}

fn n_sb_init(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.<init>");
    r.borrow_mut().slots[0] = Value::Str(Rc::new(String::new()));
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
    r.borrow_mut().slots[0] = Value::Str(Rc::new(text));
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

fn n_sb_to_string(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.toString");
    Ok(Value::Ref(Some(object::string_from_rc(sb_text(r)))))
}

fn n_sb_length(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.length");
    Ok(Value::Int(utf16_units(&sb_text(r)).len() as i32))
}

// ---------------------------------------------------------------- Throwable

fn n_throwable_init(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.<init>");
    r.borrow_mut().slots[0] = Value::Ref(None);
    Ok(Value::Void)
}

fn n_throwable_init_s(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.<init>");
    let msg = match arg(args, "Throwable.<init>") {
        Value::Ref(None) => Value::Ref(None),
        Value::Ref(Some(o)) if is_string(o) => Value::Ref(Some(o.clone())),
        other => return Err(internal_error(&format!("Throwable.<init> arg {other:?}"))),
    };
    r.borrow_mut().slots[0] = msg;
    Ok(Value::Void)
}

fn n_throwable_get_message(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.getMessage");
    Ok(r.borrow().slots[0].clone())
}

fn n_throwable_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.toString");
    let cls = dotted(&r.borrow().jclass);
    let msg = match &r.borrow().slots[0] {
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

macro_rules! boxed_class {
    (
        $class:expr, $prim:expr,
        value_of: $value_of:ident, slot_get: $slot_get:ident,
        unbox: $unbox:ident, $slot_ret:ident,
        to_string: $to_string:ident, $kind:expr
    ) => {
        fn $value_of(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let v = arg(args, "valueOf").$slot_get();
            let r = object::new_obj($class, 1)?;
            object::obj_set(&r, 0, Value::$slot_ret(v));
            Ok(Value::Ref(Some(r)))
        }
        fn $unbox(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let r = this(recv, "unboxing");
            Ok(Value::$slot_ret(r.borrow().slots[0].$slot_get()))
        }
        fn $to_string(
            recv: Option<&JRef>,
            _args: &[Value],
            _ctx: Dispatch,
        ) -> Result<Value, Throw> {
            let r = this(recv, "boxed toString");
            let v = r.borrow().slots[0].clone();
            let text = concat::format_value(&v, $kind, Dispatch::default())?;
            Ok(Value::Ref(Some(object::new_string(&text))))
        }
    };
}

boxed_class! {
    "java/lang/Integer", "I",
    value_of: n_integer_value_of, slot_get: expect_int,
    unbox: n_integer_int_value, Int,
    to_string: n_integer_to_string, 'I'
}
boxed_class! {
    "java/lang/Long", "J",
    value_of: n_long_value_of, slot_get: expect_long,
    unbox: n_long_long_value, Long,
    to_string: n_long_to_string, 'J'
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
    to_string: n_byte_to_string, 'I'
}
boxed_class! {
    "java/lang/Short", "S",
    value_of: n_short_value_of, slot_get: expect_int,
    unbox: n_short_short_value, Int,
    to_string: n_short_to_string, 'I'
}
boxed_class! {
    "java/lang/Boolean", "Z",
    value_of: n_boolean_value_of, slot_get: expect_int,
    unbox: n_boolean_boolean_value, Int,
    to_string: n_boolean_to_string, 'Z'
}
boxed_class! {
    "java/lang/Character", "C",
    value_of: n_character_value_of, slot_get: expect_int,
    unbox: n_character_char_value, Int,
    to_string: n_character_to_string, 'C'
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

fn n_long_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &arg(args, "Long.toString").expect_long().to_string(),
    ))))
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

fn n_integer_parse_int(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    let s = match arg(args, "Integer.parseInt") {
        Value::Ref(Some(o)) if is_string(o) => Some(object::string_value(o).to_string()),
        Value::Ref(None) => None,
        _ => return Err(internal_error("Integer.parseInt needs a String")),
    };
    let v = java_parse_i64(s.as_deref(), i32::MIN as i64, i32::MAX as i64)?;
    Ok(Value::Int(v as i32))
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

// ---------------------------------------------------------------- Table

/// The natives table: `(class, method, descriptor, implementation)`.
/// Classes use internal form (`a/b/C`). Keep sorted-ish by class for review.
pub static NATIVES: &[(&str, &str, &str, NativeFn)] = &[
    // java/lang/Object
    ("java/lang/Object", "<init>", "()V", n_object_init),
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
    ("java/lang/String", "length", "()I", n_string_length),
    ("java/lang/String", "isEmpty", "()Z", n_string_is_empty),
    ("java/lang/String", "charAt", "(I)C", n_string_char_at),
    (
        "java/lang/String",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_string_equals,
    ),
    ("java/lang/String", "hashCode", "()I", n_string_hash_code),
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
    (
        "java/lang/StringBuilder",
        "toString",
        "()Ljava/lang/String;",
        n_sb_to_string,
    ),
    ("java/lang/StringBuilder", "length", "()I", n_sb_length),
    // java/lang/Throwable
    ("java/lang/Throwable", "<init>", "()V", n_throwable_init),
    (
        "java/lang/Throwable",
        "<init>",
        "(Ljava/lang/String;)V",
        n_throwable_init_s,
    ),
    (
        "java/lang/Throwable",
        "getMessage",
        "()Ljava/lang/String;",
        n_throwable_get_message,
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
    (
        "java/lang/Long",
        "parseLong",
        "(Ljava/lang/String;)J",
        n_long_parse_long,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn no_ctx() -> Dispatch {
        Dispatch::default()
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
        assert_eq!(&*r.borrow().jclass, "java/lang/Integer");
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
