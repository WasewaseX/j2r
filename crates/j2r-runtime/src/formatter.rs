//! JDK `Formatter` subset — `String.format(String, Object...)` and
//! `PrintStream.printf(String, Object...)`.
//!
//! Temurin-pinned by corpus/FmtProbe.java (43 rows on 21.0.13). The most
//! important discovery: `%.2f` of 2.675 prints `2.68` — the JDK rounds the
//! SHORTEST decimal representation (FormattedFloatingDecimal, the same
//! shortest-repr `Double.toString` uses) HALF_UP, NOT the binary expansion
//! (a BigDecimal(2.675) = 2.674999... would give 2.67). So the %f/%e paths
//! round the `javafmt::double_to_string` output as a decimal STRING.
//!
//! Supported: %s %S (width/precision/flags), %d (width, 0-pad, +, comma
//! grouping, left, parenthesized negatives), %x %X %o (+ `#`), %f %e %g
//! (width.precision, HALF_UP on the shortest repr), %a (hex float,
//! Temurin-pinned by corpus/FmtResidualProbe.java), %b %B %c %C %h %H,
//! %%, %n, argument indexing (`%2$s`, `%<`). The uppercase conversion
//! letters (S B H C X E G A) are the JDK's uppercase variants: the LOWER
//! case engine renders, then the whole piece is uppercased — that is why
//! `%E` of Infinity is `INFINITY` and `%G` of NaN is `NAN`.
//!
//! Parse-time laws (all FmtResidualProbe-pinned, all fire BEFORE argument
//! resolution): (a) `-` without a width on a numeric conversion is
//! MissingFormatWidthException carrying the raw spec text ("%-d");
//! (b) the `,` flag is legal ONLY on d/f/g — anything else throws
//! FormatFlagsConversionMismatchException "Conversion = <lower>, Flags = ,"
//! (so `%d` of an Integer is still `f != java.lang.Integer`);
//! (c) `#` is illegal on d/g. Known-conversion error messages use the
//! LOWERCASED conversion char (`%E` of an Integer: `e != java.lang.Integer`),
//! while UnknownFormatConversionException keeps the original (`Conversion
//! = 'Q'`). null prints `null` (uppercased by the upper variants) through
//! EVERY conversion except %b (`false`/`FALSE`) — including %d, %c and %h.
//! The float family does NOT widen integral types: `%f` of an Integer is
//! IllegalFormatConversionException `f != java.lang.Integer`.
//! Exact error messages: `Conversion = 'q'`
//! (UnknownFormatConversionException), `d != java.lang.String`
//! (IllegalFormatConversionException), `Format specifier '%d'`
//! (MissingFormatArgumentException).

use crate::object::is_string;
use crate::{heap, object, throw_new, Dispatch, JRef, Throw, Value};

fn err(cls: &str, msg: &str) -> Throw {
    throw_new(cls, Some(msg))
}

/// Renders a value with the default conversions (the `%s` path): null →
/// "null", strings verbatim, else `toString()` dispatch (so a user enum or
/// object formats like on the JVM).
fn to_display(v: &Value, ctx: Dispatch) -> Result<String, Throw> {
    match v {
        Value::Int(i) => Ok(i.to_string()),
        Value::Long(l) => Ok(l.to_string()),
        Value::Float(f) => Ok(crate::javafmt::float_to_string(*f)),
        Value::Double(d) => Ok(crate::javafmt::double_to_string(*d)),
        Value::Str(s) => Ok(s.to_string()),
        Value::Ref(r) => crate::concat::format_ref(r.as_ref(), ctx),
        Value::Void => Err(crate::internal_error("void value in format")),
    }
}

