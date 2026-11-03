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
        o.slots[STRINGWRITER_PAYLOAD] = Value::Str(std::rc::Rc::new(String::new()));
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
        (Some(Value::Ref(Some(o))), Some(i), Some(l))
            if crate::object::is_string(o) =>
        {
            (
                crate::object::string_value(o).to_string(),
                i.expect_int(),
                l.expect_int(),
            )
        }
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
        Some(Value::Ref(None)) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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

pub fn n_sw_to_string(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
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
        Some(Value::Ref(None)) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
        _ => return Err(missing("StringReader.<init>")),
    };
    with_obj_mut(r, |o| {
        o.slots[0] = Value::Ref(Some(r));
        o.slots[STRINGWRITER_PAYLOAD] = Value::Str(std::rc::Rc::new(s));
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
        o.slots[STRINGWRITER_PAYLOAD] = Value::Str(std::rc::Rc::new(rest))
    });
    Ok(Value::Int(head as i32))
}

/// `Reader.read(char[], off, len)` — bulk read, returns the count or -1.
pub fn n_sr_read_bulk(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
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
        o.slots[STRINGWRITER_PAYLOAD] = Value::Str(std::rc::Rc::new(rest))
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
        Some(Value::Ref(None)) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
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
