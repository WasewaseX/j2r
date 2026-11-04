//! Java-exact `double`/`float` → `String` conversion (`Double.toString`,
//! `Float.toString`, used by `println`, string concatenation and
//! `String.valueOf`).
//!
//! Java rules (JDK 19+ `Double.toString`):
//! * `NaN`, `Infinity`, `-Infinity` spelled out; `-0.0` keeps its sign.
//! * Normal values use the shortest decimal digits that round-trip — Rust's
//!   `{:e}` already produces those.
//! * Subnormal values use the fewest digits `N >= 2` such that the decimal
//!   lands within half an ulp (2^-1074 / 2^-149) of the exact value — that is
//!   NOT the same as plain shortest (`Double.MIN_VALUE` is `4.9E-324`, not
//!   `5E-324`, even though both round-trip). This module reproduces Java's
//!   rule exactly using exact decimal expansions of `m * 2^-k`.
//! * If the decimal exponent satisfies `-3 <= e < 7`, plain decimal notation
//!   (always with a fractional part, e.g. `100.0`); otherwise scientific
//!   notation `d.dddEn` (e.g. `1.0E-4`).

fn shortest_digits<T: std::fmt::LowerExp>(v: T) -> (String, i32) {
    let s = format!("{v:e}"); // e.g. "1.2345e7"
    let (mantissa, exp) = s.split_once('e').expect("LowerExp always has 'e'");
    let e: i32 = exp.parse().expect("LowerExp exponent parses");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    (digits, e)
}

fn plain_decimal(digits: &str, e: i32) -> String {
    // value == digits[0].digits[1..] * 10^e
    let n = digits.len() as i32;
    let mut out = String::with_capacity(digits.len() + 8);
    if e >= 0 {
        if n - 1 <= e {
            out.push_str(digits);
            for _ in 0..(e - (n - 1)) {
                out.push('0');
            }
            out.push_str(".0");
        } else {
            out.push_str(&digits[..(e + 1) as usize]);
            out.push('.');
            out.push_str(&digits[(e + 1) as usize..]);
        }
    } else {
        out.push_str("0.");
        for _ in 0..(-e - 1) {
            out.push('0');
        }
        out.push_str(digits);
    }
    out
}

fn scientific(digits: &str, e: i32) -> String {
    let mut out = String::with_capacity(digits.len() + 8);
    out.push_str(&digits[..1]);
    out.push('.');
    if digits.len() > 1 {
        out.push_str(&digits[1..]);
    } else {
        out.push('0');
    }
    out.push('E');
    out.push_str(&e.to_string());
    out
}

fn render(neg: bool, digits: String, e: i32) -> String {
    let body = if (-3..7).contains(&e) {
        plain_decimal(&digits, e)
    } else {
        scientific(&digits, e)
    };
    if neg {
        format!("-{body}")
    } else {
        body
    }
}

// ------------------------------------------------------------- bignum (small)

/// Minimal unsigned bignum (little-endian base 2^32) for exact subnormal
/// decimal expansion.
#[derive(Clone)]
struct Big(Vec<u32>);

impl Big {
    fn from_u64(v: u64) -> Big {
        Big(vec![v as u32, (v >> 32) as u32])
    }

    fn add_small(&mut self, m: u32) {
        let mut carry = m as u64;
        for limb in self.0.iter_mut() {
            carry += *limb as u64;
            *limb = carry as u32;
            carry >>= 32;
            if carry == 0 {
                break;
            }
        }
        if carry > 0 {
            self.0.push(carry as u32);
        }
    }

    fn mul_small(&mut self, m: u32) {
        let mut carry: u64 = 0;
        for limb in self.0.iter_mut() {
            let prod = (*limb as u64) * (m as u64) + carry;
            *limb = prod as u32;
            carry = prod >> 32;
        }
        while carry > 0 {
            self.0.push(carry as u32);
            carry >>= 32;
        }
    }

    fn cmp(&self, other: &Big) -> std::cmp::Ordering {
        let n = self.0.len().max(other.0.len());
        for i in (0..n).rev() {
            let a = self.0.get(i).copied().unwrap_or(0);
            let b = other.0.get(i).copied().unwrap_or(0);
            if a != b {
                return a.cmp(&b);
            }
        }
        std::cmp::Ordering::Equal
    }

