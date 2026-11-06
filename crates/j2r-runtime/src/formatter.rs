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
//! grouping, left, parenthesized negatives), %x %X %o (+ `#`), %f %e
//! (width.precision, HALF_UP on the shortest repr), %b %c %h %%, %n,
//! argument indexing (`%2$s`, `%<`). Exact error messages:
//! `Conversion = 'q'` (UnknownFormatConversionException),
//! `d != java.lang.String` (IllegalFormatConversionException),
//! `Format specifier '%d'` (MissingFormatArgumentException).

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

/// Widens a numeric arg to f64 for %f/%e (JDK widens Int/Long/Float; a
/// non-numeric arg is IllegalFormatConversionException).
fn as_f64(v: &Value, conv: char) -> Result<Option<f64>, Throw> {
    Ok(match v {
        Value::Int(i) => Some(*i as f64),
        Value::Long(l) => Some(*l as f64),
        Value::Float(f) => Some(*f as f64),
        Value::Double(d) => Some(*d),
        Value::Ref(None) => None, // JDK: (null).doubleValue? not pinned; NaN-ish
        _ => {
            return Err(err(
                "java/util/IllegalFormatConversionException",
                &format!("{conv} != {}", arg_class(v)),
            ))
        }
    })
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
    let neg = v < 0.0;
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
    let total_exp = dec_exp + int_len - 1;
    let mut digits = format!("{}{}", int_part, frac_part);
    // keep precision+1 significant digits, HALF_UP
    let keep = precision + 1;
    if digits.len() > keep {
        digits = round_digits(&digits, keep);
    }
    while digits.len() < keep {
        digits.push('0');
    }
    // a rounding carry may have grown the digit string by one
    let mut total_exp = total_exp;
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
    // numeric + char conversions auto-unbox (varargs auto-boxing); %s/%b
    // keep the original value (a boxed Boolean's truth is not its int)
    let prim = to_primitive(arg);
    let arg = match c {
        's' | 'S' | 'b' => arg,
        _ => &prim,
    };
    match c {
        's' | 'S' => {
            let mut s = to_display(arg, ctx)?;
            if let Some(p) = precision {
                s = s.chars().take(p).collect();
            }
            if c == 'S' {
                s = s.to_uppercase();
            }
            Ok(pad(&s, width, sp.left))
        }
        'd' => {
            let s = render_int(arg, sp.plus, sp.space, sp.comma, sp.paren)?;
            if sp.zero && !sp.left {
                Ok(pad_zero(&s, width))
            } else {
                Ok(pad(&s, width, sp.left))
            }
        }
        'x' | 'X' | 'o' => {
            if matches!(arg, Value::Ref(None)) {
                return Ok(pad("null", width, sp.left));
            }
            let base = if c == 'o' { 8 } else { 16 };
            let mut s = hex_oct(arg, base, c == 'X', sp.hash).map_err(|_| {
                err(
                    "java/util/IllegalFormatConversionException",
                    &format!("{c} != {}", arg_class(arg)),
                )
            })?;
            if sp.comma {
                // JDK supports %,x grouping; not pinned by the probe — apply
                // plain grouping for the common case, no-op for short values
                s = group(&s);
            }
            if sp.zero && !sp.left {
                Ok(pad_zero(&s, width))
            } else {
                Ok(pad(&s, width, sp.left))
            }
        }
        'f' => {
            let v = as_f64(arg, 'f')?;
            let Some(v) = v else {
                return Ok(pad("null", width, sp.left));
            };
            let p = precision.unwrap_or(6);
            let mut s = render_f(v, p);
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
            if sp.zero && !sp.left {
                Ok(pad_zero(&s, width))
            } else {
                Ok(pad(&s, width, sp.left))
            }
        }
        'e' => {
            let v = as_f64(arg, 'e')?;
            let Some(v) = v else {
                return Ok(pad("null", width, sp.left));
            };
            let p = precision.unwrap_or(6);
            let s = render_e(v, p);
            if sp.zero && !sp.left {
                Ok(pad_zero(&s, width))
            } else {
                Ok(pad(&s, width, sp.left))
            }
        }
        'b' => {
            let s = bool_of(arg, ctx)?.to_string();
            Ok(pad(&s, width, sp.left))
        }
        'c' => {
            let ch = match arg {
                Value::Int(i) => char::from_u32(*i as u32)
                    .ok_or_else(|| err("java/lang/IllegalArgumentException", "bad char"))?,
                _ => {
                    return Err(err(
                        "java/util/IllegalFormatConversionException",
                        &format!("c != {}", arg_class(arg)),
                    ))
                }
            };
            Ok(pad(&ch.to_string(), width, sp.left))
        }
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
            let s = format!("{:x}", hash as u32);
            Ok(pad(&s, width, sp.left))
        }
        _ => Err(err(
            "java/util/UnknownFormatConversionException",
            &format!("Conversion = '{c}'"),
        )),
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
    }
}
