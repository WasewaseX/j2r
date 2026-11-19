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
//! FormatFlagsConversionMismatchException "Conversion = lc, Flags = ,"
//! where lc is the LOWERCASED conversion char;
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

use crate::internal_error;
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
        Value::Void => Err(internal_error("void value in format")),
    }
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
        _ => return Err(internal_error("j2r internal: String.format spec")),
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
        _ => return Err(internal_error("j2r internal: String.format varargs")),
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
        None => return Err(internal_error("j2r internal: PrintStream.printf recv")),
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

// --- scanaislop split: the items below live verbatim in the child modules ---
mod num;
mod pieces;
mod spec;
#[cfg(test)]
#[allow(clippy::approx_constant)]
mod tests;
pub(crate) use num::*;
pub(crate) use pieces::*;
pub(crate) use spec::*;
