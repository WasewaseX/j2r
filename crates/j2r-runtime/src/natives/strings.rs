use super::*;

pub(crate) fn n_string_init(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(String::new())));
    Ok(Value::Void)
}
/// `String(char[])` / `String(char[], off, len)` (corpus wave 1): build
/// from UTF-16 code units — the same unit convention as getChars.
fn string_from_char_range(args: &[Value], op: &str) -> Result<String, Throw> {
    let (cbuf, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(a))), Some(o), Some(l)) => (*a, o.expect_int(), l.expect_int()),
        (Some(Value::Ref(None)), _, _) => return Err(crate::npe()),
        _ => return Err(internal_error(&format!("j2r internal: {op} args"))),
    };
    let clen = crate::array::array_length(&cbuf)?;
    if off < 0 || len < 0 || off.checked_add(len).is_none_or(|e| e > clen) {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!("offset {}, count {}, length {}", off, len, clen)),
        ));
    }
    let mut units: Vec<u16> = Vec::with_capacity(len as usize);
    for i in 0..len {
        match crate::array::array_load(&cbuf, off + i)? {
            Value::Int(c) => units.push(c as u16),
            other => {
                return Err(internal_error(&format!(
                    "j2r internal: char[] element {other:?}"
                )))
            }
        }
    }
    Ok(String::from_utf16_lossy(&units))
}
pub(crate) fn n_string_init_chars(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    let cbuf = first_arg_ref!(
        args,
        Err(internal_error("String.<init>([C) needs a char array"))
    );
    let len = crate::array::array_length(&cbuf)?;
    let text = string_from_char_range(
        &[Value::Ref(Some(cbuf)), Value::Int(0), Value::Int(len)],
        "String.<init>([C)",
    )?;
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}
pub(crate) fn n_string_init_chars_range(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    let text = string_from_char_range(args, "String.<init>([CII)")?;
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}
pub(crate) fn n_string_init_s(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    let o = match arg(args, "String.<init>") {
        Value::Ref(Some(o)) if is_string(o) => o,
        _ => return Err(internal_error("String.<init>(String) needs a String")),
    };
    let text = object::string_value(o).to_string();
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}
pub(crate) fn n_string_length(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.length");
    Ok(Value::Int(
        utf16_units(&object::string_value(r)).len() as i32
    ))
}
/// Java `String.toString()` returns `this` — registered so dispatched
/// `toString()` calls (collections, concat, `Objects.toString`) reach the
/// string itself instead of falling through to `Object.toString`.
pub fn n_string_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(*this(recv, "String.toString"))))
}
pub(crate) fn n_string_is_empty(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.isEmpty");
    Ok(Value::Int(object::string_value(r).is_empty() as i32))
}
pub(crate) fn n_string_char_at(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.charAt");
    let idx = arg(args, "String.charAt").expect_int();
    let units = utf16_units(&object::string_value(r));
    let len = units.len() as i32;
    if idx < 0 || idx >= len {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!("Index {idx} out of bounds for length {len}")),
        ));
    }
    Ok(Value::Int(units[idx as usize] as i32))
}
pub(crate) fn n_string_equals(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.equals");
    let eq = match arg(args, "String.equals") {
        Value::Ref(Some(o)) if is_string(o) => *object::string_value(r) == *object::string_value(o),
        _ => false,
    };
    Ok(Value::Int(eq as i32))
}
/// `String.getChars(int, int, char[], int)` (corpus wave 1): copy the
/// UTF-16 code units of the range into the destination char[]. Indices are
/// UTF-16 units (the string model's documented convention); the JDK's
/// StringIndexOutOfBoundsException message is reproduced.
pub(crate) fn n_string_get_chars(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.getChars");
    let (begin, end, dst, dst_begin) = match (args.first(), args.get(1), args.get(2), args.get(3)) {
        (
            Some(Value::Int(b)),
            Some(Value::Int(e)),
            Some(Value::Ref(Some(d))),
            Some(Value::Int(db)),
        ) => (*b, *e, *d, *db),
        (Some(Value::Ref(None)), _, _, _) | (_, _, Some(Value::Ref(None)), _) => {
            return Err(crate::npe())
        }
        _ => return Err(internal_error("j2r internal: String.getChars args")),
    };
    let units: Vec<u16> = object::string_value(r).encode_utf16().collect();
    if begin < 0 || end > units.len() as i32 || begin > end {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!(
                "begin {}, end {}, length {}",
                begin,
                end,
                units.len()
            )),
        ));
    }
    for i in begin..end {
        crate::array::array_store(
            &dst,
            dst_begin + (i - begin),
            Value::Int(units[i as usize] as i32),
            ctx,
        )?;
    }
    Ok(Value::Void)
}

