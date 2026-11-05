//! java.io behavior natives (corpus wave 1, gap F3).
//!
//! F1 registered the writer/reader HIERARCHY; this module provides the
//! BEHAVIOR the minimal-json corpus actually drives:
//!
//! * `StringWriter` — payload string in own slot 1 (slot 0 is Writer's
//!   inherited `lock`), `write` appends, `toString` returns the payload,
//!   `close` is a no-op exactly like the JDK. The payload is a `Value::Str`
//!   held directly in the slot — appends are O(n) total via Rust string
//!   growth, and no intermediate buffer object is allocated.
//! * `java/io/Writer` rows for the abstract declarations the compile-time
//!   surface walk must find (`write(I)V`, `write([CII)V`,
//!   `write(String)V`, `write(String,II)V`). These are markers: every
//!   concrete Writer subclass overrides them (StringWriter here, user
//!   classes like WritingBuffer in compiled code), so dispatching to a
//!   marker means the object model was violated — fail loud.
//!
//! Writer's `flush` is NOT registered: nothing in the corpus dispatches
//! it, and the honest surface rule is "register what is called". The
//! corpus runner will name it loudly if that ever changes.
//!
//! Range writes slice by Unicode scalar values (Rust `char`), which
//! matches UTF-16 code units for the BMP — everything the corpus driver
//! writes is ASCII. Supplementary-plane slicing by UTF-16 index is a
//! documented limit until the string wave lands.

use crate::heap::{with_obj, with_obj_mut, JRef};
use crate::{internal_error, throw_new, Dispatch, Throw, Value};

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

fn missing(op: &str) -> Throw {
    internal_error(&format!("j2r internal: {op} missing argument"))
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
    let (r, s) = sw_payload(recv, op)?;
    let mut s = s;
    s.push_str(add);
    with_obj_mut(r, |o| o.slots[STRINGWRITER_PAYLOAD] = Value::Str(s.into()));
    Ok(Value::Void)
}

pub fn n_sw_write_int(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let c = match args.first() {
        Some(v) => v.expect_int(),
        None => return Err(missing("StringWriter.write")),
    };
    let ch = char::from_u32(c as u32).unwrap_or('\u{fffd}');
    let mut buf = String::with_capacity(4);
    buf.push(ch);
    sw_append(recv, "StringWriter.write", &buf)
}

pub fn n_sw_write_str(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let s_arg = match args.first() {
        Some(Value::Ref(Some(o))) if crate::object::is_string(o) => {
            crate::object::string_value(o).to_string()
        }
        Some(Value::Ref(None)) | None => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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
        (Some(Value::Ref(None)), _, _) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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
        (Some(Value::Ref(None)), _, _) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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
    let cbuf = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(missing("StringWriter.write")),
    };
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
// The payload slot holds the REMAINING text as a `Value::Str`; reads consume
// from the front, so the cursor is implicit in the remaining string. read()
// returns a Unicode scalar value or -1 at EOF (the module-level scalar
// convention applies to supplementary-plane inputs).

pub fn n_sr_init(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("StringReader.<init>")),
    };
    let s = match args.first() {
        Some(Value::Ref(Some(o))) if crate::object::is_string(o) => {
            crate::object::string_value(o).to_string()
        }
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(missing("StringReader.<init>")),
    };
    with_obj_mut(r, |o| {
        o.slots[0] = Value::Ref(Some(r));
        o.slots[STRINGWRITER_PAYLOAD] = Value::Str(std::sync::Arc::new(s));
    });
    Ok(Value::Void)
}

/// `Reader.read()` — one character, -1 at EOF.
pub fn n_sr_read(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("StringReader.read")),
    };
    let remaining = with_obj(r, |o| match &o.slots[STRINGWRITER_PAYLOAD] {
        Value::Str(st) => st.to_string(),
        other => panic!("j2r internal: StringReader payload {other:?}"),
    });
    let mut it = remaining.chars();
    let head = match it.next() {
        Some(c) => c,
        None => return Ok(Value::Int(-1)),
    };
    let rest: String = it.collect();
    with_obj_mut(r, |o| {
        o.slots[STRINGWRITER_PAYLOAD] = Value::Str(std::sync::Arc::new(rest))
    });
    Ok(Value::Int(head as i32))
}

