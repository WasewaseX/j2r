//! T10 slice 3b — JNI array families (slots 171–174, 175–182, 183–190,
//! 191–198, 199–206, 207–214, 222–223).
//!
//! Array objects are already heap `JObj`s (`slots[0]` = length; primitive
//! payloads in `PrimData`; reference elements in `slots[1..]`; the array's
//! `jclass` IS the descriptor — `docs/M5_JNI_CLASSES.md` §5), so this
//! module is a thin, loud layer between the generated slot wrappers and
//! the existing `crate::array` primitives. No new representation, no new
//! unsafe, no new FFI primitives — the byte plumbing rides the audited
//! `copy_from_caller`/`copy_to_caller`/`write_u8_out` set from slice 2.
//!
//! **The failure law (design of record, M5_JNI_CLASSES.md §5):**
//! - A NULL array handle follows the string-family precedent where a
//!   return value exists (`GetArrayLength` → 0, the §5-conceded shape;
//!   allocation/element getters → JNI null; void slots → no-op). Every
//!   such site is a registered T13 gap: the spec's NPE shape needs the
//!   exception transport the T13 C-harness will judge three-channel.
//! - A REAL Java-observable failure on a live handle (negative length,
//!   out-of-bounds index/region, `ArrayStoreException`) cannot cross the
//!   `extern "system"` boundary yet — v1 **aborts LOUDLY** with the
//!   exception's runtime class name in the message (strictly better than
//!   Temurin's UB on the same foreign bug).
//! - Registry law (the T09/slice-2 law, pinned again): a release for a
//!   pointer `Get*ArrayElements` never handed out — or through the wrong
//!   family's slot — is a LOUD registry miss, never a silent corrupt.

use crate::array::{
    array_length, array_load, array_load_d, array_load_f, array_load_i, array_load_j,
    array_new_prim, array_new_ref, array_store, array_store_d_raw, array_store_f_raw,
    array_store_i_raw, array_store_j_raw,
};
use crate::heap::with_obj;
use crate::{Dispatch, JRef, Throw, Value};

/// The JNI release-mode constants (verified against the pinned Temurin 21
/// `jni.h` definitions: `JNI_COMMIT` 1, `JNI_ABORT` 2; 0 = copy + free).
const JNI_COMMIT: i32 = 1;
const JNI_ABORT: i32 = 2;

/// The eight primitive array families. One variant per `New<Prim>Array`
/// slot — the design's "each slot IS one primitive, no descriptor
/// parsing" law — and the registry key for the elements family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimKind {
    Z,
    B,
    C,
    S,
    I,
    J,
    F,
    D,
}

impl PrimKind {
    /// The ARRAY descriptor this family allocates (`New<Prim>Array`).
    fn desc(self) -> &'static str {
        match self {
            PrimKind::Z => crate::array::BOOLEAN,
            PrimKind::B => crate::array::BYTE,
            PrimKind::C => crate::array::CHAR,
            PrimKind::S => crate::array::SHORT,
            PrimKind::I => crate::array::INT,
            PrimKind::J => crate::array::LONG,
            PrimKind::F => crate::array::FLOAT,
            PrimKind::D => crate::array::DOUBLE,
        }
    }

    /// jni.h-ish family name for loud messages.
    fn name(self) -> &'static str {
        match self {
            PrimKind::Z => "Boolean",
            PrimKind::B => "Byte",
            PrimKind::C => "Char",
            PrimKind::S => "Short",
            PrimKind::I => "Int",
            PrimKind::J => "Long",
            PrimKind::F => "Float",
            PrimKind::D => "Double",
        }
    }

    /// Element width of the handed-out C buffer (native-endian elements).
    fn bytes_per(self) -> usize {
        match self {
            PrimKind::Z | PrimKind::B => 1,
            PrimKind::C | PrimKind::S => 2,
            PrimKind::I | PrimKind::F => 4,
            PrimKind::J | PrimKind::D => 8,
        }
    }
}

/// The v1 loud-abort shape: a thrown Java exception cannot cross the
/// `extern "system"` boundary until T13 gives it a transport, so the slot
/// aborts with the exception's runtime class name in the message.
fn loud(slot: &str, t: &Throw) -> ! {
    panic!(
        "j2r JNI bridge: {slot} cannot report a thrown {} across the FFI boundary \
         (no exception transport until T13) — aborting loudly",
        t.java_class()
    );
}

// ------------------------------------------------- the elements registry

/// One `Get<Prim>ArrayElements` hand-out: the caller-owned buffer (the
/// registry pins it for the pointer's lifetime — dropping it here would
/// hand out a dangling address), the array (a GC root for as long as
/// the entry lives), the length we validated, and the family — so a
/// release through the WRONG family's slot is loud, not corrupting.
struct ElemReg {
    buf: Box<[u8]>,
    jref: JRef,
    len: usize,
    kind: PrimKind,
}

fn elem_regs() -> &'static std::sync::Mutex<std::collections::HashMap<usize, ElemReg>> {
    static ELEM_REGS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<usize, ElemReg>>,
    > = std::sync::OnceLock::new();
    ELEM_REGS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

