use super::*;

pub(crate) struct Spec {
    pub(crate) arg_index: Option<usize>,
    pub(crate) reuse: bool,
    pub(crate) left: bool,
    pub(crate) plus: bool,
    pub(crate) space: bool,
    pub(crate) zero: bool,
    pub(crate) comma: bool,
    pub(crate) paren: bool,
    pub(crate) hash: bool,
    pub(crate) width: Option<usize>,
    pub(crate) precision: Option<usize>,
    pub(crate) conv: char,
}
/// Parses one `%…` spec starting AT the '%' (i points at it); returns the
/// parsed spec and the index just past the conversion char.
pub(crate) fn parse_spec(spec: &str, i: usize) -> Result<(Spec, usize), Throw> {
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
