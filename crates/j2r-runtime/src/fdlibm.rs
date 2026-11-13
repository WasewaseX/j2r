//! EXTERNAL-B9 (jbox2d port, 2026-11-13): the transcendental surface —
//! Math/StrictMath sin, cos, tan, atan, atan2, hypot.
//!
//! A faithful transliteration of the fdlibm kernels **as musl carries
//! them** (`src/math/{__rem_pio2,__sin,__cos,__tan,atan,atan2}.c`, fetched
//! verbatim from git.musl-libc.org on 2026-11-13) plus `hypot` from the
//! JDK's own Java translation (`java.lang.FdLibm.Hypot`, OpenJDK 21u — the
//! exact code `StrictMath.hypot` specifies). Arithmetic is plain `f64`
//! with no FMA contraction: Rust never contracts floating-point
//! expressions by default, which matches the C compiled without
//! `-ffp-contract=fast` — do NOT rewrite any expression below into
//! `mul_add` or similar; the bit-exactness depends on the exact
//! operation order and rounding points.
//!
//! Measured (owner, 2026-11-12, 200k LCG doubles — docs/external-port/
//! jbox2d_bug.md B3): these kernels are bit-identical to the C fdlibm
//! reference; fdlibm-C vs the Temurin JVM differs on 4 sin / 9 cos
//! inputs per 200k because HotSpot's Math intrinsics are NOT pure
//! fdlibm on every input (O1 — open, needs HotSpot's actual reduction
//! tables). tan/atan/atan2/hypot: zero observed mismatches.
//!
//! Loud residual (O2): the Payne–Hanek `__rem_pio2_large` kernel is NOT
//! implemented — reduction of |x| >= 2^20·(π/2) (~3.3e6) panics with a
//! named message rather than returning a plausible wrong value. inf/NaN
//! still return NaN through the wrappers before the reduction is
//! attempted.

// The decimal constants below are the C sources' own literals, carried
// verbatim as the transliteration contract (fdlibm's stated law: "the
// decimal values may be used, provided that the compiler will convert
// from decimal to binary accurately" — Rust's parser is correctly
// rounded, so they land on the same f64 bits). Reformatting them to
// shorter/`consts::` forms would preserve the bits but destroy the
// source-tracing property, so both clippy lints are off for this file.
// `x - x` (the C idiom for the quiet-NaN result of inf/NaN inputs) trips
// clippy::eq_op for the same verbatim reason.
#![allow(clippy::approx_constant, clippy::excessive_precision, clippy::eq_op)]

use crate::heap::JRef;
use crate::Dispatch;
use crate::Throw;
use crate::Value;

// ---------------------------------------------------------------- constants

/// 2^52 * 1.5 — the rint trick constant (`1.5/EPS` in the C source).
const TOINT: f64 = 6755399441055744.0;
/// π/4, `0x1.921fb54442d18p-1` (shortest round-trip decimal).
const PIO4: f64 = 0.7853981633974483;
/// 2^-120 (the `0x1p-120f` atan huge-|x| offset, exact as a decimal).
const ATAN_TINY: f64 = 7.52316384526264e-37;

/// 2^600 / 2^-600 — hypot's scaling factors.
const TWO_600: f64 = 4.149515568880993e180;
const TWO_MINUS_600: f64 = 2.409919865102884e-181;
/// 2^1022 — hypot's subnormal-b scaling factor.
const TWO_1022: f64 = 4.49423283715579e307;
/// `0x1.00000ffffffffp500` — the largest double with exponent 500
/// (a > this ⇒ scale down). Pinned through bits, not a decimal.
const HYPOT_BIG_LIMIT: f64 = f64::from_bits(0x5f300000ffffffff);
/// 2^-500 — hypot's small-b threshold (b below this scales up).
const TWO_MINUS_500: f64 = f64::from_bits(0x20b0000000000000);

// ---------------------------------------------------------------- rem_pio2

/* origin: FreeBSD /usr/src/lib/msun/src/e_rem_pio2.c (musl __rem_pio2.c)
 * invpio2:  53 bits of 2/pi
 * pio2_1:   first  33 bit of pi/2
 * pio2_1t:  pi/2 - pio2_1
 * pio2_2:   second 33 bit of pi/2
 * pio2_2t:  pi/2 - (pio2_1+pio2_2)
 * pio2_3:   third  33 bit of pi/2
 * pio2_3t:  pi/2 - (pio2_1+pio2_2+pio2_3)
 */
