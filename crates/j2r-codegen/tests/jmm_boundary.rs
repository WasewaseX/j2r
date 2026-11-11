//! G4.2 arm (b): the JMM boundary is ENFORCED, not just documented. A
//! hand-assembled class carries one `ACC_VOLATILE` static, one
//! `ACC_FINAL` instance field (no ConstantValue), one static final WITH
//! a ConstantValue, and one plain field. The emitted crate must (a)
//! list exactly the volatile + non-folded-final fields in the
//! `src/jmm_boundary.rs` sidecar, (b) carry the boundary marker comment
//! at every generated access to those fields, and (c) carry NO marker
//! for the plain field or the folded constant.

mod common;

use common::builder::{method, Builder, ACC_PUBLIC, ACC_STATIC};

/// One field_info entry: access(2) name_idx(2) desc_idx(2) attrs.
fn field(access: u16, name_idx: u16, desc_idx: u16, attrs: &[&[u8]]) -> Vec<u8> {
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

/// One attribute: name_idx(2) length(4) bytes.
fn attr(name_idx: u16, bytes: &[u8]) -> Vec<u8> {
    let mut a = Vec::new();
    a.extend_from_slice(&name_idx.to_be_bytes());
    a.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    a.extend_from_slice(bytes);
    a
}

#[test]
fn jmm_boundary_markers_enforced_in_emitted_crate() {
    const ACC_VOLATILE: u16 = 0x0040;
    const ACC_FINAL: u16 = 0x0010;

    let mut b = Builder::new();
    let cn = b.utf8("JmmFix");
    let this = b.class(cn);
    let on = b.utf8("java/lang/Object");
    let super_cls = b.class(on);
    let obj_utf = b.utf8("java/lang/Object");
    let obj_cls = b.class(obj_utf);
    let init_n = b.utf8("<init>");
    let init_d = b.utf8("()V");
    let obj_nat = b.name_and_type(init_n, init_d);
    let obj_init = b.method_ref(obj_cls, obj_nat);

    let vflag_n = b.utf8("vflag");
    let i_d = b.utf8("I");
    let ffinal_n = b.utf8("ffinal");
    let plain_n = b.utf8("plain");
    let k_n = b.utf8("k");
    let self_nat_v = b.name_and_type(vflag_n, i_d);
    let self_nat_i = b.name_and_type(ffinal_n, i_d);
    let self_nat_p = b.name_and_type(plain_n, i_d);
    let self_ref_v = b.field_ref(this, self_nat_v);
    let self_ref_i = b.field_ref(this, self_nat_i);
    let self_ref_p = b.field_ref(this, self_nat_p);

    // Constant pool entry for the folded constant: Integer 42 (tag 3).
    // `pool` and `idx` are pub — emulate `take()` by hand.
    let int_idx = b.idx;
    b.pool.push(3);
    b.pool.extend_from_slice(&42i32.to_be_bytes());
    b.idx += 1;
    let cv_n = b.utf8("ConstantValue");
    let cv_attr = attr(cv_n, &int_idx.to_be_bytes());

    // fields: vflag = public static volatile I; ffinal = public final I;
    // plain = public I; k = public static final I WITH ConstantValue.
    let fields: Vec<Vec<u8>> = vec![
        field(ACC_PUBLIC | ACC_STATIC | ACC_VOLATILE, vflag_n, i_d, &[]),
        field(ACC_PUBLIC | ACC_FINAL, ffinal_n, i_d, &[]),
        field(ACC_PUBLIC, plain_n, i_d, &[]),
        field(ACC_PUBLIC | ACC_STATIC | ACC_FINAL, k_n, i_d, &[&cv_attr]),
    ];
    let field_refs: Vec<&[u8]> = fields.iter().map(|f| f.as_slice()).collect();

    // <init>: aload_0; invokespecial Object.<init>; aload_0; iconst_5;
    // putfield ffinal; return
    let ctor_code = [
        0x2A,
        0xB7,
        ((obj_init >> 8) as u8),
        (obj_init as u8),
        0x2A,
        0x06,
        0xB5,
        ((self_ref_i >> 8) as u8),
        (self_ref_i as u8),
        0xB1,
    ];
    let ctor = method(
        ACC_PUBLIC,
        init_n,
        init_d,
        2,
        1,
        &ctor_code,
        &[],
        b.utf8("Code"),
    );

    // run(): new JmmFix; dup; invokespecial <init>; astore_1;
    //   aload_1; iconst_3; putfield plain;     <- plain: NO marker
    //   aload_1; getfield plain; pop;
    //   aload_1; getfield ffinal; pop;         <- final: marker
    //   getstatic vflag; pop;                  <- volatile: marker
    //   getstatic vflag; pop;                  <- volatile: marker (again)
    //   return
    let run_n = b.utf8("run");
    let run_d = b.utf8("()V");
    let code_n = b.utf8("Code");
    let self_new = b.class(cn); // class ref for `new`
    let _ = self_new;
    let run_code: [u8; 32] = [
        0xBB,
        ((this >> 8) as u8),
        (this as u8), // new JmmFix
        0x59,         // dup
        0xB7,
        ((obj_init >> 8) as u8),
        (obj_init as u8), // invokespecial
        0x4C,             // astore_1
        0x2B,
        0x06,
        0xB5,
        ((self_ref_p >> 8) as u8),
        (self_ref_p as u8), // putfield plain
        0x2B,
        0xB4,
        ((self_ref_p >> 8) as u8),
        (self_ref_p as u8),
        0x57, // getfield plain
        0x2B,
        0xB4,
        ((self_ref_i >> 8) as u8),
        (self_ref_i as u8),
        0x57, // getfield ffinal
        0xB2,
        ((self_ref_v >> 8) as u8),
        (self_ref_v as u8),
        0x57, // getstatic vflag
        0xB2,
        ((self_ref_v >> 8) as u8),
        (self_ref_v as u8),
        0x57, // getstatic vflag
        0xB1, // return
    ];
    let run_code: Vec<u8> = run_code.to_vec();
    let run = method(
        ACC_PUBLIC | ACC_STATIC,
        run_n,
        run_d,
        3,
        2,
        &run_code,
        &[],
        code_n,
    );

    let bytes = b.finish(
        ACC_PUBLIC,
        this,
        super_cls,
        &[],
        &field_refs,
        &[&ctor, &run],
        &[],
    );

    let cf = j2r_classfile::ClassFile::parse(&bytes).expect("handcrafted class parses");
    assert_eq!(cf.this_class, "JmmFix");

    let dir = std::env::temp_dir().join(format!(
        "j2r-jmm-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let class_path = dir.join("JmmFix.class");
    std::fs::write(&class_path, &bytes).unwrap();

    let opts = j2r_codegen::Options {
        classpath: Vec::new(),
        inputs: vec![class_path],
        main_class: None,
        crate_name: "j2r-jmm-app".to_string(),
        out_dir: dir.join("out"),
        emit_cfg: false,
        emit_typed: false,
        partition: None,
    };
    j2r_codegen::compile_to_crate(&opts).expect("codegen succeeds");

    // (a) the sidecar lists EXACTLY vflag (volatile) + ffinal (final) —
    // the folded constant `k` and the plain field must be absent.
    let sidecar = std::fs::read_to_string(dir.join("out/src/jmm_boundary.rs")).unwrap();
    assert!(
        sidecar.contains("(\"JmmFix\", \"vflag\", \"volatile\")"),
        "sidecar: {sidecar}"
    );
    assert!(
        sidecar.contains("(\"JmmFix\", \"ffinal\", \"final\")"),
        "sidecar: {sidecar}"
    );
    assert!(!sidecar.contains("\"plain\""), "sidecar: {sidecar}");
    assert!(!sidecar.contains("\"k\""), "sidecar: {sidecar}");

    // (b) every boundary access carries the marker; (c) the plain field's
    // accesses carry none.
    let jmmfix_rs = std::fs::read_to_string(dir.join("out/src/classes/JmmFix.rs")).unwrap();
    let marker_count = |needle: &str| jmmfix_rs.matches(needle).count();
    let volatile_marker = "J2R-JMM-BOUNDARY: JmmFix.vflag is volatile";
    let final_marker = "J2R-JMM-BOUNDARY: JmmFix.ffinal is final";
    // vflag: 2× getstatic in run() + the reflection-table row is a
    // different file; ffinal: putfield in <init> + getfield in run().
    assert_eq!(
        marker_count(volatile_marker),
        2,
        "volatile accesses must each carry the marker"
    );
    assert_eq!(
        marker_count(final_marker),
        2,
        "final accesses must each carry the marker"
    );
    assert!(
        !jmmfix_rs.contains("J2R-JMM-BOUNDARY: JmmFix.plain"),
        "plain field must carry NO marker"
    );
    assert!(
        !jmmfix_rs.contains("J2R-JMM-BOUNDARY: JmmFix.k"),
        "folded constant must carry NO marker"
    );

    // The registry rows carry the marker too (the reflection-table site).
    let registry = std::fs::read_to_string(dir.join("out/src/registry.rs")).unwrap();
    assert!(
        registry.contains(volatile_marker),
        "registry row marker: {registry}"
    );
    assert!(
        registry.contains(final_marker),
        "registry row marker: {registry}"
    );
    assert!(!registry.contains("J2R-JMM-BOUNDARY: JmmFix.plain"));

    std::fs::remove_dir_all(&dir).ok();
}
