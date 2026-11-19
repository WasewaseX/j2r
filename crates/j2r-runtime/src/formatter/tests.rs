use super::*;

#[test]
fn shortest_repr_shapes() {
    let _rt = crate::runtime();
    println!("2.675  => {}", crate::javafmt::double_to_string(2.675));
    println!("3.14159=> {}", crate::javafmt::double_to_string(3.14159));
    println!(
        "12345.6789 => {}",
        crate::javafmt::double_to_string(12345.6789)
    );
    println!("0.125  => {}", crate::javafmt::double_to_string(0.125));
    println!("%.2f 3.14159 => {}", render_f(3.14159, 2));
    println!("%.2f 2.675  => {}", render_f(2.675, 2));
    println!("%.0f 2.5     => {}", render_f(2.5, 0));
    println!("%.1e 12345.6789 => {}", render_e(12345.6789, 1));
    println!("%e 12345.6789  => {}", render_e(12345.6789, 6));
}

#[test]
fn render_f_pins() {
    let _rt = crate::runtime();
    assert_eq!(render_f(3.14, 6), "3.140000");
    assert_eq!(render_f(3.14159, 2), "3.14");
    assert_eq!(render_f(2.675, 2), "2.68");
    assert_eq!(render_f(0.125, 2), "0.13");
    assert_eq!(render_f(2.5, 0), "3");
    assert_eq!(render_f(-2.5, 0), "-3");
    assert_eq!(render_f(-0.005, 2), "-0.01");
    // B1 (external jbox2d port report): the scientific-repr band —
    // the old code destroyed every %f of a magnitude < 1e-3 or
    // >= 1e7 by treating the "E" tail as fraction digits
    assert_eq!(render_f(6.149717374e-4, 9), "0.000614972");
    assert_eq!(render_f(-6.149717374e-4, 9), "-0.000614972");
    assert_eq!(render_f(1.0e-10, 9), "0.000000000");
    assert_eq!(render_f(9.99e-5, 3), "0.000");
    assert_eq!(render_f(9.99e-5, 4), "0.0001");
    assert_eq!(render_f(1.2345678e7, 3), "12345678.000");
    assert_eq!(render_f(1.0e7, 1), "10000000.0");
    // rounding-carry re-split: the old fixed split_at(int_len)
    // printed an extra fraction digit (or truncated the integer
    // part at precision 0)
    assert_eq!(render_f(9.99, 1), "10.0");
    assert_eq!(render_f(9.99, 0), "10");
    assert_eq!(render_f(9.9999, 2), "10.00");
}

#[test]
fn render_e_pins() {
    let _rt = crate::runtime();
    assert_eq!(render_e(12345.6789, 6), "1.234568e+04");
    assert_eq!(render_e(12345.6789, 1), "1.2e+04");
    // -0.0 keeps its sign (FmtResidualProbe U9: %E of -0.0 is
    // "-0.000000E+00"; the old `v < 0.0` test dropped it)
    assert_eq!(render_e(-0.0, 6), "-0.000000e+00");
    // rounding carry bumps the exponent (A9: 9.99999999e5 -> 1E+06)
    assert_eq!(render_e(9.99999999e5, 6), "1.000000e+06");
}

#[test]
fn render_g_pins() {
    let _rt = crate::runtime();
    // FmtResidualProbe B-rows — LOWER-ENGINE level: the renderer emits
    // lowercase; %G's uppercase comes from the unified tail in render()
    // (the format-level law test pins the uppercase shapes)
    assert_eq!(render_g(1234.5678, None), "1234.57");
    assert_eq!(render_g(1e7, None), "1.00000e+07");
    assert_eq!(render_g(1e6, None), "1.00000e+06");
    assert_eq!(render_g(999999.0, None), "999999");
    assert_eq!(render_g(1e-5, None), "1.00000e-05");
    // NO trailing-zero stripping in the fixed branch (B6 — the C-%g
    // stripping law does NOT apply to the JVM)
    assert_eq!(render_g(1e-4, None), "0.000100000");
    assert_eq!(render_g(123.456, Some(2)), "1.2e+02");
    assert_eq!(render_g(12.34, Some(2)), "12");
    assert_eq!(render_g(0.0, None), "0.00000");
    assert_eq!(render_g(-1234.5678, None), "-1234.57");
    // the rounding carry re-decides the branch (S6/S7): 9.99 rounds to
    // 1.0e1 -> fixed "10"; 999.99 rounds to 1.00e3 -> stays scientific
    assert_eq!(render_g(9.99, Some(2)), "10");
    assert_eq!(render_g(999.99, Some(3)), "1.00e+03");
    // precision 0 is taken as 1 (U7); more corners
    assert_eq!(render_g(12345.0, Some(0)), "1e+04");
    assert_eq!(render_g(0.001, None), "0.00100000");
    assert_eq!(render_g(0.5, None), "0.500000");
    assert_eq!(render_g(1.0 / 3.0, Some(10)), "0.3333333333");
    assert_eq!(render_g(-0.0, None), "-0.00000");
    assert_eq!(render_g(123.456, Some(4)), "123.5");
}

