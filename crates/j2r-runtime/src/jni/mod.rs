//! `j2r-runtime::jni` — the JNI bridge scaffolding (M5 wave 2, T08).
//!
//! Design of record: M5_PLAN §3 and STRATEGIC_GAPS §R3. The ABI this
//! module presents is **the Temurin 21 `jni.h`** — the same JDK the whole
//! project uses as its differential oracle — regenerated mechanically by
//! `scripts/gen_jni_env_table.py` into [`tables`]:
//!
//! * [`tables::JNINativeInterface_`] holds all 235 slots (4 reserved NULL
//!   plus 231 functions) index-for-index identical to the header, pinned
//!   by a const-evaluated `offset_of!` assertion per slot — layout drift
//!   is a compile error, never a segfault inside someone else's binary.
//! * [`tables::JNIInvokeInterface_`] holds the 8-slot VM table.
//! * Every non-reserved slot holds a real function or a **loud stub**
//!   naming the slot and its wave-2 task. No null slots — a foreign
//!   library calling an unimplemented slot fails loudly instead of
//!   crashing. The loudness shape is a process abort, not a catchable
//!   panic: panics cannot unwind across the `extern "system"` boundary
//!   (Rust >= 1.81), so the default hook prints the slot-named message
//!   and the process aborts — never a silent wrong answer.
//! * The two static entry points handed to foreign code,
//!   [`tables::jni_env`] and [`tables::java_vm`], are created without a
//!   single `unsafe`: creating a raw pointer is safe; only the foreign
//!   side dereferences it (their FFI is theirs — M5_PLAN §3.1).
//!
//! Real functions so far (later tasks fill more): slot 4 `GetVersion`
//! (reports `JNI_VERSION_1_8`, the LWJGL3 requirement) and VM slot 6
//! `GetEnv`'s unsupported-version path.
//!
//! ## Registered wave-2 findings (owned by T09/T11, not hidden)
//!
//! **F-J1 — pointer-out slots vs zero-`unsafe`. RESOLVED (T09).** JNI
//! functions that must write through a caller-supplied pointer (`GetEnv`
//! on success, `AttachCurrentThread`/`AttachCurrentThreadAsDaemon`, the
//! `Get<Prim>ArrayRegion`/`GetStringRegion` families writing into caller
//! buffers) cannot be implemented in a crate with
//! `#![forbid(unsafe_code)]`. The resolution, per the design of record in
//! `docs/M5_JNI_FFI.md`, is the dedicated dependency-free crate
//! `j2r-jni-ffi` (this workspace's single scoped `unsafe` exception, its
//! occurrence count pinned by gate-10); its primitives are reachable only
//! from here, and the SAFE half of the bridge lives in [`bridge`] — the
//! handle table, local/global refs, frames and the critical registry, all
//! under this crate's `forbid(unsafe_code)`.
//!
//! **F-J2 — variadic call slots.** The C-varargs `NewObject` and
//! `Call*Method` slot variants are typed at their A-form arity (args
//! pointer widened to `*const c_void`) and the V variants take a
//! pointer-width `va_list` stand-in: a stub never reads either, and on
//! SysV x86_64 the never-read registers are caller-owned anyway. The real
//! variadic-ABI trade-off note is T11's deliverable.
//!
//! ## Type-fidelity notes
//!
//! * `jclass`/`jstring`/`jthrowable`/`jweak`/`jarray`/every `j*Array` are
//!   `jobject` aliases — exactly the header's own aliasing (verified
//!   mechanically by the generator, which fails loudly on any mismatch).
//! * `jmethodID`/`jfieldID` are opaque pointer-width handles (the header
//!   uses opaque struct pointers; identical ABI). T08 initially mapped
//!   them to bare `c_void` in the table signatures — a stand-in that
//!   made those slots uninhabited; the T10 slice-3 amendment restored
//!   the ABI-faithful pointer form (verified against the header's own
//!   `typedef struct _jfieldID *jfieldID;` by the generator). The
//!   handle-bridge gives them their meaning: a `jfieldID` is a marked
//!   tag into `jni::classes`'s field-ID registry.
//! * `GetPrimitiveArrayCritical` (slot 213) pins trivially because the
//!   M2 arena never moves objects — a documented design advantage, restated
//!   in the T09 note.

pub mod bridge;
pub mod classes;
pub mod strings;
pub mod tables;

pub use bridge::{
    critical_register, critical_release, delete_global, delete_local, detach_thread,
    jobject_to_tag, new_global, new_local, pop_local_frame, push_local_frame, resolve,
    tag_to_jobject,
};
pub use tables::{
    java_vm, jni_env, JNIEnv, JNIEnv_, JNIInvokeInterface_, JNINativeInterface_, JObjectOpaque,
    JavaVM, JavaVM_,
};

