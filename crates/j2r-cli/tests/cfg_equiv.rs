//! Wave-4 output-equivalence test (M2_PLAN §6.2): for every fixture in the
//! differential suite, the crate compiled with `--emit-cfg` (the Wave-4
//! structured-lift investigation, default OFF) must produce **byte-identical
//! stdout, stderr and exit status** to the crate compiled with the approved
//! default pc-dispatch emission.
//!
//! This is the prototype's gate: a structured lift that changes any
//! observable fails here, regardless of the unit tests in j2r-codegen.
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

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Compiles `fixture` into a native binary with the given emission mode and
/// runs it, capturing all three channels.
fn build_and_run(fixture: &std::path::Path, emit_cfg: bool, tag: &str) -> Out {
    let tmp = std::env::temp_dir().join(format!(
        "j2r-cfg-equiv-{}-{}",
        tag,
        fixture.file_stem().unwrap().to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("tmp dir");

    let classes = tmp.join("classes");
    std::fs::create_dir_all(&classes).expect("classes dir");
    let jc = Command::new("javac")
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
        emit_cfg,
    };
    let dir = j2r_codegen::compile_to_crate(&opts).expect("codegen");
    let build = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
        .arg("build")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(dir.join("Cargo.toml"))
        .output()
        .expect("spawn cargo");
    assert!(
        build.status.success(),
        "cargo build failed (emit_cfg={emit_cfg}) for {}: {}",
        fixture.display(),
        String::from_utf8_lossy(&build.stderr)
    );
    let bin = dir.join("target").join("debug").join("j2r_cfg_equiv");
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

    let mut failures = Vec::new();
    for fixture in &fixtures {
        let name = fixture.file_stem().unwrap().to_string_lossy().into_owned();
        let base = build_and_run(fixture, false, "base");
        let cfg = build_and_run(fixture, true, "cfg");
        if base.code != cfg.code || base.stdout != cfg.stdout || base.stderr != cfg.stderr {
            failures.push(format!(
                "{name}: exit {} vs {}, stdout equal: {}, stderr equal: {}",
                base.code,
                cfg.code,
                base.stdout == cfg.stdout,
                base.stderr == cfg.stderr
            ));
        }
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("j2r-cfg-equiv-base-{name}")),
        );
        let _ =
            std::fs::remove_dir_all(std::env::temp_dir().join(format!("j2r-cfg-equiv-cfg-{name}")));
    }
    assert!(
        failures.is_empty(),
        "emit_cfg changed observable output for {} fixture(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
