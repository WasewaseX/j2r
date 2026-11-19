use super::testsupport::*;
use super::*;
/// A {1=a,3=c,5=e,7=g,9=i} map plus the [3,7) view the probe rides.
fn tm_views_fixture() -> (JRef, JRef) {
    let m = tm_new();
    for (k, v) in [(1, "a"), (3, "c"), (5, "e"), (7, "g"), (9, "i")] {
        n_tm_put(
            Some(&m),
            &[boxed_i32(k), Value::Ref(Some(new_string(v)))],
            ctx(),
        )
        .unwrap();
    }
    let sub = match n_tm_sub_map(Some(&m), &[boxed_i32(3), boxed_i32(7)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("subMap view"),
    };
    (m, sub)
}
#[test]
fn submap_range_surface_law() {
    let _rt = crate::runtime();
    // V1/V2: contents (from-inclusive, to-EXCLUSIVE), computed size,
    // toString; the 4-arg inclusive variants; empty views
    let (m, sub) = tm_views_fixture();
    assert_eq!(
        s(&n_sms_to_string(Some(&sub), &[], ctx()).unwrap()),
        "{3=c, 5=e}"
    );
    assert_eq!(n_sms_size(Some(&sub), &[], ctx()).unwrap().expect_int(), 2);
    assert_eq!(
        n_sms_is_empty(Some(&sub), &[], ctx()).unwrap().expect_int(),
        0
    );
    // the absolute builders: headMap/tailMap + the four-arg flags
    let head = match n_tm_head_map(Some(&m), &[boxed_i32(5)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("headMap"),
    };
    assert_eq!(
        s(&n_sms_to_string(Some(&head), &[], ctx()).unwrap()),
        "{1=a, 3=c}"
    );
    let tail = match n_tm_tail_map(Some(&m), &[boxed_i32(5)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("tailMap"),
    };
    assert_eq!(
        s(&n_sms_to_string(Some(&tail), &[], ctx()).unwrap()),
        "{5=e, 7=g, 9=i}"
    );
    let ii = match n_tm_sub_map4(
        Some(&m),
        &[boxed_i32(3), Value::Int(1), boxed_i32(7), Value::Int(1)],
        ctx(),
    )
    .unwrap()
    {
        Value::Ref(Some(v)) => v,
        _ => panic!("subMap4"),
    };
    assert_eq!(
        s(&n_sms_to_string(Some(&ii), &[], ctx()).unwrap()),
        "{3=c, 5=e, 7=g}"
    );
    // V3: first/lastKey over the range; NSEE on an empty view
    assert_eq!(
        unbox_i32(&n_sms_first_key(Some(&sub), &[], ctx()).unwrap()),
        3
    );
    assert_eq!(
        unbox_i32(&n_sms_last_key(Some(&sub), &[], ctx()).unwrap()),
        5
    );
    let none = match n_tm_sub_map(Some(&m), &[boxed_i32(10), boxed_i32(20)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("empty view"),
    };
    assert_eq!(n_sms_size(Some(&none), &[], ctx()).unwrap().expect_int(), 0);
    assert_eq!(
        n_sms_is_empty(Some(&none), &[], ctx())
            .unwrap()
            .expect_int(),
        1
    );
    assert_eq!(s(&n_sms_to_string(Some(&none), &[], ctx()).unwrap()), "{}");
    assert_eq!(
        n_sms_first_key(Some(&none), &[], ctx())
            .unwrap_err()
            .java_class(),
        "java/util/NoSuchElementException"
    );
    // the c07c law: lastKey on an empty view whose lo is above every
    // key — Floor(hi) lands below lo and must fence to NSEE
    assert_eq!(
        n_sms_last_key(Some(&none), &[], ctx())
            .unwrap_err()
            .java_class(),
        "java/util/NoSuchElementException"
    );
}
#[test]
fn submap_range_gated_map_law() {
    let _rt = crate::runtime();
    // V4: reads silently miss out-of-range keys; put throws the exact
    // IAE; null keys NPE and non-Comparable keys CCE through the range
    // check
    let (_m, sub) = tm_views_fixture();
    let get = |k: i32| n_sms_get(Some(&sub), &[boxed_i32(k)], ctx()).unwrap();
    assert_eq!(s(&get(3)), "c");
    assert!(get(6).is_null());
    assert!(get(1).is_null(), "below the lo: silent null");
    assert!(get(7).is_null(), "at the exclusive hi: silent null");
    assert!(get(99).is_null(), "above the hi: silent null");
    let ck = |k: i32| n_sms_contains_key(Some(&sub), &[boxed_i32(k)], ctx()).unwrap();
    assert_eq!(ck(3).expect_int(), 1);
    assert_eq!(ck(1).expect_int(), 0);
    assert_eq!(ck(7).expect_int(), 0);
    let gd = n_sms_get_or_default(
        Some(&sub),
        &[boxed_i32(1), Value::Ref(Some(new_string("D")))],
        ctx(),
    )
    .unwrap();
    assert_eq!(s(&gd), "D");
    // remove: silent null out-of-range (the c34a shape even on an
    // empty view)
    let none = match n_tm_sub_map(Some(&_m), &[boxed_i32(10), boxed_i32(20)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("empty view"),
    };
    assert!(n_sms_remove(Some(&none), &[boxed_i32(1)], ctx())
        .unwrap()
        .is_null());
    // put: IAE "key out of range" below-lo, at-hi-exc, above-hi
    for k in [1, 7, 99] {
        let err = n_sms_put(
            Some(&sub),
            &[boxed_i32(k), Value::Ref(Some(new_string("x")))],
            ctx(),
        )
        .unwrap_err();
        assert_eq!(err.java_class(), "java/lang/IllegalArgumentException");
        assert_eq!(throw_message(&err), "key out of range");
    }
    // null key: NPE through the range compare (the c18 shape)
    let err = n_sms_put(
        Some(&sub),
        &[Value::Ref(None), Value::Ref(Some(new_string("x")))],
        ctx(),
    )
    .unwrap_err();
    assert_eq!(err.java_class(), "java/lang/NullPointerException");
    // non-Comparable key: CCE through the range compare (c19)
    let obj = new_obj("java/lang/Object", 0).unwrap();
    let err = n_sms_put(
        Some(&sub),
        &[Value::Ref(Some(obj)), Value::Ref(Some(new_string("x")))],
        ctx(),
    )
    .unwrap_err();
    assert_eq!(err.java_class(), "java/lang/ClassCastException");
}
#[test]
fn submap_narrowing_matrix_law() {
    let _rt = crate::runtime();
    // V7: the probed narrowing matrix — full inRange on the fromKey
    // side, inclusivity-blind strictly-above-hi on the toKey side, the
    // crossed check last; the absolute builders never range-check
    let (m, s37) = tm_views_fixture(); // s37: lo=3 inc, hi=7 EXC
    let (tail5, head7) = matrix_s37_window(m, s37);
    matrix_derived_windows(m, s37, tail5, head7);
    matrix_hi_inclusive(m, s37);
}
/// The narrowing-matrix scenarios on the S37 window (verbatim from
/// `submap_narrowing_matrix_law`, round-6 reshaping): the two probe
/// views first, then the a/c/e/f/g window laws. Returns the probes
/// for the derived-window group.
fn matrix_s37_window(m: JRef, s37: JRef) -> (JRef, JRef) {
    let tail5 = match n_tm_tail_map(Some(&m), &[boxed_i32(5)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("tail5"),
    };
    let head7 = match n_tm_head_map(Some(&m), &[boxed_i32(7)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("head7"),
    };
    // a. re-requesting the same bounds is LEGAL (toKey == exclusive hi
    //    passes the toKey-side gate)
    n_sms_sub_map(Some(&s37), &[boxed_i32(3), boxed_i32(7)], ctx()).unwrap();
    // c. below the lo: "fromKey out of range"
    assert_eq!(
        iae(n_sms_sub_map(
            Some(&s37),
            &[boxed_i32(2), boxed_i32(5)],
            ctx()
        )),
        "fromKey out of range"
    );
    // e. headMap ACCEPTS toKey == the exclusive hi (c16e) — the result
    //    is the same-range view [3inc, 7exc]
    let e = match n_sms_head_map(Some(&s37), &[boxed_i32(7)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("c16e"),
    };
    assert_eq!(
        s(&n_sms_to_string(Some(&e), &[], ctx()).unwrap()),
        "{3=c, 5=e}"
    );
    // f. tailMap REJECTS fromKey == the exclusive hi (the asymmetry)
    assert_eq!(
        iae(n_sms_tail_map(Some(&s37), &[boxed_i32(7)], ctx())),
        "fromKey out of range"
    );
    // g. headMap down to the inclusive lo: legal EMPTY view
    let g = match n_sms_head_map(Some(&s37), &[boxed_i32(3)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("c16g"),
    };
    assert_eq!(s(&n_sms_to_string(Some(&g), &[], ctx()).unwrap()), "{}");
    (tail5, head7)
}
/// The narrowing-matrix scenarios over the derived views (verbatim
/// continuation): i/j/k/o on the probes, l/mm/n on the absolute map
/// (chain), q/s/t/u/w/x on the window and the big view.
fn matrix_derived_windows(m: JRef, s37: JRef, tail5: JRef, head7: JRef) {
    // i. on an unbounded-hi view: headMap creates a new exclusive hi
    let i = match n_sms_head_map(Some(&tail5), &[boxed_i32(7)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("c16i"),
    };
    assert_eq!(s(&n_sms_to_string(Some(&i), &[], ctx()).unwrap()), "{5=e}");
    // j. on an exclusive-hi view: tailMap from that hi throws
    assert_eq!(
        iae(n_sms_tail_map(Some(&head7), &[boxed_i32(7)], ctx())),
        "fromKey out of range"
    );
    // k. above the exclusive hi on an unbounded-hi parent: accepted
    let k = match n_sms_tail_map(Some(&tail5), &[boxed_i32(9)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("c16k"),
    };
    assert_eq!(s(&n_sms_to_string(Some(&k), &[], ctx()).unwrap()), "{9=i}");
    // l. crossed bounds on the ABSOLUTE map: "fromKey > toKey"
    assert_eq!(
        iae(n_tm_sub_map(Some(&m), &[boxed_i32(9), boxed_i32(3)], ctx())),
        "fromKey > toKey"
    );
    // m. from == to on the absolute map: legal empty view
    let mm = match n_tm_sub_map(Some(&m), &[boxed_i32(5), boxed_i32(5)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("c16m"),
    };
    assert_eq!(s(&n_sms_to_string(Some(&mm), &[], ctx()).unwrap()), "{}");
    // n. chained twice
    let chain = match n_tm_sub_map(Some(&m), &[boxed_i32(3), boxed_i32(9)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("chain"),
    };
    let nn = match n_sms_sub_map(Some(&chain), &[boxed_i32(5), boxed_i32(7)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("c16n"),
    };
    assert_eq!(s(&n_sms_to_string(Some(&nn), &[], ctx()).unwrap()), "{5=e}");
    // o. no-intersection above: chained narrowing THROWS
    let o1 = match n_sms_head_map(Some(&tail5), &[boxed_i32(7)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("c16o parent"),
    };
    assert_eq!(
        iae(n_sms_tail_map(Some(&o1), &[boxed_i32(7)], ctx())),
        "fromKey out of range"
    );
    // q. crossed INSIDE a legal window: "fromKey > toKey" (c16q)
    assert_eq!(
        iae(n_sms_sub_map(
            Some(&chain),
            &[boxed_i32(8), boxed_i32(5)],
            ctx()
        )),
        "fromKey > toKey"
    );
    // s. subMap's fromKey == the exclusive hi: REJECTED (full inRange)
    assert_eq!(
        iae(n_sms_sub_map(
            Some(&s37),
            &[boxed_i32(7), boxed_i32(7)],
            ctx()
        )),
        "fromKey out of range"
    );
    // t. tailMap's fromKey above the hi: REJECTED
    let big = match n_tm_sub_map(Some(&m), &[boxed_i32(1), boxed_i32(9)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("big"),
    };
    assert_eq!(
        iae(n_sms_tail_map(Some(&big), &[boxed_i32(99)], ctx())),
        "fromKey out of range"
    );
    // u. headMap below the lo: "toKey out of range" (the full-tooLow
    //    half of the toKey-side gate)
    assert_eq!(
        iae(n_sms_head_map(Some(&s37), &[boxed_i32(2)], ctx())),
        "toKey out of range"
    );
    // w. chained subMap with toKey BELOW the lo: "toKey out of range"
    //    fires BEFORE the crossed check (c16w)
    assert_eq!(
        iae(n_sms_sub_map(
            Some(&big),
            &[boxed_i32(5), boxed_i32(0)],
            ctx()
        )),
        "toKey out of range"
    );
    // x. fromKey range precedes the crossed check (c16x)
    assert_eq!(
        iae(n_sms_sub_map(
            Some(&s37),
            &[boxed_i32(2), boxed_i32(1)],
            ctx()
        )),
        "fromKey out of range"
    );
}
/// The narrowing-matrix tail pair (verbatim continuation): the
/// HI-INCLUSIVE parent law (y) and the view comparator law (v).
fn matrix_hi_inclusive(m: JRef, s37: JRef) {
    // y. headMap == hi on a HI-INCLUSIVE parent: accepted (strictly-
    //    above-hi is inclusivity-blind)
    let inc_hi = match n_tm_sub_map4(
        Some(&m),
        &[boxed_i32(3), Value::Int(1), boxed_i32(7), Value::Int(1)],
        ctx(),
    )
    .unwrap()
    {
        Value::Ref(Some(v)) => v,
        _ => panic!("c16y parent"),
    };
    let y = match n_sms_head_map(Some(&inc_hi), &[boxed_i32(7)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("c16y"),
    };
    assert_eq!(
        s(&n_sms_to_string(Some(&y), &[], ctx()).unwrap()),
        "{3=c, 5=e}"
    );
    // v. the view's comparator: null for natural ordering (deref'd from
    //    the backing map — the c20 law)
    assert!(n_sms_comparator(Some(&s37), &[], ctx()).unwrap().is_null());
}
/// The IAE probe (the test's `iae` closure, verbatim): every
/// rejected narrowing must be an IllegalArgumentException.
fn iae(r: Result<Value, Throw>) -> String {
    let e = r.unwrap_err();
    assert_eq!(e.java_class(), "java/lang/IllegalArgumentException");
    throw_message(&e)
}
#[test]
fn submap_live_and_slice_clear_law() {
    let _rt = crate::runtime();
    // V5: live both ways; V6: slice-only clear
    let (m, sub) = tm_views_fixture();
    // put through the view at an interior key: visible in both
    n_sms_put(
        Some(&sub),
        &[boxed_i32(4), Value::Ref(Some(new_string("d4")))],
        ctx(),
    )
    .unwrap();
    assert_eq!(
        s(&n_sms_to_string(Some(&sub), &[], ctx()).unwrap()),
        "{3=c, 4=d4, 5=e}"
    );
    assert_eq!(
        s(&n_tm_to_string(Some(&m), &[], ctx()).unwrap()),
        "{1=a, 3=c, 4=d4, 5=e, 7=g, 9=i}"
    );
    n_sms_remove(Some(&sub), &[boxed_i32(4)], ctx()).unwrap();
    // parent put in-range appears in the view; out-of-range does not
    n_tm_put(
        Some(&m),
        &[boxed_i32(6), Value::Ref(Some(new_string("six")))],
        ctx(),
    )
    .unwrap();
    assert_eq!(
        s(&n_sms_to_string(Some(&sub), &[], ctx()).unwrap()),
        "{3=c, 5=e, 6=six}"
    );
    assert_eq!(
        n_sms_contains_value(Some(&sub), &[Value::Ref(Some(new_string("six")))], ctx())
            .unwrap()
            .expect_int(),
        1
    );
    assert_eq!(
        n_sms_contains_value(Some(&sub), &[Value::Ref(Some(new_string("a")))], ctx())
            .unwrap()
            .expect_int(),
        0
    );
    n_tm_remove(Some(&m), &[boxed_i32(6)], ctx()).unwrap();
    // V6: slice-only clear — [3,7) drains 3,5; 7 is the exclusive hi
    // and survives with 1 and 9
    n_sms_clear(Some(&sub), &[], ctx()).unwrap();
    assert_eq!(s(&n_sms_to_string(Some(&sub), &[], ctx()).unwrap()), "{}");
    assert_eq!(
        s(&n_tm_to_string(Some(&m), &[], ctx()).unwrap()),
        "{1=a, 7=g, 9=i}"
    );
    assert_eq!(n_tm_size(Some(&m), &[], ctx()).unwrap().expect_int(), 3);
    // the c35 shape: a 50-key map, clear [10,20), 40 survive
    let big = tm_new();
    for k in 0..50 {
        n_tm_put(Some(&big), &[boxed_i32(k), boxed_i32(k * k)], ctx()).unwrap();
    }
    let mid = match n_tm_sub_map(Some(&big), &[boxed_i32(10), boxed_i32(20)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("mid"),
    };
    assert_eq!(n_sms_size(Some(&mid), &[], ctx()).unwrap().expect_int(), 10);
    n_sms_clear(Some(&mid), &[], ctx()).unwrap();
    assert_eq!(n_sms_size(Some(&mid), &[], ctx()).unwrap().expect_int(), 0);
    assert_eq!(n_tm_size(Some(&big), &[], ctx()).unwrap().expect_int(), 40);
    let ck = |k: i32| {
        n_tm_contains_key(Some(&big), &[boxed_i32(k)], ctx())
            .unwrap()
            .expect_int()
    };
    assert_eq!(ck(10), 0);
    assert_eq!(ck(20), 1, "the exclusive hi survives");
    assert_eq!(ck(9), 1);
    assert_eq!(ck(21), 1);
}
#[test]
fn submap_views_and_iterators_law() {
    let _rt = crate::runtime();
    // V9: the view-of-view objects + the bounded iterators (order,
    // mid-walk remove, the hi fence)
    let (m, sub) = tm_views_fixture();
    let ks = match n_sms_key_set(Some(&sub), &[], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("keySet"),
    };
    let vs = match n_sms_values(Some(&sub), &[], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("values"),
    };
    let es = match n_sms_entry_set(Some(&sub), &[], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("entrySet"),
    };
    assert_eq!(
        s(&n_smsks_to_string(Some(&ks), &[], ctx()).unwrap()),
        "[3, 5]"
    );
    assert_eq!(
        s(&n_smsvs_to_string(Some(&vs), &[], ctx()).unwrap()),
        "[c, e]"
    );
    assert_eq!(
        s(&n_smses_to_string(Some(&es), &[], ctx()).unwrap()),
        "[3=c, 5=e]"
    );
    assert_eq!(n_smsks_size(Some(&ks), &[], ctx()).unwrap().expect_int(), 2);
    // contains rides the range gate
    assert_eq!(
        n_smsks_contains(Some(&ks), &[boxed_i32(3)], ctx())
            .unwrap()
            .expect_int(),
        1
    );
    assert_eq!(
        n_smsks_contains(Some(&ks), &[boxed_i32(1)], ctx())
            .unwrap()
            .expect_int(),
        0
    );
    // the bounded key iterator: mid-walk remove, order intact, no CME
    let it = match n_smsks_iterator(Some(&ks), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator"),
    };
    let mut walked = Vec::new();
    let mut removed = None;
    while n_sms_i_has_next(Some(&it), &[], ctx())
        .unwrap()
        .expect_int()
        == 1
    {
        let k = n_smki_next(Some(&it), &[], ctx()).unwrap();
        if unbox_i32(&k) == 5 {
            n_tm_i_remove(Some(&it), &[], ctx()).unwrap();
            removed = Some(k.clone());
        } else {
            walked.push(unbox_i32(&k));
        }
    }
    assert_eq!(walked, vec![3]);
    assert_eq!(unbox_i32(&removed.expect("removed")), 5);
    assert_eq!(
        s(&n_sms_to_string(Some(&sub), &[], ctx()).unwrap()),
        "{3=c}"
    );
    assert_eq!(
        s(&n_tm_to_string(Some(&m), &[], ctx()).unwrap()),
        "{1=a, 3=c, 7=g, 9=i}"
    );
    // the hi fence stops the walk: [5,9) over the restored map
    n_tm_put(
        Some(&m),
        &[boxed_i32(5), Value::Ref(Some(new_string("e")))],
        ctx(),
    )
    .unwrap();
    let tail = match n_tm_tail_map(Some(&m), &[boxed_i32(5)], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("tail"),
    };
    let tks = match n_sms_key_set(Some(&tail), &[], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("tail keySet"),
    };
    let it2 = match n_smsks_iterator(Some(&tks), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("tail iterator"),
    };
    let mut keys = Vec::new();
    while n_sms_i_has_next(Some(&it2), &[], ctx())
        .unwrap()
        .expect_int()
        == 1
    {
        keys.push(unbox_i32(&n_smki_next(Some(&it2), &[], ctx()).unwrap()));
    }
    assert_eq!(keys, vec![5, 7, 9]);
    // the bounded entry iterator's next IS the backing entry node (the
    // Map$Entry checkcast law carries)
    let es2 = match n_sms_entry_set(Some(&sub), &[], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("entrySet2"),
    };
    let it3 = match n_smses_iterator(Some(&es2), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("entry iterator"),
    };
    let node = match n_smei_next(Some(&it3), &[], ctx()).unwrap() {
        Value::Ref(Some(e)) => e,
        _ => panic!("entry node"),
    };
    assert_eq!(
        with_obj(node, |o| o.jclass.to_string()),
        "java/util/TreeMap$Entry"
    );
}
#[test]
fn submap_iterator_exception_laws() {
    let _rt = crate::runtime();
    // V8: ISE-first remove, double-remove ISE, exhausted NSEE, and the
    // probed c27 law — a foreign OUT-of-range put still comodifies
    let (m, sub) = tm_views_fixture();
    let ks = match n_sms_key_set(Some(&sub), &[], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        _ => panic!("keySet"),
    };
    // remove without next -> ISE (BEFORE any CME check)
    let it = match n_smsks_iterator(Some(&ks), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("it"),
    };
    assert_eq!(
        n_tm_i_remove(Some(&it), &[], ctx())
            .unwrap_err()
            .java_class(),
        "java/lang/IllegalStateException"
    );
    // next, then a foreign put through the PARENT inside the range -> CME
    n_smki_next(Some(&it), &[], ctx()).unwrap();
    n_tm_put(
        Some(&m),
        &[boxed_i32(6), Value::Ref(Some(new_string("x")))],
        ctx(),
    )
    .unwrap();
    assert_eq!(
        n_smki_next(Some(&it), &[], ctx()).unwrap_err().java_class(),
        CME_CLASS
    );
    n_tm_remove(Some(&m), &[boxed_i32(6)], ctx()).unwrap();
    // c27: a foreign put OUTSIDE the range STILL comodifies (the
    // expectedModCount is the backing map's)
    let it2 = match n_smsks_iterator(Some(&ks), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("it2"),
    };
    n_smki_next(Some(&it2), &[], ctx()).unwrap();
    n_tm_put(
        Some(&m),
        &[boxed_i32(99), Value::Ref(Some(new_string("zz")))],
        ctx(),
    )
    .unwrap();
    assert_eq!(
        n_smki_next(Some(&it2), &[], ctx())
            .unwrap_err()
            .java_class(),
        CME_CLASS
    );
    n_tm_remove(Some(&m), &[boxed_i32(99)], ctx()).unwrap();
    // double remove -> ISE; the exhausted walk -> NSEE
    let it3 = match n_smsks_iterator(Some(&ks), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("it3"),
    };
    n_smki_next(Some(&it3), &[], ctx()).unwrap();
    n_tm_i_remove(Some(&it3), &[], ctx()).unwrap();
    assert_eq!(
        n_tm_i_remove(Some(&it3), &[], ctx())
            .unwrap_err()
            .java_class(),
        "java/lang/IllegalStateException"
    );
    while n_sms_i_has_next(Some(&it3), &[], ctx())
        .unwrap()
        .expect_int()
        == 1
    {
        n_smki_next(Some(&it3), &[], ctx()).unwrap();
    }
    assert_eq!(
        n_smki_next(Some(&it3), &[], ctx())
            .unwrap_err()
            .java_class(),
        "java/util/NoSuchElementException"
    );
}
