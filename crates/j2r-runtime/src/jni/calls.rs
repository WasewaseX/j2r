//! T11 slice 11b — the Call*MethodA surface (design of record:
//! `docs/M5_JNI_CALLS.md`). The A-forms are the v1 workhorse: a plain
//! `jvalue*` array of 8-byte words, marshalled per the method's own
//! descriptor and invoked through the host dispatch. No variadics, no
//! unsafe, no approximation — and the FIRST faithful exception
//! transport: a called method that throws returns the zero value with
//! the throwable PENDING (the per-thread cell in [`super::bridge`]),
//! reported by `ExceptionOccurred`/`ExceptionCheck` and cleared by
//! `ExceptionClear`.
//!
//! Failure laws (M5_JNI_CALLS.md §4):
//! - a foreign/corrupted `jmethodID` is a LOUD registry miss (T09 law);
//! - a null receiver on a virtual call pends `NullPointerException` and
//!   returns the zero value — the oracle's shape;
//! - a `jvalue*` that is null while the descriptor demands arguments
//!   pends `NullPointerException` (Temurin's own call would be UB);
//! - builtin classes have no compiled method table: `GetMethodID`
//!   pends `NoSuchMethodError` and returns null (v1 gap, T12 owns the
//!   native-binding story).

use crate::desc::{parse_method_desc, TypeDesc};
use crate::jni::classes::resolve_method_id;
use crate::{Throw, Value};

/// Decodes one 8-byte `jvalue` word per the parameter descriptor.
/// Integral sub-word kinds ride the LOW bytes (little-endian) of the
/// union slot; `F` occupies the low 4 bytes of its 8-byte slot.
fn decode_jvalue(word: &[u8], t: &TypeDesc) -> Value {
    debug_assert_eq!(word.len(), 8, "a jvalue word is 8 bytes");
    match t {
        TypeDesc::Boolean => Value::Int((word[0] != 0) as i32),
        TypeDesc::Byte => Value::Int(word[0] as i8 as i32),
        TypeDesc::Short => Value::Int(i16::from_le_bytes([word[0], word[1]]) as i32),
        TypeDesc::Char => Value::Int(u16::from_le_bytes([word[0], word[1]]) as i32),
        TypeDesc::Int => Value::Int(i32::from_le_bytes([word[0], word[1], word[2], word[3]])),
        TypeDesc::Long => Value::Long(i64::from_le_bytes(word.try_into().expect("8 bytes"))),
        TypeDesc::Float => Value::Float(f32::from_le_bytes([word[0], word[1], word[2], word[3]])),
        TypeDesc::Double => Value::Double(f64::from_le_bytes(word.try_into().expect("8 bytes"))),
        TypeDesc::Object(_) | TypeDesc::Array(_) => {
            // The pointer word IS a handle-table tag (the bridge's own
            // representation crosses untouched); 0 = JNI null.
            let tag = usize::from_le_bytes(word.try_into().expect("8 bytes")) as u64;
            if tag == 0 {
                Value::Ref(None)
            } else {
                Value::Ref(Some(super::bridge::resolve(tag)))
            }
        }
        TypeDesc::Void => {
            panic!("j2r internal: a method parameter has descriptor void — never emitted by javac")
        }
    }
}

/// The zero value a throwing call returns per the return descriptor
/// (JNI: the caller must check ExceptionOccurred instead of the result).
fn zero_value(t: &TypeDesc) -> Value {
    match t {
        TypeDesc::Boolean | TypeDesc::Byte | TypeDesc::Short | TypeDesc::Char | TypeDesc::Int => {
            Value::Int(0)
        }
        TypeDesc::Long => Value::Long(0),
        TypeDesc::Float => Value::Float(0.0),
        TypeDesc::Double => Value::Double(0.0),
        TypeDesc::Object(_) | TypeDesc::Array(_) | TypeDesc::Void => Value::Ref(None),
    }
}
// (Void in a PARAMETER position never occurs — javac cannot emit it;
// the arm keeps the match exhaustive.)

