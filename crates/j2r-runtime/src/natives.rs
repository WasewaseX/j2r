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
        fn $name(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
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

fn pw_sink_of(recv: Option<&JRef>) -> Result<JRef, Throw> {
    let r = this(recv, "PrintWriter method");
    // Writer owns slot 0 (the JDK `lock`); the sink ref is own-slot 0 =
    // absolute slot 1, autoFlush is absolute slot 2.
    let sink = crate::heap::with_obj(*r, |o| o.slots[1].clone());
    match sink {
        Value::Ref(Some(s)) => Ok(s),
        other => Err(internal_error(&format!(
            "j2r internal: PrintWriter sink {other:?}"
        ))),
    }
}

fn pw_write_str(sink: &JRef, text: &str, newline: bool) -> Result<(), Throw> {
    let class = crate::heap::with_obj(*sink, |o| o.jclass.to_string());
    match class.as_str() {
        "java/io/PrintStream" => {
            let id = crate::heap::with_obj(*sink, |o| o.slots[0].expect_int());
            print_string(id, text, newline);
            Ok(())
        }
        "java/io/StringWriter" => {
            io::sw_append_ref(sink, text)?;
            if newline {
                io::sw_append_ref(sink, "\n")?;
            }
            Ok(())
        }
        other => Err(throw_new(
            "java/lang/UnsupportedOperationException",
            Some(&format!(
                "J2R: PrintWriter over {other} sink is not supported (corpus wave 3 subset: native stdout/stderr and StringWriter sinks)"
            )),
        )),
    }
}

fn pw_write(recv: Option<&JRef>, args: &[Value]) -> Result<Value, Throw> {
    let sink = pw_sink_of(recv)?;
    let text = match args.first() {
        Some(Value::Ref(Some(o))) if crate::object::is_string(o) => {
            crate::object::string_value(o).to_string()
        }
        Some(Value::Ref(None)) | None => "null".to_string(),
        other => {
            return Err(internal_error(&format!(
                "j2r internal: PrintWriter.write arg {other:?}"
            )))
        }
    };
    pw_write_str(&sink, &text, false)?;
    Ok(Value::Void)
}

fn n_pw_init_os(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "PrintWriter.<init>");
    let sink = match args.first() {
        Some(Value::Ref(Some(s))) => *s,
        other => {
            return Err(internal_error(&format!(
                "j2r internal: PrintWriter.<init> sink {other:?}"
            )))
        }
    };
    let autoflush = match args.get(1) {
        Some(Value::Int(1)) => 1,
        Some(Value::Int(0)) => 0,
        other => {
            return Err(internal_error(&format!(
                "j2r internal: PrintWriter.<init> autoflush {other:?}"
            )))
        }
    };
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[1] = Value::Ref(Some(sink));
        o.slots[2] = Value::Int(autoflush);
    });
    Ok(Value::Void)
}

fn n_pw_init_writer(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "PrintWriter.<init>");
    let sink = match args.first() {
        Some(Value::Ref(Some(s))) => *s,
        other => {
            return Err(internal_error(&format!(
                "j2r internal: PrintWriter.<init> sink {other:?}"
            )))
        }
    };
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[1] = Value::Ref(Some(sink));
        o.slots[2] = Value::Int(0);
    });
    Ok(Value::Void)
}

fn n_pw_print_c(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let sink = pw_sink_of(recv)?;
    let c = arg(args, "PrintWriter.print").expect_int();
    let ch = char::from_u32(c as u32).unwrap_or('\u{fffd}');
    let mut buf = String::with_capacity(4);
    buf.push(ch);
    pw_write_str(&sink, &buf, false)?;
    Ok(Value::Void)
}

fn n_pw_print_s(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let _ = ctx;
    pw_write(recv, args)
}

fn n_pw_println(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let sink = pw_sink_of(recv)?;
    pw_write_str(&sink, "\n", false)?;
    Ok(Value::Void)
}

fn n_pw_println_s(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    // JDK: print(x) then println() — two write calls, one newline.
    n_pw_print_s(recv, args, ctx)?;
    n_pw_println(recv, &[], ctx)
}

fn n_pw_write_s(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let _ = ctx;
    pw_write(recv, args)
}

fn n_pw_flush(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    // Native streams are flushed per write; StringWriter keeps everything
    // in the payload. Nothing buffered here to move.
    Ok(Value::Void)
}

fn n_pw_close(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    // JDK close() closes the sink; the supported sinks are process-wide
    // (stdout/stderr) or in-memory (StringWriter, whose close is a no-op
    // like the JDK's), so there is nothing to close either way.
    Ok(Value::Void)
}

// ------------------------------------------------- corpus wave 3 loud rows
//
// JAMA drags in three surfaces the runtime has no machinery for yet. They
// are REGISTERED LOUDLY (the wave-1 serialization precedent): every entry
// resolves and instantiates so real programs compile end to end, and every
// behavior native throws UnsupportedOperationException naming the gap.
// The corpus driver never calls them (Matrix.read / Matrix.print are
// uncalled), so the differential proof is unaffected; closing them with
// probe-pinned implementations is future corpus work
// (docs/CORPUS_WAVE.md §wave 3 open gaps).

fn loud_unsupported(what: &str) -> Result<Value, Throw> {
    Err(throw_new(
        "java/lang/UnsupportedOperationException",
        Some(what),
    ))
}

fn n_loud_vector(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    loud_unsupported(
        "J2R: java.util.Vector is not supported (corpus wave 3 registration: legacy synchronized collections machinery does not exist; JAMA's uncalled Matrix.read references it)",
    )
}

fn n_loud_stream_tokenizer(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    loud_unsupported(
        "J2R: java.io.StreamTokenizer is not supported (corpus wave 3 registration: the syntax-table tokenizer is unmodeled; JAMA's uncalled Matrix.read references it)",
    )
}

fn n_loud_decimal_format(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    loud_unsupported(
        "J2R: java.text.DecimalFormat/NumberFormat is not supported (corpus wave 3 registration: the HALF_EVEN formatting engine with symbol tables is a registered gap; JAMA's uncalled Matrix.print references it)",
    )
}

/// `java/util/Locale.US` as a `Value` (corpus wave 3: the only locale
/// constant real code references here).
pub fn builtin_locale_us() -> Value {
    Value::Ref(Some(object::locale_us_ref()))
}

/// `Double.valueOf(String)` — parse then box (JAMA's uncalled Matrix.read
/// parses tokens through it; the parsing core is the probed
/// parseDouble contract).
fn n_double_value_of_s(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = n_double_parse_double(_recv, args, ctx)?;
    let Value::Double(d) = v else {
        return Err(internal_error("j2r internal: parseDouble result"));
    };
    let r = object::new_obj("java/lang/Double", 1)?;
    object::obj_set(&r, 0, Value::Double(d));
    Ok(Value::Ref(Some(r)))
}

// ---------------------------------------------------------------- Object

fn n_object_init(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Void)
}

// Object monitor condition methods (M4 Wave 3, T11/T12): real
// implementations over the Wave-3 monitor (crate::monitor). All three
// `wait` shapes funnel into one implementation; argument validation and
// the ownership check live there (HotSpot order: ownership first).
fn n_object_wait(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Object.wait");
    let (millis, nanos) = match args {
        [] => (0i64, 0i32),
        [Value::Long(v)] => (*v, 0i32),
        [Value::Long(v), Value::Int(n)] => (*v, *n),
        _ => (0, 0), // unreachable through the registered shapes
    };
    crate::monitor::object_wait(r, millis, nanos)
}

fn n_object_notify(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Object.notify");
    crate::monitor::object_notify(r, false)?;
    Ok(Value::Void)
}

fn n_object_notify_all(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.notifyAll");
    crate::monitor::object_notify(r, true)?;
    Ok(Value::Void)
}

fn n_object_hash_code(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.hashCode");
    // Identity hash = the arena index: distinct for distinct live objects,
    // stable for a live object across collections (M2_PLAN §4.6).
    Ok(Value::Int(r.index() as i32))
}

fn n_object_clone(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Object.clone");
    Ok(Value::Ref(Some(object::object_shallow_clone(r)?)))
}

// ------------------------- java/lang/Enum (corpus wave 2, JZlib)

fn enum_slot(recv: Option<&JRef>, who: &str, idx: usize) -> Value {
    let r = this(recv, who);
    crate::heap::with_obj(*r, |o| o.slots[idx].clone())
}

fn n_enum_init(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Enum.<init>");
    let name = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        Some(Value::Ref(Some(o))) if is_string(o) => object::string_value(o).to_string(),
        _ => return Err(internal_error("j2r internal: Enum.<init> name argument")),
    };
    let ord = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(internal_error("j2r internal: Enum.<init> ordinal argument")),
    };
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = Value::Str(std::sync::Arc::new(name));
        o.slots[1] = Value::Int(ord);
    });
    Ok(Value::Void)
}

fn n_enum_name(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    match enum_slot(recv, "Enum.name", 0) {
        Value::Str(s) => Ok(Value::Ref(Some(object::new_string(&s)))),
        _ => Err(internal_error("j2r internal: Enum.name slot")),
    }
}

fn n_enum_ordinal(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    match enum_slot(recv, "Enum.ordinal", 1) {
        Value::Int(v) => Ok(Value::Int(v)),
        _ => Err(internal_error("j2r internal: Enum.ordinal slot")),
    }
}

/// JDK `Enum.toString()` returns the name.
fn n_enum_to_string(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    match enum_slot(recv, "Enum.toString", 0) {
        Value::Str(s) => Ok(Value::Ref(Some(object::new_string(&s)))),
        _ => Err(internal_error("j2r internal: Enum.toString slot")),
    }
}

/// JDK `Enum.equals` is final identity (`this == other`).
fn n_enum_equals(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Enum.equals");
    let same = match args.first() {
        Some(Value::Ref(Some(other))) => JRef::ptr_eq(r, other),
        _ => false,
    };
    Ok(Value::Int(same as i32))
}

/// JDK `Enum.compareTo` — the ordinal compare with the exact three-way
/// branch shape (same ordinals of DIFFERENT subclasses subtract too).
fn n_enum_compare_to(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Enum.compareTo");
    let other = match args.first() {
        Some(Value::Ref(Some(o))) => *o,
        _ => return Err(internal_error("j2r internal: Enum.compareTo argument")),
    };
    let mine = match enum_slot(Some(r), "Enum.compareTo", 1) {
        Value::Int(v) => v,
        _ => return Err(internal_error("j2r internal: Enum.compareTo ordinal slot")),
    };
    let theirs = match enum_slot(Some(&other), "Enum.compareTo", 1) {
        Value::Int(v) => v,
        _ => return Err(internal_error("j2r internal: Enum.compareTo ordinal slot")),
    };
    // ClassCastException parity is the call site's job (javac inserts the
    // checkcast; the bridge method carries the typed descriptor).
    if mine < theirs {
        Ok(Value::Int(-1))
    } else if mine > theirs {
        Ok(Value::Int(1))
    } else {
        Ok(Value::Int(mine.wrapping_sub(theirs)))
    }
}

/// JDK `Enum.getDeclaringClass` — the `Class` object for the enum class
/// itself (final, returns the receiver's runtime class).
fn n_enum_get_declaring_class(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Enum.getDeclaringClass");
    let name = crate::heap::with_obj(*r, |o| o.jclass.to_string());
    Ok(reflect::class_value(&name))
}

/// JDK `Enum.valueOf(Class, String)`. javac's synthetic `E.valueOf(String)`
/// delegates here, so one implementation covers both entry points. The
/// constant table is the class's `$VALUES` static (javac synthesizes it for
/// every enum), read through the generated crate's pool: `meta` names the
/// field, `static_get` reads the slot. The constant's name lives in the
/// Enum slot 0 (the same slot `<init>` writes).
///
/// Temurin order pinned by corpus/EnumProbe.java: the directory lookup runs
/// FIRST, then a null name throws NPE("Name is null"), then a miss throws
/// IAE(`No enum constant <binary name>.<name>`) — the binary (dotted) name,
/// so a nested enum prints `EnumProbe.Color.red`.
fn n_enum_value_of(_recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let cls = match args.first() {
        Some(Value::Ref(Some(r))) => *r,
        // Temurin dereferences the class argument without a message NPE.
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(internal_error("j2r internal: Enum.valueOf class argument")),
    };
    let name: Option<String> = match args.get(1) {
        Some(Value::Str(s)) => Some(s.to_string()),
        Some(Value::Ref(Some(o))) if object::is_string(o) => {
            Some(object::string_value(o).to_string())
        }
        Some(Value::Ref(None)) => None,
        _ => return Err(internal_error("j2r internal: Enum.valueOf name argument")),
    };
    let cls_name = reflect::class_name_of(&cls);
    let values = match (ctx.meta, ctx.static_get) {
        (Some(meta), Some(get)) => match meta(&cls_name) {
            Some(info) => info
                .fields
                .iter()
                .find(|f| f.is_static && f.name == "$VALUES")
                .map(|f| get(f.static_slot)),
            None => None,
        },
        _ => None,
    };
    match values {
        Some(Value::Ref(Some(arr))) => {
            let len = crate::array::array_length(&arr)?;
            for i in 0..len {
                let elem = crate::array::array_load(&arr, i)?;
                if let Value::Ref(Some(obj)) = &elem {
                    if let Value::Str(s) = enum_slot(Some(obj), "Enum.valueOf", 0) {
                        if let Some(want) = &name {
                            if s.as_ref() == want.as_str() {
                                return Ok(elem);
                            }
                        }
                    }
                }
            }
        }
        // Not a compiled enum class (builtin pseudo-enum, uncompiled
        // dependency): no constant table to search — loud internal marker,
        // mirroring the serialization-marker rule.
        _ => {
            return Err(internal_error(&format!(
                "j2r internal: Enum.valueOf: no $VALUES static pool for {cls_name}"
            )))
        }
    }
    match name {
        None => Err(throw_new(
            "java/lang/NullPointerException",
            Some("Name is null"),
        )),
        Some(n) => Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some(&format!(
                "No enum constant {}.{}",
                reflect::canonical_name(&cls_name),
                n
            )),
        )),
    }
}

fn n_object_equals(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Object.equals");
    let same = match args.first() {
        Some(Value::Ref(Some(other))) => JRef::ptr_eq(r, other),
        _ => false,
    };
    Ok(Value::Int(same as i32))
}

fn n_object_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.toString");
    let (cls, hash) = crate::heap::with_obj(*r, |o| (dotted(&o.jclass), o_slots_identity(r)));
    Ok(Value::Ref(Some(object::new_string(&format!(
        "{cls}@{}",
        javafmt::int_to_hex_string(hash)
    )))))
}

/// The identity-hash component for `Object.toString` (`Class@hash`): the
/// arena index, stable for a live object (M2_PLAN §4.6).
fn o_slots_identity(r: &JRef) -> i32 {
    r.index() as i32
}

// ---------------------------------------------------------------- System

fn n_system_exit(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let code = arg(args, "System.exit").expect_int();
    std::process::exit(code);
}

fn n_system_current_time_millis(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Ok(Value::Long(ms))
}

fn n_system_nano_time(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Long(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0),
    ))
}

/// Java serialization does not exist in J2R (no stream format, no
/// serialization machinery). Registered so javac-compiled calls — like
/// minimal-json's readObject hook — fail LOUDLY at runtime with a named
/// reason, never silently. Same pattern as the M3 Object.wait rows.
fn n_object_input_stream_default_read_object(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Err(throw_new(
        "java/lang/UnsupportedOperationException",
        Some("J2R: java.io serialization (ObjectInputStream) is not supported"),
    ))
}

fn n_system_arraycopy(_recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    crate::array::system_arraycopy(args, ctx)
}

/// `System.loadLibrary(String)` (T13 13c — the loading story,
/// docs/M5_JNI_HARNESS.md §7): resolve the platform library filename
/// (`lib<name>.so` on this ABI), dlopen it, and run its `JNI_OnLoad` with
/// the VM table pointer through audited primitive 12. `JNI_OnLoad` then
/// `GetEnv`s the env table (VM slot 6, real since 13c) and binds its
/// natives through slot 215 — the SAME mechanism Temurin serves, which is
/// what makes the ONE harness `.so` work on both runtimes. No `JNI_OnLoad`
/// symbol is success (the spec's default-version case); a post-1.8
/// version answer is `UnsatisfiedLinkError` (the oracle's family); a
/// dlopen miss aborts loudly inside the primitive (v1: never a silent
/// fake success; the UnsatisfiedLinkError-on-load-failure transport is a
/// registered v1 gap, unit-pinned via the primitive's own loudness).
fn n_system_load_library(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let name = match arg(args, "System.loadLibrary") {
        Value::Ref(Some(o)) if is_string(o) => object::string_value(o).to_string(),
        Value::Ref(None) => {
            return Err(crate::throw_new("java/lang/NullPointerException", None));
        }
        _ => {
            panic!("j2r internal: System.loadLibrary needs a String")
        }
    };
    // v1: this ABI's loader filename. A non-Linux platform is a loud
    // unsupported (the standing law; CI and the parity runner are Linux).
    if !cfg!(target_os = "linux") {
        panic!("j2r JNI bridge: System.loadLibrary is Linux-only in v1 (loud unsupported)");
    }
    let path = format!("lib{name}.so");
    let Ok(cpath) = std::ffi::CString::new(path.clone()) else {
        return Err(crate::throw_new(
            "java/lang/UnsatisfiedLinkError",
            Some(&format!("{path}: embedded NUL in the library name")),
        ));
    };
    let vm = crate::jni::tables::java_vm() as usize as *mut std::os::raw::c_void;
    let ver = j2r_jni_ffi::ffi::load_foreign_jni_library(cpath.as_ptr(), vm);
    let ok = ver == 0
        || matches!(
            ver,
            crate::jni::JNI_VERSION_1_1
                | crate::jni::JNI_VERSION_1_2
                | crate::jni::JNI_VERSION_1_4
                | crate::jni::JNI_VERSION_1_6
                | crate::jni::JNI_VERSION_1_8
                | crate::jni::JNI_VERSION_9
                | crate::jni::JNI_VERSION_10
        );
    if !ok {
        return Err(crate::throw_new(
            "java/lang/UnsatisfiedLinkError",
            Some(&format!("unsupported JNI version 0x{ver:08x}")),
        ));
    }
    Ok(Value::Ref(None))
}

// float/double predicates (corpus wave 1) — exact IEEE semantics
fn n_float_is_infinite(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let f = arg(args, "Float.isInfinite").expect_float();
    Ok(Value::Int(f.is_infinite() as i32))
}

fn n_float_is_nan(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let f = arg(args, "Float.isNaN").expect_float();
    Ok(Value::Int(f.is_nan() as i32))
}

fn n_double_is_infinite(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let d = arg(args, "Double.isInfinite").expect_double();
    Ok(Value::Int(d.is_infinite() as i32))
}

fn n_double_is_nan(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let d = arg(args, "Double.isNaN").expect_double();
    Ok(Value::Int(d.is_nan() as i32))
}

// ---------------------------------------------------------------- Math
//
// Math is implemented explicitly (no macro gymnastics) so Java's NaN and
// signed-zero semantics for float/double stay obvious.

fn n_math_abs_i(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(
        arg(args, "Math.abs").expect_int().wrapping_abs(),
    ))
}

fn n_math_abs_j(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Long(
        arg(args, "Math.abs").expect_long().wrapping_abs(),
    ))
}

fn n_math_abs_f(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Float(arg(args, "Math.abs").expect_float().abs()))
}

fn n_math_abs_d(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(arg(args, "Math.abs").expect_double().abs()))
}

fn java_max_d(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        // +0.0 wins over -0.0
        if a.is_sign_positive() {
            a
        } else {
            b
        }
    } else if a > b {
        a
    } else {
        b
    }
}

fn java_min_d(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        // -0.0 wins over +0.0
        if a.is_sign_negative() {
            a
        } else {
            b
        }
    } else if a < b {
        a
    } else {
        b
    }
}

fn n_math_max_i(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.max").expect_int();
    let b = args[1].expect_int();
    Ok(Value::Int(if a >= b { a } else { b }))
}

fn n_math_max_j(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.max").expect_long();
    let b = args[1].expect_long();
    Ok(Value::Long(if a >= b { a } else { b }))
}

fn n_math_max_f(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.max").expect_float();
    let b = args[1].expect_float();
    Ok(Value::Float(java_max_d(a as f64, b as f64) as f32))
}

fn n_math_max_d(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.max").expect_double();
    let b = args[1].expect_double();
    Ok(Value::Double(java_max_d(a, b)))
}

fn n_math_min_i(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.min").expect_int();
    let b = args[1].expect_int();
    Ok(Value::Int(if a <= b { a } else { b }))
}

fn n_math_min_j(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.min").expect_long();
    let b = args[1].expect_long();
    Ok(Value::Long(if a <= b { a } else { b }))
}

fn n_math_min_f(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.min").expect_float();
    let b = args[1].expect_float();
    Ok(Value::Float(java_min_d(a as f64, b as f64) as f32))
}

fn n_math_min_d(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = arg(args, "Math.min").expect_double();
    let b = args[1].expect_double();
    Ok(Value::Double(java_min_d(a, b)))
}

// ---------------------------------------------------------------- String

fn utf16_units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn n_string_init(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(String::new())));
    Ok(Value::Void)
}

/// `String(char[])` / `String(char[], off, len)` (corpus wave 1): build
/// from UTF-16 code units — the same unit convention as getChars.
fn string_from_char_range(args: &[Value], op: &str) -> Result<String, Throw> {
    let (cbuf, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(a))), Some(o), Some(l)) => (*a, o.expect_int(), l.expect_int()),
        (Some(Value::Ref(None)), _, _) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
        _ => return Err(internal_error(&format!("j2r internal: {op} args"))),
    };
    let clen = crate::array::array_length(&cbuf)?;
    if off < 0 || len < 0 || off.checked_add(len).is_none_or(|e| e > clen) {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!("offset {}, count {}, length {}", off, len, clen)),
        ));
    }
    let mut units: Vec<u16> = Vec::with_capacity(len as usize);
    for i in 0..len {
        match crate::array::array_load(&cbuf, off + i)? {
            Value::Int(c) => units.push(c as u16),
            other => {
                return Err(internal_error(&format!(
                    "j2r internal: char[] element {other:?}"
                )))
            }
        }
    }
    Ok(String::from_utf16_lossy(&units))
}

fn n_string_init_chars(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    let cbuf = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(internal_error("String.<init>([C) needs a char array")),
    };
    let len = crate::array::array_length(&cbuf)?;
    let text = string_from_char_range(
        &[Value::Ref(Some(cbuf)), Value::Int(0), Value::Int(len)],
        "String.<init>([C)",
    )?;
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}

fn n_string_init_chars_range(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    let text = string_from_char_range(args, "String.<init>([CII)")?;
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}

fn n_string_init_s(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    let o = match arg(args, "String.<init>") {
        Value::Ref(Some(o)) if is_string(o) => o,
        _ => return Err(internal_error("String.<init>(String) needs a String")),
    };
    let text = object::string_value(o).to_string();
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}

fn n_string_length(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.length");
    Ok(Value::Int(
        utf16_units(&object::string_value(r)).len() as i32
    ))
}

/// Java `String.toString()` returns `this` — registered so dispatched
/// `toString()` calls (collections, concat, `Objects.toString`) reach the
/// string itself instead of falling through to `Object.toString`.
pub fn n_string_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(*this(recv, "String.toString"))))
}

fn n_string_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.isEmpty");
    Ok(Value::Int(object::string_value(r).is_empty() as i32))
}

fn n_string_char_at(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.charAt");
    let idx = arg(args, "String.charAt").expect_int();
    let units = utf16_units(&object::string_value(r));
    let len = units.len() as i32;
    if idx < 0 || idx >= len {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!("Index {idx} out of bounds for length {len}")),
        ));
    }
    Ok(Value::Int(units[idx as usize] as i32))
}

fn n_string_equals(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.equals");
    let eq = match arg(args, "String.equals") {
        Value::Ref(Some(o)) if is_string(o) => *object::string_value(r) == *object::string_value(o),
        _ => false,
    };
    Ok(Value::Int(eq as i32))
}

/// `String.getChars(int, int, char[], int)` (corpus wave 1): copy the
/// UTF-16 code units of the range into the destination char[]. Indices are
/// UTF-16 units (the string model's documented convention); the JDK's
/// StringIndexOutOfBoundsException message is reproduced.
fn n_string_get_chars(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "String.getChars");
    let (begin, end, dst, dst_begin) = match (args.first(), args.get(1), args.get(2), args.get(3)) {
        (
            Some(Value::Int(b)),
            Some(Value::Int(e)),
            Some(Value::Ref(Some(d))),
            Some(Value::Int(db)),
        ) => (*b, *e, *d, *db),
        (Some(Value::Ref(None)), _, _, _) | (_, _, Some(Value::Ref(None)), _) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
        _ => return Err(internal_error("j2r internal: String.getChars args")),
    };
    let units: Vec<u16> = object::string_value(r).encode_utf16().collect();
    if begin < 0 || end > units.len() as i32 || begin > end {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!(
                "begin {}, end {}, length {}",
                begin,
                end,
                units.len()
            )),
        ));
    }
    for i in begin..end {
        crate::array::array_store(
            &dst,
            dst_begin + (i - begin),
            Value::Int(units[i as usize] as i32),
            ctx,
        )?;
    }
    Ok(Value::Void)
}

