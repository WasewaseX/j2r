//! T12 slice 12c — the native-method round-trip e2e (the M5_PLAN §3
//! acceptance demo, docs/M5_JNI_NATIVES.md §6).
//!
//! The fixture declares a `native int add(int, int)` and calls it. With
//! NO binding installed (the v1 e2e shape: the Rust-shape binding path
//! is unit-pinned in j2r-runtime::jni::natives; the C `.so` harness is
//! T13), both runtimes must show the SAME observable behavior:
//!
//! - stdout stops after "before" (the throw unwinds through println),
//! - stderr's HEADER line is byte-identical — Temurin's message is the
//!   quoted Java declaration `'int NativeProbe.add(int, int)'` (probed
//!   live on Temurin 21.0.13+9) and our `UnsatisfiedLinkError` builder
//!   reproduces exactly that shape,
//! - the exit code is 1 on both sides.
//!
//! The stack-FRAME lines under the header differ by construction (ours
//! is a Rust frame walk, Temurin's shows `Native Method`) — frame
//! normalization is the established three-channel law
//! (compare::tests::frames_are_normalized_but_the_header_is_not); the
//! full frame-level parity pin belongs to T13's C harness, which owns
//! the bound-C-native direction too (primitive 11).

use std::process::Command;

const PROBE: &str = r#"
public class NativeProbe {
    public static native int add(int a, int b);
    public static void main(String[] args) {
        System.out.println("before");
        System.out.println(add(20, 22));
        System.out.println("after");
    }
}
"#;

fn javac_path() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("J2R_JAVAC") {
        let p = std::path::PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    which("javac")
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

fn java_path() -> Option<std::path::PathBuf> {
    // Prefer the JDK next to J2R_JAVAC (the pinned Temurin), then PATH.
    if let Some(javac) = javac_path() {
        let java = javac.parent().expect("bin dir").join("java");
        if java.is_file() {
            return Some(java);
        }
    }
    which("java")
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(bin: &std::path::Path) -> Run {
    let out = Command::new(bin).output().expect("binary runs");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

#[test]
fn unbound_native_throws_the_oracle_shape_on_both_runtimes() {
    let Some(javac) = javac_path() else {
        // Skips silently when no JDK is available (the soe_e2e law).
        return;
    };
    let Some(java) = java_path() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!(
        "j2r-native-e2e-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("NativeProbe.java");
    std::fs::write(&src, PROBE).unwrap();

    let st = Command::new(&javac)
        .arg(&src)
        .current_dir(&dir)
        .status()
        .expect("javac runs");
    assert!(st.success(), "javac must compile the fixture");

    // The oracle: Temurin throws UnsatisfiedLinkError for an unbound
    // native — stdout stops after "before", exit 1.
    let oracle_out = Command::new(&java)
        .arg("-cp")
        .arg(&dir)
        .arg("NativeProbe")
        .output()
        .expect("java runs");
    let oracle = Run {
        code: oracle_out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&oracle_out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&oracle_out.stderr).to_string(),
    };
    assert_eq!(oracle.code, 1, "Temurin exits 1, got {}", oracle.stderr);
    assert_eq!(
        oracle.stdout, "before\n",
        "Temurin stdout stops at the throw"
    );
    let oracle_header = oracle
        .stderr
        .lines()
        .next()
        .expect("Temurin stderr has a header")
        .to_string();
    assert!(
        oracle_header.starts_with("Exception in thread \"main\" java.lang.UnsatisfiedLinkError: "),
        "unexpected oracle header: {oracle_header}"
    );

    // Ours: j2r build → run. The emitted call-through body consults the
    // registry, misses, and throws the SAME shape.
    let exe = env!("CARGO_BIN_EXE_j2r");
    let st = Command::new(exe)
        .arg("build")
        .arg(dir.join("NativeProbe.class"))
        .arg("-o")
        .arg(dir.join("out"))
        .status()
        .expect("j2r build runs");
    assert!(
        st.success(),
        "j2r build must succeed (native methods compile now)"
    );
    let bin = dir.join("out/target/debug/j2r_app");
    assert!(bin.is_file(), "the built binary exists at {bin:?}");
    let ours = run(&bin);

    // THREE channels: stdout byte-identical, exit identical, and the
    // stderr HEADER byte-identical (frames differ by construction —
    // the normalized-frame law; full parity is T13's pin).
    assert_eq!(ours.stdout, oracle.stdout, "stdout must match the oracle");
    assert_eq!(ours.code, oracle.code, "exit code must match the oracle");
    let ours_header = ours
        .stderr
        .lines()
        .next()
        .expect("our stderr has a header")
        .to_string();
    assert_eq!(
        ours_header, oracle_header,
        "the UnsatisfiedLinkError header must be byte-identical"
    );
}

/// The BOUND round-trip (the acceptance's positive arm, unit-level twin
/// of j2r-runtime::jni::natives' tests): the emitted call-through fn is
/// reachable through the generated metadata — asserted here by
/// compiling the fixture and checking the EMITTED SOURCE carries the
/// call-through body and the is_native metadata row (the runtime half
/// is pinned by the natives tests).
#[test]
fn the_emitted_source_carries_the_call_through_and_metadata() {
    let Some(javac) = javac_path() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!(
        "j2r-native-emit-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("NativeProbe.java");
    std::fs::write(&src, PROBE).unwrap();
    let st = Command::new(&javac)
        .arg(&src)
        .current_dir(&dir)
        .status()
        .expect("javac runs");
    assert!(st.success(), "javac must compile the fixture");

    // Build (the emitted source stays under out/src for inspection).
    let exe = env!("CARGO_BIN_EXE_j2r");
    let out = dir.join("emit");
    let st = Command::new(exe)
        .arg("build")
        .arg(dir.join("NativeProbe.class"))
        .arg("-o")
        .arg(&out)
        .status()
        .expect("j2r build runs");
    assert!(
        st.success(),
        "j2r build must succeed (native methods compile now)"
    );
    let registry_rs =
        std::fs::read_to_string(out.join("src/registry.rs")).expect("emitted registry.rs");
    assert!(
        registry_rs.contains("is_native: true"),
        "the metadata row must carry is_native: true"
    );
    // The call-through body: the native method's fn delegates to the
    // registry entry point (and the lookup table still maps the triple
    // to it — the dispatch stays uniform across layouts).
    let classes_dir = out.join("src/classes");
    let mut found_call_through = false;
    if let Ok(entries) = std::fs::read_dir(&classes_dir) {
        for e in entries.flatten() {
            if let Ok(body) = std::fs::read_to_string(e.path()) {
                if body.contains("call_native") && body.contains("FrameGuard::enter") {
                    found_call_through = true;
                    break;
                }
            }
        }
    }
    assert!(
        found_call_through,
        "the emitted crate must contain the call-through body \
         (FrameGuard + call_native) for the native method"
    );
}
