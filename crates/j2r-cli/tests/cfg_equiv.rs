//! Wave-4 output-equivalence test (M2_PLAN §6.2): for every fixture in the
//! differential suite, the crate compiled with `--emit-cfg` (the Wave-4
//! structured-lift investigation, default OFF) — and, since the typed wave,
//! the crate compiled with `--emit-typed` (the registered typed-operand
//! lever, docs/TYPED_REGISTER_READINESS.md §5, default OFF) — must produce
//! **byte-identical stdout, stderr and exit status** to the crate compiled
//! with the approved default pc-dispatch emission.
//!
//! This is the prototype's gate: a structured lift or a typed body that
//! changes any observable fails here, regardless of the unit tests in
//! j2r-codegen. Unsupported shapes fall back per method, so the modes can
//! only differ by emission BUGS, never by design.
//!
//! Skips silently when no JDK is available (CI installs one) — fixture
//! compilation needs `javac`.

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
    // Platform separator via std (':' Unix, ';' Windows) — audit A7.
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn javac_binary() -> String {
    // honor J2R_JAVAC like the availability check does (corpus wave 1 fix:
    // the two must never disagree or the suite panics on spawn)
    std::env::var("J2R_JAVAC").unwrap_or_else(|_| "javac".to_string())
}

/// One emission mode under test. `Base` is the approved default; the
/// investigation modes must never change observable output.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Base,
    Cfg,
    Typed,
}

impl Mode {
    fn flag_name(self) -> &'static str {
        match self {
            Mode::Base => "base",
            Mode::Cfg => "emit_cfg",
            Mode::Typed => "emit_typed",
        }
    }
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Compiles `fixture` into a native binary with the given emission mode and
/// runs it, capturing all three channels.
fn build_and_run(fixture: &std::path::Path, mode: Mode, tag: &str) -> Out {
    // Stable per-mode crate dirs (no fixture suffix): the vendored runtime
    // path stays identical across fixtures, so with the shared
    // CARGO_TARGET_DIR below the expensive runtime compile unit builds ONCE
    // per suite instead of once per fixture × mode (the typed wave made
    // that 3 × N cold builds ≈ 23 min; with the cache it is minutes). The
    // crate src and classes are wiped before every regeneration so stale
    // modules of a previous fixture cannot leak into this one's build.
    // The single #[test] drives everything sequentially, so the shared
    // target's fixed bin path is never raced between build and run.
    let tmp = std::env::temp_dir().join(format!("j2r-cfg-equiv-{tag}"));
    std::fs::create_dir_all(&tmp).expect("tmp dir");

    let classes = tmp.join("classes");
    let _ = std::fs::remove_dir_all(&classes);
    std::fs::create_dir_all(&classes).expect("classes dir");
    let _ = std::fs::remove_dir_all(tmp.join("crate").join("src"));
    let jc = Command::new(javac_binary())
        .arg("-d")
        .arg(&classes)
        .arg(fixture)
        .output()
        .expect("spawn javac");
    assert!(
        jc.status.success(),
        "javac failed for {}: {}",
        fixture.display(),
        String::from_utf8_lossy(&jc.stderr)
    );
    let mut class_files: Vec<_> = std::fs::read_dir(&classes)
        .expect("classes")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().map(|e| e == "class").unwrap_or(false))
        .collect();
    class_files.sort();

    let opts = j2r_codegen::Options {
        classpath: Vec::new(),
        inputs: class_files,
        main_class: None,
        crate_name: "j2r-cfg-equiv".to_string(),
        out_dir: tmp.join("crate"),
        emit_cfg: mode == Mode::Cfg,
        emit_typed: mode == Mode::Typed,
    };
    let dir = j2r_codegen::compile_to_crate(&opts).expect("codegen");
    // One shared target dir for the whole suite: per-mode crate dirs keep
    // the vendored runtime path stable, and the runtime sources are
    // identical for every fixture, so after the first build per mode the
    // runtime artifact is a cache hit (content-idempotent vendoring in
    // emit_crate keeps its mtimes valid).
    let shared_target = std::env::temp_dir().join("j2r-cfg-equiv-target");
    std::fs::create_dir_all(&shared_target).expect("shared target dir");
    let build = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
        .arg("build")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &shared_target)
        .output()
        .expect("spawn cargo");
    assert!(
        build.status.success(),
        "cargo build failed (mode={}) for {}: {}",
        mode.flag_name(),
        fixture.display(),
        String::from_utf8_lossy(&build.stderr)
    );
    let bin = shared_target.join("debug").join("j2r_cfg_equiv");
    let run = Command::new(&bin).output().expect("run native binary");
    Out {
        code: run.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&run.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&run.stderr).into_owned(),
    }
}