const INVPIO2: f64 = 6.36619772367581382433e-01;
const PIO2_1: f64 = 1.57079632673412561417e+00;
const PIO2_1T: f64 = 6.07710050650619224932e-11;
const PIO2_2: f64 = 6.07710050630396597660e-11;
const PIO2_2T: f64 = 2.02226624879595063154e-21;
const PIO2_3: f64 = 2.02226624871116645580e-21;
const PIO2_3T: f64 = 8.47842766036889956997e-32;

#[inline]
fn high_word(x: f64) -> u32 {
    (x.to_bits() >> 32) as u32
}

#[inline]
fn with_low_word(x: f64, low: u32) -> f64 {
    f64::from_bits((x.to_bits() & 0xFFFF_FFFF_0000_0000) | (low as u64))
}

/// The medium path shared by every `goto medium` site in the C source.
/// Returns (n, y0, y1): x = n·π/2 + y0 + y1 with |y0| < π/4 + 3ε.
fn rem_pio2_medium(x: f64) -> (i32, f64, f64) {
    /* rint(x/(pi/2)) */
    let mut fnv = x * INVPIO2 + TOINT - TOINT;
    let mut n = fnv as i32;
    let mut r = x - fnv * PIO2_1;
    let mut w = fnv * PIO2_1T; /* 1st round, good to 85 bits */
    /* Matters with directed rounding. */
    if r - w < -PIO4 {
        n -= 1;
        fnv -= 1.0;
        r = x - fnv * PIO2_1;
        w = fnv * PIO2_1T;
    } else if r - w > PIO4 {
        n += 1;
        fnv += 1.0;
        r = x - fnv * PIO2_1;
        w = fnv * PIO2_1T;
    }
    let mut y0 = r - w;
    let mut ey = ((y0.to_bits() >> 52) & 0x7ff) as i64;
    let ex = ((high_word(x) & 0x7fff_ffff) >> 20) as i64; /* |x|'s exponent */
    if ex - ey > 16 {
        /* 2nd round, good to 118 bits */
        let t = r;
        w = fnv * PIO2_2;
        r = t - w;
        w = fnv * PIO2_2T - ((t - r) - w);
        y0 = r - w;
        ey = ((y0.to_bits() >> 52) & 0x7ff) as i64;
        if ex - ey > 49 {
            /* 3rd round, good to 151 bits, covers all cases */
            let t = r;
            w = fnv * PIO2_3;
            r = t - w;
            w = fnv * PIO2_3T - ((t - r) - w);
            y0 = r - w;
        }
    }
    let y1 = (r - y0) - w;
    (n, y0, y1)
}

