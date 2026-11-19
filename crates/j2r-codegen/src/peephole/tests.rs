//! `peephole`::tests — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use crate::ir::{ElemKind, Ldc, PrimType};

use crate::ir::Instr as I;

/// The canonical `Map<String,Double> m = new HashMap<>(); m.put("a",
/// 0.5); double x = m.get("a");` main body (javac 21 shape, realistic
/// pcs): creation store to local 1, put core at 13, get core at 26.
fn canonical() -> Vec<(u32, I)> {
    vec![
        (
            0,
            I::New {
                class: "java/util/HashMap".into(),
            },
        ),
        (3, I::Dup),
        (
            4,
            I::InvokeSpecial {
                class: "java/util/HashMap".into(),
                name: "<init>".into(),
                desc: "()V".into(),
            },
        ),
        (7, I::Store(1)),
        (8, I::Load(1)),
        (9, I::Ldc(Ldc::String("a".into()))),
        (10, I::Ldc2W(crate::ir::Ldc2::Double(0.5))),
        (
            13,
            I::InvokeStatic {
                class: "java/lang/Double".into(),
                name: "valueOf".into(),
                desc: "(D)Ljava/lang/Double;".into(),
            },
        ),
        (
            16,
            I::InvokeInterface {
                class: "java/util/Map".into(),
                name: "put".into(),
                desc: "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;".into(),
            },
        ),
        (21, I::Pop),
        (22, I::Load(1)),
        (23, I::Ldc(Ldc::String("a".into()))),
        (
            26,
            I::InvokeInterface {
                class: "java/util/Map".into(),
                name: "get".into(),
                desc: "(Ljava/lang/Object;)Ljava/lang/Object;".into(),
            },
        ),
        (
            31,
            I::CheckCast {
                class: "java/lang/Double".into(),
            },
        ),
        (
            34,
            I::InvokeVirtual {
                class: "java/lang/Double".into(),
                name: "doubleValue".into(),
                desc: "()D".into(),
            },
        ),
        (37, I::Store(2)),
        (38, I::Return(j2r_runtime::TypeDesc::Void)),
    ]
}

fn shapes_of(instrs: &[(u32, I)]) -> crate::verify::MethodShapes {
    let m = crate::MethodPlan {
        name: "main".into(),
        desc: "([Ljava/lang/String;)V".into(),
        is_static: true,
        is_native: false,
        is_sync: false,
        modifiers: 0,
        signature: None,
        lvtt: Vec::new(),
        array_ops: Default::default(),
        fn_name: "m_main".into(),
        instrs: instrs.to_vec(),
        exception_table: Vec::new(),
        max_locals: 4,
        max_stack: 6,
    };
    let concats = std::collections::BTreeMap::new();
    let lambdas = std::collections::BTreeMap::new();
    crate::verify::verify_method_shapes(&m, &concats, &lambdas).expect("test body verifies")
}

#[test]
fn put_and_get_cores_rewrite() {
    let mut instrs = canonical();
    let shapes = shapes_of(&instrs);
    let n = map_prim_rewrites(&mut instrs, &[], &shapes, &Default::default());
    assert_eq!(n, 2, "both cores rewrite");
    assert_eq!(
        instrs[7].1,
        I::MapPutPrim {
            kind: crate::ir::MapKind::HashMap,
            prim: PrimType::Double
        },
        "the put core collapses at its valueOf pc"
    );
    assert_eq!(instrs[7].0, 13);
    assert_eq!(
        instrs[10].1,
        I::MapGetPrim {
            kind: crate::ir::MapKind::HashMap,
            prim: PrimType::Double
        },
        "the get core collapses at its get pc"
    );
    assert_eq!(instrs[10].0, 26);
}

// ---- G3.3 slice 4: the rewrites ride both new storages ----

