use super::*;

const SR_TEXT: usize = 1;
const SR_POS: usize = 2;
const SR_MARK: usize = 3;
const SR_CLOSED: usize = 4;
/// The text's UTF-16 units + current pos, cloned out of the heap so the
/// object lock is not held across read loops.
fn sr_state(r: &JRef) -> (Arc<String>, i32) {
    with_obj(*r, |o| {
        let text = match &o.slots[SR_TEXT] {
            Value::Str(s) => s.clone(),
            other => panic!("j2r internal: StringReader text {other:?}"),
        };
        let pos = match &o.slots[SR_POS] {
            Value::Int(v) => *v,
            other => panic!("j2r internal: StringReader pos {other:?}"),
        };
        (text, pos)
    })
}
fn sr_set_pos(r: &JRef, pos: i32) {
    with_obj_mut(*r, |o| o.slots[SR_POS] = Value::Int(pos));
}
fn sr_state_int(r: &JRef, slot: usize) -> i32 {
    with_obj(*r, |o| match &o.slots[slot] {
        Value::Int(v) => *v,
        other => panic!("j2r internal: StringReader slot {slot} is {other:?}"),
    })
}
fn sr_ensure_open(r: &JRef) -> Result<(), Throw> {
    if sr_state_int(r, SR_CLOSED) != 0 {
        return Err(throw_new("java/io/IOException", Some("Stream closed")));
    }
    Ok(())
}
pub fn n_sr_init(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("StringReader.<init>")),
    };
    let s = match args.first() {
        Some(Value::Ref(Some(o))) if crate::object::is_string(o) => {
            crate::object::string_value(o).to_string()
        }
        Some(Value::Ref(None)) => return Err(crate::npe()),
        _ => return Err(missing("StringReader.<init>")),
    };
    with_obj_mut(r, |o| {
        o.slots[0] = Value::Ref(Some(r));
        o.slots[SR_TEXT] = Value::Str(std::sync::Arc::new(s));
        o.slots[SR_POS] = Value::Int(0);
        o.slots[SR_MARK] = Value::Int(0);
        o.slots[SR_CLOSED] = Value::Int(0);
    });
    Ok(Value::Void)
}
/// `Reader.read()` — one UTF-16 code unit, -1 at EOF (JDK law: surrogate
/// pairs arrive as two reads; the old scalar convention is retired).
pub fn n_sr_read(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("StringReader.read")),
    };
    sr_ensure_open(&r)?;
    let (text, pos) = sr_state(&r);
    let units: Vec<u16> = text.encode_utf16().collect();
    if pos as usize >= units.len() {
        return Ok(Value::Int(-1));
    }
    sr_set_pos(&r, pos + 1);
    Ok(Value::Int(units[pos as usize] as i32))
}
/// `Reader.read(char[], off, len)` — UTF-16 code units, count or -1.
pub fn n_sr_read_bulk(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("StringReader.read")),
    };
    sr_ensure_open(&r)?;
    let (cbuf, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(a))), Some(o), Some(l)) => (*a, o.expect_int(), l.expect_int()),
        (Some(Value::Ref(None)), _, _) => return Err(crate::npe()),
        _ => return Err(missing("StringReader.read")),
    };
    // JDK: plain IndexOutOfBoundsException carrying the modern
    // Objects.checkFromIndexRange message, checked BEFORE the len==0
    // short-circuit (Temurin-21-empirical, pinned by StringIoSemantics).
    let alen = crate::array::array_length(&cbuf)?;
    if off < 0 || len < 0 || off > alen || off + len > alen || off + len < 0 {
        return Err(throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&format!(
                "Range [{off}, {off} + {len}) out of bounds for length {alen}"
            )),
        ));
    }
    if len == 0 {
        return Ok(Value::Int(0));
    }
    let (text, pos) = sr_state(&r);
    let units: Vec<u16> = text.encode_utf16().collect();
    let avail = units.len() as i32 - pos;
    if avail <= 0 {
        return Ok(Value::Int(-1));
    }
    let n = len.min(avail);
    for i in 0..n {
        let v = units[(pos + i) as usize] as i32;
        crate::array::array_store(&cbuf, off + i, Value::Int(v), ctx)?;
    }
    sr_set_pos(&r, pos + n);
    Ok(Value::Int(n))
}
/// `Reader.skip(long)` — UTF-16 units, clamped to the remaining count.
/// Temurin-21-EMPIRICAL law (the probe caught it): a NEGATIVE n returns 0
/// and does NOT throw — the old IllegalArgumentException("skip value is
/// negative") is gone from the modern JDK (the Java-13-era CSR change).
pub fn n_sr_skip(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("StringReader.skip")),
    };
    sr_ensure_open(&r)?;
    let n = match args.first() {
        Some(v) => v.expect_long(),
        None => return Err(missing("StringReader.skip")),
    };
    if n <= 0 {
        return Ok(Value::Long(0));
    }
    let (text, pos) = sr_state(&r);
    let total = text.encode_utf16().count() as i64;
    let moved = n.min(total - pos as i64);
    sr_set_pos(&r, pos + moved as i32);
    Ok(Value::Long(moved))
}
/// `Reader.ready()` — true unless closed.
pub fn n_sr_ready(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("StringReader.ready")),
    };
    sr_ensure_open(&r)?;
    Ok(Value::Int(1))
}
/// `Reader.markSupported()` — StringReader supports mark/reset. The modern
/// JDK is a plain `return true` — NO ensureOpen (it declares no checked
/// exception, so close() cannot affect it; pinned by the cms_closed row).
pub fn n_sr_mark_supported(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = match recv {
        Some(r) => *r,
        None => return Err(missing("StringReader.markSupported")),
    };
    Ok(Value::Int(1))
}
/// `Reader.mark(int)` — records the current unit offset; negative limits
/// are rejected ("Read-ahead limit < 0" — still live on Temurin 21).
pub fn n_sr_mark(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("StringReader.mark")),
    };
    sr_ensure_open(&r)?;
    let limit = match args.first() {
        Some(v) => v.expect_int(),
        None => return Err(missing("StringReader.mark")),
    };
    if limit < 0 {
        return Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some("Read-ahead limit < 0"),
        ));
    }
    let pos = sr_state_int(&r, SR_POS);
    with_obj_mut(r, |o| o.slots[SR_MARK] = Value::Int(pos));
    Ok(Value::Void)
}
/// `Reader.reset()` — restores the marked offset. Temurin-21-EMPIRICAL
/// law (the probe caught it): mark initializes to 0, so reset() before
/// any mark() resets to the BEGINNING and does NOT throw — the old
/// IOException("Mark invalid") is gone from the modern JDK.
pub fn n_sr_reset(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("StringReader.reset")),
    };
    sr_ensure_open(&r)?;
    let mark = sr_state_int(&r, SR_MARK);
    sr_set_pos(&r, mark);
    Ok(Value::Void)
}
/// `StringReader.close()` — idempotent; every subsequent operation throws
/// IOException("Stream closed") (the BufferedInputStream closed-flag law).
pub fn n_sr_close(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("StringReader.close")),
    };
    with_obj_mut(r, |o| o.slots[SR_CLOSED] = Value::Int(1));
    Ok(Value::Void)
}

