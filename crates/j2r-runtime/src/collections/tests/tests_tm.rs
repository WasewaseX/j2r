use super::testsupport::*;
use super::*;
#[test]
fn treemap_sorted_order_laws() {
    let _rt = crate::runtime();
    let m = tm_new();
    for (k, v) in [(42, "a"), (-7, "b"), (0, "c"), (100, "d"), (13, "e")] {
        n_tm_put(
            Some(&m),
            &[boxed_i32(k), Value::Ref(Some(new_string(v)))],
            ctx(),
        )
        .unwrap();
    }
    // T1: iteration in KEY order (the whole point)
    assert_eq!(
        s(&n_tm_to_string(Some(&m), &[], ctx()).unwrap()),
        "{-7=b, 0=c, 13=e, 42=a, 100=d}"
    );
    // T2: firstKey/lastKey
    assert_eq!(
        unbox_i32(&n_tm_first_key(Some(&m), &[], ctx()).unwrap()),
        -7
    );
    assert_eq!(
        unbox_i32(&n_tm_last_key(Some(&m), &[], ctx()).unwrap()),
        100
    );
    // T3: duplicate put replaces, keeps position, returns old
    let old = n_tm_put(
        Some(&m),
        &[boxed_i32(13), Value::Ref(Some(new_string("E13")))],
        ctx(),
    )
    .unwrap();
    assert_eq!(s(&old), "e");
    assert_eq!(
        s(&n_tm_get(Some(&m), &[boxed_i32(13)], ctx()).unwrap()),
        "E13"
    );
    assert_eq!(n_tm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 5);
    assert_eq!(
        s(&n_tm_to_string(Some(&m), &[], ctx()).unwrap()),
        "{-7=b, 0=c, 13=E13, 42=a, 100=d}"
    );
    // T4: remove returns old, absent -> null, order preserved
    let old = n_tm_remove(Some(&m), &[boxed_i32(0)], ctx()).unwrap();
    assert_eq!(s(&old), "c");
    let old = n_tm_remove(Some(&m), &[boxed_i32(999)], ctx()).unwrap();
    assert!(old.is_null());
    assert_eq!(n_tm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 4);
    let ks = match n_tm_key_set(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(k)) => k,
        _ => panic!("keySet"),
    };
    assert_eq!(
        s(&n_tks_to_string(Some(&ks), &[], ctx()).unwrap()),
        "[-7, 13, 42, 100]"
    );
}
#[test]
fn treemap_navigable_laws() {
    let _rt = crate::runtime();
    let m = tm_new();
    for (k, v) in [(42, "a"), (-7, "b"), (0, "c"), (100, "d"), (13, "e")] {
        n_tm_put(
            Some(&m),
            &[boxed_i32(k), Value::Ref(Some(new_string(v)))],
            ctx(),
        )
        .unwrap();
    }
    // T5: ceiling/floor accept exact; higher/lower are strict;
    // out-of-range -> null (the probed case-5 row)
    type TmNavFn = fn(Option<&JRef>, &[Value], Dispatch) -> Result<Value, Throw>;
    let nav = |f: TmNavFn, key: i32, want: Option<i32>, name: &str| {
        let got = f(Some(&m), &[boxed_i32(key)], ctx()).unwrap();
        match want {
            Some(w) => assert_eq!(unbox_i32(&got), w, "{name}"),
            None => assert!(got.is_null(), "{name}"),
        }
    };
    nav(n_tm_ceiling_key, 13, Some(13), "ceil13");
    nav(n_tm_floor_key, 13, Some(13), "floor13");
    nav(n_tm_ceiling_key, 14, Some(42), "ceil14");
    nav(n_tm_floor_key, 14, Some(13), "floor14");
    nav(n_tm_higher_key, 13, Some(42), "hi13");
    // the oracle's lo13 = -7 was AFTER case 4 removed key 0; here 0 is
    // present, so lower(13) = 0
    nav(n_tm_lower_key, 13, Some(0), "lo13");
    nav(n_tm_ceiling_key, 999, None, "ceil999");
    nav(n_tm_floor_key, -99, None, "floor-99");
    nav(n_tm_higher_key, 100, None, "hi100");
    nav(n_tm_lower_key, -7, None, "lo-7");
}
#[test]
fn treemap_comparator_and_exceptions_law() {
    let _rt = crate::runtime();
    // T6: the Comparator surface reorders everything (the probed
    // lambda `(x, y) -> y - x` — the unit test rides the builtin
    // Comparator.reverseOrder() singleton, the same Comparator walk)
    let d = new_obj("java/util/TreeMap", 4).unwrap();
    let cmp = match crate::jdkutil::n_comparator_reverse_order(None, &[], ctx()).unwrap() {
        Value::Ref(Some(c)) => c,
        _ => panic!("reverseOrder"),
    };
    n_tm_init_cmp(Some(&d), &[Value::Ref(Some(cmp))], ctx()).unwrap();
    for (k, v) in [(42, "a"), (-7, "b"), (0, "c"), (100, "d")] {
        n_tm_put(
            Some(&d),
            &[boxed_i32(k), Value::Ref(Some(new_string(v)))],
            ctx(),
        )
        .unwrap();
    }
    assert_eq!(
        s(&n_tm_to_string(Some(&d), &[], ctx()).unwrap()),
        "{100=d, 42=a, 0=c, -7=b}"
    );
    assert_eq!(
        unbox_i32(&n_tm_first_key(Some(&d), &[], ctx()).unwrap()),
        100
    );
    assert_eq!(unbox_i32(&n_tm_last_key(Some(&d), &[], ctx()).unwrap()), -7);
    // T7: put(null) under natural ordering -> NPE (even on an empty map)
    let n = tm_new();
    let err = n_tm_put(
        Some(&n),
        &[Value::Ref(None), Value::Ref(Some(new_string("x")))],
        ctx(),
    )
    .unwrap_err();
    assert_eq!(err.java_class(), "java/lang/NullPointerException");
    // T8: put(non-Comparable) -> ClassCastException
    let obj = new_obj("java/lang/Object", 0).unwrap();
    let err = n_tm_put(
        Some(&n),
        &[Value::Ref(Some(obj)), Value::Ref(Some(new_string("x")))],
        ctx(),
    )
    .unwrap_err();
    assert_eq!(err.java_class(), "java/lang/ClassCastException");
}
#[test]
fn treemap_ctor_from_map_law() {
    let _rt = crate::runtime();
    // T9: the copy ctor sorts regardless of the source's iteration order
    let src = map_new();
    for (k, v) in [(5, "v5"), (-3, "v-3"), (27, "v27")] {
        n_hm_put(
            Some(&src),
            &[boxed_i32(k), Value::Ref(Some(new_string(v)))],
            ctx(),
        )
        .unwrap();
    }
    let c = new_obj("java/util/TreeMap", 4).unwrap();
    n_tm_init_map(Some(&c), &[Value::Ref(Some(src))], ctx()).unwrap();
    assert_eq!(
        s(&n_tm_to_string(Some(&c), &[], ctx()).unwrap()),
        "{-3=v-3, 5=v5, 27=v27}"
    );
}
#[test]
fn treemap_iterator_remove_and_cme_laws() {
    let _rt = crate::runtime();
    let m = tm_new();
    for k in 1..=4 {
        n_tm_put(
            Some(&m),
            &[boxed_i32(k), Value::Ref(Some(new_string("v")))],
            ctx(),
        )
        .unwrap();
    }
    let ks = match n_tm_key_set(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(k)) => k,
        _ => panic!("keySet"),
    };
    let it = match n_tks_iterator(Some(&ks), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator"),
    };
    // T10: remove mid-walk — order intact, no CME (probed case 9)
    let mut walked: Vec<i32> = Vec::new();
    while n_tm_i_has_next(Some(&it), &[], ctx()).unwrap().expect_int() == 1 {
        let k = n_tki_next(Some(&it), &[], ctx()).unwrap();
        if unbox_i32(&k) == 2 {
            n_tm_i_remove(Some(&it), &[], ctx()).unwrap();
        } else {
            walked.push(unbox_i32(&k));
        }
    }
    assert_eq!(walked, vec![1, 3, 4]);
    assert_eq!(n_tm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 3);
    // double remove: next + remove (ok) + remove (ISE — current
    // cleared; probed case 3), on a FRESH iterator so the walk's
    // last-returned state does not leak in
    let ita = match n_tks_iterator(Some(&ks), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator a"),
    };
    n_tki_next(Some(&ita), &[], ctx()).unwrap();
    n_tm_i_remove(Some(&ita), &[], ctx()).unwrap();
    let err = n_tm_i_remove(Some(&ita), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/IllegalStateException");
    // remove without ANY next on a fresh iterator -> ISE
    let itb = match n_tks_iterator(Some(&ks), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator b"),
    };
    let err = n_tm_i_remove(Some(&itb), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/IllegalStateException");
    // fresh iterator + foreign modification + remove without next ->
    // ISE FIRST (the run-4 probed combined law, Temurin case 1/5)
    let it2 = match n_tks_iterator(Some(&ks), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator 2"),
    };
    n_tm_put(
        Some(&m),
        &[boxed_i32(99), Value::Ref(Some(new_string("z")))],
        ctx(),
    )
    .unwrap();
    let err = n_tm_i_remove(Some(&it2), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/IllegalStateException");
    // current set + foreign mod -> CME (the ISE check passed, CME fires)
    let it3 = match n_tks_iterator(Some(&ks), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator 3"),
    };
    n_tki_next(Some(&it3), &[], ctx()).unwrap();
    n_tm_put(
        Some(&m),
        &[boxed_i32(98), Value::Ref(Some(new_string("z")))],
        ctx(),
    )
    .unwrap();
    let err = n_tm_i_remove(Some(&it3), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), CME_CLASS);
    // next after a foreign modification -> CME (probed case 10)
    let it4 = match n_tks_iterator(Some(&ks), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator 4"),
    };
    n_tki_next(Some(&it4), &[], ctx()).unwrap();
    n_tm_put(
        Some(&m),
        &[boxed_i32(97), Value::Ref(Some(new_string("z")))],
        ctx(),
    )
    .unwrap();
    let err = n_tki_next(Some(&it4), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), CME_CLASS);
}
/// The red-black integrity check: BST bounds, no red parent with a red
/// child, identical black heights on every root-to-leaf path.
fn tm_check_rb(e: &JRef, lo: Option<i32>, hi: Option<i32>) -> i32 {
    let k = unbox_i32(&tm_entry_key(e));
    if let Some(lo) = lo {
        assert!(k > lo, "BST bound violated: {k} <= {lo}");
    }
    if let Some(hi) = hi {
        assert!(k < hi, "BST bound violated: {k} >= {hi}");
    }
    let color = tm_color(Some(e));
    let l = tm_child(e, TME_LEFT);
    let r = tm_child(e, TME_RIGHT);
    let lb = match l {
        Some(c) => {
            if color == TM_RED {
                assert_ne!(tm_color(Some(&c)), TM_RED, "red-red at {k}.left");
            }
            tm_check_rb(&c, lo, Some(k))
        }
        None => 1,
    };
    let rb = match r {
        Some(c) => {
            if color == TM_RED {
                assert_ne!(tm_color(Some(&c)), TM_RED, "red-red at {k}.right");
            }
            tm_check_rb(&c, Some(k), hi)
        }
        None => 1,
    };
    assert_eq!(lb, rb, "black-height mismatch at {k}");
    lb + (color == TM_BLACK) as i32
}
#[test]
fn treemap_red_black_integrity_law() {
    let _rt = crate::runtime();
    // the ascending 1..=64 rung: the worst case for the right spine,
    // every rotation + recolor shape exercised
    let m = tm_new();
    for k in 1..=64 {
        n_tm_put(
            Some(&m),
            &[boxed_i32(k), Value::Ref(Some(new_string("v")))],
            ctx(),
        )
        .unwrap();
        if let Some(r) = tm_root(&m) {
            tm_check_rb(&r, None, None);
        }
    }
    let root = tm_root(&m).expect("root after 64 inserts");
    assert_eq!(tm_color(Some(&root)), TM_BLACK, "the root is black");
    tm_check_rb(&root, None, None);
    // remove every even key (two-child + leaf deletes, the sentinel
    // fixup path), then re-prove integrity and order
    for k in (2..=64).step_by(2) {
        let old = n_tm_remove(Some(&m), &[boxed_i32(k)], ctx()).unwrap();
        assert!(!old.is_null());
        if let Some(r) = tm_root(&m) {
            tm_check_rb(&r, None, None);
        }
    }
    assert_eq!(n_tm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 32);
    let root = tm_root(&m).expect("root after the even-key deletes");
    tm_check_rb(&root, None, None);
    let mut keys = Vec::new();
    for (k, _) in tm_all_entries(&m).unwrap() {
        keys.push(unbox_i32(&k));
    }
    let want: Vec<i32> = (1..=64).filter(|k| k % 2 == 1).collect();
    assert_eq!(keys, want, "odd keys remain in key order");
    // a descending insert rung (the left-spine mirror)
    let d = tm_new();
    for k in (1..=50).rev() {
        n_tm_put(
            Some(&d),
            &[boxed_i32(k), Value::Ref(Some(new_string("v")))],
            ctx(),
        )
        .unwrap();
    }
    let root = tm_root(&d).expect("root after the descending rung");
    tm_check_rb(&root, None, None);
}

