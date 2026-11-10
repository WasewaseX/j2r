//! T13 slice 13c — the three-channel JNI parity runner (the M5_PLAN §3.4
//! evidence bar, REVIEW_CHECKLIST G1.1/G1.3).
//!
//! The ONE harness `.so` (`tests/jni_harness/harness.c`, compiled ONCE
//! against the pinned JDK's `jni.h` of record) runs against BOTH runtimes:
//!
//! * **Temurin** — `java -Djava.library.path=... HarnessDriver`; the
//!   driver's `System.loadLibrary("jni_harness")` loads the `.so`, its
//!   `JNI_OnLoad` GetEnvs the VM and `RegisterNatives`-binds the probes.
//! * **j2r** — the SAME driver compiled by `j2r build`; the SAME
//!   `System.loadLibrary` runs through the 13c loading story (audited
//!   primitive 12: dlopen + dlsym + the ABI-fixed `JNI_OnLoad(vm)` call),
//!   after which the SAME `.so` drives OUR env table.
//!
//! Three channels, raw bytes: stdout byte-identical, stderr byte-identical
//! (EMPTY on the happy path — the C side never writes a channel), exit
//! identical (0; the driver's `check` exits 3 on any probe miss, so a
//! broken probe fails BOTH runtimes loudly instead of preserving parity
//! with a wrong answer).
//!
//! Skips (loudly, the soe_e2e law) when no JDK or no C compiler is
//! present. The CI gate wiring is 13d.

use std::process::Command;

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

fn javac_path() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("J2R_JAVAC") {
        let p = std::path::PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    which("javac")
}

fn java_path() -> Option<std::path::PathBuf> {
    if let Some(javac) = javac_path() {
        let java = javac.parent().expect("bin dir").join("java");
        if java.is_file() {
            return Some(java);
        }
    }
    which("java")
}

/// The JDK home (for `include/jni.h`): JAVA_HOME when set, else derived
/// from the discovered javac (`<jdk>/bin/javac`).
fn jdk_home() -> Option<std::path::PathBuf> {
    if let Ok(h) = std::env::var("JAVA_HOME") {
        let h = std::path::PathBuf::from(h);
        if h.join("include").join("jni.h").is_file() {
            return Some(h);
        }
    }
    let javac = javac_path()?;
    let home = javac
        .parent()
        .and_then(|b| b.parent())
        .expect("javac sits at <jdk>/bin/javac")
        .to_path_buf();
    if home.join("include").join("jni.h").is_file() {
        Some(home)
    } else {
        None
    }
}

struct Run {
    code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_raw(mut cmd: Command) -> Run {
    let out = cmd.output().expect("child runs");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: out.stdout,
        stderr: out.stderr,
    }
}

#[test]
fn the_one_c_harness_is_three_channel_parity_on_both_runtimes() {
    let Some(javac) = javac_path() else {
        eprintln!("skipping the JNI parity battery: javac not found");
        return;
    };
    let Some(java) = java_path() else {
        eprintln!("skipping the JNI parity battery: java not found");
        return;
    };
    let Some(jdk) = jdk_home() else {
        eprintln!("skipping the JNI parity battery: jdk include/jni.h not found");
        return;
    };
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    if which(&cc).is_none() {
        eprintln!("skipping the JNI parity battery: no C compiler ({cc})");
        return;
    }

    let dir = std::env::temp_dir().join(format!(
        "j2r-jni-parity-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/jni_harness");
    let harness_c = src_dir.join("harness.c");
    let driver_java = src_dir.join("HarnessDriver.java");
    let so = dir.join("libjni_harness.so");

    // 1. The harness: compiled ONCE, run against both runtimes (G1.1).
    let st = Command::new(&cc)
        .args([
            "-O1",
            "-shared",
            "-fPIC",
            "-I",
            jdk.join("include").to_str().unwrap(),
            "-I",
            jdk.join("include").join("linux").to_str().unwrap(),
            harness_c.to_str().unwrap(),
            "-o",
            so.to_str().unwrap(),
        ])
        .status()
        .expect("cc runs");
    assert!(st.success(), "compiling libjni_harness.so failed");

    // 2. The driver fixture.
    let st = Command::new(&javac)
        .args(["-d", dir.to_str().unwrap(), driver_java.to_str().unwrap()])
        .status()
        .expect("javac runs");
    assert!(st.success(), "javac must compile the driver fixture");

    // 3. The oracle: Temurin loads the same .so via System.loadLibrary.
    let mut oracle_cmd = Command::new(&java);
    oracle_cmd
        .args([
            "-cp",
            dir.to_str().unwrap(),
            &format!("-Djava.library.path={}", dir.display()),
        ])
        .env("LD_LIBRARY_PATH", &dir)
        .arg("HarnessDriver");
    let oracle = run_raw(oracle_cmd);

    // 4. Ours: the same fixture compiled by j2r; the same System.loadLibrary
    //    through the 13c loading story; the same .so drives our env table.
    let exe = env!("CARGO_BIN_EXE_j2r");
    let out_dir = dir.join("out");
    let st = Command::new(exe)
        .args([
            "build",
            "-p",
            dir.to_str().unwrap(),
            dir.join("HarnessDriver.class").to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
        ])
        .status()
        .expect("j2r build runs");
    assert!(st.success(), "j2r build must compile the driver fixture");
    let bin = out_dir.join("target/debug/j2r_app");
    assert!(bin.is_file(), "the built binary exists at {bin:?}");
    let ours_cmd = {
        let mut c = Command::new(&bin);
        c.env("LD_LIBRARY_PATH", &dir);
        c
    };
    let ours = run_raw(ours_cmd);

    // 5. THREE channels, raw bytes. stderr must be EMPTY on both sides —
    //    any abort/exception text is a failure by itself.
    assert_eq!(
        ours.code,
        0,
        "our side must exit 0; stdout so far: {}",
        String::from_utf8_lossy(&ours.stdout)
    );
    assert_eq!(
        oracle.code,
        0,
        "the oracle must exit 0; stderr: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&ours.stdout),
        String::from_utf8_lossy(&oracle.stdout),
        "stdout must be byte-identical vs Temurin"
    );
    assert!(
        oracle.stderr.is_empty(),
        "oracle stderr must be empty, got: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    assert!(
        ours.stderr.is_empty(),
        "our stderr must be empty, got: {}",
        String::from_utf8_lossy(&ours.stderr)
    );
    let stdout = String::from_utf8_lossy(&ours.stdout);
    assert!(
        stdout.contains("P done"),
        "the battery must run to completion, got:\n{stdout}"
    );
}