/// `int` family renderers for %x/%X/%o: JDK two's-complement by argument
/// width (Int → u32, Long → u64; `-255` prints `ffffff01` — FmtProbe row).
fn hex_oct(v: &Value, base: u32, upper: bool, hash: bool) -> Result<String, Throw> {
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

fn arg_class(v: &Value) -> String {
    match v {
        Value::Int(_) => "java.lang.Integer".to_string(),
        Value::Long(_) => "java.lang.Long".to_string(),
        Value::Float(_) => "java.lang.Float".to_string(),
        Value::Double(_) => "java.lang.Double".to_string(),
        Value::Str(_) => "java.lang.String".to_string(),
        Value::Ref(Some(o)) => {
            heap::with_obj(*o, |ob| crate::reflect::binary_name(ob.jclass.as_ref()))
        }
        Value::Ref(None) => "java.lang.Null".to_string(),
        Value::Void => "void".to_string(),
    }
}

fn bool_of(v: &Value, ctx: Dispatch) -> Result<bool, Throw> {
    Ok(match v {
        Value::Ref(None) => false,
        _ => to_display(v, ctx)?.as_str() != "false",
    })
}

/// Auto-unboxes a boxed heap object (javac varargs auto-box EVERYTHING, so
/// `%d` receives an `Integer` heap object whose slots[0] holds the int —
/// the unbox_by_desc law). %s/%b must see the ORIGINAL (a Boolean's truth
/// differs from its unboxed int), so render() unboxes per-branch.
fn to_primitive(v: &Value) -> Value {
    match v {
        Value::Ref(Some(r)) => {
            let jc = heap::with_obj(*r, |o| o.jclass.to_string());
            match jc.as_str() {
                "java/lang/Integer"
                | "java/lang/Long"
                | "java/lang/Float"
                | "java/lang/Double"
                | "java/lang/Byte"
                | "java/lang/Short"
                | "java/lang/Character"
                | "java/lang/Boolean" => heap::with_obj(*r, |o| o.slots[0].clone()),
                _ => v.clone(),
            }
        }
        _ => v.clone(),
    }
}

/// Widens a floating arg to f64 for %f/%e/%g/%a (JDK: Float widens
/// exactly; Double passes through). The JDK does NOT widen integral types
/// — %f of an Integer is IllegalFormatConversionException
/// `f != java.lang.Integer` (FmtResidualProbe U13a-d; %e of a Long is
/// `e != java.lang.Long`), %f of a String is `f != java.lang.String` (V8).
/// The conv char arrives LOWERCASED (the known-conversion message law).
fn as_f64(v: &Value, conv: char) -> Result<f64, Throw> {
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
fn round_digits(digits: &str, round_idx: usize) -> String {
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
fn render_f(v: f64, precision: usize) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-Infinity" } else { "Infinity" }.to_string();
    }
    let neg = v < 0.0 || (v == 0.0 && v.is_sign_negative());
    let repr = crate::javafmt::double_to_string(v.abs());
    // split the shortest repr into digit string + integer-digit count
    let (int_part, frac_part) = match repr.split_once('.') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (repr.clone(), String::new()),
    };
    let (int_part, frac_part) = if int_part.is_empty() {
        ("0".to_string(), frac_part)
    } else {
        (int_part, frac_part)
    };
    let int_len = int_part.len();
    let mut digits = format!("{}{}", int_part, frac_part);
    let round_idx = int_len + precision;
    if digits.len() > round_idx {
        digits = round_digits(&digits, round_idx);
    }
    // re-pad to exactly `precision` frac digits
    while digits.len() < int_len + precision {
        digits.push('0');
    }
    let (mut ip, fp) = digits.split_at(int_len);
    if ip.is_empty() {
        ip = "0";
    }
    // zero precision: NO decimal point (Temurin: %.0f of 2.5 -> "3")
    let mut s = if precision == 0 {
        ip.to_string()
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
fn render_e(v: f64, precision: usize) -> String {
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
fn render_g(v: f64, precision: Option<usize>) -> String {
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
fn round_hex(digits: &str, keep: usize) -> Option<String> {
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
fn render_a(v: f64, precision: Option<usize>) -> String {
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
fn render_int(
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

/// thousands grouping for the integer part only
fn group(digits: &str) -> String {
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let lead = bytes.len() % 3;
    if lead > 0 {
        out.push_str(&digits[..lead]);
        if bytes.len() > lead {
            out.push(',');
        }
    }
    for (k, ch) in digits[lead..].chars().enumerate() {
        out.push(ch);
        if (k + 1) % 3 == 0 && k + 1 < digits.len() - lead {
            out.push(',');
        }
    }
    out
}

/// space-padding to width, left- or right-justify
fn pad(s: &str, width: Option<usize>, left: bool) -> String {
    match width {
        Some(w) if w > s.chars().count() => {
            let fill = " ".repeat(w - s.chars().count());
            if left {
                format!("{s}{fill}")
            } else {
                format!("{fill}{s}")
            }
        }
        _ => s.to_string(),
    }
}

/// zero-padding to width for numeric renders (sign-aware)
fn pad_zero(s: &str, width: Option<usize>) -> String {
    match width {
        Some(w) if w > s.chars().count() => {
            let need = w - s.chars().count();
            let (sign, rest) = if let Some(r) = s.strip_prefix('-') {
                ("-", r)
            } else if let Some(r) = s.strip_prefix('+') {
                ("+", r)
            } else {
                ("", s)
            };
            if sign.is_empty() {
                format!("{}{}", "0".repeat(need), s)
            } else {
                format!("{}{}{}", sign, "0".repeat(need), rest)
            }
        }
        _ => s.to_string(),
    }
}

struct Spec {
    arg_index: Option<usize>,
    reuse: bool,
    left: bool,
    plus: bool,
    space: bool,
    zero: bool,
    comma: bool,
    paren: bool,
    hash: bool,
    width: Option<usize>,
    precision: Option<usize>,
    conv: char,
}

/// Parses one `%…` spec starting AT the '%' (i points at it); returns the
/// parsed spec and the index just past the conversion char.
fn parse_spec(spec: &str, i: usize) -> Result<(Spec, usize), Throw> {
    let bytes = spec.as_bytes();
    let mut j = i + 1;
    if j >= bytes.len() {
        return Err(err(
            "java/util/MissingFormatArgumentException",
            "Format specifier '%'",
        ));
    }
    // argument index (digits followed by '$')
    let mut s = Spec {
        arg_index: None,
        reuse: false,
        left: false,
        plus: false,
        space: false,
        zero: false,
        comma: false,
        paren: false,
        hash: false,
        width: None,
        precision: None,
        conv: '%',
    };
    let start = j;
    while j < bytes.len() && bytes[j].is_ascii_digit() {
        j += 1;
    }
    if j < bytes.len() && bytes[j] == b'$' && j > start {
        s.arg_index = Some(spec[start..j].parse::<usize>().unwrap_or(1));
        j += 1;
    } else if bytes[j] == b'<' {
        s.reuse = true;
        j += 1;
    } else {
        j = start;
    }
    while j < bytes.len() {
        match bytes[j] {
            b'-' => s.left = true,
            b'+' => s.plus = true,
            b' ' => s.space = true,
            b'0' => s.zero = true,
            b',' => s.comma = true,
            b'(' => s.paren = true,
            b'#' => s.hash = true,
            _ => break,
        }
        j += 1;
    }
    let wstart = j;
    while j < bytes.len() && bytes[j].is_ascii_digit() {
        j += 1;
    }
    if j > wstart {
        s.width = Some(spec[wstart..j].parse::<usize>().unwrap_or(0));
    }
    if j < bytes.len() && bytes[j] == b'.' {
        j += 1;
        let pstart = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        s.precision = Some(if j > pstart {
            spec[pstart..j].parse::<usize>().unwrap_or(0)
        } else {
            0
        });
    }
    let conv = spec[j..].chars().next().ok_or_else(|| {
        err(
            "java/util/MissingFormatArgumentException",
            "Format specifier '%'",
        )
    })?;
    s.conv = conv;
    j += conv.len_utf8();
    // JDK parse-time laws (FmtResidualProbe D-loop, B14, U6, V3, W9,
    // X1-X9; all fire BEFORE argument resolution — S4: "%,x" of null
    // throws FFCME, not "null"). They run on the LOWERCASED conversion
    // (the JDK maps the upper variants to the lower engine) and only for
    // KNOWN conversions — an unknown one stays UnknownFormatConversion-
    // Exception with the original char (W5).
    let known = match conv {
        'S' => Some('s'),
        'B' => Some('b'),
        'H' => Some('h'),
        'C' => Some('c'),
        'X' => Some('x'),
        'E' => Some('e'),
        'G' => Some('g'),
        'A' => Some('a'),
        's' | 'b' | 'h' | 'c' | 'd' | 'o' | 'x' | 'e' | 'f' | 'g' | 'a' => Some(conv),
        _ => None,
    };
    if let Some(lc) = known {
        // (a) '-' without a width on a numeric conversion: MissingFormat-
        // WidthException carrying the RAW spec text ("%-d", "%-,x") — it
        // precedes the flag checks ("%-,x" with no width throws MFW).
        if s.left && s.width.is_none() && matches!(lc, 'd' | 'o' | 'x' | 'e' | 'f' | 'g' | 'a') {
            return Err(err("java/util/MissingFormatWidthException", &spec[i..j]));
        }
        // (b) ',' is legal ONLY on d/f/g (X1/X8 legal; D-loop, W9, X2-X6
        // illegal): "Conversion = <lower>, Flags = ,"
        if s.comma && !matches!(lc, 'd' | 'f' | 'g') {
            return Err(err(
                "java/util/FormatFlagsConversionMismatchException",
                &format!("Conversion = {lc}, Flags = ,"),
            ));
        }
        // (c) '#' is illegal on d and g (U6, B14): "Flags = #"
        if s.hash && matches!(lc, 'd' | 'g') {
            return Err(err(
                "java/util/FormatFlagsConversionMismatchException",
                &format!("Conversion = {lc}, Flags = #"),
            ));
        }
    }
    Ok((s, j))
}

/// The engine: `format(spec, args)` with the JDK's argument-resolution law
/// (implicit sequential, `N$` explicit then continuing after it, `<` reuse).
pub fn format(spec: &str, args: &[Value], ctx: Dispatch) -> Result<String, Throw> {
    let mut out = String::new();
    let mut next: usize = 0; // 0-based cursor over args
    let mut last: Option<usize> = None;
    let mut i = 0usize;
    while let Some(c) = spec[i..].chars().next() {
        if c != '%' {
            out.push(c);
            i += c.len_utf8();
            continue;
        }
        let (sp, ni) = parse_spec(spec, i)?;
        i = ni;
        if sp.conv == '%' {
            out.push('%');
            continue;
        }
        if sp.conv == 'n' {
            out.push('\n'); // platform line separator on every j2r target
            continue;
        }
        let idx: usize = if sp.reuse {
            last.ok_or_else(|| {
                err(
                    "java/util/MissingFormatArgumentException",
                    "Format specifier '<'",
                )
            })?
        } else if let Some(n) = sp.arg_index {
            n.saturating_sub(1)
        } else {
            let n = next;
            next += 1;
            n
        };
        last = Some(idx);
        let arg = args.get(idx).ok_or_else(|| {
            err(
                "java/util/MissingFormatArgumentException",
                &format!("Format specifier '%{}'", sp.conv),
            )
        })?;
        let piece = render(&sp, arg, ctx)?;
        out.push_str(&piece);
    }
    Ok(out)
}

fn render(sp: &Spec, arg: &Value, ctx: Dispatch) -> Result<String, Throw> {
    let c = sp.conv;
    let width = sp.width;
    let precision = sp.precision;
    // The uppercase conversion letters are the JDK's uppercase VARIANTS of
    // the lower engine (module doc): render lower, uppercase the whole
    // piece at the end. Unknown conversions keep the original char (W5:
    // %Q -> "Conversion = 'Q'"; %D is NOT a JDK conversion -> UFCE too).
    let (lc, upper) = match c {
        'S' => ('s', true),
        'B' => ('b', true),
        'H' => ('h', true),
        'C' => ('c', true),
        'X' => ('x', true),
        'E' => ('e', true),
        'G' => ('g', true),
        'A' => ('a', true),
        other => (other, false),
    };
    // numeric + char conversions auto-unbox (varargs auto-boxing); %s/%b
    // keep the original value (a boxed Boolean's truth is not its int)
    let prim = to_primitive(arg);
    let arg = match lc {
        's' | 'b' => arg,
        _ => &prim,
    };
    // JDK: null through EVERY conversion except %b prints "null" — pinned
    // for %f/%e/%g/%a/%d/%x/%c (FmtResidualProbe E1-E7) and %h (U4; the
    // old code PANICKED here) — and the upper variants print "NULL"
    // (E8-E10, W10). %b(null) stays "false" (bool_of).
    if matches!(arg, Value::Ref(None)) && lc != 'b' {
        let s = if upper { "NULL" } else { "null" };
        return Ok(pad(s, width, sp.left));
    }
    let piece = match lc {
        's' => {
            let mut s = to_display(arg, ctx)?;
            if let Some(p) = precision {
                s = s.chars().take(p).collect();
            }
            s
        }
        'd' => render_int(arg, sp.plus, sp.space, sp.comma, sp.paren)?,
        'x' | 'o' => {
            let base = if lc == 'o' { 8 } else { 16 };
            // upper=false: the unified tail uppercases the whole piece, so
            // %#X prints "0XFF" (W6) — the old code uppercased the digits
            // but left a lowercase "0x" prefix.
            hex_oct(arg, base, false, sp.hash).map_err(|_| {
                err(
                    "java/util/IllegalFormatConversionException",
                    &format!("{lc} != {}", arg_class(arg)),
                )
            })?
        }
        'f' => {
            let v = as_f64(arg, lc)?;
            let mut s = render_f(v, precision.unwrap_or(6));
            if sp.comma {
                let neg = s.starts_with('-');
                let body = if neg { &s[1..] } else { &s[..] };
                let (ip, fp) = body.split_once('.').unwrap_or((body, ""));
                let mut g = group(ip);
                if !fp.is_empty() {
                    g.push('.');
                    g.push_str(fp);
                }
                s = if neg { format!("-{g}") } else { g };
            }
            s
        }
        'e' => render_e(as_f64(arg, lc)?, precision.unwrap_or(6)),
        'g' => {
            let mut s = render_g(as_f64(arg, lc)?, precision);
            // %,g groups the FIXED-path integer part (X1: %,g of 1234.5 ->
            // "1,234.50"); the scientific branch stays ungrouped (the
            // comma+sci combination is unpinned — no fixture row).
            if sp.comma && !s.contains('e') {
                let neg = s.starts_with('-');
                let body = if neg { &s[1..] } else { &s[..] };
                let (ip, fp) = body.split_once('.').unwrap_or((body, ""));
                let mut g = group(ip);
                if !fp.is_empty() {
                    g.push('.');
                    g.push_str(fp);
                }
                s = if neg { format!("-{g}") } else { g };
            }
            s
        }
        'a' => render_a(as_f64(arg, lc)?, precision),
        'b' => bool_of(arg, ctx)?.to_string(),
        'c' => match arg {
            Value::Int(i) => char::from_u32(*i as u32)
                .ok_or_else(|| err("java/lang/IllegalArgumentException", "bad char"))?
                .to_string(),
            _ => {
                return Err(err(
                    "java/util/IllegalFormatConversionException",
                    &format!("{lc} != {}", arg_class(arg)),
                ))
            }
        },
        'h' => {
            let hash = match arg {
                // strings hash by content (deterministic), objects by the
                // identity hash — the same call the JDK's %h makes
                Value::Str(_) | Value::Ref(_) => {
                    // strings hash by content via the String.hashCode native
                    // (deterministic); other refs dispatch hashCode normally
                    let this_v = if matches!(arg, Value::Str(_)) {
                        // interned-const path: wrap via new_string so the
                        // native sees a heap string object
                        Value::Ref(Some(object::new_string(match arg {
                            Value::Str(s) => s.as_ref(),
                            _ => "",
                        })))
                    } else {
                        arg.clone()
                    };
                    let recv = match &this_v {
                        Value::Ref(Some(r)) => Some(*r),
                        _ => None,
                    };
                    match crate::invoke_virtual(
                        &recv.expect("hash arg"),
                        "java/lang/Object",
                        "hashCode",
                        "()I",
                        &[],
                        ctx,
                    ) {
                        Ok(Value::Int(h)) => h,
                        _ => 0,
                    }
                }
                Value::Int(i) => *i,
                Value::Long(l) => (l ^ (l >> 32)) as i32,
                _ => 0,
            };
            format!("{:x}", hash as u32)
        }
        _ => {
            return Err(err(
                "java/util/UnknownFormatConversionException",
                &format!("Conversion = '{c}'"),
            ))
        }
    };
    // unified tail: the uppercase flag uppercases the WHOLE rendered piece
    // (that is why %E of Infinity is "INFINITY", %G of NaN is "NAN", %A of
    // 3.14 is "0X1.91EB851EB851FP1", %H of "key" is "19E5F"), then the
    // width padding. Zero-pad applies to the numeric conversions only.
    let s = if upper { piece.to_uppercase() } else { piece };
    if sp.zero && !sp.left && matches!(lc, 'd' | 'o' | 'x' | 'f' | 'e' | 'g' | 'a') {
        Ok(pad_zero(&s, width))
    } else {
        Ok(pad(&s, width, sp.left))
    }
}

/// `String.format(String, Object...)`.
pub fn n_string_format(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let spec = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        Some(Value::Ref(Some(o))) if is_string(o) => object::string_value(o).to_string(),
        Some(Value::Ref(None)) => {
            return Err(err("java/lang/NullPointerException", "format == null"))
        }
        _ => return Err(crate::internal_error("j2r internal: String.format spec")),
    };
    // javac packs the varargs into ONE Object[] argument — unpack its
    // elements as the format arguments (the JDK's receive-variargs law).
    let mut rest: Vec<Value> = Vec::new();
    match args.get(1) {
        Some(Value::Ref(Some(arr))) => {
            let len = crate::array::array_length(arr)?;
            for k in 0..len {
                rest.push(crate::array::array_load(arr, k)?);
            }
        }
        Some(Value::Ref(None)) => {}
        None => {}
        _ => return Err(crate::internal_error("j2r internal: String.format varargs")),
    }
    let s = format(&spec, &rest, ctx)?;
    Ok(Value::Ref(Some(object::new_string(&s))))
}

/// `PrintStream.printf(String, Object...)` and the `format` alias: format,
/// print WITHOUT a newline, return the receiver (chained calls).
pub fn n_print_stream_printf(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => {
            return Err(crate::internal_error(
                "j2r internal: PrintStream.printf recv",
            ))
        }
    };
    let formatted = n_string_format(None, args, ctx)?;
    crate::invoke_virtual(
        &r,
        "java/io/PrintStream",
        "print",
        "(Ljava/lang/String;)V",
        &[formatted],
        ctx,
    )?;
    Ok(Value::Ref(Some(r)))
}

#[cfg(test)]
#[allow(clippy::approx_constant)]
mod tests {
    use super::*;

    #[test]
    fn shortest_repr_shapes() {
        let _rt = crate::test_support::runtime();
        println!("2.675  => {}", crate::javafmt::double_to_string(2.675));
        println!("3.14159=> {}", crate::javafmt::double_to_string(3.14159));
        println!(
            "12345.6789 => {}",
            crate::javafmt::double_to_string(12345.6789)
        );
        println!("0.125  => {}", crate::javafmt::double_to_string(0.125));
        println!("%.2f 3.14159 => {}", render_f(3.14159, 2));
        println!("%.2f 2.675  => {}", render_f(2.675, 2));
        println!("%.0f 2.5     => {}", render_f(2.5, 0));
        println!("%.1e 12345.6789 => {}", render_e(12345.6789, 1));
        println!("%e 12345.6789  => {}", render_e(12345.6789, 6));
    }

    #[test]
    fn render_f_pins() {
        let _rt = crate::test_support::runtime();
        assert_eq!(render_f(3.14, 6), "3.140000");
        assert_eq!(render_f(3.14159, 2), "3.14");
        assert_eq!(render_f(2.675, 2), "2.68");
        assert_eq!(render_f(0.125, 2), "0.13");
        assert_eq!(render_f(2.5, 0), "3");
        assert_eq!(render_f(-2.5, 0), "-3");
        assert_eq!(render_f(-0.005, 2), "-0.01");
    }

    #[test]
    fn render_e_pins() {
        let _rt = crate::test_support::runtime();
        assert_eq!(render_e(12345.6789, 6), "1.234568e+04");
        assert_eq!(render_e(12345.6789, 1), "1.2e+04");
        // -0.0 keeps its sign (FmtResidualProbe U9: %E of -0.0 is
        // "-0.000000E+00"; the old `v < 0.0` test dropped it)
        assert_eq!(render_e(-0.0, 6), "-0.000000e+00");
        // rounding carry bumps the exponent (A9: 9.99999999e5 -> 1E+06)
        assert_eq!(render_e(9.99999999e5, 6), "1.000000e+06");
    }

    #[test]
    fn render_g_pins() {
        let _rt = crate::test_support::runtime();
        // FmtResidualProbe B-rows — LOWER-ENGINE level: the renderer emits
        // lowercase; %G's uppercase comes from the unified tail in render()
        // (the format-level law test pins the uppercase shapes)
        assert_eq!(render_g(1234.5678, None), "1234.57");
        assert_eq!(render_g(1e7, None), "1.00000e+07");
        assert_eq!(render_g(1e6, None), "1.00000e+06");
        assert_eq!(render_g(999999.0, None), "999999");
        assert_eq!(render_g(1e-5, None), "1.00000e-05");
        // NO trailing-zero stripping in the fixed branch (B6 — the C-%g
        // stripping law does NOT apply to the JVM)
        assert_eq!(render_g(1e-4, None), "0.000100000");
        assert_eq!(render_g(123.456, Some(2)), "1.2e+02");
        assert_eq!(render_g(12.34, Some(2)), "12");
        assert_eq!(render_g(0.0, None), "0.00000");
        assert_eq!(render_g(-1234.5678, None), "-1234.57");
        // the rounding carry re-decides the branch (S6/S7): 9.99 rounds to
        // 1.0e1 -> fixed "10"; 999.99 rounds to 1.00e3 -> stays scientific
        assert_eq!(render_g(9.99, Some(2)), "10");
        assert_eq!(render_g(999.99, Some(3)), "1.00e+03");
        // precision 0 is taken as 1 (U7); more corners
        assert_eq!(render_g(12345.0, Some(0)), "1e+04");
        assert_eq!(render_g(0.001, None), "0.00100000");
        assert_eq!(render_g(0.5, None), "0.500000");
        assert_eq!(render_g(1.0 / 3.0, Some(10)), "0.3333333333");
        assert_eq!(render_g(-0.0, None), "-0.00000");
        assert_eq!(render_g(123.456, Some(4)), "123.5");
    }

    #[test]
    fn render_a_pins() {
        let _rt = crate::test_support::runtime();
        // FmtResidualProbe C-rows (Temurin 21.0.13, 2026-11-05)
        assert_eq!(render_a(1.0, None), "0x1.0p0");
        assert_eq!(render_a(3.14, None), "0x1.91eb851eb851fp1");
        assert_eq!(render_a(0.0, None), "0x0.0p0");
        assert_eq!(render_a(-0.0, None), "-0x0.0p0");
        assert_eq!(render_a(2.0, None), "0x1.0p1");
        assert_eq!(render_a(0.5, None), "0x1.0p-1");
        // MIN_VALUE: subnormal 0.x form, exp -1022
        assert_eq!(render_a(4.9e-324, None), "0x0.0000000000001p-1022");
        // subnormals strip trailing zero nibbles too (T1: 2^-1023)
        assert_eq!(render_a(f64::from_bits(1), None), "0x0.0000000000001p-1022");
        assert_eq!(render_a(f64::from_bits(1 << 51), None), "0x0.8p-1022");
        // precision: rounds HALF_UP on >= 8 (C13) and pads to exactly p
        // (U2/U3); precision 0 is taken as 1 (V2)
        assert_eq!(render_a(3.14, Some(2)), "0x1.92p1");
        assert_eq!(render_a(3.14, Some(1)), "0x1.9p1");
        assert_eq!(render_a(3.14, Some(0)), "0x1.9p1");
        assert_eq!(render_a(1.0, Some(13)), "0x1.0000000000000p0");
        assert_eq!(render_a(3.14, Some(14)), "0x1.91eb851eb851f0p1");
        // rounding carry renormalizes with e+1 (V1/V10/W7)
        assert_eq!(render_a(1.96875, Some(1)), "0x1.0p1"); // 0x1.F8p0
        assert_eq!(render_a(1.999755859375, Some(2)), "0x1.00p1"); // 0x1.FFFp0
                                                                   // subnormal WITH precision renormalizes to the 1.x true-exp form (T3)
        assert_eq!(render_a(f64::from_bits(1 << 51), Some(3)), "0x1.000p-1023");
    }

    fn fmt(spec: &str, args: &[Value]) -> Result<String, Throw> {
        // the caller HOLDS the test runtime lock — never take it here (a
        // per-call guard drops between fmt() and msg_of(), letting libtest's
        // parallel tests race the heap / STW handshake from a "Parked" thread)
        format(spec, args, crate::Dispatch::default())
    }

    fn msg_of(t: &Throw) -> String {
        // throw_new stores detailMessage as a HEAP STRING ref. Read the
        // slot FIRST (lock released when with_obj returns), THEN follow
        // the ref — object::string_value takes the heap lock itself and
        // the Mutex is not re-entrant (nested with_obj deadlocks).
        let slot = crate::heap::with_obj(t.obj, |o| o.slots[0].clone());
        match slot {
            Value::Str(s) => s.to_string(),
            Value::Ref(Some(r)) => crate::object::string_value(&r).to_string(),
            _ => String::new(),
        }
    }

    fn law(spec: &str, args: &[Value]) -> String {
        msg_of(&fmt(spec, args).unwrap_err())
    }

    #[test]
    fn format_upper_and_law_pins() {
        // ONE guard for the whole test — every fmt()/law() call below needs
        // the heap; dropping the lock between calls deadlocks the STW
        // handshake under libtest's parallel test threads
        let _rt = crate::test_support::runtime();
        // the uppercase-variant family (A/C/W rows on Temurin 21.0.13):
        // the WHOLE piece uppercases — INFINITY/NAN included
        assert_eq!(
            fmt("%E", &[Value::Double(12345.6789)]).unwrap(),
            "1.234568E+04"
        );
        assert_eq!(
            fmt("%E", &[Value::Double(f64::INFINITY)]).unwrap(),
            "INFINITY"
        );
        assert_eq!(fmt("%E", &[Value::Double(f64::NAN)]).unwrap(), "NAN");
        assert_eq!(fmt("%G", &[Value::Double(1234.5678)]).unwrap(), "1234.57");
        assert_eq!(
            fmt("%A", &[Value::Double(3.14)]).unwrap(),
            "0X1.91EB851EB851FP1"
        );
        assert_eq!(
            fmt("%A", &[Value::Double(f64::INFINITY)]).unwrap(),
            "INFINITY"
        );
        assert_eq!(fmt("%#X", &[Value::Int(255)]).unwrap(), "0XFF");
        assert_eq!(fmt("%B", &[Value::Int(1)]).unwrap(), "TRUE");
        assert_eq!(
            fmt("%H", &[Value::Str(std::sync::Arc::new("key".to_string()))]).unwrap(),
            "19E5F"
        );
        assert_eq!(fmt("%C", &[Value::Int('q' as i32)]).unwrap(), "Q");
        // null through every non-b conversion (E1-E7, U4), uppercased by
        // the upper variants (E8-E10, W10)
        for (spec, want) in [
            ("%f", "null"),
            ("%e", "null"),
            ("%g", "null"),
            ("%a", "null"),
            ("%d", "null"),
            ("%x", "null"),
            ("%c", "null"),
            ("%h", "null"),
            ("%s", "null"),
            ("%E", "NULL"),
            ("%G", "NULL"),
            ("%A", "NULL"),
            ("%S", "NULL"),
        ] {
            assert_eq!(fmt(spec, &[Value::Ref(None)]).unwrap(), want, "{spec}");
        }
        assert_eq!(fmt("%b", &[Value::Ref(None)]).unwrap(), "false");
        assert_eq!(fmt("%B", &[Value::Ref(None)]).unwrap(), "FALSE");
        // the float family does NOT widen integral types (U13a-d) — the
        // message uses the LOWERCASED conv for the upper variants (W8)
        assert_eq!(law("%f", &[Value::Int(42)]), "f != java.lang.Integer");
        assert_eq!(law("%e", &[Value::Long(42)]), "e != java.lang.Long");
        assert_eq!(law("%E", &[Value::Int(42)]), "e != java.lang.Integer");
        assert_eq!(law("%d", &[Value::Double(3.14)]), "d != java.lang.Double");
        // unknown conversions keep the original char (W5)
        assert_eq!(law("%Q", &[Value::Int(1)]), "Conversion = 'Q'");
        // parse-time flag laws, BEFORE argument resolution (S4)
        assert_eq!(law("%,x", &[Value::Int(255)]), "Conversion = x, Flags = ,");
        assert_eq!(law("%,x", &[Value::Ref(None)]), "Conversion = x, Flags = ,");
        assert_eq!(
            law("%,e", &[Value::Double(1.5)]),
            "Conversion = e, Flags = ,"
        );
        assert_eq!(law("%#d", &[Value::Int(5)]), "Conversion = d, Flags = #");
        assert_eq!(
            law("%#G", &[Value::Double(1.0)]),
            "Conversion = g, Flags = #"
        );
        // '-' without a width on a numeric conversion: MFW with the raw
        // spec (V3/U5) — before the flag check ("%-,x" has no width)
        assert_eq!(law("%-d", &[Value::Int(5)]), "%-d");
        // comma stays legal on d/f/g (X1/X7/F-rows)
        assert_eq!(
            fmt("%,f", &[Value::Double(1234567.891)]).unwrap(),
            "1,234,567.891000"
        );
        assert_eq!(fmt("%,g", &[Value::Double(1234.5)]).unwrap(), "1,234.50");
        assert_eq!(fmt("%,(d", &[Value::Int(-1234)]).unwrap(), "(1,234)");
        // %E zero-pad after the sign (A8)
        assert_eq!(
            fmt("%012.2E", &[Value::Double(-std::f64::consts::PI)]).unwrap(),
            "-0003.14E+00"
        );
    }
}