// ---------------------------------------- String <-> byte[] (JZlib corpus)
//
// The byte-side String natives (corpus wave 2): the everyday encoders with
// JDK-exact contracts, pinned by probes on Temurin 21.0.13 (CharsetProbe /
// RangeProbe): unknown charset name -> UnsupportedEncodingException whose
// message is the name AS GIVEN; null name -> NullPointerException with
// message "null"; non-representable chars encode as '?' for Latin-1 and
// ASCII; malformed UTF-8 decodes to one U+FFFD per maximal bad subpart;
// ASCII decode of a high byte yields U+FFFD; range overflows throw
// StringIndexOutOfBoundsException "Range [off, off+len) out of bounds for
// length N"; names are case-insensitive ("utf-8" works).
//
// Registered divergence (loud, documented): the standard charset provider
// registry BEYOND the UTF-16 family / UTF-8 / Latin-1 / ASCII (UTF-32,
// KOI8_R, ... — JDK-supported names that SUCCEED on Temurin) still throws
// UnsupportedEncodingException here. The UTF-16 family landed with the
// skip/charset wave, pinned by corpus/Utf16Probe.java on Temurin 21.0.13.

#[derive(Clone, Copy)]
enum BytesCs {
    Utf8,
    Latin1,
    Ascii,
    Utf16,
    Utf16Be,
    Utf16Le,
}

fn resolve_charset(v: &Value, who: &str) -> Result<BytesCs, Throw> {
    let name = match v {
        Value::Str(s) => s.to_string(),
        Value::Ref(Some(o)) if is_string(o) => object::string_value(o).to_string(),
        Value::Ref(None) => return Err(throw_new("java/lang/NullPointerException", Some("null"))),
        _ => return Err(internal_error(&format!("j2r internal: {who} charset name"))),
    };
    let norm: String = name.chars().map(|c| c.to_ascii_lowercase()).collect();
    match norm.as_str() {
        "utf-8" | "utf8" => Ok(BytesCs::Utf8),
        "iso-8859-1" | "iso8859-1" | "iso_8859_1" | "8859_1" | "latin1" | "l1" => {
            Ok(BytesCs::Latin1)
        }
        "us-ascii" | "ascii" | "ansi_x3.4-1968" | "iso646-us" | "646" => Ok(BytesCs::Ascii),
        // UTF-16 family (skip/charset wave, corpus/Utf16Probe.java): the
        // JDK canonical names plus the common Charset aliases.
        "utf-16" | "utf16" | "unicode" | "unicodebig" | "utf_16" => Ok(BytesCs::Utf16),
        "utf-16be" | "unicodebigunmarked" | "utf_16be" => Ok(BytesCs::Utf16Be),
        "utf-16le" | "unicodelittleunmarked" | "utf_16le" => Ok(BytesCs::Utf16Le),
        _ => Err(throw_new(
            "java/io/UnsupportedEncodingException",
            Some(&name),
        )),
    }
}

fn encode_cs(text: &str, cs: BytesCs) -> Vec<u8> {
    match cs {
        BytesCs::Utf8 => text.as_bytes().to_vec(),
        BytesCs::Latin1 => text
            .chars()
            .map(|c| if (c as u32) <= 0xFF { c as u8 } else { b'?' })
            .collect(),
        BytesCs::Ascii => text
            .chars()
            .map(|c| if (c as u32) <= 0x7F { c as u8 } else { b'?' })
            .collect(),
        BytesCs::Utf16 => {
            // BOM (FE FF) + big-endian units (Temurin: "A" -> feff0041).
            let mut out = vec![0xFE, 0xFF];
            out.extend(utf16_bytes(text, true));
            out
        }
        BytesCs::Utf16Be => utf16_bytes(text, true),
        BytesCs::Utf16Le => utf16_bytes(text, false),
    }
}

/// UTF-16 code units, byte-expanded. Astral chars split into the standard
/// surrogate pair; the j2r string model carries U+FFFD where the JDK has an
/// unpaired surrogate (the classfile loader normalizes), and Temurin's
/// encoder also emits U+FFFD for one — the bytes agree (Utf16Probe pins
/// `\uD834` -> `fffd`).
fn utf16_bytes(text: &str, big_endian: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    let push_unit = |u: u16, out: &mut Vec<u8>| {
        let [hi, lo] = u.to_be_bytes();
        if big_endian {
            out.push(hi);
            out.push(lo);
        } else {
            out.push(lo);
            out.push(hi);
        }
    };
    for c in text.chars() {
        let u = c as u32;
        if u > 0xFFFF {
            let v = u - 0x10000;
            push_unit(0xD800 + (v >> 10) as u16, &mut out);
            push_unit(0xDC00 + (v & 0x3FF) as u16, &mut out);
        } else {
            push_unit(u as u16, &mut out);
        }
    }
    out
}

/// Decode UTF-16 units from bytes; a dangling trailing byte decodes as one
/// U+FFFD (Temurin-pinned: {00 41 00} -> charAt(1) == 65533), an unpaired
/// surrogate unit also yields U+FFFD (JDK substitution).
fn utf16_decode(bytes: &[u8], big_endian: bool) -> String {
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i + 1 < bytes.len() {
        let u = if big_endian {
            ((bytes[i] as u16) << 8) | bytes[i + 1] as u16
        } else {
            ((bytes[i + 1] as u16) << 8) | bytes[i] as u16
        };
        units.push(u);
        i += 2;
    }
    if i < bytes.len() {
        // dangling byte
        units.push(0xFFFD);
    }
    let mut chars: Vec<char> = Vec::with_capacity(units.len());
    let mut k = 0;
    while k < units.len() {
        let u = units[k];
        if (0xD800..0xDC00).contains(&u) {
            if k + 1 < units.len() && (0xDC00..0xE000).contains(&units[k + 1]) {
                let c = 0x10000 + (((u - 0xD800) as u32) << 10) + (units[k + 1] - 0xDC00) as u32;
                chars.push(char::from_u32(c).unwrap_or(char::REPLACEMENT_CHARACTER));
                k += 2;
            } else {
                chars.push(char::REPLACEMENT_CHARACTER);
                k += 1;
            }
        } else if (0xDC00..0xE000).contains(&u) {
            chars.push(char::REPLACEMENT_CHARACTER);
            k += 1;
        } else {
            chars.push(char::from_u32(u as u32).unwrap_or(char::REPLACEMENT_CHARACTER));
            k += 1;
        }
    }
    chars.into_iter().collect()
}

fn decode_cs(bytes: &[u8], cs: BytesCs) -> String {
    match cs {
        BytesCs::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        BytesCs::Latin1 => bytes.iter().map(|&b| b as char).collect(),
        BytesCs::Ascii => bytes
            .iter()
            .map(|&b| {
                if b <= 0x7F {
                    b as char
                } else {
                    char::REPLACEMENT_CHARACTER
                }
            })
            .collect(),
        BytesCs::Utf16 => {
            // BOM detection (Temurin-pinned): FE FF -> BE, FF FE -> LE,
            // both consumed; NO BOM defaults to big-endian (bytes {41 00}
            // decode as U+4100, NOT "A").
            if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
                utf16_decode(&bytes[2..], true)
            } else if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
                utf16_decode(&bytes[2..], false)
            } else {
                utf16_decode(bytes, true)
            }
        }
        BytesCs::Utf16Be => utf16_decode(bytes, true),
        BytesCs::Utf16Le => utf16_decode(bytes, false),
    }
}

fn bytes_out(bytes: &[u8], ctx: Dispatch) -> Result<Value, Throw> {
    let out = crate::array::array_new_prim("[B", bytes.len() as i32)?;
    for (i, b) in bytes.iter().enumerate() {
        crate::array::array_store(&out, i as i32, Value::Int(*b as i8 as i32), ctx)?;
    }
    Ok(Value::Ref(Some(out)))
}

fn bytes_in(r: &JRef, who: &str) -> Result<Vec<u8>, Throw> {
    let len = crate::array::array_length(r)?;
    let mut bytes = Vec::with_capacity(len as usize);
    for i in 0..len {
        match crate::array::array_load(r, i)? {
            Value::Int(v) => bytes.push(v as u8),
            _ => return Err(internal_error(&format!("j2r internal: {who} byte array"))),
        }
    }
    Ok(bytes)
}

fn n_string_get_bytes_named(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.getBytes");
    let name = arg(args, "String.getBytes");
    let cs = resolve_charset(name, "String.getBytes")?;
    let bytes = encode_cs(&object::string_value(r), cs);
    bytes_out(&bytes, ctx)
}

fn n_string_get_bytes_default(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.getBytes");
    // The platform default charset is UTF-8 (the JVM's file.encoding on
    // every platform j2r targets; Temurin 21 on Linux pins it too).
    let bytes = encode_cs(&object::string_value(r), BytesCs::Utf8);
    bytes_out(&bytes, ctx)
}

fn n_string_init_bytes_named(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    let buf = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => {
            return Err(internal_error(
                "String.<init>([B,String) needs a byte array",
            ))
        }
    };
    let name = args
        .get(1)
        .ok_or_else(|| internal_error("j2r internal: String.<init> missing charset name"))?;
    let cs = resolve_charset(name, "String.<init>")?;
    let bytes = bytes_in(&buf, "String.<init>")?;
    let text = decode_cs(&bytes, cs);
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}

fn n_string_init_bytes_named_range(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.<init>");
    let buf = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => {
            return Err(internal_error(
                "String.<init>([BII,String) needs a byte array",
            ))
        }
    };
    let (off, len) = match (args.get(1), args.get(2)) {
        (Some(Value::Int(o)), Some(Value::Int(l))) => (*o, *l),
        _ => {
            return Err(internal_error(
                "j2r internal: String.<init>([BII,String) args",
            ))
        }
    };
    let total = crate::array::array_length(&buf)?;
    if off < 0 || len < 0 || off > total || len > total - off {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!(
                "Range [{off}, {off} + {len}) out of bounds for length {total}"
            )),
        ));
    }
    let name = args
        .get(3)
        .ok_or_else(|| internal_error("j2r internal: String.<init> missing charset name"))?;
    let cs = resolve_charset(name, "String.<init>")?;
    let mut bytes = Vec::with_capacity(len as usize);
    for i in off..off + len {
        match crate::array::array_load(&buf, i)? {
            Value::Int(v) => bytes.push(v as u8),
            _ => {
                return Err(internal_error(
                    "j2r internal: String.<init> byte array element",
                ))
            }
        }
    }
    let text = decode_cs(&bytes, cs);
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}

fn n_string_hash_code(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "String.hashCode");
    let mut h: i32 = 0;
    for u in object::string_value(r).encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(u as i32);
    }
    Ok(Value::Int(h))
}

fn n_string_value_of_i(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &arg(args, "String.valueOf").expect_int().to_string(),
    ))))
}

fn n_string_value_of_j(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &arg(args, "String.valueOf").expect_long().to_string(),
    ))))
}

fn n_string_value_of_c(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    let v = arg(args, "String.valueOf").expect_int();
    let c = char::from_u32(v as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
    Ok(Value::Ref(Some(object::new_string(&c.to_string()))))
}

fn n_string_value_of_z(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    let v = arg(args, "String.valueOf").expect_int();
    Ok(Value::Ref(Some(object::new_string(if v != 0 {
        "true"
    } else {
        "false"
    }))))
}

fn n_string_value_of_f(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &javafmt::float_to_string(arg(args, "String.valueOf").expect_float()),
    ))))
}

fn n_string_value_of_d(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &javafmt::double_to_string(arg(args, "String.valueOf").expect_double()),
    ))))
}

fn n_string_value_of_l(_r: Option<&JRef>, args: &[Value], c: Dispatch) -> Result<Value, Throw> {
    let text = concat::format_value(arg(args, "String.valueOf"), 'L', c)?;
    Ok(Value::Ref(Some(object::new_string(&text))))
}

// ---------------------------------------------------------------- StringBuilder

fn sb_text(r: &JRef) -> Arc<String> {
    crate::heap::with_obj(*r, |b| match &b.slots[0] {
        Value::Str(s) => s.clone(),
        other => panic!("j2r internal: StringBuilder slot0 is {other:?}"),
    })
}

fn sb_push_str(r: &JRef, text: &str) {
    crate::heap::with_obj_mut(*r, |b| match &mut b.slots[0] {
        Value::Str(s) => Arc::make_mut(s).push_str(text),
        other => panic!("j2r internal: StringBuilder slot0 is {other:?}"),
    })
}

fn n_sb_init(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.<init>");
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(String::new())));
    Ok(Value::Void)
}

fn n_sb_init_s(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.<init>");
    let o = match arg(args, "StringBuilder.<init>") {
        Value::Ref(Some(o)) if is_string(o) => o,
        _ => {
            return Err(internal_error(
                "StringBuilder.<init>(String) needs a String",
            ))
        }
    };
    let text = object::string_value(o).to_string();
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Str(Arc::new(text)));
    Ok(Value::Void)
}

macro_rules! sb_append {
    ($name:ident, $kind:expr) => {
        fn $name(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
            let r = this(recv, "StringBuilder.append");
            let text = concat::format_value(arg(args, "StringBuilder.append"), $kind, ctx)?;
            sb_push_str(r, &text);
            Ok(Value::Ref(Some(r.clone())))
        }
    };
}

sb_append!(n_sb_append_z, 'Z');
sb_append!(n_sb_append_c, 'C');
sb_append!(n_sb_append_i, 'I');
sb_append!(n_sb_append_j, 'J');
sb_append!(n_sb_append_f, 'F');
sb_append!(n_sb_append_d, 'D');
sb_append!(n_sb_append_s, 'L');
sb_append!(n_sb_append_l, 'L');

/// `StringBuilder.append(char[], int, int)` / `append(char[])` (corpus
/// wave 1): copy UTF-16 units from the array into the builder.
fn sb_append_char_range(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.append");
    let (cbuf, off, len) = match (args.first(), args.get(1), args.get(2)) {
        (Some(Value::Ref(Some(a))), Some(o), Some(l)) => (*a, o.expect_int(), l.expect_int()),
        (Some(Value::Ref(None)), _, _) => {
            return Err(throw_new("java/lang/NullPointerException", None))
        }
        _ => return Err(internal_error("j2r internal: StringBuilder.append args")),
    };
    let clen = crate::array::array_length(&cbuf)?;
    if off < 0 || len < 0 || off.checked_add(len).is_none_or(|e| e > clen) {
        return Err(throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&format!("offset {}, count {}, length {}", off, len, clen)),
        ));
    }
    let mut units: Vec<u16> = Vec::with_capacity(len as usize);
    for i in 0..len {
        match crate::array::array_load(&cbuf, off + i)? {
            Value::Int(c) => units.push(c as u16),
            other => {
                return Err(internal_error(&format!(
                    "j2r internal: char[] element {other:?}"
                )))
            }
        }
    }
    sb_push_str(r, &String::from_utf16_lossy(&units));
    Ok(Value::Ref(Some(*r)))
}

fn sb_append_char_all(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let cbuf = match args.first() {
        Some(Value::Ref(Some(a))) => *a,
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(internal_error("j2r internal: StringBuilder.append args")),
    };
    let len = crate::array::array_length(&cbuf)?;
    sb_append_char_range(
        recv,
        &[Value::Ref(Some(cbuf)), Value::Int(0), Value::Int(len)],
        Dispatch::default(),
    )
}

fn n_sb_to_string(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.toString");
    Ok(Value::Ref(Some(object::string_from_rc(sb_text(r)))))
}

fn n_sb_length(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "StringBuilder.length");
    Ok(Value::Int(utf16_units(&sb_text(r)).len() as i32))
}

// ---------------------------------------------------------------- Throwable
//
// Heap layout (audit A2 — suppressed-exception machinery): slot 0
// `detailMessage`, slot 1 `cause`, slot 2 the suppressed list as a Ref to a
// `java/lang/Throwable[]` snapshot in insertion order (null until the first
// `addSuppressed`). `getSuppressed` returns a fresh copy of the snapshot —
// the JVM returns a clone, so mutating the returned array must not affect
// the throwable. Natives registered on `Throwable` resolve for every
// builtin exception family through the parent-chain dispatch, which is what
// makes javac's try-with-resources lowering (JLS 14.20.3 emits
// `Throwable.addSuppressed`) run unmodified.
//
// Exception-message bytes probed on Temurin 21.0.13+9 (see
// docs/AUDIT_VERIFICATION.md, 2026-11-03): "Self-suppression not permitted"
// (IllegalArgumentException, checked before the null case, as in OpenJDK)
// and "Cannot suppress a null exception." (NullPointerException, with the
// trailing period).

fn n_throwable_init(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.<init>");
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = Value::Ref(None);
        o.slots[1] = Value::Ref(None);
        o.slots[2] = Value::Ref(None);
    });
    Ok(Value::Void)
}

fn n_throwable_init_s(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.<init>");
    let msg = match arg(args, "Throwable.<init>") {
        Value::Ref(None) => Value::Ref(None),
        Value::Ref(Some(o)) if is_string(o) => Value::Ref(Some(*o)),
        other => return Err(internal_error(&format!("Throwable.<init> arg {other:?}"))),
    };
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = msg;
        o.slots[1] = Value::Ref(None);
        o.slots[2] = Value::Ref(None);
    });
    Ok(Value::Void)
}

/// `Throwable(String message, Throwable cause)` — the message is used
/// verbatim (unlike the `(Throwable)` constructor, which derives one).
fn n_throwable_init_msg_cause(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.<init>");
    if args.len() != 2 {
        return Err(internal_error("Throwable.<init> arity"));
    }
    let msg = match &args[0] {
        Value::Ref(None) => Value::Ref(None),
        Value::Ref(Some(o)) if is_string(o) => Value::Ref(Some(*o)),
        other => return Err(internal_error(&format!("Throwable.<init> arg {other:?}"))),
    };
    let cause = match &args[1] {
        Value::Ref(c) => *c,
        other => return Err(internal_error(&format!("Throwable.<init> arg {other:?}"))),
    };
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = msg;
        o.slots[1] = Value::Ref(cause);
        o.slots[2] = Value::Ref(None);
    });
    Ok(Value::Void)
}

/// `Throwable(Throwable cause)` — JVM semantics: when `cause != null` the
/// detail message is `cause.toString()`, computed through real virtual
/// dispatch so a user `toString` override wins (OpenJDK Throwable.java).
fn n_throwable_init_cause(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.<init>");
    let cause = match arg(args, "Throwable.<init>") {
        Value::Ref(c) => *c,
        other => return Err(internal_error(&format!("Throwable.<init> arg {other:?}"))),
    };
    let msg: Value = match cause {
        None => Value::Ref(None),
        Some(c) => {
            let _nr = crate::frame::NativeRoots::push(&[c]);
            match crate::invoke_virtual(
                &c,
                "java/lang/Object",
                "toString",
                "()Ljava/lang/String;",
                &[],
                ctx,
            ) {
                Ok(Value::Ref(Some(s))) => Value::Ref(Some(s)),
                _ => Value::Ref(None),
            }
        }
    };
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = msg;
        o.slots[1] = Value::Ref(cause);
        o.slots[2] = Value::Ref(None);
    });
    Ok(Value::Void)
}

fn n_throwable_get_message(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.getMessage");
    Ok(crate::heap::with_obj(*r, |o| o.slots[0].clone()))
}

// EXTERNAL-B7 (jbox2d port, 2026-11-13): AssertionError's primitive and
// Object constructors. OpenJDK AssertionError.java: every constructor
// delegates `super(String.valueOf(detailMessage))` — so the message is the
// String.valueOf rendering of the argument (float/double through
// Float/Double.toString, Object through real virtual toString dispatch).
// The (Object) form: Temurin 21.0.13 pins `new
// AssertionError((Object) null).toString()` = "java.lang.AssertionError:
// null" — the old reset-the-message-to-null branch is gone upstream, so a
// null argument stores the MESSAGE "null" (getMessage() is the string
// "null", not Java null). The primitive forms can never produce a null
// message. (() and (String) already resolve through the Throwable rows
// above via the parent-chain dispatch; (String, Throwable) rides
// n_throwable_init_msg_cause the same way.)

/// String.valueOf rendering for the primitive constructor arguments
/// (int/long/float/double share one native, matched by value shape).
/// Booleans and chars arrive as Value::Int, so they get their own natives
/// below — the descriptor is the only thing that tells them apart.
fn assertion_error_message(v: &Value, ctx: Dispatch) -> Result<Value, Throw> {
    let text = match v {
        Value::Int(i) => i.to_string(),
        Value::Long(l) => l.to_string(),
        Value::Float(f) => crate::javafmt::float_to_string(*f),
        Value::Double(d) => crate::javafmt::double_to_string(*d),
        // String.valueOf law: null -> "null"; anything else renders through
        // real toString dispatch so a user override wins (the same law
        // n_throwable_init_cause states). Temurin 21.0.13 pinned (the
        // Externals fixture): AssertionError((Object) null).toString() is
        // "java.lang.AssertionError: null" — the old
        // reset-detailMessage-to-null branch is gone upstream, so a null
        // argument leaves the MESSAGE "null", unlike Throwable(String).
        Value::Ref(_) => {
            if let Value::Ref(Some(o)) = v {
                let _nr = crate::frame::NativeRoots::push(&[*o]);
            }
            concat::format_value(v, 'L', ctx)?
        }
        other => {
            return Err(internal_error(&format!(
                "AssertionError.<init> arg {other:?}"
            )))
        }
    };
    Ok(Value::Ref(Some(object::new_string(&text))))
}

/// Stores the message + the two null Throwable slots (the same shape
/// n_throwable_init_s writes).
fn set_throwable_message(r: &JRef, msg: Value) {
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = msg;
        o.slots[1] = Value::Ref(None);
        o.slots[2] = Value::Ref(None);
    });
}

/// `AssertionError(int|long|float|double)` — String.valueOf rendering.
fn n_assertion_error_init(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "AssertionError.<init>");
    let msg = assertion_error_message(arg(args, "AssertionError.<init>"), ctx)?;
    set_throwable_message(r, msg);
    Ok(Value::Void)
}

/// `AssertionError(boolean)` — String.valueOf(Z): "true"/"false".
fn n_assertion_error_init_z(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "AssertionError.<init>");
    let b = arg(args, "AssertionError.<init>").expect_int();
    let msg = Value::Ref(Some(object::new_string(if b != 0 {
        "true"
    } else {
        "false"
    })));
    set_throwable_message(r, msg);
    Ok(Value::Void)
}

/// `AssertionError(char)` — String.valueOf(C): the one-character string.
fn n_assertion_error_init_c(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "AssertionError.<init>");
    let i = arg(args, "AssertionError.<init>").expect_int();
    let ch = char::from_u32(i as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
    let msg = Value::Ref(Some(object::new_string(&ch.to_string())));
    set_throwable_message(r, msg);
    Ok(Value::Void)
}

/// `AssertionError(Object)` — String.valueOf rendering; a null argument
/// stores the MESSAGE "null" (Temurin 21.0.13-pinned; see the
/// assertion_error_message note), so it shares the generic native.
fn n_assertion_error_init_obj(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    n_assertion_error_init(recv, args, ctx)
}

fn n_throwable_get_cause(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.getCause");
    Ok(crate::heap::with_obj(*r, |o| o.slots[1].clone()))
}

/// Snapshot of the suppressed list as a fresh `Throwable[]` (empty when
/// nothing was suppressed — `getSuppressed` never returns null, probed on
/// Temurin: empty-len=0).
fn n_throwable_get_suppressed(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.getSuppressed");
    let cur = crate::heap::with_obj(*r, |o| match &o.slots[2] {
        Value::Ref(x) => *x,
        other => panic!("j2r internal: Throwable slot 2 is {other:?}"),
    });
    let out = match cur {
        None => crate::array::array_new_ref("java/lang/Throwable", 0)?,
        Some(a) => {
            let len = crate::array::array_length(&a)?;
            let out = crate::array::array_new_ref("java/lang/Throwable", len)?;
            for i in 0..len {
                let v = crate::array::array_load(&a, i)?;
                crate::array::array_store(&out, i, v, ctx)?;
            }
            out
        }
    };
    Ok(Value::Ref(Some(out)))
}

/// `addSuppressed`: append to the snapshot list, growing a fresh
/// `Throwable[]` each time (no GC can run mid-native, so the read-copy-
/// write is race-free and keeps every intermediate array reachable from
/// slot 2 until the final store).
fn n_throwable_add_suppressed(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.addSuppressed");
    let exc = match arg(args, "Throwable.addSuppressed") {
        Value::Ref(x) => *x,
        other => {
            return Err(internal_error(&format!(
                "Throwable.addSuppressed arg {other:?}"
            )))
        }
    };
    // OpenJDK order: the self check fires before the null check.
    match exc {
        Some(e) if *r == e => {
            return Err(throw_new(
                "java/lang/IllegalArgumentException",
                Some("Self-suppression not permitted"),
            ))
        }
        None => {
            return Err(throw_new(
                "java/lang/NullPointerException",
                Some("Cannot suppress a null exception."),
            ))
        }
        Some(_) => {}
    }
    let old = crate::heap::with_obj(*r, |o| match &o.slots[2] {
        Value::Ref(x) => *x,
        other => panic!("j2r internal: Throwable slot 2 is {other:?}"),
    });
    let len = match old {
        Some(a) => crate::array::array_length(&a)?,
        None => 0,
    };
    let out = crate::array::array_new_ref("java/lang/Throwable", len + 1)?;
    if let Some(a) = old {
        for i in 0..len {
            let v = crate::array::array_load(&a, i)?;
            crate::array::array_store(&out, i, v, ctx)?;
        }
    }
    crate::array::array_store(&out, len, Value::Ref(exc), ctx)?;
    crate::heap::with_obj_mut(*r, |o| o.slots[2] = Value::Ref(Some(out)));
    Ok(Value::Void)
}

