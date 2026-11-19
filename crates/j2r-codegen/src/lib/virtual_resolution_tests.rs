//! `lib`::virtual_resolution_tests — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).

#[cfg(test)]
pub(crate) mod virtual_resolution_tests {
    use crate::*;
    use std::collections::BTreeSet;

    /// A minimal ClassPlan carrying only what the resolver reads:
    /// name / super_name / is_interface / module / methods.
    fn plan(name: &str, sup: Option<&str>, methods: &[(&str, &str)]) -> ClassPlan {
        let methods: Vec<MethodPlan> = methods
            .iter()
            .map(|(n, d)| MethodPlan {
                name: (*n).to_string(),
                desc: (*d).to_string(),
                is_static: false,
                is_native: false,
                is_sync: false,
                modifiers: 0x0001,
                signature: None,
                lvtt: Vec::new(),
                array_ops: Default::default(),
                fn_name: method_fn_name(n, d),
                instrs: Vec::new(),
                exception_table: Vec::new(),
                max_locals: 0,
                max_stack: 0,
            })
            .collect();
        ClassPlan {
            name: name.to_string(),
            super_name: sup.map(|s| s.to_string()),
            interfaces: Vec::new(),
            is_interface: false,
            is_abstract: false,
            simple_name: None,
            nest_host: None,
            module: sanitize_module(name),
            id: 0,
            base_slots: 0,
            total_slots: 0,
            default_spec: String::new(),
            fields: Vec::new(),
            methods,
            clinit_fn: None,
            concats: BTreeMap::new(),
            lambdas: BTreeMap::new(),
            static_monitor: None,
            refs: BTreeSet::new(),
            input_sha: String::new(),
            cf: ClassFile {
                minor_version: 0,
                major_version: 52,
                constant_pool: j2r_classfile::ConstantPool::default(),
                access: 0x0021,
                this_class: name.to_string(),
                super_class: sup.map(|s| s.to_string()),
                interfaces: Vec::new(),
                fields: Vec::new(),
                methods: Vec::new(),
                bootstrap_methods: Vec::new(),
                inner_name: None,
                signature: None,
                nest_host: None,
            },
        }
    }

    fn prog(classes: Vec<ClassPlan>) -> Program {
        let mut user_index = BTreeMap::new();
        for (i, c) in classes.iter().enumerate() {
            user_index.insert(c.name.clone(), i);
        }
        Program {
            classes,
            user_index,
            total_statics: 0,
            main_class: None,
        }
    }

    const MV: &str = "()V";
    const TS: &str = "()Ljava/lang/String;";

    fn user_target(t: Option<CallTarget>) -> Option<(String, String)> {
        match t {
            Some(CallTarget::User { module, fn_name }) => Some((module, fn_name)),
            _ => None,
        }
    }

    #[test]
    fn inherited_method_folds_to_owner() {
        // A.m compiled; B extends A without an override — both walks land
        // on A.m, so even the A-typed site folds.
        let p = prog(vec![
            plan("A", Some("java/lang/Object"), &[("m", MV)]),
            plan("B", Some("A"), &[]),
        ]);
        let a_m = resolve_virtual_target(&p, "A", "m", MV).unwrap();
        let b_m = resolve_virtual_target(&p, "B", "m", MV).unwrap();
        match (user_target(Some(a_m)), user_target(Some(b_m))) {
            (Some((ma, fa)), Some((mb, fb))) => {
                assert_eq!(
                    (ma.as_str(), fa.as_str()),
                    ("A", method_fn_name("m", MV).as_str())
                );
                assert_eq!(
                    (mb.as_str(), fb.as_str()),
                    ("A", method_fn_name("m", MV).as_str())
                );
            }
            _ => panic!("both sites must fold to A.m"),
        }
    }

    #[test]
    fn polymorphic_override_stays_dynamic() {
        // B overrides m: an A-typed site diverges (A.m vs B.m) — dynamic.
        // The B-typed site is monomorphic — folds to B.m.
        let p = prog(vec![
            plan("A", Some("java/lang/Object"), &[("m", MV)]),
            plan("B", Some("A"), &[("m", MV)]),
        ]);
        assert!(resolve_virtual_target(&p, "A", "m", MV).is_none());
        let folded = user_target(resolve_virtual_target(&p, "B", "m", MV)).unwrap();
        assert_eq!(
            (folded.0.as_str(), folded.1.as_str()),
            ("B", method_fn_name("m", MV).as_str())
        );
    }

