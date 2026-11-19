use super::*;

fn no_ctx() -> Dispatch {
    Dispatch::default()
}

fn boxed(class: &str, v: Value) -> JRef {
    let r = new_obj(class, 1).unwrap();
    object::obj_set(&r, 0, v);
    r
}

#[test]
fn integer_value_of_caches_minus128_to_127() {
    let _rt = crate::runtime();
    let f = |args: &[Value]| match n_integer_value_of(None, args, no_ctx()).unwrap() {
        Value::Ref(Some(r)) => r,
        other => panic!("expected a reference, got {other:?}"),
    };
    let a = f(&[Value::Int(127)]);
    let b = f(&[Value::Int(127)]);
    assert!(crate::ref_eq(&Value::Ref(Some(a)), &Value::Ref(Some(b))));
    let c = f(&[Value::Int(128)]);
    let d = f(&[Value::Int(128)]);
    assert!(!crate::ref_eq(&Value::Ref(Some(c)), &Value::Ref(Some(d))));
    let neg = f(&[Value::Int(-128)]);
    let neg2 = f(&[Value::Int(-128)]);
    assert!(crate::ref_eq(
        &Value::Ref(Some(neg)),
        &Value::Ref(Some(neg2))
    ));
}

#[test]
fn boxed_equals_compares_payload_and_type() {
    let _rt = crate::runtime();
    let i1 = boxed("java/lang/Integer", Value::Int(1000));
    let i2 = boxed("java/lang/Integer", Value::Int(1000));
    let r = n_integer_equals(Some(&i1), &[Value::Ref(Some(i2))], no_ctx()).unwrap();
    assert_eq!(r.expect_int(), 1);
    let l = boxed("java/lang/Long", Value::Long(1000));
    let r = n_integer_equals(Some(&i1), &[Value::Ref(Some(l))], no_ctx()).unwrap();
    // Integer(1000).equals(Long(1000)) is false in the JDK
    assert_eq!(r.expect_int(), 0);
    let i3 = boxed("java/lang/Integer", Value::Int(7));
    let r = n_integer_equals(Some(&i1), &[Value::Ref(Some(i3))], no_ctx()).unwrap();
    assert_eq!(r.expect_int(), 0);
}

#[test]
fn float_equals_uses_bits_nan_is_nan() {
    let _rt = crate::runtime();
    let nan: f32 = f32::NAN;
    let a = boxed("java/lang/Float", Value::Float(nan));
    let b = boxed("java/lang/Float", Value::Float(nan));
    let r = n_float_equals(Some(&a), &[Value::Ref(Some(b))], no_ctx()).unwrap();
    // Float.equals: NaN equals NaN
    assert_eq!(r.expect_int(), 1);
    let z = boxed("java/lang/Float", Value::Float(0.0));
    let nz = boxed("java/lang/Float", Value::Float(-0.0));
    let r = n_float_equals(Some(&z), &[Value::Ref(Some(nz))], no_ctx()).unwrap();
    // +0.0 and -0.0 are NOT equal under Float.equals
    assert_eq!(r.expect_int(), 0);
}

#[test]
fn boxed_hash_codes_match_jdk_contract() {
    let _rt = crate::runtime();
    let i = boxed("java/lang/Integer", Value::Int(42));
    assert_eq!(
        n_integer_hash_code(Some(&i), &[], no_ctx())
            .unwrap()
            .expect_int(),
        42
    );
    let l = boxed("java/lang/Long", Value::Long(0x1_0000_0042));
    // Long.hashCode: (int)(v ^ (v >>> 32))
    let v: i64 = 0x1_0000_0042;
    assert_eq!(
        n_long_hash_code(Some(&l), &[], no_ctx())
            .unwrap()
            .expect_int(),
        (v ^ (v >> 32)) as i32
    );
    let t = boxed("java/lang/Boolean", Value::Int(1));
    assert_eq!(
        n_boolean_hash_code(Some(&t), &[], no_ctx())
            .unwrap()
            .expect_int(),
        1231
    );
    let f = boxed("java/lang/Boolean", Value::Int(0));
    assert_eq!(
        n_boolean_hash_code(Some(&f), &[], no_ctx())
            .unwrap()
            .expect_int(),
        1237
    );
}

