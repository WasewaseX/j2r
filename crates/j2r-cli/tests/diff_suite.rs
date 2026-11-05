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
    // M4 Wave 2 added the real-thread fixtures (ThreadHello, ThreadSafepoint)
    // — 86 on disk, guard one below that.
    assert!(
        fixtures.len() >= 88,
        "differential suite shrank: {} fixtures (expected >= 88)",
        fixtures.len()
    );

    // Optional sharding (ops): J2R_DIFF_SHARD=i/N keeps only the fixtures
    // whose sorted index satisfies index % N == i. The corpus guard above
    // still asserts the full on-disk suite, so a shard can never silently
    // shrink the evidence. Unset (CI, default) = every fixture, unchanged.
    // Introduced to shard the gate-9 gc-stress run (a collection at every
    // allocation point) into bounded chunks on constrained machines; the
    // gate itself always runs unfiltered.
    if let Ok(spec) = std::env::var("J2R_DIFF_SHARD") {
        let (i, n) = spec
            .split_once('/')
            .unwrap_or_else(|| panic!("J2R_DIFF_SHARD must be i/N, got {spec}"));
        let (i, n): (usize, usize) = (
            i.parse().unwrap_or_else(|_| panic!("bad shard index {i}")),
            n.parse().unwrap_or_else(|_| panic!("bad shard total {n}")),
        );
        assert!(n > 0 && i < n, "bad shard spec {spec}");
        let total = fixtures.len();
        fixtures = fixtures
            .into_iter()
            .enumerate()
            .filter(|(k, _)| k % n == i)
            .map(|(_, p)| p)
            .collect();
        eprintln!("shard {spec}: {} of {total} fixtures", fixtures.len());
    }

    // Optional explicit selection (ops): J2R_DIFF_FIXTURES="A,B,C" keeps
    // only the named fixtures (stems, no .java). Added 2026-11-05 for
    // gate-9's duration-balanced buckets — CI #59 proved stride sharding
    // balances counts, not durations (legs 14m/1m/>=120m/1m/1m/>=120m on
    // one tree), so the gate names its buckets explicitly from the
    // measured weight table (scripts/measure_gc_weights.py). Every named
    // fixture must exist — a typo must fail loudly, never pass by
    // selecting nothing. Unknown names and empty selections are errors.
    // Composable with J2R_DIFF_SHARD? No — the two are mutually
    // exclusive by assertion; a bucket is already the unit of parallelism.
    if let Ok(spec) = std::env::var("J2R_DIFF_FIXTURES") {
        assert!(
            std::env::var("J2R_DIFF_SHARD").is_err(),
            "J2R_DIFF_FIXTURES and J2R_DIFF_SHARD are mutually exclusive"
        );
        let wanted: Vec<String> = spec
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        assert!(
            !wanted.is_empty(),
            "J2R_DIFF_FIXTURES set but selects nothing"
        );
        let total = fixtures.len();
        let mut by_stem: std::collections::HashMap<String, std::path::PathBuf> =
            std::collections::HashMap::new();
        for path in &fixtures {
            let stem = path
                .file_stem()
                .expect("fixture path has a stem")
                .to_string_lossy()
                .into_owned();
            assert!(
                by_stem.insert(stem.clone(), path.clone()).is_none(),
                "duplicate fixture stem {stem}"
            );
        }
        let mut selected = Vec::with_capacity(wanted.len());
        for name in &wanted {
            let path = by_stem
                .get(name)
                .unwrap_or_else(|| panic!("J2R_DIFF_FIXTURES names unknown fixture {name:?}"));
            selected.push(path.clone());
        }
        // Keep the suite's sorted order so shard legs run in a stable,
        // reproducible sequence regardless of how the bucket is spelled.
        selected.sort();
        fixtures = selected;
        eprintln!("explicit selection: {} of {total} fixtures", fixtures.len());
    }

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