fn n_throwable_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Throwable.toString");
    let cls = dotted(&crate::heap::with_obj(*r, |o| o.jclass.clone()));
    let msg = match &crate::heap::with_obj(*r, |o| o.slots[0].clone()) {
        Value::Ref(Some(o)) if is_string(o) => Some(object::string_value(o).to_string()),
        _ => None,
    };
    let text = match msg {
        Some(m) => format!("{cls}: {m}"),
        None => cls,
    };
    Ok(Value::Ref(Some(object::new_string(&text))))
}

// ---------------------------------------------------------------- Boxed types
//
// Every boxed class stores its payload in slot 0. valueOf boxes, xValue
// unboxes, toString renders with Java's formatting rules.
//
// JLS 7 §5.1.7 requires `valueOf` to return **cached, identical** objects
// for Integer/Long/Short/Byte in [-128, 127], Character in [0, 127] and
// Boolean (TRUE/FALSE); `==` on boxed values inside those ranges must be
// true. Float and Double are exempt (fresh object each time). Boxed
// `equals` compares payloads and rejects mixed types; `hashCode` follows
// the JDK contract (Long folds high/low, Boolean 1231/1237, Float/Double
// use canonicalized IEEE bits).

/// (class, payload-as-i64) -> cached boxed object. Only classes with a
/// mandatory cache are inserted; Float/Double never consult this.
/// Process-global since M4 Wave 1 (T04); `OnceLock` because `HashMap::new`
/// is not const. Allocation under its own lock is heap-lock law L2
/// (cache→heap nesting, never the reverse).
static BOXED_CACHE: OnceLock<Mutex<HashMap<(&'static str, i64), JRef>>> = OnceLock::new();

fn boxed_cache() -> &'static Mutex<HashMap<(&'static str, i64), JRef>> {
    BOXED_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns the shared boxed object for `(class, v)` when `v` is inside the
/// class's mandatory identity range, allocating it on first use.
fn boxed_cached(class: &'static str, v: i64, lo: i64, hi: i64) -> Option<JRef> {
    if v < lo || v > hi {
        return None;
    }
    let mut cache = boxed_cache().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = cache.get(&(class, v)) {
        return Some(*existing);
    }
    let slot = if class == "java/lang/Long" {
        Value::Long(v)
    } else {
        Value::Int(v as i32)
    };
    let r = new_obj(class, 1).ok()?;
    object::obj_set(&r, 0, slot);
    cache.insert((class, v), r);
    Some(r)
}

/// Canonicalizes an f32 the way `Float.floatToIntBits` does: all NaNs become
/// the canonical 0x7fc00000 pattern (negative zero is preserved).
fn float_to_int_bits(v: f32) -> i32 {
    if v.is_nan() {
        return 0x7fc00000u32 as i32;
    }
    v.to_bits() as i32
}

/// Canonicalizes an f64 the way `Double.doubleToLongBits` does.
fn double_to_long_bits(v: f64) -> i64 {
    if v.is_nan() {
        return 0x7ff8000000000000u64 as i64;
    }
    v.to_bits() as i64
}

// Exact IEEE bit exposure (corpus gap F4 support): the HexFloat fixture
// pins hex-float rounding through floatToIntBits/doubleToLongBits — the
// only JVM-exact, formatting-independent way to compare bit patterns.
fn n_float_to_int_bits(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(float_to_int_bits(
        arg(args, "Float.floatToIntBits").expect_float(),
    )))
}
fn n_float_to_raw_int_bits(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(
        arg(args, "Float.floatToRawIntBits")
            .expect_float()
            .to_bits() as i32,
    ))
}
fn n_double_to_long_bits(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Long(double_to_long_bits(
        arg(args, "Double.doubleToLongBits").expect_double(),
    )))
}
fn n_double_to_raw_long_bits(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Long(
        arg(args, "Double.doubleToRawLongBits")
            .expect_double()
            .to_bits() as i64,
    ))
}

// EXTERNAL-B8 (jbox2d port, 2026-11-13): the two reverse
// bit-reinterpretation statics. Raw bit moves, exactly like HotSpot's
// intrinsics: intBitsToFloat(longBitsToDouble) of any pattern — NaN
// payloads included — preserves the bits (floatToRawIntBits of the result
// round-trips), while floatToIntBits still canonicalizes the NaN on the
// way OUT, which is the distinction Float.equals is specified through.
fn n_int_bits_to_float(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Float(f32::from_bits(
        arg(args, "Float.intBitsToFloat").expect_int() as u32,
    )))
}
fn n_long_bits_to_double(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Double(f64::from_bits(
        arg(args, "Double.longBitsToDouble").expect_long() as u64,
    )))
}

macro_rules! boxed_class {
    (
        $class:expr, $prim:expr,
        value_of: $value_of:ident, slot_get: $slot_get:ident,
        unbox: $unbox:ident, $slot_ret:ident,
        to_string: $to_string:ident, $kind:expr
        $(, cache: $lo:expr, $hi:expr)?
    ) => {
        // pub(crate): reflect.rs routes Field.get / Method.invoke returns
        // through the same valueOf natives so the identity caches apply.
        pub(crate) fn $value_of(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let v = arg(args, "valueOf").$slot_get();
            $(
                if let Some(r) = boxed_cached($class, v as i64, $lo, $hi) {
                    return Ok(Value::Ref(Some(r)));
                }
            )?
            let r = object::new_obj($class, 1)?;
            object::obj_set(&r, 0, Value::$slot_ret(v));
            Ok(Value::Ref(Some(r)))
        }
        fn $unbox(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let r = this(recv, "unboxing");
            Ok(Value::$slot_ret(
                crate::heap::with_obj(*r, |o| o.slots[0].clone()).$slot_get(),
            ))
        }
        fn $to_string(
            recv: Option<&JRef>,
            _args: &[Value],
            _ctx: Dispatch,
        ) -> Result<Value, Throw> {
            let r = this(recv, "boxed toString");
            let v = crate::heap::with_obj(*r, |o| o.slots[0].clone());
            let text = concat::format_value(&v, $kind, Dispatch::default())?;
            Ok(Value::Ref(Some(object::new_string(&text))))
        }
    };
}

/// `equals` for a boxed class: the argument must be an object of exactly the
/// same class with the same payload (`Integer(1).equals(Long(1L))` is false,
/// like the JDK, because the classes differ).
macro_rules! boxed_equals {
    ($name:ident, $class:expr, $pat:pat => $conv:expr) => {
        fn $name(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let r = this(recv, "boxed equals");
            let mine = crate::heap::with_obj(*r, |o| o.slots[0].clone());
            let other = match &arg(args, "equals") {
                Value::Ref(Some(o)) if crate::heap::with_obj(*o, |obj| &*obj.jclass == $class) => {
                    crate::heap::with_obj(*o, |obj| obj.slots[0].clone())
                }
                _ => return Ok(Value::Int(0)),
            };
            let a: i64 = match mine {
                $pat => $conv,
                _ => return Ok(Value::Int(0)),
            };
            let b: i64 = match other {
                $pat => $conv,
                _ => return Ok(Value::Int(0)),
            };
            Ok(Value::Int((a == b) as i32))
        }
    };
}

boxed_equals!(n_integer_equals, "java/lang/Integer", Value::Int(v) => v as i64);
boxed_equals!(n_long_equals, "java/lang/Long", Value::Long(v) => v);
boxed_equals!(n_byte_equals, "java/lang/Byte", Value::Int(v) => v as i64);
boxed_equals!(n_short_equals, "java/lang/Short", Value::Int(v) => v as i64);
boxed_equals!(n_boolean_equals, "java/lang/Boolean", Value::Int(v) => v as i64);
boxed_equals!(n_character_equals, "java/lang/Character", Value::Int(v) => v as i64);

fn n_float_equals(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Float.equals");
    let mine = crate::heap::with_obj(*r, |o| o.slots[0].clone());
    let other = match &arg(args, "equals") {
        Value::Ref(Some(o))
            if crate::heap::with_obj(*o, |obj| &*obj.jclass == "java/lang/Float") =>
        {
            crate::heap::with_obj(*o, |obj| obj.slots[0].clone())
        }
        _ => return Ok(Value::Int(0)),
    };
    let (a, b) = match (mine, other) {
        (Value::Float(a), Value::Float(b)) => (a, b),
        _ => return Ok(Value::Int(0)),
    };
    // Float.equals compares floatToIntBits: NaN==NaN, +0.0 != -0.0
    Ok(Value::Int(
        (float_to_int_bits(a) == float_to_int_bits(b)) as i32,
    ))
}

fn n_double_equals(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Double.equals");
    let mine = crate::heap::with_obj(*r, |o| o.slots[0].clone());
    let other = match &arg(args, "equals") {
        Value::Ref(Some(o))
            if crate::heap::with_obj(*o, |obj| &*obj.jclass == "java/lang/Double") =>
        {
            crate::heap::with_obj(*o, |obj| obj.slots[0].clone())
        }
        _ => return Ok(Value::Int(0)),
    };
    let (a, b) = match (mine, other) {
        (Value::Double(a), Value::Double(b)) => (a, b),
        _ => return Ok(Value::Int(0)),
    };
    Ok(Value::Int(
        (double_to_long_bits(a) == double_to_long_bits(b)) as i32,
    ))
}

/// `hashCode` for a boxed class holding an Int payload (Integer, Short,
/// Byte, Boolean, Character): the value itself, except Boolean (1231/1237).
macro_rules! boxed_int_hash_code {
    ($name:ident) => {
        fn $name(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let r = this(recv, "boxed hashCode");
            Ok(Value::Int(
                crate::heap::with_obj(*r, |o| o.slots[0].clone()).expect_int(),
            ))
        }
    };
}

boxed_int_hash_code!(n_integer_hash_code);
boxed_int_hash_code!(n_byte_hash_code);
boxed_int_hash_code!(n_short_hash_code);
boxed_int_hash_code!(n_character_hash_code);

fn n_boolean_hash_code(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Boolean.hashCode");
    Ok(Value::Int(
        if crate::heap::with_obj(*r, |o| o.slots[0].clone()).expect_int() != 0 {
            1231
        } else {
            1237
        },
    ))
}

fn n_long_hash_code(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Long.hashCode");
    let v = crate::heap::with_obj(*r, |o| o.slots[0].clone()).expect_long();
    Ok(Value::Int((v ^ (v >> 32)) as i32))
}

fn n_float_hash_code(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Float.hashCode");
    Ok(Value::Int(float_to_int_bits(
        crate::heap::with_obj(*r, |o| o.slots[0].clone()).expect_float(),
    )))
}

fn n_double_hash_code(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Double.hashCode");
    let bits =
        double_to_long_bits(crate::heap::with_obj(*r, |o| o.slots[0].clone()).expect_double());
    Ok(Value::Int((bits ^ (bits >> 32)) as i32))
}

boxed_class! {
    "java/lang/Integer", "I",
    value_of: n_integer_value_of, slot_get: expect_int,
    unbox: n_integer_int_value, Int,
    to_string: n_integer_to_string, 'I', cache: -128, 127
}
boxed_class! {
    "java/lang/Long", "J",
    value_of: n_long_value_of, slot_get: expect_long,
    unbox: n_long_long_value, Long,
    to_string: n_long_to_string, 'J', cache: -128, 127
}
boxed_class! {
    "java/lang/Float", "F",
    value_of: n_float_value_of, slot_get: expect_float,
    unbox: n_float_float_value, Float,
    to_string: n_float_to_string, 'F'
}
boxed_class! {
    "java/lang/Double", "D",
    value_of: n_double_value_of, slot_get: expect_double,
    unbox: n_double_double_value, Double,
    to_string: n_double_to_string, 'D'
}
boxed_class! {
    "java/lang/Byte", "B",
    value_of: n_byte_value_of, slot_get: expect_int,
    unbox: n_byte_byte_value, Int,
    to_string: n_byte_to_string, 'I', cache: -128, 127
}
boxed_class! {
    "java/lang/Short", "S",
    value_of: n_short_value_of, slot_get: expect_int,
    unbox: n_short_short_value, Int,
    to_string: n_short_to_string, 'I', cache: -128, 127
}
boxed_class! {
    "java/lang/Boolean", "Z",
    value_of: n_boolean_value_of, slot_get: expect_int,
    unbox: n_boolean_boolean_value, Int,
    to_string: n_boolean_to_string, 'Z', cache: 0, 1
}
boxed_class! {
    "java/lang/Character", "C",
    value_of: n_character_value_of, slot_get: expect_int,
    unbox: n_character_char_value, Int,
    to_string: n_character_to_string, 'C', cache: 0, 127
}

// EXTERNAL-B6 (jbox2d port, 2026-11-13): the boxed classes are
// instantiable — `new Integer(int)` and friends now resolve. JDK-exact
// law: a constructor FRESH-allocates. The JLS 5.1.7 identity cache
// belongs to boxing conversion (valueOf/autoboxing) only; on Temurin
// 21.0.13 `new Integer(127) == new Integer(127)` is FALSE, so the
// <init> rows deliberately do NOT route through the valueOf caches.
// (The owner's bug.md said the rows "alias valueOf so the identity
// cache applies" — the pinned JVM vector says otherwise; the fixture
// carries the real output and the correction is recorded in the wave
// evidence.) The (String) forms delegate to the JDK-exact parse
// helpers; Short/Byte range-check with the JDK's exact
// "Value out of range" message quoting the input string.

fn boxed_init_slot(recv: Option<&JRef>, v: Value) -> Result<Value, Throw> {
    let r = this(recv, "boxed <init>");
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = v);
    Ok(Value::Void)
}

macro_rules! boxed_init_prim {
    ($name:ident, $who:expr) => {
        fn $name(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let v = arg(args, $who).clone();
            boxed_init_slot(recv, v)
        }
    };
}

boxed_init_prim!(n_integer_init_i, "Integer.<init>");
boxed_init_prim!(n_long_init_j, "Long.<init>");
boxed_init_prim!(n_double_init_d, "Double.<init>");
boxed_init_prim!(n_float_init_f, "Float.<init>");
boxed_init_prim!(n_boolean_init_z, "Boolean.<init>");
boxed_init_prim!(n_character_init_c, "Character.<init>");
boxed_init_prim!(n_short_init_s_prim, "Short.<init>");
boxed_init_prim!(n_byte_init_b_prim, "Byte.<init>");

/// `Integer(String)` — `this(parseInt(s, 10))`, fresh box.
fn n_integer_init_s(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let v = n_integer_parse_int(None, args, ctx)?;
    boxed_init_slot(recv, v)
}

/// `Long(String)` — `this(parseLong(s, 10))`, fresh box.
fn n_long_init_s(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let v = n_long_parse_long(None, args, ctx)?;
    boxed_init_slot(recv, v)
}

/// `Float(String)` — `this(parseFloat(s))`, fresh box.
fn n_float_init_s(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let v = n_float_parse_float(None, args, ctx)?;
    boxed_init_slot(recv, v)
}

/// `Double(String)` — `this(parseDouble(s))`, fresh box.
fn n_double_init_s(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let v = n_double_parse_double(None, args, ctx)?;
    boxed_init_slot(recv, v)
}

/// `Boolean(String)` — `this(parseBoolean(s))`: exactly
/// `"true".equalsIgnoreCase(s)`; null yields false like the JDK.
fn n_boolean_init_s(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let v = match arg(args, "Boolean.<init>") {
        Value::Ref(Some(r)) if is_string(r) => {
            let s = object::string_value(r);
            // "true" is pure ASCII, so ASCII case-folding reproduces the
            // JDK's Character-based equalsIgnoreCase exactly here.
            Value::Int(if s.eq_ignore_ascii_case("true") { 1 } else { 0 })
        }
        Value::Ref(None) => Value::Int(0),
        other => return Err(internal_error(&format!("Boolean.<init> arg {other:?}"))),
    };
    boxed_init_slot(recv, v)
}

/// Shared shape of `Short(String)` / `Byte(String)`: parse as int, then
/// the JDK's exact range message (`"Value:\"" + s + "\" Radix:" + radix`
/// with radix 10).
fn boxed_init_narrow(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
    lo: i64,
    hi: i64,
    who: &str,
) -> Result<Value, Throw> {
    let v = n_integer_parse_int(None, args, ctx)?.expect_int() as i64;
    if v < lo || v > hi {
        let s = match arg(args, who) {
            Value::Ref(Some(r)) if is_string(r) => object::string_value(r).to_string(),
            _ => String::new(),
        };
        return Err(throw_new(
            "java/lang/NumberFormatException",
            Some(&format!("Value out of range. Value:\"{s}\" Radix:10")),
        ));
    }
    boxed_init_slot(recv, Value::Int(v as i32))
}

fn n_short_init_s(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    boxed_init_narrow(
        recv,
        args,
        ctx,
        i16::MIN as i64,
        i16::MAX as i64,
        "Short.<init>",
    )
}

fn n_byte_init_s(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    boxed_init_narrow(
        recv,
        args,
        ctx,
        i8::MIN as i64,
        i8::MAX as i64,
        "Byte.<init>",
    )
}

// EXTERNAL-B10 (jbox2d port, 2026-11-13): java.util.Random as a real
// builtin object — the JDK's 48-bit LCG reproduced exactly from OpenJDK
// Random.java (seed in slot 0 as a Long; haveNextNextGaussian in slot 1
// as an Int flag; nextNextGaussian in slot 2 as a Double). jbox2d's
// MathUtils uses it for contact-jitter, so the produced stream must be
// BIT-IDENTICAL to the JDK's — `new Random(42)` is fully deterministic
// and every one of these natives is differential-fixture-pinned below.
// The no-arg constructor is seeded from the uniquifier XOR nanoTime like
// the JDK: deliberately unspecified, never printed in a fixture.

const RANDOM_MULTIPLIER: i64 = 0x5DEECE66D;
const RANDOM_ADDEND: i64 = 0xB;
const RANDOM_MASK: i64 = (1i64 << 48) - 1;
const RANDOM_DOUBLE_UNIT: f64 = f64::from_bits(0x3CA0000000000000); // 2^-53

/// The JDK scramble: `this.seed = (seed ^ multiplier) & mask`.
fn random_scramble(s: i64) -> i64 {
    (s ^ RANDOM_MULTIPLIER) & RANDOM_MASK
}

fn random_seed_of(r: &JRef) -> i64 {
    crate::heap::with_obj(*r, |o| o.slots[0].clone().expect_long())
}

/// `next(bits)` — the LCG core: seed = (seed*multiplier + addend) & mask;
/// return (int)(seed >>> (48 - bits)).
fn random_next(r: &JRef, bits: i32) -> i32 {
    let seed = random_seed_of(r);
    let next = seed
        .wrapping_mul(RANDOM_MULTIPLIER)
        .wrapping_add(RANDOM_ADDEND)
        & RANDOM_MASK;
    crate::heap::with_obj_mut(*r, |o| o.slots[0] = Value::Long(next));
    ((next as u64) >> (48 - bits) as u64) as i32
}

/// The shared seed-uniquifier state (JDK: an AtomicLong stepped by the
/// odd multiplier each no-arg construction).
static RANDOM_UNIQUIFIER: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(8682522807148012);

/// The JDK's CAS loop (`seedUniquifier()`): multiply by the odd constant
/// until the compare-and-set wins.
fn random_next_uniquifier() -> i64 {
    use std::sync::atomic::Ordering;
    loop {
        let current = RANDOM_UNIQUIFIER.load(Ordering::SeqCst);
        let next = current.wrapping_mul(1181783497276652981);
        if RANDOM_UNIQUIFIER
            .compare_exchange_weak(current, next, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return current;
        }
    }
}

/// `Random()` — seedUniquifier ^ System.nanoTime (unspecified stream).
fn n_random_init(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Random.<init>");
    let uniq = random_next_uniquifier();
    let nano = n_system_nano_time(None, &[], _ctx)?.expect_long();
    let seed = random_scramble(uniq ^ nano);
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = Value::Long(seed);
        o.slots[1] = Value::Int(0);
        o.slots[2] = Value::Double(0.0);
    });
    Ok(Value::Void)
}

/// `Random(long seed)` — the deterministic JDK constructor.
fn n_random_init_j(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Random.<init>");
    let seed = random_scramble(arg(args, "Random.<init>").expect_long());
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = Value::Long(seed);
        o.slots[1] = Value::Int(0);
        o.slots[2] = Value::Double(0.0);
    });
    Ok(Value::Void)
}

/// `setSeed(long)` — the same scramble + gaussian-cache reset.
fn n_random_set_seed(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Random.setSeed");
    let seed = random_scramble(arg(args, "Random.setSeed").expect_long());
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[0] = Value::Long(seed);
        o.slots[1] = Value::Int(0);
        o.slots[2] = Value::Double(0.0);
    });
    Ok(Value::Void)
}

/// `nextInt()` — next(32).
fn n_random_next_int(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextInt");
    Ok(Value::Int(random_next(r, 32)))
}

/// `nextInt(bound)` — the JDK's power-of-two / rejection algorithm with
/// the exact IllegalArgumentException("bound must be positive").
fn n_random_next_int_bound(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextInt");
    let bound = arg(args, "Random.nextInt").expect_int();
    if bound <= 0 {
        return Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some("bound must be positive"),
        ));
    }
    let m = bound - 1;
    if (bound & m) == 0 {
        // power of two: (int)((bound * (long)next(31)) >> 31)
        let r31 = random_next(r, 31) as i64;
        return Ok(Value::Int(((bound as i64 * r31) >> 31) as i32));
    }
    // rejection loop: JDK `for (int u = r; u - (r = u % bound) + m < 0; u = next(31));`
    let mut u = random_next(r, 31);
    let mut res = u % bound;
    while u.wrapping_sub(res).wrapping_add(m) < 0 {
        u = random_next(r, 31);
        res = u % bound;
    }
    Ok(Value::Int(res))
}

/// `nextLong()` — ((long)next(32) << 32) + next(32).
fn n_random_next_long(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextLong");
    let hi = random_next(r, 32) as i64;
    let lo = random_next(r, 32) as i64;
    Ok(Value::Long((hi << 32).wrapping_add(lo)))
}

/// `nextBoolean()` — next(1) != 0.
fn n_random_next_boolean(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextBoolean");
    Ok(Value::Int(if random_next(r, 1) != 0 { 1 } else { 0 }))
}

/// `nextFloat()` — next(24) / (float)(1 << 24).
fn n_random_next_float(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextFloat");
    let v = random_next(r, 24) as f32;
    Ok(Value::Float(v / 16777216.0f32))
}

/// `nextDouble()` — ((long)next(26) << 27 + next(27)) * 2^-53.
fn n_random_next_double(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextDouble");
    let hi = random_next(r, 26) as i64;
    let lo = random_next(r, 27) as i64;
    Ok(Value::Double(
        ((hi << 27).wrapping_add(lo)) as f64 * RANDOM_DOUBLE_UNIT,
    ))
}

/// `nextGaussian()` — the JDK's polar method with the one-slot cache
/// (slots 1-2): strictly follow Random.java's loop and branch order.
/// The internal `Math.log(s)` rides Rust's `f64::ln`, which on this
/// platform is the same correctly-rounded system libm the HotSpot
/// Math.log intrinsic calls — the Externals fixture proves the produced
/// stream byte-identical vs Temurin 21.0.13.
fn n_random_next_gaussian(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextGaussian");
    let cached = crate::heap::with_obj(*r, |o| o.slots[1].clone().expect_int());
    if cached != 0 {
        crate::heap::with_obj_mut(*r, |o| o.slots[1] = Value::Int(0));
        return Ok(crate::heap::with_obj(*r, |o| o.slots[2].clone()));
    }
    let (v1, v2, s) = loop {
        // 2 * nextDouble() - 1, through the same generator state
        let d1 = match n_random_next_double(Some(r), &[], ctx)? {
            Value::Double(d) => d,
            _ => unreachable!("nextDouble returns a double"),
        };
        let d2 = match n_random_next_double(Some(r), &[], ctx)? {
            Value::Double(d) => d,
            _ => unreachable!("nextDouble returns a double"),
        };
        let v1 = 2.0 * d1 - 1.0;
        let v2 = 2.0 * d2 - 1.0;
        let s = v1 * v1 + v2 * v2;
        if s < 1.0 && s != 0.0 {
            break (v1, v2, s);
        }
    };
    let multiplier = (-2.0 * s.ln() / s).sqrt();
    let result = v1 * multiplier;
    let next_next = v2 * multiplier;
    crate::heap::with_obj_mut(*r, |o| {
        o.slots[1] = Value::Int(1);
        o.slots[2] = Value::Double(next_next);
    });
    Ok(Value::Double(result))
}

/// `next(int bits)` — protected; exposed for subclasses (jbox2d does not
/// subclass Random, but the JDK method is part of the surface).
fn n_random_next(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Random.next");
    Ok(Value::Int(random_next(
        r,
        arg(args, "Random.next").expect_int(),
    )))
}

/// `nextBytes(byte[])` — the JDK's 4-bytes-per-rnd loop:
/// `for (i = 0; i < len; ) for (rnd = nextInt(), n = min(len-i, 4);
/// n-- > 0; rnd >>= 8) bytes[i++] = (byte)rnd;`
fn n_random_next_bytes(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Random.nextBytes");
    let arr = match arg(args, "Random.nextBytes") {
        Value::Ref(Some(a)) => *a,
        Value::Ref(None) => return Err(throw_new("java/lang/NullPointerException", None)),
        other => return Err(internal_error(&format!("Random.nextBytes arg {other:?}"))),
    };
    let len = crate::array::array_length(&arr)?;
    let mut i: i32 = 0;
    while i < len {
        let mut rnd = random_next(r, 32);
        let n = (len - i).min(4);
        let mut k = n;
        while k > 0 {
            crate::array::array_store(&arr, i, Value::Int(rnd as i8 as i32), _ctx)?;
            i += 1;
            rnd >>= 8;
            k -= 1;
        }
    }
    Ok(Value::Void)
}