/// Pends `t` and answers the zero return for the descriptor.
fn pend_and_zero(t: &TypeDesc, thrown: Throw) -> Value {
    super::bridge::pend_exception(thrown);
    zero_value(t)
}

/// `Call<K>MethodA` body (K over the ten return kinds): resolve the
/// methodID, decode the jvalue words per the descriptor, dispatch.
/// NEVER fails: every Java-observable failure pends a throwable and
/// answers the zero value — the JNI-faithful shape the wrappers rely on.
///
/// Dispatch per family — this slice lands the VIRTUAL family (dispatch
/// by the RECEIVER's runtime class, the ordinary override law); the
/// static/nonvirtual families are slice 11b part 2 (same core, same
/// tests' shape — the enum arm is already the honest placeholder).
fn call_method_a(mid: u64, obj_tag: u64, args_ptr: usize) -> Value {
    let m = resolve_method_id(mid);
    let (params, ret) = parse_method_desc(&m.sig).unwrap_or_else(|| {
        panic!(
            "j2r internal: method {}::{} has unparseable descriptor {}",
            m.owner, m.name, m.sig
        )
    });
    if m.is_static {
        panic!(
            "j2r JNI bridge: Call*MethodA given a STATIC method ID for {}::{} — \
             foreign-side bug",
            m.owner, m.name
        );
    }
    // Fetch the jvalue words through the audited primitive — null args
    // with a demanding descriptor pends NPE (Temurin's is UB there).
    let mut cooked: Vec<Value> = Vec::with_capacity(params.len());
    if !params.is_empty() {
        if args_ptr == 0 {
            return pend_and_zero(
                &ret,
                crate::throw_new("java/lang/NullPointerException", None),
            );
        }
        let bytes = j2r_jni_ffi::ffi::copy_from_caller(args_ptr as *const u8, params.len() * 8);
        for (i, t) in params.iter().enumerate() {
            cooked.push(decode_jvalue(&bytes[i * 8..i * 8 + 8], t));
        }
    }
    // Virtual dispatch: the RECEIVER's runtime class decides the
    // override; a null receiver pends NPE and answers the zero value.
    if obj_tag == 0 {
        return pend_and_zero(
            &ret,
            crate::throw_new("java/lang/NullPointerException", None),
        );
    }
    let recv = super::bridge::resolve(obj_tag);
    let ctx = super::host_dispatch();
    let jc = crate::heap::with_obj(recv, |o| o.jclass.to_string());
    let Some(f) = ctx.find_method(&jc, &m.name, &m.sig) else {
        return pend_and_zero(
            &ret,
            crate::internal_error(&format!(
                "virtual target {jc}.{}{} not in dispatch table",
                m.name, m.sig
            )),
        );
    };
    match f(Some(&recv), &cooked) {
        Ok(v) => {
            if matches!(ret, TypeDesc::Void) {
                Value::Ref(None)
            } else {
                v
            }
        }
        Err(t) => pend_and_zero(&ret, t),
    }
}

// ------------------------------------------------------- slot-level API
//
// One body per return kind; the generated wrappers (thin, like the
// string family) decode the C types, hand over (mid, obj_tag, args_ptr)
// and encode the answer. Every body pends-and-answers-zero on a thrown
// exception — the wrapper CANNOT fail, the JNI-faithful shape.

fn tag_or_zero(v: Value) -> u64 {
    match v {
        Value::Ref(Some(r)) => super::bridge::new_local(r),
        _ => 0, // Void/null both answer JNI null at this boundary
    }
}

pub fn slot_call_object_method_a(mid: u64, obj_tag: u64, args_ptr: usize) -> u64 {
    tag_or_zero(call_method_a(mid, obj_tag, args_ptr))
}

