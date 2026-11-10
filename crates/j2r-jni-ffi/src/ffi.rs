//! The audited primitive surface (M5_JNI_FFI.md §4).
//!
//! Twelve total functions; every `unsafe` occurrence in the workspace outside
//! `#![forbid(unsafe_code)]` lives here, inside these functions, with a
//! `// SAFETY:` justification naming the JNI contract. gate-10 counts and
//! pins the occurrences (`AUDITED_UNSAFE_OCCURRENCES` = 12; the T09
//! evidence record carries the actual final count).
//!
//! Design refinements registered at T09 (vs the note's first draft):
//! * `read_caller_str` returns the raw bytes up to NUL (`Option<Vec<u8>>`)
//!   rather than a `String`: modified UTF-8 is not always valid UTF-8 (the
//!   encoded NUL `0xC0 0x80`, surrogate pairs), and the mUTF-8 decode
//!   belongs safe-side in `j2r-runtime`, which owns the classfile decoder.
//!   The note's own wording already required the decode to be safe-side.
//! * `write_ptr_out` takes the out-value as `usize` (the note's shape) and
//!   writes it as one pointer-sized word — the only pointer-width store
//!   the pointer-out slots need.
//!
//! T11 slice 11c amendment (M5_JNI_CALLS.md §3 decision 2): primitives
//! 8–10 (`read_u32_at`, `read_word_at`, `read_fp_word_at`) serve the SysV
//! `__va_list_tag` save-area walk the `Call*MethodV` forms need. Reads
//! only — the walk consumes the record into a LOCAL cursor safe-side, so
//! no write primitive exists to misuse. The occurrence pin (≤ 12) still
//! holds with headroom (9 used).
//!
//! T13 slice 13b added primitive 11 (`call_foreign_native`, the fixed
//! SysV register-window call thunk — occurrences 9 → 10). T13 slice 13c
//! adds primitive 12 (`load_foreign_jni_library` — dlopen + dlsym + the
//! ABI-fixed `JNI_OnLoad` call in ONE audited block; occurrences 10 →
//! 11), the loading story's runtime half (docs/M5_JNI_HARNESS.md §7).
//! 11 of the 12-slot pin used; one headroom slot remains.

use std::os::raw::{c_char, c_void};

/// 1. Read a caller-owned byte region (`Set<Prim>ArrayRegion`, `DefineClass`
///    class bytes, `NewString`).
///
/// Total: a null `src` or zero `len` yields an empty `Vec` — the safe
/// wrapper in `j2r-runtime::jni::bridge` turns a null into a loud JNI-level
/// failure before this is ever called.
///
/// # SAFETY (the one `unsafe` below)
/// Caller contract (JNI spec): `src` points to `len` readable bytes for the
/// duration of the call. Used only by `j2r-runtime::jni` slot
/// implementations immediately on entry, with the region length the JNI
/// signature itself supplies (`len`/`buflen` parameters).
#[allow(clippy::not_unsafe_ptr_arg_deref)] // deliberate: total fn, null/zero-len guarded
pub fn copy_from_caller(src: *const u8, len: usize) -> Vec<u8> {
    if src.is_null() || len == 0 {
        return Vec::new();
    }
    // SAFETY: per the JNI caller contract above, `src` names `len` readable
    // bytes; `from_raw_parts` only borrows them for the `to_vec` copy.
    unsafe { std::slice::from_raw_parts(src, len).to_vec() }
}

/// 2. Write a caller-owned byte region (`Get<Prim>ArrayRegion`,
///    `GetStringRegion`, `GetStringUTFRegion`).
///
/// Total: null `dst` or empty `src` is a no-op. Byte-oriented: the element
/// width conversion (`GetIntArrayRegion` writes `jint`s) is done safe-side
/// by passing the little-endian/native byte view of the values — the JNI
/// ABI hands the region a raw byte buffer either way.
///
/// # SAFETY (the one `unsafe` below)
/// Caller contract (JNI spec): `dst` holds `src.len()` writable bytes for
/// the duration of the call.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // deliberate: total fn, null/empty guarded
pub fn copy_to_caller(dst: *mut u8, src: &[u8]) {
    if dst.is_null() || src.is_empty() {
        return;
    }
    // SAFETY: per the JNI caller contract above, `dst` names at least
    // `src.len()` writable bytes; `copy_nonoverlapping` never reads `dst`.
    unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len()) }
}

