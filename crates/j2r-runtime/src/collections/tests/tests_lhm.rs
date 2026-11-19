use super::testsupport::*;
use super::*;
#[test]
fn lhm_order_laws_insertion_refixture_remove_tail() {
    let _rt = crate::runtime();
    let m = lhm_new();
    for (k, v) in [("a", 1), ("b", 2), ("c", 3)] {
        n_lhm_put(
            Some(&m),
            &[Value::Ref(Some(new_string(k))), boxed_i32(v)],
            ctx(),
        )
        .unwrap();
    }
    // L1: insertion order
    assert_eq!(
        s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
        "{a=1, b=2, c=3}"
    );
    // L2: re-put replaces the value and KEEPS position
    n_lhm_put(
        Some(&m),
        &[Value::Ref(Some(new_string("a"))), boxed_i32(10)],
        ctx(),
    )
    .unwrap();
    assert_eq!(
        s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
        "{a=10, b=2, c=3}"
    );
    // L3: remove + re-put moves to the tail
    let old = n_lhm_remove(Some(&m), &[Value::Ref(Some(new_string("a")))], ctx()).unwrap();
    assert_eq!(unbox_i32(&old), 10);
    n_lhm_put(
        Some(&m),
        &[Value::Ref(Some(new_string("a"))), boxed_i32(11)],
        ctx(),
    )
    .unwrap();
    assert_eq!(
        s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
        "{b=2, c=3, a=11}"
    );
    // L4: get / containsKey never reorder (the inherited rows are
    // order-neutral; the order links untouched)
    let got = n_hm_get(Some(&m), &[Value::Ref(Some(new_string("b")))], ctx()).unwrap();
    assert_eq!(unbox_i32(&got), 2);
    n_hm_contains_key(Some(&m), &[Value::Ref(Some(new_string("c")))], ctx()).unwrap();
    assert_eq!(
        s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
        "{b=2, c=3, a=11}"
    );
}
#[test]
fn lhm_views_iterate_in_link_order() {
    let _rt = crate::runtime();
    let m = lhm_new();
    for (k, v) in [("a", 1), ("b", 2), ("c", 3)] {
        n_lhm_put(
            Some(&m),
            &[Value::Ref(Some(new_string(k))), boxed_i32(v)],
            ctx(),
        )
        .unwrap();
    }
    let ks = match n_lhm_key_set(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(k)) => k,
        _ => panic!("keySet"),
    };
    assert_eq!(
        s(&n_lks_to_string(Some(&ks), &[], ctx()).unwrap()),
        "[a, b, c]"
    );
    let vs = match n_lhm_values(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("values"),
    };
    assert_eq!(
        s(&n_lvs_to_string(Some(&vs), &[], ctx()).unwrap()),
        "[1, 2, 3]"
    );
    // the entry iterator walks the link chain; getKey/getValue work
    // through the $Entry node (the Node prefix layout)
    let es = match n_lhm_entry_set(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(e)) => e,
        _ => panic!("entrySet"),
    };
    let it = match n_les_iterator(Some(&es), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator"),
    };
    let mut out = String::new();
    while n_lei_has_next(Some(&it), &[], ctx()).unwrap().expect_int() == 1 {
        let e = n_lei_next(Some(&it), &[], ctx()).unwrap();
        let node = match e {
            Value::Ref(Some(n)) => n,
            _ => panic!("entry node"),
        };
        let k = n_node_get_key(Some(&node), &[], ctx()).unwrap();
        let v = n_node_get_value(Some(&node), &[], ctx()).unwrap();
        out.push_str(&format!("{}={};", s(&k), unbox_i32(&v)));
    }
    assert_eq!(out, "a=1;b=2;c=3;");
    // size/isEmpty/contains delegate through the inherited shapes
    assert_eq!(n_lks_size(Some(&ks), &[], ctx()).unwrap().expect_int(), 3);
    assert_eq!(
        n_lks_contains(Some(&ks), &[Value::Ref(Some(new_string("b")))], ctx())
            .unwrap()
            .expect_int(),
        1
    );
    assert_eq!(
        n_lvs_contains(Some(&vs), &[boxed_i32(3)], ctx())
            .unwrap()
            .expect_int(),
        1
    );
}
#[test]
fn lhm_iterator_remove_order_and_modcount_law() {
    let _rt = crate::runtime();
    let m = lhm_new();
    for (k, v) in [("a", 1), ("b", 2), ("c", 3)] {
        n_lhm_put(
            Some(&m),
            &[Value::Ref(Some(new_string(k))), boxed_i32(v)],
            ctx(),
        )
        .unwrap();
    }
    let es = match n_lhm_entry_set(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(e)) => e,
        _ => panic!("entrySet"),
    };
    let it = match n_les_iterator(Some(&es), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator"),
    };
    n_lei_next(Some(&it), &[], ctx()).unwrap();
    n_lhm_itr_remove(Some(&it), &[], ctx()).unwrap();
    // the head is gone, order intact for the rest
    assert_eq!(
        s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
        "{b=2, c=3}"
    );
    // continued iteration: no CME (expectedModCount re-synced)
    let e = n_lei_next(Some(&it), &[], ctx()).unwrap();
    let node = match e {
        Value::Ref(Some(n)) => n,
        _ => panic!("entry node"),
    };
    assert_eq!(s(&n_node_get_key(Some(&node), &[], ctx()).unwrap()), "b");
    // remove again right away is LEGAL (lastRet = b)
    n_lhm_itr_remove(Some(&it), &[], ctx()).unwrap();
    assert_eq!(s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()), "{c=3}");
    // remove WITHOUT a next = IllegalStateException (lastRet cleared)
    let err = n_lhm_itr_remove(Some(&it), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/IllegalStateException");
    // CME: a structural modification behind the iterator's back
    n_lhm_put(
        Some(&m),
        &[Value::Ref(Some(new_string("z"))), boxed_i32(9)],
        ctx(),
    )
    .unwrap();
    let err = n_lei_next(Some(&it), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), CME_CLASS);
}
#[test]
fn lhm_itr_remove_ise_before_cme_law() {
    let _rt = crate::runtime();
    // the probed LinkedHashMap combined law (Temurin case 5): a fresh
    // link iterator over a foreign-modified map, remove() without any
    // next() → ISE, NOT CME — the original CME-first order diverged
    // from Temurin and was corrected against the ItrOrderProbe oracle
    let m = lhm_new();
    for (k, v) in [("a", 1), ("b", 2)] {
        n_lhm_put(
            Some(&m),
            &[Value::Ref(Some(new_string(k))), boxed_i32(v)],
            ctx(),
        )
        .unwrap();
    }
    let es = match n_lhm_entry_set(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(e)) => e,
        _ => panic!("entrySet"),
    };
    let it = match n_les_iterator(Some(&es), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator"),
    };
    // foreign modification BEFORE any next()
    n_lhm_remove(Some(&m), &[Value::Ref(Some(new_string("a")))], ctx()).unwrap();
    let err = n_lhm_itr_remove(Some(&it), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/IllegalStateException");
    // lastRet set + foreign mod → CME (the ISE check passed, CME fires)
    let it2 = match n_les_iterator(Some(&es), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator 2"),
    };
    n_lei_next(Some(&it2), &[], ctx()).unwrap();
    n_lhm_remove(Some(&m), &[Value::Ref(Some(new_string("b")))], ctx()).unwrap();
    let err = n_lhm_itr_remove(Some(&it2), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), CME_CLASS);
}
#[test]
fn lhm_ctor_from_map_copies_source_order() {
    let _rt = crate::runtime();
    // LHM source: link order
    let src = lhm_new();
    for (k, v) in [("a", 1), ("b", 2), ("c", 3)] {
        n_lhm_put(
            Some(&src),
            &[Value::Ref(Some(new_string(k))), boxed_i32(v)],
            ctx(),
        )
        .unwrap();
    }
    let cp = new_obj("java/util/LinkedHashMap", 6).unwrap();
    n_lhm_init(Some(&cp), &[Value::Ref(Some(src))], ctx()).unwrap();
    assert_eq!(
        s(&n_lhm_to_string(Some(&cp), &[], ctx()).unwrap()),
        "{a=1, b=2, c=3}"
    );
    // HashMap source: the source's OWN table order
    let hm = map_new();
    for (k, v) in [("x", 1), ("y", 2), ("z", 3)] {
        n_hm_put(
            Some(&hm),
            &[Value::Ref(Some(new_string(k))), boxed_i32(v)],
            ctx(),
        )
        .unwrap();
    }
    let cp2 = new_obj("java/util/LinkedHashMap", 6).unwrap();
    n_lhm_init(Some(&cp2), &[Value::Ref(Some(hm))], ctx()).unwrap();
    // the law: the copy iterates in the SOURCE's table order
    let expected_entries = hm_all_entries(&hm).unwrap();
    let expected = format!(
        "{{{}}}",
        expected_entries
            .iter()
            .map(|(k, v)| format!("{}={}", s(k), unbox_i32(v)))
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert_eq!(
        s(&n_lhm_to_string(Some(&cp2), &[], ctx()).unwrap()),
        expected
    );
    assert_eq!(with_obj(cp2, |o| o.slots[HM_SIZE].expect_int()), 3);
}

// ------------------------------------------------------------------
// The HashMap copy-ctor laws (the registered copy-ctor gap CLOSED).
// Every expected order below is the Temurin 21.0.13+9 oracle captured
// in scripts/evidence/hm_copy_ctor/temurin_oracle_2026-11-17.txt
// (HashMapCopyProbe L1-L9) — the fixture is the three-channel check;
// these tests pin the internals.
/// L4b: the LHM ctor from a TREE source iterates the tree in KEY
/// order — the latent misread the shared source-storage law fixed
/// (the old `_` arm walked hash slots on a red-black object).
#[test]
fn lhm_ctor_from_tree_source_law() {
    let _rt = crate::runtime();
    let src = tm_new();
    for i in (0..10).rev() {
        n_tm_put(Some(&src), &[boxed_i32(i * 3), boxed_i32(i)], ctx()).unwrap();
    }
    let cp = new_obj("java/util/LinkedHashMap", 6).unwrap();
    n_lhm_init(Some(&cp), &[Value::Ref(Some(src))], ctx()).unwrap();
    // the L4b oracle line: the KEY order 0 3 6 9 12 15 18 21 24 27;
    // the values ride the source mapping key k*3 -> k.
    assert_eq!(
        s(&n_lhm_to_string(Some(&cp), &[], ctx()).unwrap()),
        "{0=0, 3=1, 6=2, 9=3, 12=4, 15=5, 18=6, 21=7, 24=8, 27=9}"
    );
}
#[test]
fn lhm_null_key_value_and_clear() {
    let _rt = crate::runtime();
    let m = lhm_new();
    n_lhm_put(
        Some(&m),
        &[Value::Ref(Some(new_string("a"))), boxed_i32(1)],
        ctx(),
    )
    .unwrap();
    // L9: one null key, null values
    n_lhm_put(Some(&m), &[Value::Ref(None), boxed_i32(0)], ctx()).unwrap();
    assert_eq!(
        s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
        "{a=1, null=0}"
    );
    n_lhm_put(
        Some(&m),
        &[Value::Ref(Some(new_string("n"))), Value::Ref(None)],
        ctx(),
    )
    .unwrap();
    assert_eq!(
        s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
        "{a=1, null=0, n=null}"
    );
    let old = n_lhm_remove(Some(&m), &[Value::Ref(None)], ctx()).unwrap();
    assert_eq!(unbox_i32(&old), 0);
    assert_eq!(
        s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()),
        "{a=1, n=null}"
    );
    // L10: clear resets everything; re-adds re-link at the tail
    n_lhm_clear(Some(&m), &[], ctx()).unwrap();
    assert_eq!(with_obj(m, |o| o.slots[HM_SIZE].expect_int()), 0);
    n_lhm_put(
        Some(&m),
        &[Value::Ref(Some(new_string("q"))), boxed_i32(7)],
        ctx(),
    )
    .unwrap();
    assert_eq!(s(&n_lhm_to_string(Some(&m), &[], ctx()).unwrap()), "{q=7}");
}
#[test]
fn lhm_is_a_hashmap_subtype_law() {
    // L6: the builtin tables carry the OpenJDK subclass shape
    assert_eq!(
        crate::builtin::builtin_parent_of("java/util/LinkedHashMap"),
        Some("java/util/HashMap")
    );
    assert_eq!(
        crate::builtin::builtin_parent_of("java/util/LinkedHashMap$Entry"),
        Some("java/util/HashMap$Node")
    );
    assert_eq!(
        crate::builtin::builtin_total_slots("java/util/LinkedHashMap"),
        Some(6)
    );
    let ifaces = crate::builtin::builtin_interfaces_of("java/util/LinkedHashMap");
    assert!(ifaces.contains(&"java/util/Map"));
    let entry_ifaces = crate::builtin::builtin_interfaces_of("java/util/LinkedHashMap$Entry");
    assert!(entry_ifaces.contains(&"java/util/Map$Entry"));
}
#[test]
fn lhm_get_default_and_contains_value_laws() {
    let _rt = crate::runtime();
    let m = lhm_new();
    for (k, v) in [("a", 1), ("b", 2)] {
        n_lhm_put(
            Some(&m),
            &[Value::Ref(Some(new_string(k))), boxed_i32(v)],
            ctx(),
        )
        .unwrap();
    }
    // getOrDefault/containsValue inherit the HashMap rows (order-neutral)
    let d = n_hm_get_or_default(
        Some(&m),
        &[Value::Ref(Some(new_string("b"))), boxed_i32(-1)],
        ctx(),
    )
    .unwrap();
    assert_eq!(unbox_i32(&d), 2);
    let d = n_hm_get_or_default(
        Some(&m),
        &[Value::Ref(Some(new_string("zz"))), boxed_i32(-1)],
        ctx(),
    )
    .unwrap();
    assert_eq!(unbox_i32(&d), -1);
    assert_eq!(
        n_hm_contains_value(Some(&m), &[boxed_i32(2)], ctx())
            .unwrap()
            .expect_int(),
        1
    );
    assert_eq!(
        n_hm_contains_value(Some(&m), &[boxed_i32(99)], ctx())
            .unwrap()
            .expect_int(),
        0
    );
}

