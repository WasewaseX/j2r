//! The audited primitive surface (M5_JNI_FFI.md §4, v1 — complete).
//!
//! Six total functions; every `unsafe` occurrence in the workspace outside
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

/// The number of `unsafe` occurrences in this file. Asserted by a test
/// below (source-scanned, so drift fails locally before gate-10 does).
pub const ACTUAL_UNSAFE_OCCURRENCES: u32 = 5;

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
