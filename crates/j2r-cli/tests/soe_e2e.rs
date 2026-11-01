//! Wave-1 SOE end-to-end tests (`docs/M2_PLAN.md` §3.2).
//!
//! Two properties are proven here, both *runtime-level* rather than
//! differential (the `J2R_MAX_JAVA_FRAMES` env var does not exist in
//! HotSpot, so its behaviour cannot be differentially compared):
//!
//! 1. **The process never aborts at any limit.** A recursive program run
//!    with a deliberately tiny `J2R_MAX_JAVA_FRAMES` exits 0 with
//!    `caught-soe` on stdout — no SIGSEGV, no Rust panic (101).
//! 2. **The override-boundary matrix.** default / minimum / maximum are
//!    accepted and run to completion; `0`, `MAX+1` and non-numeric values
//!    are rejected with `j2r: invalid J2R_MAX_JAVA_FRAMES=…` and exit 2
//!    before any Java code runs.
//!
//! Skips silently when no JDK is available (CI installs Temurin 21).

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

/// The program used by both properties: recursion deep enough to SOE at any
/// limit, catching it and continuing (differential fixtures prove the
/// Java-visible semantics against HotSpot; here we prove process behaviour).
const DEEP_CATCH: &str = r#"
public class DeepCatch {
    static int down(int n) { return down(n + 1) + 1; }
    public static void main(String[] args) {
        try {
            down(1);
            System.out.println("no-soe");
        } catch (StackOverflowError e) {
            System.out.println("caught-soe");
        }
        System.out.println("continued");
    }
}
"#;

struct Built {
    dir: std::path::PathBuf,
    bin: std::path::PathBuf,
}

fn build_deep_catch(release: bool) -> Built {
    let exe = env!("CARGO_BIN_EXE_j2r");
    let dir = std::env::temp_dir().join(format!(
        "j2r-soe-e2e-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let java = dir.join("DeepCatch.java");
    std::fs::write(&java, DEEP_CATCH).unwrap();

    let javac = std::env::var("J2R_JAVAC").unwrap_or_else(|_| "javac".to_string());
    let st = Command::new(&javac)
        .arg(&java)
        .current_dir(&dir)
        .status()
        .expect("javac runs");
    assert!(st.success(), "javac must compile the fixture");

    let mut cmd = Command::new(exe);
    cmd.arg("build")
        .arg(dir.join("DeepCatch.class"))
        .arg("-o")
        .arg(dir.join("out"));
    if release {
        cmd.arg("--release");
    }
    let st = cmd.status().expect("j2r build runs");
    assert!(st.success(), "j2r build must succeed");
    let profile = if release { "release" } else { "debug" };
    Built {
        bin: dir.join(format!("out/target/{}/j2r_app", profile)),
        dir,
    }
}

fn run(bin: &std::path::Path, limit: Option<&str>) -> (i32, String, String) {
    let mut cmd = Command::new(bin);
    if let Some(v) = limit {
        cmd.env("J2R_MAX_JAVA_FRAMES", v);
    } else {
        cmd.env_remove("J2R_MAX_JAVA_FRAMES");
    }
    let out = cmd.output().expect("binary runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn soe_process_never_aborts_and_boundary_matrix_holds() {
    if !javac_available() {
        eprintln!("skipping SOE e2e: javac not found");
        return;
    }
    // Release build: the hard-ceiling boundary runs at MAX_FRAMES_ABS
    // frames, which is only proven safe for the calibrated (release) frame
    // footprint (M2_PLAN §3.1.1).
    let built = build_deep_catch(true);
    let max_abs = 131_072usize; // frame::MAX_FRAMES_ABS (const-folded here so
                                // the test documents the exact boundary)

    // --- property 1: tiny limit, process never aborts ---
    let (code, stdout, stderr) = run(&built.bin, Some("64"));
    assert_eq!(code, 0, "exit must be 0, stderr: {stderr}");
    assert!(
        stdout.contains("caught-soe") && stdout.contains("continued"),
        "stdout must show the caught SOE and continuation: {stdout}"
    );

    // --- property 2: the six-case boundary matrix ---
    // default (unset): accepted, program runs to its caught SOE
    let (code, stdout, _) = run(&built.bin, None);
    assert_eq!(code, 0);
    assert!(stdout.contains("caught-soe"));

    // minimum (1): accepted; the very first recursive call SOEs
    let (code, stdout, _) = run(&built.bin, Some("1"));
    assert_eq!(code, 0);
    assert!(stdout.contains("caught-soe"));

    // maximum (MAX_FRAMES_ABS): accepted, completes
    let (code, stdout, _) = run(&built.bin, Some("131072"));
    assert_eq!(code, 0, "max accepted value must not abort");
    assert!(stdout.contains("caught-soe"));

    // max + 1: rejected with exit 2 before any Java code runs
    let over = (max_abs + 1).to_string();
    let (code, stdout, stderr) = run(&built.bin, Some(&over));
    assert_eq!(code, 2, "MAX+1 must be rejected");
    assert!(stdout.is_empty(), "no Java output on rejection");
    assert!(
        stderr.contains(&format!("j2r: invalid J2R_MAX_JAVA_FRAMES={over}")),
        "rejection diagnostic missing: {stderr}"
    );

    // zero: rejected
    let (code, _, stderr) = run(&built.bin, Some("0"));
    assert_eq!(code, 2);
    assert!(stderr.contains("j2r: invalid J2R_MAX_JAVA_FRAMES=0"));

    // non-numeric: rejected
    let (code, _, stderr) = run(&built.bin, Some("abc"));
    assert_eq!(code, 2);
    assert!(stderr.contains("j2r: invalid J2R_MAX_JAVA_FRAMES=abc"));

    let _ = std::fs::remove_dir_all(&built.dir);
}