// --- run 6: the bounded views (the V-laws; probe scripts/evidence/
// treemap_views/, 35 Temurin cases) ---
#[test]
fn tm_put_prim_stores_raw_and_the_readers_materialize() {
    let _rt = crate::runtime();
    let m = tm_new();
    // The specialized put: the CLRS walk runs (size/modCount move),
    // the value slot holds the RAW prim, the node is stamped 'I'.
    tm_put_prim(&m, key("b"), Value::Int(2), ctx()).unwrap();
    tm_put_prim(&m, key("a"), Value::Int(1), ctx()).unwrap();
    assert_eq!(n_tm_size(Some(&m), &[], ctx()).unwrap(), Value::Int(2));
    let root = tm_root(&m).expect("root after two puts");
    let (v, tag) = with_obj(root, |o| {
        (o.slots[TME_VALUE].clone(), o.slots[TME_TAG].clone())
    });
    assert_eq!(v, Value::Int(2), "the value slot holds the RAW prim");
    assert_eq!(
        tag,
        Value::Int('I' as i32),
        "the node is stamped with the desc char"
    );
    // The faithful get materializes (and caches the box back).
    let g1 = n_tm_get(Some(&m), &[key("a")], ctx()).unwrap();
    let Value::Ref(Some(b)) = &g1 else {
        panic!("materialized get must be a ref: {g1:?}")
    };
    assert_eq!(with_obj(*b, |o| o.jclass.to_string()), "java/lang/Integer");
    let g2 = n_tm_get(Some(&m), &[key("a")], ctx()).unwrap();
    assert_eq!(g1, g2, "get-twice-returns-the-same-ref (probe law L1)");
    // toString + containsValue ride tm_all_entries (materialized).
    let ts = n_tm_to_string(Some(&m), &[], ctx()).unwrap();
    assert_eq!(s(&ts), "{a=1, b=2}", "sorted order, boxes materialized");
    assert_eq!(
        n_tm_contains_value(Some(&m), &[boxed_i32(1)], ctx()).unwrap(),
        Value::Int(1)
    );
    // getOrDefault materializes; the miss returns the default.
    assert_eq!(
        unbox_i32(&n_tm_get_or_default(Some(&m), &[key("b"), boxed_i32(9)], ctx()).unwrap()),
        2
    );
    assert_eq!(
        unbox_i32(&n_tm_get_or_default(Some(&m), &[key("zz"), boxed_i32(9)], ctx()).unwrap()),
        9
    );
}
#[test]
fn tm_put_overwrite_and_remove_returns_materialize() {
    let _rt = crate::runtime();
    let m = tm_new();
    tm_put_prim(&m, key("k"), Value::Long(7), ctx()).unwrap();
    // A FAITHFUL overwrite returns the OLD value — materialized out
    // of the raw slot (the Map.put contract).
    let old_box = new_obj("java/lang/Long", 1).unwrap();
    with_obj_mut(old_box, |o| o.slots[0] = Value::Long(7));
    let old = n_tm_put(Some(&m), &[key("k"), Value::Ref(Some(old_box))], ctx()).unwrap();
    let Value::Ref(Some(b)) = &old else {
        panic!("faithful overwrite must return the materialized box: {old:?}")
    };
    assert_eq!(with_obj(*b, |o| o.jclass.to_string()), "java/lang/Long");
    // The slot now holds the box under a CLEARED tag; a core get
    // still unwraps through the fused law's checkcast path.
    assert_eq!(
        tm_get_prim(&m, key("k"), "java/lang/Long", ctx()).unwrap(),
        Value::Long(7)
    );
    // remove materializes the old value too.
    tm_put_prim(&m, key("d"), Value::Long(9), ctx()).unwrap();
    let removed = n_tm_remove(Some(&m), &[key("d")], ctx()).unwrap();
    let Value::Ref(Some(rb)) = &removed else {
        panic!("remove must return the materialized box: {removed:?}")
    };
    assert_eq!(with_obj(*rb, |o| o.slots[0].clone()), Value::Long(9));
}
#[test]
fn tm_get_prim_failure_modes_are_the_fused_law() {
    let _rt = crate::runtime();
    let m = tm_new();
    // Missing key: the bare NPE (the null box reaching xValue()).
    let err = tm_get_prim(&m, key("no"), "java/lang/Integer", ctx()).unwrap_err();
    assert_eq!(
        with_obj(err.obj, |o| o.jclass.to_string()),
        "java/lang/NullPointerException"
    );
    // Tag mismatch: the CCE with the tag's class as the actual type.
    tm_put_prim(&m, key("i"), Value::Int(5), ctx()).unwrap();
    let err = tm_get_prim(&m, key("i"), "java/lang/Double", ctx()).unwrap_err();
    assert_eq!(
        with_obj(err.obj, |o| o.jclass.to_string()),
        "java/lang/ClassCastException"
    );
    // A faithful FOREIGN box: the same CCE (the checkcast law).
    let foreign = new_obj("java/lang/String", 1).unwrap();
    with_obj_mut(foreign, |o| o.slots[0] = Value::Ref(None));
    n_tm_put(Some(&m), &[key("s"), Value::Ref(Some(foreign))], ctx()).unwrap();
    let err = tm_get_prim(&m, key("s"), "java/lang/Integer", ctx()).unwrap_err();
    assert_eq!(
        with_obj(err.obj, |o| o.jclass.to_string()),
        "java/lang/ClassCastException"
    );
    // The self-compare law: a non-Comparable key throws CCE on the
    // FIRST put (compare(key,key), the addEntryToEmptyMap law).
    let obj = new_obj("java/lang/Object", 0).unwrap();
    let err = tm_put_prim(&m, Value::Ref(Some(obj)), Value::Int(1), ctx()).unwrap_err();
    assert_eq!(
        with_obj(err.obj, |o| o.jclass.to_string()),
        "java/lang/ClassCastException"
    );
}
#[test]
fn tm_delete_transplant_moves_the_tag_with_the_value() {
    let _rt = crate::runtime();
    let m = tm_new();
    // Three raw-prim entries: the root ("b") has two children, so
    // the delete walks the transplant-by-copy (the successor's kv
    // moves into the root slot) — the tag must ride or the surviving
    // raw slot becomes unnamed.
    tm_put_prim(&m, key("b"), Value::Int(2), ctx()).unwrap();
    tm_put_prim(&m, key("a"), Value::Int(1), ctx()).unwrap();
    tm_put_prim(&m, key("c"), Value::Int(3), ctx()).unwrap();
    // Faithful remove of the ROOT ("b"): the successor ("c") copies
    // up. The core get of "c" must still unwrap.
    assert_eq!(
        unbox_i32(&n_tm_remove(Some(&m), &[key("b")], ctx()).unwrap()),
        2
    );
    assert_eq!(
        tm_get_prim(&m, key("c"), "java/lang/Integer", ctx()).unwrap(),
        Value::Int(3)
    );
    let ts = n_tm_to_string(Some(&m), &[], ctx()).unwrap();
    assert_eq!(s(&ts), "{a=1, c=3}");
}
#[test]
fn tm_view_readers_ride_the_raw_slots() {
    let _rt = crate::runtime();
    let m = tm_new();
    tm_put_prim(&m, key("a"), Value::Int(1), ctx()).unwrap();
    tm_put_prim(&m, key("b"), Value::Int(2), ctx()).unwrap();
    tm_put_prim(&m, key("z"), Value::Int(26), ctx()).unwrap();
    // The bounded view (slice-4's "the views ride"): the view's get,
    // toString, containsValue and value iterator all materialize.
    let view = n_tm_sub_map(Some(&m), &[key("a"), key("z")], ctx()).unwrap();
    let Value::Ref(Some(v)) = &view else {
        panic!("subMap view")
    };
    let g = n_sms_get(Some(v), &[key("b")], ctx()).unwrap();
    let Value::Ref(Some(bg)) = &g else {
        panic!("view get must materialize: {g:?}")
    };
    assert_eq!(with_obj(*bg, |o| o.jclass.to_string()), "java/lang/Integer");
    let ts = n_sms_to_string(Some(v), &[], ctx()).unwrap();
    // The to-EXCLUSIVE range law (run 6): z sits OUTSIDE [a, z).
    assert_eq!(s(&ts), "{a=1, b=2}");
    assert_eq!(
        n_sms_contains_value(Some(v), &[boxed_i32(2)], ctx()).unwrap(),
        Value::Int(1)
    );
    // The SubMap value iterator materializes too.
    let it_v = sms_new_iterator(v, "java/util/TreeMap$SubMap$ValueIterator", ctx()).unwrap();
    let Value::Ref(Some(it)) = &it_v else {
        panic!("values iterator")
    };
    let first = n_smvi_next(Some(it), &[], ctx()).unwrap();
    let Value::Ref(Some(bf)) = &first else {
        panic!("view iterator must materialize: {first:?}")
    };
    assert_eq!(with_obj(*bf, |o| o.slots[0].clone()), Value::Int(1));
}
