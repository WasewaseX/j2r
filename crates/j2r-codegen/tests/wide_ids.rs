//! Wide-identifier regression tests: J2R's internal class ids and
//! static-field pool slots are `u32`, so programs with far more than
//! 65,535 static fields must compile and run correctly.
//!
//! The JVM caps a single class at 65,535 fields and 65,535 constant-pool
//! entries, so the >65,535 pool is built from **two synthetic class files**
//! with 40,000 static fields each (40,000 < 65,535 per-class limits; the
//! combined pool is 80,000 slots). The main method reads slots 0 and
//! 76,543 — crossing the 65,535 boundary that used to wrap a `u16`.
//!
//! This test would fail (debug overflow panic) or miscompile on the
//! pre-hardening `u16` slot allocator.

mod common;

use common::builder::{method, Builder, ACC_PUBLIC, ACC_STATIC};

const PER_CLASS: usize = 40_000;

/// Assembles a class with `n` static int fields named `f0..f(n-1)` and, for
/// the "main" variant, a main that prints slot 0 of `Wide` and slot 76,543
/// (`WideB.f36543`, since `WideB` slots start at 40,000).
fn build_class(name: &str, n: usize, with_main: bool, other: Option<&str>) -> Vec<u8> {
    let mut b = Builder::new();
    let cn = b.utf8(name);
    let this = b.class(cn);
    let on = b.utf8("java/lang/Object");
    let super_cls = b.class(on);

    let int_desc = b.utf8("I");

    // unique names f0..f(n-1)
    let mut name_idx = Vec::with_capacity(n);
    for i in 0..n {
        name_idx.push(b.utf8(&format!("f{i}")));
    }

    // field_info: access_flags(2) name_idx(2) desc_idx(2) attributes_count(2)
    let field_access = ACC_PUBLIC | ACC_STATIC;
    let mut fields: Vec<Vec<u8>> = Vec::with_capacity(n);
    for ni in &name_idx {
        let mut f = Vec::with_capacity(8);
        f.extend_from_slice(&field_access.to_be_bytes());
        f.extend_from_slice(&ni.to_be_bytes());
        f.extend_from_slice(&int_desc.to_be_bytes());
        f.extend_from_slice(&0u16.to_be_bytes());
        fields.push(f);
    }

    let mut methods: Vec<Vec<u8>> = Vec::new();
    if with_main {
        let main_n = b.utf8("main");
        let main_d = b.utf8("([Ljava/lang/String;)V");
        let code_n = b.utf8("Code");
        // constant pool plumbing for System.out + println(I)
        let ps_utf = b.utf8("java/io/PrintStream");
        let ps_cls = b.class(ps_utf);
        let out_n = b.utf8("out");
        let ps_desc = b.utf8("Ljava/io/PrintStream;");
        let _out_nat = b.name_and_type(out_n, ps_desc);
        let sys_utf = b.utf8("java/lang/System");
        let sys_cls = b.class(sys_utf);
        let sys_nat = b.name_and_type(out_n, ps_desc);
        let sys_out = b.field_ref(sys_cls, sys_nat);
        let pln_n = b.utf8("println");
        let pln_i_desc = b.utf8("(I)V");
        let pln_i_nat = b.name_and_type(pln_n, pln_i_desc);
        let pln_i = b.method_ref(ps_cls, pln_i_nat);
        // field refs: slot 0 of this class, slot 76543 of `other`
        let self_nat = b.name_and_type(name_idx[0], int_desc);
        let self_ref = b.field_ref(this, self_nat);
        let other_name = other.expect("main variant needs the other class");
        let other_utf = b.utf8(other_name);
        let other_cls = b.class(other_utf);
        // the other class's f36543 (slot 40000 + 36543 = 76543)
        let other_f36543 = b.utf8(&format!("f{}", 76_543 - PER_CLASS));
        let other_nat = b.name_and_type(other_f36543, int_desc);
        let other_ref = b.field_ref(other_cls, other_nat);

        let code: Vec<u8> = [&[
            0xb2,
            (sys_out >> 8) as u8,
            sys_out as u8, // getstatic System.out
            0xb2,
            (self_ref >> 8) as u8,
            self_ref as u8, // getstatic Wide.f0 (slot 0)
            0xb6,
            (pln_i >> 8) as u8,
            pln_i as u8, // println(I)
            0xb2,
            (sys_out >> 8) as u8,
            sys_out as u8, // getstatic System.out
            0xb2,
            (other_ref >> 8) as u8,
            other_ref as u8, // getstatic WideB.f36543 (slot 76543)
            0xb6,
            (pln_i >> 8) as u8,
            pln_i as u8, // println(I)
            0xb1,        // return
        ][..]]
        .concat();
        methods.push(method(
            ACC_PUBLIC | ACC_STATIC,
            main_n,
            main_d,
            3,
            1,
            &code,
            &[],
            code_n,
        ));
    }

    let field_refs: Vec<&[u8]> = fields.iter().map(|f| f.as_slice()).collect();
    let method_refs: Vec<&[u8]> = methods.iter().map(|m| m.as_slice()).collect();
    b.finish(
        ACC_PUBLIC,
        this,
        super_cls,
        &[],
        &field_refs,
        &method_refs,
        &[],
    )
}

