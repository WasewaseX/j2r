//! T10 slice 1 — JNI string marshalling (the modified-UTF-8 boundary).
//!
//! JNI strings cross the boundary as **modified UTF-8** (JVMS §4.4.7:
//! NUL encoded as `C0 80`, supplementary characters as surrogate pairs of
//! 3-byte sequences, no 4-byte form). The j2r string model is a Rust
//! `String` (slots[0] of a `java/lang/String` object), so every crossing
//! encodes or decodes at this module, never at a call site.
//!
//! **The decode is a byte-for-byte mirror of the differential oracle's
//! own semantics**: `j2r-classfile::decode_modified_utf8` (the loader that
//! reads every class the JDK ever hands us) — including its measured
//! lossy law for unpaired surrogates (JVMS 4.4.7 allows them; a Rust
//! `String` cannot hold one, so each normalizes to U+FFFD, the same
//! substitution the JDK's encoders emit — found and pinned by
//! corpus/Utf16Probe.java). The zero-dependency law forbids reaching into
//! `j2r-classfile`, so the mirror lives here, next to the tests that pin
//! BOTH sides against the same fixtures.
//!
//! Everything here is SAFE: bytes in, values out; the only raw-pointer
//! moments (reading the caller's C string, handing out our char buffer)
//! go through the audited `j2r-jni-ffi` primitives and the registry
//! pattern from the T09 bridge.

/// Decodes modified UTF-8 (JVMS §4.4.7) with the classfile loader's
/// pinned semantics: `None` on malformed sequences, U+FFFD normalization
/// for unpaired surrogates, `C0 80` → U+0000.
pub fn decode_modified_utf8(bytes: &[u8]) -> Option<String> {
    if bytes.iter().all(|b| *b != 0 && *b < 0x80) {
        return Some(String::from_utf8_lossy(bytes).into_owned());
    }
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        let unit = if b & 0x80 == 0 {
            i += 1;
            b as u16
        } else if b & 0xE0 == 0xC0 {
            let b2 = *bytes.get(i + 1)?;
            if b2 & 0xC0 != 0x80 {
                return None;
            }
            i += 2;
            (((b & 0x1F) as u16) << 6) | (b2 & 0x3F) as u16
        } else if b & 0xF0 == 0xE0 {
            let (b2, b3) = (*bytes.get(i + 1)?, *bytes.get(i + 2)?);
            if b2 & 0xC0 != 0x80 || b3 & 0xC0 != 0x80 {
                return None;
            }
            i += 3;
            (((b & 0x0F) as u16) << 12) | (((b2 & 0x3F) as u16) << 6) | (b3 & 0x3F) as u16
        } else {
            return None;
        };
        units.push(unit);
    }
    // Lossy, exactly like the loader: unpaired surrogates normalize to
    // U+FFFD (see the module doc — the corpus-pinned law).
    Some(String::from_utf16_lossy(&units))
}

/// Encodes a Rust `String` as modified UTF-8 (JVMS §4.4.7): U+0000 as
/// `C0 80`, supplementary characters as a surrogate pair of two 3-byte
/// sequences. Total — a Rust `String` holds no unpaired surrogates, so
/// every char has a modified-UTF-8 form.
pub fn encode_modified_utf8(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() + 1);
    for unit in s.encode_utf16() {
        match unit {
            0x0000 => out.extend_from_slice(&[0xC0, 0x80]),
            0x0001..=0x007F => out.push(unit as u8),
            0x0080..=0x07FF => {
                out.push(0xC0 | (unit >> 6) as u8);
                out.push(0x80 | (unit & 0x3F) as u8);
            }
            _ => {
                out.push(0xE0 | (unit >> 12) as u8);
                out.push(0x80 | ((unit >> 6) & 0x3F) as u8);
                out.push(0x80 | (unit & 0x3F) as u8);
            }
        }
    }
    out
}

// ------------------------------------------------------------- registry

