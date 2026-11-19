use super::*;

pub fn n_math_floor_div_i(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = i32_at(args, 0, "Math.floorDiv");
    let b = i32_at(args, 1, "Math.floorDiv");
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    let q = a.wrapping_div(b);
    let r = a.wrapping_rem(b);
    // floor semantics: adjust when signs differ and remainder != 0
    let adjusted = if (r != 0) && ((r < 0) != (b < 0)) {
        q.wrapping_sub(1)
    } else {
        q
    };
    Ok(Value::Int(adjusted))
}
pub fn n_math_floor_mod_i(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = i32_at(args, 0, "Math.floorMod");
    let b = i32_at(args, 1, "Math.floorMod");
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    let r = a.wrapping_rem(b);
    let m = if (r != 0) && ((r < 0) != (b < 0)) {
        r + b
    } else {
        r
    };
    Ok(Value::Int(m))
}
fn floor_div_jj_core(a: i64, b: i64) -> Result<Value, Throw> {
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    let q = a.wrapping_div(b);
    let r = a.wrapping_rem(b);
    let adjusted = if (r != 0) && ((r < 0) != (b < 0)) {
        q.wrapping_sub(1)
    } else {
        q
    };
    Ok(Value::Long(adjusted))
}
fn floor_mod_jj_core(a: i64, b: i64) -> Result<Value, Throw> {
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    let r = a.wrapping_rem(b);
    let m = if (r != 0) && ((r < 0) != (b < 0)) {
        r + b
    } else {
        r
    };
    Ok(Value::Long(m))
}
pub fn n_math_floor_div_j(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.floorDiv").expect_long();
    let b = arg(args, 1, "Math.floorDiv").expect_long();
    floor_div_jj_core(a, b)
}
pub fn n_math_floor_mod_j(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.floorMod").expect_long();
    let b = arg(args, 1, "Math.floorMod").expect_long();
    floor_mod_jj_core(a, b)
}
/// `Math.floorDiv(long, int)` — the JDK 9+ mixed overload. OpenJDK
/// widens the int divisor to long and delegates to the (long, long)
/// form; the result is a LONG. Pinned differentially by
/// tests/suite/MathExact.java vs Temurin 21 (mixed signs, Long.MIN_VALUE
/// numerator, zero-divisor message).
pub fn n_math_floor_div_ji(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.floorDiv").expect_long();
    let b = i64::from(i32_at(args, 1, "Math.floorDiv"));
    floor_div_jj_core(a, b)
}
/// `Math.floorMod(long, int)` — the JDK 9+ mixed overload. NOTE the
/// asymmetric JDK shape: floorDiv(long, int) returns LONG but
/// floorMod(long, int) returns INT (javap-pinned: `(JI)I`). OpenJDK
/// computes `(int) floorMod(x, (long) y)` — the long floor-mod always
/// fits an int when the divisor is an int (the result lies in [0, y) or
/// (y, 0] with y in int range), so the narrowing cast is exact. Pinned
/// by tests/suite/MathExact.java vs Temurin 21.
pub fn n_math_floor_mod_ji(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.floorMod").expect_long();
    let b = i64::from(i32_at(args, 1, "Math.floorMod"));
    match floor_mod_jj_core(a, b)? {
        Value::Long(m) => Ok(Value::Int(m as i32)),
        _ => Err(throw_new(
            "java/lang/InternalError",
            Some("floor_mod_ji_core returned a non-long"),
        )),
    }
}
/// `Math.round(double)` — OpenJDK 21's JDK-8010430 algorithm: the
/// MATHEMATICAL floor(a + 1/2), not floor(a + 0.5) evaluated in binary FP.
/// The naive form returns 1 for `0.49999999999999994` (a + 0.5 rounds up to
/// 1.0, but a is still below one half) where every JDK since Java 7 returns
/// 0, and ...498 for the mathematical integer 4503599627370497.0 (a + 0.5
/// ties up to ...498) where the (long) cast branch must win. Pinned
/// differentially by tests/suite/MathSemantics.java vs Temurin 21.
pub fn n_math_round_d(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.round").expect_double();
    const SIGNIF_BITS: u64 = 0x000f_ffff_ffff_ffff; // Double.SIGNIF_BIT_MASK
    let bits = a.to_bits();
    let biased_exp = ((bits & 0x7ff0_0000_0000_0000) >> 52) as i64;
    // OpenJDK's shift = (SIGNIFICAND_WIDTH - 2 + EXP_BIAS) - biased_exp
    // = (53 - 2 + 1023) - biased_exp. In [0, 63] exactly when
    // pow(2,-64) <= ulp(a) < 1: there the bit trick computes the exact
    // mathematical floor(a + 1/2), including every just-below-a-half FP
    // artifact. Outside it, a is NaN/±inf/a mathematical integer/tiny, and
    // Java's (long) cast applies: NaN -> 0, saturating at the Long bounds —
    // which is also Rust's `as` cast contract.
    let shift = 1074i64 - biased_exp;
    if (0..64).contains(&shift) {
        let mut r = ((bits & SIGNIF_BITS) | (SIGNIF_BITS + 1)) as i64;
        if bits >> 63 != 0 {
            r = -r;
        }
        return Ok(Value::Long(((r >> (shift as u32)) + 1) >> 1));
    }
    Ok(Value::Long(a as i64))
}
/// `Math.round(float)` — the same JDK-8010430 law at f32 precision
/// (SIGNIFICAND_WIDTH 24, EXP_BIAS 127, 32-bit result saturating at the
/// Int bounds). Pinned differentially by tests/suite/MathSemantics.java.
pub fn n_math_round_f(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.round").expect_float();
    const SIGNIF_BITS: u32 = 0x007f_ffff; // Float.SIGNIF_BIT_MASK
    let bits = a.to_bits();
    let biased_exp = ((bits & 0x7f80_0000) >> 23) as i32;
    let shift = 149i32 - biased_exp;
    if (0..32).contains(&shift) {
        let mut r = ((bits & SIGNIF_BITS) | (SIGNIF_BITS + 1)) as i32;
        if bits >> 31 != 0 {
            r = -r;
        }
        return Ok(Value::Int(((r >> (shift as u32)) + 1) >> 1));
    }
    Ok(Value::Int(a as i32))
}
pub fn n_math_sqrt(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(
        arg(args, 0, "Math.sqrt").expect_double().sqrt(),
    ))
}
pub fn n_math_cbrt(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(
        arg(args, 0, "Math.cbrt").expect_double().cbrt(),
    ))
}
pub fn n_math_pow(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.pow").expect_double();
    let b = arg(args, 1, "Math.pow").expect_double();
    Ok(Value::Double(a.powf(b)))
}
/// Shared state of `Math.random()` — JDK semantics: `new
/// Random().nextDouble()` on one shared generator. That generator is
/// java.util.Random's 48-bit LCG, lazily seeded from a uniquifier XORed
/// with `System.nanoTime()` (Random.java's `Random()` no-arg path), one
/// CAS per `next(bits)` call so threads share the stream exactly like the
/// JDK's holder. The produced values are deliberately unpredictable and
/// unspecified — no differential fixture may print them; the native exists
/// so real programs that *reference* `Math.random` compile (JAMA wave 3:
/// `Matrix.random`, uncalled by the corpus driver).
static MATH_RANDOM_SEED: AtomicI64 = AtomicI64::new(0);
static MATH_RANDOM_SEEDED: AtomicBool = AtomicBool::new(false);
const LCG_MULTIPLIER: i64 = 0x5DEECE66D;
const LCG_ADDEND: i64 = 0xB;
fn math_random_next(bits: u32) -> i64 {
    const MASK: i64 = (1 << 48) - 1;
    loop {
        let old = MATH_RANDOM_SEED.load(Ordering::Acquire);
        let next = (old.wrapping_mul(LCG_MULTIPLIER).wrapping_add(LCG_ADDEND)) & MASK;
        if MATH_RANDOM_SEED
            .compare_exchange_weak(old, next, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return next >> (48 - bits);
        }
    }
}
pub fn n_math_random(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    if !MATH_RANDOM_SEEDED.load(Ordering::Acquire) {
        let uniqu = MATH_RANDOM_UNIQUIFIER
            .try_update(Ordering::AcqRel, Ordering::Acquire, |u: i64| {
                Some(u.wrapping_mul(1181783497276652981))
            })
            .unwrap_or(8682522807148012199);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        MATH_RANDOM_SEED.store(uniqu ^ now, Ordering::Release);
        MATH_RANDOM_SEEDED.store(true, Ordering::Release);
    }
    // Random.nextDouble(): ((long)(next(26)) << 27) + next(27), scaled by
    // 0x1.0p-53. (1 << 53) is a power of two, so the divisor is exact.
    let hi = math_random_next(26);
    let lo = math_random_next(27);
    let unit = 1.0f64 / (1u64 << 53) as f64;
    Ok(Value::Double(((hi << 27) + lo) as f64 * unit))
}
pub fn n_math_floor(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(
        arg(args, 0, "Math.floor").expect_double().floor(),
    ))
}
pub fn n_math_ceil(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(
        arg(args, 0, "Math.ceil").expect_double().ceil(),
    ))
}
pub fn n_math_signum_d(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.signum").expect_double();
    let out = if a.is_nan() {
        f64::NAN
    } else if a == 0.0 {
        a // preserves the sign of ±0.0
    } else if a > 0.0 {
        1.0
    } else {
        -1.0
    };
    Ok(Value::Double(out))
}
pub fn n_math_signum_f(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.signum").expect_float();
    let out = if a.is_nan() {
        f32::NAN
    } else if a == 0.0 {
        a
    } else if a > 0.0 {
        1.0
    } else {
        -1.0
    };
    Ok(Value::Float(out))
}
pub fn n_math_to_int_exact(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, 0, "Math.toIntExact").expect_long();
    let t = v as i32;
    if t as i64 != v {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("integer overflow"),
        ));
    }
    Ok(Value::Int(t))
}

