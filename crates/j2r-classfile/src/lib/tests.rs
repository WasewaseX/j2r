use super::*;
use testclass::Builder;

/// The Object class path the fixture builders repeat.
const OBJ_NAME: &str = "java/lang/Object";

/// Parse-or-panic: a fixture that doesn't parse is a bug.
fn parse_ok(bytes: &[u8]) -> ClassFile {
    ClassFile::parse(bytes).unwrap()
}

#[test]
fn rejects_bad_magic() {
    let err = ClassFile::parse(&[1, 2, 3, 4]).unwrap_err();
    assert!(err.message.contains("magic"));
}

#[test]
fn rejects_trailing_garbage() {
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[], &[], &[]);
    let mut bad = bytes.clone();
    bad.push(0xFF);
    let err = ClassFile::parse(&bad).unwrap_err();
    assert!(err.message.contains("trailing"));
    // and the clean version parses
    let cf = parse_ok(&bytes);
    assert_eq!(cf.this_class, "T");
    assert_eq!(cf.super_class.as_deref(), Some("java/lang/Object"));
}

#[test]
fn long_takes_two_pool_slots() {
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let field_name = b.utf8("BIG");
    let field_desc = b.utf8("J");
    let long_idx = b.long(0x7FFF_FFFF_FFFF_FFFF);
    let cv_name = b.utf8("ConstantValue");
    let field = testclass::field(
        flags::ACC_PUBLIC | flags::ACC_STATIC | flags::ACC_FINAL,
        field_name,
        field_desc,
        Some(long_idx),
        cv_name,
    );
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[&field], &[], &[]);
    let cf = parse_ok(&bytes);
    let f = cf.field("BIG").unwrap();
    assert_eq!(f.descriptor, "J");
    assert!(f.is_static());
    assert_eq!(
        f.constant_value,
        Some(ConstantValue::Long(0x7FFF_FFFF_FFFF_FFFF))
    );
    assert_eq!(
        cf.constant_pool.long(long_idx).unwrap(),
        0x7FFF_FFFF_FFFF_FFFF
    );
}

#[test]
fn methods_with_code_and_exceptions_parse() {
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let m_name = b.utf8("run");
    let m_desc = b.utf8("()V");
    let code_name = b.utf8("Code");
    let exc_name = b.utf8("java/lang/RuntimeException");
    let exc_cls = b.class(exc_name);
    let m = testclass::method(
        flags::ACC_PUBLIC,
        m_name,
        m_desc,
        2,
        1,
        &[0xB1], // return
        &[(0, 1, 1, exc_cls)],
        code_name,
    );
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[], &[&m], &[]);
    let cf = parse_ok(&bytes);
    let m = cf.method("run", "()V").unwrap();
    let code = m.code().unwrap();
    assert_eq!(code.code, vec![0xB1]);
    assert_eq!(code.max_stack, 2);
    assert_eq!(code.exception_table.len(), 1);
    assert_eq!(
        code.exception_table[0].catch_type.as_deref(),
        Some("java/lang/RuntimeException")
    );
}

#[test]
fn modified_utf8_nul_decodes() {
    let mut b = Builder::new();
    // raw Utf8 entry with modified-UTF-8 NUL (0xC0 0x80): "a\u{0}" is 3 bytes
    b.pool.push(1);
    b.pool.extend_from_slice(&3u16.to_be_bytes());
    b.pool.extend_from_slice(&[b'a', 0xC0, 0x80]);
    let idx = b.idx;
    b.idx += 1;
    let cls = b.class(idx);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[], &[], &[]);
    let cf = parse_ok(&bytes);
    assert_eq!(cf.this_class, "a\u{0}");
}

// -------------------------------------------------- audit A4 validation

#[test]
fn rejects_constant_value_with_wrong_attribute_length() {
    for bad_len in [0u32, 1, 3, 4] {
        let mut b = Builder::new();
        let name = b.utf8("T");
        let cls = b.class(name);
        let obj_name = b.utf8(OBJ_NAME);
        let obj_cls = b.class(obj_name);
        let f_name = b.utf8("K");
        let f_desc = b.utf8("I");
        let int_idx = b.integer(42);
        let cv_name = b.utf8("ConstantValue");
        let field = testclass::field_with_constant_value_len(
            flags::ACC_PUBLIC | flags::ACC_STATIC | flags::ACC_FINAL,
            f_name,
            f_desc,
            bad_len,
            int_idx,
            cv_name,
        );
        let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[&field], &[], &[]);
        let err = ClassFile::parse(&bytes).unwrap_err();
        assert!(
            err.message.contains("ConstantValue"),
            "attribute_length {bad_len}: {}",
            err.message
        );
    }
}