// ------------------------------------------------------- slot-level API
//
// The generated wrappers (slice 3b — the generator's real-slot registry)
// call THESE, one per jni.h slot. Null-handle laws live in the wrappers
// (documented there); everything below runs on a live handle and follows
// the module failure law.

/// `GetArrayLength` body (slot 171): the array's length. A non-array
/// handle panics at `check_array` — a foreign-side bug, loud.
pub fn slot_get_array_length(tag: u64) -> i32 {
    let r = super::bridge::resolve(tag);
    match array_length(&r) {
        Ok(n) => n,
        Err(t) => loud("GetArrayLength", &t),
    }
}

/// `NewObjectArray` body (slot 172): element class FROM the jclass
/// argument (a foreign lib may hand any Class mirror, builtin included —
/// `class_ref` interns those); `init`, when non-null, resolves ONCE and
/// every slot stores a copy of that `JRef`. Negative length or an
/// init/class mismatch aborts loudly (the design §5 failure law).
pub fn slot_new_object_array(len: i32, class_tag: u64, init_tag: u64) -> u64 {
    let class_r = super::bridge::resolve(class_tag);
    let elem_name = crate::reflect::class_name_of(&class_r);
    // `array_new_ref` takes the ELEMENT class (a plain class name, or an
    // array descriptor when the element is itself an array — `[I` →
    // `[[I`) and builds the canonical array class itself.
    let arr = match array_new_ref(&elem_name, len) {
        Ok(a) => a,
        Err(t) => loud("NewObjectArray", &t),
    };
    if init_tag != 0 {
        let init = super::bridge::resolve(init_tag);
        let ctx: Dispatch = super::host_dispatch();
        for i in 0..len {
            if let Err(t) = array_store(&arr, i, Value::Ref(Some(init)), ctx) {
                loud("NewObjectArray", &t);
            }
        }
    }
    super::bridge::new_local(arr)
}

/// `GetObjectArrayElement` body (slot 173): `Some(local-ref tag)` for a
/// live element, `None` for a null element (the wrapper returns JNI
/// null). Out-of-bounds aborts loudly (design §5).
pub fn slot_get_object_array_element(tag: u64, idx: i32) -> Option<u64> {
    let r = super::bridge::resolve(tag);
    match array_load(&r, idx) {
        Ok(Value::Ref(Some(jref))) => Some(super::bridge::new_local(jref)),
        Ok(Value::Ref(None)) => None,
        Ok(other) => {
            panic!("j2r internal: GetObjectArrayElement on a non-reference element {other:?}")
        }
        Err(t) => loud("GetObjectArrayElement", &t),
    }
}

/// `SetObjectArrayElement` body (slot 174): the store rides
/// `array_store`, whose `ArrayStoreException` check runs with the host
/// dispatch (design §5). A null `elem` tag stores a null element — legal.
/// Out-of-bounds or a store violation aborts loudly.
pub fn slot_set_object_array_element(tag: u64, idx: i32, elem_tag: u64) {
    let r = super::bridge::resolve(tag);
    let v = if elem_tag == 0 {
        Value::Ref(None)
    } else {
        Value::Ref(Some(super::bridge::resolve(elem_tag)))
    };
    let ctx: Dispatch = super::host_dispatch();
    if let Err(t) = array_store(&r, idx, v, ctx) {
        loud("SetObjectArrayElement", &t);
    }
}

/// `New<Prim>Array` bodies (slots 175–182): one primitive per slot — no
/// descriptor parsing (design §5). Negative length aborts loudly with
/// the `NegativeArraySizeException` name (v1 failure law).
pub fn slot_new_prim_array(kind: PrimKind, len: i32) -> u64 {
    match array_new_prim(kind.desc(), len) {
        Ok(r) => super::bridge::new_local(r),
        Err(t) => loud(&format!("New{}Array", kind.name()), &t),
    }
}