#[test]
fn render_a_pins() {
    let _rt = crate::runtime();
    // FmtResidualProbe C-rows (Temurin 21.0.13, 2026-11-05)
    assert_eq!(render_a(1.0, None), "0x1.0p0");
    assert_eq!(render_a(3.14, None), "0x1.91eb851eb851fp1");
    assert_eq!(render_a(0.0, None), "0x0.0p0");
    assert_eq!(render_a(-0.0, None), "-0x0.0p0");
    assert_eq!(render_a(2.0, None), "0x1.0p1");
    assert_eq!(render_a(0.5, None), "0x1.0p-1");
    // MIN_VALUE: subnormal 0.x form, exp -1022
    assert_eq!(render_a(4.9e-324, None), "0x0.0000000000001p-1022");
    // subnormals strip trailing zero nibbles too (T1: 2^-1023)
    assert_eq!(render_a(f64::from_bits(1), None), "0x0.0000000000001p-1022");
    assert_eq!(render_a(f64::from_bits(1 << 51), None), "0x0.8p-1022");
    // precision: rounds HALF_UP on >= 8 (C13) and pads to exactly p
    // (U2/U3); precision 0 is taken as 1 (V2)
    assert_eq!(render_a(3.14, Some(2)), "0x1.92p1");
    assert_eq!(render_a(3.14, Some(1)), "0x1.9p1");
    assert_eq!(render_a(3.14, Some(0)), "0x1.9p1");
    assert_eq!(render_a(1.0, Some(13)), "0x1.0000000000000p0");
    assert_eq!(render_a(3.14, Some(14)), "0x1.91eb851eb851f0p1");
    // rounding carry renormalizes with e+1 (V1/V10/W7)
    assert_eq!(render_a(1.96875, Some(1)), "0x1.0p1"); // 0x1.F8p0
    assert_eq!(render_a(1.999755859375, Some(2)), "0x1.00p1"); // 0x1.FFFp0
                                                               // subnormal WITH precision renormalizes to the 1.x true-exp form (T3)
    assert_eq!(render_a(f64::from_bits(1 << 51), Some(3)), "0x1.000p-1023");
}

fn fmt(spec: &str, args: &[Value]) -> Result<String, Throw> {
    // the caller HOLDS the test runtime lock — never take it here (a
    // per-call guard drops between fmt() and msg_of(), letting libtest's
    // parallel tests race the heap / STW handshake from a "Parked" thread)
    format(spec, args, crate::Dispatch::default())
}

fn msg_of(t: &Throw) -> String {
    // throw_new stores detailMessage as a HEAP STRING ref. Read the
    // slot FIRST (lock released when with_obj returns), THEN follow
    // the ref — object::string_value takes the heap lock itself and
    // the Mutex is not re-entrant (nested with_obj deadlocks).
    let slot = crate::heap::with_obj(t.obj, |o| o.slots[0].clone());
    match slot {
        Value::Str(s) => s.to_string(),
        Value::Ref(Some(r)) => crate::object::string_value(&r).to_string(),
        _ => String::new(),
    }
}

fn law(spec: &str, args: &[Value]) -> String {
    msg_of(&fmt(spec, args).unwrap_err())
}