/// The canonical body with the creation run and the invoke receiver
/// classes parameterized: `new <new_class>` stored to local 1, the
/// put/get invokes on `invoke_class` (the static receiver type).
fn canonical_for(new_class: &str, invoke_class: &str) -> Vec<(u32, I)> {
    vec![
        (
            0,
            I::New {
                class: new_class.into(),
            },
        ),
        (3, I::Dup),
        (
            4,
            I::InvokeSpecial {
                class: new_class.into(),
                name: "<init>".into(),
                desc: "()V".into(),
            },
        ),
        (7, I::Store(1)),
        (8, I::Load(1)),
        (9, I::Ldc(Ldc::String("a".into()))),
        (10, I::Ldc2W(crate::ir::Ldc2::Double(0.5))),
        (
            13,
            I::InvokeStatic {
                class: "java/lang/Double".into(),
                name: "valueOf".into(),
                desc: "(D)Ljava/lang/Double;".into(),
            },
        ),
        (
            16,
            I::InvokeInterface {
                class: invoke_class.into(),
                name: "put".into(),
                desc: "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;".into(),
            },
        ),
        (21, I::Pop),
        (22, I::Load(1)),
        (23, I::Ldc(Ldc::String("a".into()))),
        (
            26,
            I::InvokeInterface {
                class: invoke_class.into(),
                name: "get".into(),
                desc: "(Ljava/lang/Object;)Ljava/lang/Object;".into(),
            },
        ),
        (
            31,
            I::CheckCast {
                class: "java/lang/Double".into(),
            },
        ),
        (
            34,
            I::InvokeVirtual {
                class: "java/lang/Double".into(),
                name: "doubleValue".into(),
                desc: "()D".into(),
            },
        ),
        (37, I::Store(2)),
        (38, I::Return(j2r_runtime::TypeDesc::Void)),
    ]
}

#[test]
fn treemap_local_rewrites_with_the_red_black_kind() {
    // Map-typed local over a TreeMap creation run (the invoke class
    // stays java/util/Map) ...
    let mut instrs = canonical_for("java/util/TreeMap", "java/util/Map");
    let shapes = shapes_of(&instrs);
    assert_eq!(
        map_prim_rewrites(&mut instrs, &[], &shapes, &Default::default()),
        2
    );
    assert_eq!(
        instrs[7].1,
        I::MapPutPrim {
            kind: crate::ir::MapKind::TreeMap,
            prim: PrimType::Double
        }
    );
    assert_eq!(
        instrs[10].1,
        I::MapGetPrim {
            kind: crate::ir::MapKind::TreeMap,
            prim: PrimType::Double
        }
    );
    // ... and a TreeMap-TYPED local (the invokes carry the static
    // receiver type java/util/TreeMap) rewrites the same way.
    let mut instrs = canonical_for("java/util/TreeMap", "java/util/TreeMap");
    let shapes = shapes_of(&instrs);
    assert_eq!(
        map_prim_rewrites(&mut instrs, &[], &shapes, &Default::default()),
        2
    );
    assert!(matches!(
        instrs[7].1,
        I::MapPutPrim {
            kind: crate::ir::MapKind::TreeMap,
            ..
        }
    ));
}

#[test]
fn lhm_local_rewrites_with_the_link_chain_kind() {
    let mut instrs = canonical_for("java/util/LinkedHashMap", "java/util/LinkedHashMap");
    let shapes = shapes_of(&instrs);
    assert_eq!(
        map_prim_rewrites(&mut instrs, &[], &shapes, &Default::default()),
        2
    );
    assert_eq!(
        instrs[7].1,
        I::MapPutPrim {
            kind: crate::ir::MapKind::LinkedHashMap,
            prim: PrimType::Double
        }
    );
    assert_eq!(
        instrs[10].1,
        I::MapGetPrim {
            kind: crate::ir::MapKind::LinkedHashMap,
            prim: PrimType::Double
        }
    );
}

#[test]
fn lhm_access_order_ctor_refuses_the_whole_local() {
    // `new LinkedHashMap<>(4, 0.75f, true)` — three const args
    // between the Dup and the <init>: the access-order ctor. The
    // rewrite must NOT fire (j2r's LHM is insertion-ordered; the
    // faithful path stands everywhere). Built with fresh monotonic
    // pcs: New(0) Dup(3) iconst(4) ldc f(5) iconst(6) <init>(7)
    // Store(10), then the canonical body shifted by +10.
    let mut instrs: Vec<(u32, I)> = vec![
        (
            0,
            I::New {
                class: "java/util/LinkedHashMap".into(),
            },
        ),
        (3, I::Dup),
        (4, I::IConst(4)),
        (5, I::Ldc(Ldc::Float(0.75))),
        (6, I::IConst(1)),
        (
            7,
            I::InvokeSpecial {
                class: "java/util/LinkedHashMap".into(),
                name: "<init>".into(),
                desc: "(IFZ)V".into(),
            },
        ),
        (10, I::Store(1)),
    ];
    for (pc, i) in canonical_for("java/util/LinkedHashMap", "java/util/Map") {
        if pc > 7 {
            instrs.push((pc + 3, i));
        }
    }
    let shapes = shapes_of(&instrs);
    assert_eq!(
        map_prim_rewrites(&mut instrs, &[], &shapes, &Default::default()),
        0,
        "the access-order creation run refuses the whole local"
    );
}

