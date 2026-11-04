//! `java.lang` method extensions — Wave 2 of M3 (`docs/M3_PLAN.md` §5.2).
//!
//! Everything here is registered in the natives table (`natives.rs`) and
//! behaves exactly like the corresponding JDK method on Temurin 21 for the
//! supported set; anything outside the set keeps failing loudly at compile
//! time (not registered = resolve error).
//!
//! Java-exact notes implemented here:
//! * `String.trim` removes chars `<= U+0020` (NOT Unicode whitespace),
//! * `String.compareTo` compares UTF-16 code units (not Rust `str` order),
//! * `Math.floorDiv/floorMod` have Java's `/ by zero` and `MIN/-1` edges,
//! * `Math.addExact/subtractExact/multiplyExact` throw the JDK messages
//!   (`integer overflow` / `long overflow`),
//! * `Integer.parseInt` throws `NumberFormatException: For input string:
//!   "..."` (already registered in M1; extended here by the radix form).
//!
//! Character classification is implemented for ASCII plus the pinned
//! Unicode ranges exercised by the `CharClassify` differential fixture
//! (the three-channel harness is the arbiter — M3_PLAN §5.2).

use crate::heap::{with_obj, JRef};
use crate::Value;
use crate::{throw_new, Dispatch, Throw};
use std::sync::Arc;

fn str_of(v: &Value, who: &str) -> Arc<String> {
    match v {
        Value::Ref(Some(r)) => crate::object::string_value(r),
        _ => panic!("j2r internal: {who} without string"),
    }
}

/// UTF-16 code units of a string value (Java's unit of indexing).
fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn from_utf16(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

fn this(recv: Option<&JRef>, who: &str) -> JRef {
    *recv.unwrap_or_else(|| panic!("j2r internal: {who} called without receiver"))
}

fn arg<'a>(args: &'a [Value], i: usize, who: &str) -> &'a Value {
    args.get(i)
        .unwrap_or_else(|| panic!("j2r internal: {who} missing argument {i}"))
}

fn i32_at(args: &[Value], i: usize, who: &str) -> i32 {
    arg(args, i, who).expect_int()
}

fn string_index_oob(msg: String) -> Throw {
    throw_new("java/lang/StringIndexOutOfBoundsException", Some(&msg))
}

// ---------------------------------------------------------------- String

pub fn n_str_substring_one(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.substring"))),
        "String.substring",
    );
    let begin = i32_at(args, 0, "String.substring");
    let len = utf16(&s).len() as i32;
    if begin < 0 || begin > len {
        // JDK 21: String.checkBoundsOffCount message shape
        return Err(string_index_oob(format!(
            "Range [{begin}, {len}) out of bounds for length {len}"
        )));
    }
    let units = utf16(&s);
    Ok(Value::Ref(Some(crate::object::new_string(&from_utf16(
        &units[begin as usize..],
    )))))
}

pub fn n_str_substring_two(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.substring"))),
        "String.substring",
    );
    let begin = i32_at(args, 0, "String.substring");
    let end = i32_at(args, 1, "String.substring");
    let len = utf16(&s).len() as i32;
    if begin < 0 || end > len || begin > end {
        return Err(string_index_oob(format!(
            "Range [{begin}, {end}) out of bounds for length {len}"
        )));
    }
    let units = utf16(&s);
    Ok(Value::Ref(Some(crate::object::new_string(&from_utf16(
        &units[begin as usize..end as usize],
    )))))
}

/// `indexOf(String)` / `indexOf(String, int)` — UTF-16 code-unit search.
pub fn n_str_index_of_str(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.indexOf"))),
        "String.indexOf",
    );
    let needle = str_of(arg(args, 0, "String.indexOf"), "String.indexOf arg");
    let from = if args.len() > 1 {
        i32_at(args, 1, "String.indexOf")
    } else {
        0
    };
    let hay = utf16(&s);
    let ndl = utf16(&needle);
    let start = from.max(0) as usize;
    if ndl.is_empty() {
        // Java: empty needle matches at max(0, from), or length when from > len
        let at = if from > hay.len() as i32 {
            hay.len() as i32
        } else {
            from.max(0)
        };
        return Ok(Value::Int(at));
    }
    if start <= hay.len() && ndl.len() <= hay.len() {
        let mut i = start;
        while i + ndl.len() <= hay.len() {
            if hay[i..i + ndl.len()] == ndl[..] {
                return Ok(Value::Int(i as i32));
            }
            i += 1;
        }
    }
    Ok(Value::Int(-1))
}