    fn to_decimal(&self) -> String {
        // repeated division by 10^9
        let mut limbs = self.0.clone();
        while limbs.last() == Some(&0) {
            limbs.pop();
        }
        if limbs.is_empty() {
            return "0".to_string();
        }
        let mut parts: Vec<u32> = Vec::new();
        while !limbs.is_empty() {
            let mut rem: u64 = 0;
            for limb in limbs.iter_mut().rev() {
                let cur = (rem << 32) | *limb as u64;
                *limb = (cur / 1_000_000_000) as u32;
                rem = cur % 1_000_000_000;
            }
            while limbs.last() == Some(&0) {
                limbs.pop();
            }
            parts.push(rem as u32);
        }
        let mut out = String::new();
        for (i, p) in parts.iter().enumerate().rev() {
            if i == parts.len() - 1 {
                out.push_str(&p.to_string());
            } else {
                out.push_str(&format!("{p:09}"));
            }
        }
        out
    }
}

/// Computes `5^k` as an exact bignum.
fn subnormal_pow5(k: u32) -> Big {
    let mut pow5 = Big::from_u64(1);
    let mut remaining = k;
    const CH: u32 = 13; // 5^13 = 1220703125 < 2^31
    let ch5: u32 = 1_220_703_125;
    while remaining >= CH {
        pow5.mul_small(ch5);
        remaining -= CH;
    }
    let small5: [u32; 13] = [
        1,
        5,
        25,
        125,
        625,
        3125,
        15625,
        78125,
        390625,
        1_953_125,
        9_765_625,
        48_828_125,
        244_140_625,
    ];
    pow5.mul_small(small5[remaining as usize]);
    pow5
}

/// Exact decimal expansion of the subnormal value `m * 2^-k`.
/// Returns `(digits_without_dot, exponent10)` with `digits[0] != '0'`.
fn subnormal_digits(m: u64, k: u32) -> (String, i32) {
    // v = m / 2^k = (m * 5^k) / 10^k
    let pow5 = subnormal_pow5(k);
    // M = m * 5^k via schoolbook multiplication
    let m_big = Big::from_u64(m);
    let mut prod = vec![0u32; m_big.0.len() + pow5.0.len() + 2];
    for (i, a) in pow5.0.iter().enumerate() {
        let mut carry: u64 = 0;
        for (j, b) in m_big.0.iter().enumerate() {
            let idx = i + j;
            let cur = prod[idx] as u64 + (*a as u64) * (*b as u64) + carry;
            prod[idx] = cur as u32;
            carry = cur >> 32;
        }
        let mut idx = i + m_big.0.len();
        while carry > 0 {
            let cur = prod[idx] as u64 + carry;
            prod[idx] = cur as u32;
            carry = cur >> 32;
            idx += 1;
        }
    }
    while prod.last() == Some(&0) {
        prod.pop();
    }
    let m_int = Big(prod);
    let s = m_int.to_decimal();
    let len = s.len() as i32;
    // v = s * 10^-k, scientific exponent:
    (s, len - 1 - k as i32)
}