/// 3. Write one pointer-sized out-param (`GetJavaVM`, `GetEnv` success path,
///    `AttachCurrentThread`/`AttachCurrentThreadAsDaemon`).
///
/// Total: null `slot` is a no-op (the wrapper reports `JNI_ERR` instead —
/// a null out-slot is a caller bug the safe side refuses loudly).
///
/// # SAFETY (the one `unsafe` below)
/// Caller contract (JNI spec): `slot` points to one writable
/// pointer-sized out-slot (the `void**`/`JavaVM**`/`JNIEnv**` the caller
/// passed by address).
#[allow(clippy::not_unsafe_ptr_arg_deref)] // deliberate: total fn, null guarded
pub fn write_ptr_out(slot: *mut *mut c_void, value: usize) {
    if slot.is_null() {
        return;
    }
    // SAFETY: per the JNI caller contract above, `slot` is one valid
    // pointer-sized out-slot; this is its single specified write.
    unsafe {
        *slot = value as *mut c_void;
    }
}

/// 4. Read a NUL-terminated caller-owned C string (`ThrowNew`, `DefineClass`
///    name, `NewStringUTF`), returning the raw bytes **without** the
///    terminator. The modified-UTF-8 decode happens safe-side in
///    `j2r-runtime` (see the module-level refinement note).
///
/// Total: null → `None`.
///
/// # SAFETY (the one `unsafe` below)
/// Caller contract (JNI spec): `ptr` is a NUL-terminated C string owned by
/// the caller for the duration of the call. `CStr::from_ptr` reads up to
/// and including the first NUL.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // deliberate: total fn, null guarded
pub fn read_caller_str(ptr: *const c_char) -> Option<Vec<u8>> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: per the JNI caller contract above, `ptr` is a valid
    // NUL-terminated C string; `from_ptr` stops at the terminator.
    let bytes = unsafe { std::ffi::CStr::from_ptr(ptr) }.to_bytes();
    Some(bytes.to_vec())
}

/// 5. Re-read a region behind a pointer **we** handed out earlier
///    (`Release<Prim>ArrayElements` copy-back for copy-mode registrations).
///
/// The registry check (address-as-usize match, M5_JNI_FFI.md §2 option (c))
/// happens safe-side in `j2r-runtime::jni::bridge` BEFORE this runs; a
/// registry miss never reaches this function. `mode` handling (JNI_ABORT /
/// JNI_COMMIT) is also safe-side — this primitive only performs the raw
/// re-read the copy-back needs.
///
/// Total: null `elems` or empty `dst` is a no-op.
///
/// # SAFETY (the one `unsafe` below)
/// Contract: `elems` is a pointer this runtime handed out via
/// `Get<Prim>ArrayElements` and the registry matched before the call; the
/// caller may have mutated its contents — re-reading them is the
/// operation's purpose — so the read cannot be an aliasing violation on
/// our side.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // deliberate: total fn, null/empty guarded
pub fn copy_back_elements(elems: *mut c_void, dst: &mut [u8]) {
    if elems.is_null() || dst.is_empty() {
        return;
    }
    // SAFETY: `elems` is registry-matched storage we allocated (the JNI
    // copy-out buffer), `dst.len()` bytes of which remain valid and
    // caller-writable by construction; reading them is the JNI-specified
    // copy-back.
    unsafe { std::ptr::copy_nonoverlapping(elems as *const u8, dst.as_mut_ptr(), dst.len()) }
}

