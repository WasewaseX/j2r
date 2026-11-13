//! G3.3 slice 1 — the signature-pin layer against REAL javac-produced
//! classfiles. Env-gated: the test runs only when `J2R_SIG_PROBE_DIR`
//! points at a directory holding `default/SigProbe.class` (plain
//! `javac`) and `g/SigProbe.class` (`javac -g`) — the evidence script
//! `scripts/evidence/g3_3/run_signature_probe.sh` builds both with the
//! pinned Temurin javac 21.0.13. Without the variable the test prints a
//! note and passes — CI needs no JDK for this file.
//!
//! What it proves, per build:
//! * default flags — javac emits `Signature` attributes (field, method,
//!   class levels) but NO LocalVariableTypeTable: the recorded javac
//!   flag law the rewrite slices must respect (a harness-compiled
//!   fixture's generic LOCAL has no signature on disk; its pin comes
//!   from the valueOf/checkcast patterns, not the LVTT).
//! * `-g` — the LVTT carries the generic locals; the pin helpers read
//!   `Double`/`Integer` off them and refuse the raw and bounded-
//!   wildcard shapes.

use j2r_classfile::ClassFile;
use j2r_codegen::signature::{map_value_boxed, parse_method_signature, BoxedPrim};

#[test]
fn javac_sig_probe_pins() {
    let Ok(dir) = std::env::var("J2R_SIG_PROBE_DIR") else {
        eprintln!(
            "skipped: J2R_SIG_PROBE_DIR not set (run \
             scripts/evidence/g3_3/run_signature_probe.sh for the real-javac pass)"
        );
        return;
    };

    // ---- the default-flags build --------------------------------------
    let bytes = std::fs::read(format!("{dir}/default/SigProbe.class"))
        .unwrap_or_else(|e| panic!("read default build: {e}"));
    let cf = ClassFile::parse(&bytes).expect("javac output must parse");

    // 1. The FIELD pin: `static Map<String, Double> cache` — Signature
    //    attributes need no -g.
    let cache = cf.field("cache").expect("cache field present");
    let sig = cache
        .signature
        .as_deref()
        .expect("javac emits a field Signature");
    eprintln!("field cache: desc={} sig={sig}", cache.descriptor);
    assert_eq!(cache.descriptor, "Ljava/util/Map;");
    assert_eq!(map_value_boxed(sig), Some(BoxedPrim::Double));

    // 2. The METHOD-PARAMETER pin: `blend(Map<String, Double>, String)`.
    let blend = cf
        .method("blend", "(Ljava/util/Map;Ljava/lang/String;)D")
        .expect("blend present");
    let msig = blend
        .signature
        .as_deref()
        .expect("javac emits a method Signature");
    eprintln!("method blend: sig={msig}");
    let parsed = parse_method_signature(msig).expect("method signature parses");
    assert_eq!(parsed.params.len(), 2);
    let j2r_codegen::signature::SigType::Class { name, args } = &parsed.params[0] else {
        panic!("param 0 is a class type");
    };
    assert_eq!(name, "java/util/Map");
    assert_eq!(args.len(), 2);
    // The value argument is an exact `Ljava/lang/Double;` — the same pin
    // map_value_boxed reads off a field/local string, inspected directly
    // here because a method signature carries the param types inline.
    let j2r_codegen::signature::SigArg::In(inner) = &args[1] else {
        panic!("param 0 arg 1 is exact (no wildcard)");
    };
    let j2r_codegen::signature::SigType::Class {
        name: vname,
        args: vargs,
    } = inner
    else {
        panic!("value arg is a class type");
    };
    assert_eq!(vname, "java/lang/Double");
    assert!(vargs.is_empty());
    assert_eq!(BoxedPrim::boxed_prim_of(vname), Some(BoxedPrim::Double));

    // 3. THE JAVAC FLAG LAW: under default flags the Code attribute
    //    carries NO LocalVariableTypeTable at all — a harness-compiled
    //    fixture's generic local has its type arguments nowhere on disk.
    let main = cf
        .method("main", "([Ljava/lang/String;)V")
        .expect("main present");
    let code = main.code().expect("main has code");
    assert!(
        code.lvtt.is_empty(),
        "default javac must not emit a LocalVariableTypeTable"
    );

    // ---- the -g build ---------------------------------------------------
    let bytes = std::fs::read(format!("{dir}/g/SigProbe.class"))
        .unwrap_or_else(|e| panic!("read -g build: {e}"));
    let cf = ClassFile::parse(&bytes).expect("javac output must parse");
    let main = cf
        .method("main", "([Ljava/lang/String;)V")
        .expect("main present");
    let code = main.code().expect("main has code");

    // 4. The LOCAL pin via LVTT: `Map<String, Double> m` — the
    //    BoxingBoxedBench hot-loop shape.
    let lv_m = code
        .lvtt
        .iter()
        .find(|e| e.name == "m")
        .expect("-g javac records the generic local m in the LVTT");
    eprintln!("local m: slot={} sig={}", lv_m.index, lv_m.signature);
    assert_eq!(map_value_boxed(&lv_m.signature), Some(BoxedPrim::Double));

    // 5. The Integer local: the cached pair pins too.
    let lv_im = code
        .lvtt
        .iter()
        .find(|e| e.name == "im")
        .expect("-g javac records the generic local im");
    eprintln!("local im: sig={}", lv_im.signature);
    assert_eq!(map_value_boxed(&lv_im.signature), Some(BoxedPrim::Integer));

    // 6. The refusals: the bounded wildcard pins to NOTHING; the RAW
    //    `Map` local is not even IN the LVTT (javac records only generic
    //    references there — the raw local's erased shape has no entry).
    let lv_rm = code.lvtt.iter().find(|e| e.name == "rm");
    eprintln!(
        "local rm: {}",
        match lv_rm {
            Some(e) => format!("sig={}", e.signature),
            None => "absent from the LVTT (raw Map — javac's own law)".to_string(),
        }
    );
    assert!(
        lv_rm.is_none(),
        "raw Map must not appear in the LocalVariableTypeTable"
    );
    let lv_wm = code
        .lvtt
        .iter()
        .find(|e| e.name == "wm")
        .expect("-g javac records the wildcard local wm");
    eprintln!("local wm: sig={}", lv_wm.signature);
    assert_eq!(
        map_value_boxed(&lv_wm.signature),
        None,
        "bounded wildcard: conservative refusal"
    );
}
