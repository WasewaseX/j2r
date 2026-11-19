use super::*;

fn baos_parts(r: &JRef, op: &str) -> Result<(JRef, i32), Throw> {
    let (buf, count) = with_obj(*r, |o| (o.slots[0].clone(), o.slots[1].clone()));
    let b = match buf {
        Value::Ref(Some(a)) => a,
        _ => return Err(internal_error(&format!("j2r internal: {op} buf slot"))),
    };
    let Value::Int(c) = count else {
        return Err(internal_error(&format!("j2r internal: {op} count slot")));
    };
    Ok((b, c))
}
/// Doubles the buffer until it fits `need` more bytes (JDK grow rule:
/// newCapacity = old << 1 while < minCapacity).
fn baos_ensure(r: &JRef, buf: &JRef, count: i32, need: i32) -> Result<JRef, Throw> {
    let cap = crate::array::array_length(buf)?;
    if count + need <= cap {
        return Ok(*buf);
    }
    let mut newcap = cap.max(1);
    while newcap < count + need {
        newcap = newcap.saturating_mul(2);
    }
    let bigger = crate::array::array_new_prim("[B", newcap)?;
    for i in 0..count {
        let v = crate::array::array_load(buf, i)?;
        crate::array::array_store(&bigger, i, v, crate::Dispatch::default())?;
    }
    with_obj_mut(*r, |o| o.slots[0] = Value::Ref(Some(bigger)));
    Ok(bigger)
}
pub fn n_baos_init(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayOutputStream.<init>")),
    };
    // JDK: buf = new byte[32], count = 0.
    let buf = crate::array::array_new_prim("[B", 32)?;
    with_obj_mut(r, |o| {
        o.slots[0] = Value::Ref(Some(buf));
        o.slots[1] = Value::Int(0);
    });
    Ok(Value::Void)
}
pub fn n_baos_init_size(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayOutputStream.<init>")),
    };
    let size = match args.first() {
        Some(Value::Int(v)) => *v,
        _ => return Err(missing("ByteArrayOutputStream.<init>")),
    };
    if size < 0 {
        // JDK: IllegalArgumentException("Negative initial size: " + size)
        return Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some(&format!("Negative initial size: {size}")),
        ));
    }
    let buf = crate::array::array_new_prim("[B", size)?;
    with_obj_mut(r, |o| {
        o.slots[0] = Value::Ref(Some(buf));
        o.slots[1] = Value::Int(0);
    });
    Ok(Value::Void)
}
pub fn n_baos_write_int(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayOutputStream.write")),
    };
    let b = match args.first() {
        Some(Value::Int(v)) => *v,
        _ => return Err(missing("ByteArrayOutputStream.write")),
    };
    let (buf, count) = baos_parts(&r, "ByteArrayOutputStream.write")?;
    let buf = baos_ensure(&r, &buf, count, 1)?;
    crate::array::array_store(
        &buf,
        count,
        Value::Int(b as i8 as i32),
        crate::Dispatch::default(),
    )?;
    with_obj_mut(r, |o| o.slots[1] = Value::Int(count + 1));
    Ok(Value::Void)
}
pub fn n_baos_write_bulk(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    // JDK: write(b) == write(b, 0, b.length) on SELF.
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayOutputStream.write")),
    };
    let b = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) | None => return Err(crate::npe()),
        _ => return Err(missing("ByteArrayOutputStream.write")),
    };
    let len = crate::array::array_length(&b)?;
    crate::invoke_virtual(
        &r,
        "java/io/ByteArrayOutputStream",
        "write",
        "([BII)V",
        &[Value::Ref(Some(b)), Value::Int(0), Value::Int(len)],
        ctx,
    )?;
    Ok(Value::Void)
}
pub fn n_baos_write_range(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayOutputStream.write")),
    };
    let b = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) | None => return Err(crate::npe()),
        _ => return Err(missing("ByteArrayOutputStream.write")),
    };
    let off = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(missing("ByteArrayOutputStream.write")),
    };
    let len = match args.get(2) {
        Some(Value::Int(v)) => *v,
        _ => return Err(missing("ByteArrayOutputStream.write")),
    };
    check_write_range(&b, off, len, "ByteArrayOutputStream")?;
    let (buf, count) = baos_parts(&r, "ByteArrayOutputStream.write")?;
    let buf = baos_ensure(&r, &buf, count, len)?;
    for i in 0..len {
        let v = crate::array::array_load(&b, off + i)?;
        crate::array::array_store(&buf, count + i, v, crate::Dispatch::default())?;
    }
    with_obj_mut(r, |o| o.slots[1] = Value::Int(count + len));
    Ok(Value::Void)
}
pub fn n_baos_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayOutputStream.size")),
    };
    let (_, count) = baos_parts(&r, "ByteArrayOutputStream.size")?;
    Ok(Value::Int(count))
}
pub fn n_baos_reset(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayOutputStream.reset")),
    };
    with_obj_mut(r, |o| o.slots[1] = Value::Int(0));
    Ok(Value::Void)
}
/// `toByteArray()` — Arrays.copyOf(buf, count).
pub fn n_baos_to_byte_array(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayOutputStream.toByteArray")),
    };
    let (buf, count) = baos_parts(&r, "ByteArrayOutputStream.toByteArray")?;
    let out = crate::array::array_new_prim("[B", count)?;
    for i in 0..count {
        let v = crate::array::array_load(&buf, i)?;
        crate::array::array_store(&out, i, v, ctx)?;
    }
    Ok(Value::Ref(Some(out)))
}
/// `toString()` — UTF-8 (platform default) decode of buf[0..count).
pub fn n_baos_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayOutputStream.toString")),
    };
    let (buf, count) = baos_parts(&r, "ByteArrayOutputStream.toString")?;
    let mut bytes = Vec::with_capacity(count as usize);
    for i in 0..count {
        match crate::array::array_load(&buf, i)? {
            Value::Int(v) => bytes.push(v as u8),
            _ => return Err(internal_error("j2r internal: BAOS toString element")),
        }
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok(Value::Ref(Some(crate::object::new_string(&text))))
}
pub fn n_baos_write_to(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayOutputStream.writeTo")),
    };
    let out = match args.first() {
        Some(Value::Ref(Some(o))) => *o,
        Some(Value::Ref(None)) | None => return Err(crate::npe()),
        _ => return Err(missing("ByteArrayOutputStream.writeTo")),
    };
    let (buf, count) = baos_parts(&r, "ByteArrayOutputStream.writeTo")?;
    crate::invoke_virtual(
        &out,
        "java/io/OutputStream",
        "write",
        "([BII)V",
        &[Value::Ref(Some(buf)), Value::Int(0), Value::Int(count)],
        ctx,
    )?;
    Ok(Value::Void)
}

