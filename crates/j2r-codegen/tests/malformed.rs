//! End-to-end verification tests: hand-assembled **malformed** class files
//! must be rejected by `compile_to_crate` with structured, pc-annotated
//! diagnostics — never a panic, never a miscompile. These exercise the
//! full pipeline (parse → decode → verify) on hostile input.

mod common;

use common::builder::{method, Builder, ACC_PUBLIC, ACC_STATIC};

/// Builds a class `T` whose `main` body is the raw `code` bytes, writes it
/// to `dir/T.class`, and returns the compile result.
fn compile_main(
    dir: &std::path::Path,
    code: &[u8],
    max_stack: u16,
    max_locals: u16,
) -> Result<(), j2r_codegen::CodegenError> {
    let mut b = Builder::new();
    let cn = b.utf8("T");
    let this = b.class(cn);
    let on = b.utf8("java/lang/Object");
    let super_cls = b.class(on);
    let main_n = b.utf8("main");
    let main_d = b.utf8("([Ljava/lang/String;)V");
    let code_n = b.utf8("Code");
    let main = method(
        ACC_PUBLIC | ACC_STATIC,
        main_n,
        main_d,
        max_stack,
        max_locals,
        code,
        &[],
        code_n,
    );
    let bytes = b.finish(ACC_PUBLIC, this, super_cls, &[], &[], &[&main], &[]);

    let class_path = dir.join("T.class");
    std::fs::write(&class_path, &bytes).unwrap();
    let opts = j2r_codegen::Options {
        inputs: vec![class_path],
        main_class: None,
        crate_name: "j2r-verify-app".to_string(),
        out_dir: dir.join("out"),
    };
    j2r_codegen::compile_to_crate(&opts).map(|_| ())
}

fn fresh_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "j2r-verify-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn expect_verify_error(tag: &str, code: &[u8], max_stack: u16, max_locals: u16, needle: &str) {
    let dir = fresh_dir(tag);
    let res = compile_main(&dir, code, max_stack, max_locals);
    let _ = std::fs::remove_dir_all(&dir);
    let err = res.expect_err("malformed bytecode must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("verification error"),
        "expected a Verify diagnostic, got: {msg}"
    );
    assert!(
        msg.contains(needle),
        "expected message containing {needle:?}, got: {msg}"
    );
}

#[test]
fn pop_on_empty_stack_is_a_compile_error() {
    // pop (0x57) then return
    expect_verify_error("popempty", &[0x57, 0xb1], 1, 1, "underflow");
}

#[test]
fn branch_into_middle_of_an_instruction_is_a_compile_error() {
    // goto +1 (target pc 2, which is the middle of the 3-byte goto at 1)
    // 0: nop; 1: goto -> 4 (3 bytes: 1..4); ... target 2 is not a boundary
    expect_verify_error(
        "midbranch",
        &[0x00, 0xa7, 0x00, 0x01, 0xb1],
        1,
        1,
        "boundary",
    );
}

#[test]
fn load_from_out_of_range_local_is_a_compile_error() {
    // aload 7 with max_locals 0
    expect_verify_error("badlocal", &[0x19, 0x07, 0x57, 0xb1], 2, 1, "out of range");
}

#[test]
fn swap_of_long_is_a_compile_error() {
    // lconst_0 (0x09); iconst_1 (0x04); swap (0x5f); return
    expect_verify_error("swaplong", &[0x09, 0x04, 0x5f, 0xb1], 3, 1, "category-1");
}

#[test]
fn iadd_on_a_reference_is_a_compile_error() {
    // aconst_null (0x01); iconst_1 (0x04); iadd (0x60); ireturn (0xac)
    expect_verify_error("addref", &[0x01, 0x04, 0x60, 0xac], 2, 1, "expected Int");
}

#[test]
fn lastore_with_int_value_is_a_compile_error() {
    // iconst_1 (0x03); newarray long (0xbc 0x0b); iconst_0 (0x03);
    // iconst_5 (0x04)  [wrong: int value for a long[]]
    // lastore (0x50); return
    expect_verify_error(
        "lastoreint",
        &[0x03, 0xbc, 0x0b, 0x03, 0x04, 0x50, 0xb1],
        5,
        1,
        "expected Long",
    );
}

#[test]
fn falling_off_the_end_is_a_compile_error() {
    // bipush 1 (0x10 0x01); pop (0x57) — no return
    expect_verify_error("fallsoff", &[0x10, 0x01, 0x57], 2, 1, "falls off");
}

/// Assembles a class whose main runs `multianewarray` on the CP class
/// named `desc` with the given dims byte, preceded by `stack_depth` ints.
fn multianewarray_code(b: &mut Builder, desc: &str, dims: u8, stack_depth: u8) -> Vec<u8> {
    let desc_idx = b.utf8(desc);
    let desc_cls = b.class(desc_idx);
    let mut code = vec![0x04; stack_depth as usize]; // iconst_1 x stack_depth
    code.push(0xc5);
    code.extend_from_slice(&desc_cls.to_be_bytes());
    code.push(dims);
    code.push(0x57); // pop the result
    code.push(0xb1); // return
    code
}