/// `__rem_pio2(x, y)` — the remainder of x rem π/2 in (y0, y1), returned
/// as (n, y0, y1). CALLERS MUST HANDLE |x| ~<= π/4 themselves (no
/// reduction needed there — the C source's stated caller contract).
/// Large |x| (>= 2^20·π/2) panics: Payne–Hanek is not implemented (O2).
fn rem_pio2(x: f64) -> (i32, f64, f64) {
    let bits = x.to_bits();
    let sign = bits >> 63;
    let ix = high_word(x) & 0x7fff_ffff;

    if ix <= 0x400f_6a7a {
        /* |x| ~<= 5pi/4 */
        if (ix & 0xf_ffff) == 0x921fb {
            /* |x| ~= pi/2 or 2pi/2 -- cancellation -- use medium case */
            return rem_pio2_medium(x);
        }
        if ix <= 0x4002_d97c {
            /* |x| ~<= 3pi/4 */
            if sign == 0 {
                let z = x - PIO2_1; /* one round good to 85 bits */
                let y0 = z - PIO2_1T;
                let y1 = (z - y0) - PIO2_1T;
                return (1, y0, y1);
            } else {
                let z = x + PIO2_1;
                let y0 = z + PIO2_1T;
                let y1 = (z - y0) + PIO2_1T;
                return (-1, y0, y1);
            }
        } else {
            if sign == 0 {
                let z = x - 2.0 * PIO2_1;
                let y0 = z - 2.0 * PIO2_1T;
                let y1 = (z - y0) - 2.0 * PIO2_1T;
                return (2, y0, y1);
            } else {
                let z = x + 2.0 * PIO2_1;
                let y0 = z + 2.0 * PIO2_1T;
                let y1 = (z - y0) + 2.0 * PIO2_1T;
                return (-2, y0, y1);
            }
        }
    }
    if ix <= 0x401c_463b {
        /* |x| ~<= 9pi/4 */
        if ix <= 0x4015_fdbc {
            /* |x| ~<= 7pi/4 */
            if ix == 0x4012_d97c {
                /* |x| ~= 3pi/2 */
                return rem_pio2_medium(x);
            }
            if sign == 0 {
                let z = x - 3.0 * PIO2_1;
                let y0 = z - 3.0 * PIO2_1T;
                let y1 = (z - y0) - 3.0 * PIO2_1T;
                return (3, y0, y1);
            } else {
                let z = x + 3.0 * PIO2_1;
                let y0 = z + 3.0 * PIO2_1T;
                let y1 = (z - y0) + 3.0 * PIO2_1T;
                return (-3, y0, y1);
            }
        } else {
            if ix == 0x4019_21fb {
                /* |x| ~= 4pi/2 */
                return rem_pio2_medium(x);
            }
            if sign == 0 {
                let z = x - 4.0 * PIO2_1;
                let y0 = z - 4.0 * PIO2_1T;
                let y1 = (z - y0) - 4.0 * PIO2_1T;
                return (4, y0, y1);
            } else {
                let z = x + 4.0 * PIO2_1;
                let y0 = z + 4.0 * PIO2_1T;
                let y1 = (z - y0) + 4.0 * PIO2_1T;
                return (-4, y0, y1);
            }
        }
    }
    if ix < 0x4139_21fb {
        /* |x| ~< 2^20*(pi/2), medium size */
        return rem_pio2_medium(x);
    }
    /* all other (large) arguments */
    if ix >= 0x7ff0_0000 {
        /* x is inf or NaN */
        return (0, x - x, x - x);
    }
    panic!(
        "j2r fdlibm: __rem_pio2_large (Payne-Hanek) not implemented: |x| = {x:e} \
         exceeds the 2^20*(pi/2) medium-reduction range (residual O2, \
         docs/external-port/jbox2d_bug.md B3/O2)"
    );
}

// ---------------------------------------------------------------- kernels

/* origin: FreeBSD /usr/src/lib/msun/src/k_sin.c (musl __sin.c) */

const S1: f64 = -1.66666666666666324348e-01;
const S2: f64 = 8.33333333332248946124e-03;
const S3: f64 = -1.98412698298579493134e-04;
const S4: f64 = 2.75573137070700676789e-06;
const S5: f64 = -2.50507602534068634195e-08;
const S6: f64 = 1.58969099521155010221e-10;

/// `__sin(x, y, iy)` — kernel sin on ~[-π/4, π/4]; `y` is the tail of x,
/// `iy` says whether y is nonzero.
fn kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
    let z = x * x;
    let w = z * z;
    let r = S2 + z * (S3 + z * S4) + z * w * (S5 + z * S6);
    let v = z * x;
    if iy == 0 {
        x + v * (S1 + z * r)
    } else {
        x - ((z * (0.5 * y - v * r) - y) - v * S1)
    }
}

/* origin: FreeBSD /usr/src/lib/msun/src/k_cos.c (musl __cos.c) */

const C1: f64 = 4.16666666666666019037e-02;
const C2: f64 = -1.38888888888741095749e-03;
const C3: f64 = 2.48015872894767294178e-05;
const C4: f64 = -2.75573143513906633035e-07;
const C5: f64 = 2.08757232129817482790e-09;
const C6: f64 = -1.13596475577881948265e-11;

/// `__cos(x, y)` — kernel cos on [-π/4, π/4]; `y` is the tail of x.
fn kernel_cos(x: f64, y: f64) -> f64 {
    let z = x * x;
    let w = z * z;
    let r = z * (C1 + z * (C2 + z * C3)) + w * w * (C4 + z * (C5 + z * C6));
    let hz = 0.5 * z;
    let w = 1.0 - hz;
    w + (((1.0 - w) - hz) + (z * r - x * y))
}