// ---------------------------------------- String <-> byte[] (JZlib corpus)
//
// The byte-side String natives (corpus wave 2): the everyday encoders with
// JDK-exact contracts, pinned by probes on Temurin 21.0.13 (CharsetProbe /
// RangeProbe): unknown charset name -> UnsupportedEncodingException whose
// message is the name AS GIVEN; null name -> NullPointerException with
// message "null"; non-representable chars encode as '?' for Latin-1 and
// ASCII; malformed UTF-8 decodes to one U+FFFD per maximal bad subpart;
// ASCII decode of a high byte yields U+FFFD; range overflows throw
// StringIndexOutOfBoundsException "Range [off, off+len) out of bounds for
// length N"; names are case-insensitive ("utf-8" works).
//
// Registered divergence (loud, documented): the standard charset provider
// registry BEYOND the UTF-16 family / UTF-8 / Latin-1 / ASCII (UTF-32,
// KOI8_R, ... — JDK-supported names that SUCCEED on Temurin) still throws
// UnsupportedEncodingException here. The UTF-16 family landed with the
// skip/charset wave, pinned by corpus/Utf16Probe.java on Temurin 21.0.13.
#[derive(Clone, Copy)]
enum BytesCs {
    Utf8,
    Latin1,
    Ascii,
    Utf16,
    Utf16Be,
    Utf16Le,
}
fn resolve_charset(v: &Value, who: &str) -> Result<BytesCs, Throw> {
    let name = match v {
        Value::Str(s) => s.to_string(),
        Value::Ref(Some(o)) if is_string(o) => object::string_value(o).to_string(),
        Value::Ref(None) => return Err(throw_new("java/lang/NullPointerException", Some("null"))),
        _ => return Err(internal_error(&format!("j2r internal: {who} charset name"))),
    };
    let norm: String = name.chars().map(|c| c.to_ascii_lowercase()).collect();
    match norm.as_str() {
        "utf-8" | "utf8" => Ok(BytesCs::Utf8),
        "iso-8859-1" | "iso8859-1" | "iso_8859_1" | "8859_1" | "latin1" | "l1" => {
            Ok(BytesCs::Latin1)
        }
        "us-ascii" | "ascii" | "ansi_x3.4-1968" | "iso646-us" | "646" => Ok(BytesCs::Ascii),
        // UTF-16 family (skip/charset wave, corpus/Utf16Probe.java): the
        // JDK canonical names plus the common Charset aliases.
        "utf-16" | "utf16" | "unicode" | "unicodebig" | "utf_16" => Ok(BytesCs::Utf16),
        "utf-16be" | "unicodebigunmarked" | "utf_16be" => Ok(BytesCs::Utf16Be),
        "utf-16le" | "unicodelittleunmarked" | "utf_16le" => Ok(BytesCs::Utf16Le),
        _ => Err(throw_new(
            "java/io/UnsupportedEncodingException",
            Some(&name),
        )),
    }
}
fn encode_cs(text: &str, cs: BytesCs) -> Vec<u8> {
    match cs {
        BytesCs::Utf8 => text.as_bytes().to_vec(),
        BytesCs::Latin1 => text
            .chars()
            .map(|c| if (c as u32) <= 0xFF { c as u8 } else { b'?' })
            .collect(),
        BytesCs::Ascii => text
            .chars()
            .map(|c| if (c as u32) <= 0x7F { c as u8 } else { b'?' })
            .collect(),
        BytesCs::Utf16 => {
            // BOM (FE FF) + big-endian units (Temurin: "A" -> feff0041).
            let mut out = vec![0xFE, 0xFF];
            out.extend(utf16_bytes(text, true));
            out
        }
        BytesCs::Utf16Be => utf16_bytes(text, true),
        BytesCs::Utf16Le => utf16_bytes(text, false),
    }
}
/// UTF-16 code units, byte-expanded. Astral chars split into the standard
/// surrogate pair; the j2r string model carries U+FFFD where the JDK has an
/// unpaired surrogate (the classfile loader normalizes), and Temurin's
/// encoder also emits U+FFFD for one — the bytes agree (Utf16Probe pins
/// `\uD834` -> `fffd`).
fn utf16_bytes(text: &str, big_endian: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    let push_unit = |u: u16, out: &mut Vec<u8>| {
        let [hi, lo] = u.to_be_bytes();
        if big_endian {
            out.push(hi);
            out.push(lo);
        } else {
            out.push(lo);
            out.push(hi);
        }
    };
    for c in text.chars() {
        let u = c as u32;
        if u > 0xFFFF {
            let v = u - 0x10000;
            push_unit(0xD800 + (v >> 10) as u16, &mut out);
            push_unit(0xDC00 + (v & 0x3FF) as u16, &mut out);
        } else {
            push_unit(u as u16, &mut out);
        }
    }
    out
}
/// Decode UTF-16 units from bytes; a dangling trailing byte decodes as one
/// U+FFFD (Temurin-pinned: {00 41 00} -> charAt(1) == 65533), an unpaired
/// surrogate unit also yields U+FFFD (JDK substitution).
fn utf16_decode(bytes: &[u8], big_endian: bool) -> String {
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i + 1 < bytes.len() {
        let u = if big_endian {
            ((bytes[i] as u16) << 8) | bytes[i + 1] as u16
        } else {
            ((bytes[i + 1] as u16) << 8) | bytes[i] as u16
        };
        units.push(u);
        i += 2;
    }
    if i < bytes.len() {
        // dangling byte
        units.push(0xFFFD);
    }
    let mut chars: Vec<char> = Vec::with_capacity(units.len());
    let mut k = 0;
    while k < units.len() {
        let u = units[k];
        if (0xD800..0xDC00).contains(&u) {
            if k + 1 < units.len() && (0xDC00..0xE000).contains(&units[k + 1]) {
                let c = 0x10000 + (((u - 0xD800) as u32) << 10) + (units[k + 1] - 0xDC00) as u32;
                chars.push(char::from_u32(c).unwrap_or(char::REPLACEMENT_CHARACTER));
                k += 2;
            } else {
                chars.push(char::REPLACEMENT_CHARACTER);
                k += 1;
            }
        } else if (0xDC00..0xE000).contains(&u) {
            chars.push(char::REPLACEMENT_CHARACTER);
            k += 1;
        } else {
            chars.push(char::from_u32(u as u32).unwrap_or(char::REPLACEMENT_CHARACTER));
            k += 1;
        }
    }
    chars.into_iter().collect()
}
fn decode_cs(bytes: &[u8], cs: BytesCs) -> String {
    match cs {
        BytesCs::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        BytesCs::Latin1 => bytes.iter().map(|&b| b as char).collect(),
        BytesCs::Ascii => bytes
            .iter()
            .map(|&b| {
                if b <= 0x7F {
                    b as char
                } else {
                    char::REPLACEMENT_CHARACTER
                }
            })
            .collect(),
        BytesCs::Utf16 => {
            // BOM detection (Temurin-pinned): FE FF -> BE, FF FE -> LE,
            // both consumed; NO BOM defaults to big-endian (bytes {41 00}
            // decode as U+4100, NOT "A").
            if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
                utf16_decode(&bytes[2..], true)
            } else if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
                utf16_decode(&bytes[2..], false)
            } else {
                utf16_decode(bytes, true)
            }
        }
        BytesCs::Utf16Be => utf16_decode(bytes, true),
        BytesCs::Utf16Le => utf16_decode(bytes, false),
    }
}
fn bytes_out(bytes: &[u8], ctx: Dispatch) -> Result<Value, Throw> {
    let out = crate::array::array_new_prim("[B", bytes.len() as i32)?;
    for (i, b) in bytes.iter().enumerate() {
        crate::array::array_store(&out, i as i32, Value::Int(*b as i8 as i32), ctx)?;
    }
    Ok(Value::Ref(Some(out)))
}
fn bytes_in(r: &JRef, who: &str) -> Result<Vec<u8>, Throw> {
    let len = crate::array::array_length(r)?;
    let mut bytes = Vec::with_capacity(len as usize);
    for i in 0..len {
        match crate::array::array_load(r, i)? {
            Value::Int(v) => bytes.push(v as u8),
            _ => return Err(internal_error(&format!("j2r internal: {who} byte array"))),
        }
    }
    Ok(bytes)
}
pub(crate) fn n_string_get_bytes_named(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.getBytes");
    let name = arg(args, "String.getBytes");
    let cs = resolve_charset(name, "String.getBytes")?;
    let bytes = encode_cs(&object::string_value(r), cs);
    bytes_out(&bytes, ctx)
}
pub(crate) fn n_string_get_bytes_default(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.getBytes");
    // The platform default charset is UTF-8 (the JVM's file.encoding on
    // every platform j2r targets; Temurin 21 on Linux pins it too).
    let bytes = encode_cs(&object::string_value(r), BytesCs::Utf8);
    bytes_out(&bytes, ctx)
}
pub(crate) fn n_string_init_bytes_named(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    let buf = first_arg_ref!(
        args,
        Err(internal_error(
            "String.<init>([B,String) needs a byte array"
        ))
    );
    let name = args
        .get(1)
        .ok_or_else(|| internal_error("j2r internal: String.<init> missing charset name"))?;
    let cs = resolve_charset(name, "String.<init>")?;
    let bytes = bytes_in(&buf, "String.<init>")?;
    let text = decode_cs(&bytes, cs);
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}
pub(crate) fn n_string_init_bytes_named_range(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    let buf = first_arg_ref!(
        args,
        Err(internal_error(
            "String.<init>([BII,String) needs a byte array"
        ))
    );
    let (off, len) = match (args.get(1), args.get(2)) {
        (Some(Value::Int(o)), Some(Value::Int(l))) => (*o, *l),
        _ => {
            return Err(internal_error(
                "j2r internal: String.<init>([BII,String) args",
            ))
        }
    };
    let total = crate::array::array_length(&buf)?;
    if off < 0 || len < 0 || off > total || len > total - off {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!(
                "Range [{off}, {off} + {len}) out of bounds for length {total}"
            )),
        ));
    }
    let name = args
        .get(3)
        .ok_or_else(|| internal_error("j2r internal: String.<init> missing charset name"))?;
    let cs = resolve_charset(name, "String.<init>")?;
    let mut bytes = Vec::with_capacity(len as usize);
    for i in off..off + len {
        match crate::array::array_load(&buf, i)? {
            Value::Int(v) => bytes.push(v as u8),
            _ => {
                return Err(internal_error(
                    "j2r internal: String.<init> byte array element",
                ))
            }
        }
    }
    let text = decode_cs(&bytes, cs);
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}
pub(crate) fn n_string_hash_code(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.hashCode");
    let mut h: i32 = 0;
    for u in object::string_value(r).encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(u as i32);
    }
    Ok(Value::Int(h))
}
pub(crate) fn n_string_value_of_i(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &arg(args, "String.valueOf").expect_int().to_string(),
    ))))
}
pub(crate) fn n_string_value_of_j(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &arg(args, "String.valueOf").expect_long().to_string(),
    ))))
}
pub(crate) fn n_string_value_of_c(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, "String.valueOf").expect_int();
    let c = char::from_u32(v as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
    Ok(Value::Ref(Some(object::new_string(&c.to_string()))))
}
pub(crate) fn n_string_value_of_z(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, "String.valueOf").expect_int();
    Ok(Value::Ref(Some(object::new_string(if v != 0 {
        "true"
    } else {
        "false"
    }))))
}
pub(crate) fn n_string_value_of_f(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &javafmt::float_to_string(arg(args, "String.valueOf").expect_float()),
    ))))
}
pub(crate) fn n_string_value_of_d(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &javafmt::double_to_string(arg(args, "String.valueOf").expect_double()),
    ))))
}
pub(crate) fn n_string_value_of_l(
    _r: Option<&JRef>,
    args: &[Value],
    c: Dispatch,
) -> Result<Value, Throw> {
    let text = concat::format_value(arg(args, "String.valueOf"), 'L', c)?;
    Ok(Value::Ref(Some(object::new_string(&text))))
}