#[test]
fn emit_cfg_output_equivalence_across_suite() {
    let _ = env!("CARGO_BIN_EXE_j2r"); // ensures the CLI builds first
    if !javac_available() {
        eprintln!("skipping cfg-equivalence suite: javac not found");
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

    // Same harness law as diff_suite (bf8a14a): optional stride sharding
    // and explicit selection, for local windows that cannot fit the whole
    // suite. CI always runs the unsharded full suite.
    if let Ok(spec) = std::env::var("J2R_DIFF_SHARD") {
        assert!(
            std::env::var("J2R_DIFF_FIXTURES").is_err(),
            "J2R_DIFF_SHARD and J2R_DIFF_FIXTURES are mutually exclusive"
        );
        let (i, n) = spec.split_once('/').expect("shard spec must be i/N");
        let (i, n): (usize, usize) = (
            i.parse().expect("shard index"),
            n.parse().expect("shard count"),
        );
        assert!(n > 0, "shard count must be positive");
        assert!(i < n, "shard index {i} out of range for {n} shards");
        let total = fixtures.len();
        // Deterministic stride: position in the sorted census (the same
        // law as diff_suite — shard selects fixture POSITIONS, so the
        // partition is stable as fixtures join or leave).
        let sharded: Vec<_> = fixtures
            .iter()
            .enumerate()
            .filter(|(pos, _)| pos % n == i)
            .map(|(_, p)| p.clone())
            .collect();
        assert!(
            !sharded.is_empty(),
            "shard {i}/{n} selected zero fixtures of {total}"
        );
        eprintln!(
            "cfg-equiv shard {i}/{n}: {} of {total} fixtures",
            sharded.len()
        );
        fixtures = sharded;
    }
    if let Ok(spec) = std::env::var("J2R_DIFF_FIXTURES") {
        let names: Vec<&str> = spec
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        assert!(!names.is_empty(), "J2R_DIFF_FIXTURES names no fixtures");
        let mut selected = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for name in &names {
            assert!(
                !seen.contains(*name),
                "J2R_DIFF_FIXTURES lists {name:?} twice"
            );
            seen.insert(*name);
            let path = suite_dir.join(format!("{name}.java"));
            assert!(
                path.is_file(),
                "J2R_DIFF_FIXTURES names unknown fixture {name:?}"
            );
            selected.push(path);
        }
        selected.sort();
        fixtures = selected;
        eprintln!("cfg-equiv explicit selection: {} fixtures", fixtures.len());
    }

    let mut failures = Vec::new();
    for fixture in &fixtures {
        let name = fixture.file_stem().unwrap().to_string_lossy().into_owned();
        let base = build_and_run(fixture, Mode::Base, "base");
        for (mode, run) in [
            (Mode::Cfg, build_and_run(fixture, Mode::Cfg, "cfg")),
            (Mode::Typed, build_and_run(fixture, Mode::Typed, "typed")),
        ] {
            if base.code != run.code
                || base.stdout != run.stdout
                || base.stderr != run.stderr
            {
                failures.push(format!(
                    "{name} ({}): exit {} vs {}, stdout equal: {}, stderr equal: {}",
                    mode.flag_name(),
                    base.code,
                    run.code,
                    base.stdout == run.stdout,
                    base.stderr == run.stderr
                ));
            }
        }
        // The stable per-mode dirs are reused across fixtures (src and
        // classes are wiped inside build_and_run), so there is nothing to
        // remove here anymore.
    }
    assert!(
        failures.is_empty(),
        "an investigation mode changed observable output for {} fixture(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
