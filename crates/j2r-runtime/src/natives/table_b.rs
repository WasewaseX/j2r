use super::*;

pub static PART: &[(&str, &str, &str, NativeFn)] = &[
    (
        "java/lang/Double",
        "longBitsToDouble",
        "(J)D",
        n_long_bits_to_double,
    ),
    // HexFloatProbe wave: the JDK hex-string formatters (Temurin-pinned)
    (
        "java/lang/Float",
        "toHexString",
        "(F)Ljava/lang/String;",
        n_float_to_hex_string,
    ),
    (
        "java/lang/Double",
        "toHexString",
        "(D)Ljava/lang/String;",
        n_double_to_hex_string,
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
    // the registered copy-ctor gap CLOSED: putMapEntries on a fresh
    // table — the pre-size law + the source's iteration order drives
    // insertion (the HashMapCopyProbe laws, scripts/evidence/hm_copy_ctor/)
    (
        "java/util/HashMap",
        "<init>",
        "(Ljava/util/Map;)V",
        c::n_hm_init,
    ),
    ("java/util/HashMap", "put", SIG_OBJECT_OBJECT, c::n_hm_put),
    ("java/util/HashMap", "get", SIG_OBJECT, c::n_hm_get),
    (
        "java/util/HashMap",
        "getOrDefault",
        SIG_OBJECT_OBJECT,
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
    ("java/util/HashMap", "remove", SIG_OBJECT, c::n_hm_remove),
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
    (
        "java/util/HashMap$EntryIterator",
        "remove",
        "()V",
        c::n_hi_remove,
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
];