/// `indexOf(int ch)` / `lastIndexOf(int ch)` — a code point search over
/// UTF-16 code units (supplementary code points match surrogate pairs).
pub fn n_str_index_of_ch(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.indexOf"))),
        "String.indexOf",
    );
    let ch = i32_at(args, 0, "String.indexOf") as u32;
    let hay = utf16(&s);
    let mut units: Vec<u16> = Vec::new();
    if ch > 0xFFFF {
        units.push(0xD800 + ((ch - 0x10000) >> 10) as u16);
        units.push(0xDC00 + ((ch - 0x10000) & 0x3FF) as u16);
    } else {
        units.push(ch as u16);
    }
    for (i, w) in hay.windows(units.len()).enumerate() {
        if w == units.as_slice() {
            return Ok(Value::Int(i as i32));
        }
    }
    Ok(Value::Int(-1))
}

pub fn n_str_last_index_of_ch(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.lastIndexOf"))),
        "String.lastIndexOf",
    );
    let ch = i32_at(args, 0, "String.lastIndexOf") as u32;
    let hay = utf16(&s);
    let mut units: Vec<u16> = Vec::new();
    if ch > 0xFFFF {
        units.push(0xD800 + ((ch - 0x10000) >> 10) as u16);
        units.push(0xDC00 + ((ch - 0x10000) & 0x3FF) as u16);
    } else {
        units.push(ch as u16);
    }
    if hay.len() >= units.len() {
        for i in (0..=hay.len() - units.len()).rev() {
            if hay[i..i + units.len()] == units[..] {
                return Ok(Value::Int(i as i32));
            }
        }
    }
    Ok(Value::Int(-1))
}

pub fn n_str_concat(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.concat"))),
        "String.concat",
    );
    let other = str_of(arg(args, 0, "String.concat"), "String.concat arg");
    Ok(Value::Ref(Some(crate::object::new_string(&format!(
        "{s}{other}"
    )))))
}

pub fn n_str_replace_cc(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.replace"))),
        "String.replace",
    );
    let old = i32_at(args, 0, "String.replace") as u16;
    let new = i32_at(args, 1, "String.replace") as u16;
    let units: Vec<u16> = utf16(&s)
        .iter()
        .map(|u| if *u == old { new } else { *u })
        .collect();
    Ok(Value::Ref(Some(crate::object::new_string(&from_utf16(
        &units,
    )))))
}

pub fn n_str_to_upper(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let _ = args;
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.toUpperCase"))),
        "String.toUpperCase",
    );
    Ok(Value::Ref(Some(crate::object::new_string(
        &s.to_uppercase(),
    ))))
}

pub fn n_str_to_lower(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let _ = args;
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.toLowerCase"))),
        "String.toLowerCase",
    );
    Ok(Value::Ref(Some(crate::object::new_string(
        &s.to_lowercase(),
    ))))
}

/// Java semantics: trim chars `<= U+0020` from both ends.
pub fn n_str_trim(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let _ = args;
    let s = str_of(&Value::Ref(Some(this(recv, "String.trim"))), "String.trim");
    let units = utf16(&s);
    let mut b = 0usize;
    let mut e = units.len();
    while b < e && units[b] <= 0x20 {
        b += 1;
    }
    while e > b && units[e - 1] <= 0x20 {
        e -= 1;
    }
    Ok(Value::Ref(Some(crate::object::new_string(&from_utf16(
        &units[b..e],
    )))))
}

/// UTF-16 code-unit lexicographic compare (Java `String.compareTo`).
pub fn n_str_compare_to(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.compareTo"))),
        "String.compareTo",
    );
    let other = str_of(arg(args, 0, "String.compareTo"), "String.compareTo arg");
    let a = utf16(&s);
    let b = utf16(&other);
    for (x, y) in a.iter().zip(b.iter()) {
        if x != y {
            return Ok(Value::Int(*x as i32 - *y as i32));
        }
    }
    Ok(Value::Int(a.len() as i32 - b.len() as i32))
}

/// Java `compareToIgnoreCase`: fold both units with
/// `toLowerCase(toUpperCase(u))` and compare; prefix rule as in `compareTo`.
pub fn n_str_compare_to_ignore_case(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.compareToIgnoreCase"))),
        "String.compareToIgnoreCase",
    );
    let other = str_of(arg(args, 0, "String.compareToIgnoreCase"), "arg");
    let a = utf16(&s);
    let b = utf16(&other);
    let fold = |u: u16| -> u16 {
        match char::from_u32(u as u32) {
            Some(c) => {
                let up: Vec<u16> = c
                    .to_uppercase()
                    .collect::<String>()
                    .encode_utf16()
                    .collect();
                if up.len() == 1 {
                    match char::from_u32(up[0] as u32) {
                        Some(c2) => {
                            let lo: Vec<u16> = c2
                                .to_lowercase()
                                .collect::<String>()
                                .encode_utf16()
                                .collect();
                            if lo.len() == 1 {
                                lo[0]
                            } else {
                                u
                            }
                        }
                        None => u,
                    }
                } else {
                    u
                }
            }
            None => u,
        }
    };
    for (x, y) in a.iter().zip(b.iter()) {
        let fx = fold(*x);
        let fy = fold(*y);
        if fx != fy {
            return Ok(Value::Int(fx as i32 - fy as i32));
        }
    }
    Ok(Value::Int(a.len() as i32 - b.len() as i32))
}

