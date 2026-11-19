use super::*;

pub static PART: &[(&str, &str, &str, NativeFn)] = &[
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
];
