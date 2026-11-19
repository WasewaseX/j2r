use super::*;

const RANDOM_MULTIPLIER: i64 = 0x5DEECE66D;
const RANDOM_ADDEND: i64 = 0xB;
const RANDOM_MASK: i64 = (1i64 << 48) - 1;
const RANDOM_DOUBLE_UNIT: f64 = f64::from_bits(0x3CA0000000000000); // 2^-53
/// The JDK scramble: `this.seed = (seed ^ multiplier) & mask`.
fn random_scramble(s: i64) -> i64 {
    (s ^ RANDOM_MULTIPLIER) & RANDOM_MASK
}
fn random_seed_of(r: &JRef) -> i64 {
    crate::heap::with_obj(*r, |o| o.slots[0].clone().expect_long())
}
/// `next(bits)` — the LCG core: seed = (seed*multiplier + addend) & mask;
/// return (int)(seed >>> (48 - bits)).
fn random_next(r: &JRef, bits: i32) -> i32 {
    let seed = random_seed_of(r);
    let next = seed
        .wrapping_mul(RANDOM_MULTIPLIER)
        .wrapping_add(RANDOM_ADDEND)
        & RANDOM_MASK;
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Long(next));
    ((next as u64) >> (48 - bits) as u64) as i32
}
/// The shared seed-uniquifier state (JDK: an AtomicLong stepped by the
/// odd multiplier each no-arg construction).
static RANDOM_UNIQUIFIER: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(8682522807148012);
/// The JDK's CAS loop (`seedUniquifier()`): multiply by the odd constant
/// until the compare-and-set wins.
fn random_next_uniquifier() -> i64 {
    use std::sync::atomic::Ordering;
    loop {
        let current = RANDOM_UNIQUIFIER.load(Ordering::SeqCst);
        let next = current.wrapping_mul(1181783497276652981);
        if RANDOM_UNIQUIFIER
            .compare_exchange_weak(current, next, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return current;
        }
    }
}
/// `Random()` — seedUniquifier ^ System.nanoTime (unspecified stream).
pub(crate) fn n_random_init(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.<init>");
    let uniq = random_next_uniquifier();
    let nano = n_system_nano_time(None, &[], _ctx)?.expect_long();
    let seed = random_scramble(uniq ^ nano);
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = Value::Long(seed);
        o.slots[1] = Value::Int(0);
        o.slots[2] = Value::Double(0.0);
    });
    Ok(Value::Void)
}
/// `Random(long seed)` — the deterministic JDK constructor.
pub(crate) fn n_random_init_j(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.<init>");
    let seed = random_scramble(arg(args, "Random.<init>").expect_long());
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = Value::Long(seed);
        o.slots[1] = Value::Int(0);
        o.slots[2] = Value::Double(0.0);
    });
    Ok(Value::Void)
}
/// `setSeed(long)` — the same scramble + gaussian-cache reset.
pub(crate) fn n_random_set_seed(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.setSeed");
    let seed = random_scramble(arg(args, "Random.setSeed").expect_long());
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = Value::Long(seed);
        o.slots[1] = Value::Int(0);
        o.slots[2] = Value::Double(0.0);
    });
    Ok(Value::Void)
}
/// `nextInt()` — next(32).
pub(crate) fn n_random_next_int(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextInt");
    Ok(Value::Int(random_next(r, 32)))
}
/// `nextInt(bound)` — the JDK's power-of-two / rejection algorithm with
/// the exact IllegalArgumentException("bound must be positive").
pub(crate) fn n_random_next_int_bound(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextInt");
    let bound = arg(args, "Random.nextInt").expect_int();
    if bound <= 0 {
        return Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some("bound must be positive"),
        ));
    }
    let m = bound - 1;
    if (bound & m) == 0 {
        // power of two: (int)((bound * (long)next(31)) >> 31)
        let r31 = random_next(r, 31) as i64;
        return Ok(Value::Int(((bound as i64 * r31) >> 31) as i32));
    }
    // rejection loop: JDK `for (int u = r; u - (r = u % bound) + m < 0; u = next(31));`
    let mut u = random_next(r, 31);
    let mut res = u % bound;
    while u.wrapping_sub(res).wrapping_add(m) < 0 {
        u = random_next(r, 31);
        res = u % bound;
    }
    Ok(Value::Int(res))
}
/// `nextLong()` — ((long)next(32) << 32) + next(32).
pub(crate) fn n_random_next_long(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextLong");
    let hi = random_next(r, 32) as i64;
    let lo = random_next(r, 32) as i64;
    Ok(Value::Long((hi << 32).wrapping_add(lo)))
}
/// `nextBoolean()` — next(1) != 0.
pub(crate) fn n_random_next_boolean(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextBoolean");
    Ok(Value::Int(if random_next(r, 1) != 0 { 1 } else { 0 }))
}
/// `nextFloat()` — next(24) / (float)(1 << 24).
pub(crate) fn n_random_next_float(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextFloat");
    let v = random_next(r, 24) as f32;
    Ok(Value::Float(v / 16777216.0f32))
}
/// `nextDouble()` — ((long)next(26) << 27 + next(27)) * 2^-53.
pub(crate) fn n_random_next_double(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextDouble");
    let hi = random_next(r, 26) as i64;
    let lo = random_next(r, 27) as i64;
    Ok(Value::Double(
        ((hi << 27).wrapping_add(lo)) as f64 * RANDOM_DOUBLE_UNIT,
    ))
}
/// `nextGaussian()` — the JDK's polar method with the one-slot cache
/// (slots 1-2): strictly follow Random.java's loop and branch order.
/// The internal `Math.log(s)` rides Rust's `f64::ln`, which on this
/// platform is the same correctly-rounded system libm the HotSpot
/// Math.log intrinsic calls — the Externals fixture proves the produced
/// stream byte-identical vs Temurin 21.0.13.
pub(crate) fn n_random_next_gaussian(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextGaussian");
    let cached = crate::heap::with_obj(*r, |o| o.slots[1].clone().expect_int());
    if cached != 0 {
        crate::heap::with_obj_mut(*r, |o| o.slots[1] = Value::Int(0));
        return Ok(crate::heap::with_obj(*r, |o| o.slots[2].clone()));
    }
    let (v1, v2, s) = loop {
        // 2 * nextDouble() - 1, through the same generator state
        let d1 = match n_random_next_double(Some(r), &[], ctx)? {
            Value::Double(d) => d,
            _ => unreachable!("nextDouble returns a double"),
        };
        let d2 = match n_random_next_double(Some(r), &[], ctx)? {
            Value::Double(d) => d,
            _ => unreachable!("nextDouble returns a double"),
        };
        let v1 = 2.0 * d1 - 1.0;
        let v2 = 2.0 * d2 - 1.0;
        let s = v1 * v1 + v2 * v2;
        if s < 1.0 && s != 0.0 {
            break (v1, v2, s);
        }
    };
    let multiplier = (-2.0 * s.ln() / s).sqrt();
    let result = v1 * multiplier;
    let next_next = v2 * multiplier;
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[1] = Value::Int(1);
        o.slots[2] = Value::Double(next_next);
    });
    Ok(Value::Double(result))
}
/// `next(int bits)` — protected; exposed for subclasses (jbox2d does not
/// subclass Random, but the JDK method is part of the surface).
pub(crate) fn n_random_next(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.next");
    Ok(Value::Int(random_next(
        r,
        arg(args, "Random.next").expect_int(),
    )))
}
/// `nextBytes(byte[])` — the JDK's 4-bytes-per-rnd loop:
/// `for (i = 0; i < len; ) for (rnd = nextInt(), n = min(len-i, 4);
/// n-- > 0; rnd >>= 8) bytes[i++] = (byte)rnd;`
pub(crate) fn n_random_next_bytes(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextBytes");
    let arr = match arg(args, "Random.nextBytes") {
        Value::Ref(Some(a)) => *a,
        Value::Ref(None) => return Err(crate::npe()),
        other => return Err(internal_error(&format!("Random.nextBytes arg {other:?}"))),
    };
    let len = crate::array::array_length(&arr)?;
    let mut i: i32 = 0;
    while i < len {
        let mut rnd = random_next(r, 32);
        let n = (len - i).min(4);
        let mut k = n;
        while k > 0 {
            crate::array::array_store(&arr, i, Value::Int(rnd as i8 as i32), _ctx)?;
            i += 1;
            rnd >>= 8;
            k -= 1;
        }
    }
    Ok(Value::Void)
}
pub(crate) fn n_integer_to_string_static(
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
pub(crate) fn n_integer_to_string_radix(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let i = arg(args, "Integer.toString").expect_int();
    let radix = match args.get(1) {
        Some(v) => v.expect_int(),
        None => return Err(internal_error("j2r internal: Integer.toString radix")),
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
pub(crate) fn n_long_to_string_static(
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
pub(crate) fn n_long_to_string_radix(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let l = arg(args, "Long.toString").expect_long();
    let radix = match args.get(1) {
        Some(v) => v.expect_int(),
        None => return Err(internal_error("j2r internal: Long.toString radix")),
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
pub(crate) fn n_double_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &javafmt::double_to_string(arg(args, "Double.toString").expect_double()),
    ))))
}
pub(crate) fn n_float_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &javafmt::float_to_string(arg(args, "Float.toString").expect_float()),
    ))))
}
pub(crate) fn n_boolean_to_string_static(
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