/// `Reader.read(char[], off, len)` — bulk read, returns the count or -1.
pub fn n_sr_read_bulk(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing("StringReader.read")),
    };
    let (cbuf, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(a))), Some(o), Some(l)) => (*a, o.expect_int(), l.expect_int()),
        (Some(Value::Ref(None)), _, _) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
        _ => return Err(missing("StringReader.read")),
    };
    if len == 0 {
        return Ok(Value::Int(0));
    }
    let remaining = with_obj(r, |o| match &o.slots[STRINGWRITER_PAYLOAD] {
        Value::Str(st) => st.to_string(),
        other => panic!("j2r internal: StringReader payload {other:?}"),
    });
    let mut it = remaining.chars();
    let mut n = 0i32;
    let mut out: Vec<Value> = Vec::new();
    while n < len {
        match it.next() {
            Some(c) => {
                out.push(Value::Int(c as i32));
                n += 1;
            }
            None => break,
        }
    }
    if n == 0 {
        return Ok(Value::Int(-1));
    }
    let rest: String = it.collect();
    with_obj_mut(r, |o| {
        o.slots[STRINGWRITER_PAYLOAD] = Value::Str(std::sync::Arc::new(rest))
    });
    for (i, v) in out.into_iter().enumerate() {
        crate::array::array_store(&cbuf, off + i as i32, v, ctx)?;
    }
    Ok(Value::Int(n))
}

/// `StringReader.close()` has no effect on the JDK — same here.
pub fn n_sr_close(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
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
    let cbuf = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(missing("Writer.write")),
    };
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
        Some(Value::Ref(None)) | None => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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

fn stream_field(recv: Option<&JRef>, op: &str) -> Result<(JRef, JRef), Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing(op)),
    };
    let inner = match with_obj(r, |o| o.slots[0].clone()) {
        Value::Ref(Some(o)) => o,
        Value::Ref(None) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(internal_error(&format!("j2r internal: {op} field slot"))),
    };
    Ok((r, inner))
}

/// JDK 21's shared bounds shape for `write(b, off, len)` on the stream
/// classes: `Objects.requireNonNull(b)` then the packed
/// `(off | len | (b.length - (len + off)) | (off + len)) < 0` check.
fn check_write_range(b: &JRef, off: i32, len: i32, op: &str) -> Result<i32, Throw> {
    let blen = crate::array::array_length(b)?;
    if (off | len | (blen - (len + off)) | (off + len)) < 0 {
        return Err(throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&format!(
                "j2r: {op} write range [{off}, {off}+{len}) of {blen}"
            )),
        ));
    }
    Ok(blen)
}

pub fn n_fos_init(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (r, out) = stream_field_pair(recv, args, "FilterOutputStream.<init>")?;
    with_obj_mut(r, |o| o.slots[0] = Value::Ref(Some(out)));
    Ok(Value::Void)
}

fn stream_field_pair(recv: Option<&JRef>, args: &[Value], op: &str) -> Result<(JRef, JRef), Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing(op)),
    };
    let inner = match args.first() {
        Some(Value::Ref(Some(o))) => *o,
        Some(Value::Ref(None)) | None => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
        _ => return Err(missing(op)),
    };
    Ok((r, inner))
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
    let b = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(missing("FilterOutputStream.write")),
    };
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
    let b = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(missing("FilterOutputStream.write")),
    };
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
    let b = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(missing("OutputStream.write")),
    };
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
    let b = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(missing("OutputStream.write")),
    };
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
    let b = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(missing("FilterInputStream.read")),
    };
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
    let b = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(missing("InputStream.read")),
    };
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

pub fn n_is_read_range(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Err(internal_error(
        "j2r: InputStream.read([BII) is abstract — a concrete subclass must override it",
    ))
}

/// Registered divergence (loud, documented): the JDK's `skip(J)` default
/// loop-reads through a skip buffer; nothing in the corpus drives the
/// default path, so it is not modelled — calling it fails loud instead of
/// silently mis-skipping. JZlib's InflaterInputStream overrides skip.
pub fn n_is_skip(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Err(internal_error(
        "j2r: InputStream.skip(J) default is not modelled (registered divergence) — override it",
    ))
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
        Some(Value::Ref(None)) | None => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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
        Some(Value::Ref(None)) | None => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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
        Some(Value::Ref(None)) | None => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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