macro_rules! abstract_reader_marker {
    ($name:ident, $sig:expr) => {
        pub fn $name(
            _recv: Option<&JRef>,
            _args: &[Value],
            _ctx: Dispatch,
        ) -> Result<Value, Throw> {
            Err(internal_error(concat!(
                "j2r internal: abstract java/io/Reader.",
                $sig,
                " dispatched — every concrete Reader must override it"
            )))
        }
    };
}

abstract_reader_marker!(n_sr_read_marker, "read()I");
abstract_reader_marker!(n_sr_read_bulk_marker, "read([CII)I");

macro_rules! abstract_writer_marker {
    ($name:ident, $sig:expr) => {
        pub fn $name(
            _recv: Option<&JRef>,
            _args: &[Value],
            _ctx: Dispatch,
        ) -> Result<Value, Throw> {
            Err(internal_error(concat!(
                "j2r internal: abstract java/io/Writer.",
                $sig,
                " dispatched — every concrete Writer must override it"
            )))
        }
    };
}

abstract_writer_marker!(n_writer_write_int_marker, "write(I)V");
abstract_writer_marker!(n_writer_write_chars_marker, "write([CII)V");
abstract_writer_marker!(n_writer_write_str_range_marker, "write(String,II)V");
pub fn n_fos_init(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (r, out) = stream_field_pair(recv, args, "FilterOutputStream.<init>")?;
    with_obj_mut(r, |o| o.slots[0] = Value::Ref(Some(out)));
    Ok(Value::Void)
}
pub fn n_fos_write_int(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (_, out) = stream_field(recv, "FilterOutputStream.write")?;
    crate::invoke_virtual(&out, "java/io/OutputStream", "write", "(I)V", args, ctx)?;
    Ok(Value::Void)
}
/// JDK: `write(b, 0, b.length)` ON SELF — a virtual self-call so a user
/// override of `write([BII)` wins.
pub fn n_fos_write_bulk(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let (r, _) = stream_field(recv, "FilterOutputStream.write")?;
    let b = first_arg_ref!(args, Err(missing("FilterOutputStream.write")));
    let len = crate::array::array_length(&b)?;
    crate::invoke_virtual(
        &r,
        "java/io/FilterOutputStream",
        "write",
        "([BII)V",
        &[Value::Ref(Some(b)), Value::Int(0), Value::Int(len)],
        ctx,
    )?;
    Ok(Value::Void)
}
/// JDK 21 FilterOutputStream.write([BII): requireNonNull, the packed
/// range check, then the per-byte loop THROUGH `out` — every byte is one
/// virtual `out.write(I)` (the documented behavior programs depend on).
pub fn n_fos_write_range(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let (_, out) = stream_field(recv, "FilterOutputStream.write")?;
    let b = first_arg_ref!(args, Err(missing("FilterOutputStream.write")));
    let off = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(missing("FilterOutputStream.write")),
    };
    let len = match args.get(2) {
        Some(Value::Int(v)) => *v,
        _ => return Err(missing("FilterOutputStream.write")),
    };
    check_write_range(&b, off, len, "FilterOutputStream")?;
    for i in off..off + len {
        let v = crate::array::array_load(&b, i)?;
        crate::invoke_virtual(&out, "java/io/OutputStream", "write", "(I)V", &[v], ctx)?;
    }
    Ok(Value::Void)
}
pub fn n_fos_flush(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (_, out) = stream_field(recv, "FilterOutputStream.flush")?;
    crate::invoke_virtual(&out, "java/io/OutputStream", "flush", "()V", &[], ctx)?;
    Ok(Value::Void)
}
/// JDK: `try { flush(); } finally { out.close(); }`.
pub fn n_fos_close(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (_, out) = stream_field(recv, "FilterOutputStream.close")?;
    let flush = crate::invoke_virtual(&out, "java/io/OutputStream", "flush", "()V", &[], ctx);
    let close = crate::invoke_virtual(&out, "java/io/OutputStream", "close", "()V", &[], ctx);
    flush?;
    close?;
    Ok(Value::Void)
}

