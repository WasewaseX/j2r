//! EXTERNAL-HARNESS H-1/H-2 (jbox2d port process findings, 2026-11-13):
//! the `j2r cfg-report --surface` one-page surface report.
//!
//! The owner's finding: each compile reports only the FIRST unresolved
//! reference, so mapping the missing natives surface took one ~10 s
//! compile per gap (B4-B12 = ten cycles). These pins prove the report
//! answers the whole question at once — using the compiler's OWN
//! resolution functions as the classifier, so it can never disagree
//! with a later real compile:
//!
//! * the clean surface of a B-wave-shaped probe reports 0 missing
//!   (exit 0);
//! * a probe that needs one genuinely-absent member (Math.asin — the
//!   fdlibm module carries sin/cos/tan/atan/atan2/hypot, deliberately
//!   NOT asin) names it in ONE pass and exits 1.
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

const PROBE: &str = "tests/surface_probe/SurfProbe.java";

#[test]
fn surface_report_names_every_gap_in_one_pass() {
    let exe = env!("CARGO_BIN_EXE_j2r");
    if !javac_available() {
        eprintln!("skipping surface report test: javac not found");
        return;
    }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let probe_src = manifest.join(PROBE);
    assert!(probe_src.is_file(), "probe source missing: {PROBE}");
    let work = std::env::temp_dir().join(format!("j2r-surface-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("work dir");

    // javac the probe (the removed-deprecated ctors warn — fine)
    let javac = std::env::var("J2R_JAVAC")
        .map(PathBuf::from)
        .unwrap_or_else(|_| which("javac").expect("javac"));
    let st = Command::new(&javac)
        .arg("-d")
        .arg(&work)
        .arg(&probe_src)
        .output()
        .expect("run javac");
    assert!(
        st.status.success(),
        "javac failed: {}",
        String::from_utf8_lossy(&st.stderr)
    );
    let probe_class = work.join("SurfProbe.class");
    assert!(probe_class.is_file());

    // H-1: the full surface mode on the CLEAN B-wave fixture — every
    // site marked ok, exit 0
    let externals_src = manifest.join("tests/suite/Externals.java");
    assert!(externals_src.is_file(), "the Externals fixture is missing");
    let st = Command::new(&javac)
        .arg("-d")
        .arg(&work)
        .arg(&externals_src)
        .output()
        .expect("run javac on Externals");
    assert!(
        st.status.success(),
        "javac failed: {}",
        String::from_utf8_lossy(&st.stderr)
    );
    let externals_class = work.join("Externals.class");
    let out = Command::new(exe)
        .arg("cfg-report")
        .arg(&externals_class)
        .arg("--surface")
        .output()
        .expect("run j2r cfg-report --surface");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "clean surface must exit 0:\n{stdout}");
    assert!(
        stdout.contains("j2r surface report: ") && stdout.contains("0 missing"),
        "the B-wave surface must be fully ok on this tree:\n{stdout}"
    );

    // H-1 diagnostics on the gap probe: header + per-site marks + exit 1
    let out = Command::new(exe)
        .arg("cfg-report")
        .arg(&probe_class)
        .arg("--surface")
        .output()
        .expect("run j2r cfg-report --surface");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(!out.status.success(), "missing>0 must exit 1:\n{stdout}");
    assert!(
        stdout.contains("MISSING java/lang/Math.asin(D)D"),
        "asin must be the named gap:\n{stdout}"
    );

    // H-2: the diagnostics mode — walk everything, name the ONE absent
    // member (Math.asin), exit 1. One page, not N compile cycles.
    let out = Command::new(exe)
        .arg("cfg-report")
        .arg(&probe_class)
        .arg("--surface")
        .arg("--diagnostics")
        .output()
        .expect("run j2r cfg-report --surface --diagnostics");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(!out.status.success(), "diagnostics must exit 1:\n{stdout}");
    assert!(
        stdout.contains("MISSING java/lang/Math.asin(D)D"),
        "asin must be the named gap:\n{stdout}"
    );
    assert_eq!(
        stdout.matches("MISSING").count(),
        1,
        "exactly one gap on this probe:\n{stdout}"
    );

    let _ = std::fs::remove_dir_all(&work);
}

use std::path::PathBuf;