/* origin: FreeBSD /usr/src/lib/msun/src/k_tan.c (musl __tan.c) */

const T: [f64; 13] = [
    3.33333333333334091986e-01,  /* T[0]  3FD55555, 55555563 */
    1.33333333333201242699e-01,  /* T[1]  3FC11111, 1110FE7A */
    5.39682539762260521377e-02,  /* T[2]  3FABA1BA, 1BB341FE */
    2.18694882948595424599e-02,  /* T[3]  3F9664F4, 8406D637 */
    8.86323982359930005737e-03,  /* T[4]  3F8226E3, E96E8493 */
    3.59207910759131235356e-03,  /* T[5]  3F6D6D22, C9560328 */
    1.45620945432529025516e-03,  /* T[6]  3F57DBC8, FEE08315 */
    5.88041240820264096874e-04,  /* T[7]  3F4344D8, F2F26501 */
    2.46463134818469906812e-04,  /* T[8]  3F3026F7, 1A8D1068 */
    7.81794442939557092300e-05,  /* T[9]  3F147E88, A03792A6 */
    7.14072491382608190305e-05,  /* T[10] 3F12B80F, 32F0A7E9 */
    -1.85586374855275456654e-05, /* T[11] BEF375CB, DB605373 */
    2.59073051863633712884e-05,  /* T[12] 3EFB2A70, 74BF7AD4 */
];
const PIO4_TAN: f64 = 0.7853981633974483; /* 3FE921FB, 54442D18 */
const PIO4LO_TAN: f64 = 3.06161699786838301793e-17; /* 3C81A626, 33145C07 */

/// `__tan(x, y, odd)` — kernel tan on ~[-π/4, π/4]; returns tan(x) when
/// `odd` is 0 and -1/tan(x) when `odd` is 1.
fn kernel_tan(x: f64, y: f64, odd: i32) -> f64 {
    let mut x = x;
    let mut y = y;
    let hx = high_word(x);
    let big = (hx & 0x7fff_ffff) >= 0x3FE5_9428; /* |x| >= 0.6744 */
    let negative = hx >> 31 != 0;
    if big {
        if negative {
            x = -x;
            y = -y;
        }
        x = (PIO4_TAN - x) + (PIO4LO_TAN - y);
        y = 0.0;
    }
    let z = x * x;
    let w = z * z;
    /*
     * Break x^5*(T[1]+x^2*T[2]+...) into
     * x^5(T[1]+x^4*T[3]+...+x^20*T[11]) +
     * x^5(x^2*(T[2]+x^4*T[4]+...+x^22*[T12]))
     */
    let mut r = T[1] + w * (T[3] + w * (T[5] + w * (T[7] + w * (T[9] + w * T[11]))));
    let v = z * (T[2] + w * (T[4] + w * (T[6] + w * (T[8] + w * (T[10] + w * T[12])))));
    let s = z * x;
    r = y + z * (s * (r + v) + y) + s * T[0];
    let w = x + r;
    if big {
        let s2 = 1.0 - 2.0 * odd as f64;
        let v = s2 - 2.0 * (x + (r - w * w / (w + s2)));
        return if negative { -v } else { v };
    }
    if odd == 0 {
        return w;
    }
    /* -1.0/(x+r) has up to 2ulp error, so compute it accurately */
    let mut w0 = w;
    w0 = with_low_word(w0, 0);
    let v = r - (w0 - x); /* w0+v = r+x */
    let a = -1.0 / w;
    let a0 = with_low_word(a, 0);
    a0 + a * (1.0 + a0 * w0 + a0 * v)
}

// ---------------------------------------------------------------- wrappers

/* origin: FreeBSD /usr/src/lib/msun/src/s_sin.c (musl sin.c) */