// (The exact-arithmetic functions are written out explicitly below —
// generated code must stay reviewable, no macro gymnastics.)
pub fn n_math_add_exact_i(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = i32_at(args, 0, "Math.addExact");
    let b = i32_at(args, 1, "Math.addExact");
    match a.checked_add(b) {
        Some(v) => Ok(Value::Int(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("integer overflow"),
        )),
    }
}
pub fn n_math_subtract_exact_i(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = i32_at(args, 0, "Math.subtractExact");
    let b = i32_at(args, 1, "Math.subtractExact");
    match a.checked_sub(b) {
        Some(v) => Ok(Value::Int(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("integer overflow"),
        )),
    }
}
pub fn n_math_multiply_exact_i(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = i32_at(args, 0, "Math.multiplyExact");
    let b = i32_at(args, 1, "Math.multiplyExact");
    match a.checked_mul(b) {
        Some(v) => Ok(Value::Int(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("integer overflow"),
        )),
    }
}
pub fn n_math_add_exact_j(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.addExact").expect_long();
    let b = arg(args, 1, "Math.addExact").expect_long();
    match a.checked_add(b) {
        Some(v) => Ok(Value::Long(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("long overflow"),
        )),
    }
}
pub fn n_math_subtract_exact_j(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.subtractExact").expect_long();
    let b = arg(args, 1, "Math.subtractExact").expect_long();
    match a.checked_sub(b) {
        Some(v) => Ok(Value::Long(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("long overflow"),
        )),
    }
}
pub fn n_math_multiply_exact_j(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.multiplyExact").expect_long();
    let b = arg(args, 1, "Math.multiplyExact").expect_long();
    match a.checked_mul(b) {
        Some(v) => Ok(Value::Long(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("long overflow"),
        )),
    }
}

// ---------------------------------------------------------------- Integer / Long

// Java also overloads addExact/subtractExact/multiplyExact as (long, int):
// javac emits those descriptors for mixed-width call sites.
pub fn n_math_add_exact_ji(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.addExact").expect_long();
    let b = i32_at(args, 1, "Math.addExact") as i64;
    match a.checked_add(b) {
        Some(v) => Ok(Value::Long(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("long overflow"),
        )),
    }
}
pub fn n_math_subtract_exact_ji(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.subtractExact").expect_long();
    let b = i32_at(args, 1, "Math.subtractExact") as i64;
    match a.checked_sub(b) {
        Some(v) => Ok(Value::Long(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("long overflow"),
        )),
    }
}
pub fn n_math_multiply_exact_ji(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.multiplyExact").expect_long();
    let b = i32_at(args, 1, "Math.multiplyExact") as i64;
    match a.checked_mul(b) {
        Some(v) => Ok(Value::Long(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("long overflow"),
        )),
    }
}