#[test]
fn comparator_ctor_refuses_the_whole_local() {
    // `new TreeMap<>(comp)` — the comparator is a LOAD (not a const
    // push), so the creation run's matcher refuses: the faithful
    // path stands. Hand-built with monotonic pcs: the comparator
    // local assigned first, then New(4) Dup(7) Load(2)(8) <init>(9).
    let mut instrs: Vec<(u32, I)> = vec![
        (0, I::Ldc(Ldc::String("c".into()))),
        (3, I::Store(2)),
        (
            4,
            I::New {
                class: "java/util/TreeMap".into(),
            },
        ),
        (7, I::Dup),
        (8, I::Load(2)),
        (
            9,
            I::InvokeSpecial {
                class: "java/util/TreeMap".into(),
                name: "<init>".into(),
                desc: "(Ljava/util/Comparator;)V".into(),
            },
        ),
        (12, I::Store(1)),
    ];
    for (pc, i) in canonical_for("java/util/TreeMap", "java/util/Map") {
        if pc > 7 {
            instrs.push((pc + 5, i));
        }
    }
    let shapes = shapes_of(&instrs);
    assert_eq!(
        map_prim_rewrites(&mut instrs, &[], &shapes, &Default::default()),
        0,
        "the comparator creation run refuses the whole local"
    );
}

#[test]
fn treemap_typed_receiver_without_provenance_stays_faithful() {
    // A TreeMap-typed receiver that is NOT a fresh creation run —
    // local 1 assigned from a plain non-creation Store (the
    // param/arg-shaped local): the faithful path stands.
    let mut instrs: Vec<(u32, I)> = vec![(0, I::Ldc(Ldc::String("x".into()))), (3, I::Store(1))];
    for (pc, i) in canonical_for("java/util/TreeMap", "java/util/TreeMap") {
        if pc > 7 {
            instrs.push((pc, i));
        }
    }
    let shapes = shapes_of(&instrs);
    assert_eq!(
        map_prim_rewrites(&mut instrs, &[], &shapes, &Default::default()),
        0,
        "no creation run, no rewrite"
    );
}

#[test]
fn non_creation_store_refuses() {
    let mut instrs = canonical();
    // A second store to the map local that is not a fresh creation:
    // the local's provenance is no longer provable. Inserted directly
    // after the creation store (index 3); the rest re-spaces by +2 so
    // every pc stays distinct.
    instrs.insert(4, (8, I::AConstNull));
    instrs.insert(5, (9, I::Store(1)));
    for pc_entry in instrs.iter_mut().skip(6) {
        pc_entry.0 += 2;
    }
    let shapes = shapes_of(&instrs);
    let n = map_prim_rewrites(&mut instrs, &[], &shapes, &Default::default());
    assert_eq!(n, 0, "a non-creation store kills freshness");
}

#[test]
fn escape_load_refuses_closure() {
    let mut instrs = canonical();
    // A load of the map local consumed by something that is not a
    // rewritten core's receiver (here: pushed and discarded — the
    // argument-pass / field-store shape class).
    instrs.push((39, I::Load(1)));
    instrs.push((40, I::Pop));
    instrs.push((41, I::Return(j2r_runtime::TypeDesc::Void)));
    let shapes = shapes_of(&instrs);
    let n = map_prim_rewrites(&mut instrs, &[], &shapes, &Default::default());
    assert_eq!(n, 0, "an uncovered load kills the whole local");
}

