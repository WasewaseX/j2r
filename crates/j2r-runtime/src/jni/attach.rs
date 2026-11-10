//! `j2r-runtime::jni::attach` — the VM attach lifecycle (T13 13d).
//!
//! Design of record: docs/M5_JNI_HARNESS.md §9 (the 13d precision
//! paragraph) and REVIEW_CHECKLIST G1.4. The MEASURED oracle contract
//! (Temurin 21.0.13+9, probed live by the 13d C probe committed under
//! `scripts/evidence/attach_lifecycle_probe/`, 2026-11-10):
//!
//! * a fresh (never-attached) thread's VM `GetEnv` answers
//!   `JNI_EDETACHED`; the same thread after `DetachCurrentThread` again
//!   answers `JNI_EDETACHED`;
//! * `AttachCurrentThread` answers `JNI_OK` and hands out the env table;
//!   a re-attach of an ALREADY-attached thread answers `JNI_OK` too
//!   (idempotent) — and `AttachCurrentThreadAsDaemon` on an attached
//!   thread answers `JNI_OK`;
//! * `DetachCurrentThread` answers `JNI_OK`; extra detaches of an
//!   unattached thread answer `JNI_OK` as well (measured: three
//!   consecutive detaches all `JNI_OK`) — the oracle tolerates, it does
//!   not punish;
//! * the main thread is attached at birth (the 13c battery's
//!   `probe_version` GetEnv arm proves it on the oracle).
//!
//! The registry here is deliberately tiny: one global set of attached
//! thread ids. Detaching sweeps the thread's local-ref frame stacks via
//! [`crate::jni::bridge::detach_thread`] — the 12b-wired hook — so the
//! frame-stack lifecycle is exercised end to end by the C harness's
//! pthread battery (G1.4).

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

fn attached() -> &'static Mutex<HashSet<std::thread::ThreadId>> {
    static ATTACHED: OnceLock<Mutex<HashSet<std::thread::ThreadId>>> = OnceLock::new();
    ATTACHED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Marks the calling thread attached (idempotent). The generated program
/// calls this for the main thread at startup (HotSpot semantics: main is
/// attached at birth); the VM-table `AttachCurrentThread` slot calls it
/// for every foreign thread.
pub fn mark_attached() {
    attached()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(std::thread::current().id());
}

/// Whether the calling thread is currently attached.
pub(crate) fn is_attached() -> bool {
    attached()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&std::thread::current().id())
}

/// The detach hook: unregisters the calling thread and, when it WAS
/// attached, sweeps its frame stack (every local ref it still holds is
/// freed; globals are untouched — the JNI law). Returns whether the
/// thread was attached (the slot uses it only for its own bookkeeping;
/// the measured oracle answers `JNI_OK` either way).
pub(crate) fn run_detach_hook() -> bool {
    let was = attached()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&std::thread::current().id());
    if was {
        crate::jni::bridge::detach_thread();
    }
    was
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_registry_marks_and_unmarks_the_calling_thread() {
        // Fresh cargo-test worker thread: never attached.
        assert!(!is_attached());
        mark_attached();
        assert!(is_attached());
        // Idempotent (the measured oracle answers OK to a re-attach; the
        // registry shape must tolerate it the same way).
        mark_attached();
        assert!(is_attached());
        // The detach hook unregisters AND runs the frame-stack sweep
        // (bridge::detach_thread on a thread with no frames is a no-op).
        assert!(run_detach_hook());
        assert!(!is_attached());
        // A detach of an unattached thread is a tolerated no-op answering
        // the same shape (the measured oracle: extra detaches are OK).
        assert!(!run_detach_hook());
    }
}
