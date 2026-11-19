use super::*;

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
pub(crate) fn n_pw_init_os(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
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
pub(crate) fn n_pw_init_writer(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
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
pub(crate) fn n_pw_print_c(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let sink = pw_sink_of(recv)?;
    let c = arg(args, "PrintWriter.print").expect_int();
    let ch = char::from_u32(c as u32).unwrap_or('\u{fffd}');
    let mut buf = String::with_capacity(4);
    buf.push(ch);
    pw_write_str(&sink, &buf, false)?;
    Ok(Value::Void)
}
pub(crate) fn n_pw_print_s(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = ctx;
    pw_write(recv, args)
}
pub(crate) fn n_pw_println(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let sink = pw_sink_of(recv)?;
    pw_write_str(&sink, "\n", false)?;
    Ok(Value::Void)
}
pub(crate) fn n_pw_println_s(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    // JDK: print(x) then println() — two write calls, one newline.
    n_pw_print_s(recv, args, ctx)?;
    n_pw_println(recv, &[], ctx)
}
pub(crate) fn n_pw_write_s(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = ctx;
    pw_write(recv, args)
}
pub(crate) fn n_pw_flush(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    // Native streams are flushed per write; StringWriter keeps everything
    // in the payload. Nothing buffered here to move.
    Ok(Value::Void)
}
pub(crate) fn n_pw_close(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
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
pub(crate) fn n_loud_vector(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    loud_unsupported(
        "J2R: java.util.Vector is not supported (corpus wave 3 registration: legacy synchronized collections machinery does not exist; JAMA's uncalled Matrix.read references it)",
    )
}
pub(crate) fn n_loud_stream_tokenizer(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    loud_unsupported(
        "J2R: java.io.StreamTokenizer is not supported (corpus wave 3 registration: the syntax-table tokenizer is unmodeled; JAMA's uncalled Matrix.read references it)",
    )
}
pub(crate) fn n_loud_decimal_format(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    loud_unsupported(
        "J2R: java.text.DecimalFormat/NumberFormat is not supported (corpus wave 3 registration: the HALF_EVEN formatting engine with symbol tables is a registered gap; JAMA's uncalled Matrix.print references it)",
    )
}
