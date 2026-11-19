//! The natives table: JDK methods implemented directly in Rust.
//!
//! The code generator validates every method reference against this table
//! (for `java/*` classes) at compile time, so an unsupported JDK method is a
//! *compile error* with a clear message — never a silent miscompile.
//!
//! Conventions:
//! * instance natives get `recv = Some(&JRef)`, statics get `None`,
//! * `java/lang/String` and `StringBuilder` store their characters as
//!   `Value::Str` in slot 0 (copy-on-write via `Arc::make_mut`),
//! * `Throwable.detailMessage` is slot 0 (`Value::Ref` to a String or null),
//! * boxed classes store their payload in slot 0,
//! * `java/io/PrintStream` stores its sink id (1 = stdout, 2 = stderr) in
//!   slot 0,
//! * string lengths, indices and hashes follow Java's UTF-16 semantics.

use crate::collections as c;
use crate::concat;
use crate::fdlibm as fd;
use crate::first_arg_ref;
use crate::formatter;
use crate::heap::with_obj;
use crate::heap::JRef;
use crate::io;
use crate::jdkutil as ju;
use crate::lambda;
use crate::langext as le;
use crate::object::{self, is_string, new_obj};
use crate::reflect;
use crate::regex;
use crate::Value;
use crate::{internal_error, javafmt, throw_new, Dispatch, NativeFn, Throw};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
/// The JNI descriptors and class paths the natives table repeats row
/// after row — one spelling each, named at the use site.
const SIG_OBJECT: &str = "(Ljava/lang/Object;)Ljava/lang/Object;";
const SIG_OBJECT_OBJECT: &str = "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;";
const UNMOD_LIST: &str = "java/util/Collections$UnmodifiableList";
fn this<'a>(recv: Option<&'a JRef>, who: &str) -> &'a JRef {
    recv.unwrap_or_else(|| panic!("j2r internal: {who} called without receiver"))
}
fn arg<'a>(args: &'a [Value], who: &str) -> &'a Value {
    args.first()
        .ok_or_else(|| internal_error(&format!("{who} without argument")))
        .unwrap_or_else(|_| panic!("j2r internal: {who} without argument"))
}
fn dotted(class: &str) -> String {
    class.replace('/', ".")
}
fn print_string(sink: i32, text: &str, newline: bool) {
    use std::io::Write;
    let mut bytes = Vec::with_capacity(text.len() + 1);
    bytes.extend_from_slice(text.as_bytes());
    if newline {
        bytes.push(b'\n');
    }
    if sink == 1 {
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        let _ = lock.write_all(&bytes);
        let _ = lock.flush();
    } else {
        let stderr = std::io::stderr();
        let mut lock = stderr.lock();
        let _ = lock.write_all(&bytes);
        let _ = lock.flush();
    }
}
fn println_print(
    recv: Option<&JRef>,
    args: &[Value],
    kind: char,
    newline: bool,
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let sink = crate::heap::with_obj(*this(recv, "PrintStream method"), |o| {
        o.slots[0].expect_int()
    });
    let text = match kind {
        'N' => String::new(),
        _ => concat::format_value(arg(args, "PrintStream"), kind, ctx)?,
    };
    print_string(sink, &text, newline);
    Ok(Value::Void)
}

macro_rules! ps {
    ($name:ident, $kind:expr, $newline:expr) => {
        pub fn $name(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
            println_print(recv, args, $kind, $newline, ctx)
        }
    };
}

ps!(n_ps_println_n, 'N', true);
ps!(n_ps_println_z, 'Z', true);
ps!(n_ps_println_c, 'C', true);
ps!(n_ps_println_i, 'I', true);
ps!(n_ps_println_j, 'J', true);
ps!(n_ps_println_f, 'F', true);
ps!(n_ps_println_d, 'D', true);
ps!(n_ps_println_s, 'L', true);
ps!(n_ps_println_l, 'L', true);
ps!(n_ps_print_z, 'Z', false);
ps!(n_ps_print_c, 'C', false);
ps!(n_ps_print_i, 'I', false);
ps!(n_ps_print_j, 'J', false);
ps!(n_ps_print_f, 'F', false);
ps!(n_ps_print_d, 'D', false);
ps!(n_ps_print_s, 'L', false);
ps!(n_ps_print_l, 'L', false);

