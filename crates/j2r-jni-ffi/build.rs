//! The REAL C probe for the foreign-call thunk's tests (M5_JNI_HARNESS.md
//! §6-13b) — the first C in the repo, built ONLY for the test binaries.
//!
//! Why a build script and not the `cc` crate: every member manifest must
//! stay valid when copied outside this workspace (`j2r build` vendors
//! `j2r-jni-ffi` source into generated projects — see the workspace
//! Cargo.toml note), and a `[build-dependencies]` entry would enter the
//! vendored manifest and break the generated crates' `--offline` builds
//! (the standalone gate). This script therefore invokes the platform C
//! compiler DIRECTLY, and only this file does it — build.rs itself is
//! never vendored (copy_runtime copies Cargo.toml + src/ only), so a
//! generated project never sees any of this.
//!
//! Failure law: with no usable C compiler the script silently skips the
//! cfg — the `#[cfg(j2r_c_probe)]` tests compile out, and the crate
//! (plus every vendored generated project) builds exactly as before.
//! The symbols are referenced only from `#[cfg(test)]` code, so the
//! probe object is linked into test binaries alone
//! (`cargo:rustc-link-arg-tests`).

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const PROBE_C: &str = r#"/* The C-harness shape (docs/M5_JNI_HARNESS.md section 1): bare-form C
   natives as RegisterNatives would hand them over. Two probes cover the
   two classification channels of the SysV register window: an
   integer-class-only native (a jint add) and a mixed native ((I D)D)
   whose return rides the SSE channel. Compiled by build.rs and linked
   into TEST binaries only; the library target never references them. */
long long c_probe_add(void *env, void *recv,
                      unsigned long long a, unsigned long long b) {
    (void)env; (void)recv;
    return (long long)(a + b);
}
double c_probe_mad(void *env, void *recv,
                   unsigned long long a, double b) {
    (void)env; (void)recv;
    return (double)a * b;
}
"#;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CC");
    let out = PathBuf::from(env::var("OUT_DIR").expect("cargo provides OUT_DIR"));
    let src = out.join("j2r_c_probe.c");
    let obj = out.join("j2r_c_probe.o");
    fs::write(&src, PROBE_C).expect("write the C probe source into OUT_DIR");
    let cc = env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let compiled = Command::new(&cc)
        .args([
            "-O1",
            "-c",
            src.to_str().expect("OUT_DIR path is utf-8"),
            "-o",
            obj.to_str().expect("OUT_DIR path is utf-8"),
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !compiled {
        // No usable C compiler: cfg the real-C tests out, change nothing
        // else. The Rust extern "C" probes still cover the window math.
        println!(
            "cargo:warning=j2r-jni-ffi: no C compiler found; the real-C \
             probe tests are disabled (the crate builds unchanged)"
        );
        return;
    }
    // Test binaries only: the library target never names these symbols,
    // so ordinary (and vendored) builds link nothing extra.
    println!("cargo:rustc-link-arg-tests={}", obj.display());
    println!("cargo:rustc-cfg=j2r_c_probe");
}