#[test]
fn format_upper_and_law_pins() {
    // ONE guard for the whole test — every fmt()/law() call below needs
    // the heap; dropping the lock between calls deadlocks the STW
    // handshake under libtest's parallel test threads
    let _rt = crate::runtime();
    // the uppercase-variant family (A/C/W rows on Temurin 21.0.13):
    // the WHOLE piece uppercases — INFINITY/NAN included
    assert_eq!(
        fmt("%E", &[Value::Double(12345.6789)]).unwrap(),
        "1.234568E+04"
    );
    assert_eq!(
        fmt("%E", &[Value::Double(f64::INFINITY)]).unwrap(),
        "INFINITY"
    );
    assert_eq!(fmt("%E", &[Value::Double(f64::NAN)]).unwrap(), "NAN");
    assert_eq!(fmt("%G", &[Value::Double(1234.5678)]).unwrap(), "1234.57");
    assert_eq!(
        fmt("%A", &[Value::Double(3.14)]).unwrap(),
        "0X1.91EB851EB851FP1"
    );
    assert_eq!(
        fmt("%A", &[Value::Double(f64::INFINITY)]).unwrap(),
        "INFINITY"
    );
    assert_eq!(fmt("%#X", &[Value::Int(255)]).unwrap(), "0XFF");
    assert_eq!(fmt("%B", &[Value::Int(1)]).unwrap(), "TRUE");
    assert_eq!(
        fmt("%H", &[Value::Str(std::sync::Arc::new("key".to_string()))]).unwrap(),
        "19E5F"
    );
    assert_eq!(fmt("%C", &[Value::Int('q' as i32)]).unwrap(), "Q");
    // null through every non-b conversion (E1-E7, U4), uppercased by
    // the upper variants (E8-E10, W10)
    for (spec, want) in [
        ("%f", "null"),
        ("%e", "null"),
        ("%g", "null"),
        ("%a", "null"),
        ("%d", "null"),
        ("%x", "null"),
        ("%c", "null"),
        ("%h", "null"),
        ("%s", "null"),
        ("%E", "NULL"),
        ("%G", "NULL"),
        ("%A", "NULL"),
        ("%S", "NULL"),
    ] {
        assert_eq!(fmt(spec, &[Value::Ref(None)]).unwrap(), want, "{spec}");
    }
    assert_eq!(fmt("%b", &[Value::Ref(None)]).unwrap(), "false");
    assert_eq!(fmt("%B", &[Value::Ref(None)]).unwrap(), "FALSE");
    // the float family does NOT widen integral types (U13a-d) — the
    // message uses the LOWERCASED conv for the upper variants (W8)
    assert_eq!(law("%f", &[Value::Int(42)]), "f != java.lang.Integer");
    assert_eq!(law("%e", &[Value::Long(42)]), "e != java.lang.Long");
    assert_eq!(law("%E", &[Value::Int(42)]), "e != java.lang.Integer");
    assert_eq!(law("%d", &[Value::Double(3.14)]), "d != java.lang.Double");
    // unknown conversions keep the original char (W5)
    assert_eq!(law("%Q", &[Value::Int(1)]), "Conversion = 'Q'");
    // parse-time flag laws, BEFORE argument resolution (S4)
    assert_eq!(law("%,x", &[Value::Int(255)]), "Conversion = x, Flags = ,");
    assert_eq!(law("%,x", &[Value::Ref(None)]), "Conversion = x, Flags = ,");
    assert_eq!(
        law("%,e", &[Value::Double(1.5)]),
        "Conversion = e, Flags = ,"
    );
    assert_eq!(law("%#d", &[Value::Int(5)]), "Conversion = d, Flags = #");
    assert_eq!(
        law("%#G", &[Value::Double(1.0)]),
        "Conversion = g, Flags = #"
    );
    // '-' without a width on a numeric conversion: MFW with the raw
    // spec (V3/U5) — before the flag check ("%-,x" has no width)
    assert_eq!(law("%-d", &[Value::Int(5)]), "%-d");
    // comma stays legal on d/f/g (X1/X7/F-rows)
    assert_eq!(
        fmt("%,f", &[Value::Double(1234567.891)]).unwrap(),
        "1,234,567.891000"
    );
    assert_eq!(fmt("%,g", &[Value::Double(1234.5)]).unwrap(), "1,234.50");
    assert_eq!(fmt("%,(d", &[Value::Int(-1234)]).unwrap(), "(1,234)");
    // %E zero-pad after the sign (A8)
    assert_eq!(
        fmt("%012.2E", &[Value::Double(-std::f64::consts::PI)]).unwrap(),
        "-0003.14E+00"
    );
}