/// 6. Normalize a direct-buffer address for bookkeeping
///    (`NewDirectByteBuffer` / `GetDirectBufferAddress` records).
///
/// No dereference — address arithmetic only, hence **zero** `unsafe`:
/// the pointer→`usize` cast is safe. Kept in this crate so the audited
/// surface is the complete, closed list from the design note.
pub fn direct_buffer_ok(ptr: *mut c_void) -> usize {
    ptr as usize
}

/// 7. Write one byte out-param (`GetStringUTFChars`'s `isCopy`, and later
///    the `Get<Prim>ArrayElements` family's `isCopy`). Added by the T10
///    slice-2 amendment of M5_JNI_FFI.md §3/§4: primitive 3 writes a full
///    pointer word and would overrun the 1-byte `jboolean` out-slot, so
///    the audited surface grows 6 → 7 primitives; the occurrence pin
///    (≤ 12) is unchanged and still holds with headroom.
///
/// Total: null `slot` is a no-op (the JNI spec: a null out-param means the
/// caller does not ask — the wrapper skips the write entirely).
///
/// # SAFETY (the one `unsafe` below)
/// Caller contract (JNI spec): `slot` points to one writable `jboolean`
/// out-slot for the duration of the call; this is its single specified
/// write.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // deliberate: total fn, null guarded
pub fn write_u8_out(slot: *mut u8, value: u8) {
    if slot.is_null() {
        return;
    }
    // SAFETY: per the JNI caller contract above, `slot` is one valid
    // writable byte out-slot; this is its single specified write.
    unsafe {
        *slot = value;
    }
}

/// 8. Read one 4-byte little-endian word at `ptr` — the `gp_offset` /
///    `fp_offset` cursor fields of the SysV `__va_list_tag` record the
///    `Call*MethodV` forms receive. Added by the T11 slice-11c amendment
///    (M5_JNI_CALLS.md §3 decision 2); the walk itself is safe-side in
///    `j2r-runtime::jni::calls` and consumes the record into a LOCAL
///    cursor, so no write primitive exists to misuse.
///
/// Total: null `ptr` → `None` (the walk turns it into a loud
/// internal-error pend — an unreadable record is never silently
/// mis-marshalled).
///
/// # SAFETY (the one `unsafe` below)
/// Contract (x86-64 SysV ABI §3.5.6): `ptr` names 4 readable bytes —
/// a cursor field of the 24-byte `__va_list_tag` record whose address the
/// C caller passed as the `va_list` argument. Aligned reads suffice (the
/// ABI lays the record out 4/4/8/8); `read_unaligned` is the honest
/// weaker form and costs nothing here.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // deliberate: total fn, null guarded
pub fn read_u32_at(ptr: *const u8) -> Option<u32> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: per the ABI contract above, `ptr` names 4 readable bytes of
    // the caller's va_list record; one aligned 4-byte read, no write.
    Some(unsafe { (ptr as *const u32).read_unaligned() })
}

/// 9. Read one 8-byte little-endian word at `ptr` — a GP-class vararg
///    slot: either inside the save area's 48-byte GP region
///    (`reg_save_area + gp_offset`) or an 8-byte overflow-area word.
///    Added by the T11 slice-11c amendment (M5_JNI_CALLS.md §3
///    decision 2).
///
/// Total: null `ptr` → `None` (loud internal-error pend safe-side).
///
/// # SAFETY (the one `unsafe` below)
/// Contract (x86-64 SysV ABI §3.5.6 + the JNI V-form caller): `ptr` is
/// the exact address the record's cursors name for the next GP vararg —
/// 8 readable bytes holding the argument word. One 8-byte read, no
/// write; `read_unaligned` covers an overflow area the C caller aligned
/// to 8 anyway.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // deliberate: total fn, null guarded
pub fn read_word_at(ptr: *const u8) -> Option<u64> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: per the ABI contract above, `ptr` names 8 readable bytes —
    // the next GP vararg word; one read, no write.
    Some(unsafe { (ptr as *const u64).read_unaligned() })
}

