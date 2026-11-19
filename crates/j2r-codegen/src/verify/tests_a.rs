//! `verify`::tests_a — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use crate::ir::{ArithOp, Instr, NumKind};
use crate::MethodPlan;
use j2r_runtime::TypeDesc;
use std::collections::BTreeMap;

use crate::ir::*;

pub(crate) fn plan(
    desc: &str,
    is_static: bool,
    max_stack: usize,
    max_locals: usize,
    instrs: Vec<(u32, Instr)>,
) -> MethodPlan {
    MethodPlan {
        name: "t".to_string(),
        desc: desc.to_string(),
        is_static,
        is_native: false,
        is_sync: false,
        modifiers: 0,
        signature: None,
        lvtt: Vec::new(),
        array_ops: Default::default(),
        fn_name: "m_t".to_string(),
        instrs,
        exception_table: Vec::new(),
        max_locals,
        max_stack,
    }
}

pub(crate) fn verify_ok(m: &MethodPlan) {
    let concats = BTreeMap::new();
    let lambdas = BTreeMap::new();
    verify_method(m, &concats, &lambdas).unwrap_or_else(|e| panic!("expected OK, got: {e}"));
}

pub(crate) fn verify_err(m: &MethodPlan) -> VerifyError {
    let concats = BTreeMap::new();
    let lambdas = BTreeMap::new();
    verify_method(m, &concats, &lambdas).expect_err("expected a verification error")
}

#[test]
fn straight_line_add_is_valid() {
    // iconst_1 iconst_2 iadd ireturn — ()I
    let m = plan(
        "()I",
        true,
        2,
        0,
        vec![
            (0, Instr::IConst(1)),
            (1, Instr::IConst(2)),
            (
                2,
                Instr::Arith {
                    op: ArithOp::Add,
                    kind: NumKind::Int,
                },
            ),
            (3, Instr::Return(TypeDesc::Int)),
        ],
    );
    verify_ok(&m);
}

#[test]
fn pop_on_empty_stack_is_rejected() {
    let m = plan(
        "()V",
        true,
        1,
        0,
        vec![(0, Instr::Pop), (1, Instr::Return(TypeDesc::Void))],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("underflow"), "{e}");
}