/// `GetStringUTFChars` hands the caller a NUL-terminated buffer WE
/// allocate; the registry keeps the allocation alive until
/// `ReleaseStringUTFChars` (keyed by address-as-usize, the T09 bridge
/// pattern — a miss is a foreign-side bug and panics loudly).
fn utf_chars() -> &'static std::sync::Mutex<std::collections::HashMap<usize, Box<[u8]>>> {
    static UTF_CHARS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<usize, Box<[u8]>>>,
    > = std::sync::OnceLock::new();
    UTF_CHARS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

// ------------------------------------------------------- slot-level API
//
// The JNIEnv slot wrappers (slice 2 — the generator's real-slot registry)
// call THESE; each name matches its jni.h slot. Null handling follows the
// JNI spec per slot: a null `NewStringUTF` argument throws NPE (the slot
// wrapper maps the `None` here), `ReleaseStringUTFChars(null, ..)` is a
// no-op, `GetStringUTFChars` on a non-String handle panics at resolve.

/// `NewStringUTF` body: mUTF-8 bytes → a rooted local-ref tag to a
/// `java/lang/String`. `None` = the caller passed malformed mUTF-8 (the
/// slot wrapper returns JNI null; the spec's NoSuchElementError shape is
/// the T13 C-harness judge's business).
pub fn slot_new_string_utf(bytes: &[u8]) -> Option<u64> {
    let s = decode_modified_utf8(bytes)?;
    Some(super::bridge::new_local(crate::object::new_string(&s)))
}

/// `NewString` body: UTF-16 code units → a local-ref tag (the bytes came
/// in through `copy_from_caller` in the slot wrapper; the u16 view is
/// native-endian, the JNI ABI's own shape). Unpaired surrogates normalize
/// to U+FFFD — the pinned lossy law.
pub fn slot_new_string(units: &[u16]) -> u64 {
    let s = String::from_utf16_lossy(units);
    super::bridge::new_local(crate::object::new_string(&s))
}

/// `GetStringUTFLength` body: the modified-UTF-8 byte count, terminator
/// excluded (the JNI spec's own definition).
pub fn slot_get_string_utf_length(tag: u64) -> usize {
    let r = super::bridge::resolve(tag);
    encode_modified_utf8(&crate::object::string_value(&r)).len()
}

/// `GetStringUTFChars` body: hands out the address of a NUL-terminated
/// modified-UTF-8 copy, registry-pinned until release. `isCopy`, when the
/// caller asks, is `JNI_TRUE` (it is always a copy here — JNI_TRUE = 1).
pub fn slot_get_string_utf_chars(tag: u64, is_copy: Option<&mut u8>) -> usize {
    let r = super::bridge::resolve(tag);
    let mut buf = encode_modified_utf8(&crate::object::string_value(&r));
    buf.push(0); // the NUL terminator the JNI contract requires
    let boxed: Box<[u8]> = buf.into_boxed_slice();
    let addr = boxed.as_ptr() as usize;
    if let Some(flag) = is_copy {
        *flag = 1; // JNI_TRUE
    }
    utf_chars()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(addr, boxed);
    addr
}