#[test]
fn non_core_get_refuses_closure() {
    let mut instrs = canonical();
    // `Object o = m.get("a")` discarded: the get's result is NOT
    // consumed by checkcast+unbox, so no get core forms and the
    // receiver load is uncovered. Keep the creation + the put core,
    // then a bare get whose result is popped.
    instrs.truncate(10);
    instrs.push((22, I::Load(1)));
    instrs.push((23, I::Ldc(Ldc::String("a".into()))));
    instrs.push((
        26,
        I::InvokeInterface {
            class: "java/util/Map".into(),
            name: "get".into(),
            desc: "(Ljava/lang/Object;)Ljava/lang/Object;".into(),
        },
    ));
    instrs.push((31, I::Pop));
    instrs.push((32, I::Return(j2r_runtime::TypeDesc::Void)));
    let shapes = shapes_of(&instrs);
    let n = map_prim_rewrites(&mut instrs, &[], &shapes, &Default::default());
    assert_eq!(n, 0, "a non-core get refuses the closure");
}

#[test]
fn partial_handler_overlap_refuses() {
    let instrs = canonical();
    let shapes = shapes_of(&instrs);
    let mut instrs2 = instrs.clone();
    // A handler that ENDS between the valueOf and the put: the
    // synthetic faults at the core's first pc, so a boundary inside
    // the span can move a throw across it.
    let table = vec![(8u32, 17u32, 38u32, None)];
    let n = map_prim_rewrites(&mut instrs2, &table, &shapes, &Default::default());
    assert_eq!(n, 0, "a handler boundary inside the span refuses");
    // A handler covering the WHOLE span is fine.
    let mut instrs3 = instrs;
    let table2 = vec![(8u32, 22u32, 38u32, None)];
    let n2 = map_prim_rewrites(&mut instrs3, &table2, &shapes, &Default::default());
    assert_eq!(n2, 2, "a full-span handler keeps both cores");
}

fn fill_run(len: i32, vals: &[i32], putstatic: bool) -> Vec<(u32, I)> {
    let mut v: Vec<(u32, I)> = Vec::new();
    let mut pc = 0u32;
    v.push((pc, I::IConst(len)));
    pc += 1;
    v.push((pc, I::NewArray(PrimType::Int)));
    pc += 1;
    for (idx, val) in vals.iter().enumerate() {
        v.push((pc, I::Dup));
        pc += 1;
        v.push((pc, I::IConst(idx as i32)));
        pc += 1;
        v.push((pc, I::IConst(*val)));
        pc += 1;
        v.push((pc, I::AStore(ElemKind::IntGroup)));
        pc += 1;
    }
    if putstatic {
        v.push((
            pc,
            I::PutStatic {
                class: "T".into(),
                field: "a".into(),
                desc: "[I".into(),
            },
        ));
    }
    v
}

#[test]
fn collapses_a_literal_fill_run() {
    let vals: Vec<i32> = (0..32).map(|k| k * 7 - 3).collect();
    let mut instrs = fill_run(32, &vals, true);
    let n = collapse_array_fill_runs(&mut instrs);
    assert_eq!(n, 1);
    assert_eq!(instrs.len(), 2); // fill + putstatic
    assert_eq!(instrs[0].0, 0);
    match &instrs[0].1 {
        I::NewIntArrayFilled { vals: got, .. } => assert_eq!(got, &vals),
        other => panic!("expected NewIntArrayFilled, got {other:?}"),
    }
    assert!(matches!(instrs[1].1, I::PutStatic { .. }));
}

#[test]
fn refuses_when_group_count_differs_from_len() {
    // len says 10 but only 8 stores follow (partial fill elsewhere)
    let mut instrs = fill_run(10, &(0..8).collect::<Vec<_>>(), true);
    assert_eq!(collapse_array_fill_runs(&mut instrs), 0);
    assert_eq!(instrs.len(), 2 + 8 * 4 + 1);
}

#[test]
fn refuses_below_threshold() {
    let vals = vec![1i32; 4];
    let mut instrs = fill_run(4, &vals, true);
    assert_eq!(collapse_array_fill_runs(&mut instrs), 0);
}

#[test]
fn refuses_interior_branch_target() {
    // a goto into the middle of the run: index 1's dup — must not collapse
    let mut instrs = fill_run(16, &(0..16).collect::<Vec<_>>(), true);
    // find the pc of the second group's dup (pc 2 + 4 + 1)
    let mid_pc = 2 + 4 + 1;
    instrs.push((mid_pc + 40, I::Goto(mid_pc))); // jump INTO the run
    assert_eq!(collapse_array_fill_runs(&mut instrs), 0);
}

