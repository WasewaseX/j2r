use super::*;

pub(crate) fn render(sp: &Spec, arg: &Value, ctx: Dispatch) -> Result<String, Throw> {
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
        's' => render_s_piece(sp, arg, lc, precision, ctx)?,
        'd' | 'x' | 'o' => render_int_piece(sp, arg, lc, precision, ctx)?,
        'f' => render_f_piece(sp, arg, lc, precision, ctx)?,
        'e' | 'g' | 'a' => render_float_piece(sp, arg, lc, precision, ctx)?,
        'b' | 'c' => render_bool_char_piece(sp, arg, lc, precision, ctx)?,
        'h' => render_h_piece(sp, arg, lc, precision, ctx)?,
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
/// The %s piece (verbatim from `render`, round-6 reshaping): the
/// display truncation at the precision.
pub(crate) fn render_s_piece(
    _sp: &Spec,
    arg: &Value,
    lc: char,
    precision: Option<usize>,
    ctx: Dispatch,
) -> Result<String, Throw> {
    Ok(match lc {
        's' => {
            let mut s = to_display(arg, ctx)?;
            if let Some(p) = precision {
                s = s.chars().take(p).collect();
            }
            s
        }
        _ => unreachable!("j2r internal: not a render_s_piece conversion"),
    })
}
/// The integer and radix pieces (verbatim): %d's sign/parens/comma
/// engine and the %x/%o radix with the unified-tail uppercase law.
pub(crate) fn render_int_piece(
    sp: &Spec,
    arg: &Value,
    lc: char,
    _precision: Option<usize>,
    _ctx: Dispatch,
) -> Result<String, Throw> {
    Ok(match lc {
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
        _ => unreachable!("j2r internal: not a render_int_piece conversion"),
    })
}
/// The %f piece (verbatim): the fixed rendering with the %,g-style
/// grouping of the fixed-path integer part.
pub(crate) fn render_f_piece(
    sp: &Spec,
    arg: &Value,
    lc: char,
    precision: Option<usize>,
    _ctx: Dispatch,
) -> Result<String, Throw> {
    Ok(match lc {
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
        _ => unreachable!("j2r internal: not a render_f_piece conversion"),
    })
}
/// The %e/%g/%a pieces (verbatim): scientific, general (with the
/// fixed-path grouping law) and hex-float.
pub(crate) fn render_float_piece(
    sp: &Spec,
    arg: &Value,
    lc: char,
    precision: Option<usize>,
    _ctx: Dispatch,
) -> Result<String, Throw> {
    Ok(match lc {
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
        _ => unreachable!("j2r internal: not a render_float_piece conversion"),
    })
}
/// The %b/%c pieces (verbatim): bool_of and the char law with the
/// IllegalFormatConversionException shape.
pub(crate) fn render_bool_char_piece(
    _sp: &Spec,
    arg: &Value,
    lc: char,
    _precision: Option<usize>,
    ctx: Dispatch,
) -> Result<String, Throw> {
    Ok(match lc {
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
        _ => unreachable!("j2r internal: not a render_bool_char_piece conversion"),
    })
}
/// The %h piece (verbatim): content hash for strings (via the
/// String.hashCode native), identity hash for refs, the prim folds.
pub(crate) fn render_h_piece(
    _sp: &Spec,
    arg: &Value,
    lc: char,
    _precision: Option<usize>,
    ctx: Dispatch,
) -> Result<String, Throw> {
    Ok(match lc {
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
        _ => unreachable!("j2r internal: not a render_h_piece conversion"),
    })
}
