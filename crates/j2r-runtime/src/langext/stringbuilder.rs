use super::*;

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