// ---------------------------------------------------------------------------
// The host dispatch slot (T10 slice 3 — design of record:
// docs/M5_JNI_CLASSES.md §1). The env-table slots are `extern "system"`
// fn(JNIEnv, ...) — the jni.h ABI fixes the signatures, so no context
// parameter can be threaded to a slot body, and `NativeFn`'s
// `ctx: Dispatch` never reaches FFI entries. The generated program
// therefore installs its composed Dispatch here at startup (one emission
// in `main_run`, next to the statics init — both the single-crate and the
// partitioned layouts install before any Java code runs; in partitioned
// builds the installed value is the composed dispatch, so the JNI surface
// sees every partition's classes).
// ---------------------------------------------------------------------------

/// The installed program dispatch, `None` until
/// [`install_host_dispatch`] runs.
static HOST_DISPATCH: std::sync::Mutex<Option<crate::Dispatch>> = std::sync::Mutex::new(None);

/// Installs the generated program's [`crate::Dispatch`] as the JNI
/// surface's dispatch context. Called once by the emitted `main_run`
/// before any Java code runs (M5_JNI_CLASSES.md §1).
pub fn install_host_dispatch(d: crate::Dispatch) {
    *HOST_DISPATCH.lock().unwrap_or_else(|e| e.into_inner()) = Some(d);
}

/// The installed dispatch, or the all-`None` default before install
/// (slots that need compiled-program facts then fail their existence
/// checks loudly/honestly instead of guessing). Every JNI class/field/
/// array slot reads through here; never held across heap work (lock law
/// L-J2).
pub(crate) fn host_dispatch() -> crate::Dispatch {
    HOST_DISPATCH
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .unwrap_or_default()
}

// Values verified against the pinned Temurin 21 jni.h (lines 160-166,
// 1986-1995). The unit tests below re-assert the ones this module uses.
pub const JNI_OK: i32 = 0;
pub const JNI_ERR: i32 = -1;
pub const JNI_EDETACHED: i32 = -2;
pub const JNI_EVERSION: i32 = -3;
pub const JNI_ENOMEM: i32 = -4;
pub const JNI_EEXIST: i32 = -5;
pub const JNI_EINVAL: i32 = -6;