pub fn slot_call_boolean_method_a(mid: u64, obj_tag: u64, args_ptr: usize) -> u8 {
    match call_method_a(mid, obj_tag, args_ptr) {
        Value::Int(i) => (i != 0) as u8,
        _ => 0,
    }
}

pub fn slot_call_byte_method_a(mid: u64, obj_tag: u64, args_ptr: usize) -> i8 {
    match call_method_a(mid, obj_tag, args_ptr) {
        Value::Int(i) => i as i8,
        _ => 0,
    }
}

pub fn slot_call_char_method_a(mid: u64, obj_tag: u64, args_ptr: usize) -> u16 {
    match call_method_a(mid, obj_tag, args_ptr) {
        Value::Int(i) => i as u16,
        _ => 0,
    }
}

pub fn slot_call_short_method_a(mid: u64, obj_tag: u64, args_ptr: usize) -> i16 {
    match call_method_a(mid, obj_tag, args_ptr) {
        Value::Int(i) => i as i16,
        _ => 0,
    }
}

pub fn slot_call_int_method_a(mid: u64, obj_tag: u64, args_ptr: usize) -> i32 {
    match call_method_a(mid, obj_tag, args_ptr) {
        Value::Int(i) => i,
        _ => 0,
    }
}

pub fn slot_call_long_method_a(mid: u64, obj_tag: u64, args_ptr: usize) -> i64 {
    match call_method_a(mid, obj_tag, args_ptr) {
        Value::Long(j) => j,
        _ => 0,
    }
}

pub fn slot_call_float_method_a(mid: u64, obj_tag: u64, args_ptr: usize) -> f32 {
    match call_method_a(mid, obj_tag, args_ptr) {
        Value::Float(f) => f,
        _ => 0.0,
    }
}

pub fn slot_call_double_method_a(mid: u64, obj_tag: u64, args_ptr: usize) -> f64 {
    match call_method_a(mid, obj_tag, args_ptr) {
        Value::Double(d) => d,
        _ => 0.0,
    }
}