/// 10. Read the value half (low 8 bytes) of one 16-byte SSE save-area
///     slot — `reg_save_area + fp_offset`, addressed by the record's
///     `fp_offset` cursor (`f32` lives in the low 4 bytes, `f64` in the
///     low 8; the walk advances `fp_offset` by 16 per SSE vararg per the
///     ABI). Added by the T11 slice-11c amendment (M5_JNI_CALLS.md §3
///     decision 2).
///
/// Total: null `ptr` → `None` (loud internal-error pend safe-side).
///
/// # SAFETY (the one `unsafe` below)
/// Contract (x86-64 SysV ABI §3.5.6 + the JNI V-form caller): `ptr` is
/// `reg_save_area + fp_offset` — the low half of one 16-byte XMM save
/// slot, 8 readable bytes holding the FP argument. One 8-byte read, no
/// write; the upper 8 bytes of the slot are never touched.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // deliberate: total fn, null guarded
pub fn read_fp_word_at(ptr: *const u8) -> Option<u64> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: per the ABI contract above, `ptr` names the 8 value bytes
    // of one XMM save slot; one read, no write.
    Some(unsafe { (ptr as *const u64).read_unaligned() })
}

/// 11. Call a bare-form foreign native through the FIXED SysV
///     max-register signature — the OUTBOUND half of the variadic
///     answer (M5_JNI_HARNESS.md §2; the inbound halves are the A-forms
///     and the 11c save-area walk). Added by the T13 slice-13b
///     amendment; the occurrence pin (≤ 12) still holds (10 used).
///
/// The fixed signature `(env, recv, usize × 4, f64 × 8)`: on x86-64
/// SysV the integer-class args land in rdi/rsi/rdx/rcx/r8/r9 (env +
/// recv consume the first two) and the SSE-class args land in xmm0–7
/// IN ORDER — so a bare C native whose varargs fit 4 GP + 8 FP
/// register words is called correctly, with `gp`/`fp` filled per the
/// descriptor's classification (the same marshalling law the A-form
/// decode uses, applied safe-side in `j2r-runtime::jni::natives`).
///
/// The return is read as BOTH channels — the 16-byte `NativeRet`
/// (INTEGER + SSE) comes back in rax + xmm0, so a C fn returning any
/// jint/jlong/jobject-shaped word is read from `.i` and a jdouble (or
/// jfloat) from `.d`; the caller picks per the return descriptor.
/// This is why ONE thunk serves every return kind without lying about
/// the callee's own return type.
///
/// Beyond the register window (`gp.len() > 4 || fp.len() > 8`): a LOUD
/// panic naming the window law — stack-frame construction is libffi's
/// exact problem and stays banned by both standing laws. The harness
/// fixtures stay inside the window (documented per fixture).
///
/// # SAFETY (the one `unsafe` below)
/// Contract (M5_JNI_HARNESS.md §2): `ptr` is the `fnPtr` the foreign
/// caller handed `RegisterNatives` for a native whose declared
/// descriptor classifies into the register window above; calling it
/// with (env, recv, gp, fp) in the ABI's own register order is exactly
/// what the JVM itself does for the same registration. The callee owns
/// its own safety; we never dereference `ptr` as data.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // deliberate: the pointer IS a fn to call
#[allow(clippy::too_many_arguments)] // the fixed SysV shape is the point
pub fn call_foreign_native(
    ptr: *const c_void,
    env: *mut c_void,
    recv: *mut c_void,
    gp: &[usize],
    fp: &[f64],
) -> NativeRet {
    assert!(
        gp.len() <= 4 && fp.len() <= 8,
        "j2r JNI bridge: foreign native call outside the SysV register \
         window ({} GP words, {} FP words; max 4 + 8) — stack-frame \
         construction is not served (libffi stays banned); widen the \
         harness fixture or bind a Rust-shape native",
        gp.len(),
        fp.len()
    );
    type BareNative = extern "C" fn(
        *mut c_void, // env
        *mut c_void, // recv / jclass
        usize,
        usize,
        usize,
        usize, // gp words (rdx, rcx, r8, r9)
        f64,
        f64,
        f64,
        f64,
        f64,
        f64,
        f64,
        f64, // fp words (xmm0..7)
    ) -> NativeRet;
    let w = |k: usize| gp.get(k).copied().unwrap_or(0);
    let d = |k: usize| fp.get(k).copied().unwrap_or(0.0);
    // SAFETY: per the contract above — `ptr` is a registered C native
    // fn inside the register window; the transmute only fixes the
    // calling convention (the SysV classification argument is in the
    // design note), and the call passes the ABI's own register shapes.
    unsafe {
        let f: BareNative = std::mem::transmute(ptr);
        f(
            env,
            recv,
            w(0),
            w(1),
            w(2),
            w(3),
            d(0),
            d(1),
            d(2),
            d(3),
            d(4),
            d(5),
            d(6),
            d(7),
        )
    }
}

