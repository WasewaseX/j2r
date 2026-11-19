use super::testsupport::*;
use super::*;
#[test]
fn arraylist_add_get_set_remove_tostring() {
    let _rt = crate::runtime();
    let l = list_new();
    for k in 0..3 {
        list_add(&l, Value::Ref(Some(new_string(&format!("e{k}")))));
    }
    let out = n_al_to_string(Some(&l), &[], ctx()).unwrap();
    assert_eq!(s(&out), "[e0, e1, e2]");
    // set returns the old element
    let old = n_al_set(
        Some(&l),
        &[Value::Int(1), Value::Ref(Some(new_string("x")))],
        ctx(),
    )
    .unwrap();
    assert_eq!(s(&old), "e1");
    assert_eq!(
        s(&n_al_get(Some(&l), &[Value::Int(1)], ctx()).unwrap()),
        "x"
    );
    // remove(I) returns the removed element and shifts
    let rem = n_al_remove_at(Some(&l), &[Value::Int(0)], ctx()).unwrap();
    assert_eq!(s(&rem), "e0");
    assert_eq!(s(&n_al_to_string(Some(&l), &[], ctx()).unwrap()), "[x, e2]");
    // remove(Object) by dispatched equals
    let found = n_al_remove_obj(Some(&l), &[Value::Ref(Some(new_string("e2")))], ctx()).unwrap();
    assert_eq!(found.expect_int(), 1);
    assert_eq!(s(&n_al_to_string(Some(&l), &[], ctx()).unwrap()), "[x]");
}
#[test]
fn arraylist_bounds_messages_are_java_exact() {
    let _rt = crate::runtime();
    let l = list_new();
    list_add(&l, Value::Ref(Some(new_string("a"))));
    // get(-1) / get(size): Objects.checkIndex message
    let err = n_al_get(Some(&l), &[Value::Int(-1)], ctx()).unwrap_err();
    assert_eq!(
        s(&Value::Ref(Some(with_obj(err.obj, |o| o.slots[0]
            .clone()
            .expect_ref()
            .unwrap())))),
        "Index -1 out of bounds for length 1"
    );
    let err = n_al_get(Some(&l), &[Value::Int(1)], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/IndexOutOfBoundsException");
    // add(IL,E)V uses rangeCheckForAdd: "Index: i, Size: s"
    let err = n_al_add_at(Some(&l), &[Value::Int(5), Value::Ref(None)], ctx()).unwrap_err();
    let msg = with_obj(err.obj, |o| o.slots[0].clone());
    assert_eq!(s(&msg), "Index: 5, Size: 1");
}
#[test]
fn arraylist_iterator_cme_and_exhaustion() {
    let _rt = crate::runtime();
    let l = list_new();
    list_add(&l, Value::Ref(Some(new_string("a"))));
    list_add(&l, Value::Ref(Some(new_string("b"))));
    let it = match n_al_iterator(Some(&l), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator"),
    };
    assert_eq!(
        n_itr_has_next(Some(&it), &[], ctx()).unwrap().expect_int(),
        1
    );
    assert_eq!(s(&n_itr_next(Some(&it), &[], ctx()).unwrap()), "a");
    // structural modification through the list -> next() throws CME
    list_add(&l, Value::Ref(Some(new_string("c"))));
    let err = n_itr_next(Some(&it), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), CME_CLASS);
    // fresh iterator, exhaust it -> NoSuchElementException
    let it2 = match n_al_iterator(Some(&l), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator"),
    };
    while n_itr_has_next(Some(&it2), &[], ctx()).unwrap().expect_int() == 1 {
        n_itr_next(Some(&it2), &[], ctx()).unwrap();
    }
    let err = n_itr_next(Some(&it2), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/util/NoSuchElementException");
    // Iterator.remove() is Java-faithful: removes the last-returned
    // element, shrinks the list, and re-syncs modCount (the differential
    // oracle pinned this; see M3_EVIDENCE.md for the plan deviation)
    let it3 = match n_al_iterator(Some(&l), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator"),
    };
    let _first = n_itr_next(Some(&it3), &[], ctx()).unwrap();
    n_itr_remove(Some(&it3), &[], ctx()).unwrap();
    assert_eq!(
        n_al_size(Some(&l), &[], ctx()).unwrap().expect_int(),
        2,
        "remove shrinks the list"
    );
    // removal through the iterator is NOT a CME for this iterator; the
    // cursor was rewound to lastRet, so next() yields the element after
    // the removed one
    let second = n_itr_next(Some(&it3), &[], ctx()).unwrap();
    assert_eq!(s(&second), "b", "cursor rewound to lastRet");
    // remove() without any prior next() -> IllegalStateException
    // (message null); Java allows remove() after every next(), so a
    // fresh iterator with lastRet = -1 is required to hit it
    let it4 = match n_al_iterator(Some(&l), &[], ctx()).unwrap() {
        Value::Ref(Some(i)) => i,
        _ => panic!("iterator"),
    };
    let err = n_itr_remove(Some(&it4), &[], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/IllegalStateException");
}
#[test]
fn arraylist_copy_constructor_copies_then_diverges() {
    let _rt = crate::runtime();
    // source [a, b] -> copy; copy equals source; later source mutation
    // must not touch the copy (real copy semantics, corpus wave 1)
    let src = list_new();
    list_add(&src, Value::Ref(Some(new_string("a"))));
    list_add(&src, Value::Ref(Some(new_string("b"))));
    let cp = new_obj("java/util/ArrayList", 3).unwrap();
    n_al_init_from_collection(Some(&cp), &[Value::Ref(Some(src))], ctx()).unwrap();
    assert_eq!(n_al_size(Some(&cp), &[], ctx()).unwrap().expect_int(), 2);
    assert_eq!(
        s(&n_al_get(Some(&cp), &[Value::Int(0)], ctx()).unwrap()),
        "a"
    );
    assert_eq!(
        s(&n_al_get(Some(&cp), &[Value::Int(1)], ctx()).unwrap()),
        "b"
    );
    // mutate the source; the copy diverges
    list_add(&src, Value::Ref(Some(new_string("c"))));
    assert_eq!(
        n_al_size(Some(&cp), &[], ctx()).unwrap().expect_int(),
        2,
        "copy must not see later source appends"
    );
    assert_eq!(n_al_size(Some(&src), &[], ctx()).unwrap().expect_int(), 3);
    // null source -> NPE, like the JDK
    let cp2 = new_obj("java/util/ArrayList", 3).unwrap();
    let err = n_al_init_from_collection(Some(&cp2), &[Value::Ref(None)], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/NullPointerException");
}
#[test]
fn arraylist_copy_constructor_reads_through_unmodifiable_view() {
    let _rt = crate::runtime();
    // the copy-ctor must iterate through ANY collection impl — here the
    // Collections$UnmodifiableList view over an ArrayList — and the view
    // itself must reject writes with UnsupportedOperationException
    let src = list_new();
    list_add(&src, Value::Ref(Some(new_string("x"))));
    list_add(&src, Value::Ref(Some(new_string("y"))));
    let view = match crate::jdkutil::n_unmod_list(None, &[Value::Ref(Some(src))], ctx()).unwrap() {
        Value::Ref(Some(v)) => v,
        other => panic!("view: {other:?}"),
    };
    let cp = new_obj("java/util/ArrayList", 3).unwrap();
    n_al_init_from_collection(Some(&cp), &[Value::Ref(Some(view))], ctx()).unwrap();
    assert_eq!(n_al_size(Some(&cp), &[], ctx()).unwrap().expect_int(), 2);
    assert_eq!(
        s(&n_al_get(Some(&cp), &[Value::Int(1)], ctx()).unwrap()),
        "y"
    );
    // the view rejects writes (JDK wrapper semantics), reads still live
    let err = crate::jdkutil::n_unmod_add(Some(&view), &[Value::Ref(Some(new_string("z")))], ctx())
        .unwrap_err();
    assert_eq!(err.java_class(), "java/lang/UnsupportedOperationException");
    // the backing list is untouched by the rejected write
    assert_eq!(n_al_size(Some(&src), &[], ctx()).unwrap().expect_int(), 2);
}
#[test]
fn arraylist_clear_and_null_elements() {
    let _rt = crate::runtime();
    let l = list_new();
    list_add(&l, Value::Ref(None));
    list_add(&l, Value::Ref(Some(new_string("z"))));
    assert_eq!(
        s(&n_al_to_string(Some(&l), &[], ctx()).unwrap()),
        "[null, z]"
    );
    n_al_clear(Some(&l), &[], ctx()).unwrap();
    assert_eq!(n_al_size(Some(&l), &[], ctx()).unwrap().expect_int(), 0);
    assert_eq!(s(&n_al_to_string(Some(&l), &[], ctx()).unwrap()), "[]");
}

// ---- HashMap ----