// ---------------------------------------------------------------- PrintWriter
//
// Real-world corpus wave 3 (JAMA): JDK-exact subset of java.io.PrintWriter.
// The JDK class wraps its sink (OutputStream → BufferedWriter(
// OutputStreamWriter(out)); Writer → passthrough) and funnels every
// print/println/write into write(String) + newLine(); autoflush flushes
// after println. The subset here models exactly the surface the corpus
// (and typical console output) exercises:
//   * sinks: the native stdout/stderr streams (PrintStream sink-id objects,
//     as returned by System.out/err) and StringWriter buffers;
//   * print(String)/print(char)/println()/println(String)/write(String)/
//     flush/close.
// JDK source semantics kept verbatim: write(s) = write(s == null ? "null"
// : s); print(c) = write(String.valueOf(c)); println() = newLine() =
// write(line.separator) — "\n" on Linux; println(x) = print(x); println().
// Autoflush on these sinks is a no-op (native streams are already
// unbuffered from the runtime's side; StringWriter has nothing to flush).
// Slots: absolute 0 = inherited Writer `lock`, absolute 1 = sink ref,
// absolute 2 = autoFlush flag (Int).
/// `java/util/Locale.US` as a `Value` (corpus wave 3: the only locale
/// constant real code references here).
pub fn builtin_locale_us() -> Value {
    Value::Ref(Some(object::locale_us_ref()))
}
/// Exact lookup: `(class, name, descriptor)`.
/// The natives table: `(class, method, descriptor, implementation)`.
/// Classes use internal form (`a/b/C`). Keep sorted-ish by class for review.
/// The 2026-11-19 scanaislop split carries the rows verbatim in five
/// contiguous part files; the table itself is a slice of slices, so the
/// row bytes and the lookup order are unchanged.
pub static NATIVES: &[&[(&str, &str, &str, NativeFn)]] = &[
    table_a::PART,
    table_b::PART,
    table_c::PART,
    table_d::PART,
    table_e::PART,
];

pub fn lookup_exact(class: &str, name: &str, desc: &str) -> Option<NativeFn> {
    NATIVES
        .iter()
        .flat_map(|t| t.iter())
        .find(|(c, n, d, _)| *c == class && *n == name && *d == desc)
        .map(|(_, _, _, f)| *f)
}
/// Calls an exact native entry (used by statically-resolved
/// `invokestatic`/`invokespecial` emission). Missing entries are an internal
/// error because the code generator validated them at compile time.
pub fn invoke_exact(
    class: &str,
    name: &str,
    desc: &str,
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    match lookup_exact(class, name, desc) {
        Some(f) => f(recv, args, ctx),
        None => Err(internal_error(&format!(
            "native method {class}.{name}{desc} not found"
        ))),
    }
}
/// `java/lang/System.out` as a `Value`.
pub fn system_out() -> Value {
    Value::Ref(Some(object::system_out_ref()))
}
/// `java/lang/System.err` as a `Value`.
pub fn system_err() -> Value {
    Value::Ref(Some(object::system_err_ref()))
}
/// Collector hook: pushes the indices of every object held by the runtime's
/// own permanent structures. The JLS 5.1.7 boxed-identity cache entries are
/// permanent roots — `Integer.valueOf(127)` must return the same object for
/// the whole process lifetime, across any number of collections.
pub fn scan_runtime_roots(out: &mut Vec<u32>) {
    for r in boxed_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
    {
        out.push(r.index());
    }
}
/// Test-only access to the boxed cache (heap unit tests use it to prove the
/// cache is scanned as a root set).
#[cfg(test)]
pub fn boxed_cached_for_test(v: i64) -> Option<crate::heap::JRef> {
    boxed_cached("java/lang/Integer", v, -128, 127)
}

