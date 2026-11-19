use super::*;

const STRINGWRITER_PAYLOAD: usize = 1;
/// `StringWriter()` / the object-model init: slot 0 is Writer's inherited
/// `lock` (the JDK sets it to the writer itself), slot 1 the payload.
pub fn n_sw_init(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("StringWriter.<init>")),
    };
    with_obj_mut(r, |o| {
        o.slots[0] = Value::Ref(Some(r));
        o.slots[STRINGWRITER_PAYLOAD] = Value::Str(std::sync::Arc::new(String::new()));
    });
    Ok(Value::Void)
}
fn sw_payload(recv: Option<&JRef>, op: &str) -> Result<(JRef, String), Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing(op)),
    };
    let s = with_obj(r, |o| match &o.slots[STRINGWRITER_PAYLOAD] {
        Value::Str(s) => s.to_string(),
        other => panic!("j2r internal: StringWriter payload {other:?}"),
    });
    Ok((r, s))
}
fn sw_append(recv: Option<&JRef>, op: &str, add: &str) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing(op)),
    };
    // In-place growth: Arc::make_mut clones the buffer only if a snapshot
    // is currently shared, else appends into the existing allocation.
    // Linear total; the previous clone-payload-then-restore path was
    // quadratic in the number of writes (the org.json report's StringWriter
    // finding, reproduced here).
    with_obj_mut(r, |o| match &mut o.slots[STRINGWRITER_PAYLOAD] {
        Value::Str(s) => std::sync::Arc::make_mut(s).push_str(add),
        other => panic!("j2r internal: StringWriter payload {other:?}"),
    });
    Ok(Value::Void)
}
/// PrintWriter (corpus wave 3, JAMA) sink support: append `add` to a
/// StringWriter's payload. `r` is the StringWriter object itself (not the
/// PrintWriter); used by the PrintWriter write path when its sink is a
/// StringWriter.
pub fn sw_append_ref(r: &JRef, add: &str) -> Result<(), Throw> {
    sw_append(Some(r), "PrintWriter.write", add).map(|_| ())
}
pub fn n_sw_write_int(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let c = match args.first() {
        Some(v) => v.expect_int(),
        None => return Err(missing("StringWriter.write")),
    };
    // JDK Writer.write(int): only the 16 low-order bits are written —
    // `write(0x1F340)` appends the single unit 0xF340, never the astral
    // scalar. A lone surrogate unit degrades to U+FFFD (Rust String
    // payload limit, documented in the module header); every BMP unit is
    // exact.
    let unit = (c as u32) & 0xFFFF;
    let ch = char::from_u32(unit).unwrap_or('\u{fffd}');
    let mut buf = String::with_capacity(4);
    buf.push(ch);
    sw_append(recv, "StringWriter.write", &buf)
}
pub fn n_sw_write_str(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let s_arg = match args.first() {
        Some(Value::Ref(Some(o))) if crate::object::is_string(o) => {
            crate::object::string_value(o).to_string()
        }
        Some(Value::Ref(None)) | None => return Err(crate::npe()),
        _ => return Err(missing("StringWriter.write")),
    };
    sw_append(recv, "StringWriter.write", &s_arg)
}
pub fn n_sw_write_str_range(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let (s_arg, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(o))), Some(i), Some(l)) if crate::object::is_string(o) => (
            crate::object::string_value(o).to_string(),
            i.expect_int(),
            l.expect_int(),
        ),
        (Some(Value::Ref(None)), _, _) => return Err(crate::npe()),
        _ => return Err(missing("StringWriter.write")),
    };
    // Scalar-value slice (see module limit note).
    let units: Vec<char> = s_arg.chars().collect();
    if off < 0 || len < 0 || (off + len) as usize > units.len() {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!(
                "begin {}, end {}, length {}",
                off,
                off + len,
                units.len()
            )),
        ));
    }
    let piece: String = units[off as usize..(off + len) as usize].iter().collect();
    sw_append(recv, "StringWriter.write", &piece)
}
pub fn n_sw_write_chars(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    // write(char[], off, len): load the range through the array model so
    // bounds and class checks stay in one place.
    let (cbuf, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(a))), Some(o), Some(l)) => (*a, o.expect_int(), l.expect_int()),
        (Some(Value::Ref(None)), _, _) => return Err(crate::npe()),
        _ => return Err(missing("StringWriter.write")),
    };
    let mut buf = String::with_capacity(len as usize);
    for i in 0..len {
        let v = crate::array::array_load(&cbuf, off + i)?;
        let c = match v {
            Value::Int(c) => char::from_u32(c as u32).unwrap_or('\u{fffd}'),
            other => {
                return Err(internal_error(&format!(
                    "j2r internal: char[] element {other:?}"
                )))
            }
        };
        buf.push(c);
    }
    sw_append(recv, "StringWriter.write", &buf)
}
/// `write(char[])` — the whole-array convenience overload.
pub fn n_sw_write_chars_all(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let cbuf = first_arg_ref!(args, Err(missing("StringWriter.write")));
    let len = crate::array::array_length(&cbuf)?;
    let mut buf = String::with_capacity(len as usize);
    for i in 0..len {
        let v = crate::array::array_load(&cbuf, i)?;
        let c = match v {
            Value::Int(c) => char::from_u32(c as u32).unwrap_or('\u{fffd}'),
            other => {
                return Err(internal_error(&format!(
                    "j2r internal: char[] element {other:?}"
                )))
            }
        };
        buf.push(c);
    }
    sw_append(recv, "StringWriter.write", &buf)
}
pub fn n_sw_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let (_, s) = sw_payload(recv, "StringWriter.toString")?;
    Ok(Value::Ref(Some(crate::object::new_string(&s))))
}
/// `StringWriter.close()` has no effect on the JDK — same here.
pub fn n_sw_close(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Void)
}
/// `StringWriter.flush()` — the JDK override is a no-op (Writer.flush is
/// abstract; StringWriter is concrete).
pub fn n_sw_flush(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Void)
}