#[test]
fn lookup_finds_exact_entries() {
    let _rt = crate::runtime();
    assert!(lookup_exact("java/lang/Math", "max", "(II)I").is_some());
    assert!(lookup_exact("java/lang/Math", "max", "(IJ)I").is_none());
    assert!(lookup_exact("java/io/PrintStream", "println", "(Ljava/lang/String;)V").is_some());
}

#[test]
fn string_hash_code_matches_java() {
    let _rt = crate::runtime();
    let s = object::new_string("hello");
    let r = n_string_hash_code(Some(&s), &[], no_ctx()).unwrap();
    assert_eq!(r.expect_int(), 99162322);
    let e = object::new_string("");
    assert_eq!(
        n_string_hash_code(Some(&e), &[], no_ctx())
            .unwrap()
            .expect_int(),
        0
    );
}

#[test]
fn utf16_length_semantics() {
    let _rt = crate::runtime();
    // U+1F600 is one code point, two UTF-16 units — Java length() == 2
    let s = object::new_string("\u{1F600}");
    assert_eq!(
        n_string_length(Some(&s), &[], no_ctx())
            .unwrap()
            .expect_int(),
        2
    );
}

#[test]
fn parse_int_java_semantics() {
    let _rt = crate::runtime();
    let s = object::new_string("42");
    assert_eq!(
        n_integer_parse_int(None, &[Value::Ref(Some(s))], no_ctx())
            .unwrap()
            .expect_int(),
        42
    );
    let neg = object::new_string("-2147483648");
    assert_eq!(
        n_integer_parse_int(None, &[Value::Ref(Some(neg))], no_ctx())
            .unwrap()
            .expect_int(),
        i32::MIN
    );
    let over = object::new_string("2147483648");
    let err = n_integer_parse_int(None, &[Value::Ref(Some(over))], no_ctx()).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/NumberFormatException");
    let empty = object::new_string("");
    assert!(n_integer_parse_int(None, &[Value::Ref(Some(empty))], no_ctx()).is_err());
    let plus = object::new_string("+7");
    assert_eq!(
        n_integer_parse_int(None, &[Value::Ref(Some(plus))], no_ctx())
            .unwrap()
            .expect_int(),
        7
    );
}

#[test]
fn math_max_nan_and_zero_signs() {
    let _rt = crate::runtime();
    let nan = f64::NAN;
    assert!(java_max_d(nan, 1.0).is_nan());
    assert_eq!(java_max_d(0.0, -0.0), 0.0f64.copysign(1.0));
    assert_eq!(java_min_d(0.0, -0.0).to_bits(), (-0.0f64).to_bits());
}

#[test]
fn boxed_roundtrip() {
    let _rt = crate::runtime();
    let b = n_integer_value_of(None, &[Value::Int(5)], no_ctx()).unwrap();
    let r = b.expect_ref().unwrap();
    assert_eq!(
        crate::heap::with_obj(r, |o| o.jclass.to_string()),
        "java/lang/Integer"
    );
    assert_eq!(
        n_integer_int_value(Some(&r), &[], no_ctx())
            .unwrap()
            .expect_int(),
        5
    );
}

#[test]
fn sb_append_and_to_string() {
    let _rt = crate::runtime();
    let sb = object::new_builtin("java/lang/StringBuilder").unwrap();
    n_sb_init(Some(&sb), &[], no_ctx()).unwrap();
    n_sb_append_i(Some(&sb), &[Value::Int(12)], no_ctx()).unwrap();
    n_sb_append_s(
        Some(&sb),
        &[Value::Ref(Some(object::new_string(" x")))],
        no_ctx(),
    )
    .unwrap();
    let out = n_sb_to_string(Some(&sb), &[], no_ctx()).unwrap();
    assert_eq!(&*object::string_value(&out.expect_ref().unwrap()), "12 x");
}

#[test]
fn println_writes_to_stdout() {
    let _rt = crate::runtime();
    let ps = object::system_out_ref();
    n_ps_println_s(
        Some(&ps),
        &[Value::Ref(Some(object::new_string(
            "natives println works",
        )))],
        no_ctx(),
    )
    .unwrap();
}

#[test]
fn throwable_to_string_format() {
    let _rt = crate::runtime();
    let t = throw_new("pkg/MyError", Some("boom"));
    let out = n_throwable_to_string(Some(&t.obj), &[], no_ctx()).unwrap();
    assert_eq!(
        &*object::string_value(&out.expect_ref().unwrap()),
        "pkg.MyError: boom"
    );
}
