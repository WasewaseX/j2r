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
        | (_, _, Some(Value::Ref(None))) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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
    let buf = match args.first() {
        Some(Value::Ref(Some(b))) => *b,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(missing("ByteArrayInputStream.<init>")),
    };
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
        | (_, _, Some(Value::Ref(None))) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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
        | (_, _, Some(Value::Ref(None))) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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

fn bis_buf(r: &JRef, op: &str) -> Result<JRef, Throw> {
    match with_obj(*r, |o| o.slots[1].clone()) {
        Value::Ref(Some(b)) => Ok(b),
        // JDK getBufIfOpen: null buffer == closed
        Value::Ref(None) => Err(throw_new("java/io/IOException", Some("Stream closed"))),
        _ => Err(internal_error(&format!("j2r internal: {op} buf slot"))),
    }
}

fn bis_inner(r: &JRef, op: &str) -> Result<JRef, Throw> {
    match with_obj(*r, |o| o.slots[0].clone()) {
        Value::Ref(Some(s)) => Ok(s),
        Value::Ref(None) => Err(throw_new("java/io/IOException", Some("Stream closed"))),
        _ => Err(internal_error(&format!("j2r internal: {op} in slot"))),
    }
}

fn bis_state(r: &JRef) -> (i32, i32, i32, i32) {
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

fn bis_set(r: &JRef, pos: i32, count: i32, markpos: i32, marklimit: i32, buf: Option<JRef>) {
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
fn bis_fill(r: &JRef, ctx: Dispatch) -> Result<(), Throw> {
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
fn bis_read1(r: &JRef, b: &JRef, off: i32, len: i32, ctx: Dispatch) -> Result<i32, Throw> {
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

fn bis_load_byte(buf: &JRef, idx: i32, op: &str) -> Result<i32, Throw> {
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
        | (_, _, Some(Value::Ref(None))) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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
        | (_, _, Some(Value::Ref(None))) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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
    let b = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(missing("PushbackInputStream.unread")),
    };
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