fn n_integer_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &arg(args, "Integer.toString").expect_int().to_string(),
    ))))
}

/// `Integer.toString(int, int)` — radix form (corpus wave 1). JDK rule:
/// a radix outside 2..=36 falls back to base 10, digits are lowercase.
fn n_integer_to_string_radix(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let i = arg(args, "Integer.toString").expect_int();
    let radix = match args.get(1) {
        Some(v) => v.expect_int(),
        None => {
            return Err(crate::internal_error(
                "j2r internal: Integer.toString radix",
            ))
        }
    };
    let radix = if (2..=36).contains(&radix) { radix } else { 10 };
    Ok(Value::Ref(Some(object::new_string(&int_to_radix(
        i, radix,
    )))))
}

fn int_to_radix(i: i32, radix: i32) -> String {
    if i == 0 {
        return "0".to_string();
    }
    let mut digits = Vec::new();
    let mut v = i as i64;
    if v < 0 {
        // i32::MIN negated safely via i64
        v = -v;
    }
    while v > 0 {
        digits.push(std::char::from_digit((v % radix as i64) as u32, radix as u32).unwrap());
        v /= radix as i64;
    }
    if i < 0 {
        digits.push('-');
    }
    digits.into_iter().rev().collect()
}

fn n_long_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &arg(args, "Long.toString").expect_long().to_string(),
    ))))
}

/// `Long.toString(long, int)` — radix form (corpus wave 1), same JDK
/// fallback rule as the Integer variant.
fn n_long_to_string_radix(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let l = arg(args, "Long.toString").expect_long();
    let radix = match args.get(1) {
        Some(v) => v.expect_int(),
        None => return Err(crate::internal_error("j2r internal: Long.toString radix")),
    };
    let radix = if (2..=36).contains(&radix) { radix } else { 10 };
    if l == 0 {
        return Ok(Value::Ref(Some(object::new_string("0"))));
    }
    let mut digits = Vec::new();
    let mut v: i128 = l as i128;
    if v < 0 {
        v = -v;
    }
    while v > 0 {
        digits.push(std::char::from_digit((v % radix as i128) as u32, radix as u32).unwrap());
        v /= radix as i128;
    }
    if l < 0 {
        digits.push('-');
    }
    digits.reverse();
    let s: String = digits.into_iter().collect();
    Ok(Value::Ref(Some(object::new_string(&s))))
}

fn n_double_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &javafmt::double_to_string(arg(args, "Double.toString").expect_double()),
    ))))
}

fn n_float_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        &javafmt::float_to_string(arg(args, "Float.toString").expect_float()),
    ))))
}

fn n_boolean_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Ref(Some(object::new_string(
        if arg(args, "Boolean.toString").expect_int() != 0 {
            "true"
        } else {
            "false"
        },
    ))))
}

fn n_character_to_string_static(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, "Character.toString").expect_int();
    let c = char::from_u32(v as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
    Ok(Value::Ref(Some(object::new_string(&c.to_string()))))
}

/// Java-exact integer parsing shared by `Integer.parseInt`/`Long.parseLong`.
fn java_parse_i64(s: Option<&str>, lo_inclusive: i64, hi_inclusive: i64) -> Result<i64, Throw> {
    let s = match s {
        Some(s) => s,
        None => {
            return Err(throw_new(
                "java/lang/NumberFormatException",
                Some("Cannot parse null string"),
            ))
        }
    };
    let fail = || {
        throw_new(
            "java/lang/NumberFormatException",
            Some(&format!("For input string: \"{s}\"")),
        )
    };
    let (negative, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => match s.strip_prefix('+') {
            Some(rest) => (false, rest),
            None => (false, s),
        },
    };
    if digits.is_empty() {
        return Err(fail());
    }
    let mut acc: i128 = 0;
    for c in digits.chars() {
        let d = match c.to_digit(10) {
            Some(d) => d as i128,
            None => return Err(fail()),
        };
        acc = acc * 10 + d;
        if acc > -(lo_inclusive as i128) {
            return Err(fail());
        }
    }
    let v = if negative {
        -acc
    } else {
        if acc > hi_inclusive as i128 {
            return Err(fail());
        }
        acc
    };
    if v < lo_inclusive as i128 || v > hi_inclusive as i128 {
        return Err(fail());
    }
    Ok(v as i64)
}

/// Radix form of [`java_parse_i64`] with Temurin-verified messages (see
/// `n_long_parse_long_radix`). Negative accumulation keeps MIN parseable.
fn java_parse_i64_radix(
    s: Option<&str>,
    radix: i32,
    lo_inclusive: i64,
    hi_inclusive: i64,
) -> Result<i64, Throw> {
    if radix < 2 {
        return Err(throw_new(
            "java/lang/NumberFormatException",
            Some(&format!("radix {radix} less than Character.MIN_RADIX")),
        ));
    }
    if radix > 36 {
        return Err(throw_new(
            "java/lang/NumberFormatException",
            Some(&format!("radix {radix} greater than Character.MAX_RADIX")),
        ));
    }
    let s = match s {
        Some(s) => s,
        None => {
            return Err(throw_new(
                "java/lang/NumberFormatException",
                Some("Cannot parse null string"),
            ))
        }
    };
    let fail = || {
        let msg = if radix == 10 {
            format!("For input string: \"{s}\"")
        } else {
            format!("For input string: \"{s}\" under radix {radix}")
        };
        throw_new("java/lang/NumberFormatException", Some(&msg))
    };
    let (negative, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => match s.strip_prefix('+') {
            Some(rest) => (false, rest),
            None => (false, s),
        },
    };
    if digits.is_empty() {
        return Err(fail());
    }
    let r = radix as i128;
    let mut acc: i128 = 0;
    for c in digits.chars() {
        let d = match c.to_digit(radix as u32) {
            Some(d) => d as i128,
            None => return Err(fail()),
        };
        acc = acc * r - d;
        if acc < lo_inclusive as i128 {
            return Err(fail());
        }
    }
    let v: i128 = if negative {
        acc
    } else {
        if -acc > hi_inclusive as i128 {
            return Err(fail());
        }
        -acc
    };
    Ok(v as i64)
}

fn n_integer_parse_int(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    let s = match arg(args, "Integer.parseInt") {
        Value::Ref(Some(o)) if is_string(o) => Some(object::string_value(o).to_string()),
        Value::Ref(None) => None,
        _ => return Err(internal_error("Integer.parseInt needs a String")),
    };
    let v = java_parse_i64(s.as_deref(), i32::MIN as i64, i32::MAX as i64)?;
    Ok(Value::Int(v as i32))
}

// -- float/double string parsing (corpus wave 1) ---------------------------
// Semantics verified against Temurin 21.0.13 (corpus/FPProbe.java):
// null -> NPE naming String.trim(); leading/trailing whitespace trimmed;
// "empty String" for the trimmed-empty input; exact spellings Infinity /
// +/-Infinity / NaN (case-sensitive - "inf" is a NumberFormatException); one
// trailing f/F/d/D suffix accepted; correctly-rounded f32/f64 parsing
// (Rust's parse is correctly rounded, matching HotSpot's strict parsing).
// Hexadecimal forms (0x1.8p1) route to the dedicated hexfloat module
// (corpus gap F4, closed 2026-11-05; grammar + rounding pinned with
// corpus/HexFloatProbe.java).
fn n_float_parse_float(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    let s = match arg(args, "Float.parseFloat") {
        Value::Ref(Some(o)) if is_string(o) => Some(object::string_value(o).to_string()),
        Value::Ref(None) => None,
        _ => return Err(internal_error("Float.parseFloat needs a String")),
    };
    // f32 directly (never through f64: double rounding is observable)
    let t = s.as_deref().unwrap_or("").trim();
    let fail = || {
        throw_new(
            "java/lang/NumberFormatException",
            Some(&if t.is_empty() {
                "empty String".to_string()
            } else {
                format!("For input string: \"{t}\"")
            }),
        )
    };
    let body = match t.strip_suffix(['f', 'F', 'd', 'D']) {
        Some(rest) if !rest.is_empty() => rest,
        _ => t,
    };
    // hex forms first (F4): exact bit patterns, correctly rounded
    if let Some(bits) = crate::hexfloat::parse_bits(body, true) {
        return Ok(Value::Float(f32::from_bits(bits as u32)));
    }
    let f = match body {
        "Infinity" | "+Infinity" => f32::INFINITY,
        "-Infinity" => f32::NEG_INFINITY,
        "NaN" => f32::NAN,
        _ => {
            // reject the Rust-only spellings Java rejects
            let low = body.to_ascii_lowercase();
            let low = low.trim_start_matches('+');
            if low == "infinity" || low == "inf" || low == "nan" {
                return Err(fail());
            }
            body.parse::<f32>().map_err(|_| fail())?
        }
    };
    Ok(Value::Float(f))
}

fn n_double_parse_double(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    let s = match arg(args, "Double.parseDouble") {
        Value::Ref(Some(o)) if is_string(o) => Some(object::string_value(o).to_string()),
        Value::Ref(None) => None,
        _ => return Err(internal_error("Double.parseDouble needs a String")),
    };
    let t = s.as_deref().unwrap_or("").trim();
    let fail = || {
        throw_new(
            "java/lang/NumberFormatException",
            Some(&if t.is_empty() {
                "empty String".to_string()
            } else {
                format!("For input string: \"{t}\"")
            }),
        )
    };
    let body = match t.strip_suffix(['f', 'F', 'd', 'D']) {
        Some(rest) if !rest.is_empty() => rest,
        _ => t,
    };
    // hex forms first (F4): exact bit patterns, correctly rounded
    if let Some(bits) = crate::hexfloat::parse_bits(body, false) {
        return Ok(Value::Double(f64::from_bits(bits)));
    }
    let d = match body {
        "Infinity" | "+Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        "NaN" => f64::NAN,
        _ => {
            let low = body.to_ascii_lowercase();
            let low = low.trim_start_matches('+');
            if low == "infinity" || low == "inf" || low == "nan" {
                return Err(fail());
            }
            body.parse::<f64>().map_err(|_| fail())?
        }
    };
    Ok(Value::Double(d))
}

fn n_long_parse_long(_r: Option<&JRef>, args: &[Value], _c: Dispatch) -> Result<Value, Throw> {
    let s = match arg(args, "Long.parseLong") {
        Value::Ref(Some(o)) if is_string(o) => Some(object::string_value(o).to_string()),
        Value::Ref(None) => None,
        _ => return Err(internal_error("Long.parseLong needs a String")),
    };
    let v = java_parse_i64(s.as_deref(), i64::MIN, i64::MAX)?;
    Ok(Value::Long(v))
}

/// `Long.parseLong(String, int)` — radix form (corpus wave 1). Messages
/// verified against Temurin 21.0.13 (corpus/ParseProbe.java): radix bounds
/// get the MIN_RADIX/MAX_RADIX wording, the base-10 failure keeps the
/// classic message, every other radix appends "under radix N". Parsing
/// accumulates NEGATIVELY so `Long.MIN_VALUE` parses exactly.
fn n_long_parse_long_radix(
    _r: Option<&JRef>,
    args: &[Value],
    _c: Dispatch,
) -> Result<Value, Throw> {
    let s = match arg(args, "Long.parseLong") {
        Value::Ref(Some(o)) if is_string(o) => Some(object::string_value(o).to_string()),
        Value::Ref(None) => None,
        _ => return Err(internal_error("Long.parseLong needs a String")),
    };
    let radix = match args.get(1) {
        Some(v) => v.expect_int(),
        None => return Err(internal_error("j2r internal: Long.parseLong radix")),
    };
    let v = java_parse_i64_radix(s.as_deref(), radix, i64::MIN, i64::MAX)?;
    Ok(Value::Long(v))
}

// ------------------------------------------------- M3 Wave 5: reflection
// The reflection subset (M3_PLAN §8). Every failure message below is the
// exact string Temurin 21 printed during the oracle capture (O/O2/O3.java,
// recorded in M3_EVIDENCE.md wave-5).

fn n_object_get_class(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Object.getClass");
    let name = crate::heap::with_obj(*r, |o| o.jclass.to_string());
    Ok(crate::reflect::class_value(&name))
}

/// A `Class.forName` binary name → the internal form the runtime keys on
/// (dots → slashes; the array-descriptor form keeps its brackets and
/// converts only inside `L...;`). Temurin accepts both `java.lang.String`
/// and `[Ljava.lang.String;` (oracle capture, R2 slice 1 evidence).
fn for_name_internal(name: &str) -> String {
    if let Some(rest) = name.strip_prefix('[') {
        format!("[{}", for_name_internal(rest))
    } else if let Some(comp) = name.strip_prefix('L').and_then(|s| s.strip_suffix(';')) {
        format!("L{};", comp.replace('.', "/"))
    } else {
        name.replace('.', "/")
    }
}

/// Is `internal` an array whose component the closed world knows?
/// Primitive components (`[I`) are always known; `L...;` components must
/// resolve (recursively for nested descriptors).
fn for_name_known(internal: &str, ctx: Dispatch) -> bool {
    if let Some(rest) = internal.strip_prefix('[') {
        return match rest.strip_prefix('L').and_then(|s| s.strip_suffix(';')) {
            Some(comp) => for_name_known(comp, ctx),
            // primitive component (or nested weirdness the verifier would
            // have rejected) — primitives are always resolvable
            None => true,
        };
    }
    // compiled registry first (a compiled class shadows nothing — the
    // closed world is disjoint — but the registry is authoritative for
    // user classes), then the builtin tables.
    ctx.class_info(internal).is_some() || crate::builtin::builtin_class_known(internal)
}

/// `Class.forName(String)` — the R2 slice-1 opening: runtime string
/// lookups resolve against the closed world (the generated crate's
/// metadata registry + the builtin tables). Oracle capture
/// (Temurin 21.0.13, this session): unknown name →
/// `ClassNotFoundException` whose message IS the input name; null name →
/// `NullPointerException: null`; `int` is NOT resolvable (primitives are
/// not classes by name); `[I` and `[Ljava.lang.String;` resolve; the
/// message carries the BINARY name verbatim.
fn n_class_for_name(_recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let name = match args.first() {
        Some(Value::Ref(None)) => {
            return Err(crate::throw_new(
                "java/lang/NullPointerException",
                Some("null"),
            ))
        }
        Some(Value::Ref(Some(r))) => crate::object::string_value(r),
        other => {
            return Err(internal_error(&format!(
                "Class.forName: expected string arg, found {other:?}"
            )))
        }
    };
    let internal = for_name_internal(&name);
    if for_name_known(&internal, ctx) {
        Ok(crate::reflect::class_value(&internal))
    } else {
        Err(crate::throw_new(
            "java/lang/ClassNotFoundException",
            Some(&name),
        ))
    }
}

/// `Class.forName(String, boolean, ClassLoader)` — the 3-arg overload:
/// resolution is identical (closed world; the `initialize` flag is
/// accepted — the runtime's lazy-clinit law already matches Temurin's
/// observable timing — and the loader is ignored: the system loader IS
/// the closed world).
fn n_class_for_name3(_recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    n_class_for_name(_recv, &args[..1], ctx)
}

/// `ClassLoader.getSystemClassLoader()` — R2 slice 1: returns an inert
/// `java/lang/ClassLoader` node. The closed world has exactly one loader
/// (the system loader IS the world), so the node carries no state and no
/// loader-resolved lookup exists to hang behavior on; `Class.forName`'s
/// 3-arg overload accepts and ignores it (Temurin-observable behavior on
/// the closed world is identical).
fn n_classloader_get_system(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = crate::object::new_obj("java/lang/ClassLoader", 0)?;
    Ok(Value::Ref(Some(r)))
}

fn n_class_get_name(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Class.getName");
    let name = reflect::class_name_of(r);
    Ok(Value::Ref(Some(object::new_string(&reflect::binary_name(
        &name,
    )))))
}

fn n_class_get_simple_name(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getSimpleName");
    let name = reflect::class_name_of(r);
    Ok(Value::Ref(Some(object::new_string(&reflect::simple_name(
        &name, ctx,
    )))))
}

fn n_class_to_string(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Class.toString");
    let name = reflect::class_name_of(r);
    Ok(Value::Ref(Some(object::new_string(
        &reflect::to_string_of(&name, ctx),
    ))))
}

fn n_class_is_instance(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Class.isInstance");
    let name = reflect::class_name_of(r);
    let ok = match &arg(args, "Class.isInstance") {
        Value::Ref(None) => false,
        Value::Ref(Some(o)) => {
            let jc = crate::heap::with_obj(*o, |ob| ob.jclass.to_string());
            ctx.is_subtype(&jc, &name)
        }
        _ => return Err(internal_error("Class.isInstance needs a reference")),
    };
    Ok(Value::Int(ok as i32))
}

fn n_class_is_assignable_from(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.isAssignableFrom");
    let name = reflect::class_name_of(r);
    let argref = match &arg(args, "Class.isAssignableFrom") {
        Value::Ref(Some(o)) => o,
        // Temurin: isAssignableFrom(null) is a plain NPE.
        Value::Ref(None) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(internal_error("Class.isAssignableFrom needs a Class")),
    };
    if &*crate::heap::with_obj(*argref, |o| o.jclass.clone()) != "java/lang/Class" {
        // Temurin casts; a non-Class arg is an internal dispatch bug here.
        return Err(internal_error("isAssignableFrom argument is not a Class"));
    }
    let other = reflect::class_name_of(argref);
    Ok(Value::Int(ctx.is_subtype(&other, &name) as i32))
}

fn n_class_is_interface(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.isInterface");
    let name = reflect::class_name_of(r);
    Ok(Value::Int(ctx.is_interface(&name) as i32))
}

fn n_class_is_array(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = this(recv, "Class.isArray");
    Ok(Value::Int(reflect::class_name_of(r).starts_with('[') as i32))
}

fn n_class_is_primitive(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.isPrimitive");
    let name = reflect::class_name_of(r);
    // Temurin 21 oracle: void.class.isPrimitive() is TRUE (V.java capture).
    Ok(Value::Int(reflect::prim_display(&name).is_some() as i32))
}

fn n_class_get_superclass(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getSuperclass");
    let name = reflect::class_name_of(r);
    Ok(match reflect::superclass_of(&name, ctx) {
        Some(sup) => reflect::class_value(&sup),
        None => Value::Ref(None),
    })
}

fn n_class_get_component_type(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getComponentType");
    let name = reflect::class_name_of(r);
    Ok(match reflect::component_type_of(&name) {
        Some(elem) => reflect::class_value(&elem),
        None => Value::Ref(None),
    })
}

fn n_class_get_declared_field(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getDeclaredField");
    let owner = reflect::class_name_of(r);
    reflect::get_declared_field(ctx, &owner, args)
}

fn n_class_get_declared_method(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getDeclaredMethod");
    let owner = reflect::class_name_of(r);
    reflect::get_declared_method(ctx, &owner, args)
}

fn n_field_get(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let f = this(recv, "Field.get");
    let obj = match args.first() {
        Some(Value::Ref(r)) => r.as_ref(),
        _ => return Err(internal_error("Field.get needs a receiver argument")),
    };
    reflect::field_get(ctx, f, obj)
}

fn n_field_set(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let f = this(recv, "Field.set");
    let obj = match args.first() {
        Some(Value::Ref(r)) => r.as_ref(),
        _ => return Err(internal_error("Field.set needs a receiver argument")),
    };
    let value = args.get(1).cloned().unwrap_or(Value::Ref(None));
    reflect::field_set(ctx, f, obj, &value)?;
    Ok(Value::Void)
}

fn n_method_invoke(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let m = this(recv, "Method.invoke");
    let target = match args.first() {
        Some(Value::Ref(r)) => r.as_ref(),
        _ => return Err(internal_error("Method.invoke needs a receiver argument")),
    };
    let argv = match args.get(1) {
        Some(Value::Ref(r)) => r.as_ref(),
        _ => return Err(internal_error("Method.invoke needs an args array")),
    };
    reflect::method_invoke(ctx, m, target, argv)
}

fn n_field_get_name(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let f = this(recv, "Field.getName");
    let (_, name, _, _, _, _) = reflect::field_slots(f);
    Ok(Value::Ref(Some(object::new_string(&name))))
}

/// `Field.getType`: map the stored field descriptor to the interned
/// `Class` object. Primitives are keyed by their bare descriptor (`I` ->
/// `int.class`), object descriptors drop the `L...;` wrapper, and array
/// descriptors (`[I`, `[Ljava/lang/String;`) intern as-is — exactly the
/// keys `component_type_of` hands back, so `int[].class.getComponentType()
/// == int.class` holds end to end (BigCache oracle).
fn n_field_get_type(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let f = this(recv, "Field.getType");
    let (_, _, desc, _, _, _) = reflect::field_slots(f);
    let d = desc.as_str();
    let key: &str = if d.starts_with('[') {
        d
    } else if d.starts_with('L') && d.len() >= 2 && d.ends_with(';') {
        &d[1..d.len() - 1]
    } else {
        d
    };
    let r = reflect::class_ref(key);
    Ok(Value::Ref(Some(r)))
}

fn n_method_get_name(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let m = this(recv, "Method.getName");
    let (_, name, _, _) = reflect::method_slots(m);
    Ok(Value::Ref(Some(object::new_string(&name))))
}

/// `Method.equals`: structural (same declaring class, name and descriptor),
/// not identity — Temurin hands back fresh Method objects per lookup
/// (fixture oracle: `m1 == m2` false but `m1.equals(m2)` true).
fn n_method_equals(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let m = this(recv, "Method.equals");
    // M4 Wave 1 lock law L3: the class check and the slot reads are
    // separate flat heap accesses, never nested inside one guard.
    let same = match &arg(args, "Method.equals") {
        Value::Ref(Some(other)) => {
            let is_method =
                crate::heap::with_obj(*other, |o| o.jclass.as_ref() == "java/lang/reflect/Method");
            is_method && {
                let (o0, o1, o2, _) = reflect::method_slots(other);
                let (s0, s1, s2, _) = reflect::method_slots(m);
                o0 == s0 && o1 == s1 && o2 == s2
            }
        }
        _ => false,
    };
    Ok(Value::Int(same as i32))
}

fn n_system_gc(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    crate::heap::gc_now();
    Ok(Value::Void)
}

// ---------------------------------------------------------------- Table
// ---------------------------------------------------------- java/lang/Thread
// M4 Wave 2 (T06): thin wrappers onto `crate::thread` (implementation,
// JDK contract and honesty notes live there).

fn n_thread_init_v_w(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    crate::thread::n_thread_init_v(recv, args)
}

fn n_thread_init_r_w(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    crate::thread::n_thread_init_r(recv, args)
}

fn n_thread_init_n_w(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    crate::thread::n_thread_init_n(recv, args)
}

fn n_thread_init_rn_w(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    crate::thread::n_thread_init_rn(recv, args)
}

fn n_thread_start_w(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    crate::thread::n_thread_start(recv, ctx)
}

fn n_thread_run_w(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    crate::thread::n_thread_run(recv, ctx)
}

fn n_thread_join0_w(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    crate::thread::n_thread_join(recv, &[])
}

fn n_thread_join1_w(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    crate::thread::n_thread_join(recv, args)
}

fn n_thread_join2_w(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    crate::thread::n_thread_join(recv, args)
}

fn n_thread_sleep1_w(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    crate::thread::n_thread_sleep(args)
}

fn n_thread_sleep2_w(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    crate::thread::n_thread_sleep(args)
}

fn n_thread_yield_w(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    crate::thread::n_thread_yield()
}

fn n_thread_current_thread_w(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_current_thread()
}

fn n_thread_set_daemon_w(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_set_daemon(recv, args)
}

fn n_thread_is_daemon_w(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_is_daemon(recv)
}

fn n_thread_get_name_w(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_get_name(recv)
}

fn n_thread_set_name_w(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_set_name(recv, args)
}

fn n_thread_is_alive_w(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_is_alive(recv)
}

fn n_thread_active_count_w(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_active_count()
}

fn n_thread_interrupt_w(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_interrupt(recv)
}

fn n_thread_is_interrupted_w(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_is_interrupted(recv)
}

fn n_thread_interrupted_w(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_interrupted()
}