#[test]
fn pop_of_long_is_rejected() {
    let m = plan(
        "()V",
        true,
        2,
        0,
        vec![
            (0, Instr::LConst(1)),
            (1, Instr::Pop),
            (2, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("category-1"), "{e}");
}

#[test]
fn iadd_with_one_operand_is_rejected() {
    // only one int on the stack — the second pop underflows
    let m = plan(
        "()I",
        true,
        2,
        0,
        vec![
            (0, Instr::IConst(1)),
            (
                1,
                Instr::Arith {
                    op: ArithOp::Add,
                    kind: NumKind::Int,
                },
            ),
            (2, Instr::Return(TypeDesc::Int)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("underflow"), "{e}");
}

#[test]
fn iadd_on_int_and_ref_is_rejected() {
    // one operand is an int, the other a reference — kind mismatch
    let m = plan(
        "()I",
        true,
        2,
        0,
        vec![
            (0, Instr::AConstNull),
            (1, Instr::IConst(1)),
            (
                2,
                Instr::Arith {
                    op: ArithOp::Add,
                    kind: NumKind::Int,
                },
            ),
            (3, Instr::Return(TypeDesc::Int)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("expected Int"), "{e}");
}

#[test]
fn swap_of_long_is_rejected() {
    // lconst_0 iconst_1 swap — swap requires two category-1 values
    let m = plan(
        "()V",
        true,
        3,
        0,
        vec![
            (0, Instr::LConst(0)),
            (1, Instr::IConst(1)),
            (2, Instr::Swap),
            (3, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("category-1"), "{e}");
}

fn dup2x2_case(instrs: Vec<(u32, Instr)>, max_stack: usize) -> MethodPlan {
    plan("()V", true, max_stack, 0, instrs)
}

#[test]
fn dup2_x2_form1_four_cat1() {
    let m = dup2x2_case(
        vec![
            (0, Instr::IConst(1)),
            (1, Instr::IConst(2)),
            (2, Instr::IConst(3)),
            (3, Instr::IConst(4)),
            (4, Instr::Dup2X2),
            (5, Instr::Pop),
            (6, Instr::Pop),
            (7, Instr::Pop),
            (8, Instr::Pop),
            (9, Instr::Pop),
            (10, Instr::Pop),
            (11, Instr::Return(TypeDesc::Void)),
        ],
        8,
    );
    verify_ok(&m);
}

#[test]
fn dup2_x2_form2_cat2_top_jama_compound_assignment() {
    // JAMA wave 3 shape: javac's compound double assignment
    // `LUrowi[j] = LUcolj[i] -= s;` ends with a cat-2 value on top of
    // two cat-1 values — JVMS form 2. Result [v1(cat2), v3, v2, v1]:
    // the two dastores that follow consume v1,i,arr then v1,j,arr2.
    let m = dup2x2_case(
        vec![
            (0, Instr::IConst(1)),
            (1, Instr::IConst(2)),
            (2, Instr::LConst(3)),
            (3, Instr::Dup2X2),
            (4, Instr::Pop2),
            (5, Instr::Pop),
            (6, Instr::Pop),
            (7, Instr::Pop2),
            (8, Instr::Return(TypeDesc::Void)),
        ],
        6,
    );
    verify_ok(&m);
}

#[test]
fn dup2_x2_form3_cat2_at_bottom() {
    // v3(cat2), v2, v1 -> v2, v1, v3, v2, v1
    let m = dup2x2_case(
        vec![
            (0, Instr::LConst(1)),
            (1, Instr::IConst(2)),
            (2, Instr::IConst(3)),
            (3, Instr::Dup2X2),
            (4, Instr::Pop),
            (5, Instr::Pop),
            (6, Instr::Pop2),
            (7, Instr::Pop),
            (8, Instr::Pop),
            (9, Instr::Return(TypeDesc::Void)),
        ],
        6,
    );
    verify_ok(&m);
}

#[test]
fn dup2_x2_form4_two_cat2() {
    let m = dup2x2_case(
        vec![
            (0, Instr::LConst(1)),
            (1, Instr::DConst(2.0)),
            (2, Instr::Dup2X2),
            (3, Instr::Pop2),
            (4, Instr::Pop2),
            (5, Instr::Pop2),
            (6, Instr::Return(TypeDesc::Void)),
        ],
        5,
    );
    verify_ok(&m);
}

#[test]
fn dup2_x2_cat1_on_top_of_cat2_is_invalid() {
    // no JVMS form has a category-1 value on top of a category-2 one
    // (the old verifier ACCEPTED this shape with a scrambled push
    // order — the JAMA corpus wave corrected the classification)
    let m = dup2x2_case(
        vec![
            (0, Instr::LConst(2)),
            (1, Instr::IConst(1)),
            (2, Instr::Dup2X2),
            (3, Instr::Return(TypeDesc::Void)),
        ],
        5,
    );
    let e = verify_err(&m);
    assert!(e.message.contains("no JVMS form"), "{e}");
}

#[test]
fn branch_into_middle_of_instruction_is_rejected() {
    // goto 2, but pc 2 is not an instruction start (instructions at 0,1,3)
    let m = plan(
        "()V",
        true,
        1,
        0,
        vec![
            (0, Instr::Goto(2)),
            (1, Instr::Nop),
            (3, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("not an instruction boundary"), "{e}");
}

#[test]
fn out_of_range_local_is_rejected() {
    let m = plan(
        "(I)V",
        true,
        1,
        1,
        vec![
            (0, Instr::Load(5)),
            (1, Instr::Pop),
            (2, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("out of range"), "{e}");
}

#[test]
fn loading_high_half_of_long_local_is_rejected() {
    // lstore_0; lload_1 — local 1 is the high half
    let m = plan(
        "()V",
        true,
        2,
        3,
        vec![
            (0, Instr::LConst(1)),
            (1, Instr::Store(0)),
            (2, Instr::Load(1)),
            (3, Instr::Pop),
            (4, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("high half"), "{e}");
}

#[test]
fn loading_never_assigned_local_is_rejected() {
    let m = plan(
        "()V",
        true,
        1,
        2,
        vec![
            (0, Instr::Load(1)),
            (1, Instr::Pop),
            (2, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("never-assigned"), "{e}");
}

#[test]
fn join_with_different_stack_heights_is_rejected() {
    // 0: iconst_1; 1: ifeq 4  (arm A enters 4 with an empty stack)
    // 2: iconst_1; 3: goto 4  (arm B enters 4 with one int)
    let m = plan(
        "()V",
        true,
        2,
        0,
        vec![
            (0, Instr::IConst(1)),
            (1, Instr::IfZero(CmpOp::Eq, 4)),
            (2, Instr::IConst(1)),
            (3, Instr::Goto(4)),
            (4, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("stack heights"), "{e}");
}

#[test]
fn join_with_conflicting_types_is_rejected() {
    // arm A (ifeq taken) pushes an int, arm B (fallthrough) a ref;
    // both join at the pop
    let m = plan(
        "()V",
        true,
        2,
        0,
        vec![
            (0, Instr::IConst(0)),
            (1, Instr::IfZero(CmpOp::Eq, 4)),
            (2, Instr::AConstNull),
            (3, Instr::Goto(5)),
            (4, Instr::IConst(1)),
            (5, Instr::Pop),
            (6, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("incompatible"), "{e}");
}

#[test]
fn locals_of_different_types_may_join_but_not_load() {
    // arm A stores an int in local 0, arm B a ref; both join; loading
    // the local afterwards is a verification error (poisoned)
    let m = plan(
        "(I)V",
        true,
        2,
        2,
        vec![
            (0, Instr::IConst(1)),
            (1, Instr::IfZero(CmpOp::Eq, 5)),
            (2, Instr::IConst(7)),
            (3, Instr::Store(1)),
            (4, Instr::Goto(7)),
            (5, Instr::AConstNull),
            (6, Instr::Store(1)),
            (7, Instr::Load(1)),
            (8, Instr::Pop),
            (9, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("conflicting types"), "{e}");
}

#[test]
fn return_type_must_match_descriptor() {
    // ireturn in a ()V method
    let m = plan(
        "()V",
        true,
        1,
        0,
        vec![(0, Instr::IConst(0)), (1, Instr::Return(TypeDesc::Int))],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("declared"), "{e}");
}

#[test]
fn areturn_of_int_is_rejected() {
    let m = plan(
        "()Ljava/lang/Object;",
        true,
        1,
        0,
        vec![
            (0, Instr::IConst(0)),
            (1, Instr::Return(TypeDesc::Object(String::new()))),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("reference"), "{e}");
}

#[test]
fn max_stack_overflow_is_rejected() {
    let m = plan(
        "()V",
        true,
        1,
        0,
        vec![
            (0, Instr::IConst(1)),
            (1, Instr::IConst(2)),
            (2, Instr::Pop),
            (3, Instr::Pop),
            (4, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("max_stack"), "{e}");
}

#[test]
fn handler_entry_state_is_one_ref() {
    // athrow; the handler (pc 3) starts with exactly the caught ref
    let mut m = plan(
        "()V",
        true,
        2,
        0,
        vec![
            (0, Instr::AConstNull),
            (1, Instr::AThrow),
            (2, Instr::Return(TypeDesc::Void)),
            (3, Instr::Pop),
            (4, Instr::Return(TypeDesc::Void)),
        ],
    );
    m.exception_table = vec![(0, 3, 3, Some("java/lang/Throwable".to_string()))];
    verify_ok(&m);
}

#[test]
fn handler_entry_with_wrong_shape_is_rejected() {
    // the handler starts with pop2, which needs two cat-1 values, but a
    // handler entry always has exactly ONE ref — malformed
    let mut m = plan(
        "()V",
        true,
        3,
        0,
        vec![
            (0, Instr::AConstNull),
            (1, Instr::AThrow),
            (2, Instr::Return(TypeDesc::Void)),
            (3, Instr::Pop2),
            (4, Instr::Return(TypeDesc::Void)),
        ],
    );
    m.exception_table = vec![(0, 3, 3, Some("java/lang/Throwable".to_string()))];
    let e = verify_err(&m);
    assert!(e.message.contains("underflow"), "{e}");
}

#[test]
fn unknown_catch_type_is_rejected() {
    use crate::{ClassPlan, Program};
    use j2r_classfile::{ClassFile, ConstantPool};
    let mut m = plan(
        "()V",
        true,
        2,
        0,
        vec![
            (0, Instr::AConstNull),
            (1, Instr::AThrow),
            (2, Instr::Return(TypeDesc::Void)),
            (3, Instr::Pop),
            (4, Instr::Return(TypeDesc::Void)),
        ],
    );
    m.exception_table = vec![(0, 3, 3, Some("com/example/Bogus".to_string()))];
    let cf = ClassFile {
        minor_version: 0,
        major_version: 52,
        constant_pool: ConstantPool::default(),
        access: j2r_classfile::flags::ACC_PUBLIC,
        this_class: "T".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        interfaces: Vec::new(),
        fields: Vec::new(),
        methods: Vec::new(),
        bootstrap_methods: Vec::new(),
        inner_name: None,
        signature: None,
        nest_host: None,
    };
    let c = ClassPlan {
        name: "T".to_string(),
        super_name: Some("java/lang/Object".to_string()),
        interfaces: Vec::new(),
        is_interface: false,
        nest_host: None,
        is_abstract: false,
        simple_name: None,
        module: "T".to_string(),
        id: 0,
        base_slots: 0,
        total_slots: 0,
        default_spec: String::new(),
        fields: Vec::new(),
        methods: vec![m],
        clinit_fn: None,
        concats: BTreeMap::new(),
        lambdas: BTreeMap::new(),
        static_monitor: None,
        refs: std::collections::BTreeSet::new(),
        input_sha: String::new(),
        cf,
    };
    let prog = Program {
        classes: Vec::new(), // T deliberately absent → catch type unknown
        user_index: BTreeMap::new(),
        total_statics: 0,
        main_class: None,
    };
    let e = crate::verify::verify_class(&prog, &c).expect_err("bogus catch type");
    match e {
        crate::CodegenError::Verify { message, .. } => {
            assert!(message.contains("known class"), "{message}")
        }
        other => panic!("expected Verify error, got {other:?}"),
    }
}