/// Math/StrictMath sin. The C source raises inexact/underflow FP flags in
/// the tiny-|x| paths (FORCE_EVAL); flags are not observable here, the
/// returned values are identical.
pub fn sin(x: f64) -> f64 {
    let ix = high_word(x) & 0x7fff_ffff;
    /* |x| ~< pi/4 */
    if ix <= 0x3fe9_21fb {
        if ix < 0x3e50_0000 {
            /* |x| < 2**-26 */
            return x;
        }
        return kernel_sin(x, 0.0, 0);
    }
    /* sin(Inf or NaN) is NaN */
    if ix >= 0x7ff0_0000 {
        return x - x;
    }
    /* argument reduction needed */
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_sin(y0, y1, 1),
        1 => kernel_cos(y0, y1),
        2 => -kernel_sin(y0, y1, 1),
        _ => -kernel_cos(y0, y1),
    }
}

/* origin: FreeBSD /usr/src/lib/msun/src/s_cos.c (musl cos.c) */

/// Math/StrictMath cos.
pub fn cos(x: f64) -> f64 {
    let ix = high_word(x) & 0x7fff_ffff;
    /* |x| ~< pi/4 */
    if ix <= 0x3fe9_21fb {
        if ix < 0x3e46_a09e {
            /* |x| < 2**-27 * sqrt(2) */
            return 1.0;
        }
        return kernel_cos(x, 0.0);
    }
    /* cos(Inf or NaN) is NaN */
    if ix >= 0x7ff0_0000 {
        return x - x;
    }
    /* argument reduction */
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_cos(y0, y1),
        1 => -kernel_sin(y0, y1, 1),
        2 => -kernel_cos(y0, y1),
        _ => kernel_sin(y0, y1, 1),
    }
}

/* origin: FreeBSD /usr/src/lib/msun/src/s_tan.c (musl tan.c) */

/// Math/StrictMath tan.
pub fn tan(x: f64) -> f64 {
    let ix = high_word(x) & 0x7fff_ffff;
    /* |x| ~< pi/4 */
    if ix <= 0x3fe9_21fb {
        if ix < 0x3e40_0000 {
            /* |x| < 2**-27 */
            return x;
        }
        return kernel_tan(x, 0.0, 0);
    }
    /* tan(Inf or NaN) is NaN */
    if ix >= 0x7ff0_0000 {
        return x - x;
    }
    /* argument reduction */
    let (n, y0, y1) = rem_pio2(x);
    kernel_tan(y0, y1, n & 1)
}

// ---------------------------------------------------------------- atan

/* origin: FreeBSD /usr/src/lib/msun/src/s_atan.c (musl atan.c) */

const ATAN_HI: [f64; 4] = [
    4.63647609000806093515e-01, /* atan(0.5)hi 0x3FDDAC67, 0x0561BB4F */
    7.85398163397448278999e-01, /* atan(1.0)hi 0x3FE921FB, 0x54442D18 */
    9.82793723247329054082e-01, /* atan(1.5)hi 0x3FEF730B, 0xD281F69B */
    1.57079632679489655800e+00, /* atan(inf)hi 0x3FF921FB, 0x54442D18 */
];
const ATAN_LO: [f64; 4] = [
    2.26987774529616870924e-17, /* atan(0.5)lo 0x3C7A2B7F, 0x222F65E2 */
    3.06161699786838301793e-17, /* atan(1.0)lo 0x3C81A626, 0x33145C07 */
    1.39033110312309984516e-17, /* atan(1.5)lo 0x3C700788, 0x7AF0CBBD */
    6.12323399573676603587e-17, /* atan(inf)lo 0x3C91A626, 0x33145C07 */
];
const AT: [f64; 11] = [
    3.33333333333329318027e-01,  /* 0x3FD55555, 0x5555550D */
    -1.99999999998764832476e-01, /* 0xBFC99999, 0x9998EBC4 */
    1.42857142725034663711e-01,  /* 0x3FC24924, 0x920083FF */
    -1.11111104054623557880e-01, /* 0xBFBC71C6, 0xFE231671 */
    9.09088713343650656196e-02,  /* 0x3FB745CD, 0xC54C206E */
    -7.69187620504482999495e-02, /* 0xBFB3B0F2, 0xAF749A6D */
    6.66107313738753120669e-02,  /* 0x3FB10D66, 0xA0D03D51 */
    -5.83357013379057348645e-02, /* 0xBFADDE2D, 0x52DEFD9A */
    4.97687799461593236017e-02,  /* 0x3FA97B4B, 0x24760DEB */
    -3.65315727442169155270e-02, /* 0xBFA2B444, 0x2C6A6C2F */
    1.62858201153657823623e-02,  /* 0x3F90AD3A, 0xE322DA11 */
];