// ---------------------------------------------------------------- StringBuilder
fn sb_text(r: &JRef) -> Arc<String> {
    crate::heap::with_obj(*r, |b| match &b.slots[0] {
        Value::Str(s) => s.clone(),
        other => panic!("j2r internal: StringBuilder slot0 is {other:?}"),
    })
}
fn sb_push_str(r: &JRef, text: &str) {
    crate::heap::with_obj_mut(*r, |b| match &mut b.slots[0] {
        Value::Str(s) => Arc::make_mut(s).push_str(text),
        other => panic!("j2r internal: StringBuilder slot0 is {other:?}"),
    })
}
pub(crate) fn n_sb_init(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.<init>");
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(String::new())));
    Ok(Value::Void)
}
pub(crate) fn n_sb_init_s(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.<init>");
    let o = match arg(args, "StringBuilder.<init>") {
        Value::Ref(Some(o)) if is_string(o) => o,
        _ => {
            return Err(internal_error(
                "StringBuilder.<init>(String) needs a String",
            ))
        }
    };
    let text = object::string_value(o).to_string();
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}

macro_rules! sb_append {
    ($name:ident, $kind:expr) => {
        pub fn $name(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
            let r = this(recv, "StringBuilder.append");
            let text = concat::format_value(arg(args, "StringBuilder.append"), $kind, ctx)?;
            sb_push_str(r, &text);
            Ok(Value::Ref(Some(r.clone())))
        }
    };
}