// ---- HashSet ----
#[test]
fn lhm_put_prim_links_the_tail_and_materializes() {
    let _rt = crate::runtime();
    let m = lhm_new();
    // The specialized put on the LINK-CHAIN storage: the node kind
    // follows the map's runtime class (LinkedHashMap$Entry), the
    // tail links move (insertion order), the tag rides.
    hm_put_prim(&m, key("b"), Value::Int(2), ctx()).unwrap();
    hm_put_prim(&m, key("a"), Value::Int(1), ctx()).unwrap();
    hm_put_prim(&m, key("c"), Value::Int(3), ctx()).unwrap();
    // The nodes are LinkedHashMap$Entry objects with the tag slot.
    let node = hm_find(&m, &key("a"), ctx()).unwrap().expect("node");
    assert_eq!(
        with_obj(node, |o| o.jclass.to_string()),
        "java/util/LinkedHashMap$Entry",
        "the node kind follows the map's runtime class"
    );
    assert_eq!(
        with_obj(node, |o| o.slots[HM_NODE_TAG].clone()),
        Value::Int('I' as i32)
    );
    // Insertion order preserved with raw slots; values materialize.
    let ts = n_lhm_to_string(Some(&m), &[], ctx()).unwrap();
    assert_eq!(s(&ts), "{b=2, a=1, c=3}");
    // The faithful HashMap-law get materializes (the parent chain).
    let g1 = n_hm_get(Some(&m), &[key("a")], ctx()).unwrap();
    let Value::Ref(Some(b)) = &g1 else {
        panic!("LHM get must materialize: {g1:?}")
    };
    assert_eq!(with_obj(*b, |o| o.jclass.to_string()), "java/lang/Integer");
    // The fused get unwraps; the overwrite keeps position (no link
    // work) and the tail order survives.
    assert_eq!(
        hm_get_prim(&m, key("b"), "java/lang/Integer", ctx()).unwrap(),
        Value::Int(2)
    );
    hm_put_prim(&m, key("b"), Value::Int(22), ctx()).unwrap();
    let ts2 = n_lhm_to_string(Some(&m), &[], ctx()).unwrap();
    assert_eq!(s(&ts2), "{b=22, a=1, c=3}");
    // The remove unlink law: the old value materializes, the link
    // chain stays walkable.
    let removed = n_lhm_remove(Some(&m), &[key("a")], ctx()).unwrap();
    assert_eq!(unbox_i32(&removed), 1);
    let ts3 = n_lhm_to_string(Some(&m), &[], ctx()).unwrap();
    assert_eq!(s(&ts3), "{b=22, c=3}");
}

// ---- the fused get's unbox law (the faithful failure modes) ----