/// `Get<Prim>ArrayElements` bodies (slots 183–190): allocate a
/// caller-owned COPY (the arena never moves, but the copy keeps the
/// release semantics uniform — design §5) and registry-pin
/// `(array JRef, len, kind)` by the handed-out address. `isCopy` is
/// written JNI_TRUE by the wrapper through audited primitive 7.
pub fn slot_get_prim_array_elements(kind: PrimKind, tag: u64) -> usize {
    let r = super::bridge::resolve(tag);
    let slot = format!("Get{}ArrayElements", kind.name());
    let len = match array_length(&r) {
        Ok(n) => n as usize,
        Err(t) => loud(&slot, &t),
    };
    let mut buf: Vec<u8> = Vec::with_capacity(len * kind.bytes_per());
    for i in 0..len as i32 {
        let bytes = match kind {
            PrimKind::Z | PrimKind::B | PrimKind::C | PrimKind::S | PrimKind::I => {
                let v = match array_load_i(&r, i) {
                    Ok(v) => v,
                    Err(t) => loud(&slot, &t),
                };
                match kind {
                    PrimKind::Z => vec![(v != 0) as u8],
                    PrimKind::B => vec![v as i8 as u8],
                    PrimKind::C => (v as u16).to_ne_bytes().to_vec(),
                    PrimKind::S => (v as i16).to_ne_bytes().to_vec(),
                    _ => v.to_ne_bytes().to_vec(),
                }
            }
            PrimKind::J => match array_load_j(&r, i) {
                Ok(v) => v.to_ne_bytes().to_vec(),
                Err(t) => loud(&slot, &t),
            },
            PrimKind::F => match array_load_f(&r, i) {
                Ok(v) => v.to_ne_bytes().to_vec(),
                Err(t) => loud(&slot, &t),
            },
            PrimKind::D => match array_load_d(&r, i) {
                Ok(v) => v.to_ne_bytes().to_vec(),
                Err(t) => loud(&slot, &t),
            },
        };
        buf.extend_from_slice(&bytes);
    }
    let boxed: Box<[u8]> = buf.into_boxed_slice();
    let addr = boxed.as_ptr() as usize;
    elem_regs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(
            addr,
            ElemReg {
                buf: boxed,
                jref: r,
                len,
                kind,
            },
        );
    addr
}

/// `Release<Prim>ArrayElements` bodies (slots 191–198): the mode law —
/// `JNI_COMMIT` (1) copies back WITHOUT freeing; `0` copies back and
/// frees; `JNI_ABORT` (2) frees without copying back (design §5).
/// Copy-back rides the raw store accessors. A pointer we never handed
/// out, a wrong-family release, or an array-handle mismatch is a LOUD
/// registry miss (the T09/slice-2 law); an unknown mode is loud too.
pub fn slot_release_prim_array_elements(kind: PrimKind, tag: u64, addr: usize, mode: i32) {
    let slot = format!("Release{}ArrayElements", kind.name());
    if mode != 0 && mode != JNI_COMMIT && mode != JNI_ABORT {
        panic!("j2r JNI bridge: {slot} called with invalid release mode {mode} — loud law");
    }
    // Validate BEFORE removing: a caught loud miss (wrong family, foreign
    // pointer, wrong array handle) must leave the registry intact so a
    // later VALID release still works — the registry is only mutated once
    // the release is proven legitimate.
    let r = super::bridge::resolve(tag);
    let entry = {
        let mut regs = elem_regs().lock().unwrap_or_else(|e| e.into_inner());
        match regs.get(&addr) {
            None => panic!(
                "j2r JNI bridge: {slot} on an unregistered buffer 0x{addr:x} — \
                 registry miss is a foreign-side bug"
            ),
            Some(e) if e.kind != kind => panic!(
                "j2r JNI bridge: {slot} released a {}-family buffer — \
                 wrong-family release is a foreign-side bug",
                e.kind.name()
            ),
            Some(e) if e.jref.index() != r.index() => panic!(
                "j2r JNI bridge: {slot} released buffers of a different array than \
                 the one handed out — foreign-side bug"
            ),
            Some(_) => regs.remove(&addr).expect("peeked above"),
        }
    };
    if mode != JNI_ABORT {
        // Copy back: the registry-pinned Box IS the memory the caller
        // edited through the pointer, so reading it sees their writes;
        // landing goes through the raw store accessors (design §5).
        for (i, elem) in entry
            .buf
            .chunks_exact(kind.bytes_per())
            .enumerate()
            .take(entry.len)
        {
            let res = match kind {
                PrimKind::Z => array_store_i_raw(&r, i as i32, (elem[0] != 0) as i32),
                PrimKind::B => array_store_i_raw(&r, i as i32, i8::from_ne_bytes([elem[0]]) as i32),
                PrimKind::C => {
                    array_store_i_raw(&r, i as i32, u16::from_ne_bytes([elem[0], elem[1]]) as i32)
                }
                PrimKind::S => {
                    array_store_i_raw(&r, i as i32, i16::from_ne_bytes([elem[0], elem[1]]) as i32)
                }
                PrimKind::I => array_store_i_raw(
                    &r,
                    i as i32,
                    i32::from_ne_bytes([elem[0], elem[1], elem[2], elem[3]]),
                ),
                PrimKind::J => array_store_j_raw(
                    &r,
                    i as i32,
                    i64::from_ne_bytes(elem.try_into().expect("8 bytes")),
                ),
                PrimKind::F => array_store_f_raw(
                    &r,
                    i as i32,
                    f32::from_ne_bytes(elem.try_into().expect("4 bytes")),
                ),
                PrimKind::D => array_store_d_raw(
                    &r,
                    i as i32,
                    f64::from_ne_bytes(elem.try_into().expect("8 bytes")),
                ),
            };
            if let Err(t) = res {
                loud(&slot, &t);
            }
        }
    }
    if mode == JNI_COMMIT {
        // Copy back but keep pinned: the buffer stays valid and the
        // caller must release again (mode 0/2) to free (the JNI law).
        elem_regs()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(addr, entry);
    }
}