/// Math/StrictMath atan.
pub fn atan(x: f64) -> f64 {
    let mut x = x;
    let bits = x.to_bits();
    let sign = (bits >> 63) as u32;
    let ix = high_word(x) & 0x7fff_ffff;
    let id: i32;
    if ix >= 0x4410_0000 {
        /* if |x| >= 2^66 */
        if ix > 0x7ff0_0000 {
            /* isnan (any payload, quiet or signaling) */
            return x;
        }
        let z = ATAN_HI[3] + ATAN_TINY;
        return if sign != 0 { -z } else { z };
    }
    if ix < 0x3fdc_0000 {
        /* |x| < 0.4375 */
        if ix < 0x3e40_0000 {
            /* |x| < 2^-27 */
            return x; /* (the C raises the underflow flag for subnormals) */
        }
        id = -1;
    } else {
        x = x.abs();
        if ix < 0x3ff3_0000 {
            /* |x| < 1.1875 */
            if ix < 0x3fe6_0000 {
                /*  7/16 <= |x| < 11/16 */
                id = 0;
                x = (2.0 * x - 1.0) / (2.0 + x);
            } else {
                /* 11/16 <= |x| < 19/16 */
                id = 1;
                x = (x - 1.0) / (x + 1.0);
            }
        } else {
            if ix < 0x4003_8000 {
                /* |x| < 2.4375 */
                id = 2;
                x = (x - 1.5) / (1.0 + 1.5 * x);
            } else {
                /* 2.4375 <= |x| < 2^66 */
                id = 3;
                x = -1.0 / x;
            }
        }
    }
    /* end of argument reduction */
    let z = x * x;
    let w = z * z;
    /* break sum from i=0 to 10 at[i]z**(i+1) into odd and even poly */
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    if id < 0 {
        return x - x * (s1 + s2);
    }
    let z = ATAN_HI[id as usize] - (x * (s1 + s2) - ATAN_LO[id as usize] - x);
    if sign != 0 {
        -z
    } else {
        z
    }
}

// ---------------------------------------------------------------- atan2

/* origin: FreeBSD /usr/src/lib/msun/src/e_atan2.c (musl atan2.c) */

const PI: f64 = 3.1415926535897931160e+00; /* 0x400921FB, 0x54442D18 */
const PI_LO: f64 = 1.2246467991473531772e-16; /* 0x3CA1A626, 0x33145C07 */

/// Math/StrictMath atan2.
pub fn atan2(y: f64, x: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return x + y;
    }
    let xbits = x.to_bits();
    let ybits = y.to_bits();
    let ix_raw = (xbits >> 32) as u32;
    let iy_raw = (ybits >> 32) as u32;
    let lx = xbits as u32;
    let ly = ybits as u32;
    if (ix_raw.wrapping_sub(0x3ff0_0000) | lx) == 0 {
        /* x = 1.0 */
        return atan(y);
    }
    let m = ((iy_raw >> 31) & 1) | ((ix_raw >> 30) & 2); /* 2*sign(x)+sign(y) */
    let ix = ix_raw & 0x7fff_ffff;
    let iy = iy_raw & 0x7fff_ffff;
    /* when y = 0 */
    if (iy | ly) == 0 {
        return match m {
            0 | 1 => y, /* atan(+-0,+anything)=+-0 */
            2 => PI,    /* atan(+0,-anything) = pi */
            _ => -PI,   /* atan(-0,-anything) =-pi */
        };
    }
    /* when x = 0 */
    if (ix | lx) == 0 {
        return if m & 1 != 0 { -PI / 2.0 } else { PI / 2.0 };
    }
    /* when x is INF */
    if ix == 0x7ff0_0000 {
        if iy == 0x7ff0_0000 {
            return match m {
                0 => PI / 4.0,        /* atan(+INF,+INF) */
                1 => -PI / 4.0,       /* atan(-INF,+INF) */
                2 => 3.0 * PI / 4.0,  /* atan(+INF,-INF) */
                _ => -3.0 * PI / 4.0, /* atan(-INF,-INF) */
            };
        } else {
            return match m {
                0 => 0.0,  /* atan(+...,+INF) */
                1 => -0.0, /* atan(-...,+INF) */
                2 => PI,   /* atan(+...,-INF) */
                _ => -PI,  /* atan(-...,-INF) */
            };
        }
    }
    /* |y/x| > 0x1p64 */
    if ix.wrapping_add(64 << 20) < iy || iy == 0x7ff0_0000 {
        return if m & 1 != 0 { -PI / 2.0 } else { PI / 2.0 };
    }
    /* z = atan(|y/x|) without spurious underflow */
    let z: f64 = if (m & 2) != 0 && iy.wrapping_add(64 << 20) < ix {
        /* |y/x| < 0x1p-64, x<0 */
        0.0
    } else {
        atan((y / x).abs())
    };
    match m {
        0 => z,                /* atan(+,+) */
        1 => -z,               /* atan(-,+) */
        2 => PI - (z - PI_LO), /* atan(+,-) */
        _ => (z - PI_LO) - PI, /* atan(-,-) */
    }
}

