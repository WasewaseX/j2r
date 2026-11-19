//! java.io behavior natives (corpus wave 1, gap F3).
//!
//! F1 registered the writer/reader HIERARCHY; this module provides the
//! BEHAVIOR the minimal-json corpus actually drives:
//!
//! * `StringWriter` — buffer string in own slot 1 (slot 0 is Writer's
//!   inherited `lock`), `write` appends IN PLACE via `Arc::make_mut`
//!   (O(1) amortized per append, linear total — the runtime-io wave of
//!   2026-11-07 replaced the old clone-payload-per-append path, which
//!   was quadratic exactly as the external org.json conversion report
//!   predicted for naive StringWriter models), `toString` returns the
//!   payload, `close`/`flush` are no-ops exactly like the JDK.
//!   `write(I)V` keeps only the 16 low-order bits (JDK's `(char) c`
//!   law — pinned by the StringIoSemantics fixture). A payload is a
//!   Rust `String`, so LONE UTF-16 units written through `write(int)`
//!   (surrogates) degrade to U+FFFD — documented limit, unchanged by
//!   this wave; astral SCALARS truncated to units are now JDK-exact.
//! * `java/io/Writer` rows for the abstract declarations the compile-time
//!   surface walk must find (`write(I)V`, `write([CII)V`,
//!   `write(String)V`, `write(String,II)V`). These are markers: every
//!   concrete Writer subclass overrides them (StringWriter here, user
//!   classes like WritingBuffer in compiled code), so dispatching to a
//!   marker means the object model was violated — fail loud.
//!
//! Writer's `flush` is NOT registered on the abstract base: nothing in
//! the corpus dispatches it, and the honest surface rule is "register
//! what is called". The corpus runner will name it loudly if that ever
//! changes.
//!
//! * `StringReader` — the runtime-io wave (2026-11-07) replaced the
//!   remaining-string payload model (clone-per-read, quadratic drain —
//!   org.json conversion-report finding #1) with the ByteArrayInputStream
//!   cursor model: slot 0 = inherited `lock`, slot 1 = the FULL text
//!   (`Value::Str`, immutable for the reader's lifetime), slot 2 = pos
//!   (UTF-16 CODE-UNIT offset), slot 3 = mark, slot 4 = closed. reads
//!   return UTF-16 code units (JDK's Reader.read() law: one 16-bit unit,
//!   surrogate pairs as two reads) — replacing the old scalar convention
//!   for supplementary-plane inputs, which was a documented divergence
//!   from the JDK contract (org.json finding #3). The full JDK surface is
//!   now modeled with the TEMURIN-21-EMPIRICAL laws (the probe caught the
//!   Java-13-era CSR changes before they shipped): skip(negative)
//!   returns 0 and does NOT throw; reset() before any mark() resets to
//!   position 0 (mark initializes to 0 — the old "Mark invalid"
//!   IOException is gone from the modern JDK); mark(readAheadLimit)
//!   still rejects negative limits ("Read-ahead limit < 0"); close()
//!   is idempotent and every subsequent op throws IOException "Stream
//!   closed"; bulk reads range-check with the Objects.checkFromIndexRange
//!   message ("Range [off, off + len) out of bounds for length n") BEFORE
//!   the len==0 short-circuit. BMP behavior is unchanged from the old
//!   model, so corpus goldens are unaffected; the astral contract is
//!   pinned by the StringIoSemantics suite fixture.
//!
//! Range writes slice by Unicode scalar values (Rust `char`), which
//! matches UTF-16 code units for the BMP — everything the corpus driver
//! writes is ASCII. Supplementary-plane slicing by UTF-16 index is a
//! documented limit until the string model can hold lone units.

use crate::first_arg_ref;
use crate::heap::{with_obj, with_obj_mut, JRef};
use crate::{internal_error, throw_new, Dispatch, Throw, Value};
use std::sync::Arc;
fn missing(op: &str) -> Throw {
    internal_error(&format!("j2r internal: {op} missing argument"))
}
fn stream_field(recv: Option<&JRef>, op: &str) -> Result<(JRef, JRef), Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing(op)),
    };
    let inner = match with_obj(r, |o| o.slots[0].clone()) {
        Value::Ref(Some(o)) => o,
        Value::Ref(None) => return Err(crate::npe()),
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
fn stream_field_pair(recv: Option<&JRef>, args: &[Value], op: &str) -> Result<(JRef, JRef), Throw> {
    let r = match recv {
        Some(r) => *r,
        None => return Err(missing(op)),
    };
    let inner = match args.first() {
        Some(Value::Ref(Some(o))) => *o,
        Some(Value::Ref(None)) | None => return Err(crate::npe()),
        _ => return Err(missing(op)),
    };
    Ok((r, inner))
}

// --- scanaislop split: the items below live verbatim in the child modules ---
mod buffered;
mod bytearray;
mod pushback;
mod reader;
mod writer;
pub(crate) use buffered::*;
pub(crate) use bytearray::*;
pub(crate) use pushback::*;
pub(crate) use reader::*;
pub(crate) use writer::*;
