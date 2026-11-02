//! Differential test suite: every fixture in `tests/suite/*.java` is
//! compiled with javac, run with java, compiled with j2r, run natively, and
//! stdout, stderr and the exit status must all match (stderr with JVM
//! stack-trace frames normalized away — see j2r-cli/src/compare.rs).
//!
//! Skips silently when no JDK is available (CI installs one).

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
    for dir in path.split(':') {
        let candidate = std::path::Path::new(dir).join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[test]
fn differential_suite_matches_jvm() {
    let exe = env!("CARGO_BIN_EXE_j2r");
    if !javac_available() {
        eprintln!("skipping differential suite: javac not found");
        return;
    }
    let suite_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/suite");
    let mut fixtures: Vec<_> = std::fs::read_dir(&suite_dir)
        .expect("suite directory")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().map(|e| e == "java").unwrap_or(false))
        .collect();
    fixtures.sort();
    assert!(
        !fixtures.is_empty(),
        "no fixtures found in {}",
        suite_dir.display()
    );
    // The suite must keep growing with the milestone; 51 = the 39 fixtures
    // through M2 (36 Wave-1 + Cycles/GcIdentity/InternAcrossGc) plus the
    // nine M3 Wave-2 collection/lang fixtures (ArrayListOps, HashMapOrder,
    // HashSetOps, ObjectsNPE, StringExt, StringBuilderExt, MathExact,
    // IntegerParse, CharClassify) and the three interface-path fixtures
    // (ForEachColl, ListAsList, MapBoxKeys).
    assert!(
        fixtures.len() >= 51,
        "differential suite shrank: {} fixtures (expected >= 51)",
        fixtures.len()
    );

    let mut failures = Vec::new();
    for fixture in &fixtures {
        let out = Command::new(exe)
            .arg("test")
            .arg(fixture)
            .output()
            .expect("run j2r test");
        if !out.status.success() {
            failures.push(format!(
                "{}\n--- stdout:\n{}\n--- stderr:\n{}",
                fixture.display(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        } else {
            eprintln!("PASS {}", fixture.display());
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} fixtures failed:\n{}",
        failures.len(),
        fixtures.len(),
        failures.join("\n\n")
    );
}