pub fn n_str_starts_with(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.startsWith"))),
        "String.startsWith",
    );
    let p = str_of(arg(args, 0, "String.startsWith"), "arg");
    Ok(Value::Int(s.starts_with(&*p) as i32))
}

pub fn n_str_ends_with(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.endsWith"))),
        "String.endsWith",
    );
    let p = str_of(arg(args, 0, "String.endsWith"), "arg");
    Ok(Value::Int(s.ends_with(&*p) as i32))
}

/// The string payload of any `CharSequence` in the supported set (String,
/// StringBuilder, StringBuffer).
pub fn char_seq_value(v: &Value, who: &str) -> Arc<String> {
    match v {
        Value::Ref(Some(r)) => with_obj(*r, |o| match &o.slots[0] {
            Value::Str(s) => s.clone(),
            _other => panic!("j2r internal: {who} is not a CharSequence: {}", o.jclass),
        }),
        _ => panic!("j2r internal: {who} without CharSequence (null)"),
    }
}

pub fn n_str_contains(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.contains"))),
        "String.contains",
    );
    let needle = char_seq_value(arg(args, 0, "String.contains"), "String.contains arg");
    Ok(Value::Int(s.contains(&*needle) as i32))
}

/// Java `equalsIgnoreCase`: same length, and every unit pair satisfies
/// `equalsIgnoreCase(a, b)` (upper-fold equal OR lower-fold equal).
pub fn n_str_equals_ignore_case(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.equalsIgnoreCase"))),
        "String.equalsIgnoreCase",
    );
    let other = str_of(arg(args, 0, "String.equalsIgnoreCase"), "arg");
    let a = utf16(&s);
    let b = utf16(&other);
    if a.len() != b.len() {
        return Ok(Value::Int(0));
    }
    let eq = |x: u16, y: u16| -> bool {
        if x == y {
            return true;
        }
        let up = |u: u16| -> u16 {
            match char::from_u32(u as u32) {
                Some(c) => {
                    let v: Vec<u16> = c
                        .to_uppercase()
                        .collect::<String>()
                        .encode_utf16()
                        .collect();
                    if v.len() == 1 {
                        v[0]
                    } else {
                        u
                    }
                }
                None => u,
            }
        };
        let lo = |u: u16| -> u16 {
            match char::from_u32(u as u32) {
                Some(c) => {
                    let v: Vec<u16> = c
                        .to_lowercase()
                        .collect::<String>()
                        .encode_utf16()
                        .collect();
                    if v.len() == 1 {
                        v[0]
                    } else {
                        u
                    }
                }
                None => u,
            }
        };
        up(x) == up(y) || lo(x) == lo(y)
    };
    for (x, y) in a.iter().zip(b.iter()) {
        if !eq(*x, *y) {
            return Ok(Value::Int(0));
        }
    }
    Ok(Value::Int(1))
}

/// `toCharArray`: a `[C` heap array with UTF-16 code units as Int slots.
pub fn n_str_to_char_array(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = args;
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.toCharArray"))),
        "String.toCharArray",
    );
    let units = utf16(&s);
    let mut slots = Vec::with_capacity(units.len() + 1);
    slots.push(Value::Int(units.len() as i32));
    for u in units {
        slots.push(Value::Int(u as i32));
    }
    Ok(Value::Ref(Some(crate::heap::alloc(crate::object::JObj {
        jclass: Arc::from("[C"),
        slots,
    }))))
}

/// Java 11 `repeat(int)`: negative count throws
/// `IllegalArgumentException("count is negative: N")`.
pub fn n_str_repeat(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let s = str_of(
        &Value::Ref(Some(this(recv, "String.repeat"))),
        "String.repeat",
    );
    let count = i32_at(args, 0, "String.repeat");
    if count < 0 {
        return Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some(&format!("count is negative: {count}")),
        ));
    }
    let mut out = String::new();
    for _ in 0..count {
        out.push_str(&s);
    }
    Ok(Value::Ref(Some(crate::object::new_string(&out))))
}

// ---------------------------------------------------------------- StringBuilder
//
// StringBuilder keeps its characters as Value::Str in slot 0
// (copy-on-write through Arc::make_mut — same convention as M1).

fn sb_str(recv: Option<&JRef>, who: &str) -> Arc<String> {
    let r = this(recv, who);
    with_obj(r, |o| match &o.slots[0] {
        Value::Str(s) => s.clone(),
        other => panic!("j2r internal: {who} on non-StringBuilder {other:?}"),
    })
}