// ---------------------------------------------------------------- hypot

/* origin: OpenJDK 21u java.lang.FdLibm.Hypot (the code StrictMath.hypot
 * specifies — the Java translation of fdlibm's e_hypot.c). */

#[inline]
fn power_of_two_d(k: i32) -> f64 {
    f64::from_bits(((k + 1023) as u64) << 52)
}

/// Math/StrictMath hypot — sqrt(x²+y²) without over/underflow.
pub fn hypot(x: f64, y: f64) -> f64 {
    let mut a = x.abs();
    let mut b = y.abs();

    if !a.is_finite() || !b.is_finite() {
        if a == f64::INFINITY || b == f64::INFINITY {
            return f64::INFINITY;
        } else {
            return a + b; // Propagate NaN significand bits
        }
    }

    if b > a {
        std::mem::swap(&mut a, &mut b);
    }

    // ha/hb: high-order 32 bits as in the JDK source (signed there; both
    // are non-negative here after the abs).
    let mut ha = (a.to_bits() >> 32) as u32;
    let mut hb = (b.to_bits() >> 32) as u32;

    if ha - hb > 0x3c0_0000 {
        return a + b; // x / y > 2**60
    }

    let mut k: i32 = 0;
    if a > HYPOT_BIG_LIMIT {
        // a > ~2**500 — scale a and b by 2**-600
        ha = ha.wrapping_sub(0x2580_0000);
        hb = hb.wrapping_sub(0x2580_0000);
        a *= TWO_MINUS_600;
        b *= TWO_MINUS_600;
        k += 600;
    }
    if b < TWO_MINUS_500 {
        if b < f64::MIN_POSITIVE {
            /* subnormal b or 0 */
            if b == 0.0 {
                return a;
            }
            b *= TWO_1022;
            a *= TWO_1022;
            k -= 1022;
        } else {
            /* scale a and b by 2^600 */
            ha = ha.wrapping_add(0x2580_0000);
            hb = hb.wrapping_add(0x2580_0000);
            a *= TWO_600;
            b *= TWO_600;
            k -= 600;
        }
    }
    // medium size a and b
    let w0 = a - b;
    let w: f64 = if w0 > b {
        let t1 = f64::from_bits((ha as u64) << 32);
        let t2 = a - t1;
        (t1 * t1 - (b * (-b) - t2 * (a + t1))).sqrt()
    } else {
        let a2 = a + a;
        let y1 = f64::from_bits((hb as u64) << 32);
        let y2 = b - y1;
        let t1 = f64::from_bits(((ha + 0x0010_0000) as u64) << 32);
        let t2 = a2 - t1;
        (t1 * y1 - (w0 * (-w0) - (t1 * y2 + t2 * b))).sqrt()
    };
    if k != 0 {
        power_of_two_d(k) * w
    } else {
        w
    }
}

// ---------------------------------------------------------------- natives

// The `Math`/`StrictMath` native wrappers — one per method, shared by
// both classes (EXTERNAL-B5's "sharing the Math implementations").