/// Chooses Java's digit count for a subnormal: smallest `N >= 2` such that
/// rounding the exact expansion to `N` significant digits stays within half
/// an ulp (`2^-k / 2`).
fn subnormal_render(m: u64, k: u32, neg: bool) -> String {
    let (all_digits, e10) = subnormal_digits(m, k);
    // half-ulp of the subnormal = 2^-(k+1). In the units of the scaled
    // mantissa M (which represents M * 10^-k) the condition is:
    //   |scaled - M| <= 10^k / 2^(k+1) = 5^k / 2
    // so we compare 2*diff against pow5 = 5^k.
    let pow5 = subnormal_pow5(k);
    let l = all_digits.len();
    let m_int = {
        // exact value as Big scaled by 10^k: m * 5^k — rebuild from digits
        let mut big = Big(vec![0]);
        for c in all_digits.chars() {
            big.mul_small(10);
            big.add_small(c.to_digit(10).unwrap());
        }
        big
    };
    let mut chosen: Option<(String, i32)> = None;
    for n in 2..=l.min(18) {
        // round all_digits to n significant digits (nearest, ties up — matches
        // how Java renders exact expansions)
        let mut digits: String = all_digits[..n].to_string();
        let round_up = match all_digits.as_bytes().get(n) {
            Some(&c) => c >= b'5',
            None => false,
        };
        if round_up {
            let mut bs: Vec<u8> = digits.bytes().collect();
            let mut i = bs.len();
            loop {
                if i == 0 {
                    bs.insert(0, b'1');
                    break;
                }
                i -= 1;
                if bs[i] == b'9' {
                    bs[i] = b'0';
                } else {
                    bs[i] += 1;
                    break;
                }
            }
            digits = String::from_utf8(bs).unwrap();
        }
        let mut exp = e10;
        if digits.len() > n {
            // carry grew the digit count: 99..9 -> 100..0
            exp += 1;
        }
        // exactness check: |digits * 10^(l - len) - M| <= half_ulp
        let npow = (l - digits.len()) as u32;
        let mut scaled = Big(vec![0]);
        for c in digits.chars() {
            scaled.mul_small(10);
            scaled.add_small(c.to_digit(10).unwrap());
        }
        for _ in 0..npow {
            scaled.mul_small(10);
        }
        // |scaled - m_int| — both bignums; subtract smaller from bigger
        let mut diff = match scaled.cmp(&m_int) {
            std::cmp::Ordering::Greater => sub_big(&scaled, &m_int),
            std::cmp::Ordering::Less => sub_big(&m_int, &scaled),
            std::cmp::Ordering::Equal => Big(vec![0]),
        };
        diff.mul_small(2);
        if diff.cmp(&pow5) != std::cmp::Ordering::Greater {
            // keep trailing zeros trimmed for the shortest form within this N
            // (e.g. 2.0E-323 prints as 2.0E-323 — one zero kept via plain
            // decimal/scientific rendering rules below)
            while digits.len() > 2 && digits.ends_with('0') {
                digits.pop();
            }
            chosen = Some((digits, exp));
            break;
        }
    }
    let (digits, exp) = chosen.unwrap_or_else(|| {
        // fallback: 18 digits (cannot happen for IEEE subnormals)
        let d = all_digits[..18.min(l)].to_string();
        (d, e10)
    });
    render(neg, digits, exp)
}