#[test]
fn static_pool_beyond_65535_slots_compiles_and_runs() {
    let dir = std::env::temp_dir().join(format!(
        "j2r-wide-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    let wide = build_class("Wide", PER_CLASS, true, Some("WideB"));
    let wide_b = build_class("WideB", PER_CLASS, false, None);
    let a = dir.join("Wide.class");
    let c = dir.join("WideB.class");
    std::fs::write(&a, &wide).unwrap();
    std::fs::write(&c, &wide_b).unwrap();

    // parse sanity
    let cf = j2r_classfile::ClassFile::parse(&wide).expect("Wide parses");
    assert_eq!(cf.fields.len(), PER_CLASS);

    let opts = j2r_codegen::Options {
        classpath: Vec::new(),
        inputs: vec![a.clone(), c.clone()],
        main_class: None,
        crate_name: "j2r-wide-app".to_string(),
        out_dir: dir.join("out"),
        emit_cfg: false,
        emit_typed: false,
        partition: None,
    };
    j2r_codegen::compile_to_crate(&opts).expect("80,000-slot program compiles");

    // the generated static pool must be u32-addressed and 80,000 entries
    let statics = std::fs::read_to_string(dir.join("out/src/statics.rs")).unwrap();
    assert!(
        statics.contains("pub fn get(idx: u32)"),
        "statics pool getter must take u32"
    );
    assert!(
        statics.contains("pub fn ensure(class_id: u32"),
        "class-init ensure must take u32"
    );
    assert!(
        !statics.contains("u16"),
        "no u16 may remain in the generated statics registry"
    );
    let main_rs = std::fs::read_to_string(dir.join("out/src/main.rs")).unwrap();
    assert!(
        main_rs.contains("init_pool(80000usize"),
        "pool must hold 80000 slots, got: {}",
        main_rs
            .lines()
            .find(|l| l.contains("init_pool"))
            .unwrap_or("<none>")
    );
    // slot 76543 (past the u16 boundary) must appear as a u32 literal
    let wide_mod = std::fs::read_to_string(dir.join("out/src/classes/Wide.rs")).unwrap();
    assert!(
        wide_mod.contains("statics::get(76543u32)"),
        "the >65535 slot must be a u32 literal"
    );

    // build the generated crate and run it: both statics are 0
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = std::process::Command::new(&cargo)
        .arg("build")
        .arg("--manifest-path")
        .arg(dir.join("out/Cargo.toml"))
        .arg("--quiet")
        .status()
        .expect("cargo runs");
    assert!(status.success(), "generated 80,000-slot crate must build");

    let out = std::process::Command::new(dir.join("out/target/debug/j2r_wide_app"))
        .output()
        .expect("binary runs");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "0\n0\n",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));

    let _ = std::fs::remove_dir_all(&dir);
}
