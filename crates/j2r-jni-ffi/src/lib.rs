//! `j2r-jni-ffi` — the audited FFI memory primitives for the JNI bridge.
//!
//! This crate is the workspace's **single scoped `unsafe` exception** to the
//! standing zero-`unsafe` law. The law amendment, the primitive list, the
//! 35-slot inventory that needs them and the gate-10 pinning are the design
//! of record in [`docs/M5_JNI_FFI.md`](../../docs/M5_JNI_FFI.md) (T09's
//! gate); read that note before changing anything here.
//!
//! The rules that make the exception auditable:
//!
//! * Every `unsafe` occurrence lives in `src/ffi.rs`, inside one of the ≤ 6
//!   primitive functions listed in M5_JNI_FFI.md §4, with a `// SAFETY:`
//!   comment naming the JNI contract that justifies it. No `unsafe` may
//!   appear anywhere else in this crate (clippy + gate-10 enforce it).
//! * Each primitive is a **total function**: a null pointer or zero length
//!   degrades to a no-op / empty result — never undefined behavior. The
//!   *policy* (what to do about a null) belongs to the safe wrappers in
//!   `j2r-runtime::jni::bridge`, which reject before calling and panic
//!   loudly; these primitives only guarantee "no UB, ever".
//! * The crate has zero dependencies (same law as `j2r-runtime` and
//!   `j2r-classfile`) and is reachable only from `j2r-runtime::jni` —
//!   never from generated code or other crates.
//! * gate-10 counts the `unsafe` occurrences in `src/ffi.rs` and pins the
//!   count (≤ 12 at adoption; the T09 evidence record carries the final
//!   number). The gate FAILS if this crate disappears — the exception is
//!   an explicit allowlist entry, not a blind spot.

pub mod ffi;

/// The audited `unsafe` occurrence pin (M5_JNI_FFI.md §3). gate-10 asserts
/// the `ffi.rs` count against this from the CI side; keeping the number here
/// too makes a drift a compile-visible const mismatch during local work.
pub const AUDITED_UNSAFE_OCCURRENCES: u32 = 12;