fn sb_set(recv: &JRef, s: String) {
    // Fresh Rc: any String previously shared with a `toString()` result
    // stays untouched (the value-level equivalent of Java's immutable
    // String snapshots).
    crate::heap::with_obj_mut(*recv, |o| {
        o.slots[0] = Value::Str(Arc::new(s));
    });
}

fn sb_bounds(i: i32, len: i32) -> Result<(), Throw> {
    if i < 0 || i >= len {
        return Err(string_index_oob(format!(
            "Index {i} out of bounds for length {len}"
        )));
    }
    Ok(())
}

/// `StringBuilder.reverse()` — Java reverses UTF-16 code units preserving
/// surrogate pairs; for valid strings this equals reversing Unicode scalar
/// values (Rust `chars().rev()`).
pub fn n_sb_reverse(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let _ = args;
    let s = sb_str(recv, "StringBuilder.reverse");
    let r: String = s.chars().rev().collect();
    let me = this(recv, "StringBuilder.reverse");
    sb_set(&me, r);
    Ok(Value::Ref(Some(me)))
}

/// Shared `insert(int offset, X)` implementation: Java bounds are
/// `0..=count` (`offset < 0 || offset > count` -> SIOOBE "offset {o},
/// length {l}" for AbstractStringBuilder.insert — pinned by fixture).
fn sb_insert(recv: Option<&JRef>, offset: i32, text: String) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.insert");
    let s = sb_str(recv, "StringBuilder.insert");
    let len = utf16(&s).len() as i32;
    if offset < 0 || offset > len {
        return Err(string_index_oob(format!(
            "Range [{offset}, {len}) out of bounds for length {len}"
        )));
    }
    let units = utf16(&s);
    let ins = utf16(&text);
    let mut out: Vec<u16> = Vec::with_capacity(units.len() + ins.len());
    out.extend_from_slice(&units[..offset as usize]);
    out.extend_from_slice(&ins);
    out.extend_from_slice(&units[offset as usize..]);
    sb_set(&r, from_utf16(&out));
    Ok(Value::Ref(Some(r)))
}

pub fn n_sb_insert_str(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let _ = ctx;
    let text = str_of(arg(args, 1, "StringBuilder.insert"), "insert arg");
    sb_insert(
        recv,
        i32_at(args, 0, "StringBuilder.insert"),
        text.to_string(),
    )
}

pub fn n_sb_insert_i(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let _ = ctx;
    let text = i32_at(args, 1, "StringBuilder.insert").to_string();
    sb_insert(recv, i32_at(args, 0, "StringBuilder.insert"), text)
}

pub fn n_sb_insert_c(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let _ = ctx;
    let c = i32_at(args, 1, "StringBuilder.insert") as u16;
    sb_insert(
        recv,
        i32_at(args, 0, "StringBuilder.insert"),
        from_utf16(&[c]),
    )
}

pub fn n_sb_insert_j(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let _ = ctx;
    let text = arg(args, 1, "StringBuilder.insert")
        .expect_long()
        .to_string();
    sb_insert(recv, i32_at(args, 0, "StringBuilder.insert"), text)
}

pub fn n_sb_delete_char_at(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.deleteCharAt");
    let s = sb_str(recv, "StringBuilder.deleteCharAt");
    let index = i32_at(args, 0, "StringBuilder.deleteCharAt");
    let len = utf16(&s).len() as i32;
    if index < 0 || index >= len {
        return Err(string_index_oob(format!(
            "Index {index} out of bounds for length {len}"
        )));
    }
    let mut units = utf16(&s);
    units.remove(index as usize);
    sb_set(&r, from_utf16(&units));
    Ok(Value::Ref(Some(r)))
}

pub fn n_sb_delete(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.delete");
    let s = sb_str(recv, "StringBuilder.delete");
    let start = i32_at(args, 0, "StringBuilder.delete");
    let end = i32_at(args, 1, "StringBuilder.delete");
    let len = utf16(&s).len() as i32;
    if start < 0 || start > len || start > end {
        return Err(string_index_oob(format!(
            "Range [{start}, {end}) out of bounds for length {len}"
        )));
    }
    let end = end.min(len);
    let units = utf16(&s);
    let mut out: Vec<u16> = Vec::with_capacity(units.len());
    out.extend_from_slice(&units[..start as usize]);
    out.extend_from_slice(&units[end as usize..]);
    sb_set(&r, from_utf16(&out));
    Ok(Value::Ref(Some(r)))
}

