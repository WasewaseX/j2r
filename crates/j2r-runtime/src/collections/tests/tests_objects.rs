use super::testsupport::*;
use super::*;
#[test]
fn objects_require_non_null_overloads() {
    let _rt = crate::runtime();
    let v = Value::Ref(Some(new_string("ok")));
    assert!(matches!(
        n_objects_require_non_null(None, std::slice::from_ref(&v), ctx()).unwrap(),
        Value::Ref(_)
    ));
    let err = n_objects_require_non_null(None, &[Value::Ref(None)], ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/NullPointerException");
    // null message (no-arg NPE)
    let msg = with_obj(err.obj, |o| o.slots[0].clone());
    assert!(msg.is_null());
    let err = n_objects_require_non_null_msg(
        None,
        &[Value::Ref(None), Value::Ref(Some(new_string("was null")))],
        ctx(),
    )
    .unwrap_err();
    let msg = with_obj(err.obj, |o| o.slots[0].clone());
    assert_eq!(s(&msg), "was null");
}
#[test]
fn objects_to_string_is_null_and_hash() {
    let _rt = crate::runtime();
    let out = n_objects_to_string(None, &[Value::Ref(None)], ctx()).unwrap();
    assert_eq!(s(&out), "null");
    let out = n_objects_to_string(None, &[Value::Ref(Some(new_string("x")))], ctx()).unwrap();
    assert_eq!(s(&out), "x");
    assert_eq!(
        n_objects_hash_code(None, &[Value::Ref(None)], ctx())
            .unwrap()
            .expect_int(),
        0
    );
    assert_eq!(
        n_objects_is_null(None, &[Value::Ref(None)], ctx())
            .unwrap()
            .expect_int(),
        1
    );
    assert_eq!(
        n_objects_non_null(None, &[Value::Ref(Some(new_string("")))], ctx())
            .unwrap()
            .expect_int(),
        1
    );
}

// ---- GC integration (M3_PLAN §5.4) ----
