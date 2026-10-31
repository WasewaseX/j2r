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

#[test]
fn well_formed_code_still_compiles() {
    // sanity: iconst_1 iconst_1 iadd pop return — must succeed
    let dir = fresh_dir("goodcode");
    let res = compile_main(&dir, &[0x04, 0x04, 0x60, 0x57, 0xb1], 2, 1);
    let _ = std::fs::remove_dir_all(&dir);
    res.expect("well-formed bytecode must compile");
}