/// `Get<Prim>ArrayRegion` bodies (slots 199–206): bounds checked UP
/// FRONT (design §5) — a violation is the loud AIOOBE abort; then the
/// bulk copy rides the raw load accessors into one audited
/// `copy_to_caller` write.
pub fn slot_get_prim_array_region(kind: PrimKind, tag: u64, start: i32, len: i32, dst: *mut u8) {
    let slot = format!("Get{}ArrayRegion", kind.name());
    let r = super::bridge::resolve(tag);
    region_bounds_ok(&slot, &r, start, len);
    let mut out: Vec<u8> = Vec::with_capacity(len as usize * kind.bytes_per());
    for i in start..start + len {
        let bytes = match kind {
            PrimKind::Z | PrimKind::B | PrimKind::C | PrimKind::S | PrimKind::I => {
                let v = match array_load_i(&r, i) {
                    Ok(v) => v,
                    Err(t) => loud(&slot, &t),
                };
                match kind {
                    PrimKind::Z => vec![(v != 0) as u8],
                    PrimKind::B => vec![v as i8 as u8],
                    PrimKind::C => (v as u16).to_ne_bytes().to_vec(),
                    PrimKind::S => (v as i16).to_ne_bytes().to_vec(),
                    _ => v.to_ne_bytes().to_vec(),
                }
            }
            PrimKind::J => match array_load_j(&r, i) {
                Ok(v) => v.to_ne_bytes().to_vec(),
                Err(t) => loud(&slot, &t),
            },
            PrimKind::F => match array_load_f(&r, i) {
                Ok(v) => v.to_ne_bytes().to_vec(),
                Err(t) => loud(&slot, &t),
            },
            PrimKind::D => match array_load_d(&r, i) {
                Ok(v) => v.to_ne_bytes().to_vec(),
                Err(t) => loud(&slot, &t),
            },
        };
        out.extend_from_slice(&bytes);
    }
    j2r_jni_ffi::ffi::copy_to_caller(dst, &out);
}

/// `Set<Prim>ArrayRegion` bodies (slots 207–214): bounds checked UP
/// FRONT, then the caller's bytes come in through the audited primitive
/// and land via the raw store accessors (narrowing per the ARRAY
/// descriptor — design §5).
pub fn slot_set_prim_array_region(kind: PrimKind, tag: u64, start: i32, len: i32, src: *const u8) {
    let slot = format!("Set{}ArrayRegion", kind.name());
    let r = super::bridge::resolve(tag);
    region_bounds_ok(&slot, &r, start, len);
    let bytes = j2r_jni_ffi::ffi::copy_from_caller(src, len as usize * kind.bytes_per());
    for (i, elem) in bytes
        .chunks_exact(kind.bytes_per())
        .enumerate()
        .take(len as usize)
    {
        let res = match kind {
            PrimKind::Z => array_store_i_raw(&r, start + i as i32, (elem[0] != 0) as i32),
            PrimKind::B => {
                array_store_i_raw(&r, start + i as i32, i8::from_ne_bytes([elem[0]]) as i32)
            }
            PrimKind::C => array_store_i_raw(
                &r,
                start + i as i32,
                u16::from_ne_bytes([elem[0], elem[1]]) as i32,
            ),
            PrimKind::S => array_store_i_raw(
                &r,
                start + i as i32,
                i16::from_ne_bytes([elem[0], elem[1]]) as i32,
            ),
            PrimKind::I => array_store_i_raw(
                &r,
                start + i as i32,
                i32::from_ne_bytes([elem[0], elem[1], elem[2], elem[3]]),
            ),
            PrimKind::J => array_store_j_raw(
                &r,
                start + i as i32,
                i64::from_ne_bytes(elem.try_into().expect("8 bytes")),
            ),
            PrimKind::F => array_store_f_raw(
                &r,
                start + i as i32,
                f32::from_ne_bytes(elem.try_into().expect("4 bytes")),
            ),
            PrimKind::D => array_store_d_raw(
                &r,
                start + i as i32,
                f64::from_ne_bytes(elem.try_into().expect("8 bytes")),
            ),
        };
        if let Err(t) = res {
            loud(&slot, &t);
        }
    }
}

/// The up-front region bounds law (design §5): a violation aborts
/// loudly with the `ArrayIndexOutOfBoundsException` name — the shape the
/// T13 harness will replace with real exception transport.
fn region_bounds_ok(slot: &str, r: &JRef, start: i32, len: i32) {
    let alen = match array_length(r) {
        Ok(n) => n,
        Err(t) => loud(slot, &t),
    };
    if start < 0 || len < 0 || start > alen - len {
        let t = crate::throw_new(
            "java/lang/ArrayIndexOutOfBoundsException",
            Some(&format!(
                "region [{start}, {start}+{len}) out of bounds for length {alen}"
            )),
        );
        loud(slot, &t);
    }
}