    #[test]
    fn user_override_beats_the_native_row_and_inherits() {
        // A overrides toString; D extends A — both walk to A's compiled
        // method, never the Object native row.
        let p = prog(vec![
            plan("A", Some("java/lang/Object"), &[("toString", TS)]),
            plan("D", Some("A"), &[]),
        ]);
        let folded = user_target(resolve_virtual_target(&p, "A", "toString", TS)).unwrap();
        assert_eq!(
            (folded.0.as_str(), folded.1.as_str()),
            ("A", method_fn_name("toString", TS).as_str())
        );
    }

    #[test]
    fn divergence_user_vs_native_stays_dynamic() {
        // A has no toString (walk reaches the Object native row) but B
        // extends A and overrides it — the A-typed site diverges.
        let p = prog(vec![
            plan("A", Some("java/lang/Object"), &[]),
            plan("B", Some("A"), &[("toString", TS)]),
        ]);
        assert!(resolve_virtual_target(&p, "A", "toString", TS).is_none());
        // The B-typed site still folds to B's own override.
        let folded = user_target(resolve_virtual_target(&p, "B", "toString", TS)).unwrap();
        assert_eq!(
            (folded.0.as_str(), folded.1.as_str()),
            ("B", method_fn_name("toString", TS).as_str())
        );
    }

    #[test]
    fn native_row_folds_when_no_user_touches_it() {
        // Nothing in the program overrides hashCode: every walk from A
        // reaches the Object native row.
        let p = prog(vec![plan("A", Some("java/lang/Object"), &[])]);
        match resolve_virtual_target(&p, "A", "hashCode", "()I") {
            Some(CallTarget::Native { class }) => {
                assert_eq!(class, "java/lang/Object");
            }
            other => panic!("expected the Object native row, got {other:?}"),
        }
    }

    #[test]
    fn deep_chain_folds_through_the_middle() {
        // A.m; B extends A; C extends B — the C-typed site folds to A.m
        // through two inherited levels.
        let p = prog(vec![
            plan("A", Some("java/lang/Object"), &[("m", MV)]),
            plan("B", Some("A"), &[]),
            plan("C", Some("B"), &[]),
        ]);
        let folded = user_target(resolve_virtual_target(&p, "C", "m", MV)).unwrap();
        assert_eq!(
            (folded.0.as_str(), folded.1.as_str()),
            ("A", method_fn_name("m", MV).as_str())
        );
    }

    #[test]
    fn sibling_overrides_keep_the_parent_site_dynamic() {
        // A.m; B and C both extend A and both override m — three distinct
        // walks from A: A.m, B.m, C.m — dynamic.
        let p = prog(vec![
            plan("A", Some("java/lang/Object"), &[("m", MV)]),
            plan("B", Some("A"), &[("m", MV)]),
            plan("C", Some("A"), &[("m", MV)]),
        ]);
        assert!(resolve_virtual_target(&p, "A", "m", MV).is_none());
        assert!(user_target(resolve_virtual_target(&p, "B", "m", MV)).is_some());
        assert!(user_target(resolve_virtual_target(&p, "C", "m", MV)).is_some());
    }

    #[test]
    fn builtin_array_and_unknown_declared_types_stay_dynamic() {
        let p = prog(vec![plan("A", Some("java/lang/Object"), &[])]);
        // builtin declared type: open receiver set
        assert!(resolve_virtual_target(
            &p,
            "java/util/HashMap",
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;"
        )
        .is_none());
        // array declared type: the special forms stay on the dynamic path
        assert!(resolve_virtual_target(&p, "[LA;", "clone", "()Ljava/lang/Object;").is_none());
        // unknown name: not in the closed world at all
        assert!(resolve_virtual_target(&p, "NotHere", "m", MV).is_none());
    }