#[test]
fn accepts_constant_value_with_exact_length() {
    // Control for the rejection test above: length 2 parses and yields
    // the constant (JVMS §4.7.16, audit A4).
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let f_name = b.utf8("K");
    let f_desc = b.utf8("I");
    let int_idx = b.integer(42);
    let cv_name = b.utf8("ConstantValue");
    let field = testclass::field_with_constant_value_len(
        flags::ACC_PUBLIC | flags::ACC_STATIC | flags::ACC_FINAL,
        f_name,
        f_desc,
        2,
        int_idx,
        cv_name,
    );
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[&field], &[], &[]);
    let cf = parse_ok(&bytes);
    assert_eq!(
        cf.field("K").unwrap().constant_value,
        Some(ConstantValue::Int(42))
    );
}

#[test]
fn rejects_zero_code_length() {
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let m_name = b.utf8("run");
    let m_desc = b.utf8("()V");
    let code_name = b.utf8("Code");
    let m = testclass::method_with_code_len(
        testclass::access_public_static(),
        m_name,
        m_desc,
        1,
        1,
        0,
        &[],
        code_name,
    );
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[], &[&m], &[]);
    let err = ClassFile::parse(&bytes).unwrap_err();
    assert!(err.message.contains("code_length"), "{}", err.message);
}

#[test]
fn rejects_oversized_code_length() {
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let m_name = b.utf8("run");
    let m_desc = b.utf8("()V");
    let code_name = b.utf8("Code");
    // JVMS §4.7.3 upper bound: code_length < 65536. One real byte of
    // code, a length field claiming 65536 — the bound check must fire
    // before the byte read.
    let m = testclass::method_with_code_len(
        testclass::access_public_static(),
        m_name,
        m_desc,
        1,
        1,
        65536,
        &[0xB1],
        code_name,
    );
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[], &[&m], &[]);
    let err = ClassFile::parse(&bytes).unwrap_err();
    assert!(
        err.message.contains("code_length") && err.message.contains("65536"),
        "{}",
        err.message
    );
}

#[test]
fn code_sub_attributes_are_walked() {
    // Positive: a Code body carrying one well-formed sub-attribute
    // (LineNumberTable with an empty table) parses, and the walk
    // consumes the body exactly.
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let m_name = b.utf8("run");
    let m_desc = b.utf8("()V");
    let code_name = b.utf8("Code");
    let lnt_name = b.utf8("LineNumberTable");
    let mut lnt = Vec::new();
    lnt.extend_from_slice(&lnt_name.to_be_bytes());
    lnt.extend_from_slice(&2u32.to_be_bytes()); // length: u2 line-number count
    lnt.extend_from_slice(&0u16.to_be_bytes()); // zero entries
    let m = testclass::method_with_sub_attrs(
        testclass::access_public_static(),
        m_name,
        m_desc,
        1,
        1,
        &[0xB1],
        &[&lnt],
        code_name,
    );
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[], &[&m], &[]);
    let cf = ClassFile::parse(&bytes).expect("sub-attribute walk must accept well-formed body");
    assert!(cf.method("run", "()V").unwrap().code().is_ok());
}

#[test]
fn code_sub_attribute_with_wrong_length_is_rejected() {
    // The sub-attribute length undershoots its payload (claims 1,
    // payload is 2 bytes): the walk leaves trailing bytes inside the
    // Code body — previously silent, now a precise error (audit A4).
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let m_name = b.utf8("run");
    let m_desc = b.utf8("()V");
    let code_name = b.utf8("Code");
    let lnt_name = b.utf8("LineNumberTable");
    let mut lnt = Vec::new();
    lnt.extend_from_slice(&lnt_name.to_be_bytes());
    lnt.extend_from_slice(&1u32.to_be_bytes()); // claims 1...
    lnt.extend_from_slice(&[0x00, 0x00]); // ...payload is 2
    let m = testclass::method_with_sub_attrs(
        testclass::access_public_static(),
        m_name,
        m_desc,
        1,
        1,
        &[0xB1],
        &[&lnt],
        code_name,
    );
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[], &[&m], &[]);
    let err = ClassFile::parse(&bytes).unwrap_err();
    assert!(
        err.message.contains("trailing") || err.message.contains("unexpected"),
        "{}",
        err.message
    );
}