pub const JNI_VERSION_1_1: i32 = 0x0001_0001;
pub const JNI_VERSION_1_2: i32 = 0x0001_0002;
pub const JNI_VERSION_1_4: i32 = 0x0001_0004;
pub const JNI_VERSION_1_6: i32 = 0x0001_0006;
pub const JNI_VERSION_1_8: i32 = 0x0001_0008;
pub const JNI_VERSION_9: i32 = 0x0009_0000;
pub const JNI_VERSION_10: i32 = 0x000a_0000;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jni::tables::{env_slot_addrs, java_vm, jni_env, ENV_TABLE, VM_TABLE};
    use std::os::raw::{c_char, c_void};
    use std::ptr;

    #[test]
    fn get_version_through_the_table_is_1_8() {
        // Copy the slot-4 function pointer out of the static table and call
        // it — the same positional ABI a foreign library uses, exercised
        // with zero unsafe (calling a copied extern fn value is safe).
        let get_version: extern "system" fn(JNIEnv) -> i32 = ENV_TABLE.GetVersion;
        assert_eq!(get_version(jni_env()), JNI_VERSION_1_8);
    }

    #[test]
    fn every_env_slot_is_non_null() {
        let slots = env_slot_addrs();
        assert_eq!(slots.len(), 231);
        for (name, addr) in slots {
            assert_ne!(addr, 0, "slot {name} is null — a native lib would segfault");
        }
    }

    #[test]
    fn env_and_vm_point_at_the_static_tables() {
        // JNIEnv_/JavaVM_ wrap the table pointer as their first (offset-0)
        // field; the statics must hand out exactly the generated tables.
        assert!(ptr::eq(
            tables::JNI_ENV.functions,
            &tables::ENV_TABLE as *const tables::JNINativeInterface_
        ));
        assert!(ptr::eq(
            tables::JAVA_VM.functions,
            &tables::VM_TABLE as *const tables::JNIInvokeInterface_
        ));
    }

    #[test]
    fn get_env_rejects_null_penv_and_unsupported_versions() {
        let get_env: extern "system" fn(JavaVM, *mut *mut c_void, i32) -> i32 = VM_TABLE.GetEnv;
        assert_eq!(
            get_env(java_vm(), ptr::null_mut(), JNI_VERSION_1_8),
            JNI_ERR
        );
        let mut slot: *mut c_void = ptr::null_mut();
        assert_eq!(
            get_env(java_vm(), &mut slot as *mut *mut c_void, JNI_VERSION_10),
            JNI_EVERSION
        );
        // the unsupported-version path must not have written anything
        assert!(slot.is_null());
    }

    #[test]
    fn get_env_supported_version_aborts_loudly() {
        // A supported version cannot yet be served (F-J1: the required
        // pointer write is impossible under forbid(unsafe_code)); the slot
        // must abort the process with a message naming the slot — panics
        // cannot unwind across the extern "system" boundary, so the stub's
        // panic aborts. Observed the soe_e2e way: a child process.
        if std::env::var("J2R_JNI_STUB_ABORT_CHILD").is_ok() {
            let get_env: extern "system" fn(JavaVM, *mut *mut c_void, i32) -> i32 = VM_TABLE.GetEnv;
            let mut slot: *mut c_void = ptr::null_mut();
            let _ = get_env(java_vm(), &mut slot as *mut *mut c_void, JNI_VERSION_1_8);
            return; // unreachable: the call above aborts
        }
        let exe = std::env::current_exe().expect("current_exe");
        let out = std::process::Command::new(exe)
            .args([
                "jni::tests::get_env_supported_version_aborts_loudly",
                "--exact",
                "--nocapture",
            ])
            .env("J2R_JNI_STUB_ABORT_CHILD", "1")
            .output()
            .expect("spawn child test process");
        assert!(
            !out.status.success(),
            "supported-version GetEnv must abort, got {out:?}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(
                out.status.signal(),
                Some(6), // SIGABRT
                "abort is the loud failure shape, got {out:?}"
            );
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("GetEnv"),
            "the abort message must name the slot, got: {stderr}"
        );
    }

    #[test]
    fn release_string_utf_chars_registry_miss_aborts_loudly() {
        // T10 slice 2: a pointer that GetStringUTFChars never handed out is
        // a registry miss (the T09 loud law). The panic starts inside
        // j2r-runtime::jni::strings and must cross the extern "system"
        // boundary of the generated wrapper, where unwinding is impossible:
        // the process aborts. Pinned the soe_e2e way: a child process.
        if std::env::var("J2R_JNI_RELEASE_ABORT_CHILD").is_ok() {
            let release: extern "system" fn(JNIEnv, tables::jobject, *const c_char) =
                ENV_TABLE.ReleaseStringUTFChars;
            let bogus_string: tables::jobject = 1 as tables::jobject; // non-null: passes the wrapper's null guard
                                                                      // Non-null, aligned, never handed out by GetStringUTFChars —
                                                                      // exactly the foreign-pointer shape the loud law condemns.
            let foreign_chars: *const c_char = std::ptr::dangling::<c_char>();
            release(jni_env(), bogus_string, foreign_chars);
            return; // unreachable: the call above aborts
        }
        let exe = std::env::current_exe().expect("current_exe");
        let out = std::process::Command::new(exe)
            .args([
                "jni::tests::release_string_utf_chars_registry_miss_aborts_loudly",
                "--exact",
                "--nocapture",
            ])
            .env("J2R_JNI_RELEASE_ABORT_CHILD", "1")
            .output()
            .expect("spawn child test process");
        assert!(
            !out.status.success(),
            "a foreign-pointer release must abort, got {out:?}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(
                out.status.signal(),
                Some(6), // SIGABRT
                "abort is the loud failure shape, got {out:?}"
            );
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("ReleaseStringUTFChars"),
            "the abort message must name the slot family, got: {stderr}"
        );
    }

    #[test]
    fn version_and_error_constants_match_the_header() {
        assert_eq!(JNI_OK, 0);
        assert_eq!(JNI_ERR, -1);
        assert_eq!(JNI_EVERSION, -3);
        assert_eq!(JNI_VERSION_1_1, 0x0001_0001);
        assert_eq!(JNI_VERSION_1_4, 0x0001_0004);
        assert_eq!(JNI_VERSION_1_8, 0x0001_0008);
        assert_eq!(JNI_VERSION_9, 0x0009_0000);
        assert_eq!(JNI_VERSION_10, 0x000a_0000);
    }

    #[test]
    fn entry_point_constructors_are_plain_table_pointers() {
        // The pointers handed to foreign code point at the static wrapper
        // structs whose first word is the table pointer (offset-0, pinned
        // by the generated const assertion table).
        assert_eq!(std::mem::size_of::<JNIEnv>(), std::mem::size_of::<usize>());
        assert_eq!(std::mem::size_of::<JavaVM>(), std::mem::size_of::<usize>());
        let _env = jni_env();
        let _vm = java_vm();
    }
}
