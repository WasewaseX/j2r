//! Hexadecimal floating-point parsing — the `0x1.8p1` forms accepted by
//! `Float.parseFloat` / `Double.parseDouble` (JLS 3.10.2 literal grammar as
//! `Double.valueOf` accepts it), the last corpus gap **F4**.
//!
//! Every semantic below is pinned against Temurin 21.0.13 with
//! `corpus/HexFloatProbe.java` (97 cases, exact IEEE bits captured via
//! `floatToIntBits`/`doubleToLongBits`):
//!
//! * grammar: `[+-]? 0[xX] HexDigits [. HexDigits?] [pP] [+-]? DecDigits`,
//!   the binary exponent is REQUIRED, `0x` is REQUIRED (`.8p1` is an NFE —
//!   the Javadoc's `. HexDigits` significand branch is NOT accepted),
//!   the exponent sign IS accepted both ways, leading-zero exponent digits
//!   are accepted, trailing `f/F/d/D` suffixes are stripped by the caller;
//! * rounding: round-half-to-even on the exact binary value — the value is
//!   a hex significand times a power of two, so no decimal approximation
//!   ever enters the picture; the guard/sticky discipline below is exact;
//! * overflow: `|value| >= 2^(emax+1) - ulp/2` rounds to infinity (the
//!   max-finite + half-ulp tie rounds UP to infinity — even rule);
//! * subnormals: the grid drops to `2^(emin-p+1)`; exactly half the
//!   smallest subnormal rounds to zero (even rule), `1.5` ulp rounds to
//!   2 ulp, `0.75` ulp rounds to 1 ulp;
//! * zeros: `-0x0p0` keeps the sign (bits `0x80000000` / `0x8000...0`);
//! * no prefix (`.8p1`, `8p1`, `1p1`), missing exponent (`0x1.8`), missing
//!   digits (`0x`, `0xp1`), double dots, trailing junk → None → the caller
//!   raises the same `NumberFormatException` as decimal garbage.