// -- StringReader ----------------------------------------------------------
//
// The runtime-io wave (2026-11-07) cursor model, following the
// ByteArrayInputStream precedent: slot 0 = inherited `lock`, slot 1 = the
// FULL text (Value::Str, immutable for the reader's lifetime), slot 2 =
// pos (UTF-16 CODE-UNIT offset), slot 3 = mark, slot 4 = closed. Reads
// return UTF-16 code units (the JDK Reader law: read() yields one 16-bit
// unit, surrogate pairs as two reads) — the old scalar-value convention
// diverged from the JDK contract on astral input (the external org.json
// conversion report's finding #3). Draining is linear: no clone-per-read.
/// `Writer.write(char[])` — CONCRETE template method in the JDK: forwards
/// to `write(cbuf, 0, cbuf.length)` on the same receiver. Found by the
/// corpus: WritingBuffer inherits it while overriding only the abstract
/// pair, so a marker here would reject a call the JVM accepts.
pub fn n_writer_write_chars_all(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("Writer.write")),
    };
    let cbuf = first_arg_ref!(args, Err(missing("Writer.write")));
    let len = crate::array::array_length(&cbuf)?;
    crate::invoke_virtual(
        &r,
        "java/io/Writer",
        "write",
        "([CII)V",
        &[Value::Ref(Some(cbuf)), Value::Int(0), Value::Int(len)],
        ctx,
    )?;
    Ok(Value::Void)
}
/// `Writer.write(String)` — CONCRETE template method in the JDK: forwards
/// to `write(str, 0, str.length())` on the same receiver.
pub fn n_writer_write_str(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("Writer.write")),
    };
    let sobj = match args.first() {
        Some(Value::Ref(Some(o))) if crate::object::is_string(o) => *o,
        Some(Value::Ref(None)) | None => return Err(crate::npe()),
        _ => return Err(missing("Writer.write")),
    };
    let len = crate::object::string_value(&sobj).encode_utf16().count() as i32;
    crate::invoke_virtual(
        &r,
        "java/io/Writer",
        "write",
        "(Ljava/lang/String;II)V",
        &[Value::Ref(Some(sobj)), Value::Int(0), Value::Int(len)],
        ctx,
    )?;
    Ok(Value::Void)
}

// -------------------------------------------------- byte streams (JZlib)
//
// Corpus wave 2: the byte-stream surface JZlib's stream classes extend.
// The Filter* pair holds its JDK field (`out` / `in`) in own slot 0 (see
// builtin::builtin_field_slot); user subclasses touch it by name through
// the codegen's builtin-field resolution. The abstract bases carry the
// JDK default behaviors that reach user code (write([B) forwards to
// write([BII) on SELF so overrides win; write([BII) loops single-byte
// write(I) on SELF; available() defaults to 0; mark/reset follow the JDK
// defaults). Anything abstract dispatches loud — the object model was
// violated exactly like the Writer markers above.
pub fn n_os_write_int(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Err(internal_error(
        "j2r: OutputStream.write(I) is abstract — a concrete subclass must override it",
    ))
}
/// JDK OutputStream.write([B): `write(b, 0, b.length)` on SELF.
pub fn n_os_write_bulk(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("OutputStream.write")),
    };
    let b = first_arg_ref!(args, Err(missing("OutputStream.write")));
    let len = crate::array::array_length(&b)?;
    crate::invoke_virtual(
        &r,
        "java/io/OutputStream",
        "write",
        "([BII)V",
        &[Value::Ref(Some(b)), Value::Int(0), Value::Int(len)],
        ctx,
    )?;
    Ok(Value::Void)
}
/// JDK OutputStream.write([BII) default: the packed range check, then the
/// per-byte loop on SELF's `write(I)` (override wins).
pub fn n_os_write_range(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("OutputStream.write")),
    };
    let b = first_arg_ref!(args, Err(missing("OutputStream.write")));
    let off = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(missing("OutputStream.write")),
    };
    let len = match args.get(2) {
        Some(Value::Int(v)) => *v,
        _ => return Err(missing("OutputStream.write")),
    };
    check_write_range(&b, off, len, "OutputStream")?;
    for i in off..off + len {
        let v = crate::array::array_load(&b, i)?;
        crate::invoke_virtual(&r, "java/io/OutputStream", "write", "(I)V", &[v], ctx)?;
    }
    Ok(Value::Void)
}
pub fn n_os_flush(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Void) // JDK default: no-op
}
pub fn n_os_close(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Void) // JDK default: no-op
}

// FilterInputStream: slot 0 = `in`; every read delegates through it.