/// The natives table: `(class, method, descriptor, implementation)`.
/// Classes use internal form (`a/b/C`). Keep sorted-ish by class for review.
pub static NATIVES: &[(&str, &str, &str, NativeFn)] = &[
    // java/lang/Thread (M4 Wave 2, T06 — implementation in thread.rs)
    ("java/lang/Thread", "<init>", "()V", n_thread_init_v_w),
    (
        "java/lang/Thread",
        "<init>",
        "(Ljava/lang/Runnable;)V",
        n_thread_init_r_w,
    ),
    (
        "java/lang/Thread",
        "<init>",
        "(Ljava/lang/String;)V",
        n_thread_init_n_w,
    ),
    (
        "java/lang/Thread",
        "<init>",
        "(Ljava/lang/Runnable;Ljava/lang/String;)V",
        n_thread_init_rn_w,
    ),
    ("java/lang/Thread", "start", "()V", n_thread_start_w),
    ("java/lang/Thread", "run", "()V", n_thread_run_w),
    ("java/lang/Thread", "join", "()V", n_thread_join0_w),
    ("java/lang/Thread", "join", "(J)V", n_thread_join1_w),
    ("java/lang/Thread", "join", "(JI)V", n_thread_join2_w),
    ("java/lang/Thread", "sleep", "(J)V", n_thread_sleep1_w),
    ("java/lang/Thread", "sleep", "(JI)V", n_thread_sleep2_w),
    ("java/lang/Thread", "yield", "()V", n_thread_yield_w),
    (
        "java/lang/Thread",
        "currentThread",
        "()Ljava/lang/Thread;",
        n_thread_current_thread_w,
    ),
    (
        "java/lang/Thread",
        "setDaemon",
        "(Z)V",
        n_thread_set_daemon_w,
    ),
    ("java/lang/Thread", "isDaemon", "()Z", n_thread_is_daemon_w),
    (
        "java/lang/Thread",
        "getName",
        "()Ljava/lang/String;",
        n_thread_get_name_w,
    ),
    (
        "java/lang/Thread",
        "setName",
        "(Ljava/lang/String;)V",
        n_thread_set_name_w,
    ),
    ("java/lang/Thread", "isAlive", "()Z", n_thread_is_alive_w),
    (
        "java/lang/Thread",
        "activeCount",
        "()I",
        n_thread_active_count_w,
    ),
    // T14 (Wave 4): interrupt status.
    ("java/lang/Thread", "interrupt", "()V", n_thread_interrupt_w),
    (
        "java/lang/Thread",
        "isInterrupted",
        "()Z",
        n_thread_is_interrupted_w,
    ),
    (
        "java/lang/Thread",
        "interrupted",
        "()Z",
        n_thread_interrupted_w,
    ),
    // java/lang/Object
    ("java/lang/Object", "<init>", "()V", n_object_init),
    // Object.clone (corpus wave 2, JZlib): the JVM default — a fresh
    // object of the same runtime class with shallow-copied field slots;
    // no constructor runs. CloneNotSupportedException must have been
    // declared by the caller (javac's Cloneable contract); the runtime
    // does not re-check it (the JDK checks at compile time).
    (
        "java/lang/Object",
        "clone",
        "()Ljava/lang/Object;",
        n_object_clone,
    ),
    ("java/lang/Object", "wait", "()V", n_object_wait),
    ("java/lang/Object", "wait", "(J)V", n_object_wait),
    ("java/lang/Object", "wait", "(JI)V", n_object_wait),
    ("java/lang/Object", "notify", "()V", n_object_notify),
    ("java/lang/Object", "notifyAll", "()V", n_object_notify_all),
    (
        "java/lang/Object",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_object_equals,
    ),
    ("java/lang/Object", "hashCode", "()I", n_object_hash_code),
    (
        "java/lang/Object",
        "toString",
        "()Ljava/lang/String;",
        n_object_to_string,
    ),
    // java/lang/Enum (corpus wave 2, JZlib WrapperType): javac compiles
    // every `enum` to `extends java/lang/Enum` with super(name, ordinal)
    // <init> calls. Slots: 0 = name (Str), 1 = ordinal (Int). equals is
    // final identity in the JDK; compareTo is the ordinal compare; valueOf
    // reads the class's synthesized $VALUES pool through the reflection
    // metadata (F6, corpus/EnumProbe.java pins the exact messages).
    (
        "java/lang/Enum",
        "getDeclaringClass",
        "()Ljava/lang/Class;",
        n_enum_get_declaring_class,
    ),
    (
        "java/lang/Enum",
        "<init>",
        "(Ljava/lang/String;I)V",
        n_enum_init,
    ),
    (
        "java/lang/Enum",
        "name",
        "()Ljava/lang/String;",
        n_enum_name,
    ),
    ("java/lang/Enum", "ordinal", "()I", n_enum_ordinal),
    (
        "java/lang/Enum",
        "toString",
        "()Ljava/lang/String;",
        n_enum_to_string,
    ),
    (
        "java/lang/Enum",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_enum_equals,
    ),
    (
        "java/lang/Enum",
        "compareTo",
        "(Ljava/lang/Enum;)I",
        n_enum_compare_to,
    ),
    (
        "java/lang/Enum",
        "valueOf",
        "(Ljava/lang/Class;Ljava/lang/String;)Ljava/lang/Enum;",
        n_enum_value_of,
    ),
    // Formatter engine (FmtProbe wave): String.format + PrintStream.printf
    // and the format alias. Implementation in formatter.rs; semantics
    // pinned by corpus/FmtProbe.java (43 rows incl. the HALF_UP-on-shortest-
    // repr %f law and the exact three error messages).
    (
        "java/lang/String",
        "format",
        "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;",
        formatter::n_string_format,
    ),
    (
        "java/io/PrintStream",
        "printf",
        "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/PrintStream;",
        formatter::n_print_stream_printf,
    ),
    (
        "java/io/PrintStream",
        "format",
        "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/PrintStream;",
        formatter::n_print_stream_printf,
    ),
    // java/lang/System
    ("java/lang/System", "exit", "(I)V", n_system_exit),
    (
        "java/lang/System",
        "currentTimeMillis",
        "()J",
        n_system_current_time_millis,
    ),
    ("java/lang/System", "nanoTime", "()J", n_system_nano_time),
    // System.arraycopy (corpus wave 1) — implementation in array.rs, JDK
    // messages verified against Temurin 21.0.13 (see its doc comment).
    (
        "java/lang/System",
        "arraycopy",
        "(Ljava/lang/Object;ILjava/lang/Object;II)V",
        n_system_arraycopy,
    ),
    // System.loadLibrary (T13 13c — the loading story): dlopen + JNI_OnLoad
    // through audited primitive 12; see the fn's doc comment.
    (
        "java/lang/System",
        "loadLibrary",
        "(Ljava/lang/String;)V",
        n_system_load_library,
    ),
    // java/lang/Math
    ("java/lang/Math", "abs", "(I)I", n_math_abs_i),
    ("java/lang/Math", "abs", "(J)J", n_math_abs_j),
    ("java/lang/Math", "abs", "(F)F", n_math_abs_f),
    ("java/lang/Math", "abs", "(D)D", n_math_abs_d),
    ("java/lang/Math", "max", "(II)I", n_math_max_i),
    ("java/lang/Math", "max", "(JJ)J", n_math_max_j),
    ("java/lang/Math", "max", "(FF)F", n_math_max_f),
    ("java/lang/Math", "max", "(DD)D", n_math_max_d),
    ("java/lang/Math", "min", "(II)I", n_math_min_i),
    ("java/lang/Math", "min", "(JJ)J", n_math_min_j),
    ("java/lang/Math", "min", "(FF)F", n_math_min_f),
    ("java/lang/Math", "min", "(DD)D", n_math_min_d),
    // java/lang/String
    ("java/lang/String", "<init>", "()V", n_string_init),
    (
        "java/lang/String",
        "<init>",
        "(Ljava/lang/String;)V",
        n_string_init_s,
    ),
    // char[] ctors (corpus wave 1)
    ("java/lang/String", "<init>", "([C)V", n_string_init_chars),
    (
        "java/lang/String",
        "<init>",
        "([CII)V",
        n_string_init_chars_range,
    ),
    ("java/lang/String", "length", "()I", n_string_length),
    (
        "java/lang/String",
        "toString",
        "()Ljava/lang/String;",
        n_string_to_string,
    ),
    ("java/lang/String", "isEmpty", "()Z", n_string_is_empty),
    ("java/lang/String", "charAt", "(I)C", n_string_char_at),
    (
        "java/lang/String",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_string_equals,
    ),
    ("java/lang/String", "hashCode", "()I", n_string_hash_code),
    // getChars (corpus wave 1)
    (
        "java/lang/String",
        "getChars",
        "(II[CI)V",
        n_string_get_chars,
    ),
    (
        "java/lang/String",
        "valueOf",
        "(I)Ljava/lang/String;",
        n_string_value_of_i,
    ),
    (
        "java/lang/String",
        "valueOf",
        "(J)Ljava/lang/String;",
        n_string_value_of_j,
    ),
    // Byte-side String natives (JZlib corpus wave 2): named-charset and
    // default-charset encode/decode with the probe-pinned JDK contracts
    // (see the implementation comment above).
    (
        "java/lang/String",
        "getBytes",
        "(Ljava/lang/String;)[B",
        n_string_get_bytes_named,
    ),
    (
        "java/lang/String",
        "getBytes",
        "()[B",
        n_string_get_bytes_default,
    ),
    (
        "java/lang/String",
        "<init>",
        "([BLjava/lang/String;)V",
        n_string_init_bytes_named,
    ),
    (
        "java/lang/String",
        "<init>",
        "([BIILjava/lang/String;)V",
        n_string_init_bytes_named_range,
    ),
    (
        "java/lang/String",
        "valueOf",
        "(C)Ljava/lang/String;",
        n_string_value_of_c,
    ),
    (
        "java/lang/String",
        "valueOf",
        "(Z)Ljava/lang/String;",
        n_string_value_of_z,
    ),
    (
        "java/lang/String",
        "valueOf",
        "(F)Ljava/lang/String;",
        n_string_value_of_f,
    ),
    (
        "java/lang/String",
        "valueOf",
        "(D)Ljava/lang/String;",
        n_string_value_of_d,
    ),
    (
        "java/lang/String",
        "valueOf",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        n_string_value_of_l,
    ),
    // java/lang/StringBuilder
    ("java/lang/StringBuilder", "<init>", "()V", n_sb_init),
    (
        "java/lang/StringBuilder",
        "<init>",
        "(Ljava/lang/String;)V",
        n_sb_init_s,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(Z)Ljava/lang/StringBuilder;",
        n_sb_append_z,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(C)Ljava/lang/StringBuilder;",
        n_sb_append_c,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(I)Ljava/lang/StringBuilder;",
        n_sb_append_i,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(J)Ljava/lang/StringBuilder;",
        n_sb_append_j,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(F)Ljava/lang/StringBuilder;",
        n_sb_append_f,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(D)Ljava/lang/StringBuilder;",
        n_sb_append_d,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(Ljava/lang/String;)Ljava/lang/StringBuilder;",
        n_sb_append_s,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(Ljava/lang/Object;)Ljava/lang/StringBuilder;",
        n_sb_append_l,
    ),
    // append(char[]) overloads (corpus wave 1)
    (
        "java/lang/StringBuilder",
        "append",
        "([CII)Ljava/lang/StringBuilder;",
        sb_append_char_range,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "([C)Ljava/lang/StringBuilder;",
        sb_append_char_all,
    ),
    (
        "java/lang/StringBuilder",
        "toString",
        "()Ljava/lang/String;",
        n_sb_to_string,
    ),
    ("java/lang/StringBuilder", "length", "()I", n_sb_length),
    // java/lang/Throwable (audit A2: cause + suppressed machinery; the
    // natives resolve for every builtin exception family through the
    // parent-chain dispatch, which is what runs javac's JLS 14.20.3
    // try-with-resources lowering unmodified)
    ("java/lang/Throwable", "<init>", "()V", n_throwable_init),
    (
        "java/lang/Throwable",
        "<init>",
        "(Ljava/lang/String;)V",
        n_throwable_init_s,
    ),
    (
        "java/lang/Throwable",
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Throwable;)V",
        n_throwable_init_msg_cause,
    ),
    (
        "java/lang/Throwable",
        "<init>",
        "(Ljava/lang/Throwable;)V",
        n_throwable_init_cause,
    ),
    // EXTERNAL-B7 (jbox2d port): AssertionError's primitive and Object
    // constructors — String.valueOf rendering into the message slot.
    // () and (String) resolve through the Throwable rows above via the
    // parent-chain dispatch; (String, Throwable) rides msg_cause.
    (
        "java/lang/AssertionError",
        "<init>",
        "(Z)V",
        n_assertion_error_init_z,
    ),
    (
        "java/lang/AssertionError",
        "<init>",
        "(C)V",
        n_assertion_error_init_c,
    ),
    (
        "java/lang/AssertionError",
        "<init>",
        "(I)V",
        n_assertion_error_init,
    ),
    (
        "java/lang/AssertionError",
        "<init>",
        "(J)V",
        n_assertion_error_init,
    ),
    (
        "java/lang/AssertionError",
        "<init>",
        "(F)V",
        n_assertion_error_init,
    ),
    (
        "java/lang/AssertionError",
        "<init>",
        "(D)V",
        n_assertion_error_init,
    ),
    (
        "java/lang/AssertionError",
        "<init>",
        "(Ljava/lang/Object;)V",
        n_assertion_error_init_obj,
    ),
    (
        "java/lang/Throwable",
        "getMessage",
        "()Ljava/lang/String;",
        n_throwable_get_message,
    ),
    (
        "java/lang/Throwable",
        "getCause",
        "()Ljava/lang/Throwable;",
        n_throwable_get_cause,
    ),
    (
        "java/lang/Throwable",
        "addSuppressed",
        "(Ljava/lang/Throwable;)V",
        n_throwable_add_suppressed,
    ),
    (
        "java/lang/Throwable",
        "getSuppressed",
        "()[Ljava/lang/Throwable;",
        n_throwable_get_suppressed,
    ),
    (
        "java/lang/Throwable",
        "toString",
        "()Ljava/lang/String;",
        n_throwable_to_string,
    ),
    // java/io/PrintStream
    ("java/io/PrintStream", "println", "()V", n_ps_println_n),
    ("java/io/PrintStream", "println", "(Z)V", n_ps_println_z),
    ("java/io/PrintStream", "println", "(C)V", n_ps_println_c),
    ("java/io/PrintStream", "println", "(I)V", n_ps_println_i),
    ("java/io/PrintStream", "println", "(J)V", n_ps_println_j),
    ("java/io/PrintStream", "println", "(F)V", n_ps_println_f),
    ("java/io/PrintStream", "println", "(D)V", n_ps_println_d),
    (
        "java/io/PrintStream",
        "println",
        "(Ljava/lang/String;)V",
        n_ps_println_s,
    ),
    (
        "java/io/PrintStream",
        "println",
        "(Ljava/lang/Object;)V",
        n_ps_println_l,
    ),
    ("java/io/PrintStream", "print", "(Z)V", n_ps_print_z),
    ("java/io/PrintStream", "print", "(C)V", n_ps_print_c),
    ("java/io/PrintStream", "print", "(I)V", n_ps_print_i),
    ("java/io/PrintStream", "print", "(J)V", n_ps_print_j),
    ("java/io/PrintStream", "print", "(F)V", n_ps_print_f),
    ("java/io/PrintStream", "print", "(D)V", n_ps_print_d),
    (
        "java/io/PrintStream",
        "print",
        "(Ljava/lang/String;)V",
        n_ps_print_s,
    ),
    (
        "java/io/PrintStream",
        "print",
        "(Ljava/lang/Object;)V",
        n_ps_print_l,
    ),
    // boxed classes
    (
        "java/lang/Integer",
        "valueOf",
        "(I)Ljava/lang/Integer;",
        n_integer_value_of,
    ),
    ("java/lang/Integer", "intValue", "()I", n_integer_int_value),
    (
        "java/lang/Integer",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_integer_equals,
    ),
    ("java/lang/Integer", "hashCode", "()I", n_integer_hash_code),
    (
        "java/lang/Integer",
        "toString",
        "()Ljava/lang/String;",
        n_integer_to_string,
    ),
    (
        "java/lang/Integer",
        "toString",
        "(I)Ljava/lang/String;",
        n_integer_to_string_static,
    ),
    // radix form (corpus wave 1)
    (
        "java/lang/Integer",
        "toString",
        "(II)Ljava/lang/String;",
        n_integer_to_string_radix,
    ),
    (
        "java/lang/Integer",
        "parseInt",
        "(Ljava/lang/String;)I",
        n_integer_parse_int,
    ),
    (
        "java/lang/Long",
        "valueOf",
        "(J)Ljava/lang/Long;",
        n_long_value_of,
    ),
    ("java/lang/Long", "longValue", "()J", n_long_long_value),
    (
        "java/lang/Long",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_long_equals,
    ),
    ("java/lang/Long", "hashCode", "()I", n_long_hash_code),
    (
        "java/lang/Long",
        "toString",
        "()Ljava/lang/String;",
        n_long_to_string,
    ),
    (
        "java/lang/Long",
        "toString",
        "(J)Ljava/lang/String;",
        n_long_to_string_static,
    ),
    // radix form (corpus wave 1)
    (
        "java/lang/Long",
        "toString",
        "(JI)Ljava/lang/String;",
        n_long_to_string_radix,
    ),
    (
        "java/lang/Long",
        "parseLong",
        "(Ljava/lang/String;)J",
        n_long_parse_long,
    ),
    // radix form (corpus wave 1)
    (
        "java/lang/Long",
        "parseLong",
        "(Ljava/lang/String;I)J",
        n_long_parse_long_radix,
    ),
    // StringReader (corpus wave 1): remaining-string payload model
    (
        "java/io/StringReader",
        "<init>",
        "(Ljava/lang/String;)V",
        io::n_sr_init,
    ),
    ("java/io/StringReader", "read", "()I", io::n_sr_read),
    (
        "java/io/StringReader",
        "read",
        "([CII)I",
        io::n_sr_read_bulk,
    ),
    ("java/io/StringReader", "skip", "(J)J", io::n_sr_skip),
    ("java/io/StringReader", "ready", "()Z", io::n_sr_ready),
    (
        "java/io/StringReader",
        "markSupported",
        "()Z",
        io::n_sr_mark_supported,
    ),
    ("java/io/StringReader", "mark", "(I)V", io::n_sr_mark),
    ("java/io/StringReader", "reset", "()V", io::n_sr_reset),
    ("java/io/StringReader", "close", "()V", io::n_sr_close),
    // Reader abstract-declaration markers (compile-time surface walk):
    // every concrete Reader overrides read([CII)I and read()I
    ("java/io/Reader", "read", "()I", io::n_sr_read_marker),
    (
        "java/io/Reader",
        "read",
        "([CII)I",
        io::n_sr_read_bulk_marker,
    ),
    // serialization marker (corpus wave 1): loud-unsupported
    (
        "java/io/ObjectInputStream",
        "defaultReadObject",
        "()V",
        n_object_input_stream_default_read_object,
    ),
    // ------------------------------- corpus wave 3 (JAMA) -----------------
    // PrintWriter — REAL subset (see the implementation block above):
    // stdout/stderr and StringWriter sinks, print/println/write/flush/close.
    (
        "java/io/PrintWriter",
        "<init>",
        "(Ljava/io/OutputStream;Z)V",
        n_pw_init_os,
    ),
    (
        "java/io/PrintWriter",
        "<init>",
        "(Ljava/io/Writer;)V",
        n_pw_init_writer,
    ),
    (
        "java/io/PrintWriter",
        "<init>",
        "(Ljava/io/Writer;Z)V",
        n_pw_init_writer,
    ),
    ("java/io/PrintWriter", "print", "(C)V", n_pw_print_c),
    (
        "java/io/PrintWriter",
        "print",
        "(Ljava/lang/String;)V",
        n_pw_print_s,
    ),
    ("java/io/PrintWriter", "println", "()V", n_pw_println),
    (
        "java/io/PrintWriter",
        "println",
        "(Ljava/lang/String;)V",
        n_pw_println_s,
    ),
    (
        "java/io/PrintWriter",
        "write",
        "(Ljava/lang/String;)V",
        n_pw_write_s,
    ),
    ("java/io/PrintWriter", "flush", "()V", n_pw_flush),
    ("java/io/PrintWriter", "close", "()V", n_pw_close),
    // Registered loud (corpus wave 3): Vector / StreamTokenizer /
    // DecimalFormat family — resolve + instantiate so real programs
    // compile; behavior throws UnsupportedOperationException naming the
    // gap. See the loud-rows block above and docs/CORPUS_WAVE.md.
    ("java/util/Vector", "<init>", "()V", n_loud_vector),
    (
        "java/util/Vector",
        "addElement",
        "(Ljava/lang/Object;)V",
        n_loud_vector,
    ),
    (
        "java/util/Vector",
        "elementAt",
        "(I)Ljava/lang/Object;",
        n_loud_vector,
    ),
    (
        "java/util/Vector",
        "copyInto",
        "([Ljava/lang/Object;)V",
        n_loud_vector,
    ),
    ("java/util/Vector", "size", "()I", n_loud_vector),
    (
        "java/io/StreamTokenizer",
        "<init>",
        "(Ljava/io/Reader;)V",
        n_loud_stream_tokenizer,
    ),
    (
        "java/io/StreamTokenizer",
        "resetSyntax",
        "()V",
        n_loud_stream_tokenizer,
    ),
    (
        "java/io/StreamTokenizer",
        "wordChars",
        "(II)V",
        n_loud_stream_tokenizer,
    ),
    (
        "java/io/StreamTokenizer",
        "whitespaceChars",
        "(II)V",
        n_loud_stream_tokenizer,
    ),
    (
        "java/io/StreamTokenizer",
        "eolIsSignificant",
        "(Z)V",
        n_loud_stream_tokenizer,
    ),
    (
        "java/io/StreamTokenizer",
        "nextToken",
        "()I",
        n_loud_stream_tokenizer,
    ),
    (
        "java/text/DecimalFormat",
        "<init>",
        "()V",
        n_loud_decimal_format,
    ),
    (
        "java/text/DecimalFormat",
        "<init>",
        "(Ljava/lang/String;)V",
        n_loud_decimal_format,
    ),
    (
        "java/text/DecimalFormat",
        "setDecimalFormatSymbols",
        "(Ljava/text/DecimalFormatSymbols;)V",
        n_loud_decimal_format,
    ),
    (
        "java/text/DecimalFormat",
        "setMinimumIntegerDigits",
        "(I)V",
        n_loud_decimal_format,
    ),
    (
        "java/text/DecimalFormat",
        "setMaximumFractionDigits",
        "(I)V",
        n_loud_decimal_format,
    ),
    (
        "java/text/DecimalFormat",
        "setMinimumFractionDigits",
        "(I)V",
        n_loud_decimal_format,
    ),
    (
        "java/text/DecimalFormat",
        "setGroupingUsed",
        "(Z)V",
        n_loud_decimal_format,
    ),
    (
        "java/text/NumberFormat",
        "format",
        "(D)Ljava/lang/String;",
        n_loud_decimal_format,
    ),
    (
        "java/text/DecimalFormatSymbols",
        "<init>",
        "(Ljava/util/Locale;)V",
        n_loud_decimal_format,
    ),
    // Double.valueOf(String) (JAMA Matrix.read parses tokens through it)
    (
        "java/lang/Double",
        "valueOf",
        "(Ljava/lang/String;)Ljava/lang/Double;",
        n_double_value_of_s,
    ),
    // EXTERNAL-B6 (jbox2d port): the boxed constructors — JDK-deprecated
    // but alive, and jbox2d emits `new Integer(int)`. Fresh boxes (no
    // identity cache — see the B6 block above); (String) forms parse
    // with the JDK-exact helpers.
    ("java/lang/Integer", "<init>", "(I)V", n_integer_init_i),
    (
        "java/lang/Integer",
        "<init>",
        "(Ljava/lang/String;)V",
        n_integer_init_s,
    ),
    ("java/lang/Long", "<init>", "(J)V", n_long_init_j),
    (
        "java/lang/Long",
        "<init>",
        "(Ljava/lang/String;)V",
        n_long_init_s,
    ),
    ("java/lang/Double", "<init>", "(D)V", n_double_init_d),
    (
        "java/lang/Double",
        "<init>",
        "(Ljava/lang/String;)V",
        n_double_init_s,
    ),
    ("java/lang/Float", "<init>", "(F)V", n_float_init_f),
    (
        "java/lang/Float",
        "<init>",
        "(Ljava/lang/String;)V",
        n_float_init_s,
    ),
    ("java/lang/Boolean", "<init>", "(Z)V", n_boolean_init_z),
    (
        "java/lang/Boolean",
        "<init>",
        "(Ljava/lang/String;)V",
        n_boolean_init_s,
    ),
    ("java/lang/Character", "<init>", "(C)V", n_character_init_c),
    ("java/lang/Short", "<init>", "(S)V", n_short_init_s_prim),
    (
        "java/lang/Short",
        "<init>",
        "(Ljava/lang/String;)V",
        n_short_init_s,
    ),
    ("java/lang/Byte", "<init>", "(B)V", n_byte_init_b_prim),
    (
        "java/lang/Byte",
        "<init>",
        "(Ljava/lang/String;)V",
        n_byte_init_s,
    ),
    // parseFloat/parseDouble (corpus wave 1)
    (
        "java/lang/Float",
        "parseFloat",
        "(Ljava/lang/String;)F",
        n_float_parse_float,
    ),
    (
        "java/lang/Double",
        "parseDouble",
        "(Ljava/lang/String;)D",
        n_double_parse_double,
    ),
    // exact IEEE bit exposure (corpus gap F4 support)
    (
        "java/lang/Float",
        "floatToIntBits",
        "(F)I",
        n_float_to_int_bits,
    ),
    (
        "java/lang/Float",
        "floatToRawIntBits",
        "(F)I",
        n_float_to_raw_int_bits,
    ),
    (
        "java/lang/Double",
        "doubleToLongBits",
        "(D)J",
        n_double_to_long_bits,
    ),
    (
        "java/lang/Double",
        "doubleToRawLongBits",
        "(D)J",
        n_double_to_raw_long_bits,
    ),
    // EXTERNAL-B8: the reverse bit-reinterpretation statics (jbox2d's
    // Vec2/Vec3 MathUtils needs them for its bit tricks)
    (
        "java/lang/Float",
        "intBitsToFloat",
        "(I)F",
        n_int_bits_to_float,
    ),
    (
        "java/lang/Double",
        "longBitsToDouble",
        "(J)D",
        n_long_bits_to_double,
    ),
    (
        "java/lang/Float",
        "valueOf",
        "(F)Ljava/lang/Float;",
        n_float_value_of,
    ),
    ("java/lang/Float", "floatValue", "()F", n_float_float_value),
    (
        "java/lang/Float",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_float_equals,
    ),
    ("java/lang/Float", "hashCode", "()I", n_float_hash_code),
    (
        "java/lang/Float",
        "toString",
        "()Ljava/lang/String;",
        n_float_to_string,
    ),
    (
        "java/lang/Float",
        "toString",
        "(F)Ljava/lang/String;",
        n_float_to_string_static,
    ),
    // float predicates (corpus wave 1: minimal-json number formatting)
    ("java/lang/Float", "isInfinite", "(F)Z", n_float_is_infinite),
    ("java/lang/Float", "isNaN", "(F)Z", n_float_is_nan),
    (
        "java/lang/Double",
        "valueOf",
        "(D)Ljava/lang/Double;",
        n_double_value_of,
    ),
    (
        "java/lang/Double",
        "doubleValue",
        "()D",
        n_double_double_value,
    ),
    (
        "java/lang/Double",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_double_equals,
    ),
    ("java/lang/Double", "hashCode", "()I", n_double_hash_code),
    // double predicates (corpus wave 1)
    (
        "java/lang/Double",
        "isInfinite",
        "(D)Z",
        n_double_is_infinite,
    ),
    ("java/lang/Double", "isNaN", "(D)Z", n_double_is_nan),
    (
        "java/lang/Double",
        "toString",
        "()Ljava/lang/String;",
        n_double_to_string,
    ),
    (
        "java/lang/Double",
        "toString",
        "(D)Ljava/lang/String;",
        n_double_to_string_static,
    ),
    (
        "java/lang/Byte",
        "valueOf",
        "(B)Ljava/lang/Byte;",
        n_byte_value_of,
    ),
    ("java/lang/Byte", "byteValue", "()B", n_byte_byte_value),
    (
        "java/lang/Byte",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_byte_equals,
    ),
    ("java/lang/Byte", "hashCode", "()I", n_byte_hash_code),
    (
        "java/lang/Byte",
        "toString",
        "()Ljava/lang/String;",
        n_byte_to_string,
    ),
    (
        "java/lang/Short",
        "valueOf",
        "(S)Ljava/lang/Short;",
        n_short_value_of,
    ),
    ("java/lang/Short", "shortValue", "()S", n_short_short_value),
    (
        "java/lang/Short",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_short_equals,
    ),
    ("java/lang/Short", "hashCode", "()I", n_short_hash_code),
    (
        "java/lang/Short",
        "toString",
        "()Ljava/lang/String;",
        n_short_to_string,
    ),
    (
        "java/lang/Boolean",
        "valueOf",
        "(Z)Ljava/lang/Boolean;",
        n_boolean_value_of,
    ),
    (
        "java/lang/Boolean",
        "booleanValue",
        "()Z",
        n_boolean_boolean_value,
    ),
    (
        "java/lang/Boolean",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_boolean_equals,
    ),
    ("java/lang/Boolean", "hashCode", "()I", n_boolean_hash_code),
    (
        "java/lang/Boolean",
        "toString",
        "()Ljava/lang/String;",
        n_boolean_to_string,
    ),
    (
        "java/lang/Boolean",
        "toString",
        "(Z)Ljava/lang/String;",
        n_boolean_to_string_static,
    ),
    (
        "java/lang/Character",
        "valueOf",
        "(C)Ljava/lang/Character;",
        n_character_value_of,
    ),
    (
        "java/lang/Character",
        "charValue",
        "()C",
        n_character_char_value,
    ),
    (
        "java/lang/Character",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_character_equals,
    ),
    (
        "java/lang/Character",
        "hashCode",
        "()I",
        n_character_hash_code,
    ),
    (
        "java/lang/Character",
        "toString",
        "()Ljava/lang/String;",
        n_character_to_string,
    ),
    (
        "java/lang/Character",
        "toString",
        "(C)Ljava/lang/String;",
        n_character_to_string_static,
    ),
    // ------------------------------------------ corpus wave 1: java.io (F3)
    // Implementations live in `io.rs`. StringWriter real behavior; the
    // Writer rows are abstract-declaration markers for the compile-time
    // surface walk and must never dispatch.
    ("java/io/StringWriter", "<init>", "()V", io::n_sw_init),
    ("java/io/StringWriter", "write", "(I)V", io::n_sw_write_int),
    (
        "java/io/StringWriter",
        "write",
        "([CII)V",
        io::n_sw_write_chars,
    ),
    (
        "java/io/StringWriter",
        "write",
        "([C)V",
        io::n_sw_write_chars_all,
    ),
    (
        "java/io/StringWriter",
        "write",
        "(Ljava/lang/String;)V",
        io::n_sw_write_str,
    ),
    (
        "java/io/StringWriter",
        "write",
        "(Ljava/lang/String;II)V",
        io::n_sw_write_str_range,
    ),
    (
        "java/io/StringWriter",
        "toString",
        "()Ljava/lang/String;",
        io::n_sw_to_string,
    ),
    ("java/io/StringWriter", "close", "()V", io::n_sw_close),
    ("java/io/StringWriter", "flush", "()V", io::n_sw_flush),
    // ------------------------------------------------ byte streams (JZlib)
    // Corpus wave 2: the Filter*/base stream surface JZlib's stream
    // classes extend (see io.rs "byte streams" section). The Filter* rows
    // delegate through the JDK field; the base rows carry the JDK default
    // behaviors and loud markers for the abstract writes.
    (
        "java/io/FilterOutputStream",
        "<init>",
        "(Ljava/io/OutputStream;)V",
        io::n_fos_init,
    ),
    (
        "java/io/FilterOutputStream",
        "write",
        "(I)V",
        io::n_fos_write_int,
    ),
    (
        "java/io/FilterOutputStream",
        "write",
        "([B)V",
        io::n_fos_write_bulk,
    ),
    (
        "java/io/FilterOutputStream",
        "write",
        "([BII)V",
        io::n_fos_write_range,
    ),
    (
        "java/io/FilterOutputStream",
        "flush",
        "()V",
        io::n_fos_flush,
    ),
    (
        "java/io/FilterOutputStream",
        "close",
        "()V",
        io::n_fos_close,
    ),
    // ByteArrayOutputStream (corpus wave 2, JZlib Inflate): JDK field
    // model in slots (buf byte[] ref, count Int); behavior in io.rs with
    // the JDK growth and bounds contracts.
    (
        "java/io/ByteArrayOutputStream",
        "<init>",
        "()V",
        io::n_baos_init,
    ),
    (
        "java/io/ByteArrayOutputStream",
        "<init>",
        "(I)V",
        io::n_baos_init_size,
    ),
    (
        "java/io/ByteArrayOutputStream",
        "write",
        "(I)V",
        io::n_baos_write_int,
    ),
    (
        "java/io/ByteArrayOutputStream",
        "write",
        "([B)V",
        io::n_baos_write_bulk,
    ),
    (
        "java/io/ByteArrayOutputStream",
        "write",
        "([BII)V",
        io::n_baos_write_range,
    ),
    (
        "java/io/ByteArrayOutputStream",
        "size",
        "()I",
        io::n_baos_size,
    ),
    (
        "java/io/ByteArrayOutputStream",
        "reset",
        "()V",
        io::n_baos_reset,
    ),
    (
        "java/io/ByteArrayOutputStream",
        "toByteArray",
        "()[B",
        io::n_baos_to_byte_array,
    ),
    (
        "java/io/ByteArrayOutputStream",
        "toString",
        "()Ljava/lang/String;",
        io::n_baos_to_string,
    ),
    (
        "java/io/ByteArrayOutputStream",
        "writeTo",
        "(Ljava/io/OutputStream;)V",
        io::n_baos_write_to,
    ),
    ("java/io/OutputStream", "write", "(I)V", io::n_os_write_int),
    (
        "java/io/OutputStream",
        "write",
        "([B)V",
        io::n_os_write_bulk,
    ),
    (
        "java/io/OutputStream",
        "write",
        "([BII)V",
        io::n_os_write_range,
    ),
    ("java/io/OutputStream", "flush", "()V", io::n_os_flush),
    ("java/io/OutputStream", "close", "()V", io::n_os_close),
    (
        "java/io/FilterInputStream",
        "<init>",
        "(Ljava/io/InputStream;)V",
        io::n_fis_init,
    ),
    ("java/io/FilterInputStream", "read", "()I", io::n_fis_read),
    (
        "java/io/FilterInputStream",
        "read",
        "([B)I",
        io::n_fis_read_bulk,
    ),
    (
        "java/io/FilterInputStream",
        "read",
        "([BII)I",
        io::n_fis_read_range,
    ),
    ("java/io/FilterInputStream", "skip", "(J)J", io::n_fis_skip),
    (
        "java/io/FilterInputStream",
        "available",
        "()I",
        io::n_fis_available,
    ),
    ("java/io/FilterInputStream", "close", "()V", io::n_fis_close),
    // PushbackInputStream (stream wave 4; Temurin-pinned by
    // corpus/PushbackProbe.java — unread exhaustion
    // "Push back buffer is full", skip consumes the pushback first,
    // markSupported false, the "Stream closed" family).
    // Slots: 0 in (inherited) / 1 buf / 2 pos.
    (
        "java/io/PushbackInputStream",
        "<init>",
        "(Ljava/io/InputStream;)V",
        io::n_pb_init,
    ),
    (
        "java/io/PushbackInputStream",
        "<init>",
        "(Ljava/io/InputStream;I)V",
        io::n_pb_init_size,
    ),
    ("java/io/PushbackInputStream", "read", "()I", io::n_pb_read),
    (
        "java/io/PushbackInputStream",
        "read",
        "([BII)I",
        io::n_pb_read_range,
    ),
    (
        "java/io/PushbackInputStream",
        "unread",
        "(I)V",
        io::n_pb_unread,
    ),
    (
        "java/io/PushbackInputStream",
        "unread",
        "([B)V",
        io::n_pb_unread_bulk,
    ),
    (
        "java/io/PushbackInputStream",
        "unread",
        "([BII)V",
        io::n_pb_unread_range,
    ),
    ("java/io/PushbackInputStream", "skip", "(J)J", io::n_pb_skip),
    (
        "java/io/PushbackInputStream",
        "available",
        "()I",
        io::n_pb_available,
    ),
    (
        "java/io/PushbackInputStream",
        "markSupported",
        "()Z",
        io::n_pb_mark_supported,
    ),
    (
        "java/io/PushbackInputStream",
        "close",
        "()V",
        io::n_pb_close,
    ),
    // BufferedInputStream (stream wave 3; Temurin-pinned by
    // corpus/BufferedProbe.java — including the skip surprise: a partially
    // consumed buffer consumes ONLY the buffer (skip(20) over 7 = 7), an
    // EMPTY buffer drains the inner stream clamped to available() (26)).
    // Slots: 0 in (inherited) / 1 buf / 2 pos / 3 count / 4 markpos /
    // 5 marklimit; close() nulls in+buf — the null slots ARE the closed
    // state ("Stream closed").
    (
        "java/io/BufferedInputStream",
        "<init>",
        "(Ljava/io/InputStream;)V",
        io::n_buf_init,
    ),
    (
        "java/io/BufferedInputStream",
        "<init>",
        "(Ljava/io/InputStream;I)V",
        io::n_buf_init_size,
    ),
    ("java/io/BufferedInputStream", "read", "()I", io::n_buf_read),
    (
        "java/io/BufferedInputStream",
        "read",
        "([BII)I",
        io::n_buf_read_range,
    ),
    (
        "java/io/BufferedInputStream",
        "skip",
        "(J)J",
        io::n_buf_skip,
    ),
    (
        "java/io/BufferedInputStream",
        "available",
        "()I",
        io::n_buf_available,
    ),
    (
        "java/io/BufferedInputStream",
        "mark",
        "(I)V",
        io::n_buf_mark,
    ),
    (
        "java/io/BufferedInputStream",
        "reset",
        "()V",
        io::n_buf_reset,
    ),
    (
        "java/io/BufferedInputStream",
        "markSupported",
        "()Z",
        io::n_buf_mark_supported,
    ),
    (
        "java/io/BufferedInputStream",
        "close",
        "()V",
        io::n_buf_close,
    ),
    // ByteArrayInputStream (skip/charset wave — the JZlib wave-2 phase-2
    // candidate; semantics pinned by corpus/SkipProbe.java: close() is a
    // no-op, negative skip returns 0, skip past EOF returns the actual
    // count, markSupported true). Slots: 0 buf / 1 pos / 2 mark / 3 count.
    (
        "java/io/ByteArrayInputStream",
        "<init>",
        "([B)V",
        io::n_bis_init,
    ),
    (
        "java/io/ByteArrayInputStream",
        "<init>",
        "([BII)V",
        io::n_bis_init_range,
    ),
    (
        "java/io/ByteArrayInputStream",
        "read",
        "()I",
        io::n_bis_read,
    ),
    (
        "java/io/ByteArrayInputStream",
        "read",
        "([BII)I",
        io::n_bis_read_range,
    ),
    (
        "java/io/ByteArrayInputStream",
        "skip",
        "(J)J",
        io::n_bis_skip,
    ),
    (
        "java/io/ByteArrayInputStream",
        "available",
        "()I",
        io::n_bis_available,
    ),
    (
        "java/io/ByteArrayInputStream",
        "mark",
        "(I)V",
        io::n_bis_mark,
    ),
    (
        "java/io/ByteArrayInputStream",
        "reset",
        "()V",
        io::n_bis_reset,
    ),
    (
        "java/io/ByteArrayInputStream",
        "markSupported",
        "()Z",
        io::n_bis_mark_supported,
    ),
    ("java/io/InputStream", "read", "()I", io::n_is_read),
    ("java/io/InputStream", "read", "([B)I", io::n_is_read_bulk),
    (
        "java/io/InputStream",
        "read",
        "([BII)I",
        io::n_is_read_range,
    ),
    ("java/io/InputStream", "skip", "(J)J", io::n_is_skip_default),
    (
        "java/io/InputStream",
        "available",
        "()I",
        io::n_is_available,
    ),
    ("java/io/InputStream", "close", "()V", io::n_is_close),
    ("java/io/InputStream", "mark", "(I)V", io::n_is_mark),
    ("java/io/InputStream", "reset", "()V", io::n_is_reset),
    (
        "java/io/InputStream",
        "markSupported",
        "()Z",
        io::n_is_mark_supported,
    ),
    (
        "java/io/Writer",
        "write",
        "(I)V",
        io::n_writer_write_int_marker,
    ),
    (
        "java/io/Writer",
        "write",
        "([CII)V",
        io::n_writer_write_chars_marker,
    ),
    (
        "java/io/Writer",
        "write",
        "([C)V",
        io::n_writer_write_chars_all,
    ),
    (
        "java/io/Writer",
        "write",
        "(Ljava/lang/String;)V",
        io::n_writer_write_str,
    ),
    (
        "java/io/Writer",
        "write",
        "(Ljava/lang/String;II)V",
        io::n_writer_write_str_range_marker,
    ),
    // ------------------------------------------------ M3 Wave 2: collections
    // (implementations live in `collections.rs`; heap-resident state, see
    // the slot-layout table in its module header)
    // java/util/ArrayList
    // EXTERNAL-B10 (jbox2d port): java.util.Random — the JDK's 48-bit
    // LCG as a real builtin object (behavior fns above, byte-exact).
    ("java/util/Random", "<init>", "()V", n_random_init),
    ("java/util/Random", "<init>", "(J)V", n_random_init_j),
    ("java/util/Random", "setSeed", "(J)V", n_random_set_seed),
    ("java/util/Random", "next", "(I)I", n_random_next),
    ("java/util/Random", "nextInt", "()I", n_random_next_int),
    (
        "java/util/Random",
        "nextInt",
        "(I)I",
        n_random_next_int_bound,
    ),
    ("java/util/Random", "nextLong", "()J", n_random_next_long),
    (
        "java/util/Random",
        "nextBoolean",
        "()Z",
        n_random_next_boolean,
    ),
    ("java/util/Random", "nextFloat", "()F", n_random_next_float),
    (
        "java/util/Random",
        "nextDouble",
        "()D",
        n_random_next_double,
    ),
    (
        "java/util/Random",
        "nextGaussian",
        "()D",
        n_random_next_gaussian,
    ),
    (
        "java/util/Random",
        "nextBytes",
        "([B)V",
        n_random_next_bytes,
    ),
    ("java/util/ArrayList", "<init>", "()V", c::n_al_init),
    ("java/util/ArrayList", "<init>", "(I)V", c::n_al_init),
    // <init>(Collection) — the copy constructor (corpus wave 1)
    (
        "java/util/ArrayList",
        "<init>",
        "(Ljava/util/Collection;)V",
        c::n_al_init_from_collection,
    ),
    (
        "java/util/ArrayList",
        "add",
        "(Ljava/lang/Object;)Z",
        c::n_al_add,
    ),
    (
        "java/util/ArrayList",
        "add",
        "(ILjava/lang/Object;)V",
        c::n_al_add_at,
    ),
    (
        "java/util/ArrayList",
        "get",
        "(I)Ljava/lang/Object;",
        c::n_al_get,
    ),
    // EXTERNAL-B11 (jbox2d port): the two toArray forms with the JDK's
    // two-branch contract (behavior in jdkutil.rs).
    (
        "java/util/ArrayList",
        "toArray",
        "()[Ljava/lang/Object;",
        ju::n_al_to_array,
    ),
    (
        "java/util/ArrayList",
        "toArray",
        "([Ljava/lang/Object;)[Ljava/lang/Object;",
        ju::n_al_to_array_typed,
    ),
    (
        "java/util/ArrayList",
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        c::n_al_set,
    ),
    (
        "java/util/ArrayList",
        "remove",
        "(I)Ljava/lang/Object;",
        c::n_al_remove_at,
    ),
    (
        "java/util/ArrayList",
        "remove",
        "(Ljava/lang/Object;)Z",
        c::n_al_remove_obj,
    ),
    ("java/util/ArrayList", "size", "()I", c::n_al_size),
    ("java/util/ArrayList", "isEmpty", "()Z", c::n_al_is_empty),
    // AbstractList hashCode/equals/lastIndexOf (corpus wave 1: JsonObject
    // hash/equals/indexOfByName go through the List-declared static type;
    // the List rows serve the compile-time walk, runtime dispatch lands on
    // the receiver's own row).
    ("java/util/ArrayList", "hashCode", "()I", c::n_al_hash_code),
    ("java/util/List", "hashCode", "()I", c::n_al_hash_code),
    (
        "java/util/ArrayList",
        "equals",
        "(Ljava/lang/Object;)Z",
        c::n_al_equals,
    ),
    (
        "java/util/List",
        "equals",
        "(Ljava/lang/Object;)Z",
        c::n_al_equals,
    ),
    (
        "java/util/ArrayList",
        "lastIndexOf",
        "(Ljava/lang/Object;)I",
        c::n_al_last_index_of,
    ),
    (
        "java/util/List",
        "lastIndexOf",
        "(Ljava/lang/Object;)I",
        c::n_al_last_index_of,
    ),
    (
        "java/util/ArrayList",
        "contains",
        "(Ljava/lang/Object;)Z",
        c::n_al_contains,
    ),
    (
        "java/util/ArrayList",
        "indexOf",
        "(Ljava/lang/Object;)I",
        c::n_al_index_of,
    ),
    ("java/util/ArrayList", "clear", "()V", c::n_al_clear),
    (
        "java/util/ArrayList",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_al_iterator,
    ),
    (
        "java/util/ArrayList",
        "toString",
        "()Ljava/lang/String;",
        c::n_al_to_string,
    ),
    (
        "java/util/ArrayList",
        "forEach",
        "(Ljava/util/function/Consumer;)V",
        c::n_al_for_each,
    ),
    (
        "java/util/ArrayList",
        "removeIf",
        "(Ljava/util/function/Predicate;)Z",
        c::n_al_remove_if,
    ),
    // Mirror rows under the `java/util/List` interface so calls on
    // interface-typed receivers (the same pattern as the Wave-2 List rows
    // above) reach the ArrayList implementations.
    (
        "java/util/List",
        "forEach",
        "(Ljava/util/function/Consumer;)V",
        c::n_al_for_each,
    ),
    (
        "java/util/List",
        "removeIf",
        "(Ljava/util/function/Predicate;)Z",
        c::n_al_remove_if,
    ),
    // java/util/Iterator (the ArrayList$Itr runtime class dispatches here)
    (
        "java/util/ArrayList$Itr",
        "hasNext",
        "()Z",
        c::n_itr_has_next,
    ),
    (
        "java/util/ArrayList$Itr",
        "next",
        "()Ljava/lang/Object;",
        c::n_itr_next,
    ),
    ("java/util/ArrayList$Itr", "remove", "()V", c::n_itr_remove),
    // interface declarations so `Iterator i = ...` / `List l = ...`
    // receiver types resolve (validate_virtual walks to the builtin
    // frontier and needs the method present there)
    ("java/util/Iterator", "hasNext", "()Z", c::n_itr_has_next),
    (
        "java/util/Iterator",
        "next",
        "()Ljava/lang/Object;",
        c::n_itr_next,
    ),
    ("java/util/Iterator", "remove", "()V", c::n_itr_remove),
    // java/util/HashMap
    ("java/util/HashMap", "<init>", "()V", c::n_hm_init),
    ("java/util/HashMap", "<init>", "(I)V", c::n_hm_init),
    (
        "java/util/HashMap",
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_put,
    ),
    (
        "java/util/HashMap",
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_get,
    ),
    (
        "java/util/HashMap",
        "getOrDefault",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_get_or_default,
    ),
    (
        "java/util/HashMap",
        "containsKey",
        "(Ljava/lang/Object;)Z",
        c::n_hm_contains_key,
    ),
    (
        "java/util/HashMap",
        "containsValue",
        "(Ljava/lang/Object;)Z",
        c::n_hm_contains_value,
    ),
    (
        "java/util/HashMap",
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_remove,
    ),
    ("java/util/HashMap", "size", "()I", c::n_hm_size),
    ("java/util/HashMap", "isEmpty", "()Z", c::n_hm_is_empty),
    ("java/util/HashMap", "clear", "()V", c::n_hm_clear),
    (
        "java/util/HashMap",
        "keySet",
        "()Ljava/util/Set;",
        c::n_hm_key_set,
    ),
    // stringsim corpus wave (2026-11-07): entrySet views — the JDK's
    // entry iterator hands out the INTERNAL Node as Map.Entry, so
    // getKey/getValue dispatch on HashMap$Node.
    (
        "java/util/HashMap",
        "entrySet",
        "()Ljava/util/Set;",
        c::n_hm_entry_set,
    ),
    (
        "java/util/HashMap$EntrySet",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_hes_iterator,
    ),
    ("java/util/HashMap$EntrySet", "size", "()I", c::n_hes_size),
    (
        "java/util/HashMap$EntrySet",
        "isEmpty",
        "()Z",
        c::n_hes_is_empty,
    ),
    (
        "java/util/HashMap$EntryIterator",
        "hasNext",
        "()Z",
        c::n_ei_has_next,
    ),
    (
        "java/util/HashMap$EntryIterator",
        "next",
        "()Ljava/lang/Object;",
        c::n_ei_next,
    ),
    // The stringsim driver dispatches getKey/getValue through the
    // java/util/Map$Entry INTERFACE type — interface-level rows, same law
    // as the java/util/Map family above.
    (
        "java/util/Map$Entry",
        "getKey",
        "()Ljava/lang/Object;",
        c::n_node_get_key,
    ),
    (
        "java/util/Map$Entry",
        "getValue",
        "()Ljava/lang/Object;",
        c::n_node_get_value,
    ),
    (
        "java/util/HashMap$Node",
        "getKey",
        "()Ljava/lang/Object;",
        c::n_node_get_key,
    ),
    (
        "java/util/HashMap$Node",
        "getValue",
        "()Ljava/lang/Object;",
        c::n_node_get_value,
    ),
    // stringsim corpus wave: Collections.unmodifiableMap wrapper +
    // Sift4's index-oriented LinkedList.
    (
        "java/util/Collections",
        "unmodifiableMap",
        "(Ljava/util/Map;)Ljava/util/Map;",
        c::n_collections_unmodifiable_map,
    ),
    (
        "java/util/Collections$UnmodifiableMap",
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_um_get,
    ),
    (
        "java/util/Collections$UnmodifiableMap",
        "size",
        "()I",
        c::n_um_size,
    ),
    (
        "java/util/Collections$UnmodifiableMap",
        "isEmpty",
        "()Z",
        c::n_um_is_empty,
    ),
    (
        "java/util/Collections$UnmodifiableMap",
        "entrySet",
        "()Ljava/util/Set;",
        c::n_um_entry_set,
    ),
    (
        "java/util/Collections$UnmodifiableMap",
        "containsKey",
        "(Ljava/lang/Object;)Z",
        c::n_um_contains_key,
    ),
    (
        "java/util/Collections$UnmodifiableMap",
        "keySet",
        "()Ljava/util/Set;",
        c::n_um_key_set,
    ),
    (
        "java/util/Collections$UnmodifiableMap",
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_um_put,
    ),
    ("java/util/LinkedList", "<init>", "()V", c::n_ll_init),
    ("java/util/LinkedList", "size", "()I", c::n_ll_size),
    (
        "java/util/LinkedList",
        "add",
        "(Ljava/lang/Object;)Z",
        c::n_ll_add,
    ),
    (
        "java/util/LinkedList",
        "get",
        "(I)Ljava/lang/Object;",
        c::n_ll_get,
    ),
    (
        "java/util/LinkedList",
        "remove",
        "(I)Ljava/lang/Object;",
        c::n_ll_remove,
    ),
    // stringsim corpus wave: the narrow java.util.regex surface. The
    // ASCII-\s law is the registered gap's first pin (docs/CORPUS_WAVE.md):
    // Java's default \s is [ \t\n\x0B\f\r], NOT Unicode-aware.
    (
        "java/util/regex/Pattern",
        "compile",
        "(Ljava/lang/String;)Ljava/util/regex/Pattern;",
        regex::n_pattern_compile,
    ),
    (
        "java/util/regex/Pattern",
        "matcher",
        "(Ljava/lang/CharSequence;)Ljava/util/regex/Matcher;",
        regex::n_pattern_matcher,
    ),
    (
        "java/util/regex/Matcher",
        "replaceAll",
        "(Ljava/lang/String;)Ljava/lang/String;",
        regex::n_matcher_replace_all,
    ),
    (
        "java/util/HashMap",
        "values",
        "()Ljava/util/Collection;",
        c::n_hm_values,
    ),
    (
        "java/util/HashMap",
        "toString",
        "()Ljava/lang/String;",
        c::n_hm_to_string,
    ),
    // java/util/Map interface declarations
    (
        "java/util/Map",
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_put,
    ),
    (
        "java/util/Map",
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_get,
    ),
    (
        "java/util/Map",
        "getOrDefault",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_get_or_default,
    ),
    (
        "java/util/Map",
        "containsKey",
        "(Ljava/lang/Object;)Z",
        c::n_hm_contains_key,
    ),
    (
        "java/util/Map",
        "containsValue",
        "(Ljava/lang/Object;)Z",
        c::n_hm_contains_value,
    ),
    (
        "java/util/Map",
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_hm_remove,
    ),
    ("java/util/Map", "size", "()I", c::n_hm_size),
    ("java/util/Map", "isEmpty", "()Z", c::n_hm_is_empty),
    ("java/util/Map", "clear", "()V", c::n_hm_clear),
    (
        "java/util/Map",
        "entrySet",
        "()Ljava/util/Set;",
        c::n_hm_entry_set,
    ),
    (
        "java/util/Map",
        "keySet",
        "()Ljava/util/Set;",
        c::n_hm_key_set,
    ),
    (
        "java/util/Map",
        "values",
        "()Ljava/util/Collection;",
        c::n_hm_values,
    ),
    // views: KeySet (Set semantics over the map's keys)
    (
        "java/util/HashMap$KeySet",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_ks_iterator,
    ),
    ("java/util/HashMap$KeySet", "size", "()I", c::n_ks_size),
    (
        "java/util/HashMap$KeySet",
        "isEmpty",
        "()Z",
        c::n_ks_is_empty,
    ),
    (
        "java/util/HashMap$KeySet",
        "contains",
        "(Ljava/lang/Object;)Z",
        c::n_ks_contains,
    ),
    (
        "java/util/HashMap$KeySet",
        "toString",
        "()Ljava/lang/String;",
        c::n_ks_to_string,
    ),
    // views: Values (Collection semantics over the map's values)
    (
        "java/util/HashMap$Values",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_vs_iterator,
    ),
    ("java/util/HashMap$Values", "size", "()I", c::n_vs_size),
    (
        "java/util/HashMap$Values",
        "isEmpty",
        "()Z",
        c::n_vs_is_empty,
    ),
    (
        "java/util/HashMap$Values",
        "contains",
        "(Ljava/lang/Object;)Z",
        c::n_vs_contains,
    ),
    (
        "java/util/HashMap$Values",
        "toString",
        "()Ljava/lang/String;",
        c::n_vs_to_string,
    ),
    // hash iterators (KeyIterator over keys, ValueIterator over values)
    (
        "java/util/HashMap$KeyIterator",
        "hasNext",
        "()Z",
        c::n_ki_has_next,
    ),
    (
        "java/util/HashMap$KeyIterator",
        "next",
        "()Ljava/lang/Object;",
        c::n_ki_next,
    ),
    (
        "java/util/HashMap$KeyIterator",
        "remove",
        "()V",
        c::n_itr_remove,
    ),
    (
        "java/util/HashMap$ValueIterator",
        "hasNext",
        "()Z",
        c::n_vi_has_next,
    ),
    (
        "java/util/HashMap$ValueIterator",
        "next",
        "()Ljava/lang/Object;",
        c::n_vi_next,
    ),
    (
        "java/util/HashMap$ValueIterator",
        "remove",
        "()V",
        c::n_itr_remove,
    ),
    // java/util/HashSet (backed by a private HashMap, Java-faithful)
    ("java/util/HashSet", "<init>", "()V", c::n_hs_init),
    ("java/util/HashSet", "<init>", "(I)V", c::n_hs_init),
    (
        "java/util/HashSet",
        "add",
        "(Ljava/lang/Object;)Z",
        c::n_hs_add,
    ),
    (
        "java/util/HashSet",
        "addAll",
        "(Ljava/util/Collection;)Z",
        c::n_hs_add_all,
    ),
    (
        "java/util/HashSet",
        "remove",
        "(Ljava/lang/Object;)Z",
        c::n_hs_remove,
    ),
    (
        "java/util/HashSet",
        "contains",
        "(Ljava/lang/Object;)Z",
        c::n_hs_contains,
    ),
    ("java/util/HashSet", "size", "()I", c::n_hs_size),
    ("java/util/HashSet", "isEmpty", "()Z", c::n_hs_is_empty),
    (
        "java/util/HashSet",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_hs_iterator,
    ),
    (
        "java/util/HashSet",
        "toString",
        "()Ljava/lang/String;",
        c::n_hs_to_string,
    ),
    // java/util/Set and java/util/Collection interface declarations
    ("java/util/Set", "add", "(Ljava/lang/Object;)Z", c::n_hs_add),
    (
        "java/util/Set",
        "remove",
        "(Ljava/lang/Object;)Z",
        c::n_hs_remove,
    ),
    (
        "java/util/Set",
        "contains",
        "(Ljava/lang/Object;)Z",
        c::n_hs_contains,
    ),
    (
        "java/util/Set",
        "addAll",
        "(Ljava/util/Collection;)Z",
        c::n_hs_add_all,
    ),
    ("java/util/Set", "size", "()I", c::n_hs_size),
    ("java/util/Set", "isEmpty", "()Z", c::n_hs_is_empty),
    (
        "java/util/Set",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_hs_iterator,
    ),
    (
        "java/util/List",
        "add",
        "(Ljava/lang/Object;)Z",
        c::n_al_add,
    ),
    (
        "java/util/List",
        "add",
        "(ILjava/lang/Object;)V",
        c::n_al_add_at,
    ),
    (
        "java/util/List",
        "get",
        "(I)Ljava/lang/Object;",
        c::n_al_get,
    ),
    (
        "java/util/List",
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        c::n_al_set,
    ),
    (
        "java/util/List",
        "remove",
        "(I)Ljava/lang/Object;",
        c::n_al_remove_at,
    ),
    (
        "java/util/List",
        "remove",
        "(Ljava/lang/Object;)Z",
        c::n_al_remove_obj,
    ),
    ("java/util/List", "size", "()I", c::n_al_size),
    ("java/util/List", "isEmpty", "()Z", c::n_al_is_empty),
    (
        "java/util/List",
        "contains",
        "(Ljava/lang/Object;)Z",
        c::n_al_contains,
    ),
    (
        "java/util/List",
        "indexOf",
        "(Ljava/lang/Object;)I",
        c::n_al_index_of,
    ),
    ("java/util/List", "clear", "()V", c::n_al_clear),
    (
        "java/util/List",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_al_iterator,
    ),
    (
        "java/util/Collection",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_al_iterator,
    ),
    (
        "java/lang/Iterable",
        "iterator",
        "()Ljava/util/Iterator;",
        c::n_al_iterator,
    ),
    // java/util/Objects
    (
        "java/util/Objects",
        "equals",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        c::n_objects_equals,
    ),
    (
        "java/util/Objects",
        "hashCode",
        "(Ljava/lang/Object;)I",
        c::n_objects_hash_code,
    ),
    (
        "java/util/Objects",
        "requireNonNull",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        c::n_objects_require_non_null,
    ),
    (
        "java/util/Objects",
        "requireNonNull",
        "(Ljava/lang/Object;Ljava/lang/String;)Ljava/lang/Object;",
        c::n_objects_require_non_null_msg,
    ),
    (
        "java/util/Objects",
        "requireNonNull",
        "(Ljava/lang/Object;Ljava/util/function/Supplier;)Ljava/lang/Object;",
        c::n_objects_require_non_null_supplier,
    ),
    (
        "java/util/Objects",
        "toString",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        c::n_objects_to_string,
    ),
    (
        "java/util/Objects",
        "isNull",
        "(Ljava/lang/Object;)Z",
        c::n_objects_is_null,
    ),
    (
        "java/util/Objects",
        "nonNull",
        "(Ljava/lang/Object;)Z",
        c::n_objects_non_null,
    ),
    // Supplier SAM row (M3 Wave 3): lambda closures dispatch here through
    // `lambda::n_sam` (Wave 2's loud placeholder became the real
    // implementation when closures arrived); user classes with a compiled
    // `get()` dispatch into their code via the registry first.
    (
        "java/util/function/Supplier",
        "get",
        "()Ljava/lang/Object;",
        lambda::n_sam,
    ),
    // ------------------------------------------------ M3 Wave 2: java.lang
    // extensions (implementations in `langext.rs`)
    (
        "java/lang/String",
        "substring",
        "(I)Ljava/lang/String;",
        le::n_str_substring_one,
    ),
    (
        "java/lang/String",
        "substring",
        "(II)Ljava/lang/String;",
        le::n_str_substring_two,
    ),
    (
        "java/lang/String",
        "indexOf",
        "(Ljava/lang/String;)I",
        le::n_str_index_of_str,
    ),
    (
        "java/lang/String",
        "indexOf",
        "(Ljava/lang/String;I)I",
        le::n_str_index_of_str,
    ),
    ("java/lang/String", "indexOf", "(I)I", le::n_str_index_of_ch),
    (
        "java/lang/String",
        "lastIndexOf",
        "(I)I",
        le::n_str_last_index_of_ch,
    ),
    (
        "java/lang/String",
        "concat",
        "(Ljava/lang/String;)Ljava/lang/String;",
        le::n_str_concat,
    ),
    (
        "java/lang/String",
        "replace",
        "(CC)Ljava/lang/String;",
        le::n_str_replace_cc,
    ),
    (
        "java/lang/String",
        "toUpperCase",
        "()Ljava/lang/String;",
        le::n_str_to_upper,
    ),
    (
        "java/lang/String",
        "toLowerCase",
        "()Ljava/lang/String;",
        le::n_str_to_lower,
    ),
    (
        "java/lang/String",
        "trim",
        "()Ljava/lang/String;",
        le::n_str_trim,
    ),
    (
        "java/lang/String",
        "compareTo",
        "(Ljava/lang/String;)I",
        le::n_str_compare_to,
    ),
    (
        "java/lang/String",
        "compareToIgnoreCase",
        "(Ljava/lang/String;)I",
        le::n_str_compare_to_ignore_case,
    ),
    (
        "java/lang/String",
        "startsWith",
        "(Ljava/lang/String;)Z",
        le::n_str_starts_with,
    ),
    (
        "java/lang/String",
        "endsWith",
        "(Ljava/lang/String;)Z",
        le::n_str_ends_with,
    ),
    (
        "java/lang/String",
        "contains",
        "(Ljava/lang/CharSequence;)Z",
        le::n_str_contains,
    ),
    (
        "java/lang/String",
        "equalsIgnoreCase",
        "(Ljava/lang/String;)Z",
        le::n_str_equals_ignore_case,
    ),
    (
        "java/lang/String",
        "toCharArray",
        "()[C",
        le::n_str_to_char_array,
    ),
    (
        "java/lang/String",
        "repeat",
        "(I)Ljava/lang/String;",
        le::n_str_repeat,
    ),
    (
        "java/lang/StringBuilder",
        "<init>",
        "(I)V",
        le::n_sb_init_capacity,
    ),
    (
        "java/lang/StringBuilder",
        "reverse",
        "()Ljava/lang/StringBuilder;",
        le::n_sb_reverse,
    ),
    (
        "java/lang/StringBuilder",
        "insert",
        "(ILjava/lang/String;)Ljava/lang/StringBuilder;",
        le::n_sb_insert_str,
    ),
    (
        "java/lang/StringBuilder",
        "insert",
        "(II)Ljava/lang/StringBuilder;",
        le::n_sb_insert_i,
    ),
    (
        "java/lang/StringBuilder",
        "insert",
        "(IC)Ljava/lang/StringBuilder;",
        le::n_sb_insert_c,
    ),
    (
        "java/lang/StringBuilder",
        "insert",
        "(IJ)Ljava/lang/StringBuilder;",
        le::n_sb_insert_j,
    ),
    (
        "java/lang/StringBuilder",
        "deleteCharAt",
        "(I)Ljava/lang/StringBuilder;",
        le::n_sb_delete_char_at,
    ),
    (
        "java/lang/StringBuilder",
        "delete",
        "(II)Ljava/lang/StringBuilder;",
        le::n_sb_delete,
    ),
    (
        "java/lang/StringBuilder",
        "replace",
        "(IILjava/lang/String;)Ljava/lang/StringBuilder;",
        le::n_sb_replace,
    ),
    (
        "java/lang/StringBuilder",
        "setLength",
        "(I)V",
        le::n_sb_set_length,
    ),
    (
        "java/lang/StringBuilder",
        "capacity",
        "()I",
        le::n_sb_capacity,
    ),
    (
        "java/lang/StringBuilder",
        "charAt",
        "(I)C",
        le::n_sb_char_at,
    ),
    (
        "java/lang/StringBuilder",
        "indexOf",
        "(Ljava/lang/String;)I",
        le::n_sb_index_of,
    ),
    (
        "java/lang/Math",
        "floorDiv",
        "(II)I",
        le::n_math_floor_div_i,
    ),
    (
        "java/lang/Math",
        "floorDiv",
        "(JJ)J",
        le::n_math_floor_div_j,
    ),
    (
        "java/lang/Math",
        "floorMod",
        "(II)I",
        le::n_math_floor_mod_i,
    ),
    (
        "java/lang/Math",
        "floorMod",
        "(JJ)J",
        le::n_math_floor_mod_j,
    ),
    (
        "java/lang/Math",
        "floorDiv",
        "(JI)J",
        le::n_math_floor_div_ji,
    ),
    (
        "java/lang/Math",
        "floorMod",
        "(JI)I",
        le::n_math_floor_mod_ji,
    ),
    ("java/lang/Math", "round", "(D)J", le::n_math_round_d),
    ("java/lang/Math", "round", "(F)I", le::n_math_round_f),
    ("java/lang/Math", "sqrt", "(D)D", le::n_math_sqrt),
    ("java/lang/Math", "cbrt", "(D)D", le::n_math_cbrt),
    ("java/lang/Math", "pow", "(DD)D", le::n_math_pow),
    ("java/lang/Math", "random", "()D", le::n_math_random),
    ("java/lang/Math", "floor", "(D)D", le::n_math_floor),
    ("java/lang/Math", "ceil", "(D)D", le::n_math_ceil),
    ("java/lang/Math", "signum", "(D)D", le::n_math_signum_d),
    ("java/lang/Math", "signum", "(F)F", le::n_math_signum_f),
    (
        "java/lang/Math",
        "toIntExact",
        "(J)I",
        le::n_math_to_int_exact,
    ),
    (
        "java/lang/Math",
        "addExact",
        "(II)I",
        le::n_math_add_exact_i,
    ),
    (
        "java/lang/Math",
        "subtractExact",
        "(II)I",
        le::n_math_subtract_exact_i,
    ),
    (
        "java/lang/Math",
        "multiplyExact",
        "(II)I",
        le::n_math_multiply_exact_i,
    ),
    (
        "java/lang/Math",
        "addExact",
        "(JJ)J",
        le::n_math_add_exact_j,
    ),
    (
        "java/lang/Math",
        "subtractExact",
        "(JJ)J",
        le::n_math_subtract_exact_j,
    ),
    (
        "java/lang/Math",
        "multiplyExact",
        "(JJ)J",
        le::n_math_multiply_exact_j,
    ),
    (
        "java/lang/Math",
        "addExact",
        "(JI)J",
        le::n_math_add_exact_ji,
    ),
    (
        "java/lang/Math",
        "subtractExact",
        "(JI)J",
        le::n_math_subtract_exact_ji,
    ),
    (
        "java/lang/Math",
        "multiplyExact",
        "(JI)J",
        le::n_math_multiply_exact_ji,
    ),
    // ------------------------------------------------ EXTERNAL-B9/B5
    // (jbox2d port, 2026-11-13): the transcendental surface from the
    // fdlibm module — registered for BOTH Math and StrictMath, exactly
    // like the owner's port (the honest O1 residual: HotSpot's own
    // intrinsics differ from fdlibm on ~6.5 inputs per 100k; see the
    // fdlibm.rs module doc).
    ("java/lang/Math", "sin", "(D)D", fd::n_math_sin),
    ("java/lang/Math", "cos", "(D)D", fd::n_math_cos),
    ("java/lang/Math", "tan", "(D)D", fd::n_math_tan),
    ("java/lang/Math", "atan", "(D)D", fd::n_math_atan),
    ("java/lang/Math", "atan2", "(DD)D", fd::n_math_atan2),
    ("java/lang/Math", "hypot", "(DD)D", fd::n_math_hypot),
    ("java/lang/StrictMath", "sin", "(D)D", fd::n_math_sin),
    ("java/lang/StrictMath", "cos", "(D)D", fd::n_math_cos),
    ("java/lang/StrictMath", "tan", "(D)D", fd::n_math_tan),
    ("java/lang/StrictMath", "atan", "(D)D", fd::n_math_atan),
    ("java/lang/StrictMath", "atan2", "(DD)D", fd::n_math_atan2),
    ("java/lang/StrictMath", "hypot", "(DD)D", fd::n_math_hypot),
    // EXTERNAL-B5 (jbox2d port): StrictMath is a separate class in the
    // JDK — its full surface mirrors the Math implementations (B5's
    // "35 rows" at the owner's base; 38 here because the …Exact(J,I)
    // trio landed after the owner's snapshot). StrictMath.random shares
    // Math.random's generator: both are deliberately unpredictable and
    // unspecified, no differential fixture may print either.
    ("java/lang/StrictMath", "abs", "(I)I", n_math_abs_i),
    ("java/lang/StrictMath", "abs", "(J)J", n_math_abs_j),
    ("java/lang/StrictMath", "abs", "(F)F", n_math_abs_f),
    ("java/lang/StrictMath", "abs", "(D)D", n_math_abs_d),
    ("java/lang/StrictMath", "max", "(II)I", n_math_max_i),
    ("java/lang/StrictMath", "max", "(JJ)J", n_math_max_j),
    ("java/lang/StrictMath", "max", "(FF)F", n_math_max_f),
    ("java/lang/StrictMath", "max", "(DD)D", n_math_max_d),
    ("java/lang/StrictMath", "min", "(II)I", n_math_min_i),
    ("java/lang/StrictMath", "min", "(JJ)J", n_math_min_j),
    ("java/lang/StrictMath", "min", "(FF)F", n_math_min_f),
    ("java/lang/StrictMath", "min", "(DD)D", n_math_min_d),
    (
        "java/lang/StrictMath",
        "floorDiv",
        "(II)I",
        le::n_math_floor_div_i,
    ),
    (
        "java/lang/StrictMath",
        "floorDiv",
        "(JJ)J",
        le::n_math_floor_div_j,
    ),
    (
        "java/lang/StrictMath",
        "floorDiv",
        "(JI)J",
        le::n_math_floor_div_ji,
    ),
    (
        "java/lang/StrictMath",
        "floorMod",
        "(II)I",
        le::n_math_floor_mod_i,
    ),
    (
        "java/lang/StrictMath",
        "floorMod",
        "(JJ)J",
        le::n_math_floor_mod_j,
    ),
    (
        "java/lang/StrictMath",
        "floorMod",
        "(JI)I",
        le::n_math_floor_mod_ji,
    ),
    ("java/lang/StrictMath", "round", "(D)J", le::n_math_round_d),
    ("java/lang/StrictMath", "round", "(F)I", le::n_math_round_f),
    ("java/lang/StrictMath", "sqrt", "(D)D", le::n_math_sqrt),
    ("java/lang/StrictMath", "cbrt", "(D)D", le::n_math_cbrt),
    ("java/lang/StrictMath", "pow", "(DD)D", le::n_math_pow),
    ("java/lang/StrictMath", "random", "()D", le::n_math_random),
    ("java/lang/StrictMath", "floor", "(D)D", le::n_math_floor),
    ("java/lang/StrictMath", "ceil", "(D)D", le::n_math_ceil),
    (
        "java/lang/StrictMath",
        "signum",
        "(D)D",
        le::n_math_signum_d,
    ),
    (
        "java/lang/StrictMath",
        "signum",
        "(F)F",
        le::n_math_signum_f,
    ),
    (
        "java/lang/StrictMath",
        "toIntExact",
        "(J)I",
        le::n_math_to_int_exact,
    ),
    (
        "java/lang/StrictMath",
        "addExact",
        "(II)I",
        le::n_math_add_exact_i,
    ),
    (
        "java/lang/StrictMath",
        "subtractExact",
        "(II)I",
        le::n_math_subtract_exact_i,
    ),
    (
        "java/lang/StrictMath",
        "multiplyExact",
        "(II)I",
        le::n_math_multiply_exact_i,
    ),
    (
        "java/lang/StrictMath",
        "addExact",
        "(JJ)J",
        le::n_math_add_exact_j,
    ),
    (
        "java/lang/StrictMath",
        "subtractExact",
        "(JJ)J",
        le::n_math_subtract_exact_j,
    ),
    (
        "java/lang/StrictMath",
        "multiplyExact",
        "(JJ)J",
        le::n_math_multiply_exact_j,
    ),
    (
        "java/lang/StrictMath",
        "addExact",
        "(JI)J",
        le::n_math_add_exact_ji,
    ),
    (
        "java/lang/StrictMath",
        "subtractExact",
        "(JI)J",
        le::n_math_subtract_exact_ji,
    ),
    (
        "java/lang/StrictMath",
        "multiplyExact",
        "(JI)J",
        le::n_math_multiply_exact_ji,
    ),
    (
        "java/lang/Integer",
        "toBinaryString",
        "(I)Ljava/lang/String;",
        le::n_int_to_binary,
    ),
    (
        "java/lang/Integer",
        "toOctalString",
        "(I)Ljava/lang/String;",
        le::n_int_to_octal,
    ),
    (
        "java/lang/Integer",
        "toHexString",
        "(I)Ljava/lang/String;",
        le::n_int_to_hex,
    ),
    ("java/lang/Integer", "compare", "(II)I", le::n_int_compare),
    ("java/lang/Integer", "max", "(II)I", le::n_int_max),
    ("java/lang/Integer", "min", "(II)I", le::n_int_min),
    ("java/lang/Integer", "sum", "(II)I", le::n_int_sum),
    ("java/lang/Integer", "bitCount", "(I)I", le::n_int_bit_count),
    (
        "java/lang/Integer",
        "numberOfLeadingZeros",
        "(I)I",
        le::n_int_nlz,
    ),
    (
        "java/lang/Integer",
        "numberOfTrailingZeros",
        "(I)I",
        le::n_int_ntz,
    ),
    (
        "java/lang/Integer",
        "highestOneBit",
        "(I)I",
        le::n_int_highest_one_bit,
    ),
    (
        "java/lang/Integer",
        "lowestOneBit",
        "(I)I",
        le::n_int_lowest_one_bit,
    ),
    (
        "java/lang/Integer",
        "rotateLeft",
        "(II)I",
        le::n_int_rotate_left,
    ),
    (
        "java/lang/Integer",
        "rotateRight",
        "(II)I",
        le::n_int_rotate_right,
    ),
    ("java/lang/Integer", "reverse", "(I)I", le::n_int_reverse),
    (
        "java/lang/Integer",
        "parseInt",
        "(Ljava/lang/String;I)I",
        le::n_int_parse_int_radix,
    ),
    (
        "java/lang/Long",
        "toBinaryString",
        "(J)Ljava/lang/String;",
        le::n_long_to_binary,
    ),
    (
        "java/lang/Long",
        "toOctalString",
        "(J)Ljava/lang/String;",
        le::n_long_to_octal,
    ),
    (
        "java/lang/Long",
        "toHexString",
        "(J)Ljava/lang/String;",
        le::n_long_to_hex,
    ),
    ("java/lang/Long", "compare", "(JJ)I", le::n_long_compare),
    ("java/lang/Long", "max", "(JJ)J", le::n_long_max),
    ("java/lang/Long", "min", "(JJ)J", le::n_long_min),
    ("java/lang/Long", "sum", "(JJ)J", le::n_long_sum),
    ("java/lang/Long", "bitCount", "(J)I", le::n_long_bit_count),
    (
        "java/lang/Long",
        "numberOfLeadingZeros",
        "(J)I",
        le::n_long_nlz,
    ),
    (
        "java/lang/Long",
        "numberOfTrailingZeros",
        "(J)I",
        le::n_long_ntz,
    ),
    (
        "java/lang/Long",
        "highestOneBit",
        "(J)J",
        le::n_long_highest_one_bit,
    ),
    (
        "java/lang/Long",
        "lowestOneBit",
        "(J)J",
        le::n_long_lowest_one_bit,
    ),
    (
        "java/lang/Long",
        "rotateLeft",
        "(JI)J",
        le::n_long_rotate_left,
    ),
    (
        "java/lang/Long",
        "rotateRight",
        "(JI)J",
        le::n_long_rotate_right,
    ),
    ("java/lang/Long", "reverse", "(J)J", le::n_long_reverse),
    (
        "java/lang/Character",
        "isDigit",
        "(C)Z",
        le::n_char_is_digit,
    ),
    (
        "java/lang/Character",
        "isLetter",
        "(C)Z",
        le::n_char_is_letter,
    ),
    (
        "java/lang/Character",
        "isLetterOrDigit",
        "(C)Z",
        le::n_char_is_letter_or_digit,
    ),
    (
        "java/lang/Character",
        "isWhitespace",
        "(C)Z",
        le::n_char_is_whitespace,
    ),
    (
        "java/lang/Character",
        "isUpperCase",
        "(C)Z",
        le::n_char_is_upper_case,
    ),
    (
        "java/lang/Character",
        "isLowerCase",
        "(C)Z",
        le::n_char_is_lower_case,
    ),
    (
        "java/lang/Character",
        "toUpperCase",
        "(C)C",
        le::n_char_to_upper_case,
    ),
    (
        "java/lang/Character",
        "toLowerCase",
        "(C)C",
        le::n_char_to_lower_case,
    ),
    (
        "java/lang/Character",
        "getNumericValue",
        "(C)I",
        le::n_char_get_numeric_value,
    ),
    (
        "java/lang/Character",
        "compare",
        "(CC)I",
        ju::n_char_compare,
    ),
    ("java/lang/Double", "compare", "(DD)I", ju::n_double_compare),
    // ------------------------------- Real-program JDK surface (jdkutil.rs)
    // Arrays: toString/fill/copyOf/copyOfRange/equals/sort. Static natives —
    // the receiver is always None. Object sorts dispatch comparisons
    // (compareTo / Comparator) so user classes, bridges and lambda
    // comparators all resolve through the ordinary walk.
    (
        "java/util/Arrays",
        "toString",
        "([I)Ljava/lang/String;",
        ju::n_arrays_to_string_int,
    ),
    (
        "java/util/Arrays",
        "toString",
        "([J)Ljava/lang/String;",
        ju::n_arrays_to_string_long,
    ),
    (
        "java/util/Arrays",
        "toString",
        "([D)Ljava/lang/String;",
        ju::n_arrays_to_string_double,
    ),
    (
        "java/util/Arrays",
        "toString",
        "([Z)Ljava/lang/String;",
        ju::n_arrays_to_string_bool,
    ),
    (
        "java/util/Arrays",
        "toString",
        "([C)Ljava/lang/String;",
        ju::n_arrays_to_string_char,
    ),
    (
        "java/util/Arrays",
        "toString",
        "([Ljava/lang/Object;)Ljava/lang/String;",
        ju::n_arrays_to_string_obj,
    ),
    ("java/util/Arrays", "fill", "([II)V", ju::n_arrays_fill_int),
    ("java/util/Arrays", "fill", "([JJ)V", ju::n_arrays_fill_long),
    // char[] fill (corpus wave 1: minimal-json number formatting)
    ("java/util/Arrays", "fill", "([CC)V", ju::n_arrays_fill_char),
    (
        "java/util/Arrays",
        "fill",
        "([Ljava/lang/Object;Ljava/lang/Object;)V",
        ju::n_arrays_fill_obj,
    ),
    (
        "java/util/Arrays",
        "copyOf",
        "([II)[I",
        ju::n_arrays_copy_of_int,
    ),
    (
        "java/util/Arrays",
        "copyOf",
        "([JI)[J",
        ju::n_arrays_copy_of_long,
    ),
    (
        "java/util/Arrays",
        "copyOf",
        "([DI)[D",
        ju::n_arrays_copy_of_double,
    ),
    (
        "java/util/Arrays",
        "copyOf",
        "([Ljava/lang/Object;I)[Ljava/lang/Object;",
        ju::n_arrays_copy_of_obj,
    ),
    (
        "java/util/Arrays",
        "copyOf",
        "([Ljava/lang/Object;ILjava/lang/Class;)[Ljava/lang/Object;",
        ju::n_arrays_copy_of_obj,
    ),
    (
        "java/util/Arrays",
        "copyOfRange",
        "([III)[I",
        ju::n_arrays_copy_of_range_int,
    ),
    (
        "java/util/Arrays",
        "copyOfRange",
        "([Ljava/lang/Object;II)[Ljava/lang/Object;",
        ju::n_arrays_copy_of_range_obj,
    ),
    (
        "java/util/Arrays",
        "equals",
        "([I[I)Z",
        ju::n_arrays_equals_int,
    ),
    (
        "java/util/Arrays",
        "equals",
        "([J[J)Z",
        ju::n_arrays_equals_long,
    ),
    (
        "java/util/Arrays",
        "equals",
        "([Ljava/lang/Object;[Ljava/lang/Object;)Z",
        ju::n_arrays_equals_obj,
    ),
    ("java/util/Arrays", "sort", "([I)V", ju::n_arrays_sort_int),
    ("java/util/Arrays", "sort", "([J)V", ju::n_arrays_sort_long),
    (
        "java/util/Arrays",
        "sort",
        "([D)V",
        ju::n_arrays_sort_double,
    ),
    (
        "java/util/Arrays",
        "sort",
        "([Ljava/lang/Object;)V",
        ju::n_arrays_sort_obj,
    ),
    (
        "java/util/Arrays",
        "sort",
        "([Ljava/lang/Object;Ljava/util/Comparator;)V",
        ju::n_arrays_sort_obj_cmp,
    ),
    // EXTERNAL-B12 (jbox2d port): the range forms with the JDK's exact
    // rangeCheck messages; a null comparator falls back to natural
    // ordering (OpenJDK's first branch).
    (
        "java/util/Arrays",
        "sort",
        "([Ljava/lang/Object;II)V",
        ju::n_arrays_sort_obj_range,
    ),
    (
        "java/util/Arrays",
        "sort",
        "([Ljava/lang/Object;IILjava/util/Comparator;)V",
        ju::n_arrays_sort_obj_range_cmp,
    ),
    // EXTERNAL-B12: Comparator.reverseOrder() — the Comparators
    // ReverseComparator singleton shape (behavior in jdkutil.rs).
    (
        "java/util/Comparator",
        "reverseOrder",
        "()Ljava/util/Comparator;",
        ju::n_comparator_reverse_order,
    ),
    (
        "java/util/Comparators$ReverseComparator",
        "compare",
        "(Ljava/lang/Object;Ljava/lang/Object;)I",
        ju::n_reverse_comparator_compare,
    ),
    // Collections: sort (comparator + natural), reverse, swap — dispatch
    // through List.get/set so any List implementation works.
    (
        "java/util/Collections",
        "sort",
        "(Ljava/util/List;Ljava/util/Comparator;)V",
        ju::n_collections_sort_cmp,
    ),
    (
        "java/util/Collections",
        "sort",
        "(Ljava/util/List;)V",
        ju::n_collections_sort_natural,
    ),
    (
        "java/util/Collections",
        "reverse",
        "(Ljava/util/List;)V",
        ju::n_collections_reverse,
    ),
    (
        "java/util/Collections",
        "swap",
        "(Ljava/util/List;II)V",
        ju::n_collections_swap,
    ),
    (
        "java/util/Collections",
        "addAll",
        "(Ljava/util/Collection;[Ljava/lang/Object;)Z",
        ju::n_collections_add_all,
    ),
    // ArrayList.subList + the live view's own surface. The view delegates
    // through List dispatch and enforces parent modCount discipline.
    (
        "java/util/ArrayList",
        "subList",
        "(II)Ljava/util/List;",
        ju::n_al_sub_list,
    ),
    // List-declared row: javac emits invokeinterface List.subList when the
    // static type is List — compile-time validation walks the DECLARED
    // hierarchy, runtime dispatch walks the receiver's (ArrayList) chain.
    (
        "java/util/List",
        "subList",
        "(II)Ljava/util/List;",
        ju::n_al_sub_list,
    ),
    (
        "java/util/ArrayList$SubList",
        "get",
        "(I)Ljava/lang/Object;",
        ju::n_sl_get,
    ),
    (
        "java/util/ArrayList$SubList",
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        ju::n_sl_set,
    ),
    (
        "java/util/ArrayList$SubList",
        "add",
        "(ILjava/lang/Object;)V",
        ju::n_sl_add_at,
    ),
    (
        "java/util/ArrayList$SubList",
        "add",
        "(Ljava/lang/Object;)Z",
        ju::n_sl_add_append,
    ),
    (
        "java/util/ArrayList$SubList",
        "remove",
        "(I)Ljava/lang/Object;",
        ju::n_sl_remove_at,
    ),
    ("java/util/ArrayList$SubList", "size", "()I", ju::n_sl_size),
    (
        "java/util/ArrayList$SubList",
        "isEmpty",
        "()Z",
        ju::n_sl_is_empty,
    ),
    (
        "java/util/ArrayList$SubList",
        "toString",
        "()Ljava/lang/String;",
        ju::n_sl_to_string,
    ),
    // Collections.unmodifiableList + the view's surface (corpus wave 1:
    // minimal-json). Reads delegate to the backing list; writes throw
    // UnsupportedOperationException — JDK wrapper semantics.
    (
        "java/util/Collections",
        "unmodifiableList",
        "(Ljava/util/List;)Ljava/util/List;",
        ju::n_unmod_list,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "get",
        "(I)Ljava/lang/Object;",
        ju::n_unmod_get,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        ju::n_unmod_set,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "add",
        "(Ljava/lang/Object;)Z",
        ju::n_unmod_add,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "add",
        "(ILjava/lang/Object;)V",
        ju::n_unmod_add_at,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "remove",
        "(I)Ljava/lang/Object;",
        ju::n_unmod_remove_at,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "remove",
        "(Ljava/lang/Object;)Z",
        ju::n_unmod_remove_obj,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "clear",
        "()V",
        ju::n_unmod_clear,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "size",
        "()I",
        ju::n_unmod_size,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "isEmpty",
        "()Z",
        ju::n_unmod_is_empty,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "contains",
        "(Ljava/lang/Object;)Z",
        ju::n_unmod_contains,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "indexOf",
        "(Ljava/lang/Object;)I",
        ju::n_unmod_index_of,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "iterator",
        "()Ljava/util/Iterator;",
        ju::n_unmod_iterator,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "toString",
        "()Ljava/lang/String;",
        ju::n_unmod_to_string,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "hashCode",
        "()I",
        ju::n_unmod_hash_code,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "equals",
        "(Ljava/lang/Object;)Z",
        ju::n_unmod_equals,
    ),
    (
        "java/util/Collections$UnmodifiableList",
        "lastIndexOf",
        "(Ljava/lang/Object;)I",
        ju::n_unmod_last_index_of,
    ),
    (
        "java/lang/String",
        "compareTo",
        "(Ljava/lang/Object;)I",
        n_str_compare_to_bridge,
    ),
    (
        "java/lang/Integer",
        "compareTo",
        "(Ljava/lang/Object;)I",
        n_integer_compare_to_bridge,
    ),
    (
        "java/lang/Long",
        "compareTo",
        "(Ljava/lang/Object;)I",
        n_long_compare_to_bridge,
    ),
    (
        "java/lang/Double",
        "compareTo",
        "(Ljava/lang/Object;)I",
        n_double_compare_to_bridge,
    ),
    // ------------------------------------------------ M3 Wave 3: lambdas
    // One SAM row per interface in the M3 set (M3_PLAN §6.1). Closure
    // objects carry their interface as the object class, so the dispatch
    // walk reaches these rows directly; the shared implementation
    // reassembles the recorded target call (see `lambda.rs`).
    (
        "java/util/function/Function",
        "apply",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        lambda::n_sam,
    ),
    (
        "java/util/function/BiFunction",
        "apply",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        lambda::n_sam,
    ),
    (
        "java/util/function/Consumer",
        "accept",
        "(Ljava/lang/Object;)V",
        lambda::n_sam,
    ),
    (
        "java/util/function/BiConsumer",
        "accept",
        "(Ljava/lang/Object;Ljava/lang/Object;)V",
        lambda::n_sam,
    ),
    (
        "java/util/Comparator",
        "compare",
        "(Ljava/lang/Object;Ljava/lang/Object;)I",
        lambda::n_sam,
    ),
    (
        "java/util/function/Predicate",
        "test",
        "(Ljava/lang/Object;)Z",
        lambda::n_sam,
    ),
    (
        "java/util/function/BiPredicate",
        "test",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        lambda::n_sam,
    ),
    (
        "java/util/function/UnaryOperator",
        "apply",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        lambda::n_sam,
    ),
    (
        "java/util/function/BinaryOperator",
        "apply",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        lambda::n_sam,
    ),
    // F2 probe wave: java.lang.Runnable.run — the LAST supported SAM
    // interface without a direct-call row. Wave 2 added Runnable for
    // `new Thread(() -> ...)` only, where the closure is consumed by
    // Thread.start() and never invoked through its interface type; a
    // direct `r.run()` on a Runnable-typed value (local, static, or
    // field) dead-ended codegen validation with "virtual call target
    // run()V not found on java/lang/Runnable" — the same row shape every
    // other supported interface has. User classes implementing Runnable
    // still dispatch through the ordinary registry walk first.
    ("java/lang/Runnable", "run", "()V", lambda::n_sam),
    // M3 Wave 5: reflection subset (M3_PLAN §8)
    (
        "java/lang/Object",
        "getClass",
        "()Ljava/lang/Class;",
        n_object_get_class,
    ),
    (
        "java/lang/Class",
        "getName",
        "()Ljava/lang/String;",
        n_class_get_name,
    ),
    (
        "java/lang/Class",
        "getSimpleName",
        "()Ljava/lang/String;",
        n_class_get_simple_name,
    ),
    (
        "java/lang/Class",
        "toString",
        "()Ljava/lang/String;",
        n_class_to_string,
    ),
    (
        "java/lang/Class",
        "isInstance",
        "(Ljava/lang/Object;)Z",
        n_class_is_instance,
    ),
    (
        "java/lang/Class",
        "isAssignableFrom",
        "(Ljava/lang/Class;)Z",
        n_class_is_assignable_from,
    ),
    (
        "java/lang/Class",
        "isInterface",
        "()Z",
        n_class_is_interface,
    ),
    ("java/lang/Class", "isArray", "()Z", n_class_is_array),
    // EXTERNAL-B12 (jbox2d port): assertions are not a j2r feature, so
    // the JDK default (disabled) is the honest answer.
    (
        "java/lang/Class",
        "desiredAssertionStatus",
        "()Z",
        ju::n_class_desired_assertion_status,
    ),
    (
        "java/lang/Class",
        "isPrimitive",
        "()Z",
        n_class_is_primitive,
    ),
    (
        "java/lang/Class",
        "forName",
        "(Ljava/lang/String;)Ljava/lang/Class;",
        n_class_for_name,
    ),
    (
        "java/lang/Class",
        "forName",
        "(Ljava/lang/String;ZLjava/lang/ClassLoader;)Ljava/lang/Class;",
        n_class_for_name3,
    ),
    (
        "java/lang/ClassLoader",
        "getSystemClassLoader",
        "()Ljava/lang/ClassLoader;",
        n_classloader_get_system,
    ),
    (
        "java/lang/Class",
        "getSuperclass",
        "()Ljava/lang/Class;",
        n_class_get_superclass,
    ),
    (
        "java/lang/Class",
        "getComponentType",
        "()Ljava/lang/Class;",
        n_class_get_component_type,
    ),
    (
        "java/lang/Class",
        "getDeclaredField",
        "(Ljava/lang/String;)Ljava/lang/reflect/Field;",
        n_class_get_declared_field,
    ),
    (
        "java/lang/Class",
        "getDeclaredMethod",
        "(Ljava/lang/String;[Ljava/lang/Class;)Ljava/lang/reflect/Method;",
        n_class_get_declared_method,
    ),
    (
        "java/lang/reflect/Field",
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        n_field_get,
    ),
    (
        "java/lang/reflect/Field",
        "set",
        "(Ljava/lang/Object;Ljava/lang/Object;)V",
        n_field_set,
    ),
    (
        "java/lang/reflect/Field",
        "getName",
        "()Ljava/lang/String;",
        n_field_get_name,
    ),
    (
        "java/lang/reflect/Field",
        "getType",
        "()Ljava/lang/Class;",
        n_field_get_type,
    ),
    (
        "java/lang/reflect/Method",
        "getName",
        "()Ljava/lang/String;",
        n_method_get_name,
    ),
    (
        "java/lang/reflect/Method",
        "equals",
        "(Ljava/lang/Object;)Z",
        n_method_equals,
    ),
    (
        "java/lang/reflect/Method",
        "invoke",
        "(Ljava/lang/Object;[Ljava/lang/Object;)Ljava/lang/Object;",
        n_method_invoke,
    ),
    ("java/lang/System", "gc", "()V", n_system_gc),
];