/// Parsed result: the exact IEEE bit pattern (zero-extended to u64 for f32).
pub fn parse_bits(body: &str, is_f32: bool) -> Option<u64> {
    let (p, emin, emax, bias) = if is_f32 {
        (24u32, -126i64, 127i64, 127i64)
    } else {
        (53u32, -1022i64, 1023i64, 1023i64)
    };
    // sign
    let (neg, rest) = match body.as_bytes().first() {
        Some(b'-') => (true, &body[1..]),
        Some(b'+') => (false, &body[1..]),
        _ => (false, body),
    };
    // 0x / 0X prefix — REQUIRED for the hex forms
    let rest = if rest.len() >= 2
        && rest.as_bytes()[0] == b'0'
        && (rest.as_bytes()[1] == b'x' || rest.as_bytes()[1] == b'X')
    {
        &rest[2..]
    } else {
        return None;
    };

    // significand: hex digits, at most one '.', at least one digit total
    let mut nibbles: Vec<u8> = Vec::new();
    let mut frac: u32 = 0; // nibbles seen after the dot
    let mut seen_dot = false;
    let mut seen_digit = false;
    let bytes = rest.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' => {
                seen_digit = true;
                if seen_dot {
                    frac += 1;
                }
                nibbles.push((b as char).to_digit(16).unwrap() as u8);
                i += 1;
            }
            b'.' => {
                if seen_dot {
                    return None; // "0x1..8p1", "0x1.8.p1"
                }
                seen_dot = true;
                i += 1;
            }
            b'p' | b'P' => break,
            _ => return None, // "0xg1p1", "0x1.8 p1", "0x1.8e" wait e is hex — fine
        }
    }
    if !seen_digit {
        return None; // "0x", "0xp1", "0x."
    }
    // binary exponent — REQUIRED
    if i >= bytes.len() || (bytes[i] != b'p' && bytes[i] != b'P') {
        return None; // "0x1.8"
    }
    i += 1;
    let mut exp_sign = 1i64;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        if bytes[i] == b'-' {
            exp_sign = -1;
        }
        i += 1;
    }
    if i >= bytes.len() {
        return None; // "0x1p"
    }
    let mut exp: i64 = 0;
    let mut exp_digits = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if !b.is_ascii_digit() {
            return None; // "0x1.8p1.5", "0x1.8px1", "0x1.8p1e2"
        }
        exp = exp
            .saturating_mul(10)
            .saturating_add(exp_sign * (b - b'0') as i64);
        exp_digits += 1;
        i += 1;
    }
    if exp_digits == 0 {
        return None;
    }

    // value = M * 2^shift, M = integer over the nibbles, shift accounts for
    // the fraction nibbles (each = 2^-4)
    let shift0 = exp.saturating_sub(4 * frac as i64);

    // skip leading zero nibbles
    let nz = &nibbles[..];
    let mut start = 0usize;
    while start < nz.len() && nz[start] == 0 {
        start += 1;
    }
    if start == nz.len() {
        // all-zero significand → signed zero (the sign survives: "-0x0p0")
        return Some(if is_f32 {
            if neg {
                0x8000_0000u64
            } else {
                0
            }
        } else if neg {
            0x8000_0000_0000_0000u64
        } else {
            0
        });
    }

    // keep at most 16 nibbles (64 bits); anything beyond is a sticky boolean.
    // With 64 kept bits and p <= 53, the final rounding's guard bit always
    // sits inside the kept value, so the tail can only feed the sticky —
    // the decision stays exact.
    const KEEP: usize = 16;
    let avail = &nz[start..];
    let keep_n = avail.len().min(KEEP);
    let dropped = avail.len() - keep_n;
    let sticky_tail = avail[keep_n..].iter().any(|&n| n != 0);
    let mut m: u64 = 0;
    for &n in &avail[..keep_n] {
        m = (m << 4) | n as u64;
    }
    let shift: i64 = shift0.saturating_add(4 * dropped as i64);
    let sticky_any = sticky_tail; // M's own low bits feed sticky inside the rounder

    // top = position of M's highest set bit; E = unbiased exponent of the
    // leading-1 position of the value
    let top = (63 - u64::from(m.leading_zeros())) as i64;
    let e = top.saturating_add(shift);

    if e > emax {
        // overflow → infinity (sign preserved)
        return Some(
            (if is_f32 {
                0x7F80_0000u64
            } else {
                0x7FF0_0000_0000_0000u64
            }) | (if neg {
                if is_f32 {
                    0x8000_0000u64
                } else {
                    0x8000_0000_0000_0000u64
                }
            } else {
                0
            }),
        );
    }

    // round M * 2^shift to the target grid g (round-half-to-even)
    let g = if e >= emin {
        e - (p as i64 - 1)
    } else {
        emin - (p as i64 - 1)
    };
    let d = shift - g; // i64
    let (mut k, guard, sticky_m): (u64, bool, bool) = if d >= 0 {
        // exact left shift — d < p is guaranteed by e <= emax above
        (m.checked_shl(d as u32)?, false, sticky_any)
    } else {
        let dd = (-d) as u32;
        if dd >= 64 {
            // every kept bit is below the guard position
            (0, false, m != 0 || sticky_any)
        } else {
            let k = m >> dd;
            let guard = (m >> (dd - 1)) & 1 == 1;
            let sticky = (m & ((1u64 << (dd - 1)) - 1)) != 0 || sticky_any;
            (k, guard, sticky)
        }
    };
    if guard && (sticky_m || k & 1 == 1) {
        k += 1;
    }

    if e >= emin {
        // normal path: k in [2^(p-1), 2^p]
        if k == 1u64 << p {
            // carry out of the mantissa → exponent bump
            k >>= 1;
            let e2 = e + 1;
            if e2 > emax {
                return Some(inf_bits(is_f32, neg));
            }
            return Some(assemble(is_f32, neg, e2, k, bias));
        }
        Some(assemble(is_f32, neg, e, k, bias))
    } else {
        // subnormal path: k counts grid units 2^(emin-p+1)
        if k == 0 {
            return Some(if is_f32 {
                if neg {
                    0x8000_0000u64
                } else {
                    0
                }
            } else if neg {
                0x8000_0000_0000_0000u64
            } else {
                0
            });
        }
        if k >= 1u64 << (p - 1) {
            // crossed into the normal range at emin
            return Some(assemble(is_f32, neg, emin, k, bias));
        }
        // subnormal bits directly
        let bits = k;
        Some(if is_f32 {
            (if neg { 0x8000_0000u64 } else { 0 }) | bits
        } else {
            (if neg { 0x8000_0000_0000_0000u64 } else { 0 }) | bits
        })
    }
}

fn inf_bits(is_f32: bool, neg: bool) -> u64 {
    let mut b = if is_f32 {
        0x7F80_0000u64
    } else {
        0x7FF0_0000_0000_0000u64
    };
    if neg {
        b |= if is_f32 {
            0x8000_0000u64
        } else {
            0x8000_0000_0000_0000u64
        };
    }
    b
}