    // ------------------------------------------- R5 slice 2: interface closure + vtables

    /// An interface plan (no methods unless given).
    fn itf_plan(name: &str, extends: &[&str], methods: &[(&str, &str)]) -> ClassPlan {
        let mut c = plan(name, Some("java/lang/Object"), methods);
        c.is_interface = true;
        c.interfaces = extends.iter().map(|s| s.to_string()).collect();
        c
    }

    /// A class plan implementing `impls`.
    fn impl_plan(
        name: &str,
        sup: Option<&str>,
        impls: &[&str],
        methods: &[(&str, &str)],
    ) -> ClassPlan {
        let mut c = plan(name, sup, methods);
        c.interfaces = impls.iter().map(|s| s.to_string()).collect();
        c
    }

    #[test]
    fn slice2_monomorphic_interface_site_folds() {
        // Supersedes the slice-1 gate above: with the implementor closure
        // the ONE-implementor interface site folds to that implementor.
        let p = prog(vec![
            itf_plan("I", &[], &[]),
            impl_plan("Impl", None, &["I"], &[("m", MV)]),
        ]);
        let folded = user_target(resolve_virtual_target(&p, "I", "m", MV)).unwrap();
        assert_eq!(
            (folded.0.as_str(), folded.1.as_str()),
            ("Impl", method_fn_name("m", MV).as_str())
        );
    }

    #[test]
    fn slice2_interface_receiver_set_is_the_implementor_closure() {
        // The closure collects implementors through super chains AND
        // sub-interfaces; unrelated classes stay out; interfaces are never
        // receivers.
        let p = prog(vec![
            itf_plan("Top", &[], &[]),
            itf_plan("Mid", &["Top"], &[]),
            impl_plan("ViaMid", None, &["Mid"], &[("m", MV)]),
            impl_plan("InhBase", None, &["Top"], &[]),
            impl_plan("InhSub", Some("InhBase"), &[], &[]),
            impl_plan("Other", None, &[], &[("m", MV)]),
            itf_plan("Unrelated", &[], &[]),
        ]);
        assert_eq!(
            virtual_receiver_set(&p, "Top").unwrap(),
            vec![
                "ViaMid".to_string(),
                "InhBase".to_string(),
                "InhSub".to_string()
            ]
        );
        // Sub-interface closures reach their super-interfaces' sites.
        assert_eq!(
            virtual_receiver_set(&p, "Mid").unwrap(),
            vec!["ViaMid".to_string()]
        );
        // An interface nobody implements: no possible non-null receiver.
        assert!(virtual_receiver_set(&p, "Unrelated").is_none());
    }

    #[test]
    fn slice2_subinterface_and_inherited_walks_fold() {
        // ViaMid.m via the Top-typed site: the walk starts at the concrete
        // class and the sub-interface edge only decides MEMBERSHIP.
        let p = prog(vec![
            itf_plan("Top", &[], &[]),
            itf_plan("Mid", &["Top"], &[]),
            impl_plan("ViaMid", None, &["Mid"], &[("m", MV)]),
        ]);
        let folded = user_target(resolve_virtual_target(&p, "Top", "m", MV)).unwrap();
        assert_eq!(
            (folded.0.as_str(), folded.1.as_str()),
            ("ViaMid", method_fn_name("m", MV).as_str())
        );
        // The inherited form: InhSub carries no m — the walk lands on
        // InhBase.m, so even the interface-typed site folds to the BASE.
        let p2 = prog(vec![
            itf_plan("Inh", &[], &[]),
            impl_plan("InhBase", None, &["Inh"], &[("m", MV)]),
            impl_plan("InhSub", Some("InhBase"), &[], &[]),
        ]);
        let folded2 = user_target(resolve_virtual_target(&p2, "Inh", "m", MV)).unwrap();
        assert_eq!(
            (folded2.0.as_str(), folded2.1.as_str()),
            ("InhBase", method_fn_name("m", MV).as_str())
        );
    }