/// Exact lookup: `(class, name, descriptor)`.
pub fn lookup_exact(class: &str, name: &str, desc: &str) -> Option<NativeFn> {
    NATIVES
        .iter()
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
        _ => return Err(throw_new("java/lang/NullPointerException", None)),
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
        _ => return Err(throw_new("java/lang/NullPointerException", None)),
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
        _ => return Err(throw_new("java/lang/NullPointerException", None)),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn no_ctx() -> Dispatch {
        Dispatch::default()
    }

    fn boxed(class: &str, v: Value) -> JRef {
        let r = new_obj(class, 1).unwrap();
        object::obj_set(&r, 0, v);
        r
    }

    #[test]
    fn integer_value_of_caches_minus128_to_127() {
        let _rt = crate::test_support::runtime();
        let f = |args: &[Value]| match n_integer_value_of(None, args, no_ctx()).unwrap() {
            Value::Ref(Some(r)) => r,
            other => panic!("expected a reference, got {other:?}"),
        };
        let a = f(&[Value::Int(127)]);
        let b = f(&[Value::Int(127)]);
        assert!(crate::ref_eq(&Value::Ref(Some(a)), &Value::Ref(Some(b))));
        let c = f(&[Value::Int(128)]);
        let d = f(&[Value::Int(128)]);
        assert!(!crate::ref_eq(&Value::Ref(Some(c)), &Value::Ref(Some(d))));
        let neg = f(&[Value::Int(-128)]);
        let neg2 = f(&[Value::Int(-128)]);
        assert!(crate::ref_eq(
            &Value::Ref(Some(neg)),
            &Value::Ref(Some(neg2))
        ));
    }

    #[test]
    fn boxed_equals_compares_payload_and_type() {
        let _rt = crate::test_support::runtime();
        let i1 = boxed("java/lang/Integer", Value::Int(1000));
        let i2 = boxed("java/lang/Integer", Value::Int(1000));
        let r = n_integer_equals(Some(&i1), &[Value::Ref(Some(i2))], no_ctx()).unwrap();
        assert_eq!(r.expect_int(), 1);
        let l = boxed("java/lang/Long", Value::Long(1000));
        let r = n_integer_equals(Some(&i1), &[Value::Ref(Some(l))], no_ctx()).unwrap();
        // Integer(1000).equals(Long(1000)) is false in the JDK
        assert_eq!(r.expect_int(), 0);
        let i3 = boxed("java/lang/Integer", Value::Int(7));
        let r = n_integer_equals(Some(&i1), &[Value::Ref(Some(i3))], no_ctx()).unwrap();
        assert_eq!(r.expect_int(), 0);
    }

    #[test]
    fn float_equals_uses_bits_nan_is_nan() {
        let _rt = crate::test_support::runtime();
        let nan: f32 = f32::NAN;
        let a = boxed("java/lang/Float", Value::Float(nan));
        let b = boxed("java/lang/Float", Value::Float(nan));
        let r = n_float_equals(Some(&a), &[Value::Ref(Some(b))], no_ctx()).unwrap();
        // Float.equals: NaN equals NaN
        assert_eq!(r.expect_int(), 1);
        let z = boxed("java/lang/Float", Value::Float(0.0));
        let nz = boxed("java/lang/Float", Value::Float(-0.0));
        let r = n_float_equals(Some(&z), &[Value::Ref(Some(nz))], no_ctx()).unwrap();
        // +0.0 and -0.0 are NOT equal under Float.equals
        assert_eq!(r.expect_int(), 0);
    }

    #[test]
    fn boxed_hash_codes_match_jdk_contract() {
        let _rt = crate::test_support::runtime();
        let i = boxed("java/lang/Integer", Value::Int(42));
        assert_eq!(
            n_integer_hash_code(Some(&i), &[], no_ctx())
                .unwrap()
                .expect_int(),
            42
        );
        let l = boxed("java/lang/Long", Value::Long(0x1_0000_0042));
        // Long.hashCode: (int)(v ^ (v >>> 32))
        let v: i64 = 0x1_0000_0042;
        assert_eq!(
            n_long_hash_code(Some(&l), &[], no_ctx())
                .unwrap()
                .expect_int(),
            (v ^ (v >> 32)) as i32
        );
        let t = boxed("java/lang/Boolean", Value::Int(1));
        assert_eq!(
            n_boolean_hash_code(Some(&t), &[], no_ctx())
                .unwrap()
                .expect_int(),
            1231
        );
        let f = boxed("java/lang/Boolean", Value::Int(0));
        assert_eq!(
            n_boolean_hash_code(Some(&f), &[], no_ctx())
                .unwrap()
                .expect_int(),
            1237
        );
    }

    #[test]
    fn lookup_finds_exact_entries() {
        let _rt = crate::test_support::runtime();
        assert!(lookup_exact("java/lang/Math", "max", "(II)I").is_some());
        assert!(lookup_exact("java/lang/Math", "max", "(IJ)I").is_none());
        assert!(lookup_exact("java/io/PrintStream", "println", "(Ljava/lang/String;)V").is_some());
    }

    #[test]
    fn string_hash_code_matches_java() {
        let _rt = crate::test_support::runtime();
        let s = object::new_string("hello");
        let r = n_string_hash_code(Some(&s), &[], no_ctx()).unwrap();
        assert_eq!(r.expect_int(), 99162322);
        let e = object::new_string("");
        assert_eq!(
            n_string_hash_code(Some(&e), &[], no_ctx())
                .unwrap()
                .expect_int(),
            0
        );
    }

    #[test]
    fn utf16_length_semantics() {
        let _rt = crate::test_support::runtime();
        // U+1F600 is one code point, two UTF-16 units — Java length() == 2
        let s = object::new_string("\u{1F600}");
        assert_eq!(
            n_string_length(Some(&s), &[], no_ctx())
                .unwrap()
                .expect_int(),
            2
        );
    }

    #[test]
    fn parse_int_java_semantics() {
        let _rt = crate::test_support::runtime();
        let s = object::new_string("42");
        assert_eq!(
            n_integer_parse_int(None, &[Value::Ref(Some(s))], no_ctx())
                .unwrap()
                .expect_int(),
            42
        );
        let neg = object::new_string("-2147483648");
        assert_eq!(
            n_integer_parse_int(None, &[Value::Ref(Some(neg))], no_ctx())
                .unwrap()
                .expect_int(),
            i32::MIN
        );
        let over = object::new_string("2147483648");
        let err = n_integer_parse_int(None, &[Value::Ref(Some(over))], no_ctx()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/NumberFormatException");
        let empty = object::new_string("");
        assert!(n_integer_parse_int(None, &[Value::Ref(Some(empty))], no_ctx()).is_err());
        let plus = object::new_string("+7");
        assert_eq!(
            n_integer_parse_int(None, &[Value::Ref(Some(plus))], no_ctx())
                .unwrap()
                .expect_int(),
            7
        );
    }

    #[test]
    fn math_max_nan_and_zero_signs() {
        let _rt = crate::test_support::runtime();
        let nan = f64::NAN;
        assert!(java_max_d(nan, 1.0).is_nan());
        assert_eq!(java_max_d(0.0, -0.0), 0.0f64.copysign(1.0));
        assert_eq!(java_min_d(0.0, -0.0).to_bits(), (-0.0f64).to_bits());
    }

    #[test]
    fn boxed_roundtrip() {
        let _rt = crate::test_support::runtime();
        let b = n_integer_value_of(None, &[Value::Int(5)], no_ctx()).unwrap();
        let r = b.expect_ref().unwrap();
        assert_eq!(
            crate::heap::with_obj(r, |o| o.jclass.to_string()),
            "java/lang/Integer"
        );
        assert_eq!(
            n_integer_int_value(Some(&r), &[], no_ctx())
                .unwrap()
                .expect_int(),
            5
        );
    }

    #[test]
    fn sb_append_and_to_string() {
        let _rt = crate::test_support::runtime();
        let sb = object::new_builtin("java/lang/StringBuilder").unwrap();
        n_sb_init(Some(&sb), &[], no_ctx()).unwrap();
        n_sb_append_i(Some(&sb), &[Value::Int(12)], no_ctx()).unwrap();
        n_sb_append_s(
            Some(&sb),
            &[Value::Ref(Some(object::new_string(" x")))],
            no_ctx(),
        )
        .unwrap();
        let out = n_sb_to_string(Some(&sb), &[], no_ctx()).unwrap();
        assert_eq!(&*object::string_value(&out.expect_ref().unwrap()), "12 x");
    }

    #[test]
    fn println_writes_to_stdout() {
        let _rt = crate::test_support::runtime();
        let ps = object::system_out_ref();
        n_ps_println_s(
            Some(&ps),
            &[Value::Ref(Some(object::new_string(
                "natives println works",
            )))],
            no_ctx(),
        )
        .unwrap();
    }

    #[test]
    fn throwable_to_string_format() {
        let _rt = crate::test_support::runtime();
        let t = throw_new("pkg/MyError", Some("boom"));
        let out = n_throwable_to_string(Some(&t.obj), &[], no_ctx()).unwrap();
        assert_eq!(
            &*object::string_value(&out.expect_ref().unwrap()),
            "pkg.MyError: boom"
        );
    }
}