// --- the G3.3 signature-pin layer: Signature + LocalVariableTypeTable

/// Encodes one field with arbitrary attributes (the test counterpart
/// of javac emitting a Signature attribute next to ConstantValue).
fn field_with_attrs(access: u16, name_idx: u16, desc_idx: u16, attrs: &[&[u8]]) -> Vec<u8> {
    let mut f = Vec::new();
    f.extend_from_slice(&access.to_be_bytes());
    f.extend_from_slice(&name_idx.to_be_bytes());
    f.extend_from_slice(&desc_idx.to_be_bytes());
    f.extend_from_slice(&(attrs.len() as u16).to_be_bytes());
    for a in attrs {
        f.extend_from_slice(a);
    }
    f
}

/// Encodes `name u2; len u4; body` — one attribute_info.
fn attr(name_idx: u16, body: &[u8]) -> Vec<u8> {
    let mut a = Vec::new();
    a.extend_from_slice(&name_idx.to_be_bytes());
    a.extend_from_slice(&(body.len() as u32).to_be_bytes());
    a.extend_from_slice(body);
    a
}

#[test]
fn field_signature_attribute_parses() {
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let f_name = b.utf8("m");
    let f_desc = b.utf8("Ljava/util/Map;");
    let sig_str = b.utf8("Ljava/util/Map<Ljava/lang/String;Ljava/lang/Double;>;");
    let sig_name = b.utf8("Signature");
    let field = field_with_attrs(
        flags::ACC_PUBLIC,
        f_name,
        f_desc,
        &[&attr(sig_name, &sig_str.to_be_bytes())],
    );
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[&field], &[], &[]);
    let cf = parse_ok(&bytes);
    assert_eq!(
        cf.field("m").unwrap().signature.as_deref(),
        Some("Ljava/util/Map<Ljava/lang/String;Ljava/lang/Double;>;")
    );
}

#[test]
fn method_signature_attribute_parses() {
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let m_name = b.utf8("run");
    let m_desc = b.utf8("()V");
    let code_name = b.utf8("Code");
    let sig_str = b.utf8("(Ljava/util/Map<Ljava/lang/String;Ljava/lang/Double;>;)V");
    let sig_name = b.utf8("Signature");
    // Two method-level attributes: Code + Signature (hand-rolled —
    // `testclass::method` emits exactly one).
    let mut body = Vec::new();
    body.extend_from_slice(&1u16.to_be_bytes()); // max_stack
    body.extend_from_slice(&0u16.to_be_bytes()); // max_locals
    body.extend_from_slice(&1u32.to_be_bytes()); // code_length
    body.extend_from_slice(&[0xB1]); // return
    body.extend_from_slice(&0u16.to_be_bytes()); // exception_table_length
    body.extend_from_slice(&0u16.to_be_bytes()); // sub-attributes
    let mut m = Vec::new();
    m.extend_from_slice(&flags::ACC_PUBLIC.to_be_bytes());
    m.extend_from_slice(&m_name.to_be_bytes());
    m.extend_from_slice(&m_desc.to_be_bytes());
    m.extend_from_slice(&2u16.to_be_bytes()); // attributes_count
    m.extend_from_slice(&attr(code_name, &body));
    m.extend_from_slice(&attr(sig_name, &sig_str.to_be_bytes()));
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[], &[&m], &[]);
    let cf = parse_ok(&bytes);
    assert_eq!(
        cf.method("run", "()V").unwrap().signature.as_deref(),
        Some("(Ljava/util/Map<Ljava/lang/String;Ljava/lang/Double;>;)V")
    );
}

#[test]
fn class_signature_attribute_parses() {
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let sig_str =
        b.utf8("<K:Ljava/lang/Object;V:Ljava/lang/Object;>Ljava/util/AbstractMap<TK;TV;>;");
    let sig_name = b.utf8("Signature");
    let bytes = b.finish(
        flags::ACC_PUBLIC,
        cls,
        obj_cls,
        &[],
        &[],
        &[],
        &[&attr(sig_name, &sig_str.to_be_bytes())],
    );
    let cf = parse_ok(&bytes);
    assert_eq!(
        cf.signature.as_deref(),
        Some("<K:Ljava/lang/Object;V:Ljava/lang/Object;>Ljava/util/AbstractMap<TK;TV;>;")
    );
}