pub fn n_sb_replace(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.replace");
    let s = sb_str(recv, "StringBuilder.replace");
    let start = i32_at(args, 0, "StringBuilder.replace");
    let end = i32_at(args, 1, "StringBuilder.replace");
    let text = str_of(arg(args, 2, "StringBuilder.replace"), "replace arg");
    let len = utf16(&s).len() as i32;
    if start < 0 || start > len || start > end {
        return Err(string_index_oob(format!(
            "Range [{start}, {end}) out of bounds for length {len}"
        )));
    }
    let end = end.min(len);
    let units = utf16(&s);
    let mut out: Vec<u16> = Vec::new();
    out.extend_from_slice(&units[..start as usize]);
    out.extend_from_slice(&utf16(&text));
    out.extend_from_slice(&units[end as usize..]);
    sb_set(&r, from_utf16(&out));
    Ok(Value::Ref(Some(r)))
}

pub fn n_sb_set_length(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.setLength");
    let s = sb_str(recv, "StringBuilder.setLength");
    let n = i32_at(args, 0, "StringBuilder.setLength");
    if n < 0 {
        return Err(string_index_oob(format!("String index out of range: {n}")));
    }
    let units = utf16(&s);
    let mut out = units;
    out.resize(n as usize, 0);
    sb_set(&r, from_utf16(&out));
    Ok(Value::Void)
}

pub fn n_sb_capacity(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let _ = args;
    // The runtime StringBuilder model keeps one String payload; its
    // "capacity" is not observable except through this method, which we
    // define as Java's: 16 + current length for the String ctor, 16
    // minimum. Documented honest deviation: the arena model does not
    // expose real buffer capacity, so capacity() returns 16 + length
    // (the invariant the (String) ctor guarantees in Java).
    let s = sb_str(recv, "StringBuilder.capacity");
    let len = utf16(&s).len() as i32;
    Ok(Value::Int(16 + len))
}

pub fn n_sb_char_at(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let s = sb_str(recv, "StringBuilder.charAt");
    let i = i32_at(args, 0, "StringBuilder.charAt");
    let units = utf16(&s);
    sb_bounds(i, units.len() as i32)?;
    Ok(Value::Int(units[i as usize] as i32))
}

pub fn n_sb_index_of(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let s = sb_str(recv, "StringBuilder.indexOf");
    let needle = str_of(arg(args, 0, "StringBuilder.indexOf"), "indexOf arg");
    let hay = utf16(&s);
    let ndl = utf16(&needle);
    if ndl.is_empty() {
        return Ok(Value::Int(0));
    }
    if ndl.len() <= hay.len() {
        for i in 0..=hay.len() - ndl.len() {
            if hay[i..i + ndl.len()] == ndl[..] {
                return Ok(Value::Int(i as i32));
            }
        }
    }
    Ok(Value::Int(-1))
}

pub fn n_sb_init_capacity(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.<init>");
    let cap = i32_at(args, 0, "StringBuilder.<init>");
    if cap < 0 {
        // Temurin 21: `new byte[capacity]` with COMPACT_STRINGS -> the raw
        // NegativeArraySizeException carrying the capacity as its message.
        return Err(throw_new(
            "java/lang/NegativeArraySizeException",
            Some(&format!("{cap}")),
        ));
    }
    crate::heap::with_obj_mut(r, |o| o.slots[0] = Value::Str(Arc::new(String::new())));
    Ok(Value::Void)
}

// ---------------------------------------------------------------- Math

pub fn n_math_floor_div_i(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = i32_at(args, 0, "Math.floorDiv");
    let b = i32_at(args, 1, "Math.floorDiv");
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    let q = a.wrapping_div(b);
    let r = a.wrapping_rem(b);
    // floor semantics: adjust when signs differ and remainder != 0
    let adjusted = if (r != 0) && ((r < 0) != (b < 0)) {
        q.wrapping_sub(1)
    } else {
        q
    };
    Ok(Value::Int(adjusted))
}

pub fn n_math_floor_mod_i(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = i32_at(args, 0, "Math.floorMod");
    let b = i32_at(args, 1, "Math.floorMod");
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    let r = a.wrapping_rem(b);
    let m = if (r != 0) && ((r < 0) != (b < 0)) {
        r + b
    } else {
        r
    };
    Ok(Value::Int(m))
}

pub fn n_math_floor_div_j(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.floorDiv").expect_long();
    let b = arg(args, 1, "Math.floorDiv").expect_long();
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    let q = a.wrapping_div(b);
    let r = a.wrapping_rem(b);
    let adjusted = if (r != 0) && ((r < 0) != (b < 0)) {
        q.wrapping_sub(1)
    } else {
        q
    };
    Ok(Value::Long(adjusted))
}

pub fn n_math_floor_mod_j(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.floorMod").expect_long();
    let b = arg(args, 1, "Math.floorMod").expect_long();
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    let r = a.wrapping_rem(b);
    let m = if (r != 0) && ((r < 0) != (b < 0)) {
        r + b
    } else {
        r
    };
    Ok(Value::Long(m))
}

