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
    // Platform separator via std (':' Unix, ';' Windows) — audit A7.
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
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
    // The suite must keep growing with the milestone; 75 = the 70 fixtures
    // through M3 Wave 5 (reflection) plus the five real-program-scale
    // fixtures (BigJson, BigHuffman, BigRpg, BigLang, BigCache — 300-500
    // line multi-class programs, not micro-probes). Standing policy from
    // the M3 review: every wave adds real-program fixtures too. M3.5 (audit
    // A2) added the four try-with-resources fixtures (TwrSuppress, TwrCaught,
    // TwrCause, TwrThree) — 82 on disk, guard one below that.
    // A3 added the MultiMain stem-preference fixture — 83 on disk, guard
    // one below that (the count itself is machine-checked by
    // scripts/check_doc_counts.py, audit A8).
    assert!(
        fixtures.len() >= 83,
        "differential suite shrank: {} fixtures (expected >= 83)",
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
