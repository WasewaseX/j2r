use super::*;

pub(crate) fn ctx() -> Dispatch {
    Dispatch::default()
}
pub(crate) fn boxed_i32(v: i32) -> Value {
    // a java/lang/Integer object with its payload in slot 0 (the same
    // layout the valueOf native produces)
    let b = new_obj("java/lang/Integer", 1).unwrap();
    with_obj_mut(b, |o| o.slots[0] = Value::Int(v));
    Value::Ref(Some(b))
}
pub(crate) fn list_new() -> JRef {
    let l = new_obj("java/util/ArrayList", 3).unwrap();
    n_al_init(Some(&l), &[], ctx()).unwrap();
    l
}
pub(crate) fn list_add(l: &JRef, v: Value) {
    n_al_add(Some(l), &[v], ctx()).unwrap();
}
pub(crate) fn map_new() -> JRef {
    let m = new_obj("java/util/HashMap", 4).unwrap();
    n_hm_init(Some(&m), &[], ctx()).unwrap();
    m
}
pub(crate) fn tm_new() -> JRef {
    let m = new_obj("java/util/TreeMap", 4).unwrap();
    n_tm_init(Some(&m), &[], ctx()).unwrap();
    m
}
pub(crate) fn set_new() -> JRef {
    let s = new_obj("java/util/HashSet", 1).unwrap();
    n_hs_init(Some(&s), &[], ctx()).unwrap();
    s
}
/// Unboxes a boxed Integer reference (collection results hold refs).
pub(crate) fn unbox_i32(v: &Value) -> i32 {
    match v {
        Value::Ref(Some(r)) => with_obj(*r, |o| o.slots[0].clone()).expect_int(),
        other => panic!("not a ref: {other:?}"),
    }
}
pub(crate) fn s(v: &Value) -> String {
    match v {
        Value::Ref(Some(r)) => string_value(r).to_string(),
        Value::Ref(None) => "null".to_string(),
        other => panic!("not a ref: {other:?}"),
    }
}
/// The detailMessage slot of a thrown object (the exact IAE messages
/// the narrowing matrix pins).
pub(crate) fn throw_message(t: &Throw) -> String {
    let v = crate::heap::with_obj(t.obj, |o| o.slots[0].clone());
    match v {
        Value::Ref(Some(r)) => string_value(&r).to_string(),
        _ => String::new(),
    }
}

// ---- tableSizeFor (OpenJDK edge behaviour) ----

pub(crate) fn key(s: &str) -> Value {
    Value::Ref(Some(new_string(s)))
}
pub(crate) fn lhm_new() -> JRef {
    let m = new_obj("java/util/LinkedHashMap", 6).unwrap();
    n_lhm_init(Some(&m), &[], ctx()).unwrap();
    m
}
