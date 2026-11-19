//! `verify`::tests_b — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::tests_a::{plan, verify_err, verify_ok};
use crate::ir::{ArithOp, CmpOp, ElemKind, Instr, Ldc, NumKind, PrimType};
use j2r_runtime::TypeDesc;

#[test]
fn long_shift_pops_int_amount() {
    // lshl: long, int -> long
    let m = plan(
        "()J",
        true,
        4,
        0,
        vec![
            (0, Instr::LConst(1)),
            (1, Instr::IConst(2)),
            (
                2,
                Instr::Arith {
                    op: ArithOp::Shl,
                    kind: NumKind::Long,
                },
            ),
            (3, Instr::Return(TypeDesc::Long)),
        ],
    );
    verify_ok(&m);
    // wrong: two ints then lshl must fail
    let m = plan(
        "()J",
        true,
        4,
        0,
        vec![
            (0, Instr::IConst(1)),
            (1, Instr::IConst(2)),
            (
                2,
                Instr::Arith {
                    op: ArithOp::Shl,
                    kind: NumKind::Long,
                },
            ),
            (3, Instr::Return(TypeDesc::Long)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("expected Long"), "{e}");
}

#[test]
fn lastore_rejects_int_value() {
    // iconst_1 (instead of lconst) then lastore — category error
    let m = plan(
        "()V",
        true,
        4,
        1,
        vec![
            (0, Instr::IConst(2)), // array length
            (1, Instr::NewArray(PrimType::Long)),
            (2, Instr::IConst(0)), // index
            (3, Instr::IConst(5)), // WRONG: int value for a long[]
            (4, Instr::AStore(ElemKind::Long)),
            (5, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("expected Long"), "{e}");
}

#[test]
fn lastore_accepts_long_value() {
    let m = plan(
        "()V",
        true,
        5,
        1,
        vec![
            (0, Instr::IConst(2)),
            (1, Instr::NewArray(PrimType::Long)),
            (2, Instr::IConst(0)),
            (3, Instr::LConst(5)),
            (4, Instr::AStore(ElemKind::Long)),
            (5, Instr::Return(TypeDesc::Void)),
        ],
    );
    verify_ok(&m);
}

#[test]
fn aaload_pushes_a_ref_not_an_int() {
    let m = plan(
        "()I",
        true,
        3,
        0,
        vec![
            (0, Instr::IConst(2)),
            (
                1,
                Instr::ANewArray {
                    elem: "java/lang/Object".to_string(),
                },
            ),
            (2, Instr::IConst(0)),
            (3, Instr::ALoad(ElemKind::Ref)),
            (
                4,
                Instr::Arith {
                    op: ArithOp::Add,
                    kind: NumKind::Int,
                },
            ),
            (5, Instr::Return(TypeDesc::Int)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("expected Int"), "{e}");
}

#[test]
fn putstatic_accepts_null_for_array_typed_static() {
    // Corpus F2: javac compiles `static char[] NONE = null;` in
    // <clinit> as aconst_null; putstatic F:[C. null is assignable to
    // every reference type — arrays included — so the verifier must
    // accept it for a typed-array slot.
    let m = plan(
        "()V",
        true,
        1,
        0,
        vec![
            (0, Instr::AConstNull),
            (
                1,
                Instr::PutStatic {
                    class: "C".to_string(),
                    field: "f".to_string(),
                    desc: "[C".to_string(),
                },
            ),
            (2, Instr::Return(TypeDesc::Void)),
        ],
    );
    verify_ok(&m);
}

#[test]
fn null_argument_to_array_typed_parameter_verifies() {
    // `foo((int[]) null)` — the same null-for-reference-shape
    // polymorphism at an invokestatic call site.
    let m = plan(
        "()V",
        true,
        1,
        0,
        vec![
            (0, Instr::AConstNull),
            (
                1,
                Instr::InvokeStatic {
                    class: "C".to_string(),
                    name: "m".to_string(),
                    desc: "([I)V".to_string(),
                },
            ),
            (2, Instr::Return(TypeDesc::Void)),
        ],
    );
    verify_ok(&m);
}

#[test]
fn null_still_rejected_for_primitive_static() {
    // The polymorphism is one-directional: null is a reference. An
    // int-typed static keeps rejecting it.
    let m = plan(
        "()V",
        true,
        1,
        0,
        vec![
            (0, Instr::AConstNull),
            (
                1,
                Instr::PutStatic {
                    class: "C".to_string(),
                    field: "f".to_string(),
                    desc: "I".to_string(),
                },
            ),
            (2, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("expected Int"), "{e}");
}

#[test]
fn wrong_primitive_still_rejected_for_array_typed_static() {
    // Exactness is kept beyond null: an int must not slip into a
    // char[]-typed static either.
    let m = plan(
        "()V",
        true,
        1,
        0,
        vec![
            (0, Instr::IConst(0)),
            (
                1,
                Instr::PutStatic {
                    class: "C".to_string(),
                    field: "f".to_string(),
                    desc: "[C".to_string(),
                },
            ),
            (2, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("expected Array"), "{e}");
}

#[test]
fn depth4_aaload_chain_yields_typed_shapes() {
    // Depth-4 wave: `[[[[I` elements are `[[[I`-shaped values
    // (RefPrimArray3Int); three aaloads descend the lattice
    // RefPrimArray3Int → RefPrimArray2Int → RefPrimArrayInt → IntGroup,
    // and the extracted `int[]` accepts iastore.
    let m = plan(
        "()V",
        true,
        4,
        0,
        vec![
            (0, Instr::IConst(0)),
            (1, Instr::IConst(0)),
            (2, Instr::IConst(0)),
            (3, Instr::IConst(0)),
            (
                4,
                Instr::MultiNewArray {
                    desc: "[[[[I".to_string(),
                    dims: 4,
                },
            ),
            (5, Instr::IConst(0)),
            (6, Instr::ALoad(ElemKind::Ref)), // -> [[[I
            (7, Instr::IConst(0)),
            (8, Instr::ALoad(ElemKind::Ref)), // -> [[I
            (9, Instr::IConst(0)),
            (10, Instr::ALoad(ElemKind::Ref)), // -> [I
            (11, Instr::IConst(0)),
            (12, Instr::IConst(42)),
            (13, Instr::AStore(ElemKind::IntGroup)), // iastore into [I
            (14, Instr::Return(TypeDesc::Void)),
        ],
    );
    verify_ok(&m);
}

#[test]
fn depth4_wrong_kind_load_names_precise_kind() {
    // The precision the wave buys: iaload directly on a `[[[[I` array
    // fails with the EXACT depth-4 shape named (the pre-wave Ref
    // fallback made the message generic).
    let m = plan(
        "()V",
        true,
        4,
        0,
        vec![
            (0, Instr::IConst(0)),
            (1, Instr::IConst(0)),
            (2, Instr::IConst(0)),
            (3, Instr::IConst(0)),
            (
                4,
                Instr::MultiNewArray {
                    desc: "[[[[I".to_string(),
                    dims: 4,
                },
            ),
            (5, Instr::IConst(0)),
            (6, Instr::ALoad(ElemKind::IntGroup)), // iaload on the outer
            (7, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(
        e.message.contains("array is Nest(Nest(Nest(IntGroup)))"),
        "{e}"
    );
}

#[test]
fn depth4_aaload_result_feeds_typed_call_site() {
    // The JZlib-wave pattern one level deeper: the extracted `[[[I`
    // rows satisfy a `([[[I)V`-typed call site — the whole reason the
    // lattice is typed instead of Ref-collapsed.
    let m = plan(
        "()V",
        true,
        4,
        0,
        vec![
            (0, Instr::IConst(0)),
            (1, Instr::IConst(0)),
            (2, Instr::IConst(0)),
            (3, Instr::IConst(0)),
            (
                4,
                Instr::MultiNewArray {
                    desc: "[[[[I".to_string(),
                    dims: 4,
                },
            ),
            (5, Instr::IConst(0)),
            (6, Instr::ALoad(ElemKind::Ref)), // -> [[[I
            (
                7,
                Instr::InvokeStatic {
                    class: "C".to_string(),
                    name: "m".to_string(),
                    desc: "([[[I)V".to_string(),
                },
            ),
            (8, Instr::Return(TypeDesc::Void)),
        ],
    );
    verify_ok(&m);
}

#[test]
fn depth5_recursive_descriptor_any_depth() {
    // The closure law (2026-11-12 wave, owner-decided design): the
    // recursive descriptor folds ANY depth — depth 5 is
    // Nest(Nest(Nest(Nest(IntGroup)))), zero new variants. The typed
    // chain still verifies (aaload yields the exact (depth-1)-shaped
    // row, satisfying a `([[[[I)V` call site), and a wrong-kind
    // primitive load is still rejected — naming the exact folded
    // shape.
    let m = plan(
        "()V",
        true,
        5,
        0,
        vec![
            (0, Instr::IConst(0)),
            (1, Instr::IConst(0)),
            (2, Instr::IConst(0)),
            (3, Instr::IConst(0)),
            (4, Instr::IConst(0)),
            (
                5,
                Instr::MultiNewArray {
                    desc: "[[[[[I".to_string(),
                    dims: 5,
                },
            ),
            (6, Instr::IConst(0)),
            (7, Instr::ALoad(ElemKind::Ref)), // -> the exact 4-deep row shape
            (
                8,
                Instr::InvokeStatic {
                    class: "C".to_string(),
                    name: "m".to_string(),
                    desc: "([[[[I)V".to_string(),
                },
            ),
            (9, Instr::Return(TypeDesc::Void)),
        ],
    );
    verify_ok(&m);

    let bad = plan(
        "()V",
        true,
        5,
        0,
        vec![
            (0, Instr::IConst(0)),
            (1, Instr::IConst(0)),
            (2, Instr::IConst(0)),
            (3, Instr::IConst(0)),
            (4, Instr::IConst(0)),
            (
                5,
                Instr::MultiNewArray {
                    desc: "[[[[[I".to_string(),
                    dims: 5,
                },
            ),
            (6, Instr::IConst(0)),
            (7, Instr::ALoad(ElemKind::IntGroup)), // iaload on the outer
            (8, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&bad);
    assert!(
        e.message
            .contains("array is Nest(Nest(Nest(Nest(IntGroup))))"),
        "{e}"
    );
}

#[test]
fn putstatic_descriptor_shape_is_checked() {
    let m = plan(
        "()V",
        true,
        2,
        0,
        vec![
            (0, Instr::Ldc(Ldc::String("x".to_string()))),
            (
                1,
                Instr::PutStatic {
                    class: "C".to_string(),
                    field: "f".to_string(),
                    desc: "I".to_string(),
                },
            ),
            (2, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("expected Int"), "{e}");
}

#[test]
fn unreachable_code_is_not_verified() {
    // dead code after return: junk shapes are fine
    let m = plan(
        "()V",
        true,
        1,
        0,
        vec![
            (0, Instr::Return(TypeDesc::Void)),
            (1, Instr::Pop),  // unreachable underflow — ignored
            (2, Instr::Swap), // unreachable — ignored
            (3, Instr::Return(TypeDesc::Void)),
        ],
    );
    verify_ok(&m);
}

#[test]
fn falling_off_the_end_is_rejected() {
    let m = plan(
        "()V",
        true,
        1,
        0,
        vec![(0, Instr::IConst(1)), (1, Instr::Pop)],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("falls off"), "{e}");
}

#[test]
fn store_into_high_half_poisons_the_long() {
    // lstore_0; iconst_1; istore_1; lload_0 — local 0 was poisoned
    let m = plan(
        "()V",
        true,
        3,
        3,
        vec![
            (0, Instr::LConst(1)),
            (1, Instr::Store(0)),
            (2, Instr::IConst(1)),
            (3, Instr::Store(1)),
            (4, Instr::Load(0)),
            (5, Instr::Pop2),
            (6, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("conflicting types"), "{e}");
}

#[test]
fn iinc_requires_int_local() {
    let m = plan(
        "()V",
        true,
        1,
        2,
        vec![
            (0, Instr::Ldc(Ldc::String("s".to_string()))),
            (1, Instr::Store(0)),
            (2, Instr::IInc { idx: 0, c: 1 }),
            (3, Instr::Return(TypeDesc::Void)),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("int local"), "{e}");
}

#[test]
fn entry_locals_come_from_the_descriptor() {
    // instance method: local 0 is `this` (a ref)
    let m = plan(
        "()Ljava/lang/Object;",
        false,
        1,
        1,
        vec![
            (0, Instr::Load(0)),
            (1, Instr::Return(TypeDesc::Object(String::new()))),
        ],
    );
    verify_ok(&m);
    // but a static method with the same body must fail: local 0 unset
    let m = plan(
        "()Ljava/lang/Object;",
        true,
        1,
        1,
        vec![
            (0, Instr::Load(0)),
            (1, Instr::Return(TypeDesc::Object(String::new()))),
        ],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("never-assigned"), "{e}");
}

#[test]
fn max_locals_too_small_for_descriptor_is_rejected() {
    // (JD)V needs 4 locals; declare 2
    let m = plan(
        "(JD)V",
        true,
        4,
        2,
        vec![(0, Instr::Return(TypeDesc::Void))],
    );
    let e = verify_err(&m);
    assert!(e.message.contains("max_locals"), "{e}");
}

#[test]
fn join_of_null_and_ref_is_accepted() {
    // one arm produces null, the other a real ref; both merge to Ref
    let m = plan(
        "(I)Ljava/lang/Object;",
        true,
        2,
        1,
        vec![
            (0, Instr::Load(0)),
            (1, Instr::IfZero(CmpOp::Eq, 4)),
            (2, Instr::AConstNull),
            (3, Instr::Goto(5)),
            (4, Instr::Ldc(Ldc::String("x".to_string()))),
            (5, Instr::Return(TypeDesc::Object(String::new()))),
        ],
    );
    verify_ok(&m);
}
