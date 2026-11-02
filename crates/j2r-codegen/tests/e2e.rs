//! End-to-end test that needs no JDK: a `Hello` class is hand-assembled as
//! raw class-file bytes, compiled to a Rust crate, built with cargo, and the
//! binary's stdout must match the JVM's semantics.

mod common;

use common::builder::{method, Builder, ACC_PUBLIC, ACC_STATIC};
use std::process::Command;

#[test]
fn handcrafted_class_runs_natively() {
    // class Hello extends Object with public static void main(String[]):
    //   getstatic System.out
    //   ldc "Hello, J2R!"
    //   invokevirtual PrintStream.println(String)
    //   getstatic System.out
    //   bipush 42
    //   invokevirtual PrintStream.println(I)
    //   return
    let mut b = Builder::new();
    let cn = b.utf8("Hello");
    let this = b.class(cn);
    let on = b.utf8("java/lang/Object");
    let super_cls = b.class(on);
    let obj_utf = b.utf8("java/lang/Object");
    let obj_cls = b.class(obj_utf);
    let init_n = b.utf8("<init>");
    let init_d = b.utf8("()V");
    let obj_nat = b.name_and_type(init_n, init_d);
    let obj_init = b.method_ref(obj_cls, obj_nat);
    let ps_utf = b.utf8("java/io/PrintStream");
    let ps_cls = b.class(ps_utf);
    let out_n = b.utf8("out");
    let ps_desc = b.utf8("Ljava/io/PrintStream;");
    let out_nat = b.name_and_type(out_n, ps_desc);
    let out_ref = b.field_ref(ps_cls, out_nat);
    let sys_utf = b.utf8("java/lang/System");
    let sys_cls = b.class(sys_utf);
    let sys_nat = b.name_and_type(out_n, ps_desc);
    let sys_out = b.field_ref(sys_cls, sys_nat);
    let msg_utf = b.utf8("Hello, J2R!");
    let msg_str = b.string(msg_utf);
    let pln_n = b.utf8("println");
    let pln_s_desc = b.utf8("(Ljava/lang/String;)V");
    let pln_s_nat = b.name_and_type(pln_n, pln_s_desc);
    let pln_s = b.method_ref(ps_cls, pln_s_nat);
    let pln_i_desc = b.utf8("(I)V");
    let pln_i_nat = b.name_and_type(pln_n, pln_i_desc);
    let pln_i = b.method_ref(ps_cls, pln_i_nat);
    let main_n = b.utf8("main");
    let main_d = b.utf8("([Ljava/lang/String;)V");
    let code_n = b.utf8("Code");
    let ctor = method(
        ACC_PUBLIC,
        init_n,
        init_d,
        1,
        1,
        &[0x2A, 0xB7, ((obj_init >> 8) as u8), (obj_init as u8), 0xB1],
        &[],
        code_n,
    );
    let main = method(
        ACC_PUBLIC | ACC_STATIC,
        main_n,
        main_d,
        4,
        1,
        &[
            0xBB,
            ((obj_cls >> 8) as u8),
            (obj_cls as u8), // new Object
            0x59,            // dup
            0xB7,
            ((obj_init >> 8) as u8),
            (obj_init as u8), // invokespecial Object.<init>
            0x57,             // pop
            0xB2,
            ((sys_out >> 8) as u8),
            (sys_out as u8), // getstatic System.out
            0x12,
            msg_str as u8, // ldc "Hello, J2R!"
            0xB6,
            ((pln_s >> 8) as u8),
            (pln_s as u8), // invokevirtual println(String)
            0xB2,
            ((sys_out >> 8) as u8),
            (sys_out as u8), // getstatic System.out
            0x10,
            42, // bipush 42
            0xB6,
            ((pln_i >> 8) as u8),
            (pln_i as u8), // invokevirtual println(I)
            0xB1,          // return
        ],
        &[],
        code_n,
    );
    // the out_ref entry keeps the pool index used for javap-style reference;
    // silence unused warning by referencing it
    let _ = out_ref;
    let bytes = b.finish(ACC_PUBLIC, this, super_cls, &[], &[], &[&ctor, &main], &[]);

    // parse sanity (through the classfile crate)
    let cf = j2r_classfile::ClassFile::parse(&bytes).expect("handcrafted class parses");
    assert_eq!(cf.this_class, "Hello");

    // compile to a crate
    let dir = std::env::temp_dir().join(format!(
        "j2r-e2e-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let class_path = dir.join("Hello.class");
    std::fs::write(&class_path, &bytes).unwrap();

    let opts = j2r_codegen::Options {
        classpath: Vec::new(),
        inputs: vec![class_path],
        main_class: None,
        crate_name: "j2r-e2e-app".to_string(),
        out_dir: dir.join("out"),
        emit_cfg: false,
    };
    j2r_codegen::compile_to_crate(&opts).expect("codegen succeeds");

    // Wave-1 emitted-shape check (M2_PLAN §3.2): every generated method
    // registers a FrameGuard as its first statement and turns a refused
    // registration into an immediate `return Err(..)`.
    let hello_rs = std::fs::read_to_string(dir.join("out/src/classes/Hello.rs")).unwrap();
    assert!(
        hello_rs.contains("::j2r_runtime::frame::FrameGuard::enter()"),
        "generated method must enter a FrameGuard first"
    );
    assert!(
        hello_rs
            .contains("::core::result::Result::Err(t) => return ::core::result::Result::Err(t),"),
        "a refused frame registration must return Err immediately"
    );
    let main_rs = std::fs::read_to_string(dir.join("out/src/main.rs")).unwrap();
    assert!(
        main_rs.contains("validate_env_or_exit"),
        "generated main must validate J2R_MAX_JAVA_FRAMES before Java code runs"
    );
    assert!(
        main_rs.contains("set_statics_scanner"),
        "generated main must register the statics root scanner (M2_PLAN §4.4)"
    );
    // Wave-2 collection-point shape (M2_PLAN §4.3/§4.8g): refresh precedes
    // every call boundary and every allocating opcode.
    assert!(
        hello_rs.contains("__frame.refresh(&locals, &stack, &thrown);"),
        "generated call sites must refresh the root snapshot"
    );
    assert!(
        hello_rs.contains("::j2r_runtime::heap::gc_point();"),
        "allocating opcodes must emit a gc_point"
    );
    let statics_rs = std::fs::read_to_string(dir.join("out/src/statics.rs")).unwrap();
    assert!(
        statics_rs.contains("pub fn gc_scan_roots"),
        "statics template must expose the root scanner"
    );

    // build
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(&cargo)
        .arg("build")
        .arg("--manifest-path")
        .arg(dir.join("out/Cargo.toml"))
        .arg("--quiet")
        .status()
        .expect("cargo runs");
    assert!(status.success(), "generated crate must build");

    // run
    let out = Command::new(dir.join("out/target/debug/j2r_e2e_app"))
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(
        stdout,
        "Hello, J2R!\n42\n",
        "unexpected stdout; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));

    let _ = std::fs::remove_dir_all(&dir);
}