fn sub_big(a: &Big, b: &Big) -> Big {
    let mut out = Vec::with_capacity(a.0.len());
    let mut borrow: i64 = 0;
    for i in 0..a.0.len() {
        let x = a.0[i] as i64 - b.0.get(i).copied().unwrap_or(0) as i64 - borrow;
        if x < 0 {
            out.push((x + (1i64 << 32)) as u32);
            borrow = 1;
        } else {
            out.push(x as u32);
            borrow = 0;
        }
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    Big(out)
}

macro_rules! java_float_fmt {
    ($name:ident, $t:ty, $k:expr, $mant_bits:expr) => {
        pub fn $name(v: $t) -> String {
            if v.is_nan() {
                return "NaN".to_string();
            }
            if v.is_infinite() {
                return if v > 0.0 {
                    "Infinity".to_string()
                } else {
                    "-Infinity".to_string()
                };
            }
            if v == 0.0 {
                return if v.is_sign_negative() {
                    "-0.0".to_string()
                } else {
                    "0.0".to_string()
                };
            }
            let neg = v.is_sign_negative();
            let av = v.abs();
            let min_normal = <$t>::MIN_POSITIVE;
            if av < min_normal {
                // subnormal: exact-expansion formatting
                let bits = av.to_bits() as u64;
                let mantissa = bits & ((1u64 << $mant_bits) - 1);
                return subnormal_render(mantissa, $k, neg);
            }
            let (digits, e) = shortest_digits(av);
            render(neg, digits, e)
        }
    };
}

java_float_fmt!(double_to_string, f64, 1074, 52);
java_float_fmt!(float_to_string, f32, 149, 23);

/// `Integer.toHexString` semantics (used by `Object.toString`).
pub fn int_to_hex_string(v: i32) -> String {
    format!("{:x}", v as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_match_java() {
        let _rt = crate::test_support::runtime();
        assert_eq!(double_to_string(1.0), "1.0");
        assert_eq!(double_to_string(-1.0), "-1.0");
        assert_eq!(double_to_string(0.5), "0.5");
        assert_eq!(double_to_string(1.5), "1.5");
        assert_eq!(double_to_string(100.0), "100.0");
        assert_eq!(double_to_string(0.001), "0.001");
        assert_eq!(double_to_string(1e-4), "1.0E-4");
        assert_eq!(double_to_string(1234567.0), "1234567.0");
        assert_eq!(double_to_string(12345678.0), "1.2345678E7");
        assert_eq!(double_to_string(-0.0), "-0.0");
        assert_eq!(double_to_string(0.0), "0.0");
        assert_eq!(double_to_string(f64::NAN), "NaN");
        assert_eq!(double_to_string(f64::INFINITY), "Infinity");
        assert_eq!(double_to_string(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(double_to_string(f64::MAX), "1.7976931348623157E308");
        assert_eq!(
            double_to_string(f64::MIN_POSITIVE),
            "2.2250738585072014E-308"
        );
        assert_eq!(double_to_string(3.75), "3.75");
        assert_eq!(double_to_string(1e23), "1.0E23");
        assert_eq!(double_to_string(2.5e-8), "2.5E-8");
    }

    #[test]
    fn doubles_subnormal_match_java() {
        let _rt = crate::test_support::runtime();
        // ground truth captured from OpenJDK 21 (see repo notes)
        assert_eq!(double_to_string(f64::from_bits(1)), "4.9E-324");
        assert_eq!(double_to_string(f64::from_bits(2)), "9.9E-324");
        assert_eq!(double_to_string(f64::from_bits(3)), "1.5E-323");
        assert_eq!(double_to_string(f64::from_bits(4)), "2.0E-323");
        assert_eq!(double_to_string(f64::from_bits(7)), "3.5E-323");
        assert_eq!(double_to_string(f64::from_bits(10)), "4.9E-323");
        assert_eq!(double_to_string(f64::from_bits(123)), "6.1E-322");
        assert_eq!(double_to_string(f64::from_bits(1000)), "4.94E-321");
        assert_eq!(double_to_string(f64::from_bits(12345)), "6.099E-320");
        assert_eq!(double_to_string(f64::from_bits(999_999)), "4.94065E-318");
        assert_eq!(double_to_string(f64::from_bits(1_234_567)), "6.09957E-318");
        assert_eq!(
            double_to_string(f64::from_bits(4_503_599_627_370_495)),
            "2.225073858507201E-308"
        );
    }

    #[test]
    fn floats_match_java() {
        let _rt = crate::test_support::runtime();
        assert_eq!(float_to_string(1.0f32), "1.0");
        assert_eq!(float_to_string(3.75f32), "3.75");
        assert_eq!(float_to_string(0.001f32), "0.001");
        assert_eq!(float_to_string(1e-4f32), "1.0E-4");
        assert_eq!(float_to_string(f32::MAX), "3.4028235E38");
        assert_eq!(float_to_string(f32::MIN_POSITIVE), "1.1754944E-38");
        assert_eq!(float_to_string(100.0f32), "100.0");
        assert_eq!(float_to_string(f32::NAN), "NaN");
        assert_eq!(float_to_string(f32::INFINITY), "Infinity");
    }

    #[test]
    fn floats_subnormal_match_java() {
        let _rt = crate::test_support::runtime();
        assert_eq!(float_to_string(f32::from_bits(1)), "1.4E-45");
    }

    #[test]
    fn hex_matches_java() {
        let _rt = crate::test_support::runtime();
        assert_eq!(int_to_hex_string(255), "ff");
        assert_eq!(int_to_hex_string(-1), "ffffffff");
        assert_eq!(int_to_hex_string(0), "0");
    }
}