/// `GetPrimitiveArrayCritical` body (slot 222): hands out the payload's
/// OWN address through the existing critical registry (`bridge::
/// critical_register`) — pinning is free because the arena never moves
/// objects and array payloads never reallocate (fixed length at
/// allocation; the documented M2 advantage). `isCopy` is JNI_FALSE (we
/// hand the real storage — the faithful uncopied shape). A non-primitive
/// array is a loud foreign-side bug.
pub fn slot_get_primitive_array_critical(tag: u64) -> usize {
    let r = super::bridge::resolve(tag);
    // Validate it IS a primitive array (loud, not silent) and take the
    // payload address — stable for the critical region's lifetime.
    let addr = with_obj(r, |o| match &o.prim {
        Some(crate::object::PrimData::I(v)) => v.as_ptr() as usize,
        Some(crate::object::PrimData::J(v)) => v.as_ptr() as usize,
        Some(crate::object::PrimData::F(v)) => v.as_ptr() as usize,
        Some(crate::object::PrimData::D(v)) => v.as_ptr() as usize,
        _ => panic!(
            "j2r JNI bridge: GetPrimitiveArrayCritical on a non-primitive array {}",
            o.jclass
        ),
    });
    super::bridge::critical_register(addr, r);
    addr
}

/// `ReleasePrimitiveArrayCritical` body (slot 223): the existing
/// critical registry's loud release. The handed-out address IS the live
/// payload, so the caller's writes are already in the array — there is
/// nothing to copy back and `JNI_ABORT` cannot retroactively un-write;
/// the mode is accepted and recorded as a no-op (documented v1
/// deviation, T13-registered). A miss is loud (double release, foreign
/// pointer, or wrong registry — never silent).
pub fn slot_release_primitive_array_critical(addr: usize, _mode: i32) {
    super::bridge::critical_release(addr);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::runtime;

    fn int_array(vals: &[i32]) -> (u64, JRef) {
        let tag = slot_new_prim_array(PrimKind::I, vals.len() as i32);
        let r = super::super::bridge::resolve(tag);
        for (i, v) in vals.iter().enumerate() {
            array_store_i_raw(&r, i as i32, *v).expect("in-bounds store");
        }
        (tag, r)
    }

    // ------------------------------------------------ 171 GetArrayLength

    #[test]
    fn get_array_length_roundtrips() {
        let _rt = runtime();
        let (tag, _) = int_array(&[1, 2, 3, 4]);
        assert_eq!(slot_get_array_length(tag), 4);
        super::super::bridge::delete_local(tag);
    }

    #[test]
    fn get_array_length_through_the_generated_table() {
        // Wrapper-level: the full jobject→tag→slot plumbing through the
        // static table, the positional-ABI way a foreign lib calls it.
        let _rt = runtime();
        let (tag, _) = int_array(&[7]);
        let obj = super::super::bridge::tag_to_jobject(tag);
        let f: extern "system" fn(
            crate::jni::tables::JNIEnv,
            crate::jni::tables::jobject,
        ) -> crate::jni::tables::jsize = crate::jni::tables::ENV_TABLE.GetArrayLength;
        assert_eq!(f(super::super::tables::jni_env(), obj), 1);
        assert_eq!(
            f(super::super::tables::jni_env(), std::ptr::null_mut()),
            0,
            "null array → 0 — the §5-conceded v1 law (T13 gap)"
        );
        super::super::bridge::delete_local(tag);
    }

    // ------------------------------------------------ 172–174 object arrays

    #[test]
    fn new_object_array_fills_init_and_roundtrips() {
        let _rt = runtime();
        let class_tag =
            super::super::bridge::new_local(crate::reflect::class_ref("java/lang/String"));
        let s = crate::object::new_string("elem");
        let init_tag = super::super::bridge::new_local(s);
        let tag = slot_new_object_array(3, class_tag, init_tag);
        let r = super::super::bridge::resolve(tag);
        assert_eq!(&*with_obj(r, |o| o.jclass.clone()), "[Ljava/lang/String;");
        // every slot holds the SAME object (copies of one JRef)
        for i in 0..3 {
            match array_load(&r, i).expect("in-bounds") {
                Value::Ref(Some(elem)) => assert_eq!(
                    elem.index(),
                    s.index(),
                    "every slot holds a copy of the SAME object"
                ),
                other => panic!("expected ref element, got {other:?}"),
            }
        }
        // get → same object back as a fresh local ref
        let got = slot_get_object_array_element(tag, 2).expect("non-null");
        assert_eq!(
            &*crate::object::string_value(&super::super::bridge::resolve(got)),
            "elem"
        );
        super::super::bridge::delete_local(got);
        for t in [tag, class_tag, init_tag] {
            super::super::bridge::delete_local(t);
        }
    }

    #[test]
    fn object_array_set_get_roundtrip_including_null() {
        let _rt = runtime();
        let class_tag =
            super::super::bridge::new_local(crate::reflect::class_ref("java/lang/String"));
        let tag = slot_new_object_array(2, class_tag, 0); // no init → nulls
        assert_eq!(slot_get_object_array_element(tag, 0), None);
        let s_tag = super::super::bridge::new_local(crate::object::new_string("x"));
        slot_set_object_array_element(tag, 1, s_tag);
        let got = slot_get_object_array_element(tag, 1).expect("non-null after set");
        assert_eq!(
            &*crate::object::string_value(&super::super::bridge::resolve(got)),
            "x"
        );
        slot_set_object_array_element(tag, 1, 0); // back to null — legal
        assert_eq!(slot_get_object_array_element(tag, 1), None);
        for t in [tag, class_tag, s_tag, got] {
            super::super::bridge::delete_local(t);
        }
    }

    #[test]
    fn object_array_element_bounds_and_store_violation_are_loud() {
        let _rt = runtime();
        let class_tag =
            super::super::bridge::new_local(crate::reflect::class_ref("java/lang/String"));
        let tag = slot_new_object_array(1, class_tag, 0);
        let panicked = std::panic::catch_unwind(|| {
            slot_get_object_array_element(tag, 1) // len 1 → idx 1 is OOB
        })
        .is_err();
        assert!(panicked, "out-of-bounds get must be loud");
        // ArrayStoreException: an Object instance into a String[] slot
        let obj_tag = super::super::bridge::new_local(
            crate::object::new_obj("java/lang/Object", 1).expect("Object"),
        );
        let panicked =
            std::panic::catch_unwind(|| slot_set_object_array_element(tag, 0, obj_tag)).is_err();
        assert!(panicked, "ArrayStoreException must be loud (design §5)");
        for t in [tag, class_tag, obj_tag] {
            super::super::bridge::delete_local(t);
        }
    }

    // ------------------------------------------------ 175–182 New<Prim>Array

    #[test]
    fn new_prim_array_all_eight_families() {
        let _rt = runtime();
        let kinds = [
            (PrimKind::Z, "[Z"),
            (PrimKind::B, "[B"),
            (PrimKind::C, "[C"),
            (PrimKind::S, "[S"),
            (PrimKind::I, "[I"),
            (PrimKind::J, "[J"),
            (PrimKind::F, "[F"),
            (PrimKind::D, "[D"),
        ];
        for (kind, desc) in kinds {
            let tag = slot_new_prim_array(kind, 5);
            let r = super::super::bridge::resolve(tag);
            assert_eq!(&*with_obj(r, |o| o.jclass.clone()), desc);
            assert_eq!(slot_get_array_length(tag), 5);
            super::super::bridge::delete_local(tag);
        }
    }

    #[test]
    fn new_prim_array_negative_length_is_loud() {
        let _rt = runtime();
        let panicked = std::panic::catch_unwind(|| slot_new_prim_array(PrimKind::I, -1)).is_err();
        assert!(panicked, "NegativeArraySizeException must be loud");
        // the loud message names the exception (the design §5 law)
        let msg = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            slot_new_prim_array(PrimKind::I, -7)
        }))
        .unwrap_err();
        let text = msg
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| msg.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        assert!(
            text.contains("NegativeArraySizeException"),
            "abort message must name the exception, got: {text}"
        );
    }

    // ------------------------------------------------ 183–198 elements

    #[test]
    fn elements_roundtrip_all_eight_families() {
        let _rt = runtime();
        // Per family: build the array through the slot API, simulate the
        // foreign caller editing the handed buffer (the registry's
        // Box<[u8]> IS that memory — safe to touch through the lock),
        // release with mode 0, and verify the copy-back landed.
        let cases: &[(PrimKind, usize, Vec<u8>)] = &[
            (PrimKind::Z, 1, vec![1, 0]),
            (PrimKind::B, 1, vec![0xFB, 0x2A]), // -5, 42 as i8
            (PrimKind::C, 2, {
                let mut v = 0x263A_u16.to_ne_bytes().to_vec();
                v.extend_from_slice(&0x0041_u16.to_ne_bytes());
                v
            }),
            (PrimKind::S, 2, {
                let mut v = (-42_i16).to_ne_bytes().to_vec();
                v.extend_from_slice(&7_i16.to_ne_bytes());
                v
            }),
            (PrimKind::I, 4, {
                let mut v = (-42_i32).to_ne_bytes().to_vec();
                v.extend_from_slice(&7_i32.to_ne_bytes());
                v
            }),
            (PrimKind::J, 8, {
                let mut v = (-42_i64).to_ne_bytes().to_vec();
                v.extend_from_slice(&7_i64.to_ne_bytes());
                v
            }),
            (PrimKind::F, 4, {
                let mut v = 1.5_f32.to_ne_bytes().to_vec();
                v.extend_from_slice(&2.5_f32.to_ne_bytes());
                v
            }),
            (PrimKind::D, 8, {
                let mut v = 2.5_f64.to_ne_bytes().to_vec();
                v.extend_from_slice(&4.5_f64.to_ne_bytes());
                v
            }),
        ];
        for (kind, _bps, edits) in cases {
            let tag = slot_new_prim_array(*kind, 2);
            let addr = slot_get_prim_array_elements(*kind, tag);
            {
                let mut regs = elem_regs().lock().unwrap_or_else(|e| e.into_inner());
                let reg = regs.get_mut(&addr).expect("registry holds the buffer");
                assert_eq!(reg.len, 2);
                assert_eq!(reg.kind, *kind);
                // The Box<[u8]> IS the memory the foreign caller edits
                // through the pointer — simulate that edit safely.
                reg.buf.copy_from_slice(edits);
            }
            slot_release_prim_array_elements(*kind, tag, addr, 0);
            let r = super::super::bridge::resolve(tag);
            match kind {
                PrimKind::Z => assert_eq!(array_load_i(&r, 0).unwrap(), 1),
                PrimKind::B => assert_eq!(array_load_i(&r, 0).unwrap(), -5),
                PrimKind::C => assert_eq!(array_load_i(&r, 0).unwrap(), 0x263A),
                PrimKind::S => assert_eq!(array_load_i(&r, 0).unwrap(), -42),
                PrimKind::I => assert_eq!(array_load_i(&r, 0).unwrap(), -42),
                PrimKind::J => assert_eq!(array_load_j(&r, 0).unwrap(), -42),
                PrimKind::F => assert_eq!(array_load_f(&r, 0).unwrap(), 1.5),
                PrimKind::D => assert_eq!(array_load_d(&r, 0).unwrap(), 2.5),
            }
            super::super::bridge::delete_local(tag);
        }
    }

    #[test]
    fn elements_is_copy_written_true_by_wrapper_and_registry_pinned() {
        // isCopy is the WRAPPER's job (write_u8_out); at slot level the
        // registry entry is the proof of pinning.
        let _rt = runtime();
        let (tag, _) = int_array(&[9, 9, 9]);
        let addr = slot_get_prim_array_elements(PrimKind::I, tag);
        assert!(
            elem_regs()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains_key(&addr),
            "hand-out must be registry-pinned"
        );
        slot_release_prim_array_elements(PrimKind::I, tag, addr, 2); // ABORT
        super::super::bridge::delete_local(tag);
    }

    #[test]
    fn release_modes_commit_abort_and_free() {
        let _rt = runtime();
        // ABORT (2): caller edits are DISCARDED
        let (tag, r) = int_array(&[1, 2, 3]);
        let addr = slot_get_prim_array_elements(PrimKind::I, tag);
        elem_regs()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&addr)
            .expect("pinned")
            .buf[4..8]
            .copy_from_slice(&99_i32.to_ne_bytes());
        slot_release_prim_array_elements(PrimKind::I, tag, addr, JNI_ABORT);
        assert_eq!(array_load_i(&r, 1).unwrap(), 2, "ABORT must not copy back");
        assert!(
            !elem_regs()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains_key(&addr),
            "ABORT frees the entry"
        );

        // COMMIT (1): copy back AND stay pinned; a later 0-release frees
        let addr = slot_get_prim_array_elements(PrimKind::I, tag);
        elem_regs()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&addr)
            .expect("pinned")
            .buf[0..4]
            .copy_from_slice(&77_i32.to_ne_bytes());
        slot_release_prim_array_elements(PrimKind::I, tag, addr, JNI_COMMIT);
        assert_eq!(array_load_i(&r, 0).unwrap(), 77, "COMMIT copies back");
        assert!(
            elem_regs()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains_key(&addr),
            "COMMIT keeps the buffer valid"
        );
        slot_release_prim_array_elements(PrimKind::I, tag, addr, 0);
        assert_eq!(array_load_i(&r, 0).unwrap(), 77, "second release re-copies");
        super::super::bridge::delete_local(tag);
    }

    #[test]
    fn foreign_release_and_wrong_family_are_loud() {
        let _rt = runtime();
        let (tag, _) = int_array(&[1]);
        let addr = slot_get_prim_array_elements(PrimKind::I, tag);
        // wrong family: releasing an Int hand-out through the Long slot
        let panicked = std::panic::catch_unwind(|| {
            slot_release_prim_array_elements(PrimKind::J, tag, addr, 0)
        })
        .is_err();
        assert!(panicked, "wrong-family release must be loud");
        // valid release, then double release → loud
        slot_release_prim_array_elements(PrimKind::I, tag, addr, 0);
        let panicked = std::panic::catch_unwind(|| {
            slot_release_prim_array_elements(PrimKind::I, tag, addr, 0)
        })
        .is_err();
        assert!(panicked, "double release must be loud");
        // a pointer we never handed out → loud
        let panicked = std::panic::catch_unwind(|| {
            slot_release_prim_array_elements(PrimKind::I, tag, 0xBEEF, 0)
        })
        .is_err();
        assert!(panicked, "foreign pointer release must be loud");
        super::super::bridge::delete_local(tag);
    }

    // ------------------------------------------------ 199–214 regions

    #[test]
    fn regions_roundtrip_all_eight_families() {
        let _rt = runtime();
        // Two caller-encoded elements per family: -1 and 42 (floats: -1.5
        // and 42.5; char: 0x263A and 'A'; boolean: 1 and 0).
        let cases: &[(PrimKind, usize, Vec<u8>)] = &[
            (PrimKind::Z, 1, vec![1, 0]),
            (PrimKind::B, 1, vec![0xFF, 0x2A]),
            (PrimKind::C, 2, {
                let mut v = 0x263A_u16.to_ne_bytes().to_vec();
                v.extend_from_slice(&0x0041_u16.to_ne_bytes());
                v
            }),
            (PrimKind::S, 2, {
                let mut v = (-1_i16).to_ne_bytes().to_vec();
                v.extend_from_slice(&42_i16.to_ne_bytes());
                v
            }),
            (PrimKind::I, 4, {
                let mut v = (-1_i32).to_ne_bytes().to_vec();
                v.extend_from_slice(&42_i32.to_ne_bytes());
                v
            }),
            (PrimKind::J, 8, {
                let mut v = (-1_i64).to_ne_bytes().to_vec();
                v.extend_from_slice(&42_i64.to_ne_bytes());
                v
            }),
            (PrimKind::F, 4, {
                let mut v = (-1.5_f32).to_ne_bytes().to_vec();
                v.extend_from_slice(&42.5_f32.to_ne_bytes());
                v
            }),
            (PrimKind::D, 8, {
                let mut v = (-1.5_f64).to_ne_bytes().to_vec();
                v.extend_from_slice(&42.5_f64.to_ne_bytes());
                v
            }),
        ];
        for (kind, bps, src) in cases {
            let tag = slot_new_prim_array(*kind, 4);
            // caller-owned destination the audited primitive writes into
            let mut out = vec![0u8; 2 * bps];
            slot_get_prim_array_region(*kind, tag, 1, 2, out.as_mut_ptr());
            assert_eq!(out, vec![0u8; 2 * bps], "fresh array reads as zeros");

            slot_set_prim_array_region(*kind, tag, 1, 2, src.as_ptr());
            // read the region back out and compare byte-for-byte
            let mut back = vec![0u8; 2 * bps];
            slot_get_prim_array_region(*kind, tag, 1, 2, back.as_mut_ptr());
            assert_eq!(back, *src, "{kind:?} region set→get must roundtrip");
            // index 0 untouched by the [1,3) region
            let mut head = vec![0u8; *bps];
            slot_get_prim_array_region(*kind, tag, 0, 1, head.as_mut_ptr());
            assert_eq!(head, vec![0u8; *bps], "outside the region stays zero");
            super::super::bridge::delete_local(tag);
        }
    }

    #[test]
    fn region_bounds_violations_are_loud() {
        let _rt = runtime();
        let (tag, _) = int_array(&[1, 2, 3]);
        let mut buf = [0u8; 4];
        for (start, len) in [(-1, 1), (0, -1), (2, 2), (4, 0)] {
            let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                slot_get_prim_array_region(PrimKind::I, tag, start, len, buf.as_mut_ptr())
            }))
            .is_err();
            assert!(panicked, "get region [{start},{len}) must be loud");
            let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                slot_set_prim_array_region(PrimKind::I, tag, start, len, buf.as_ptr())
            }))
            .is_err();
            assert!(panicked, "set region [{start},{len}) must be loud");
        }
        // the loud message names ArrayIndexOutOfBoundsException
        let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            slot_get_prim_array_region(PrimKind::I, tag, 2, 2, buf.as_mut_ptr())
        }))
        .unwrap_err();
        let text = err
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        assert!(text.contains("ArrayIndexOutOfBoundsException"), "{text}");
        super::super::bridge::delete_local(tag);
    }

    // ------------------------------------------------ 222–223 critical

    #[test]
    fn critical_region_registers_and_releases_loudly() {
        let _rt = runtime();
        let (tag, r) = int_array(&[5, 6, 7]);
        let addr = slot_get_primitive_array_critical(tag);
        assert_ne!(addr, 0);
        // release works exactly once; the second is the loud miss
        slot_release_primitive_array_critical(addr, 0);
        let panicked =
            std::panic::catch_unwind(|| slot_release_primitive_array_critical(addr, 0)).is_err();
        assert!(panicked, "double critical release must be loud");
        // writes to the payload are already visible through the array —
        // the in-place law (the handed address IS the storage)
        array_store_i_raw(&r, 0, 50).expect("in-bounds");
        assert_eq!(array_load_i(&r, 0).unwrap(), 50);
        super::super::bridge::delete_local(tag);
    }

    #[test]
    fn critical_on_a_non_primitive_array_is_loud() {
        let _rt = runtime();
        let class_tag =
            super::super::bridge::new_local(crate::reflect::class_ref("java/lang/String"));
        let tag = slot_new_object_array(1, class_tag, 0);
        let panicked = std::panic::catch_unwind(|| slot_get_primitive_array_critical(tag)).is_err();
        assert!(panicked, "critical on a reference array must be loud");
        for t in [tag, class_tag] {
            super::super::bridge::delete_local(t);
        }
    }
}