/// Void has no zero to answer — the unit return is the whole story.
pub fn slot_call_void_method_a(mid: u64, obj_tag: u64, args_ptr: usize) {
    let _ = call_method_a(mid, obj_tag, args_ptr);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::runtime;

    #[test]
    fn jvalue_words_decode_per_descriptor() {
        // Int-group sub-word kinds ride the LOW bytes (little-endian);
        // F occupies the low 4 bytes of its 8-byte slot.
        let mut w = [0u8; 8];
        w[0] = 1;
        assert_eq!(decode_jvalue(&w, &TypeDesc::Boolean), Value::Int(1));
        w[0] = 0xFB; // -5 as i8
        assert_eq!(decode_jvalue(&w, &TypeDesc::Byte), Value::Int(-5));
        w[0..2].copy_from_slice(&0xFFD6_u16.to_le_bytes()); // -42 as i16
        assert_eq!(decode_jvalue(&w, &TypeDesc::Short), Value::Int(-42));
        w[0..2].copy_from_slice(&0x263A_u16.to_le_bytes());
        assert_eq!(decode_jvalue(&w, &TypeDesc::Char), Value::Int(0x263A));
        w[0..4].copy_from_slice(&(-42_i32).to_le_bytes());
        assert_eq!(decode_jvalue(&w, &TypeDesc::Int), Value::Int(-42));
        w.copy_from_slice(&(-42_i64).to_le_bytes());
        assert_eq!(decode_jvalue(&w, &TypeDesc::Long), Value::Long(-42));
        w[0..4].copy_from_slice(&1.5_f32.to_le_bytes());
        assert_eq!(decode_jvalue(&w, &TypeDesc::Float), Value::Float(1.5));
        w.copy_from_slice(&2.5_f64.to_le_bytes());
        assert_eq!(decode_jvalue(&w, &TypeDesc::Double), Value::Double(2.5));
        // reference: the pointer word IS the tag; 0 = null
        assert_eq!(
            decode_jvalue(&[0; 8], &TypeDesc::Object("x".into())),
            Value::Ref(None)
        );
        let _rt = runtime();
        let tag = super::super::bridge::new_local(crate::object::new_string("hi"));
        let mut ptr_word = [0u8; 8];
        ptr_word.copy_from_slice(&tag.to_le_bytes());
        match decode_jvalue(&ptr_word, &TypeDesc::Object("x".into())) {
            Value::Ref(Some(r)) => assert_eq!(
                &*crate::object::string_value(&r),
                "hi",
                "the tag crosses into a real object"
            ),
            other => panic!("expected ref, got {other:?}"),
        }
        super::super::bridge::delete_local(tag);
    }

    #[test]
    fn zero_values_match_the_return_descriptor() {
        assert_eq!(zero_value(&TypeDesc::Int), Value::Int(0));
        assert_eq!(zero_value(&TypeDesc::Long), Value::Long(0));
        assert_eq!(zero_value(&TypeDesc::Float), Value::Float(0.0));
        assert_eq!(zero_value(&TypeDesc::Double), Value::Double(0.0));
        assert_eq!(zero_value(&TypeDesc::Object("x".into())), Value::Ref(None));
        assert_eq!(zero_value(&TypeDesc::Void), Value::Ref(None));
    }

    #[test]
    fn method_ids_are_deduplicated_marked_and_disjoint_from_field_ids() {
        let _rt = runtime();
        let id1 = super::super::classes::mint_for_test("a/B", "foo", "(I)V", false);
        let id2 = super::super::classes::mint_for_test("a/B", "foo", "(I)V", false);
        assert_eq!(id1, id2, "the same triple mints the same ID forever");
        let resolved = super::super::classes::resolve_method_id(id1);
        assert_eq!(resolved.owner, "a/B");
        assert_eq!(resolved.name, "foo");
        assert_eq!(resolved.sig, "(I)V");
        assert!(!resolved.is_static);
        // the mark is bit 62 — a FIELD id (bit 63) is loud at resolve
        let panicked =
            std::panic::catch_unwind(|| super::super::classes::resolve_method_id(1 << 63 | 1))
                .is_err();
        assert!(panicked, "a field ID passed as a method ID must be loud");
        // an unmarked foreign pointer is loud too
        let panicked =
            std::panic::catch_unwind(|| super::super::classes::resolve_method_id(0xBEEF)).is_err();
        assert!(panicked, "a foreign jmethodID must be loud");
    }

    #[test]
    fn pending_exception_cell_roundtrips_per_thread() {
        let _rt = runtime();
        assert!(!super::super::bridge::has_pending_exception());
        assert_eq!(super::super::bridge::pending_exception_local(), None);
        let t = crate::throw_new("java/lang/RuntimeException", Some("boom"));
        super::super::bridge::pend_exception(t);
        assert!(super::super::bridge::has_pending_exception());
        // ExceptionOccurred is REPEATABLE: fresh local ref each call,
        // cell untouched
        let l1 = super::super::bridge::pending_exception_local().expect("pending");
        let l2 = super::super::bridge::pending_exception_local().expect("still pending");
        assert_ne!(l1, l2, "each ExceptionOccurred mints a FRESH local ref");
        assert!(super::super::bridge::has_pending_exception());
        // ExceptionClear takes it away
        let taken = super::super::bridge::take_pending_exception().expect("cleared");
        assert_eq!(
            crate::heap::with_obj(taken.obj, |o| o.jclass.to_string()),
            "java/lang/RuntimeException"
        );
        assert!(!super::super::bridge::has_pending_exception());
        assert!(super::super::bridge::take_pending_exception().is_none());
        super::super::bridge::delete_local(l1);
        super::super::bridge::delete_local(l2);
    }
}
