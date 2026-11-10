//! The REAL C probe through primitive 11 (M5_JNI_HARNESS.md §6-13b).
//!
//! `build.rs` compiles the probe C source with the platform C compiler
//! and links the object into this package's test binaries only
//! (`cargo:rustc-link-arg-tests`) — the first C in the repo. These tests
//! prove the thunk's transmute against ACTUAL C-compiler output, not
//! just Rust's `extern "C"` shapes: if the SysV register-window argument
//! in the design note (§2) were wrong for real C, this file fails.
//!
//! cfg'd out entirely on a box with no C compiler (the Rust-shaped unit
//! probes in `src/ffi.rs` still cover the window math there).

#![cfg(j2r_c_probe)]

use j2r_jni_ffi::ffi::call_foreign_native;
use std::os::raw::c_void;

extern "C" {
    /// `jint`-shaped native: integer-class args only, INTEGER return.
    pub fn c_probe_add(env: *mut c_void, recv: *mut c_void, a: usize, b: usize) -> u64;
    /// `(I D)D`-shaped native: mixed classification, SSE return.
    pub fn c_probe_mad(env: *mut c_void, recv: *mut c_void, a: usize, b: f64) -> f64;
}

#[test]
fn real_c_integer_channel_gp_window_and_rax_return() {
    // (II)I — the GP window carries both args (rdx/rcx after env+recv);
    // the INTEGER return channel (rax) reads the C function's result.
    let ret = call_foreign_native(
        c_probe_add as *const c_void,
        0x1000 as *mut c_void,
        0x2000 as *mut c_void,
        &[20, 22],
        &[],
    );
    assert_eq!(ret.i, 42, "real C: the GP words land in rdx/rcx order");
}

#[test]
fn real_c_mixed_classification_and_sse_return_channel() {
    // (I D)D — the Int rides the GP window INDEPENDENTLY of the FP
    // window (SysV classification); the SSE return channel (xmm0)
    // reads the C function's double result.
    let mixed = call_foreign_native(
        c_probe_mad as *const c_void,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        &[21],
        &[2.0],
    );
    assert!(
        (mixed.d - 42.0).abs() < 1e-9,
        "real C: the SSE return channel works"
    );
}
