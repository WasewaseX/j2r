//! M5 T04 — the partitioned-emission differential suite (docs/M5_PLAN.md
//! §2.1, docs/M5_SCALE.md §3): a fixed set of multi-class suite fixtures
//! compiled with `j2r test --partition-classes K` (a FORCED-tiny partition
//! budget, so every fixture lands in 2+ crates) and compared against the
//! JVM on all three channels by the same `j2r test` machinery the
//! 111-fixture differential suite uses.
//!
//! Why these fixtures: the partition seams are cross-partition direct
//! calls, cross-partition clinit fn pointers (`tramp`), the shared
//! statics pool, and the composite Dispatch. The set exercises each:
//! `Cycles` (mutual recursion across the web), `ForEachColl` +
//! `LambdaInCollections` (interface/virtual dispatch through the
//! composite lookup), `SingletonFactory` + `CyclicInit` (JLS 12.4.2
//! `<clinit>` gates — the clinit fn pointers cross partitions), and
//! `StringIoSemantics` (runtime natives + the shared pool).
//! `PartitionCatch` (external-B2/O5, jbox2d port report 2026-11-13):
//! exception TABLES in partitioned classes — try/catch + finally, a
//! `<clinit>` that catches (the World$1 shape), catch-type resolution
//! through the installed registry hook. Every earlier fixture was
//! try/catch-free and the class this covers was invisible to the set
//! (E0433: cannot find `registry` in the crate root).
//!
//! Skips silently when no JDK is available (CI installs one), exactly
//! like diff_suite.

use std::process::Command;

fn javac_available() -> bool {
    if let Ok(p) = std::env::var("J2R_JAVAC") {
        return std::path::Path::new(&p).is_file();
    }
    if which("javac").is_some() {
        return true;
    }
    if let Ok(entries) = std::fs::read_dir("/usr/lib/jvm") {
        for entry in entries.flatten() {
            if entry.path().join("bin").join("javac").is_file() {
                return true;
            }
        }
    }
    false
}

fn which(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var("PATH").ok()?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// The pinned partition fixtures and their forced per-crate class budget.
/// k=2 keeps every fixture at 2+ crates for its whole class web; a typo in
/// a stem must fail loud (the guard below), never select nothing.
const FIXTURES: &[(&str, usize)] = &[
    ("Cycles", 2),
    ("ForEachColl", 2),
    ("LambdaInCollections", 2),
    ("SingletonFactory", 2),
    ("CyclicInit", 2),
    ("StringIoSemantics", 2),
    ("PartitionCatch", 2),
    // EXTERNAL-HARNESS H-3 (2026-11-13): the B-wave's Externals joins the
    // partition runner — an exception-table-bearing class whose clinit
    // also catches (the World$1 shape) plus the JLS 12.4.2 launcher
    // ensure line through the PARTITIONED main template (the B2
    // registry-hook machinery carries it end-to-end).
    ("Externals", 2),
];

#[test]
fn partitioned_emission_matches_jvm() {
    let exe = env!("CARGO_BIN_EXE_j2r");
    if !javac_available() {
        eprintln!("skipping partition suite: javac not found");
        return;
    }
    let suite_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/suite");
    let mut failures = Vec::new();
    for (stem, k) in FIXTURES {
        let fixture = suite_dir.join(format!("{stem}.java"));
        assert!(
            fixture.is_file(),
            "partition fixture {stem} missing from the suite — fix the stem or add the fixture"
        );
        let out = Command::new(exe)
            .arg("test")
            .arg(&fixture)
            .arg("--partition-classes")
            .arg(k.to_string())
            .output()
            .expect("run j2r test");
        if !out.status.success() {
            failures.push(format!(
                "{stem} (k={k})\n--- stdout:\n{}\n--- stderr:\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        } else {
            eprintln!("PASS {stem} (k={k})");
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} partition fixtures failed:\n{}",
        failures.len(),
        FIXTURES.len(),
        failures.join("\n\n")
    );
}