// Abstract OutputStream: markers for the abstract writes (dispatching to
// them means a concrete subclass failed to override), JDK defaults for the
// rest.
pub fn n_fis_init(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (r, inner) = stream_field_pair(recv, args, "FilterInputStream.<init>")?;
    with_obj_mut(r, |o| o.slots[0] = Value::Ref(Some(inner)));
    Ok(Value::Void)
}
pub fn n_fis_read(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (_, inner) = stream_field(recv, "FilterInputStream.read")?;
    crate::invoke_virtual(&inner, "java/io/InputStream", "read", "()I", &[], ctx)
}
/// JDK: `read(b, 0, b.length)` on SELF.
pub fn n_fis_read_bulk(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (r, _) = stream_field(recv, "FilterInputStream.read")?;
    let b = first_arg_ref!(args, Err(missing("FilterInputStream.read")));
    let len = crate::array::array_length(&b)?;
    crate::invoke_virtual(
        &r,
        "java/io/FilterInputStream",
        "read",
        "([BII)I",
        &[Value::Ref(Some(b)), Value::Int(0), Value::Int(len)],
        ctx,
    )
}
pub fn n_fis_read_range(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let (_, inner) = stream_field(recv, "FilterInputStream.read")?;
    crate::invoke_virtual(&inner, "java/io/InputStream", "read", "([BII)I", args, ctx)
}
pub fn n_fis_skip(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (_, inner) = stream_field(recv, "FilterInputStream.skip")?;
    crate::invoke_virtual(&inner, "java/io/InputStream", "skip", "(J)J", args, ctx)
}
pub fn n_fis_available(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let (_, inner) = stream_field(recv, "FilterInputStream.available")?;
    crate::invoke_virtual(&inner, "java/io/InputStream", "available", "()I", &[], ctx)
}
pub fn n_fis_close(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (_, inner) = stream_field(recv, "FilterInputStream.close")?;
    crate::invoke_virtual(&inner, "java/io/InputStream", "close", "()V", &[], ctx)?;
    Ok(Value::Void)
}

