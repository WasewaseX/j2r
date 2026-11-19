use super::testsupport::*;
use super::*;
#[test]
fn hashmap_order_is_table_order_with_tail_bins() {
    let _rt = crate::runtime();
    // Integer keys (hash = value). 14 distinct keys inserted in the
    // order 1, 17, 0, 2..=12: keys 1 and 17 collide in bin 1 of the
    // cap-16 table (17 tail-inserted after 1); the 13th put resizes to
    // cap 32 and the Java-8 split moves 17 to bin 17.
    let m = map_new();
    let mut keys = vec![1, 17, 0];
    keys.extend(2..=12);
    for k in &keys {
        n_hm_put(Some(&m), &[boxed_i32(*k), boxed_i32(*k * 10)], ctx()).unwrap();
    }
    let out = n_hm_to_string(Some(&m), &[], ctx()).unwrap();
    assert_eq!(s(&out), "{0=0, 1=10, 2=20, 3=30, 4=40, 5=50, 6=60, 7=70, 8=80, 9=90, 10=100, 11=110, 12=120, 17=170}");
}
#[test]
fn hashmap_put_replaces_and_returns_old_without_cme() {
    let _rt = crate::runtime();
    let m = map_new();
    let old = n_hm_put(Some(&m), &[boxed_i32(7), boxed_i32(1)], ctx()).unwrap();
    assert!(old.is_null(), "fresh put returns null");
    let old = n_hm_put(Some(&m), &[boxed_i32(7), boxed_i32(2)], ctx()).unwrap();
    assert_eq!(unbox_i32(&old), 1, "replace returns the old value");
    // replacing is NOT a structural modification: iteration still works
    let ks = match n_hm_key_set(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(k)) => k,
        _ => panic!("keySet"),
    };
    let it = match n_ks_iterator(Some(&ks), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator"),
    };
    assert_eq!(unbox_i32(&n_ki_next(Some(&it), &[], ctx()).unwrap()), 7);
    assert_eq!(
        n_ki_has_next(Some(&it), &[], ctx()).unwrap().expect_int(),
        0
    );
}
#[test]
fn hashmap_remove_and_views() {
    let _rt = crate::runtime();
    let m = map_new();
    for k in 0..5 {
        n_hm_put(Some(&m), &[boxed_i32(k), boxed_i32(k + 100)], ctx()).unwrap();
    }
    let old = n_hm_remove(Some(&m), &[boxed_i32(2)], ctx()).unwrap();
    assert_eq!(unbox_i32(&old), 102);
    assert_eq!(n_hm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 4);
    // keySet view: size, contains, toString (table order)
    let ks = match n_hm_key_set(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(k)) => k,
        _ => panic!("keySet"),
    };
    assert_eq!(n_ks_size(Some(&ks), &[], ctx()).unwrap().expect_int(), 4);
    assert_eq!(
        n_ks_contains(Some(&ks), &[boxed_i32(3)], ctx())
            .unwrap()
            .expect_int(),
        1
    );
    let out = n_ks_to_string(Some(&ks), &[], ctx()).unwrap();
    assert_eq!(s(&out), "[0, 1, 3, 4]");
    // values view: faithful table order
    let vs = match n_hm_values(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("values"),
    };
    let out = n_vs_to_string(Some(&vs), &[], ctx()).unwrap();
    assert_eq!(s(&out), "[100, 101, 103, 104]");
    // getOrDefault: present vs default
    let d = n_hm_get_or_default(Some(&m), &[boxed_i32(3), boxed_i32(-1)], ctx()).unwrap();
    assert_eq!(unbox_i32(&d), 103);
    let d = n_hm_get_or_default(Some(&m), &[boxed_i32(2), boxed_i32(-1)], ctx()).unwrap();
    assert_eq!(unbox_i32(&d), -1);
}
#[test]
fn hashmap_iterator_cme_hasnext_no_check_next_checks() {
    let _rt = crate::runtime();
    let m = map_new();
    for k in 0..3 {
        n_hm_put(Some(&m), &[boxed_i32(k), boxed_i32(k)], ctx()).unwrap();
    }
    let ks = match n_hm_key_set(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(k)) => k,
        _ => panic!("keySet"),
    };
    let it = match n_ks_iterator(Some(&ks), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator"),
    };
    assert_eq!(
        n_ki_has_next(Some(&it), &[], ctx()).unwrap().expect_int(),
        1
    );
    // structural modification: hasNext still true (OpenJDK semantics),
    // next throws CME
    n_hm_put(Some(&m), &[boxed_i32(50), boxed_i32(50)], ctx()).unwrap();
    assert_eq!(
        n_ki_has_next(Some(&it), &[], ctx()).unwrap().expect_int(),
        1
    );
    let err = n_ki_next(Some(&it), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), CME_CLASS);
}
#[test]
fn hashmap_null_key_and_value() {
    let _rt = crate::runtime();
    let m = map_new();
    n_hm_put(Some(&m), &[Value::Ref(None), Value::Ref(None)], ctx()).unwrap();
    assert_eq!(n_hm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 1);
    let v = n_hm_get(Some(&m), &[Value::Ref(None)], ctx()).unwrap();
    assert!(v.is_null());
    n_hm_put(Some(&m), &[Value::Ref(None), boxed_i32(9)], ctx()).unwrap();
    let v = n_hm_get(Some(&m), &[Value::Ref(None)], ctx()).unwrap();
    assert_eq!(unbox_i32(&v), 9);
    let out = n_hm_to_string(Some(&m), &[], ctx()).unwrap();
    assert_eq!(s(&out), "{null=9}");
}
#[test]
fn hashmap_initial_capacity_negative_is_java_exact() {
    let _rt = crate::runtime();
    let m = new_obj("java/util/HashMap", 4).unwrap();
    let err = n_hm_init(Some(&m), &[Value::Int(-3)], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/IllegalArgumentException");
    let msg = with_obj(err.obj, |o| o.slots[0].clone());
    assert_eq!(s(&msg), "Illegal initial capacity: -3");
}

// ---- LinkedHashMap (run 4: the ordered map) ----
#[test]
fn hash_iter_remove_order_and_modcount_law() {
    let _rt = crate::runtime();
    let m = map_new();
    for k in 0..3 {
        n_hm_put(Some(&m), &[boxed_i32(k), boxed_i32(k + 10)], ctx()).unwrap();
    }
    let ks = match n_hm_key_set(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(k)) => k,
        _ => panic!("keySet"),
    };
    let it = match n_ks_iterator(Some(&ks), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator"),
    };
    // remove the key == 2 node mid-walk; the walk continues (no CME)
    let mut removed = false;
    while n_ki_has_next(Some(&it), &[], ctx()).unwrap().expect_int() == 1 {
        let k = n_ki_next(Some(&it), &[], ctx()).unwrap();
        if unbox_i32(&k) == 2 {
            n_hi_remove(Some(&it), &[], ctx()).unwrap();
            removed = true;
        }
    }
    assert!(removed, "walked and removed key 2");
    assert_eq!(n_hm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 2);
    let ks2 = match n_hm_key_set(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(k)) => k,
        _ => panic!("keySet 2"),
    };
    assert_eq!(
        n_ks_contains(Some(&ks2), &[boxed_i32(2)], ctx())
            .unwrap()
            .expect_int(),
        0
    );
    // double remove: current cleared → ISE
    let err = n_hi_remove(Some(&it), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/IllegalStateException");
    // foreign modification behind the iterator's back, then next → CME
    n_hm_put(Some(&m), &[boxed_i32(9), boxed_i32(90)], ctx()).unwrap();
    let err = n_ki_next(Some(&it), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), CME_CLASS);
    // THE PROBED COMBINED LAW (Temurin case 1): a fresh iterator over a
    // foreign-modified map, remove() without ANY next() → ISE, NOT CME
    // (the ISE check precedes the CME check in HashIterator.remove)
    let ks3 = match n_hm_key_set(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(k)) => k,
        _ => panic!("keySet 3"),
    };
    let it3 = match n_ks_iterator(Some(&ks3), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator 3"),
    };
    let err = n_hi_remove(Some(&it3), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/IllegalStateException");
    // the CME branch is still reachable: current set + foreign mod
    let m4 = map_new();
    for k in 0..2 {
        n_hm_put(Some(&m4), &[boxed_i32(k), boxed_i32(k)], ctx()).unwrap();
    }
    let ks4 = match n_hm_key_set(Some(&m4), &[], ctx()).unwrap() {
        Value::Ref(Some(k)) => k,
        _ => panic!("keySet 4"),
    };
    let it4 = match n_ks_iterator(Some(&ks4), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator 4"),
    };
    n_ki_next(Some(&it4), &[], ctx()).unwrap();
    n_hm_remove(Some(&m4), &[boxed_i32(0)], ctx()).unwrap();
    let err = n_hi_remove(Some(&it4), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), CME_CLASS);
}
#[test]
fn hash_iter_remove_value_and_entry_law() {
    let _rt = crate::runtime();
    let m = map_new();
    for k in 0..3 {
        n_hm_put(Some(&m), &[boxed_i32(k), boxed_i32(k + 10)], ctx()).unwrap();
    }
    // value iterator remove: the whole node (key AND value) is removed
    let vs = match n_hm_values(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("values"),
    };
    let it = match n_vs_iterator(Some(&vs), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("value iterator"),
    };
    let v = n_vi_next(Some(&it), &[], ctx()).unwrap();
    n_hi_remove(Some(&it), &[], ctx()).unwrap();
    assert_eq!(n_hm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 2);
    // continued value iteration: no CME, two values remain
    let rest = n_vi_next(Some(&it), &[], ctx()).unwrap();
    let _ = unbox_i32(&rest);
    let _ = v;
    // entry iterator remove: next() returns the node, remove() drops it
    let es = match n_hm_entry_set(Some(&m), &[], ctx()).unwrap() {
        Value::Ref(Some(e)) => e,
        _ => panic!("entrySet"),
    };
    let eit = match n_hes_iterator(Some(&es), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("entry iterator"),
    };
    let entry = match n_ei_next(Some(&eit), &[], ctx()).unwrap() {
        Value::Ref(Some(n)) => n,
        _ => panic!("entry node"),
    };
    let key = n_node_get_key(Some(&entry), &[], ctx()).unwrap();
    n_hi_remove(Some(&eit), &[], ctx()).unwrap();
    assert_eq!(n_hm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 1);
    assert_eq!(
        n_hm_contains_key(Some(&m), &[key], ctx())
            .unwrap()
            .expect_int(),
        0
    );
    // entry iterator remove without next → ISE (fresh iterator)
    let eit2 = match n_hes_iterator(Some(&es), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("entry iterator 2"),
    };
    let err = n_hi_remove(Some(&eit2), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/IllegalStateException");
}
/// Copies a source's entries in the source's iteration order — the
/// shared helper law read through the three storages.
fn hm_copy_keys(cp: &JRef) -> Vec<i32> {
    hm_all_entries(cp)
        .unwrap()
        .into_iter()
        .map(|(k, _)| unbox_i32(&k))
        .collect()
}
/// The putMapEntries pre-size boundary: threshold =
/// tableSizeFor((int)((float)s/0.75f + 1.0f)) — 11 entries -> 16,
/// 13 entries -> 32; the table stays lazy until the first put.
#[test]
fn hm_ctor_from_map_presize_boundary_law() {
    let _rt = crate::runtime();
    let src11 = lhm_new();
    for i in 0..10 {
        n_lhm_put(Some(&src11), &[boxed_i32(i), boxed_i32(i)], ctx()).unwrap();
    }
    n_lhm_put(Some(&src11), &[boxed_i32(20), boxed_i32(400)], ctx()).unwrap();
    let cp = new_obj("java/util/HashMap", 4).unwrap();
    n_hm_init(Some(&cp), &[Value::Ref(Some(src11))], ctx()).unwrap();
    // after the copy the first put has allocated at the PRE-SIZED cap
    // (the threshold the put saw was 16; the post-put threshold is
    // 16*0.75 = 12 — hm_resize's law).
    let (_, size, thr, table) = hm_state(&cp);
    assert_eq!(size, 11);
    assert_eq!(thr, 12);
    assert_eq!(hm_table_len(table.as_ref().unwrap()), 16);
    let src13 = lhm_new();
    for i in 0..12 {
        n_lhm_put(Some(&src13), &[boxed_i32(i), boxed_i32(i)], ctx()).unwrap();
    }
    n_lhm_put(Some(&src13), &[boxed_i32(20), boxed_i32(400)], ctx()).unwrap();
    let cp2 = new_obj("java/util/HashMap", 4).unwrap();
    n_hm_init(Some(&cp2), &[Value::Ref(Some(src13))], ctx()).unwrap();
    let (_, size2, thr2, table2) = hm_state(&cp2);
    assert_eq!(size2, 13);
    assert_eq!(thr2, 24);
    assert_eq!(hm_table_len(table2.as_ref().unwrap()), 32);
}
/// L1/L3: the source's iteration order drives insertion, read
/// through the chain order (three keys colliding at every candidate
/// capacity; the LHM source inserts 16, 0, 32).
#[test]
fn hm_ctor_from_map_source_order_chain_law() {
    let _rt = crate::runtime();
    let src = lhm_new();
    for k in [16, 0, 32] {
        n_lhm_put(Some(&src), &[boxed_i32(k), boxed_i32(k * 10)], ctx()).unwrap();
    }
    let cp = new_obj("java/util/HashMap", 4).unwrap();
    n_hm_init(Some(&cp), &[Value::Ref(Some(src))], ctx()).unwrap();
    let keys = hm_copy_keys(&cp);
    assert_eq!(keys, vec![16, 0, 32]);
}
/// L4: the tree source inserts in SORTED order; the copy's cap-16
/// table order is Temurin's exactly (0 18 3 21 6 24 9 27 12 15).
#[test]
fn hm_ctor_from_tree_source_law() {
    let _rt = crate::runtime();
    let src = tm_new();
    for i in (0..10).rev() {
        n_tm_put(Some(&src), &[boxed_i32(i * 3), boxed_i32(i)], ctx()).unwrap();
    }
    let cp = new_obj("java/util/HashMap", 4).unwrap();
    n_hm_init(Some(&cp), &[Value::Ref(Some(src))], ctx()).unwrap();
    let keys = hm_copy_keys(&cp);
    assert_eq!(keys, vec![0, 18, 3, 21, 6, 24, 9, 27, 12, 15]);
}
/// L6: the null map NPE (putMapEntries reads m.size() first).
#[test]
fn hm_ctor_null_source_npe_law() {
    let _rt = crate::runtime();
    let m = new_obj("java/util/HashMap", 4).unwrap();
    let err = n_hm_init(Some(&m), &[Value::Ref(None)], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/NullPointerException");
}
/// L8: the empty source keeps the table lazy (threshold 0); the
/// first put allocates the default cap 16 (1 and 17 share bucket 1,
/// the chain order = insertion order).
#[test]
fn hm_ctor_empty_source_lazy_law() {
    let _rt = crate::runtime();
    let empty = map_new();
    let cp = new_obj("java/util/HashMap", 4).unwrap();
    n_hm_init(Some(&cp), &[Value::Ref(Some(empty))], ctx()).unwrap();
    let (_, _, thr, table) = hm_state(&cp);
    assert_eq!(thr, 0);
    assert!(table.is_none());
    n_hm_put(Some(&cp), &[boxed_i32(1), boxed_i32(11)], ctx()).unwrap();
    n_hm_put(Some(&cp), &[boxed_i32(17), boxed_i32(17)], ctx()).unwrap();
    let keys = hm_copy_keys(&cp);
    assert_eq!(keys, vec![1, 17]);
}
/// L9: the post-copy growth timing — threshold 12 after the copy of
/// the 11-entry source; the 12th entry does NOT resize (12 > 12 is
/// false, cap stays 16, key 100 joins key 4's chain behind key 20);
/// the 13th does (cap 32; the lo/hi split moves key 20 to its own
/// high bucket and preserves the [4, 100] chain order).
#[test]
fn hm_ctor_post_copy_growth_timing_law() {
    let _rt = crate::runtime();
    let src = lhm_new();
    for i in 0..10 {
        n_lhm_put(Some(&src), &[boxed_i32(i), boxed_i32(i)], ctx()).unwrap();
    }
    n_lhm_put(Some(&src), &[boxed_i32(20), boxed_i32(400)], ctx()).unwrap();
    let cp = new_obj("java/util/HashMap", 4).unwrap();
    n_hm_init(Some(&cp), &[Value::Ref(Some(src))], ctx()).unwrap();
    n_hm_put(Some(&cp), &[boxed_i32(100), boxed_i32(100)], ctx()).unwrap();
    let keys = hm_copy_keys(&cp);
    assert_eq!(keys, vec![0, 1, 2, 3, 4, 20, 100, 5, 6, 7, 8, 9]);
    let (_, _, _, table) = hm_state(&cp);
    assert_eq!(hm_table_len(table.as_ref().unwrap()), 16);
    n_hm_put(Some(&cp), &[boxed_i32(101), boxed_i32(101)], ctx()).unwrap();
    let keys = hm_copy_keys(&cp);
    assert_eq!(keys, vec![0, 1, 2, 3, 4, 100, 5, 101, 6, 7, 8, 9, 20]);
    let (_, _, _, table) = hm_state(&cp);
    assert_eq!(hm_table_len(table.as_ref().unwrap()), 32);
}
#[test]
fn prim_slot_materializes_with_identity_and_jls_cache() {
    let _rt = crate::runtime();
    let m = map_new();
    // The specialized put stores the RAW prim; the node is stamped 'I'.
    hm_put_prim(&m, key("n"), Value::Int(5), ctx()).unwrap();
    // Faithful get #1 materializes; get #2 returns the SAME ref.
    let v1 = n_hm_get(Some(&m), &[key("n")], ctx()).unwrap();
    let v2 = n_hm_get(Some(&m), &[key("n")], ctx()).unwrap();
    let (Value::Ref(Some(b1)), Value::Ref(Some(b2))) = (&v1, &v2) else {
        panic!("materialized gets must be refs: {v1:?} / {v2:?}")
    };
    assert_eq!(b1, b2, "get-twice-returns-the-same-ref (probe law L1)");
    assert_eq!(with_obj(*b1, |o| o.jclass.to_string()), "java/lang/Integer");
    assert_eq!(with_obj(*b1, |o| o.slots[0].clone()), Value::Int(5));
    // The JLS identity cache: the materialized box IS
    // Integer.valueOf(5)'s shared cache box (probe law L2).
    let cached = crate::reflect::box_by_desc(Value::Int(5), "I").unwrap();
    let Value::Ref(Some(cb)) = &cached else {
        panic!("cache box")
    };
    assert_eq!(b1, cb, "materialization routes through the valueOf natives");
    // The slot now holds the box (the cache); the specialized get's
    // fused-unbox path still returns the prim payload.
    let prim = hm_get_prim(&m, key("n"), "java/lang/Integer", ctx()).unwrap();
    assert_eq!(prim, Value::Int(5));
}
#[test]
fn double_slot_materializes_with_slot_identity() {
    let _rt = crate::runtime();
    let m = map_new();
    hm_put_prim(&m, key("a"), Value::Double(0.5), ctx()).unwrap();
    let v = n_hm_get(Some(&m), &[key("a")], ctx()).unwrap();
    let Value::Ref(Some(b)) = &v else {
        panic!("ref")
    };
    assert_eq!(with_obj(*b, |o| o.jclass.to_string()), "java/lang/Double");
    assert_eq!(with_obj(*b, |o| o.slots[0].clone()), Value::Double(0.5));
    // get-twice-same-ref holds for the never-cached pair too — the
    // SLOT cache, not a JLS cache.
    let v2 = n_hm_get(Some(&m), &[key("a")], ctx()).unwrap();
    let Value::Ref(Some(b2)) = &v2 else {
        panic!("ref")
    };
    assert_eq!(b, b2);
}
#[test]
fn faithful_put_over_prim_node_returns_materialized_old_value() {
    let _rt = crate::runtime();
    let m = map_new();
    hm_put_prim(&m, key("n"), Value::Int(7), ctx()).unwrap();
    // A faithful put over the prim node: Map.put returns the OLD
    // value materialized (the box), and the node re-stamps faithful.
    let fresh = boxed_i32(9);
    let old = n_hm_put(Some(&m), &[key("n"), fresh], ctx()).unwrap();
    assert_eq!(unbox_i32(&old), 7, "the old value escapes as the box");
    let Value::Ref(Some(ob)) = &old else {
        panic!("old value must be a box: {old:?}")
    };
    assert_eq!(with_obj(*ob, |o| o.jclass.to_string()), "java/lang/Integer");
    // And a subsequent faithful get sees the new box directly.
    let v = n_hm_get(Some(&m), &[key("n")], ctx()).unwrap();
    assert_eq!(unbox_i32(&v), 9);
}
#[test]
fn remove_and_to_string_materialize() {
    let _rt = crate::runtime();
    let m = map_new();
    hm_put_prim(&m, key("a"), Value::Double(0.5), ctx()).unwrap();
    hm_put_prim(&m, key("b"), Value::Double(1.5), ctx()).unwrap();
    // toString materializes every slot and matches Java's
    // "{a=0.5, b=1.5}" (the probe's run output).
    let ts = n_hm_to_string(Some(&m), &[], ctx()).unwrap();
    assert_eq!(s(&ts), "{a=0.5, b=1.5}");
    // remove returns the materialized box.
    let rem = n_hm_remove(Some(&m), &[key("a")], ctx()).unwrap();
    let Value::Ref(Some(rb)) = &rem else {
        panic!("removed value")
    };
    assert_eq!(with_obj(*rb, |o| o.jclass.to_string()), "java/lang/Double");
}
#[test]
fn entry_get_value_materializes() {
    let _rt = crate::runtime();
    let m = map_new();
    hm_put_prim(&m, key("a"), Value::Int(5), ctx()).unwrap();
    // entrySet().iterator().next().getValue() — the registered
    // design's named generic exit.
    let es = n_hm_entry_set(Some(&m), &[], ctx()).unwrap();
    let Value::Ref(Some(esr)) = &es else {
        panic!("entrySet")
    };
    let it = n_hes_iterator(Some(esr), &[], ctx()).unwrap();
    let Value::Ref(Some(itr)) = &it else {
        panic!("iterator")
    };
    let entry = n_ei_next(Some(itr), &[], ctx()).unwrap();
    let Value::Ref(Some(er)) = &entry else {
        panic!("entry")
    };
    let gv = n_node_get_value(Some(er), &[], ctx()).unwrap();
    assert_eq!(unbox_i32(&gv), 5);
    let Value::Ref(Some(b)) = &gv else {
        panic!("materialized entry value: {gv:?}")
    };
    assert_eq!(with_obj(*b, |o| o.jclass.to_string()), "java/lang/Integer");
}

// ---- G3.3 slice 4: the rewrites ride both new storages ----
fn msg(err: &Throw) -> String {
    let m = with_obj(err.obj, |o| o.slots[0].clone());
    s(&m)
}
#[test]
fn fused_get_throws_faithful_cce_on_tag_mismatch() {
    let _rt = crate::runtime();
    let m = map_new();
    hm_put_prim(&m, key("a"), Value::Double(0.5), ctx()).unwrap();
    // A get core compiled against Integer on a Double-prim slot: the
    // tag is the faithful checkcast's actual type.
    let err = hm_get_prim(&m, key("a"), "java/lang/Integer", ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/ClassCastException");
    assert_eq!(
        msg(&err),
        "class java.lang.Double cannot be cast to class java.lang.Integer"
    );
    // The matching expect unwraps.
    let ok = hm_get_prim(&m, key("a"), "java/lang/Double", ctx()).unwrap();
    assert_eq!(ok, Value::Double(0.5));
}
#[test]
fn fused_get_throws_faithful_cce_on_foreign_box() {
    let _rt = crate::runtime();
    let m = map_new();
    hm_put_prim(&m, key("a"), Value::Double(0.5), ctx()).unwrap();
    // A non-core put (unchecked generics) wrote an Integer BOX over
    // the prim slot: the faithful checkcast runs on the box.
    n_hm_put(Some(&m), &[key("a"), boxed_i32(9)], ctx()).unwrap();
    let err = hm_get_prim(&m, key("a"), "java/lang/Double", ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/ClassCastException");
    assert_eq!(
        msg(&err),
        "class java.lang.Integer cannot be cast to class java.lang.Double"
    );
    // The right box (same class as expect) unwraps through its slot.
    n_hm_put(Some(&m), &[key("a"), boxed_i32(9)], ctx()).unwrap();
    let ok = hm_get_prim(&m, key("a"), "java/lang/Integer", ctx()).unwrap();
    assert_eq!(ok, Value::Int(9));
}
#[test]
fn fused_get_throws_bare_npe_on_null_value() {
    let _rt = crate::runtime();
    let m = map_new();
    // A non-core put stored a null VALUE (javac: aconst_null; put).
    n_hm_put(Some(&m), &[key("a"), Value::Ref(None)], ctx()).unwrap();
    let err = hm_get_prim(&m, key("a"), "java/lang/Double", ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/NullPointerException");
    // The BARE law: no message.
    assert!(matches!(
        with_obj(err.obj, |o| o.slots[0].clone()),
        Value::Ref(None)
    ));
}