sb_append!(n_sb_append_z, 'Z');
sb_append!(n_sb_append_c, 'C');
sb_append!(n_sb_append_i, 'I');
sb_append!(n_sb_append_j, 'J');
sb_append!(n_sb_append_f, 'F');
sb_append!(n_sb_append_d, 'D');
sb_append!(n_sb_append_s, 'L');
sb_append!(n_sb_append_l, 'L');
/// `StringBuilder.append(char[], int, int)` / `append(char[])` (corpus
/// wave 1): copy UTF-16 units from the array into the builder.
pub(crate) fn sb_append_char_range(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.append");
    let (cbuf, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(a))), Some(o), Some(l)) => (*a, o.expect_int(), l.expect_int()),
        (Some(Value::Ref(None)), _, _) => return Err(crate::npe()),
        _ => return Err(internal_error("j2r internal: StringBuilder.append args")),
    };
    let clen = crate::array::array_length(&cbuf)?;
    if off < 0 || len < 0 || off.checked_add(len).is_none_or(|e| e > clen) {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!("offset {}, count {}, length {}", off, len, clen)),
        ));
    }
    let mut units: Vec<u16> = Vec::with_capacity(len as usize);
    for i in 0..len {
        match crate::array::array_load(&cbuf, off + i)? {
            Value::Int(c) => units.push(c as u16),
            other => {
                return Err(internal_error(&format!(
                    "j2r internal: char[] element {other:?}"
                )))
            }
        }
    }
    sb_push_str(r, &String::from_utf16_lossy(&units));
    Ok(Value::Ref(Some(*r)))
}
pub(crate) fn sb_append_char_all(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let cbuf = first_arg_ref!(
        args,
        Err(internal_error("j2r internal: StringBuilder.append args"))
    );
    let len = crate::array::array_length(&cbuf)?;
    sb_append_char_range(
        recv,
        &[Value::Ref(Some(cbuf)), Value::Int(0), Value::Int(len)],
        Dispatch::default(),
    )
}
pub(crate) fn n_sb_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.toString");
    Ok(Value::Ref(Some(object::string_from_rc(sb_text(r)))))
}
pub(crate) fn n_sb_length(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.length");
    Ok(Value::Int(utf16_units(&sb_text(r)).len() as i32))
}

// ---------------------------------------------------------------- Throwable
//
// Heap layout (audit A2 — suppressed-exception machinery): slot 0
// `detailMessage`, slot 1 `cause`, slot 2 the suppressed list as a Ref to a
// `java/lang/Throwable[]` snapshot in insertion order (null until the first
// `addSuppressed`). `getSuppressed` returns a fresh copy of the snapshot —
// the JVM returns a clone, so mutating the returned array must not affect
// the throwable. Natives registered on `Throwable` resolve for every
// builtin exception family through the parent-chain dispatch, which is what
// makes javac's try-with-resources lowering (JLS 14.20.3 emits
// `Throwable.addSuppressed`) run unmodified.
//
// Exception-message bytes probed on Temurin 21.0.13+9 (see
// docs/AUDIT_VERIFICATION.md, 2026-11-03): "Self-suppression not permitted"
// (IllegalArgumentException, checked before the null case, as in OpenJDK)
// and "Cannot suppress a null exception." (NullPointerException, with the
// trailing period).