#[test]
fn field_signature_wrong_length_is_rejected() {
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let f_name = b.utf8("m");
    let f_desc = b.utf8("Ljava/util/Map;");
    let sig_name = b.utf8("Signature");
    // attribute_length claims 3 (must be 2) — the A4 law: a wrong
    // length would shift the reader onto the next attribute.
    let field = field_with_attrs(
        flags::ACC_PUBLIC,
        f_name,
        f_desc,
        &[&attr_len(sig_name, 3, &[0x00, 0x00, 0x00])],
    );
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[&field], &[], &[]);
    let err = ClassFile::parse(&bytes).unwrap_err();
    assert!(err.message.contains("Signature"), "{}", err.message);
}

/// `attr` with a deliberately wrong length (negative tests).
fn attr_len(name_idx: u16, len: u32, body: &[u8]) -> Vec<u8> {
    let mut a = Vec::new();
    a.extend_from_slice(&name_idx.to_be_bytes());
    a.extend_from_slice(&len.to_be_bytes());
    a.extend_from_slice(body);
    a
}

#[test]
fn local_variable_type_table_parses() {
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let m_name = b.utf8("run");
    let m_desc = b.utf8("()V");
    let code_name = b.utf8("Code");
    let lvtt_name = b.utf8("LocalVariableTypeTable");
    let lv_name = b.utf8("m");
    let lv_sig = b.utf8("Ljava/util/Map<Ljava/lang/String;Ljava/lang/Double;>;");
    // One LVTT entry: start_pc 0, length 1, name lv_name, sig lv_sig, slot 0.
    let mut lvtt_body = Vec::new();
    lvtt_body.extend_from_slice(&1u16.to_be_bytes());
    for v in [0u16, 1u16, lv_name, lv_sig, 0u16] {
        lvtt_body.extend_from_slice(&v.to_be_bytes());
    }
    let lvtt = attr(lvtt_name, &lvtt_body);
    let m = testclass::method_with_sub_attrs(
        flags::ACC_PUBLIC | flags::ACC_STATIC,
        m_name,
        m_desc,
        1,
        1,
        &[0xB1],
        &[&lvtt],
        code_name,
    );
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[], &[&m], &[]);
    let cf = parse_ok(&bytes);
    let code = cf.method("run", "()V").unwrap().code().unwrap();
    assert_eq!(code.lvtt.len(), 1);
    let e = &code.lvtt[0];
    assert_eq!(e.start_pc, 0);
    assert_eq!(e.length, 1);
    assert_eq!(e.name, "m");
    assert_eq!(
        e.signature,
        "Ljava/util/Map<Ljava/lang/String;Ljava/lang/Double;>;"
    );
    assert_eq!(e.index, 0);
}

#[test]
fn lvtt_trailing_bytes_are_rejected() {
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let m_name = b.utf8("run");
    let m_desc = b.utf8("()V");
    let code_name = b.utf8("Code");
    let lvtt_name = b.utf8("LocalVariableTypeTable");
    // count=0 but 2 stray trailing bytes — must be a precise error.
    let mut lvtt_body = Vec::new();
    lvtt_body.extend_from_slice(&0u16.to_be_bytes());
    lvtt_body.extend_from_slice(&[0xAA, 0xBB]);
    let lvtt = attr(lvtt_name, &lvtt_body);
    let m = testclass::method_with_sub_attrs(
        flags::ACC_PUBLIC | flags::ACC_STATIC,
        m_name,
        m_desc,
        1,
        1,
        &[0xB1],
        &[&lvtt],
        code_name,
    );
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[], &[&m], &[]);
    let err = ClassFile::parse(&bytes).unwrap_err();
    assert!(
        err.message.contains("LocalVariableTypeTable"),
        "{}",
        err.message
    );
}

#[test]
fn no_signature_attributes_means_none() {
    let mut b = Builder::new();
    let name = b.utf8("T");
    let cls = b.class(name);
    let obj_name = b.utf8(OBJ_NAME);
    let obj_cls = b.class(obj_name);
    let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[], &[], &[]);
    let cf = parse_ok(&bytes);
    assert_eq!(cf.signature, None);
    assert!(cf.fields.iter().all(|f| f.signature.is_none()));
    assert!(cf.methods.iter().all(|m| m.signature.is_none()));
}