#[test]
fn allows_target_at_run_start_and_after() {
    let vals: Vec<i32> = vec![5i32; 12];
    let mut instrs = fill_run(12, &vals, true);
    // branch to the run start (pc 0) and to the putstatic (pc 50): legal
    instrs.push((50 + 10, I::Goto(0)));
    instrs.push((50 + 20, I::Goto(50)));
    assert_eq!(collapse_array_fill_runs(&mut instrs), 1);
}

#[test]
fn collapses_short_and_byte_fills_too() {
    // [S fill: same group shape, NewArray(Short) — StaticTree's shape
    let mut v: Vec<(u32, I)> = Vec::new();
    v.push((0, I::IConst(9)));
    v.push((1, I::NewArray(PrimType::Short)));
    for k in 0..9u32 {
        v.push((2 + k * 4, I::Dup));
        v.push((3 + k * 4, I::IConst(k as i32)));
        v.push((4 + k * 4, I::IConst(300 + k as i32)));
        v.push((5 + k * 4, I::AStore(ElemKind::IntGroup)));
    }
    v.push((
        38,
        I::PutStatic {
            class: "T".into(),
            field: "s".into(),
            desc: "[S".into(),
        },
    ));
    assert_eq!(collapse_array_fill_runs(&mut v), 1);
    assert!(matches!(
        &v[0].1,
        I::NewIntArrayFilled {
            prim: PrimType::Short,
            ..
        }
    ));
}

#[test]
fn leaves_non_int_shape_untouched() {
    // long store into [J is AStore(ElemKind::Long) — v1 collapses [I only
    let vals = vec![1i32; 8];
    let mut instrs = fill_run(8, &vals, true);
    // corrupt one store to Ref — shape broken
    instrs[2 + 4].1 = I::AStore(ElemKind::Ref);
    assert_eq!(collapse_array_fill_runs(&mut instrs), 0);
}

#[test]
fn refuses_long_array_fill_v1() {
    // [J uses AStore(ElemKind::Long) — not an int-group store
    let vals = vec![1i32; 8];
    let mut instrs = fill_run(8, &vals, true);
    instrs[1].1 = I::NewArray(PrimType::Long);
    for k in 0..8i32 {
        instrs[2 + k as usize * 4 + 3].1 = I::AStore(ElemKind::Long);
    }
    assert_eq!(collapse_array_fill_runs(&mut instrs), 0);
}

#[test]
fn collapses_two_runs_in_one_method() {
    let mut instrs = fill_run(8, &[1i32; 8], false);
    let tail_start = instrs.last().unwrap().0 + 1;
    let mut second = fill_run(9, &[2i32; 9], true);
    for (pc, _) in second.iter_mut() {
        *pc += tail_start;
    }
    let putstatic_pc = second.last().unwrap().0;
    instrs.extend(second);
    // branch target AFTER everything: must not block either run
    instrs.push((putstatic_pc + 10, I::Return(j2r_runtime::TypeDesc::Void)));
    assert_eq!(collapse_array_fill_runs(&mut instrs), 2);
}

// ---- G3.3 slice 3: the getstatic receivers (field/materialization) ----

fn mp(name: &str, instrs: Vec<(u32, I)>) -> crate::MethodPlan {
    crate::MethodPlan {
        name: name.into(),
        desc: "()V".into(),
        is_static: true,
        is_native: false,
        is_sync: false,
        modifiers: 0,
        signature: None,
        lvtt: Vec::new(),
        array_ops: Default::default(),
        fn_name: format!("m_{name}"),
        instrs,
        exception_table: Vec::new(),
        max_locals: 4,
        max_stack: 6,
    }
}

fn map_field(sig: &str) -> crate::FieldPlan {
    crate::FieldPlan {
        name: "m".into(),
        desc: "Ljava/util/Map;".into(),
        is_static: true,
        static_slot: Some(0),
        instance_slot: None,
        constant_value: None,
        is_volatile: false,
        is_final: false,
        modifiers: 0,
        signature: Some(sig.into()),
    }
}

const SIG_DOUBLE: &str = "Ljava/util/Map<Ljava/lang/String;Ljava/lang/Double;>;";