// --- Real-program surface: natural-ordering bridge rows --------------------
// `Collections.sort`/`Arrays.sort(Object[])` dispatch `compareTo` with the
// erased `(Ljava/lang/Object;)I` bridge descriptor. User classes carry
// javac's compiled bridges already; the builtin boxes and String need these
// rows (the dispatch walk matches exact descriptors only). Instance methods:
// the receiver is the boxed self, args[0] is the boxed other — unbox both
// from their slot-0 payloads.
fn n_str_compare_to_bridge(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    le::n_str_compare_to(recv, args, ctx)
}
fn n_integer_compare_to_bridge(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = box_payload_int(recv, "Integer.compareTo");
    let b = match args.first() {
        Some(Value::Ref(Some(r))) => box_payload_int(Some(r), "Integer.compareTo arg"),
        _ => return Err(crate::npe()),
    };
    Ok(Value::Int(a.cmp(&b) as i32))
}
fn n_long_compare_to_bridge(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = box_payload_long(recv, "Long.compareTo");
    let b = match args.first() {
        Some(Value::Ref(Some(r))) => box_payload_long(Some(r), "Long.compareTo arg"),
        _ => return Err(crate::npe()),
    };
    Ok(Value::Int(a.cmp(&b) as i32))
}
fn n_double_compare_to_bridge(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = box_payload_double(recv, "Double.compareTo");
    let b = match args.first() {
        Some(Value::Ref(Some(r))) => box_payload_double(Some(r), "Double.compareTo arg"),
        _ => return Err(crate::npe()),
    };
    Ok(Value::Int(double_total_cmp(a, b)))
}
/// Java's total order on doubles: -0.0 < +0.0 (partial_cmp calls them
/// equal — the oracle sorts [-0.0, 0.0] differently), NaN > everything,
/// NaN == NaN.
pub fn double_total_cmp(a: f64, b: f64) -> i32 {
    if a == 0.0 && b == 0.0 {
        return match (a.is_sign_negative(), b.is_sign_negative()) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };
    }
    match a.partial_cmp(&b) {
        Some(ord) => ord as i32,
        None => {
            let a_nan = a.is_nan();
            let b_nan = b.is_nan();
            if a_nan && b_nan {
                0
            } else if a_nan {
                1
            } else {
                -1
            }
        }
    }
}
fn box_payload_int(r: Option<&JRef>, meth: &str) -> i32 {
    match r {
        Some(r) => with_obj(*r, |o| o.slots[0].expect_int()),
        None => panic!("j2r internal: {meth} without receiver"),
    }
}
fn box_payload_long(r: Option<&JRef>, meth: &str) -> i64 {
    match r {
        Some(r) => with_obj(*r, |o| o.slots[0].expect_long()),
        None => panic!("j2r internal: {meth} without receiver"),
    }
}
fn box_payload_double(r: Option<&JRef>, meth: &str) -> f64 {
    match r {
        Some(r) => with_obj(*r, |o| o.slots[0].expect_double()),
        None => panic!("j2r internal: {meth} without receiver"),
    }
}

// --- scanaislop split: the items below live verbatim in the child modules ---
mod boxed;
mod core;
mod misc2;
mod pw;
mod random;
mod reflection;
mod strings;
#[path = "natives/table_a.rs"]
mod table_a;
#[path = "natives/table_b.rs"]
mod table_b;
#[path = "natives/table_c.rs"]
mod table_c;
#[path = "natives/table_d.rs"]
mod table_d;
#[path = "natives/table_e.rs"]
mod table_e;
#[cfg(test)]
mod tests;
mod thread;
mod throwable;
pub(crate) use boxed::*;
pub(crate) use core::*;
pub(crate) use misc2::*;
pub(crate) use pw::*;
pub(crate) use random::*;
pub(crate) use reflection::*;
pub(crate) use strings::*;
pub(crate) use thread::*;
pub(crate) use throwable::*;