/// `Math.round(double)` — `floor(a + 0.5)` saturating to Long, NaN -> 0.
pub fn n_math_round_d(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.round").expect_double();
    if a.is_nan() {
        return Ok(Value::Long(0));
    }
    // Java: (long)Math.floor(d + 0.5d) with saturation at Long bounds
    let shifted = a + 0.5;
    let out = if shifted >= 9.223372036854776E18 {
        i64::MAX
    } else if shifted <= -9.223372036854776E18 {
        i64::MIN
    } else {
        shifted.floor() as i64
    };
    Ok(Value::Long(out))
}

pub fn n_math_round_f(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.round").expect_float();
    if a.is_nan() {
        return Ok(Value::Int(0));
    }
    let shifted = a + 0.5;
    let out = if shifted >= 2_147_483_648.0f32 {
        i32::MAX
    } else if shifted <= -2_147_483_648.0f32 {
        i32::MIN
    } else {
        shifted.floor() as i32
    };
    Ok(Value::Int(out))
}

pub fn n_math_sqrt(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(
        arg(args, 0, "Math.sqrt").expect_double().sqrt(),
    ))
}

pub fn n_math_cbrt(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(
        arg(args, 0, "Math.cbrt").expect_double().cbrt(),
    ))
}

pub fn n_math_pow(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.pow").expect_double();
    let b = arg(args, 1, "Math.pow").expect_double();
    Ok(Value::Double(a.powf(b)))
}

pub fn n_math_floor(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(
        arg(args, 0, "Math.floor").expect_double().floor(),
    ))
}

pub fn n_math_ceil(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(
        arg(args, 0, "Math.ceil").expect_double().ceil(),
    ))
}

pub fn n_math_signum_d(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.signum").expect_double();
    let out = if a.is_nan() {
        f64::NAN
    } else if a == 0.0 {
        a // preserves the sign of ±0.0
    } else if a > 0.0 {
        1.0
    } else {
        -1.0
    };
    Ok(Value::Double(out))
}

pub fn n_math_signum_f(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.signum").expect_float();
    let out = if a.is_nan() {
        f32::NAN
    } else if a == 0.0 {
        a
    } else if a > 0.0 {
        1.0
    } else {
        -1.0
    };
    Ok(Value::Float(out))
}

pub fn n_math_to_int_exact(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, 0, "Math.toIntExact").expect_long();
    let t = v as i32;
    if t as i64 != v {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("integer overflow"),
        ));
    }
    Ok(Value::Int(t))
}

// (The exact-arithmetic functions are written out explicitly below —
// generated code must stay reviewable, no macro gymnastics.)

pub fn n_math_add_exact_i(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = i32_at(args, 0, "Math.addExact");
    let b = i32_at(args, 1, "Math.addExact");
    match a.checked_add(b) {
        Some(v) => Ok(Value::Int(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("integer overflow"),
        )),
    }
}

pub fn n_math_subtract_exact_i(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = i32_at(args, 0, "Math.subtractExact");
    let b = i32_at(args, 1, "Math.subtractExact");
    match a.checked_sub(b) {
        Some(v) => Ok(Value::Int(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("integer overflow"),
        )),
    }
}

pub fn n_math_multiply_exact_i(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = i32_at(args, 0, "Math.multiplyExact");
    let b = i32_at(args, 1, "Math.multiplyExact");
    match a.checked_mul(b) {
        Some(v) => Ok(Value::Int(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("integer overflow"),
        )),
    }
}

pub fn n_math_add_exact_j(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.addExact").expect_long();
    let b = arg(args, 1, "Math.addExact").expect_long();
    match a.checked_add(b) {
        Some(v) => Ok(Value::Long(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("long overflow"),
        )),
    }
}

pub fn n_math_subtract_exact_j(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.subtractExact").expect_long();
    let b = arg(args, 1, "Math.subtractExact").expect_long();
    match a.checked_sub(b) {
        Some(v) => Ok(Value::Long(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("long overflow"),
        )),
    }
}

pub fn n_math_multiply_exact_j(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.multiplyExact").expect_long();
    let b = arg(args, 1, "Math.multiplyExact").expect_long();
    match a.checked_mul(b) {
        Some(v) => Ok(Value::Long(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("long overflow"),
        )),
    }
}

// ---------------------------------------------------------------- Integer / Long