/// The scan fixture (the Probe3 javap shapes): creation in <clinit>,
/// the put + get cores in main, all through the static field `m`.
fn field_cls_methods() -> Vec<crate::MethodPlan> {
    vec![
        mp(
            "<clinit>",
            vec![
                (
                    0,
                    I::New {
                        class: "java/util/HashMap".into(),
                    },
                ),
                (3, I::Dup),
                (
                    4,
                    I::InvokeSpecial {
                        class: "java/util/HashMap".into(),
                        name: "<init>".into(),
                        desc: "()V".into(),
                    },
                ),
                (
                    7,
                    I::PutStatic {
                        class: "T".into(),
                        field: "m".into(),
                        desc: "Ljava/util/Map;".into(),
                    },
                ),
                (10, I::Return(j2r_runtime::TypeDesc::Void)),
            ],
        ),
        mp(
            "main",
            vec![
                (
                    0,
                    I::GetStatic {
                        class: "T".into(),
                        field: "m".into(),
                        desc: "Ljava/util/Map;".into(),
                    },
                ),
                (3, I::Ldc(Ldc::String("a".into()))),
                (6, I::Ldc2W(crate::ir::Ldc2::Double(0.5))),
                (
                    9,
                    I::InvokeStatic {
                        class: "java/lang/Double".into(),
                        name: "valueOf".into(),
                        desc: "(D)Ljava/lang/Double;".into(),
                    },
                ),
                (
                    12,
                    I::InvokeInterface {
                        class: "java/util/Map".into(),
                        name: "put".into(),
                        desc: "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;".into(),
                    },
                ),
                (17, I::Pop),
                (
                    18,
                    I::GetStatic {
                        class: "T".into(),
                        field: "m".into(),
                        desc: "Ljava/util/Map;".into(),
                    },
                ),
                (21, I::Ldc(Ldc::String("a".into()))),
                (
                    24,
                    I::InvokeInterface {
                        class: "java/util/Map".into(),
                        name: "get".into(),
                        desc: "(Ljava/lang/Object;)Ljava/lang/Object;".into(),
                    },
                ),
                (
                    29,
                    I::CheckCast {
                        class: "java/lang/Double".into(),
                    },
                ),
                (
                    32,
                    I::InvokeVirtual {
                        class: "java/lang/Double".into(),
                        name: "doubleValue".into(),
                        desc: "()D".into(),
                    },
                ),
                (35, I::Store(2)),
                (36, I::Return(j2r_runtime::TypeDesc::Void)),
            ],
        ),
    ]
}

#[test]
fn scan_proves_field_and_rewrite_fires() {
    let methods = field_cls_methods();
    let fields = vec![map_field(SIG_DOUBLE)];
    let proven = proven_map_fields_of("T", &fields, &methods);
    assert_eq!(proven.len(), 1, "the field proves");
    assert_eq!(
        proven[&("T".to_string(), "m".to_string())],
        (PrimType::Double, crate::ir::MapKind::HashMap)
    );
    // The rewrite side: both field-fed cores collapse (the synthetic
    // lands at the valueOf pc / the get pc).
    let mut instrs = methods[1].instrs.clone();
    let shapes = shapes_of(&instrs);
    let n = map_prim_rewrites(&mut instrs, &[], &shapes, &proven);
    assert_eq!(n, 2, "both field-fed cores rewrite");
    assert_eq!(
        instrs[3].1,
        I::MapPutPrim {
            kind: crate::ir::MapKind::HashMap,
            prim: PrimType::Double
        }
    );
    assert_eq!(instrs[3].0, 9);
    assert_eq!(
        instrs[6].1,
        I::MapGetPrim {
            kind: crate::ir::MapKind::HashMap,
            prim: PrimType::Double
        }
    );
    assert_eq!(instrs[6].0, 24);
}