pub fn n_math_sin(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(sin(args
        .first()
        .unwrap_or_else(|| panic!("j2r internal: Math.sin without argument"))
        .expect_double())))
}

pub fn n_math_cos(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(cos(args
        .first()
        .unwrap_or_else(|| panic!("j2r internal: Math.cos without argument"))
        .expect_double())))
}

pub fn n_math_tan(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(tan(args
        .first()
        .unwrap_or_else(|| panic!("j2r internal: Math.tan without argument"))
        .expect_double())))
}

pub fn n_math_atan(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(atan(
        args.first()
            .unwrap_or_else(|| panic!("j2r internal: Math.atan without argument"))
            .expect_double(),
    )))
}

pub fn n_math_atan2(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    let y = args
        .first()
        .unwrap_or_else(|| panic!("j2r internal: Math.atan2 without argument"))
        .expect_double();
    let x = args
        .get(1)
        .unwrap_or_else(|| panic!("j2r internal: Math.atan2 missing argument 1"))
        .expect_double();
    Ok(Value::Double(atan2(y, x)))
}

pub fn n_math_hypot(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    let x = args
        .first()
        .unwrap_or_else(|| panic!("j2r internal: Math.hypot without argument"))
        .expect_double();
    let y = args
        .get(1)
        .unwrap_or_else(|| panic!("j2r internal: Math.hypot missing argument 1"))
        .expect_double();
    Ok(Value::Double(hypot(x, y)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values_match_java() {
        // Java-exact values probed from Temurin 21.0.13 (see the wave
        // fixture for the full oracle): sin/cos/tan/atan/atan2/hypot on
        // ordinary and boundary inputs, printed with Double.toString.
        let cases: &[(f64, &str, &str)] = &[
            (0.0, "sin", "0.0"),
            (0.0, "cos", "1.0"),
            (0.0, "tan", "0.0"),
            (std::f64::consts::PI, "sin", "1.2246467991473532E-16"),
            (std::f64::consts::PI, "cos", "-1.0"),
            (std::f64::consts::PI, "tan", "-1.2246467991473532E-16"),
            (1.0, "sin", "0.8414709848078965"),
            (1.0, "cos", "0.5403023058681398"),
            (1.0, "tan", "1.5574077246549023"),
            (1.0, "atan", "0.7853981633974483"),
            (1.0, "hypot", "1.4142135623730951"),
            (3.0, "hypot4", "5.0"),
        ];
        for (v, f, want) in cases {
            let got = match *f {
                "sin" => crate::javafmt::double_to_string(sin(*v)),
                "cos" => crate::javafmt::double_to_string(cos(*v)),
                "tan" => crate::javafmt::double_to_string(tan(*v)),
                "atan" => crate::javafmt::double_to_string(atan(*v)),
                "hypot" => crate::javafmt::double_to_string(hypot(*v, *v)),
                "hypot4" => crate::javafmt::double_to_string(hypot(3.0, 4.0)),
                _ => unreachable!(),
            };
            assert_eq!(&got, want, "{f}({v})");
        }
        // exact bit pins through the IEEE exposure the JVM cannot beat
        assert_eq!(sin(-0.0).to_bits(), (-0.0f64).to_bits());
        assert_eq!(tan(-0.0).to_bits(), (-0.0f64).to_bits());
        assert!(sin(f64::INFINITY).is_nan());
        assert!(cos(f64::NAN).is_nan());
        assert_eq!(atan2(0.0, -1.0).to_bits(), PI.to_bits());
        assert_eq!(atan2(-0.0, -1.0).to_bits(), (-PI).to_bits());
        assert_eq!(atan2(1.0, f64::INFINITY).to_bits(), 0.0f64.to_bits());
        assert_eq!(atan2(-1.0, f64::INFINITY).to_bits(), (-0.0f64).to_bits());
        assert_eq!(hypot(f64::INFINITY, f64::NAN), f64::INFINITY);
        assert!(hypot(f64::NAN, 2.0).is_nan());
        // the medium-reduction boundary holds (O2's loud residual starts
        // strictly beyond 2^20*pi/2)
        let _ = sin(1.0e6); // reduced through the medium path, no panic
    }
}
