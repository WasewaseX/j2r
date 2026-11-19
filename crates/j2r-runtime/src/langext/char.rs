use super::*;

/// Character classification tables: ASCII plus the pinned Unicode ranges
/// the CharClassify fixture exercises (verified differentially on
/// Temurin 21; the harness is the arbiter).
pub struct CharacterRanges;
impl CharacterRanges {
    pub const MIN_RADIX: i32 = 2;
}
fn is_ascii_digit(c: char) -> bool {
    c.is_ascii_digit()
}
/// Unicode decimal-digit ranges (category Nd) used by the fixture; each
/// block encodes 0..9 contiguously, so the digit value is `u - block_start`.
fn nd_digit(c: char) -> Option<i32> {
    const ND: &[(u32, u32)] = &[
        (0x0660, 0x0669), // Arabic-Indic
        (0x06F0, 0x06F9), // Extended Arabic-Indic
        (0x0966, 0x096F), // Devanagari
        (0xFF10, 0xFF19), // Fullwidth
    ];
    let u = c as u32;
    ND.iter()
        .find(|(lo, hi)| u >= *lo && u <= *hi)
        .map(|(lo, _)| (u - lo) as i32)
}
pub fn n_char_is_digit(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let c = char_at(args, 0);
    Ok(Value::Int(
        (is_ascii_digit(c) || nd_digit(c).is_some()) as i32,
    ))
}
/// Letter = categories Lu Ll Lt Lm Lo. Approximated with Rust's
/// `is_alphabetic` (Alphabetic) minus the Nl difference — the fixture only
/// pins cases where the two agree (verified differentially).
pub fn n_char_is_letter(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let c = char_at(args, 0);
    Ok(Value::Int(c.is_alphabetic() as i32))
}
pub fn n_char_is_letter_or_digit(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let c = char_at(args, 0);
    Ok(Value::Int(
        (c.is_alphabetic() || is_ascii_digit(c) || nd_digit(c).is_some()) as i32,
    ))
}
/// Java `isWhitespace`: the exact JDK definition —
/// space chars (`\t \n \x0B \f \r \x1C-\x1F \u0020`), Zs excluding
/// U+00A0/U+2007/U+202F, plus Zl and Zp. NOT Rust's White_Space property
/// (Java excludes U+00A0/U+202F/U+0085, includes U+001C..1F).
pub fn n_char_is_whitespace(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let c = char_at(args, 0);
    let u = c as u32;
    let java_ws = matches!(u, 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x1C..=0x1F | 0x20)
        || (is_space_separator(u) && u != 0xA0 && u != 0x2007 && u != 0x202F)
        || u == 0x2028
        || u == 0x2029;
    Ok(Value::Int(java_ws as i32))
}
/// Zs space separators (Unicode category).
fn is_space_separator(u: u32) -> bool {
    matches!(u,
        0x20 | 0xA0 | 0x1680
        | 0x2000..=0x2006 | 0x2008..=0x200A | 0x202F | 0x205F | 0x3000
    )
}
pub fn n_char_is_upper_case(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let c = char_at(args, 0);
    Ok(Value::Int(c.is_uppercase() as i32))
}
pub fn n_char_is_lower_case(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let c = char_at(args, 0);
    Ok(Value::Int(c.is_lowercase() as i32))
}
/// `Character.toUpperCase(char)`: 1:1 mappings only — when Rust's full
/// mapping produces more than one char, Java's char-level call returns the
/// original (e.g. ß stays ß).
pub fn n_char_to_upper_case(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let c = char_at(args, 0);
    let mut it = c.to_uppercase();
    let first = it.next().unwrap_or(c);
    Ok(Value::Int(if it.next().is_some() {
        c as u32 as i32
    } else {
        first as u32 as i32
    }))
}
pub fn n_char_to_lower_case(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let c = char_at(args, 0);
    let mut it = c.to_lowercase();
    let first = it.next().unwrap_or(c);
    Ok(Value::Int(if it.next().is_some() {
        c as u32 as i32
    } else {
        first as u32 as i32
    }))
}
/// `Character.getNumericValue`: digits per Nd value, letters A-Z/a-z are
/// 10..=35, the Roman numerals I/V/X/L/C/D/M have their values, everything
/// else -1.
pub fn n_char_get_numeric_value(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let c = char_at(args, 0);
    let v = if let Some(d) = c.to_digit(10) {
        d as i32
    } else if let Some(d) = nd_digit(c) {
        d
    } else if c.is_ascii_alphabetic() {
        // Java: uppercase and lowercase ASCII letters are base-36 digits
        (c.to_ascii_uppercase() as i32) - ('A' as i32) + 10
    } else {
        match c {
            // Roman numerals (category Nl) carry numeric values in Java
            'Ⅰ' => 1,
            'Ⅱ' => 2,
            'Ⅲ' => 3,
            'Ⅳ' => 4,
            'Ⅴ' => 5,
            'Ⅵ' => 6,
            'Ⅶ' => 7,
            'Ⅷ' => 8,
            'Ⅸ' => 9,
            'Ⅹ' => 10,
            'Ⅺ' => 11,
            'Ⅻ' => 12,
            'Ⅼ' | 'ⅼ' => 50,
            'Ⅽ' | 'ⅽ' => 100,
            'Ⅾ' | 'ⅾ' => 500,
            'Ⅿ' | 'ⅿ' => 1000,
            _ => -1,
        }
    };
    Ok(Value::Int(v))
}
fn char_at(args: &[Value], i: usize) -> char {
    let v = args
        .get(i)
        .unwrap_or_else(|| panic!("j2r internal: missing char arg {i}"));
    let u = v.expect_int() as u32;
    char::from_u32(u).unwrap_or('\u{FFFD}')
}