/// The dual-channel return of [`call_foreign_native`]: the SysV
/// two-eightbyte {INTEGER, SSE} return lands in rax + xmm0; the caller
/// picks the channel per the native's return descriptor.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeRet {
    /// The INTEGER-class return word (rax): jint/jlong/jobject/void.
    pub i: u64,
    /// The SSE-class return word (xmm0): jfloat (low 4 bytes) / jdouble.
    pub d: f64,
}

// -- primitive 12: the foreign-library loader (13c, docs/M5_JNI_HARNESS.md
//    §7). The three foreign calls share ONE audited unsafe block.

const RTLD_NOW: std::os::raw::c_int = 2;
const RTLD_LOCAL: std::os::raw::c_int = 0;
const JNI_ON_LOAD_SYM: &[u8] = b"JNI_OnLoad\0";

extern "C" {
    fn dlopen(filename: *const c_char, flags: std::os::raw::c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *mut c_char;
}

/// Primitive 12: load a foreign JNI library and run its `JNI_OnLoad` —
/// the `System.loadLibrary` runtime half (T13 13c; the loading story is
/// design-of-record in docs/M5_JNI_HARNESS.md §7).
///
/// `path` is the platform library filename (`libjni_harness.so`);
/// `vm` is the VM table pointer the library's `JNI_OnLoad` receives (so
/// it can `GetEnv` the env table and bind its natives through slot 215).
/// The handle is deliberately never `dlclose`d — a JNI library lives for
/// the process (HotSpot semantics), and a close would invalidate the
/// bindings the natives registry now holds.
///
/// Returns the version `JNI_OnLoad` answered; 0 = loaded, no `JNI_OnLoad`
/// symbol (the spec's default-version case). A dlopen failure is a LOUD
/// abort naming the loader's own error text (never a silent fake success;
/// the safe caller maps Java-visible failure semantics — v1: the parity
/// battery only loads libraries that exist, and the failure path is
/// pinned by a child-process test).
///
/// # SAFETY (the one `unsafe` below)
/// `path` is a NUL-terminated C string owned by the caller for the call
/// (the `read_caller_str` contract); dlopen/dlsym follow the platform
/// loader's documented contracts; the transmuted pointer is the symbol
/// the loader just resolved and its signature is fixed by the JNI spec
/// (§5.4.1: `jint JNI_OnLoad(JavaVM*, void*)`).
#[allow(clippy::not_unsafe_ptr_arg_deref)] // deliberate: null path guarded
pub fn load_foreign_jni_library(path: *const c_char, vm: *mut c_void) -> i32 {
    if path.is_null() {
        panic!("j2r JNI bridge: System.loadLibrary(null) — a null library name is a caller bug");
    }
    // SAFETY: three foreign calls under one audited block. `dlopen`
    // receives the caller-owned NUL-terminated path (the JNI-spec string
    // contract); `dlsym` names a fixed ASCII symbol; the transmute only
    // fixes the JNI-spec signature of the symbol the loader just
    // resolved. `dlerror` is read only on the failure path the loader
    // documents. The handle is never closed (see the doc above).
    unsafe {
        let handle = dlopen(path, RTLD_NOW | RTLD_LOCAL);
        if handle.is_null() {
            let reason = dlerror();
            let detail = if reason.is_null() {
                String::new()
            } else {
                std::ffi::CStr::from_ptr(reason)
                    .to_string_lossy()
                    .into_owned()
            };
            panic!(
                "j2r JNI bridge: dlopen failed ({detail}) — System.loadLibrary must not fake success"
            );
        }
        let sym = dlsym(handle, JNI_ON_LOAD_SYM.as_ptr() as *const c_char);
        if sym.is_null() {
            return 0; // loaded, no JNI_OnLoad: the spec's default-version case
        }
        type OnLoad = extern "C" fn(*mut c_void, *mut c_void) -> i32;
        let on_load: OnLoad = std::mem::transmute(sym);
        on_load(vm, std::ptr::null_mut())
    }
}

/// The number of `unsafe` occurrences in this file. Asserted by a test
/// below (source-scanned, so drift fails locally before gate-10 does).
/// T13 slice 13c: 10 → 11 (primitive 12), pin ≤ 12 unchanged (1 slot of
/// headroom left).
pub const ACTUAL_UNSAFE_OCCURRENCES: u32 = 11;

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::raw::c_void;

    #[test]
    fn copy_from_caller_roundtrip_and_total_guards() {
        let src: Vec<u8> = (0u8..=7).collect();
        let got = copy_from_caller(src.as_ptr(), src.len());
        assert_eq!(got, src);
        // total: null source / zero length never dereference
        assert!(copy_from_caller(std::ptr::null(), 4).is_empty());
        assert!(copy_from_caller(src.as_ptr(), 0).is_empty());
    }

    #[test]
    fn copy_to_caller_roundtrip_and_total_guards() {
        let mut buf = [0u8; 4];
        let src = [9u8, 8, 7, 6];
        copy_to_caller(buf.as_mut_ptr(), &src);
        assert_eq!(buf, src);
        // total: null destination / empty source never write
        copy_to_caller(std::ptr::null_mut(), &src);
        let mut empty_buf: [u8; 0] = [];
        copy_to_caller(empty_buf.as_mut_ptr(), &[]);
        assert_eq!(buf, src); // untouched by the no-ops
    }

    #[test]
    fn write_ptr_out_writes_one_pointer_word() {
        let mut slot: *mut c_void = std::ptr::null_mut();
        let table_addr = &slot as *const *mut c_void as usize;
        write_ptr_out(&mut slot, table_addr);
        assert_eq!(slot as usize, table_addr);
        // total: null slot is a no-op
        write_ptr_out(std::ptr::null_mut(), 0xdead_beef);
    }

    #[test]
    fn read_caller_str_reads_up_to_nul() {
        let c = std::ffi::CString::new("Ljava/lang/Object;").unwrap();
        assert_eq!(
            read_caller_str(c.as_ptr()),
            Some(b"Ljava/lang/Object;".to_vec())
        );
        // empty string: terminator only
        let empty = std::ffi::CString::new("").unwrap();
        assert_eq!(read_caller_str(empty.as_ptr()), Some(Vec::new()));
        // total: null -> None
        assert_eq!(read_caller_str(std::ptr::null()), None);
    }

    #[test]
    fn copy_back_elements_re_reads_mutated_storage() {
        // Storage we "handed out", mutated by the (simulated) caller.
        let mut handed_out = [1u8, 2, 3, 4];
        let elems = handed_out.as_mut_ptr() as *mut c_void;
        assert_eq!(
            &handed_out[..],
            &[1, 2, 3, 4],
            "pre-dispatch state handed out"
        );
        handed_out[0] = 42;
        handed_out[3] = 7;
        assert_eq!(
            &handed_out[..],
            &[42, 2, 3, 7],
            "caller mutated the handed-out storage"
        );
        let mut dst = [0u8; 4];
        copy_back_elements(elems, &mut dst);
        assert_eq!(dst, [42, 2, 3, 7]);
        // total: null / empty never read
        let mut out2 = [0u8; 4];
        copy_back_elements(std::ptr::null_mut(), &mut out2);
        assert_eq!(out2, [0, 0, 0, 0]);
    }

    #[test]
    fn direct_buffer_ok_is_address_identity() {
        let backing: [u8; 8] = [0; 8];
        let p = backing.as_ptr() as *mut c_void;
        assert_eq!(direct_buffer_ok(p), p as usize);
        assert_eq!(direct_buffer_ok(std::ptr::null_mut()), 0);
    }

    #[test]
    fn write_u8_out_writes_one_byte_and_guards_null() {
        let mut flag: u8 = 0;
        write_u8_out(&mut flag, 1); // JNI_TRUE
        assert_eq!(flag, 1);
        write_u8_out(&mut flag, 0);
        assert_eq!(flag, 0);
        // total: null slot is a no-op, never a dereference
        write_u8_out(std::ptr::null_mut(), 1);
    }

    #[test]
    fn read_u32_at_reads_the_cursor_fields_and_guards_null() {
        // The ABI's __va_list_tag head: gp_offset then fp_offset.
        let rec: [u8; 8] = 0x0020_0010u32
            .to_le_bytes()
            .iter()
            .chain(0x0000_00B0u32.to_le_bytes().iter())
            .copied()
            .collect::<Vec<u8>>()
            .try_into()
            .expect("8 bytes");
        assert_eq!(read_u32_at(rec.as_ptr()), Some(0x0020_0010));
        assert_eq!(read_u32_at(rec.as_ptr().wrapping_add(4)), Some(0x0000_00B0));
        // total: null -> None, never a dereference
        assert_eq!(read_u32_at(std::ptr::null()), None);
    }

    #[test]
    fn read_word_at_reads_one_gp_word_and_guards_null() {
        let slot: [u8; 8] = (-42_i64).to_le_bytes();
        assert_eq!(read_word_at(slot.as_ptr()), Some((-42_i64) as u64));
        let mut tag_word = [0u8; 8];
        tag_word.copy_from_slice(&0x0000_FFFF_0000_1234usize.to_le_bytes());
        assert_eq!(read_word_at(tag_word.as_ptr()), Some(0x0000_FFFF_0000_1234));
        // total: null -> None
        assert_eq!(read_word_at(std::ptr::null()), None);
    }

    #[test]
    fn read_fp_word_at_reads_the_value_half_of_an_sse_slot() {
        // A 16-byte XMM save slot: the value lives in the LOW 8 bytes;
        // the upper half is padding the walk never touches.
        let mut xmm = [0u8; 16];
        xmm[0..8].copy_from_slice(&2.5_f64.to_le_bytes());
        xmm[9] = 0xAA; // upper-half noise: must not leak into the answer
        assert_eq!(read_fp_word_at(xmm.as_ptr()), Some(2.5_f64.to_bits()));
        // an f32 arg lives in ITS OWN slot: f32 bits in the low 4 bytes
        let mut xmm32 = [0u8; 16];
        xmm32[0..4].copy_from_slice(&1.5_f32.to_le_bytes());
        let w = read_fp_word_at(xmm32.as_ptr()).expect("read");
        assert_eq!(
            f32::from_le_bytes(w.to_le_bytes()[0..4].try_into().expect("4")),
            1.5_f32
        );
        // total: null -> None
        assert_eq!(read_fp_word_at(std::ptr::null()), None);
    }

    // -- primitive 11: the foreign-native call thunk (13b) --------------
    //
    // The Rust `extern "C"` probes below validate the register-window
    // MATH. `build.rs` ADDITIONALLY compiles a REAL C probe (the
    // M5_JNI_HARNESS.md §1 shape — the first C in the repo) and links it
    // into this package's TEST binaries only; the tests that exercise it
    // live in `tests/c_probe.rs` (an integration-test target — a
    // `rustc-link-arg-tests` link needs one) so the transmute is also
    // proven against actual C-compiler output. 13c's harness adds the
    // full JNI-loop parity on top.

    extern "C" fn probe_add(_env: *mut c_void, _recv: *mut c_void, a: usize, b: usize) -> u64 {
        (a + b) as u64 // a jint native's INTEGER return (rax)
    }

    extern "C" fn probe_mul_d(_env: *mut c_void, _recv: *mut c_void, a: usize, b: f64) -> f64 {
        // a jdouble native's SSE return (xmm0); the Int arg rides the
        // GP window INDEPENDENTLY of the FP window (SysV classification).
        (a as f64) * b
    }

    #[test]
    fn foreign_native_call_integer_window_and_return() {
        let ret = call_foreign_native(
            probe_add as *const c_void,
            0x1000 as *mut c_void,
            0x2000 as *mut c_void,
            &[20, 22],
            &[],
        );
        assert_eq!(ret.i, 42, "the GP words land in rdx/rcx order");
        // window guard: 5 GP words is OUTSIDE the window — loud
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = call_foreign_native(
                probe_add as *const c_void,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &[1, 2, 3, 4, 5],
                &[],
            );
        }))
        .is_err();
        assert!(panicked, "beyond the GP window must be loud");
    }

    #[test]
    fn foreign_native_call_mixed_classification_and_sse_return() {
        // mix(I D)D — the Int rides gp[0] (rdx), the Double rides
        // fp[0] (xmm0); the jdouble return is read from .d.
        let ret = call_foreign_native(
            probe_mul_d as *const c_void,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &[21],
            &[2.0],
        );
        assert!((ret.d - 42.0).abs() < 1e-9, "the SSE return channel works");
        // window guard: 9 FP words — loud
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = call_foreign_native(
                probe_mul_d as *const c_void,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &[],
                &[0.0; 9],
            );
        }))
        .is_err();
        assert!(panicked, "beyond the FP window must be loud");
    }

    // -- primitive 12: the foreign-library loader (13c) -----------------

    #[test]
    #[should_panic(expected = "dlopen failed")]
    fn load_foreign_jni_library_missing_is_loud() {
        // A missing library aborts LOUDLY (the loader's own error text in
        // the message) — never a silent fake success. Panics unwind fine
        // inside a plain Rust test (no extern boundary crossed).
        let c = std::ffi::CString::new("/nonexistent/j2r/libnope.so").unwrap();
        let _ = load_foreign_jni_library(c.as_ptr(), std::ptr::null_mut());
    }

    #[test]
    #[should_panic(expected = "null library name")]
    fn load_foreign_jni_library_null_path_is_loud() {
        let _ = load_foreign_jni_library(std::ptr::null(), std::ptr::null_mut());
    }

    #[test]
    fn unsafe_occurrence_count_is_pinned_and_matches_the_gate() {
        // The gate greps `unsafe\s*(fn|impl|trait|\{)`; assert the same
        // count here so drift fails locally before CI does.
        let src = include_str!("ffi.rs");
        let mut count = 0u32;
        let mut rest = src;
        while let Some(pos) = find_unsafe(rest) {
            count += 1;
            rest = &rest[pos + "unsafe".len()..];
        }
        assert_eq!(
            count, ACTUAL_UNSAFE_OCCURRENCES,
            "ffi.rs unsafe occurrences drifted — update ACTUAL_UNSAFE_OCCURRENCES and the gate pin together"
        );
        assert!(count <= crate::AUDITED_UNSAFE_OCCURRENCES);
    }

    /// Finds the next `unsafe` followed by fn/impl/trait/{ — the exact
    /// gate-10 pattern, mirrored (a mini scanner keeps the test readable).
    fn find_unsafe(haystack: &str) -> Option<usize> {
        let mut from = 0;
        while let Some(rel) = haystack[from..].find("unsafe") {
            let at = from + rel;
            let after = &haystack[at + "unsafe".len()..];
            let trimmed = after.trim_start();
            if trimmed.starts_with("fn")
                || trimmed.starts_with("impl")
                || trimmed.starts_with("trait")
                || trimmed.starts_with('{')
            {
                return Some(at);
            }
            from = at + "unsafe".len();
        }
        None
    }
}