#[test]
fn scan_allows_generic_exit_and_mixed_prims() {
    // The probe's P4 shape plus a prim-mismatched core: with the
    // runtime's lazy materialization and the fused get's unbox, the
    // generic exit stays faithful and the mismatched core throws the
    // faithful CCE — so the scan REQUIRES nothing of the readers.
    let mut methods = field_cls_methods();
    methods.push(mp(
        "exit",
        vec![
            (
                0,
                I::GetStatic {
                    class: "T".into(),
                    field: "m".into(),
                    desc: "Ljava/util/Map;".into(),
                },
            ),
            (3, I::Ldc(Ldc::String("a".into()))),
            (
                6,
                I::InvokeInterface {
                    class: "java/util/Map".into(),
                    name: "get".into(),
                    desc: "(Ljava/lang/Object;)Ljava/lang/Object;".into(),
                },
            ),
            (
                11,
                I::Return(j2r_runtime::TypeDesc::Object("java/lang/Object".into())),
            ),
        ],
    ));
    // A prim-mismatched get core (checkcast Integer) on the same
    // field: the fused unbox carries the faithful CCE.
    let mut mixed = methods[1].instrs.clone();
    mixed[9] = (
        29,
        I::CheckCast {
            class: "java/lang/Integer".into(),
        },
    );
    mixed[10] = (
        32,
        I::InvokeVirtual {
            class: "java/lang/Integer".into(),
            name: "intValue".into(),
            desc: "()I".into(),
        },
    );
    mixed[11] = (35, I::Store(2));
    methods[1].instrs = mixed;
    let fields = vec![map_field(SIG_DOUBLE)];
    let proven = proven_map_fields_of("T", &fields, &methods);
    assert_eq!(
        proven.len(),
        1,
        "the generic exit and the mismatched core do not refuse the field"
    );
    // Both cores of the mixed method rewrite (the mismatch's CCE is
    // a runtime law, carried by the fused unbox).
    let mut instrs = methods[1].instrs.clone();
    let shapes = shapes_of(&instrs);
    assert_eq!(map_prim_rewrites(&mut instrs, &[], &shapes, &proven), 2);
    assert!(matches!(
        instrs[6].1,
        I::MapGetPrim {
            prim: PrimType::Int,
            ..
        }
    ));
}

#[test]
fn scan_refuses_non_creation_putstatic() {
    let mut methods = field_cls_methods();
    // `m = null;` — a non-creation store kills the provenance.
    methods.push(mp(
        "reassign",
        vec![
            (0, I::AConstNull),
            (
                1,
                I::PutStatic {
                    class: "T".into(),
                    field: "m".into(),
                    desc: "Ljava/util/Map;".into(),
                },
            ),
            (4, I::Return(j2r_runtime::TypeDesc::Void)),
        ],
    ));
    let fields = vec![map_field(SIG_DOUBLE)];
    assert!(proven_map_fields_of("T", &fields, &methods).is_empty());
}

#[test]
fn scan_refuses_without_creation() {
    // The use-site method alone: cores but NO PutStatic anywhere, the
    // provenance is unprovable.
    let methods = vec![field_cls_methods().remove(1)];
    let fields = vec![map_field(SIG_DOUBLE)];
    assert!(proven_map_fields_of("T", &fields, &methods).is_empty());
}

#[test]
fn getstatic_receiver_rewrites_only_when_proven() {
    let instrs_for = || {
        vec![
            (
                0,
                I::GetStatic {
                    class: "T".into(),
                    field: "m".into(),
                    desc: "Ljava/util/Map;".into(),
                },
            ),
            (3, I::Ldc(Ldc::String("a".into()))),
            (6, I::Ldc2W(crate::ir::Ldc2::Double(0.5))),
            (
                9,
                I::InvokeStatic {
                    class: "java/lang/Double".into(),
                    name: "valueOf".into(),
                    desc: "(D)Ljava/lang/Double;".into(),
                },
            ),
            (
                12,
                I::InvokeInterface {
                    class: "java/util/Map".into(),
                    name: "put".into(),
                    desc: "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;".into(),
                },
            ),
            (17, I::Pop),
            (18, I::Return(j2r_runtime::TypeDesc::Void)),
        ]
    };
    let mut proven = std::collections::BTreeMap::new();
    proven.insert(
        ("T".to_string(), "m".to_string()),
        (PrimType::Double, crate::ir::MapKind::HashMap),
    );
    let mut instrs = instrs_for();
    let shapes = shapes_of(&instrs);
    assert_eq!(
        map_prim_rewrites(&mut instrs, &[], &shapes, &proven),
        1,
        "the proven field receiver rewrites"
    );
    assert_eq!(
        instrs[3].1,
        I::MapPutPrim {
            kind: crate::ir::MapKind::HashMap,
            prim: PrimType::Double
        }
    );
    // Unproven: the faithful path stands.
    let mut instrs2 = instrs_for();
    let shapes2 = shapes_of(&instrs2);
    assert_eq!(
        map_prim_rewrites(&mut instrs2, &[], &shapes2, &Default::default()),
        0,
        "an unproven field receiver refuses"
    );
}