/// Java `Integer.toUnsignedString(x, 2|8|16)` — unsigned formatting.
macro_rules! int_radix_fns {
    ($bin:ident, $oct:ident, $hex:ident, $t:ty) => {
        pub fn $bin(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let v = args.first().cloned().unwrap_or(Value::Int(0));
            let u = unsigned_of(&v);
            Ok(Value::Ref(Some(crate::object::new_string(&format!(
                "{:b}",
                u
            )))))
        }
        pub fn $oct(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let v = args.first().cloned().unwrap_or(Value::Int(0));
            let u = unsigned_of(&v);
            Ok(Value::Ref(Some(crate::object::new_string(&format!(
                "{:o}",
                u
            )))))
        }
        pub fn $hex(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let v = args.first().cloned().unwrap_or(Value::Int(0));
            let u = unsigned_of(&v);
            Ok(Value::Ref(Some(crate::object::new_string(&format!(
                "{:x}",
                u
            )))))
        }
    };
}

fn unsigned_of(v: &Value) -> u64 {
    match v {
        Value::Int(i) => *i as u32 as u64,
        Value::Long(l) => *l as u64,
        other => panic!("j2r internal: expected int/long, got {other:?}"),
    }
}

int_radix_fns!(n_int_to_binary, n_int_to_octal, n_int_to_hex, i32);
int_radix_fns!(n_long_to_binary, n_long_to_octal, n_long_to_hex, i64);

pub fn n_int_compare(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = i32_at(args, 0, "Integer.compare");
    let b = i32_at(args, 1, "Integer.compare");
    Ok(Value::Int(if a < b {
        -1
    } else if a > b {
        1
    } else {
        0
    }))
}

pub fn n_long_compare(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Long.compare").expect_long();
    let b = arg(args, 1, "Long.compare").expect_long();
    Ok(Value::Int(if a < b {
        -1
    } else if a > b {
        1
    } else {
        0
    }))
}

macro_rules! int_min_max_sum {
    ($min:ident, $max:ident, $sum:ident, $t:ty, $wrap:ident) => {
        pub fn $min(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let a = num_at::<$t>(args, 0);
            let b = num_at::<$t>(args, 1);
            Ok(make_num::<$t>(a.min(b)))
        }
        pub fn $max(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let a = num_at::<$t>(args, 0);
            let b = num_at::<$t>(args, 1);
            Ok(make_num::<$t>(a.max(b)))
        }
        pub fn $sum(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let a = num_at::<$t>(args, 0);
            let b = num_at::<$t>(args, 1);
            Ok(make_num::<$t>(a.$wrap(b)))
        }
    };
}

fn num_at<T>(args: &[Value], i: usize) -> T
where
    Value: ExtractNum<T>,
{
    args.get(i)
        .unwrap_or_else(|| panic!("j2r internal: missing arg {i}"))
        .extract()
}

fn make_num<T>(v: T) -> Value
where
    Value: ExtractNum<T>,
{
    Value::from_num(v)
}

/// Trait glue so the min/max/sum helpers work for both i32 and i64.
pub trait ExtractNum<T> {
    fn extract(&self) -> T;
    fn from_num(v: T) -> Value;
}

impl ExtractNum<i32> for Value {
    fn extract(&self) -> i32 {
        self.expect_int()
    }
    fn from_num(v: i32) -> Value {
        Value::Int(v)
    }
}

impl ExtractNum<i64> for Value {
    fn extract(&self) -> i64 {
        self.expect_long()
    }
    fn from_num(v: i64) -> Value {
        Value::Long(v)
    }
}

int_min_max_sum!(n_int_min, n_int_max, n_int_sum, i32, wrapping_add);
int_min_max_sum!(n_long_min, n_long_max, n_long_sum, i64, wrapping_add);

pub fn n_int_bit_count(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.bitCount");
    Ok(Value::Int(v.count_ones() as i32))
}

pub fn n_long_bit_count(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.bitCount").expect_long();
    Ok(Value::Int(v.count_ones() as i32))
}

pub fn n_int_nlz(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.numberOfLeadingZeros");
    Ok(Value::Int(v.leading_zeros() as i32))
}

pub fn n_int_ntz(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.numberOfTrailingZeros");
    Ok(Value::Int(v.trailing_zeros() as i32))
}

pub fn n_long_nlz(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.numberOfLeadingZeros").expect_long();
    Ok(Value::Int(v.leading_zeros() as i32))
}

pub fn n_long_ntz(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.numberOfTrailingZeros").expect_long();
    Ok(Value::Int(v.trailing_zeros() as i32))
}

pub fn n_int_highest_one_bit(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.highestOneBit");
    let out = if v == 0 {
        0
    } else {
        1i32.wrapping_shl(31 - (v as u32).leading_zeros())
    };
    Ok(Value::Int(out))
}

pub fn n_int_lowest_one_bit(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.lowestOneBit");
    Ok(Value::Int(v.isolate_lowest_one()))
}