// ---------------------------------------------------------------------------
// ByteArrayInputStream (skip/charset wave — the JZlib wave-2 phase-2
// candidate). JDK fields: buf, pos, mark, count. Slots: 0 = buf (byte[]
// ref), 1 = pos, 2 = mark, 3 = count. Semantics pinned by
// corpus/SkipProbe.java on Temurin 21.0.13: close() is a NO-OP (use after
// close works — no "Stream closed" state on this stream), negative skip
// returns 0 without touching the stream, skip past EOF returns the actual
// count, markSupported() is true.
// ---------------------------------------------------------------------------
fn bis_parts(r: &JRef, who: &str) -> Result<(JRef, i32, i32), Throw> {
    let (buf, pos, count) = with_obj(*r, |o| {
        (
            match &o.slots[0] {
                Value::Ref(Some(b)) => *b,
                _ => panic!("j2r internal: {who} buf slot"),
            },
            match o.slots[1] {
                Value::Int(v) => v,
                _ => panic!("j2r internal: {who} pos slot"),
            },
            match o.slots[3] {
                Value::Int(v) => v,
                _ => panic!("j2r internal: {who} count slot"),
            },
        )
    });
    Ok((buf, pos, count))
}
pub fn n_bis_init(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayInputStream.<init>")),
    };
    let buf = first_arg_ref!(args, Err(missing("ByteArrayInputStream.<init>")));
    let len = crate::array::array_length(&buf)?;
    with_obj_mut(r, |o| {
        o.slots[0] = Value::Ref(Some(buf));
        o.slots[1] = Value::Int(0);
        o.slots[2] = Value::Int(0);
        o.slots[3] = Value::Int(len);
    });
    Ok(Value::Void)
}
pub fn n_bis_init_range(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayInputStream.<init>")),
    };
    let (buf, offset, length) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(b))), Some(Value::Int(off)), Some(Value::Int(len))) => {
            (*b, *off, *len)
        }
        (Some(Value::Ref(None)), ..)
        | (_, Some(Value::Ref(None)), _)
        | (_, _, Some(Value::Ref(None))) => return Err(crate::npe()),
        _ => return Err(missing("ByteArrayInputStream.<init>")),
    };
    let total = crate::array::array_length(&buf)?;
    if offset < 0 || length < 0 || offset > total {
        // JDK: Objects.checkFromIndexRange(offset, length, buf.length)
        return Err(throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    let count = std::cmp::min(offset + length, total);
    with_obj_mut(r, |o| {
        o.slots[0] = Value::Ref(Some(buf));
        o.slots[1] = Value::Int(offset);
        o.slots[2] = Value::Int(offset);
        o.slots[3] = Value::Int(count);
    });
    Ok(Value::Void)
}
pub fn n_bis_read(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayInputStream.read")),
    };
    let (buf, pos, count) = bis_parts(&r, "ByteArrayInputStream.read")?;
    if pos >= count {
        return Ok(Value::Int(-1));
    }
    let v = match crate::array::array_load(&buf, pos)? {
        Value::Int(v) => v,
        _ => return Err(missing("ByteArrayInputStream.read buf element")),
    };
    with_obj_mut(r, |o| o.slots[1] = Value::Int(pos + 1));
    Ok(Value::Int(v & 0xff))
}
pub fn n_bis_read_range(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayInputStream.read")),
    };
    let (b, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(b))), Some(Value::Int(off)), Some(Value::Int(len))) => {
            (*b, *off, *len)
        }
        (Some(Value::Ref(None)), ..)
        | (_, Some(Value::Ref(None)), _)
        | (_, _, Some(Value::Ref(None))) => return Err(crate::npe()),
        _ => return Err(missing("ByteArrayInputStream.read")),
    };
    let blen = crate::array::array_length(&b)?;
    // JDK: Objects.checkFromIndexRange(off, len, b.length) — the generic
    // message is not pinned by the fixture (out of scope), the throw is.
    if off < 0 || len < 0 || off > blen - len {
        return Err(throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    let (buf, pos, count) = bis_parts(&r, "ByteArrayInputStream.read")?;
    if pos >= count {
        return Ok(Value::Int(-1));
    }
    let avail = count - pos;
    let take = std::cmp::min(len, avail);
    if take <= 0 {
        return Ok(Value::Int(0));
    }
    for i in 0..take {
        let v = match crate::array::array_load(&buf, pos + i)? {
            Value::Int(v) => v,
            _ => return Err(missing("ByteArrayInputStream.read buf element")),
        };
        crate::array::array_store(
            &b,
            off + i,
            Value::Int(v as i8 as i32),
            crate::Dispatch::default(),
        )?;
    }
    with_obj_mut(r, |o| o.slots[1] = Value::Int(pos + take));
    Ok(Value::Int(take))
}
pub fn n_bis_skip(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayInputStream.skip")),
    };
    let n = match args.first() {
        Some(Value::Long(v)) => *v,
        _ => return Err(missing("ByteArrayInputStream.skip")),
    };
    let (_, pos, count) = bis_parts(&r, "ByteArrayInputStream.skip")?;
    // JDK: long k = count - pos; if (n < k) k = (n < 0 ? 0 : n); pos += k.
    let mut k = (count - pos) as i64;
    if n < k {
        k = if n < 0 { 0 } else { n };
    }
    let new_pos = pos + k as i32;
    with_obj_mut(r, |o| o.slots[1] = Value::Int(new_pos));
    Ok(Value::Long(k))
}
pub fn n_bis_available(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayInputStream.available")),
    };
    let (_, pos, count) = bis_parts(&r, "ByteArrayInputStream.available")?;
    Ok(Value::Int(count - pos))
}
pub fn n_bis_mark(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayInputStream.mark")),
    };
    let pos = with_obj(r, |o| match o.slots[1] {
        Value::Int(v) => v,
        _ => panic!("j2r internal: ByteArrayInputStream.mark pos slot"),
    });
    with_obj_mut(r, |o| o.slots[2] = Value::Int(pos));
    Ok(Value::Void)
}
pub fn n_bis_reset(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("ByteArrayInputStream.reset")),
    };
    let mark = with_obj(r, |o| match o.slots[2] {
        Value::Int(v) => v,
        _ => panic!("j2r internal: ByteArrayInputStream.reset mark slot"),
    });
    with_obj_mut(r, |o| o.slots[1] = Value::Int(mark));
    Ok(Value::Void)
}
pub fn n_bis_mark_supported(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(1)) // JDK ByteArrayInputStream: true
}
pub(crate) fn bis_buf(r: &JRef, op: &str) -> Result<JRef, Throw> {
    match with_obj(*r, |o| o.slots[1].clone()) {
        Value::Ref(Some(b)) => Ok(b),
        // JDK getBufIfOpen: null buffer == closed
        Value::Ref(None) => Err(throw_new("java/io/IOException", Some("Stream closed"))),
        _ => Err(internal_error(&format!("j2r internal: {op} buf slot"))),
    }
}
pub(crate) fn bis_inner(r: &JRef, op: &str) -> Result<JRef, Throw> {
    match with_obj(*r, |o| o.slots[0].clone()) {
        Value::Ref(Some(s)) => Ok(s),
        Value::Ref(None) => Err(throw_new("java/io/IOException", Some("Stream closed"))),
        _ => Err(internal_error(&format!("j2r internal: {op} in slot"))),
    }
}
pub(crate) fn bis_state(r: &JRef) -> (i32, i32, i32, i32) {
    with_obj(*r, |o| {
        (
            match o.slots[2] {
                Value::Int(v) => v,
                _ => panic!("j2r internal: BIS pos slot"),
            },
            match o.slots[3] {
                Value::Int(v) => v,
                _ => panic!("j2r internal: BIS count slot"),
            },
            match o.slots[4] {
                Value::Int(v) => v,
                _ => panic!("j2r internal: BIS markpos slot"),
            },
            match o.slots[5] {
                Value::Int(v) => v,
                _ => panic!("j2r internal: BIS marklimit slot"),
            },
        )
    })
}
pub(crate) fn bis_set(
    r: &JRef,
    pos: i32,
    count: i32,
    markpos: i32,
    marklimit: i32,
    buf: Option<JRef>,
) {
    with_obj_mut(*r, |o| {
        o.slots[2] = Value::Int(pos);
        o.slots[3] = Value::Int(count);
        o.slots[4] = Value::Int(markpos);
        o.slots[5] = Value::Int(marklimit);
        if let Some(b) = buf {
            o.slots[1] = Value::Ref(Some(b));
        }
    });
}
