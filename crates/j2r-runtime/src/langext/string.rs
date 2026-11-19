use super::*;

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
/// `toCharArray`: a `[C` heap array in the CANONICAL prim-payload
/// representation — the exact object shape `array_new_prim("[C", len)`
/// builds (slots = [length]; the UTF-16 code units char-narrowed into a
/// `PrimData::I` payload). This was the ONE outlier producer of a
/// prim-descriptor array in the `prim: None` boxed-slot form, and the
/// typed backend's raw accessors (which own the payload contract) died
/// on the first `caload` over a toCharArray result — the run-2 P0
/// ("int-group accessor on a reference array", CharArrayRepProbe red
/// proof, array_load_i <- NGram.distance on StringsSimBench). The Value
/// paths always handled both forms; the raw accessors are the loud
/// drift detector and now see one uniform `[C` representation.
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
    Ok(Value::Ref(Some(crate::heap::alloc(crate::object::JObj {
        jclass: Arc::from("[C"),
        slots: vec![Value::Int(units.len() as i32)],
        prim: Some(crate::object::PrimData::I(
            units.into_iter().map(|u| u as i32).collect(),
        )),
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