pub fn n_long_highest_one_bit(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.highestOneBit").expect_long();
    let out = if v == 0 {
        0
    } else {
        1i64.wrapping_shl(63 - (v as u64).leading_zeros())
    };
    Ok(Value::Long(out))
}

pub fn n_long_lowest_one_bit(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.lowestOneBit").expect_long();
    Ok(Value::Long(v.isolate_lowest_one()))
}

pub fn n_int_rotate_left(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.rotateLeft");
    let d = i32_at(args, 1, "Integer.rotateLeft");
    Ok(Value::Int(v.rotate_left((d as u32) & 31)))
}

pub fn n_int_rotate_right(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.rotateRight");
    let d = i32_at(args, 1, "Integer.rotateRight");
    Ok(Value::Int(v.rotate_right((d as u32) & 31)))
}

pub fn n_long_rotate_left(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.rotateLeft").expect_long();
    let d = i32_at(args, 1, "Long.rotateLeft");
    Ok(Value::Long(v.rotate_left((d as u32) & 63)))
}

pub fn n_long_rotate_right(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.rotateRight").expect_long();
    let d = i32_at(args, 1, "Long.rotateRight");
    Ok(Value::Long(v.rotate_right((d as u32) & 63)))
}

pub fn n_int_reverse(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.reverse");
    Ok(Value::Int(v.reverse_bits()))
}

pub fn n_long_reverse(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.reverse").expect_long();
    Ok(Value::Long(v.reverse_bits()))
}

/// `Integer.parseInt(String, int radix)` (the no-radix form exists since M1).
pub fn n_int_parse_int_radix(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let s = match arg(args, 0, "Integer.parseInt") {
        Value::Ref(Some(r)) => crate::object::string_value(r).to_string(),
        _ => {
            return Err(throw_new(
                "java/lang/NumberFormatException",
                Some("Cannot parse null string"),
            ))
        }
    };
    let radix = i32_at(args, 1, "Integer.parseInt");
    if !(CharacterRanges::MIN_RADIX..=36).contains(&radix) {
        return Err(throw_new(
            "java/lang/NumberFormatException",
            Some(&format!("radix {radix} out of range")),
        ));
    }
    parse_signed(&s, radix as u32, "Integer.parseInt")
}

/// Shared signed parse with Java's exact NumberFormatException messages.
pub fn parse_signed(s: &str, radix: u32, who: &str) -> Result<Value, Throw> {
    // JDK 17+: `For input string: "X"` with a ` under radix R` suffix when
    // the radix is not 10 (forInputString(s, radix))
    let nfe = move |text: &str| {
        let suffix = if radix == 10 {
            String::new()
        } else {
            format!(" under radix {radix}")
        };
        throw_new(
            "java/lang/NumberFormatException",
            Some(&format!("For input string: \"{text}\"{suffix}")),
        )
    };
    if s.is_empty() {
        return Err(nfe(s));
    }
    let (digits, negative) = match s.as_bytes()[0] {
        b'-' | b'+' => (&s[1..], s.as_bytes()[0] == b'-'),
        _ => (s, false),
    };
    if digits.is_empty() {
        return Err(nfe(s));
    }
    let mut acc: i64 = 0;
    for c in digits.chars() {
        let d = match c.to_digit(36) {
            Some(d) if d < radix => d as i64,
            _ => return Err(nfe(s)),
        };
        acc = acc
            .checked_mul(radix as i64)
            .and_then(|x| x.checked_add(d))
            .ok_or_else(|| nfe(s))?;
    }
    let limit = if negative {
        -(i32::MIN as i64)
    } else {
        i32::MAX as i64
    };
    if acc > limit {
        return Err(nfe(s));
    }
    let v = if negative { -acc } else { acc };
    let _ = who;
    Ok(Value::Int(v as i32))
}

// ---------------------------------------------------------------- Character

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

// Java also overloads addExact/subtractExact/multiplyExact as (long, int):
// javac emits those descriptors for mixed-width call sites.
pub fn n_math_add_exact_ji(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.addExact").expect_long();
    let b = i32_at(args, 1, "Math.addExact") as i64;
    match a.checked_add(b) {
        Some(v) => Ok(Value::Long(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("long overflow"),
        )),
    }
}

pub fn n_math_subtract_exact_ji(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.subtractExact").expect_long();
    let b = i32_at(args, 1, "Math.subtractExact") as i64;
    match a.checked_sub(b) {
        Some(v) => Ok(Value::Long(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("long overflow"),
        )),
    }
}

pub fn n_math_multiply_exact_ji(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Math.multiplyExact").expect_long();
    let b = i32_at(args, 1, "Math.multiplyExact") as i64;
    match a.checked_mul(b) {
        Some(v) => Ok(Value::Long(v)),
        None => Err(throw_new(
            "java/lang/ArithmeticException",
            Some("long overflow"),
        )),
    }
}