fn assemble(is_f32: bool, neg: bool, e: i64, k: u64, bias: i64) -> u64 {
    let p = if is_f32 { 24u32 } else { 53u32 };
    let mant = k - (1u64 << (p - 1)); // drop the implicit leading 1
    let bits = (((e + bias) as u64) << (p - 1)) | mant;
    if is_f32 {
        (if neg { 0x8000_0000u64 } else { 0 }) | (bits & 0xFFFF_FFFF)
    } else if neg {
        0x8000_0000_0000_0000u64 | bits
    } else {
        bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every expectation below was captured from Temurin 21.0.13 with
    // corpus/HexFloatProbe.java (floatToIntBits / doubleToLongBits).

    #[test]
    fn basic_shapes() {
        assert_eq!(parse_bits("0x1.8p1", true), Some(0x4040_0000));
        assert_eq!(parse_bits("0x1.8p1", false), Some(0x4008_0000_0000_0000));
        assert_eq!(parse_bits("0X1.8P1", true), Some(0x4040_0000));
        assert_eq!(parse_bits("0x.8p1", true), Some(0x3F80_0000)); // 1.0
        assert_eq!(parse_bits("0x1.p1", true), Some(0x4000_0000)); // 2.0
        assert_eq!(parse_bits("0x1p1", true), Some(0x4000_0000));
        assert_eq!(parse_bits("-0x1.8p1", true), Some(0xC040_0000));
        assert_eq!(parse_bits("+0x1.8p1", true), Some(0x4040_0000));
        assert_eq!(parse_bits("0x1p-1", false), Some(0x3FE0_0000_0000_0000));
        assert_eq!(parse_bits("0x1p+1", true), Some(0x4000_0000));
        assert_eq!(parse_bits("0x1p01", true), Some(0x4000_0000));
        assert_eq!(parse_bits("0x1.8p+0", false), Some(0x3FF8_0000_0000_0000));
    }

    #[test]
    fn signed_zeros() {
        assert_eq!(parse_bits("0x0p0", true), Some(0));
        assert_eq!(parse_bits("-0x0p0", true), Some(0x8000_0000));
        assert_eq!(parse_bits("+0x0p0", true), Some(0));
        assert_eq!(parse_bits("-0x0.0p0", false), Some(0x8000_0000_0000_0000));
        assert_eq!(parse_bits("0x0p-149", true), Some(0));
        assert_eq!(parse_bits("-0x0p-1074", false), Some(0x8000_0000_0000_0000));
        assert_eq!(parse_bits("0x0000p-5", false), Some(0));
    }

    #[test]
    fn grammar_rejections() {
        for s in [
            ".8p1",
            "8p1",
            "1p1",
            "0x1.8",
            "0x1p",
            "0xp1",
            "0x",
            "0x.8",
            "0x1.8p",
            "0x1.8p1.5",
            "0x1.8px1",
            "0x1.8p 1",
            "0xg1p1",
            "0x1.8.p1",
            "0x1..8p1",
            "0x1.8p1x",
            "0b101",
            "0x1.8p1e2",
            "",
        ] {
            assert_eq!(parse_bits(s, true), None, "{s} must be rejected");
            assert_eq!(parse_bits(s, false), None, "{s} must be rejected");
        }
        // 'e' is a hex digit, not an exponent marker
        assert_eq!(parse_bits("0x1.8e2p1", true), Some(0x4047_1000));
    }

    #[test]
    fn rounding_ties() {
        // 1 + 2^-56 → below f64 ulp → 1.0
        assert_eq!(
            parse_bits("0x1.00000000000001p0", false),
            Some(0x3FF0_0000_0000_0000)
        );
        // exactly halfway (1 + 2^-53) → ties to even → 1.0
        assert_eq!(
            parse_bits("0x1.00000000000008p0", false),
            Some(0x3FF0_0000_0000_0000)
        );
        // above halfway → up → 1 + 2^-52
        assert_eq!(
            parse_bits("0x1.00000000000009p0", false),
            Some(0x3FF0_0000_0000_0001)
        );
        assert_eq!(
            parse_bits("0x1.0000000000000fp0", false),
            Some(0x3FF0_0000_0000_0001)
        );
        // f32: 1 + 2^-24 is the halfway point between 1.0 and 1+2^-23 → down (even)
        assert_eq!(parse_bits("0x1.000001p0", true), Some(0x3F80_0000));
        assert_eq!(parse_bits("0x1.0000008p0", true), Some(0x3F80_0000));
        assert_eq!(parse_bits("0x1.0000009p0", true), Some(0x3F80_0000)); // still below halfway
                                                                          // max-finite boundary: 0x1.fffffep-1 stays, 0x1.ffffffp-1 ties up to 1.0
        assert_eq!(parse_bits("0x1.fffffep-1", true), Some(0x3F7F_FFFF));
        assert_eq!(parse_bits("0x1.ffffffp-1", true), Some(0x3F80_0000));
    }

    #[test]
    fn overflow_to_infinity() {
        assert_eq!(parse_bits("0x1p128", true), Some(0x7F80_0000));
        assert_eq!(parse_bits("-0x1p128", true), Some(0xFF80_0000));
        assert_eq!(parse_bits("0x1.ffffffp127", true), Some(0x7F80_0000));
        assert_eq!(parse_bits("0x1p127", true), Some(0x7F00_0000)); // finite
        assert_eq!(parse_bits("0x1.fffffep127", true), Some(0x7F7F_FFFF)); // MAX
        assert_eq!(parse_bits("0x1p1024", false), Some(0x7FF0_0000_0000_0000));
        assert_eq!(parse_bits("-0x1p2000", false), Some(0xFFF0_0000_0000_0000));
        assert_eq!(parse_bits("0x1p1023", false), Some(0x7FE0_0000_0000_0000)); // finite
        assert_eq!(
            parse_bits("0x1.fffffffffffffp1023", false),
            Some(0x7FEF_FFFF_FFFF_FFFF)
        );
        assert_eq!(
            parse_bits("0x1.ffffffffffff8p1023", false),
            Some(0x7FEF_FFFF_FFFF_FFF8)
        );
    }

    #[test]
    fn subnormal_grid() {
        assert_eq!(parse_bits("0x1p-149", true), Some(1)); // Float.MIN_VALUE
        assert_eq!(parse_bits("0x1p-150", true), Some(0)); // half → ties to even → 0
        assert_eq!(parse_bits("0x1.000001p-149", true), Some(1)); // 1 ulp + tiny
        assert_eq!(parse_bits("0x1.8p-150", true), Some(1)); // 0.75 ulp → up
        assert_eq!(parse_bits("0x1p-126", true), Some(0x0080_0000)); // min normal
        assert_eq!(parse_bits("0x1p-127", true), Some(0x0040_0000)); // subnormal
        assert_eq!(parse_bits("0x1p-1074", false), Some(1)); // Double.MIN_VALUE
        assert_eq!(parse_bits("-0x1p-1074", false), Some(0x8000_0000_0000_0001));
        assert_eq!(parse_bits("0x1p-1075", false), Some(0)); // half → 0
        assert_eq!(parse_bits("0x1.8p-1075", false), Some(1)); // 1.5x → 2 ulp? no: 0.75 → 1
        assert_eq!(parse_bits("0x1p-1022", false), Some(0x0010_0000_0000_0000));
        assert_eq!(parse_bits("0x1p-1023", false), Some(0x0008_0000_0000_0000));
        assert_eq!(
            parse_bits("0x1.fffffffffffffp-1022", false),
            Some(0x001F_FFFF_FFFF_FFFF)
        );
        assert_eq!(parse_bits("0x1p-3000", false), Some(0));
        assert_eq!(parse_bits("0x1p-3000", true), Some(0));
    }

    #[test]
    fn long_significands_trim_and_sticky() {
        // 15 hex digits (60 bits) — fits exactly in the kept window
        assert_eq!(
            parse_bits("0x123456789abcdefp0", false),
            Some(0x4372_3456_789A_BCDF)
        );
        // f32: 60 bits → 24, guard+sticky decide: rounds up to 0x91A2B4
        assert_eq!(parse_bits("0x123456789abcdefp0", true), Some(0x5B91_A2B4));
        assert_eq!(
            parse_bits("0xfedcba987654321p-10", false),
            Some(0x430F_DB97_530E_CA86)
        );
        // 29 fraction digits — deep subnormal-ish tiny normal, exact
        assert_eq!(
            parse_bits("0x0.00000000000000000000000000001p0", false),
            Some(0x38B0_0000_0000_0000)
        );
        // 21 fraction digits of 1 → 1 + 2^-84: below f64 ulp → 1.0 (sticky tail only)
        assert_eq!(
            parse_bits("0x1.0000000000000000000001p0", false),
            Some(0x3FF0_0000_0000_0000)
        );
        assert_eq!(
            parse_bits("0x1.0000000000000000000001p0", true),
            Some(0x3F80_0000)
        );
        assert_eq!(
            parse_bits("0x0000000000000000000001p3", true),
            Some(0x4100_0000)
        );
    }

    #[test]
    fn whitespace_free_zone() {
        // the caller trims; the parser itself sees clean bodies
        assert_eq!(parse_bits("2.5", true), None); // decimal stays decimal
        assert_eq!(parse_bits("Infinity", true), None);
        assert_eq!(parse_bits("NaN", false), None);
    }
}
