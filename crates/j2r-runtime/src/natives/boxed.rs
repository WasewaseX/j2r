use super::*;

/// (class, payload-as-i64) -> cached boxed object. Only classes with a
/// mandatory cache are inserted; Float/Double never consult this.
/// Process-global since M4 Wave 1 (T04); `OnceLock` because `HashMap::new`
/// is not const. Allocation under its own lock is heap-lock law L2
/// (cache→heap nesting, never the reverse).
static BOXED_CACHE: OnceLock<Mutex<HashMap<(&'static str, i64), JRef>>> = OnceLock::new();
pub(crate) fn boxed_cache() -> &'static Mutex<HashMap<(&'static str, i64), JRef>> {
    BOXED_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}
/// Returns the shared boxed object for `(class, v)` when `v` is inside the
/// class's mandatory identity range, allocating it on first use.
pub(crate) fn boxed_cached(class: &'static str, v: i64, lo: i64, hi: i64) -> Option<JRef> {
    if v < lo || v > hi {
        return None;
    }
    let mut cache = boxed_cache().lock().unwrap_or_else(|e| e.into_inner());
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
// Exact IEEE bit exposure (corpus gap F4 support): the HexFloat fixture
// pins hex-float rounding through floatToIntBits/doubleToLongBits — the
// only JVM-exact, formatting-independent way to compare bit patterns.
pub(crate) fn n_float_to_int_bits(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(float_to_int_bits(
        arg(args, "Float.floatToIntBits").expect_float(),
    )))
}
pub(crate) fn n_float_to_raw_int_bits(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(
        arg(args, "Float.floatToRawIntBits")
            .expect_float()
            .to_bits() as i32,
    ))
}
pub(crate) fn n_double_to_long_bits(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Long(double_to_long_bits(
        arg(args, "Double.doubleToLongBits").expect_double(),
    )))
}
// Float/Double.toHexString (corpus/HexFloatProbe wave): the JDK hex-string
// grammar lives in javafmt (double_to_hex_string / float_to_hex_string,
// Temurin-21.0.13-pinned law tests there); these natives are the bridge.
pub(crate) fn n_float_to_hex_string(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    let s = crate::javafmt::float_to_hex_string(arg(args, "Float.toHexString").expect_float());
    Ok(Value::Ref(Some(crate::object::new_string(&s))))
}
pub(crate) fn n_double_to_hex_string(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    let s = crate::javafmt::double_to_hex_string(arg(args, "Double.toHexString").expect_double());
    Ok(Value::Ref(Some(crate::object::new_string(&s))))
}
pub(crate) fn n_double_to_raw_long_bits(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Long(
        arg(args, "Double.doubleToRawLongBits")
            .expect_double()
            .to_bits() as i64,
    ))
}
// EXTERNAL-B8 (jbox2d port, 2026-11-13): the two reverse
// bit-reinterpretation statics. Raw bit moves, exactly like HotSpot's
// intrinsics: intBitsToFloat(longBitsToDouble) of any pattern — NaN
// payloads included — preserves the bits (floatToRawIntBits of the result
// round-trips), while floatToIntBits still canonicalizes the NaN on the
// way OUT, which is the distinction Float.equals is specified through.
pub(crate) fn n_int_bits_to_float(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Float(f32::from_bits(
        arg(args, "Float.intBitsToFloat").expect_int() as u32,
    )))
}
pub(crate) fn n_long_bits_to_double(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Double(f64::from_bits(
        arg(args, "Double.longBitsToDouble").expect_long() as u64,
    )))
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
        pub fn $unbox(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let r = this(recv, "unboxing");
            Ok(Value::$slot_ret(
                crate::heap::with_obj(*r, |o| o.slots[0].clone()).$slot_get(),
            ))
        }
        pub fn $to_string(
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
        pub fn $name(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
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
pub(crate) fn n_float_equals(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
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
pub(crate) fn n_double_equals(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
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
        pub fn $name(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
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
pub(crate) fn n_boolean_hash_code(
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
pub(crate) fn n_long_hash_code(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Long.hashCode");
    let v = crate::heap::with_obj(*r, |o| o.slots[0].clone()).expect_long();
    Ok(Value::Int((v ^ (v >> 32)) as i32))
}
pub(crate) fn n_float_hash_code(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Float.hashCode");
    Ok(Value::Int(float_to_int_bits(
        crate::heap::with_obj(*r, |o| o.slots[0].clone()).expect_float(),
    )))
}
pub(crate) fn n_double_hash_code(
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

// EXTERNAL-B6 (jbox2d port, 2026-11-13): the boxed classes are
// instantiable — `new Integer(int)` and friends now resolve. JDK-exact
// law: a constructor FRESH-allocates. The JLS 5.1.7 identity cache
// belongs to boxing conversion (valueOf/autoboxing) only; on Temurin
// 21.0.13 `new Integer(127) == new Integer(127)` is FALSE, so the
// <init> rows deliberately do NOT route through the valueOf caches.
// (The owner's bug.md said the rows "alias valueOf so the identity
// cache applies" — the pinned JVM vector says otherwise; the fixture
// carries the real output and the correction is recorded in the wave
// evidence.) The (String) forms delegate to the JDK-exact parse
// helpers; Short/Byte range-check with the JDK's exact
// "Value out of range" message quoting the input string.
fn boxed_init_slot(recv: Option<&JRef>, v: Value) -> Result<Value, Throw> {
    let r = this(recv, "boxed <init>");
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = v);
    Ok(Value::Void)
}

macro_rules! boxed_init_prim {
    ($name:ident, $who:expr) => {
        pub fn $name(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let v = arg(args, $who).clone();
            boxed_init_slot(recv, v)
        }
    };
}

boxed_init_prim!(n_integer_init_i, "Integer.<init>");
boxed_init_prim!(n_long_init_j, "Long.<init>");
boxed_init_prim!(n_double_init_d, "Double.<init>");
boxed_init_prim!(n_float_init_f, "Float.<init>");
boxed_init_prim!(n_boolean_init_z, "Boolean.<init>");
boxed_init_prim!(n_character_init_c, "Character.<init>");
boxed_init_prim!(n_short_init_s_prim, "Short.<init>");
boxed_init_prim!(n_byte_init_b_prim, "Byte.<init>");
/// `Integer(String)` — `this(parseInt(s, 10))`, fresh box.
pub(crate) fn n_integer_init_s(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = n_integer_parse_int(None, args, ctx)?;
    boxed_init_slot(recv, v)
}
/// `Long(String)` — `this(parseLong(s, 10))`, fresh box.
pub(crate) fn n_long_init_s(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = n_long_parse_long(None, args, ctx)?;
    boxed_init_slot(recv, v)
}
/// `Float(String)` — `this(parseFloat(s))`, fresh box.
pub(crate) fn n_float_init_s(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = n_float_parse_float(None, args, ctx)?;
    boxed_init_slot(recv, v)
}
/// `Double(String)` — `this(parseDouble(s))`, fresh box.
pub(crate) fn n_double_init_s(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = n_double_parse_double(None, args, ctx)?;
    boxed_init_slot(recv, v)
}
/// `Boolean(String)` — `this(parseBoolean(s))`: exactly
/// `"true".equalsIgnoreCase(s)`; null yields false like the JDK.
pub(crate) fn n_boolean_init_s(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = match arg(args, "Boolean.<init>") {
        Value::Ref(Some(r)) if is_string(r) => {
            let s = object::string_value(r);
            // "true" is pure ASCII, so ASCII case-folding reproduces the
            // JDK's Character-based equalsIgnoreCase exactly here.
            Value::Int(if s.eq_ignore_ascii_case("true") { 1 } else { 0 })
        }
        Value::Ref(None) => Value::Int(0),
        other => return Err(internal_error(&format!("Boolean.<init> arg {other:?}"))),
    };
    boxed_init_slot(recv, v)
}
/// Shared shape of `Short(String)` / `Byte(String)`: parse as int, then
/// the JDK's exact range message (`"Value:\"" + s + "\" Radix:" + radix`
/// with radix 10).
fn boxed_init_narrow(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
    lo: i64,
    hi: i64,
    who: &str,
) -> Result<Value, Throw> {
    let v = n_integer_parse_int(None, args, ctx)?.expect_int() as i64;
    if v < lo || v > hi {
        let s = match arg(args, who) {
            Value::Ref(Some(r)) if is_string(r) => object::string_value(r).to_string(),
            _ => String::new(),
        };
        return Err(throw_new(
            "java/lang/NumberFormatException",
            Some(&format!("Value out of range. Value:\"{s}\" Radix:10")),
        ));
    }
    boxed_init_slot(recv, Value::Int(v as i32))
}
pub(crate) fn n_short_init_s(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    boxed_init_narrow(
        recv,
        args,
        ctx,
        i16::MIN as i64,
        i16::MAX as i64,
        "Short.<init>",
    )
}
pub(crate) fn n_byte_init_s(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    boxed_init_narrow(
        recv,
        args,
        ctx,
        i8::MIN as i64,
        i8::MAX as i64,
        "Byte.<init>",
    )
}

// EXTERNAL-B10 (jbox2d port, 2026-11-13): java.util.Random as a real
// builtin object — the JDK's 48-bit LCG reproduced exactly from OpenJDK
// Random.java (seed in slot 0 as a Long; haveNextNextGaussian in slot 1
// as an Int flag; nextNextGaussian in slot 2 as a Double). jbox2d's
// MathUtils uses it for contact-jitter, so the produced stream must be
// BIT-IDENTICAL to the JDK's — `new Random(42)` is fully deterministic
// and every one of these natives is differential-fixture-pinned below.
// The no-arg constructor is seeded from the uniquifier XOR nanoTime like
// the JDK: deliberately unspecified, never printed in a fixture.