    #[test]
    fn slice2_polymorphic_interface_site_gets_a_vtable() {
        // Two implementors diverge: no single fold, but a two-arm vtable
        // grouped per implementor (declaration order preserved).
        let p = prog(vec![
            itf_plan("Poly", &[], &[]),
            impl_plan("P1", None, &["Poly"], &[("m", MV)]),
            impl_plan("P2", None, &["Poly"], &[("m", MV)]),
        ]);
        assert!(resolve_virtual_target(&p, "Poly", "m", MV).is_none());
        let arms = resolve_vtable(&p, "Poly", "m", MV).unwrap();
        assert_eq!(arms.len(), 2);
        assert_eq!(arms[0].classes, vec!["P1".to_string()]);
        assert_eq!(user_target(Some(arms[0].target.clone())).unwrap().0, "P1");
        assert_eq!(arms[1].classes, vec!["P2".to_string()]);
        assert_eq!(user_target(Some(arms[1].target.clone())).unwrap().0, "P2");
    }

    #[test]
    fn slice2_vtable_groups_same_target_receivers() {
        // DBase.m; DMid inherits it; DSub overrides — the Top-typed site
        // has two distinct targets and {DBase, DMid} share one arm.
        let p = prog(vec![
            itf_plan("Deep", &[], &[]),
            impl_plan("DBase", None, &["Deep"], &[("m", MV)]),
            impl_plan("DMid", Some("DBase"), &[], &[]),
            impl_plan("DSub", Some("DMid"), &[], &[("m", MV)]),
        ]);
        assert!(resolve_virtual_target(&p, "Deep", "m", MV).is_none());
        let arms = resolve_vtable(&p, "Deep", "m", MV).unwrap();
        assert_eq!(arms.len(), 2);
        assert_eq!(
            arms[0].classes,
            vec!["DBase".to_string(), "DMid".to_string()]
        );
        assert_eq!(
            user_target(Some(arms[0].target.clone())).unwrap().0,
            "DBase"
        );
        assert_eq!(arms[1].classes, vec!["DSub".to_string()]);
        assert_eq!(user_target(Some(arms[1].target.clone())).unwrap().0, "DSub");
    }

    #[test]
    fn slice2_class_typed_polymorphic_site_gets_a_vtable() {
        // The DispatchOrder shape: A.m; B extends A and overrides — the
        // A-typed site gets the per-receiver vtable.
        let p = prog(vec![
            plan("A", Some("java/lang/Object"), &[("m", MV)]),
            plan("B", Some("A"), &[("m", MV)]),
        ]);
        assert!(resolve_virtual_target(&p, "A", "m", MV).is_none());
        let arms = resolve_vtable(&p, "A", "m", MV).unwrap();
        assert_eq!(arms.len(), 2);
        assert_eq!(arms[0].classes, vec!["A".to_string()]);
        assert_eq!(arms[1].classes, vec!["B".to_string()]);
    }

    #[test]
    fn slice2_vtable_fallback_covers_the_error_walk() {
        // One implementor's walk would die the internal Error (no m on its
        // chain): it gets NO arm; the fallback re-enters the dynamic walk.
        // Two healthy implementors still give a usable vtable.
        let p = prog(vec![
            itf_plan("I", &[], &[]),
            impl_plan("Ok1", None, &["I"], &[("m", MV)]),
            impl_plan("Ok2", None, &["I"], &[("m", MV)]),
            impl_plan("Dying", None, &["I"], &[]),
        ]);
        assert!(resolve_virtual_target(&p, "I", "m", MV).is_none());
        let arms = resolve_vtable(&p, "I", "m", MV).unwrap();
        assert_eq!(arms.len(), 2);
        for arm in &arms {
            assert!(!arm.classes.iter().any(|c| c == "Dying"));
        }
        // A receiver set with exactly ONE healthy group (the rest dying):
        // the match adds nothing — stay dynamic.
        let p2 = prog(vec![
            itf_plan("I2", &[], &[]),
            impl_plan("Only", None, &["I2"], &[("m", MV)]),
            impl_plan("Dying2", None, &["I2"], &[]),
        ]);
        assert!(resolve_vtable(&p2, "I2", "m", MV).is_none());
    }

