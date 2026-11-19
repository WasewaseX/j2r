use super::*;

fn pb_parts(r: &JRef, op: &str) -> Result<(JRef, JRef, i32), Throw> {
    let buf = match with_obj(*r, |o| o.slots[1].clone()) {
        Value::Ref(Some(b)) => b,
        Value::Ref(None) => return Err(throw_new("java/io/IOException", Some("Stream closed"))),
        _ => return Err(internal_error(&format!("j2r internal: {op} buf slot"))),
    };
    let inner = match with_obj(*r, |o| o.slots[0].clone()) {
        Value::Ref(Some(s)) => s,
        Value::Ref(None) => return Err(throw_new("java/io/IOException", Some("Stream closed"))),
        _ => return Err(internal_error(&format!("j2r internal: {op} in slot"))),
    };
    let pos = with_obj(*r, |o| match o.slots[2] {
        Value::Int(v) => v,
        _ => panic!("j2r internal: {op} pos slot"),
    });
    Ok((buf, inner, pos))
}
fn pb_set_pos(r: &JRef, pos: i32) {
    with_obj_mut(*r, |o| o.slots[2] = Value::Int(pos));
}
pub fn n_pb_init(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (r, inner) = stream_field_pair(recv, args, "PushbackInputStream.<init>")?;
    // JDK: buf = new byte[1], pos = 1
    let buf = crate::array::array_new_prim("[B", 1)?;
    with_obj_mut(r, |o| {
        o.slots[0] = Value::Ref(Some(inner));
        o.slots[1] = Value::Ref(Some(buf));
        o.slots[2] = Value::Int(1);
    });
    Ok(Value::Void)
}
pub fn n_pb_init_size(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (r, inner) = stream_field_pair(recv, args, "PushbackInputStream.<init>")?;
    let size = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(missing("PushbackInputStream.<init>")),
    };
    if size <= 0 {
        return Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some("Buffer size <= 0"),
        ));
    }
    let buf = crate::array::array_new_prim("[B", size)?;
    with_obj_mut(r, |o| {
        o.slots[0] = Value::Ref(Some(inner));
        o.slots[1] = Value::Ref(Some(buf));
        o.slots[2] = Value::Int(size);
    });
    Ok(Value::Void)
}
pub fn n_pb_read(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("PushbackInputStream.read")),
    };
    let (buf, inner, pos) = pb_parts(&r, "PushbackInputStream.read")?;
    let blen = crate::array::array_length(&buf)?;
    if pos < blen {
        let v = bis_load_byte(&buf, pos, "PushbackInputStream.read")?;
        pb_set_pos(&r, pos + 1);
        return Ok(Value::Int(v));
    }
    crate::invoke_virtual(&inner, "java/io/InputStream", "read", "()I", &[], ctx)
}
pub fn n_pb_read_range(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("PushbackInputStream.read")),
    };
    let _ = pb_parts(&r, "PushbackInputStream.read")?; // closed checks
    let (b, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(b))), Some(Value::Int(off)), Some(Value::Int(len))) => {
            (*b, *off, *len)
        }
        (Some(Value::Ref(None)), ..)
        | (_, Some(Value::Ref(None)), _)
        | (_, _, Some(Value::Ref(None))) => return Err(crate::npe()),
        _ => return Err(missing("PushbackInputStream.read")),
    };
    let blen_out = crate::array::array_length(&b)?;
    if off < 0 || len < 0 || off > blen_out - len {
        return Err(throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    if len == 0 {
        return Ok(Value::Int(0));
    }
    let (buf, inner, pos) = pb_parts(&r, "PushbackInputStream.read")?;
    let blen = crate::array::array_length(&buf)?;
    let mut avail = blen - pos;
    if avail > 0 {
        if len < avail {
            avail = len;
        }
        for k in 0..avail {
            let v = bis_load_byte(&buf, pos + k, "PushbackInputStream.read")?;
            crate::array::array_store(&b, off + k, Value::Int(v), crate::Dispatch::default())?;
        }
        pb_set_pos(&r, pos + avail);
    }
    let rest = len - avail;
    if rest > 0 {
        let n = crate::invoke_virtual(
            &inner,
            "java/io/InputStream",
            "read",
            "([BII)I",
            &[
                Value::Ref(Some(b)),
                Value::Int(off + avail),
                Value::Int(rest),
            ],
            ctx,
        )?;
        if let Value::Int(v) = n {
            if v < 0 {
                // EOF from the inner stream: the pushed bytes still count
                return Ok(Value::Int(if avail == 0 { -1 } else { avail }));
            }
            return Ok(Value::Int(avail + v));
        }
        return Err(missing("PushbackInputStream.read inner"));
    }
    Ok(Value::Int(avail))
}
pub fn n_pb_unread(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("PushbackInputStream.unread")),
    };
    let (buf, _inner, pos) = pb_parts(&r, "PushbackInputStream.unread")?;
    let blen = crate::array::array_length(&buf)?;
    if pos == 0 {
        return Err(throw_new(
            "java/io/IOException",
            Some("Push back buffer is full"),
        ));
    }
    let b = match args.first() {
        Some(Value::Int(v)) => *v as u8,
        _ => return Err(missing("PushbackInputStream.unread")),
    };
    let new_pos = pos - 1;
    crate::array::array_store(
        &buf,
        new_pos,
        Value::Int(b as i8 as i32),
        crate::Dispatch::default(),
    )?;
    pb_set_pos(&r, new_pos);
    let _ = blen;
    Ok(Value::Void)
}
pub fn n_pb_unread_bulk(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    // unread(byte[] b) == unread(b, 0, b.length)
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("PushbackInputStream.unread")),
    };
    let b = first_arg_ref!(args, Err(missing("PushbackInputStream.unread")));
    let len = crate::array::array_length(&b)?;
    let wide = [args[0].clone(), Value::Int(0), Value::Int(len)];
    n_pb_unread_range(Some(&r), &wide, crate::Dispatch::default())
}
pub fn n_pb_unread_range(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("PushbackInputStream.unread")),
    };
    let (buf, _inner, pos) = pb_parts(&r, "PushbackInputStream.unread")?;
    let (b, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(b))), Some(Value::Int(off)), Some(Value::Int(len))) => {
            (*b, *off, *len)
        }
        _ => return Err(missing("PushbackInputStream.unread")),
    };
    if len > pos {
        return Err(throw_new(
            "java/io/IOException",
            Some("Push back buffer is full"),
        ));
    }
    let new_pos = pos - len;
    for k in 0..len {
        let v = bis_load_byte(&b, off + k, "PushbackInputStream.unread")?;
        crate::array::array_store(
            &buf,
            new_pos + k,
            Value::Int(v as i8 as i32),
            crate::Dispatch::default(),
        )?;
    }
    pb_set_pos(&r, new_pos);
    Ok(Value::Void)
}
pub fn n_pb_skip(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("PushbackInputStream.skip")),
    };
    let (buf, inner, pos) = pb_parts(&r, "PushbackInputStream.skip")?;
    let blen = crate::array::array_length(&buf)?;
    let n = match args.first() {
        Some(Value::Long(v)) => *v,
        _ => return Err(missing("PushbackInputStream.skip")),
    };
    if n <= 0 {
        return Ok(Value::Long(0));
    }
    let mut n = n;
    let mut pskip = (blen - pos) as i64;
    if pskip > 0 {
        if n < pskip {
            pskip = n;
        }
        pb_set_pos(&r, pos + pskip as i32);
        n -= pskip;
    }
    if n > 0 {
        let skipped = crate::invoke_virtual(
            &inner,
            "java/io/InputStream",
            "skip",
            "(J)J",
            &[Value::Long(n)],
            ctx,
        )?;
        if let Value::Long(v) = skipped {
            pskip += v;
        }
    }
    Ok(Value::Long(pskip))
}
pub fn n_pb_available(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("PushbackInputStream.available")),
    };
    let (buf, inner, pos) = pb_parts(&r, "PushbackInputStream.available")?;
    let blen = crate::array::array_length(&buf)?;
    let inner_avail =
        crate::invoke_virtual(&inner, "java/io/InputStream", "available", "()I", &[], ctx)?;
    match inner_avail {
        Value::Int(v) => Ok(Value::Int(v + (blen - pos))),
        _ => Err(missing("PushbackInputStream.available inner")),
    }
}
pub fn n_pb_mark_supported(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(0)) // JDK: false; reset falls through to the InputStream default
}
pub fn n_pb_close(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("PushbackInputStream.close")),
    };
    with_obj_mut(r, |o| {
        o.slots[0] = Value::Ref(None);
        o.slots[1] = Value::Ref(None);
    });
    Ok(Value::Void)
}