/// `ReleaseStringUTFChars` body: frees the registry-pinned buffer. A
/// double release or a pointer we never handed out is loud (the T09
/// registry law).
pub fn slot_release_string_utf_chars(addr: usize) {
    if utf_chars()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&addr)
        .is_none()
    {
        panic!(
            "j2r JNI bridge: ReleaseStringUTFChars on an unregistered buffer 0x{addr:x} \
             — registry miss is a foreign-side bug"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_roundtrips() {
        let s = "Ljava/lang/Object;";
        let enc = encode_modified_utf8(s);
        assert_eq!(enc.len(), s.len());
        assert_eq!(decode_modified_utf8(&enc).as_deref(), Some(s));
    }

    #[test]
    fn nul_encodes_as_c0_80() {
        let s = "a\u{0}b";
        let enc = encode_modified_utf8(s);
        assert_eq!(enc, vec![b'a', 0xC0, 0x80, b'b']);
        assert_eq!(decode_modified_utf8(&enc).as_deref(), Some(s));
        // a RAW 0x00 byte decodes leniently to U+0000 — exactly the
        // classfile oracle's behavior (its slow path maps any <0x80 byte,
        // zero included; only malformed multibyte sequences are None).
        // The ENCODER never produces one, which is the side we control.
        assert_eq!(
            decode_modified_utf8(&[b'a', 0x00, b'b']).as_deref(),
            Some("a\u{0}b")
        );
    }

    #[test]
    fn supplementary_chars_use_surrogate_pairs() {
        // U+1D530 (𝔐 MATHEMATICAL FRAKTUR CAPITAL M) — supplementary
        let s = "a\u{1D530}b";
        let enc = encode_modified_utf8(s);
        // 'a' + surrogate pair (2 × 3 bytes) + 'b' = 8 bytes
        assert_eq!(enc.len(), 8);
        assert_eq!(decode_modified_utf8(&enc).as_deref(), Some(s));
    }

    #[test]
    fn unpaired_surrogate_normalizes_to_replacement_char() {
        // ED A0 80 decodes to the unpaired high surrogate D800 → U+FFFD
        let got = decode_modified_utf8(&[0xED, 0xA0, 0x80]).expect("legal mUTF-8");
        assert_eq!(got, "\u{FFFD}");
    }

    #[test]
    fn malformed_sequences_are_rejected() {
        assert_eq!(decode_modified_utf8(&[0xC0]), None); // truncated 2-byte
        assert_eq!(decode_modified_utf8(&[0xE0, 0x80]), None); // truncated 3-byte
        assert_eq!(decode_modified_utf8(&[0xF0, 0x80, 0x80, 0x80]), None); // 4-byte form
        assert_eq!(decode_modified_utf8(&[0xC0, 0x00]), None); // bad continuation
    }

    #[test]
    fn utf_length_excludes_the_terminator() {
        let _rt = crate::test_support::runtime();
        let tag = super::super::bridge::new_local(crate::object::new_string("ab\u{0}c"));
        // a(1) + C0 80(2) + b(1) + c(1) = 5 bytes, +1 NUL in the handed buffer
        assert_eq!(slot_get_string_utf_length(tag), 5);
        super::super::bridge::delete_local(tag);
    }

    #[test]
    fn new_string_utf_slot_roundtrip_through_the_bridge() {
        let _rt = crate::test_support::runtime();
        let tag = slot_new_string_utf(b"hello").expect("valid mUTF-8");
        let r = super::super::bridge::resolve(tag);
        assert_eq!(&*crate::object::string_value(&r), "hello");
        super::super::bridge::delete_local(tag);
        // malformed mUTF-8 → None (slot returns JNI null)
        assert_eq!(slot_new_string_utf(&[0xF0, 0x80, 0x80, 0x80]), None);
    }

    #[test]
    fn new_string_slot_from_utf16_units() {
        let _rt = crate::test_support::runtime();
        let units: Vec<u16> = "a\u{1D530}b".encode_utf16().collect();
        let tag = slot_new_string(&units);
        let r = super::super::bridge::resolve(tag);
        assert_eq!(&*crate::object::string_value(&r), "a\u{1D530}b");
        super::super::bridge::delete_local(tag);
    }

    #[test]
    fn utf_chars_registry_roundtrip_and_loud_miss() {
        let _rt = crate::test_support::runtime();
        let tag = super::super::bridge::new_local(crate::object::new_string("ABC"));
        let mut is_copy: u8 = 0;
        let addr = slot_get_string_utf_chars(tag, Some(&mut is_copy));
        assert_eq!(
            is_copy, 1,
            "GetStringUTFChars always copies here (JNI_TRUE)"
        );
        // the handed buffer is NUL-terminated modified UTF-8
        {
            let reg = utf_chars().lock().unwrap_or_else(|e| e.into_inner());
            let buf = reg.get(&addr).expect("registry holds the buffer");
            assert_eq!(&buf[..buf.len() - 1], b"ABC");
            assert_eq!(buf[buf.len() - 1], 0);
        }
        slot_release_string_utf_chars(addr); // fine
        let panicked = std::panic::catch_unwind(|| slot_release_string_utf_chars(addr)).is_err();
        assert!(panicked, "double release must be loud");
        let panicked = std::panic::catch_unwind(|| slot_release_string_utf_chars(0xDEAD)).is_err();
        assert!(panicked, "foreign pointer release must be loud");
        super::super::bridge::delete_local(tag);
    }
}