    #[test]
    fn slice2_empty_and_builtin_interface_sites_stay_dynamic() {
        // Nobody implements the interface: no possible non-null receiver —
        // the site can only NPE (faithful already); never fold.
        let p = prog(vec![
            itf_plan("Lonely", &[], &[]),
            plan("Bystander", Some("java/lang/Object"), &[]),
        ]);
        assert!(resolve_virtual_target(&p, "Lonely", "m", MV).is_none());
        assert!(resolve_vtable(&p, "Lonely", "m", MV).is_none());
        // A builtin interface has an open receiver set (builtins implement
        // it; lambda closures carry its class) — fully dynamic.
        let p2 = prog(vec![plan("A", Some("java/lang/Object"), &[])]);
        assert!(resolve_virtual_target(
            &p2,
            "java/lang/Comparable",
            "compareTo",
            "(Ljava/lang/Object;)I"
        )
        .is_none());
        assert!(resolve_vtable(
            &p2,
            "java/lang/Comparable",
            "compareTo",
            "(Ljava/lang/Object;)I"
        )
        .is_none());
    }

    #[test]
    fn slice2_fold_beats_vtable_single_target() {
        // When the walk unifies, resolve_vtable must NOT plan a match —
        // the direct fold is the emission.
        let p = prog(vec![
            itf_plan("I", &[], &[]),
            impl_plan("Impl", None, &["I"], &[("m", MV)]),
        ]);
        assert!(resolve_virtual_target(&p, "I", "m", MV).is_some());
        assert!(resolve_vtable(&p, "I", "m", MV).is_none());
    }

    #[test]
    fn slice2_count_vtable_sites_tallies() {
        // Poly-typed site → 1 vtable site with 2 arms; the Impl-typed site
        // folds direct (not counted); the Poly2 site (single implementor)
        // folds too.
        let mut main = plan(
            "Main",
            Some("java/lang/Object"),
            &[("main", "([Ljava/lang/String;)V")],
        );
        main.methods[0].instrs = vec![
            (
                0,
                ir::Instr::InvokeInterface {
                    class: "Poly".to_string(),
                    name: "m".to_string(),
                    desc: MV.to_string(),
                },
            ),
            (
                3,
                ir::Instr::InvokeVirtual {
                    class: "P1".to_string(),
                    name: "m".to_string(),
                    desc: MV.to_string(),
                },
            ),
        ];
        let p = prog(vec![
            itf_plan("Poly", &[], &[]),
            impl_plan("P1", None, &["Poly"], &[("m", MV)]),
            impl_plan("P2", None, &["Poly"], &[("m", MV)]),
            main,
        ]);
        assert_eq!(count_vtable_sites(&p), (1, 2));
    }

    #[test]
    fn walk_that_would_die_the_error_stays_dynamic() {
        // A method the walk cannot find anywhere (no user impl, no native
        // row) — the runtime would throw the internal Error; the fold
        // never emulates that.
        let p = prog(vec![plan("A", Some("java/lang/Object"), &[])]);
        assert!(resolve_virtual_target(&p, "A", "no_such_method", MV).is_none());
    }

    #[test]
    fn count_virtual_sites_tallies_both_kinds() {
        // Two virtual sites. A/m is polymorphic (B extends A and overrides
        // m — stays dynamic); the B-typed interface site is monomorphic
        // (nothing extends B) — folds. Total 2, resolved 1.
        let mut a = plan(
            "A",
            Some("java/lang/Object"),
            &[("m", MV), ("main", "([Ljava/lang/String;)V")],
        );
        let mut b = plan("B", Some("A"), &[("m", MV)]);
        b.module = sanitize_module("B");
        // the helper's methods vec keeps the given order: [m, main]
        a.methods[1].instrs = vec![
            (
                0,
                ir::Instr::InvokeVirtual {
                    class: "A".to_string(),
                    name: "m".to_string(),
                    desc: MV.to_string(),
                },
            ),
            (
                3,
                ir::Instr::InvokeInterface {
                    class: "B".to_string(),
                    name: "m".to_string(),
                    desc: MV.to_string(),
                },
            ),
        ];
        let p = prog(vec![a, b]);
        assert_eq!(count_virtual_sites(&p), (2, 1));
    }
}
