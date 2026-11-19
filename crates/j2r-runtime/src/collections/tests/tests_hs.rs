use super::testsupport::*;
use super::*;
#[test]
fn hashset_add_contains_remove_tostring() {
    let _rt = crate::runtime();
    let st = set_new();
    assert_eq!(
        n_hs_add(Some(&st), &[boxed_i32(4)], ctx())
            .unwrap()
            .expect_int(),
        1
    );
    assert_eq!(
        n_hs_add(Some(&st), &[boxed_i32(4)], ctx())
            .unwrap()
            .expect_int(),
        0,
        "duplicate add returns false"
    );
    assert_eq!(
        n_hs_add(Some(&st), &[Value::Ref(None)], ctx())
            .unwrap()
            .expect_int(),
        1
    );
    assert_eq!(n_hs_size(Some(&st), &[], ctx()).unwrap().expect_int(), 2);
    assert_eq!(
        s(&n_hs_to_string(Some(&st), &[], ctx()).unwrap()),
        "[null, 4]"
    );
    assert_eq!(
        n_hs_contains(Some(&st), &[boxed_i32(4)], ctx())
            .unwrap()
            .expect_int(),
        1
    );
    assert_eq!(
        n_hs_remove(Some(&st), &[boxed_i32(4)], ctx())
            .unwrap()
            .expect_int(),
        1
    );
    assert_eq!(
        n_hs_remove(Some(&st), &[boxed_i32(99)], ctx())
            .unwrap()
            .expect_int(),
        0
    );
    assert_eq!(s(&n_hs_to_string(Some(&st), &[], ctx()).unwrap()), "[null]");
}

// ---- Objects ----
