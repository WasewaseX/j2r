use super::*;

/// `int` family renderers for %x/%X/%o: JDK two's-complement by argument
/// width (Int → u32, Long → u64; `-255` prints `ffffff01` — FmtProbe row).
pub(crate) fn hex_oct(v: &Value, base: u32, upper: bool, hash: bool) -> Result<String, Throw> {
    let digits: u128 = match v {
        Value::Int(i) => *i as u32 as u128,
        Value::Long(l) => *l as u64 as u128,
        _ => {
            return Err(err(
                "java/util/IllegalFormatConversionException",
                &format!("x != {}", arg_class(v)),
            ))
        }
    };
    let mut s = match base {
        16 if upper => format!("{:X}", digits),
        16 => format!("{:x}", digits),
        8 => format!("{:o}", digits),
        _ => digits.to_string(),
    };
    if hash && digits != 0 && (base == 16 || base == 8) {
        s = format!("0{}", s);
        if base == 16 {
            s = format!("0x{}", &s[1..]);
        }
    }
    Ok(s)
}
/// Widens a floating arg to f64 for %f/%e/%g/%a (JDK: Float widens
/// exactly; Double passes through). The JDK does NOT widen integral types
/// — %f of an Integer is IllegalFormatConversionException
/// `f != java.lang.Integer` (FmtResidualProbe U13a-d; %e of a Long is
/// `e != java.lang.Long`), %f of a String is `f != java.lang.String` (V8).
/// The conv char arrives LOWERCASED (the known-conversion message law).
pub(crate) fn as_f64(v: &Value, conv: char) -> Result<f64, Throw> {
    match v {
        Value::Float(f) => Ok(*f as f64),
        Value::Double(d) => Ok(*d),
        _ => Err(err(
            "java/util/IllegalFormatConversionException",
            &format!("{conv} != {}", arg_class(v)),
        )),
    }
}
/// ONE decimal rounding of a digit string, HALF_UP: `digits` has no dot;
/// `round_idx` is the index just past the kept digits; a digit >= '5'
/// triggers a carry that propagates left through the whole string.
/// Returns the (possibly longer) digit string.
pub(crate) fn round_digits(digits: &str, round_idx: usize) -> String {
    let d: Vec<u8> = digits.as_bytes().to_vec();
    let bump = d.get(round_idx).map(|c| *c >= b'5').unwrap_or(false);
    if !bump {
        // truncate to the kept prefix — the no-bump path still ROUNDS DOWN
        return String::from_utf8_lossy(&d[..round_idx.min(d.len())]).into_owned();
    }
    let mut out: Vec<u8> = d[..round_idx.min(d.len())].to_vec();
    if out.is_empty() {
        return "1".to_string();
    }
    let mut i = out.len();
    loop {
        if i == 0 {
            out.insert(0, b'1');
            break;
        }
        i -= 1;
        if out[i] < b'9' {
            out[i] += 1;
            break;
        }
        out[i] = b'0';
    }
    String::from_utf8(out).unwrap_or_else(|_| digits.to_string())
}
/// Fixed-point %f rendering of a double: HALF_UP on the shortest decimal
/// repr (see the module doc for why 2.675 -> 2.68). `precision` decimals,
/// grouped with ',' when requested.
///
/// B1 (external jbox2d port report, 2026-11-12): the shortest repr goes
/// scientific below 1e-3 and at/above 1e7 — the old code split on '.'
/// and treated the `E` tail as fraction digits, destroying every
/// %f/%.Nf/%,f of such magnitudes (6.149717374e-4 printed "6.149717374"
/// where the JVM prints "0.000614972"; 1.0e-10 printed "1.0E-100000").
/// The repr is now normalized to a plain digit string + a decimal-point
/// position BEFORE the precision rounding — the same parse render_e
/// already performs. The rounding-carry re-split is fixed too: a HALF_UP
/// bump that grows the digit string (9.99 at %.1f -> "10.0") moves the
/// decimal point right by one, and the old fixed `split_at(int_len)`
/// printed an extra fraction digit (or truncated the integer part at
/// precision 0, printing "1" for %.0f of 9.99).
pub(crate) fn render_f(v: f64, precision: usize) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-Infinity" } else { "Infinity" }.to_string();
    }
    let neg = v < 0.0 || (v == 0.0 && v.is_sign_negative());
    let repr = crate::javafmt::double_to_string(v.abs());
    // normalize: [digits][.digits][E[+-]exp] -> digits + point position
    let (mantissa, dec_exp) = match repr.split_once('E') {
        Some((m, e)) => (m.to_string(), e.parse::<i64>().unwrap_or(0)),
        None => (repr.clone(), 0i64),
    };
    let (mut int_part, frac_part) = match mantissa.split_once('.') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (mantissa.clone(), String::new()),
    };
    if int_part.is_empty() {
        int_part.push('0');
    }
    // position of the decimal point within `digits`, counted from the
    // start of the digit string (may be negative or past the end)
    let mut point = dec_exp + int_part.len() as i64;
    let mut digits = format!("{}{}", int_part, frac_part);
    // a negative point only means leading zeros: shift them into `digits`
    // so the rounding index is a plain usize (6.14972E-4 -> "000614972",
    // point 0)
    if point < 0 {
        digits.insert_str(0, &"0".repeat((-point) as usize));
        point = 0;
    }
    // HALF_UP at the (point + precision)-th digit
    let round_idx = point as usize + precision;
    if digits.len() > round_idx {
        let grown = round_digits(&digits, round_idx);
        if grown.len() > round_idx {
            // the carry pushed a digit PAST the cut ("999" kept to 2 is
            // "100"): the decimal point moved right by the overflow
            point += (grown.len() - round_idx) as i64;
        }
        digits = grown;
    }
    // pad the fraction to exactly `precision` digits
    while digits.len() < point as usize + precision {
        digits.push('0');
    }
    // split at the (possibly shifted) point
    let (mut ip, fp) = if point as usize >= digits.len() {
        let pad = "0".repeat(point as usize - digits.len());
        (format!("{digits}{pad}"), String::new())
    } else {
        (
            digits[..point as usize].to_string(),
            digits[point as usize..].to_string(),
        )
    };
    if ip.is_empty() {
        ip = "0".to_string();
    }
    // zero precision: NO decimal point (Temurin: %.0f of 2.5 -> "3")
    let mut s = if precision == 0 {
        ip
    } else {
        format!("{ip}.{fp}")
    };
    if neg {
        s.insert(0, '-');
    }
    s
}
/// Scientific %e rendering: d.dddE±XX, HALF_UP on the shortest repr; the
/// exponent can change when rounding carries (9.9999 -> 1.0E+01).
pub(crate) fn render_e(v: f64, precision: usize) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-Infinity" } else { "Infinity" }.to_string();
    }
    // is_sign_negative, NOT `< 0.0`: %E of -0.0 is "-0.000000E+00" and %g
    // of -0.0 is "-0.00000" (FmtResidualProbe U9, SProbe S9) — the old
    // `v < 0.0` test dropped the sign of negative zero.
    let neg = v.is_sign_negative();
    let repr = crate::javafmt::double_to_string(v.abs());
    // parse: [digits][.digits][E[+-]decexp]
    let (mantissa, dec_exp) = match repr.split_once('E') {
        Some((m, e)) => (m.to_string(), e.parse::<i64>().unwrap_or(0)),
        None => (repr.clone(), 0i64),
    };
    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (mantissa.clone(), String::new()),
    };
    let int_len = int_part.len() as i64;
    // true decimal exponent of the first digit: position of the '.' relative
    let mut total_exp = dec_exp + int_len - 1;
    let mut digits = format!("{}{}", int_part, frac_part);
    // The fixed-form reprs BELOW 1.0 ("0.001", "0.5") carry leading zeros —
    // the first SIGNIFICANT digit defines the exponent (%e of 0.001 is
    // "1.00000e-03", of 0.5 is "5.000000e-01"; exposed by the %g wave — the
    // original FmtProbe had no %e row below 1.0, so this survived). Strip
    // the leading zeros; a zero value keeps exp 0 ("0.00000e+00").
    match digits.bytes().position(|b| b != b'0') {
        None => total_exp = 0,
        Some(k) => {
            total_exp -= k as i64;
            digits.replace_range(..k, "");
        }
    }
    // keep precision+1 significant digits, HALF_UP
    let keep = precision + 1;
    if digits.len() > keep {
        digits = round_digits(&digits, keep);
    }
    while digits.len() < keep {
        digits.push('0');
    }
    // a rounding carry may have grown the digit string by one
    if digits.len() > keep {
        total_exp += 1;
        digits.truncate(keep);
    }
    let mut s = String::new();
    if neg {
        s.push('-');
    }
    s.push_str(&digits[..1]);
    if precision > 0 {
        s.push('.');
        s.push_str(&digits[1..]);
    }
    // JDK %e is lowercase ('%E' is the uppercase variant — unmodelled here)
    s.push('e');
    if total_exp >= 0 {
        s.push('+');
    } else {
        s.push('-');
    }
    s.push_str(&format!("{:02}", total_exp.abs()));
    s
}
/// General %g rendering (Temurin-pinned: FmtResidualProbe B1-B14, SProbe
/// S5-S10, SProbe3 U7/U8/U12, SProbe4 V4): round to `p-1` SCIENTIFIC
/// mantissa digits FIRST (the rounding carry may bump the decimal exponent
/// — %.2G of 9.99 -> 1.0e1 -> fixed "10", while %.3G of 999.99 -> 1.00e3
/// stays scientific "1.00E+03"), then branch on the ROUNDED exponent:
/// scientific when exp < -4 or exp >= p, otherwise fixed with
/// precision_after = p - 1 - exp and NO trailing-zero stripping (1e-4 ->
/// "0.000100000", 0.0 -> "0.00000"). Precision 0 is taken as 1 ("%.0G" of
/// 12345 -> "1E+04"). The uppercase variant comes from the unified upper
/// flag (%G of NaN -> "NAN", of Infinity -> "INFINITY").
pub(crate) fn render_g(v: f64, precision: Option<usize>) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-Infinity" } else { "Infinity" }.to_string();
    }
    let neg = v.is_sign_negative();
    let p = precision.unwrap_or(6).max(1);
    // scientific rounding of the absolute value with p-1 frac digits
    let sci = render_e(v.abs(), p - 1);
    // parse "d.dddde±XX" back into digit string + exponent
    let (mant, exp_s) = sci.split_once('e').expect("render_e shape");
    let exp: i64 = exp_s.parse().expect("render_e exponent");
    let digits: String = mant.chars().filter(|ch| *ch != '.').collect();
    let mut s = String::new();
    if exp < -4 || exp >= p as i64 {
        s.push_str(&sci);
    } else if exp >= 0 {
        let e = exp as usize; // < p <= digits.len() here
        let (ip, fp) = digits.split_at(e + 1);
        s.push_str(ip);
        if !fp.is_empty() {
            s.push('.');
            s.push_str(fp);
        }
    } else {
        s.push_str("0.");
        for _ in 0..(-exp - 1) {
            s.push('0');
        }
        s.push_str(&digits);
    }
    if neg {
        s.insert(0, '-');
    }
    s
}
/// ONE hex-nibble rounding of a hex digit string, HALF_UP on the >= '8'
/// threshold (FmtResidualProbe C13/S8; SProbe4 V1/V10; SProbe5 W7):
/// a dropped nibble >= '8' bumps; '9'+1 -> 'a'; 'f'+1 overflows to '0'
/// with carry; a carry out of the front overflows the whole mantissa —
/// the caller renormalizes ("0x1.F8" rounded to one nibble -> 0x2.0 ->
/// "0x1.0p1"). Returns None on that overflow, else the digit string
/// padded to exactly `keep` digits.
pub(crate) fn round_hex(digits: &str, keep: usize) -> Option<String> {
    let d: Vec<u8> = digits.as_bytes().to_vec();
    let bump = d.get(keep).map(|c| *c >= b'8').unwrap_or(false);
    let mut out: Vec<u8> = d[..keep.min(d.len())].to_vec();
    while out.len() < keep {
        out.push(b'0');
    }
    if !bump {
        return String::from_utf8(out).ok();
    }
    let mut i = out.len();
    loop {
        if i == 0 {
            return None; // 0x1.ff… rounded up -> 0x2.0
        }
        i -= 1;
        match out[i] {
            b'f' => out[i] = b'0',
            b'9' => out[i] = b'a',
            other => out[i] = other + 1,
        }
        if out[i] != b'0' {
            return String::from_utf8(out).ok();
        }
    }
}
/// Hex-float %a rendering (Temurin-pinned: FmtResidualProbe C1-C16,
/// SProbe2 T1-T3, SProbe3 U1-U3/U15, SProbe4 V1-V2/V11, SProbe5 W7):
///   no precision: the exact 13-nibble mantissa with trailing-zero nibbles
///     stripped (>= 1 kept: 1.0 -> "0x1.0p0", 2.5f -> "0x1.4p1");
///     subnormals stay in the 0.x form with exp -1022 ("0x0.8p-1022" for
///     2^-1023, "0x0.0000000000001p-1022" for MIN_VALUE); zero is
///     "0x0.0p0" with -0.0 keeping its sign.
///   precision: round the mantissa to that many hex digits HALF_UP
///     (precision 0 is taken as 1 — "%.0a" of 3.14 -> "0x1.9p1"); the
///     result is PADDED to exactly p digits (%.13a of 1.0 -> thirteen
///     zeros); a rounding carry renormalizes ("%.1a" of 0x1.F8p0 ->
///     "0x1.0p1"); a subnormal RENORMALIZES to the 1.x form with the true
///     exponent ("%.3a" of 2^-1023 -> "0x1.000p-1023").
/// The uppercase variant comes from the unified upper flag (%A of 3.14 ->
/// "0X1.91EB851EB851FP1", %A of Infinity -> "INFINITY").
pub(crate) fn render_a(v: f64, precision: Option<usize>) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-Infinity" } else { "Infinity" }.to_string();
    }
    let neg = v.is_sign_negative();
    let body = |s: String| if neg { format!("-{s}") } else { s };
    let bits = v.abs().to_bits();
    let exp_field = ((bits >> 52) & 0x7ff) as i64;
    let frac = bits & ((1u64 << 52) - 1);
    let nibbles = format!("{:013x}", frac);
    if exp_field == 0 && frac == 0 {
        return match precision {
            None => body("0x0.0p0".to_string()),
            Some(p0) => body(format!("0x0.{}p0", "0".repeat(p0.max(1)))),
        };
    }
    if exp_field > 0 {
        let e = exp_field - 1023;
        match precision {
            None => {
                let st = nibbles.trim_end_matches('0');
                let st = if st.is_empty() { "0" } else { st };
                body(format!("0x1.{st}p{e}"))
            }
            Some(p0) => {
                let p = p0.max(1);
                match round_hex(&nibbles, p) {
                    Some(f) => body(format!("0x1.{f}p{e}")),
                    None => body(format!("0x1.{}p{}", "0".repeat(p), e + 1)),
                }
            }
        }
    } else {
        match precision {
            None => {
                let st = nibbles.trim_end_matches('0');
                let st = if st.is_empty() { "0" } else { st };
                body(format!("0x0.{st}p-1022"))
            }
            Some(p0) => {
                let p = p0.max(1);
                let msb = 63 - frac.leading_zeros() as usize; // frac != 0, < 2^52
                let e_true = msb as i64 - 1074;
                let rest = frac - (1u64 << msb);
                // exact hex fraction of the 1.x form: ceil(msb/4) nibbles,
                // the bottom bits shifted up to a nibble boundary
                let nibcount = msb.div_ceil(4);
                let frac_int = rest << (nibcount * 4 - msb);
                let frac_hex = format!("{:0nibcount$x}", frac_int);
                match round_hex(&frac_hex, p) {
                    Some(f) => body(format!("0x1.{f}p{e_true}")),
                    None => body(format!("0x1.{}p{}", "0".repeat(p), e_true + 1)),
                }
            }
        }
    }
}
/// Integer decimal rendering with the numeric flag set: sign, grouping,
/// zero-pad (JDK: the pad goes AFTER the sign — "%06d" of -42 -> -00042).
pub(crate) fn render_int(
    v: &Value,
    plus: bool,
    space: bool,
    comma: bool,
    paren: bool,
) -> Result<String, Throw> {
    let (neg, mag): (bool, String) = match v {
        Value::Int(i) => (*i < 0, i.unsigned_abs().to_string()),
        Value::Long(l) => (*l < 0, l.unsigned_abs().to_string()),
        _ => {
            return Err(err(
                "java/util/IllegalFormatConversionException",
                &format!("d != {}", arg_class(v)),
            ))
        }
    };
    let mut s = if comma { group(&mag) } else { mag };
    if neg {
        s = if paren {
            format!("({s})")
        } else {
            format!("-{s}")
        };
    } else if plus {
        s = format!("+{s}");
    } else if space {
        s = format!(" {s}");
    }
    Ok(s)
}