#[test]
fn multianewarray_zero_dims_is_a_compile_error() {
    let dir = fresh_dir("multi0");
    let mut b = Builder::new();
    let cn = b.utf8("T");
    let this = b.class(cn);
    let obj_n = b.utf8("java/lang/Object");
    let super_cls = b.class(obj_n);
    let main_n = b.utf8("main");
    let main_d = b.utf8("([Ljava/lang/String;)V");
    let code_n = b.utf8("Code");
    let code = multianewarray_code(&mut b, "[[I", 0, 0);
    let main = method(
        ACC_PUBLIC | ACC_STATIC,
        main_n,
        main_d,
        2,
        1,
        &code,
        &[],
        code_n,
    );
    let bytes = b.finish(ACC_PUBLIC, this, super_cls, &[], &[], &[&main], &[]);
    let path = dir.join("T.class");
    std::fs::write(&path, &bytes).unwrap();
    let res = j2r_codegen::compile_to_crate(&j2r_codegen::Options {
        inputs: vec![path],
        main_class: None,
        crate_name: "j2r-multi0".to_string(),
        out_dir: dir.join("out"),
    });
    let _ = std::fs::remove_dir_all(&dir);
    let msg = res
        .expect_err("zero-dim multianewarray must be rejected")
        .to_string();
    assert!(msg.contains("verification error"), "{msg}");
    assert!(msg.contains("zero dimensions"), "{msg}");
}

#[test]
fn multianewarray_more_dims_than_descriptor_is_a_compile_error() {
    let dir = fresh_dir("multidepth");
    let mut b = Builder::new();
    let cn = b.utf8("T");
    let this = b.class(cn);
    let obj_n = b.utf8("java/lang/Object");
    let super_cls = b.class(obj_n);
    let main_n = b.utf8("main");
    let main_d = b.utf8("([Ljava/lang/String;)V");
    let code_n = b.utf8("Code");
    // `[I` has one array level but the instruction consumes 2 sizes
    let code = multianewarray_code(&mut b, "[I", 2, 2);
    let main = method(
        ACC_PUBLIC | ACC_STATIC,
        main_n,
        main_d,
        3,
        1,
        &code,
        &[],
        code_n,
    );
    let bytes = b.finish(ACC_PUBLIC, this, super_cls, &[], &[], &[&main], &[]);
    let path = dir.join("T.class");
    std::fs::write(&path, &bytes).unwrap();
    let res = j2r_codegen::compile_to_crate(&j2r_codegen::Options {
        inputs: vec![path],
        main_class: None,
        crate_name: "j2r-multidepth".to_string(),
        out_dir: dir.join("out"),
    });
    let _ = std::fs::remove_dir_all(&dir);
    let msg = res
        .expect_err("dims beyond descriptor depth must be rejected")
        .to_string();
    assert!(msg.contains("verification error"), "{msg}");
    assert!(msg.contains("array levels"), "{msg}");
}

#[test]
fn multianewarray_on_a_non_array_class_is_a_compile_error() {
    let dir = fresh_dir("multicls");
    let mut b = Builder::new();
    let cn = b.utf8("T");
    let this = b.class(cn);
    let obj_n = b.utf8("java/lang/Object");
    let super_cls = b.class(obj_n);
    let main_n = b.utf8("main");
    let main_d = b.utf8("([Ljava/lang/String;)V");
    let code_n = b.utf8("Code");
    let code = multianewarray_code(&mut b, "java/lang/Object", 1, 1);
    let main = method(
        ACC_PUBLIC | ACC_STATIC,
        main_n,
        main_d,
        2,
        1,
        &code,
        &[],
        code_n,
    );
    let bytes = b.finish(ACC_PUBLIC, this, super_cls, &[], &[], &[&main], &[]);
    let path = dir.join("T.class");
    std::fs::write(&path, &bytes).unwrap();
    let res = j2r_codegen::compile_to_crate(&j2r_codegen::Options {
        inputs: vec![path],
        main_class: None,
        crate_name: "j2r-multicls".to_string(),
        out_dir: dir.join("out"),
    });
    let _ = std::fs::remove_dir_all(&dir);
    let msg = res
        .expect_err("multianewarray on a class must be rejected")
        .to_string();
    assert!(msg.contains("verification error"), "{msg}");
    assert!(msg.contains("not an array class"), "{msg}");
}

#[test]
fn well_formed_multianewarray_still_compiles() {
    // sanity: iconst_1, iconst_2, multianewarray [[I 2, pop, return
    let dir = fresh_dir("multigood");
    let mut b = Builder::new();
    let cn = b.utf8("T");
    let this = b.class(cn);
    let obj_n = b.utf8("java/lang/Object");
    let super_cls = b.class(obj_n);
    let main_n = b.utf8("main");
    let main_d = b.utf8("([Ljava/lang/String;)V");
    let code_n = b.utf8("Code");
    let code = multianewarray_code(&mut b, "[[I", 2, 2);
    let main = method(
        ACC_PUBLIC | ACC_STATIC,
        main_n,
        main_d,
        3,
        1,
        &code,
        &[],
        code_n,
    );
    let bytes = b.finish(ACC_PUBLIC, this, super_cls, &[], &[], &[&main], &[]);
    let path = dir.join("T.class");
    std::fs::write(&path, &bytes).unwrap();
    let res = j2r_codegen::compile_to_crate(&j2r_codegen::Options {
        inputs: vec![path],
        main_class: None,
        crate_name: "j2r-multigood".to_string(),
        out_dir: dir.join("out"),
    });
    let _ = std::fs::remove_dir_all(&dir);
    res.expect("well-formed multianewarray must compile");
}

#[test]
fn well_formed_code_still_compiles() {
    // sanity: iconst_1 iconst_1 iadd pop return — must succeed
    let dir = fresh_dir("goodcode");
    let res = compile_main(&dir, &[0x04, 0x04, 0x60, 0x57, 0xb1], 2, 1);
    let _ = std::fs::remove_dir_all(&dir);
    res.expect("well-formed bytecode must compile");
}
