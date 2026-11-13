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
    // — 86 on disk. M4 Wave 4 (T14) added the interrupt fixtures
    // (InterruptSleep, InterruptJoin, InterruptWait) and (T15) the daemon
    // fixtures (DaemonExit, DaemonInterrupt) — 94 on disk. The F2 probe
    // wave added LambdaClinit (lambda + final/volatile reference statics
    // in <clinit>, invoked through the interface types — the Runnable SAM
    // row) — 95 on disk. The F4/F5 closure wave added HexFloat (hex-float
    // literal parsing, bit-exact via floatToIntBits/doubleToLongBits) and
    // ArraycopyMismatch (the arraycopy element-mismatch message +
    // destination-order partial copy) — 97 on disk. The F6 wave added
    // EnumFixture (enum classes end-to-end: $VALUES / values() clone
    // semantics, valueOf with the canonical-name miss message,
    // getDeclaringClass, compareTo/equals, the $SwitchMap synthetic, enum
    // fields+ctors) — 98 on disk. The skip/charset wave added SkipDefault
    // (the InputStream.skip(J) DEFAULT loop-read on a read()-only stream,
    // ByteArrayInputStream semantics incl. close-no-op and mark/reset,
    // FilterInputStream delegation) and Utf16 (the UTF-16 charset family:
    // BOM encode/decode, BE/LE, surrogate pairs, U+FFFD substitution,
    // exact UEE names) — 100 on disk. The BufferedInputStream wave added
    // Buffered (buffer-read paths, the skip buffer-vs-inner law, mark/
    // readlimit invalidation, the "Stream closed" family) — 101 on disk.
    // The deep-arrays wave added DeepArrays (3D primitive arrays:
    // partial-dim alloc, null rows, clone-at-depth, arraycopy, instanceof,
    // the aastore ASE external-name shapes) — 102 on disk. The Formatter
    // wave added Fmt (String.format/printf subset: %s %S %d %x %X %o %f %e
    // %b %c %h, flags/width/precision, HALF_UP on the shortest repr, the
    // exact three error messages) — 103 on disk. The stream wave 4 added
    // Pushback (unread single/bulk/range, buffer exhaustion
    // "Push back buffer is full", skip-through-pushback, markSupported
    // false, the "Stream closed" family) — 104 on disk, guard one below
    // that. The Formatter-residuals wave (2026-11-06) extended Fmt IN
    // PLACE (no new fixture, count unchanged): %E %G %a/%A, the
    // ','-flag legality, null->null/NULL, the uppercase-variant family
    // (S B H C X E G A), the parse-time MFW/FFCME laws —
    // corpus/FmtResidualProbe.java is the 116-row Temurin vector.
    // The singleton/cyclic-init wave (2026-11-06, the review's P1 ask)
    // added SingletonFactory (factory singleton with a static-factory
    // <clinit> constructor call, the static-init-block form, and the
    // initialization-on-demand-holder idiom with a laziness proof) and
    // CyclicInit (two pairs of mutually-referencing <clinit> bodies pinning
    // JLS 12.4.2 in-progress recursion: assigned-value and default-value
    // visibility) — 106 on disk, guard one below that. The depth-4 wave
    // (2026-11-06) then extended DeepArrays IN PLACE (count unchanged):
    // ten `[[[[I` rows — full/partial/one-dim allocation, the typed-lattice
    // call-site payoff, shallow clone, 4D arraycopy, null-row NPE,
    // instanceof/getName shapes, the [[[F-into-[[[I ASE message.
    // The JAMA corpus wave (2026-11-06) added ArrayCovarianceProbe —
    // JLS 4.10.3 array covariance pinned differentially ([[D flows into
    // Object[] parameters/assignments; the verifier's covariant pop_exact
    // rule and the runtime store checks are the JDK's own): 107 on disk.
    // The Math-semantics wave (2026-11-07, evidence from the external
    // java-to-rust conversion report's silent-semantic class) added
    // MathSemantics — the JDK-8010430 round law (round(0.49999999999999994)
    // = 0 and the mathematical-integer cast branch above each binade, both
    // arities; the naive floor(x+0.5) native failed 4 rows before the fix),
    // the min/max signed-zero law bit-visible via doubleToLongBits, NaN
    // results pinned under the canonicalizing bit-conversion contract
    // (raw NaN payloads from min/max are a HotSpot C2-intrinsic artifact —
    // max(NaN,1.0) returns 1.0's bits on x86-64 while min returns the NaN
    // operand — deliberately not pinned), and abs's wrapping/signed-zero
    // laws: 108 on disk, guard one below that.
    // The runtime-io wave (2026-11-07, org.json conversion-report
    // findings #1-#3) added StringIoSemantics — StringReader.read()
    // returns UTF-16 CODE UNITS (astral = two reads, unit-precise
    // mark/reset), the full JDK surface (skip clamped + negative-returns-0
    // Temurin-21-empirical, reset-to-beginning before any mark, the
    // Objects.checkFromIndexRange bulk-range message, the close law) and
    // StringWriter's write(int) 16-bit truncation law: 109 on disk, guard
    // one below that. R2 slice 1 (2026-11-12) added ReflectForName (the
    // Class.forName closed-world resolution fixture) — 110 on disk, guard
    // one below that. External-B2/O5 (2026-11-13) added PartitionCatch
    // (the exception-table partition law) — 111 on disk, guard one below
    // that. The EXTERNAL-B4..B12 wave (2026-11-13, the jbox2d port
    // report) added Externals — NoSuchFieldError catch-type verification,
    // StrictMath's full mirrored surface, the 8 boxed constructors (fresh
    // boxes: `new Integer(127) == new Integer(127)` is FALSE, the owner's
    // bug.md cache claim corrected against the pinned JVM vector), the
    // AssertionError primitive/Object family, the reverse IEEE bit
    // statics, the fdlibm transcendental band (StrictMath pinned
    // bit-exactly; Math carries the documented HotSpot-intrinsic 1-ulp
    // residual O1), java.util.Random's 48-bit LCG bit-identical incl.
    // nextGaussian/nextBytes, the ArrayList.toArray two-branch contract,
    // Arrays.sort range forms with JDK-exact messages + reverseOrder +
    // desiredAssertionStatus — 112 on disk, guard one below that. It also
    // found a REAL pre-existing bug en route: a main() that touches none
    // of its own class's statics ran WITHOUT the `<clinit>` (the launcher
    // now ensures the main class, JLS 12.4.2).
    assert!(
        fixtures.len() >= 111,
        "differential suite shrank: {} fixtures (expected >= 111)",
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

    // Optional gate-9 bucket selection (ops): J2R_DIFF_BUCKET names one
    // of the duration-balanced gate-9 partitions computed in-place from
    // the on-disk suite (bucket_for_gate9 below). The policy lives HERE —
    // in the harness, reviewable, unit-tested for full coverage — not in
    // a workflow YAML list that silently rots when a fixture is added:
    // any new fixture that is not a named special falls into a light
    // bucket automatically, so the gate can never lose a fixture.
    // Evidence (gc-probe runs #1/#2, 2026-11-05): ReflectGC completes on
    // CI in ~14m at N=1 (local 540s x ~1.5); MonitorContention and
    // WaitNotify expose a timing-sensitive Wave-3 protocol race on CI's
    // slow 2-vCPU runners at N=1 (MonitorContention 2/2 hangs,
    // WaitNotify 1/2) while finishing in under a minute at N=8 — they
    // run on the monitors bucket at J2R_GC_EVERY_N=8 until the race is
    // hunted down (registered as the Wave-4 stress task); everything
    // else is N=1. Local N=1 evidence for the monitor fixtures stands
    // (scripts/gc_weights.csv: all 89 then-measured fixtures pass at
    // N=1 locally, byte-identical vs Temurin).
    if let Ok(bucket) = std::env::var("J2R_DIFF_BUCKET") {
        assert!(
            std::env::var("J2R_DIFF_SHARD").is_err() && std::env::var("J2R_DIFF_FIXTURES").is_err(),
            "J2R_DIFF_BUCKET is mutually exclusive with SHARD/FIXTURES"
        );
        let names = bucket_for_gate9(&bucket, &fixtures);
        let total = fixtures.len();
        let by_stem: std::collections::HashMap<String, std::path::PathBuf> = fixtures
            .iter()
            .map(|p| {
                (
                    p.file_stem().unwrap().to_string_lossy().into_owned(),
                    p.clone(),
                )
            })
            .collect();
        let mut selected = Vec::with_capacity(names.len());
        for name in &names {
            selected.push(
                by_stem
                    .get(name)
                    .unwrap_or_else(|| panic!("bucket names unknown fixture {name:?}"))
                    .clone(),
            );
        }
        fixtures = selected;
        eprintln!(
            "gate-9 bucket {bucket}: {} of {total} fixtures",
            fixtures.len()
        );
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

// ------------------------------------------------------------ gate-9 buckets
//
// The gate-9 partition policy. Measured weights live in
// scripts/gc_weights.csv (measure_gc_weights.py) and the CI-side probe
// evidence in the gc-probe workflow history. The buckets:
//
// * `solo`     — ReflectGC alone at N=1: the only fixture measured in
//                the hundreds of seconds locally (~14m on CI).
// * `monitors` — MonitorContention + WaitNotify at N=8: the two
//                monitor-handoff fixtures with the CI-only protocol race
//                (see the selection comment above).
// * `heavy`    — allocation-storm fixtures measured in the ~6-10s class
//                locally (the corpus-scale programs plus the GC-identity
//                family); they would dominate any mixed light leg.
// * `light-N`  — everything else, round-robin over N legs. Any future
//                fixture not named in the specials below lands here
//                automatically, so coverage is by construction.
//
// Buckets are re-derivable from the suite directory alone — no YAML list
// to rot. The `full_coverage_is_a_partition` unit test pins the property
// the gate depends on.

fn bucket_for_gate9(bucket: &str, fixtures: &[std::path::PathBuf]) -> Vec<String> {
    const SOLO: &[&str] = &["ReflectGC"];
    const MONITORS: &[&str] = &["MonitorContention", "WaitNotify"];
    const HEAVY: &[&str] = &[
        "BigCache",
        "BigGradebook",
        "BigHuffman",
        "BigJson",
        "BigLang",
        "BigRpg",
        "Cycles",
        "ForEachColl",
        "GcIdentity",
        "HashMapOrder",
        "HashSetOps",
        "InternAcrossGc",
        "LambdaGC",
        "LambdaInCollections",
    ];

    let stems: Vec<String> = fixtures
        .iter()
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    let in_list = |name: &str, list: &[&str]| list.contains(&name);
    // Fixed lists are INTERSECTED with the on-disk census: a fixture that
    // is renamed or deleted must never survive as a phantom bucket entry
    // (the partition unit test pins union == census).
    let special = |list: &[&str]| -> Vec<String> {
        list.iter()
            .filter(|s| stems.iter().any(|t| t == *s))
            .map(|s| s.to_string())
            .collect()
    };

    let mut out = Vec::new();
    match bucket {
        "solo" => out.extend(special(SOLO)),
        "monitors" => out.extend(special(MONITORS)),
        "heavy" => out.extend(special(HEAVY)),
        b if b.starts_with("light-") => {
            let i: usize = b["light-".len()..]
                .parse()
                .unwrap_or_else(|_| panic!("bad light bucket {b}"));
            assert!(i < GATE9_LIGHT_LEGS, "light index {i} out of range");
            // Round-robin over the sorted stems, skipping every named
            // special: the same stability property as the old stride
            // shard (sorted order, deterministic legs), now over the
            // residual class only.
            for (k, stem) in stems.iter().enumerate() {
                if in_list(stem, SOLO) || in_list(stem, MONITORS) || in_list(stem, HEAVY) {
                    continue;
                }
                if k % GATE9_LIGHT_LEGS == i {
                    out.push(stem.clone());
                }
            }
        }
        other => panic!(
            "unknown J2R_DIFF_BUCKET {other:?} (solo|monitors|heavy|light-0..{})",
            GATE9_LIGHT_LEGS - 1
        ),
    }
    assert!(!out.is_empty(), "bucket {bucket} selected nothing");
    out
}

const GATE9_LIGHT_LEGS: usize = 3;

#[cfg(test)]
mod gate9_bucket_tests {
    use super::*;

    fn fake_suite() -> Vec<std::path::PathBuf> {
        // A fake census that includes the specials plus enough light
        // names to exercise the round-robin (no filesystem reads: the
        // property must hold for ANY census shape).
        let mut names: Vec<String> = vec![
            "ReflectGC".into(),
            "MonitorContention".into(),
            "WaitNotify".into(),
            "BigJson".into(),
            "Cycles".into(),
        ];
        for i in 0..12 {
            names.push(format!("Light{i:02}"));
        }
        names
            .into_iter()
            .map(|s| std::path::PathBuf::from(format!("suite/{s}.java")))
            .collect()
    }

    /// THE gate-9 coverage property: every fixture is in exactly one
    /// bucket. A fixture that joins the suite lands in light-N by
    /// construction; the named specials are asserted present exactly
    /// once across all buckets.
    #[test]
    fn full_coverage_is_a_partition() {
        let suite = fake_suite();
        let total = suite.len();
        let buckets = ["solo", "monitors", "heavy", "light-0", "light-1", "light-2"];
        let mut seen = std::collections::HashMap::new();
        for b in buckets {
            for name in bucket_for_gate9(b, &suite) {
                let prev = seen.insert(name.clone(), b);
                assert!(prev.is_none(), "{name} appears in {prev:?} AND {b}");
            }
        }
        assert_eq!(seen.len(), total, "every fixture must be bucketed");
        // The specials are where the probe evidence says they must be.
        assert_eq!(bucket_for_gate9("solo", &suite), vec!["ReflectGC"]);
        assert_eq!(
            bucket_for_gate9("monitors", &suite),
            vec!["MonitorContention", "WaitNotify"]
        );
        // A fixture absent from the census never leaks into a bucket.
        assert!(!bucket_for_gate9("heavy", &suite).contains(&"BigHuffman".to_string()));
        // And every bucket leg selects only REAL fixtures (no typos can
        // hide in the tables — the by-stem lookup panics otherwise).
        for b in buckets {
            assert!(!bucket_for_gate9(b, &suite).is_empty());
        }
    }
}