// Abstract InputStream: JDK defaults.
pub fn n_is_read(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Err(internal_error(
        "j2r: InputStream.read() is abstract — a concrete subclass must override it",
    ))
}
/// JDK InputStream.read([B) default: `read(b, 0, b.length)` on SELF.
pub fn n_is_read_bulk(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("InputStream.read")),
    };
    let b = first_arg_ref!(args, Err(missing("InputStream.read")));
    let len = crate::array::array_length(&b)?;
    crate::invoke_virtual(
        &r,
        "java/io/InputStream",
        "read",
        "([BII)I",
        &[Value::Ref(Some(b)), Value::Int(0), Value::Int(len)],
        ctx,
    )
}
pub fn n_is_read_range(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    // JDK `InputStream.read([BII)I` default — CONCRETE in the JDK (the old
    // loud marker was wrong: a read()-only subclass inherits it, and the
    // skip default's loop-read depends on it). Bounds check, then the
    // JDK loop: first read() EOF -> -1, else fill until len or EOF.
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("InputStream.read")),
    };
    let (b, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(b))), Some(Value::Int(off)), Some(Value::Int(len))) => {
            (*b, *off, *len)
        }
        (Some(Value::Ref(None)), ..)
        | (_, Some(Value::Ref(None)), _)
        | (_, _, Some(Value::Ref(None))) => return Err(crate::npe()),
        _ => return Err(missing("InputStream.read")),
    };
    let blen = crate::array::array_length(&b)?;
    // JDK: Objects.checkFromIndexRange(off, len, b.length). The exact
    // message is not pinned by the fixture (out of scope) — the throw is.
    if off < 0 || len < 0 || off > blen - len {
        return Err(throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    if len == 0 {
        return Ok(Value::Int(0));
    }
    let first = match crate::invoke_virtual(&r, "java/io/InputStream", "read", "()I", &[], ctx)? {
        Value::Int(c) => c,
        _ => return Err(missing("InputStream.read")),
    };
    if first == -1 {
        return Ok(Value::Int(-1));
    }
    crate::array::array_store(
        &b,
        off,
        Value::Int(first as i8 as i32),
        crate::Dispatch::default(),
    )?;
    let mut i = 1;
    while i < len {
        let c = match crate::invoke_virtual(&r, "java/io/InputStream", "read", "()I", &[], ctx)? {
            Value::Int(c) => c,
            _ => return Err(missing("InputStream.read")),
        };
        if c == -1 {
            break;
        }
        crate::array::array_store(
            &b,
            off + i,
            Value::Int(c as i8 as i32),
            crate::Dispatch::default(),
        )?;
        i += 1;
    }
    Ok(Value::Int(i))
}
pub fn n_is_available(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(0)) // JDK default
}
pub fn n_is_close(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Void) // JDK default
}
pub fn n_is_mark(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Void) // JDK default: no-op
}
pub fn n_is_reset(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Err(throw_new(
        "java/io/IOException",
        Some("mark/reset not supported"),
    ))
}
pub fn n_is_mark_supported(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(0)) // JDK default: false
}

// -------------------------------------- ByteArrayOutputStream (JZlib)
//
// Corpus wave 2: Inflate.readString/readBytes collect bytes into a BAOS.
// JDK field model in the slots (slot 0 = buf byte[] ref, slot 1 = count);
// growth policy is internal (capacity is NOT observable from Java — only
// buf contents [0..count), count, and the copies are), so a plain doubling
// growth with the JDK's minimum-capacity rule stands in for the exact
// MAX_ARRAY_SIZE overflow choreography.
/// JDK `InputStream.skip(J)` default (was a loud registered divergence): the
/// JDK loop-reads through a skip buffer via `read([BII)I` ON SELF, so a
/// subclass's bulk read drives it; EOF on the first read yields 0, a
/// partial drain yields the actual count (Temurin-pinned by
/// corpus/SkipProbe.java with a read()-only stream). MAX_SKIP_BUFFER_SIZE is
/// 2048 in the JDK. `n <= 0` returns 0 without touching the stream.
pub fn n_is_skip_default(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("InputStream.skip")),
    };
    let n = match args.first() {
        Some(Value::Long(v)) => *v,
        _ => return Err(missing("InputStream.skip")),
    };
    if n <= 0 {
        return Ok(Value::Long(0));
    }
    let buf = crate::array::array_new_prim("[B", 2048)?;
    let mut remaining = n;
    while remaining > 0 {
        let want = std::cmp::min(2048i64, remaining) as i32;
        let nr = crate::invoke_virtual(
            &r,
            "java/io/InputStream",
            "read",
            "([BII)I",
            &[Value::Ref(Some(buf)), Value::Int(0), Value::Int(want)],
            ctx,
        )?;
        match nr {
            Value::Int(v) if v < 0 => break,
            Value::Int(v) => remaining -= v as i64,
            _ => return Err(missing("InputStream.skip read result")),
        }
    }
    Ok(Value::Long(n - remaining))
}

// ---------------------------------------------------------------------------
// BufferedInputStream (the last missing stream builtin; the skip/charset
// wave's successor). Slots: 0 = in (FilterInputStream), 1 = buf (byte[]
// ref), 2 = pos, 3 = count, 4 = markpos, 5 = marklimit. Temurin-pinned by
// corpus/BufferedProbe.java — including the surprise that skip() with a
// PARTIALLY consumed buffer consumes ONLY the buffer (no inner drain:
// skip(20) over 7 buffered bytes returns 7), while a skip on an EMPTY
// buffer drains the inner stream (skip(100) returns 26). close() nulls
// buf/in; every getter then throws IOException("Stream closed").
// ---------------------------------------------------------------------------
