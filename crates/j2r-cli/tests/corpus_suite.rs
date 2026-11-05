//! Corpus runner (real-world corpus wave, wave-order item 3): the
//! production-library corpus entries must stay output-identical to Temurin
//! on all three channels, permanently, as their own test — separate from
//! the synthetic 84-fixture suite guard.
//!
//! Corpus entries live in `corpus/` (sources + mechanical assembly, see
//! docs/CORPUS_WAVE.md). Each entry: javac, java, j2r compile, native run,
//! stdout/stderr/exit must match (stderr with JVM frames normalized).
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
    // common Linux JDK locations (Unix-only scan — audit A7)
    #[cfg(unix)]
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

#[test]
fn corpus_entries_match_jvm() {
    let exe = env!("CARGO_BIN_EXE_j2r");
    if !javac_available() {
        eprintln!("skipping corpus suite: javac not found");
        return;
    }
    // The JZlib entry's generated crate (~7.2k lines with giant switch
    // tables) SIGSEGVs rustc's default thread stack while expanding — a
    // 32 MiB rust worker stack fixes it (found locally, first corpus run).
    std::env::set_var("RUST_MIN_STACK", "33554432");
    let corpus_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let entries: Vec<String> = vec![
        "MinimalJsonCorpus.java".to_string(),
        "JZlibCorpus.java".to_string(),
    ];
    let mut failures = Vec::new();
    for entry in &entries {
        let file = corpus_dir.join(entry);
        let out = Command::new(exe)
            .arg("test")
            .arg(&file)
            .output()
            .expect("run j2r test");
        if !out.status.success() {
            failures.push(format!(
                "{}\n--- stdout:\n{}\n--- stderr:\n{}",
                file.display(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        } else {
            eprintln!("PASS {}", file.display());
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} corpus entries failed:\n{}",
        failures.len(),
        entries.len(),
        failures.join("\n")
    );
}
