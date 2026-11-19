use super::*;

pub fn n_buf_init(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (r, inner) = stream_field_pair(recv, args, "BufferedInputStream.<init>")?;
    // JDK: buf = new byte[8192], pos = count = 0, markpos = -1, marklimit = 0.
    let buf = crate::array::array_new_prim("[B", 8192)?;
    with_obj_mut(r, |o| {
        o.slots[0] = Value::Ref(Some(inner));
        o.slots[1] = Value::Ref(Some(buf));
        o.slots[2] = Value::Int(0);
        o.slots[3] = Value::Int(0);
        o.slots[4] = Value::Int(-1);
        o.slots[5] = Value::Int(0);
    });
    Ok(Value::Void)
}
pub fn n_buf_init_size(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let (r, inner) = stream_field_pair(recv, args, "BufferedInputStream.<init>")?;
    let size = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(missing("BufferedInputStream.<init>")),
    };
    if size <= 0 {
        // JDK: IllegalArgumentException("Buffer size <= 0")
        return Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some("Buffer size <= 0"),
        ));
    }
    let buf = crate::array::array_new_prim("[B", size)?;
    with_obj_mut(r, |o| {
        o.slots[0] = Value::Ref(Some(inner));
        o.slots[1] = Value::Ref(Some(buf));
        o.slots[2] = Value::Int(0);
        o.slots[3] = Value::Int(0);
        o.slots[4] = Value::Int(-1);
        o.slots[5] = Value::Int(0);
    });
    Ok(Value::Void)
}
/// JDK `fill()`: mark-region shift, buffer growth, then one inner bulk read
/// of `buffer.length - pos` (NOT the marklimit — the first draft read
/// `lim - pos` and the probe caught it). Temurin-pinned shape (all rows of
/// corpus/BufferedProbe.java reproduce): markpos < 0 -> drop the buffer;
/// markpos > 0 -> shift the marked region to the front (this ALSO covers
/// the exhausted-buffer case markpos == pos, where the shift is empty but
/// re-anchors markpos to 0); markpos == 0 && pos == buffer.length ->
/// grow when marklimit fits, else invalidate the mark.
pub(crate) fn bis_fill(r: &JRef, ctx: Dispatch) -> Result<(), Throw> {
    let inner = bis_inner(r, "BufferedInputStream.fill")?;
    let buf0 = bis_buf(r, "BufferedInputStream.fill")?;
    let blen = crate::array::array_length(&buf0)?;
    let (mut pos, _count0, mut markpos, marklimit) = bis_state(r);
    let mut buf = buf0;
    if markpos < 0 {
        pos = 0;
    } else {
        if markpos > 0 {
            // shift the marked region to the front (cnt may be 0 — the
            // exhausted-buffer case — but markpos still re-anchors to 0)
            let cnt = pos - markpos;
            for i in 0..cnt {
                let v = crate::array::array_load(&buf, markpos + i)?;
                crate::array::array_store(&buf, i, v, crate::Dispatch::default())?;
            }
            pos -= markpos;
            markpos = 0;
        } else if pos >= blen {
            if marklimit >= blen {
                // grow: JDK nsz = min(pos+pos+marklimit, marklimit) = marklimit
                let nbuf = crate::array::array_new_prim("[B", marklimit)?;
                for i in 0..pos {
                    let v = crate::array::array_load(&buf, i)?;
                    crate::array::array_store(&nbuf, i, v, crate::Dispatch::default())?;
                }
                buf = nbuf;
                with_obj_mut(*r, |o| o.slots[1] = Value::Ref(Some(buf)));
            } else {
                // the mark region cannot fit: invalidate
                markpos = -1;
                pos = 0;
            }
        }
    }
    let mut count = pos;
    bis_set(r, pos, count, markpos, marklimit, None);
    let new_blen = crate::array::array_length(&buf)?;
    let n = crate::invoke_virtual(
        &inner,
        "java/io/InputStream",
        "read",
        "([BII)I",
        &[
            Value::Ref(Some(buf)),
            Value::Int(pos),
            Value::Int(new_blen - pos),
        ],
        ctx,
    )?;
    if let Value::Int(v) = n {
        if v > 0 {
            count = v + pos;
        }
    }
    bis_set(r, pos, count, markpos, marklimit, None);
    Ok(())
}
/// JDK `read1`: one buffer-or-direct read (direct bypass when `len` >=
/// marklimit, clearing the mark — pinned by the readlimit-invalidation row).
pub(crate) fn bis_read1(
    r: &JRef,
    b: &JRef,
    off: i32,
    len: i32,
    ctx: Dispatch,
) -> Result<i32, Throw> {
    let inner = bis_inner(r, "BufferedInputStream.read1")?;
    let _ = bis_buf(r, "BufferedInputStream.read1")?; // closed check
    let (pos, count, _markpos, marklimit) = bis_state(r);
    let avail = count - pos;
    if avail <= 0 {
        if len >= marklimit {
            // big read bypasses the buffer and invalidates the mark
            bis_set(r, pos, count, -1, marklimit, None);
            let n = crate::invoke_virtual(
                &inner,
                "java/io/InputStream",
                "read",
                "([BII)I",
                &[Value::Ref(Some(*b)), Value::Int(off), Value::Int(len)],
                ctx,
            )?;
            return match n {
                Value::Int(v) => Ok(v),
                _ => Err(missing("BufferedInputStream.read1")),
            };
        }
        bis_fill(r, ctx)?;
        let (pos, count, ..) = bis_state(r);
        if count - pos <= 0 {
            return Ok(-1);
        }
    }
    let (pos, count, ..) = bis_state(r);
    let buf = bis_buf(r, "BufferedInputStream.read1")?;
    let cnt = std::cmp::min(count - pos, len);
    for i in 0..cnt {
        let v = crate::array::array_load(&buf, pos + i)?;
        crate::array::array_store(b, off + i, v, crate::Dispatch::default())?;
    }
    let (.., markpos, marklimit) = bis_state(r);
    bis_set(r, pos + cnt, count, markpos, marklimit, None);
    Ok(cnt)
}
pub(crate) fn bis_load_byte(buf: &JRef, idx: i32, op: &str) -> Result<i32, Throw> {
    match crate::array::array_load(buf, idx)? {
        Value::Int(x) => Ok(x & 0xff),
        _ => Err(missing(op)),
    }
}
pub fn n_buf_read(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("BufferedInputStream.read")),
    };
    let _ = bis_buf(&r, "BufferedInputStream.read")?; // closed check
    let (pos, count, markpos, marklimit) = bis_state(&r);
    if pos < count {
        let buf = bis_buf(&r, "BufferedInputStream.read")?;
        let v = bis_load_byte(&buf, pos, "BufferedInputStream.read")?;
        bis_set(&r, pos + 1, count, markpos, marklimit, None);
        return Ok(Value::Int(v));
    }
    bis_fill(&r, ctx)?;
    let (pos, count, markpos, marklimit) = bis_state(&r);
    if pos >= count {
        return Ok(Value::Int(-1));
    }
    let buf = bis_buf(&r, "BufferedInputStream.read")?;
    let v = bis_load_byte(&buf, pos, "BufferedInputStream.read")?;
    bis_set(&r, pos + 1, count, markpos, marklimit, None);
    Ok(Value::Int(v))
}
pub fn n_buf_read_range(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("BufferedInputStream.read")),
    };
    let _ = bis_buf(&r, "BufferedInputStream.read")?; // closed check
    let (b, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(b))), Some(Value::Int(off)), Some(Value::Int(len))) => {
            (*b, *off, *len)
        }
        (Some(Value::Ref(None)), ..)
        | (_, Some(Value::Ref(None)), _)
        | (_, _, Some(Value::Ref(None))) => return Err(crate::npe()),
        _ => return Err(missing("BufferedInputStream.read")),
    };
    let blen = crate::array::array_length(&b)?;
    // JDK: Objects.checkFromIndexRange(off, len, b.length) — the throw is
    // pinned, the exact message is not (out of scope).
    if off < 0 || len < 0 || off > blen - len {
        return Err(throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    if len == 0 {
        return Ok(Value::Int(0));
    }
    let mut n = bis_read1(&r, &b, off, len, ctx)?;
    if n > 0 {
        while n < len {
            // JDK: `in != null` check — a closed inner stream stops the loop
            if bis_inner(&r, "BufferedInputStream.read").is_err() {
                break;
            }
            let n1 = bis_read1(&r, &b, off + n, len - n, ctx)?;
            if n1 <= 0 {
                break;
            }
            n += n1;
        }
    }
    Ok(Value::Int(n))
}
pub fn n_buf_skip(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("BufferedInputStream.skip")),
    };
    let _ = bis_buf(&r, "BufferedInputStream.skip")?; // closed check
    let n = match args.first() {
        Some(Value::Long(v)) => *v,
        _ => return Err(missing("BufferedInputStream.skip")),
    };
    if n <= 0 {
        return Ok(Value::Long(0));
    }
    let (pos, count, markpos, marklimit) = bis_state(&r);
    let avail = (count - pos) as i64;
    if n <= avail {
        bis_set(&r, pos + n as i32, count, markpos, marklimit, None);
        return Ok(Value::Long(n));
    }
    // JDK: pos = count — the buffered region is consumed even though the
    // skip only returned the buffer part (Temurin-pinned: the next read
    // refills from the inner stream at the right position).
    let read = avail;
    bis_set(&r, count, count, markpos, marklimit, None);
    // Temurin-pinned surprise: with a PARTIALLY consumed buffer the skip
    // stops here (no inner drain) — skip(20) over 7 buffered bytes = 7.
    // A skip on an EMPTY buffer (read == 0, no mark) drains the inner
    // stream, clamped to its available() (skip(100) over 26 = 26).
    if read == 0 && markpos < 0 {
        let inner = bis_inner(&r, "BufferedInputStream.skip")?;
        let want = n - read;
        let avail2 = match crate::invoke_virtual(
            &inner,
            "java/io/InputStream",
            "available",
            "()I",
            &[],
            ctx,
        )? {
            Value::Int(v) => v as i64,
            _ => return Err(missing("BufferedInputStream.skip available")),
        };
        let avail2 = if avail2 > want { want } else { avail2 };
        if avail2 > 0 {
            let skipped = crate::invoke_virtual(
                &inner,
                "java/io/InputStream",
                "skip",
                "(J)J",
                &[Value::Long(avail2)],
                ctx,
            )?;
            if let Value::Long(v) = skipped {
                return Ok(Value::Long(read + v));
            }
            return Err(missing("BufferedInputStream.skip inner result"));
        }
    }
    Ok(Value::Long(read))
}
pub fn n_buf_available(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("BufferedInputStream.available")),
    };
    let inner = bis_inner(&r, "BufferedInputStream.available")?;
    let (pos, count, ..) = bis_state(&r);
    let inner_avail =
        crate::invoke_virtual(&inner, "java/io/InputStream", "available", "()I", &[], ctx)?;
    match inner_avail {
        Value::Int(v) => Ok(Value::Int(v + (count - pos))),
        _ => Err(missing("BufferedInputStream.available inner")),
    }
}
pub fn n_buf_mark(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("BufferedInputStream.mark")),
    };
    let _ = bis_buf(&r, "BufferedInputStream.mark")?;
    let readlimit = match args.first() {
        Some(Value::Int(v)) => *v,
        _ => return Err(missing("BufferedInputStream.mark")),
    };
    let (pos, count, ..) = bis_state(&r);
    bis_set(&r, pos, count, pos, readlimit, None);
    Ok(Value::Void)
}
pub fn n_buf_reset(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("BufferedInputStream.reset")),
    };
    let _ = bis_buf(&r, "BufferedInputStream.reset")?;
    let (_pos, count, markpos, marklimit) = bis_state(&r);
    if markpos < 0 {
        // JDK: IOException("Resetting to invalid mark")
        return Err(throw_new(
            "java/io/IOException",
            Some("Resetting to invalid mark"),
        ));
    }
    bis_set(&r, markpos, count, markpos, marklimit, None);
    Ok(Value::Void)
}
pub fn n_buf_mark_supported(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(1))
}
pub fn n_buf_close(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("BufferedInputStream.close")),
    };
    // JDK: in = null; buf = null — the null slots ARE the closed state.
    with_obj_mut(r, |o| {
        o.slots[0] = Value::Ref(None);
        o.slots[1] = Value::Ref(None);
    });
    Ok(Value::Void)
}

// ---------------------------------------------------------------------------
// PushbackInputStream (stream wave 4 — the last unmodelled stream builtin).
// Slots: 0 = in (FilterInputStream), 1 = buf (pushback byte[] ref),
// 2 = pos (next pushback byte to READ; starts at buf.length; unread
// decrements). Temurin-pinned by corpus/PushbackProbe.java: unread past
// the buffer -> IOException("Push back buffer is full"), skip consumes the
// pushback region FIRST then inner.skip, markSupported false (reset falls
// through to the InputStream default "mark/reset not supported"),
// available() = pushback + inner, close() nulls in/buf ("Stream closed").
// ---------------------------------------------------------------------------
