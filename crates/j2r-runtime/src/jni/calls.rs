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
//! - a null receiver on a virtual/nonvirtual call pends
//!   `NullPointerException` and returns the zero value — the oracle's
//!   shape;
//! - a `jvalue*` that is null while the descriptor demands arguments
//!   pends `NullPointerException` (Temurin's own call would be UB);
//! - a null `jclass` on a static/nonvirtual call pends
//!   `NullPointerException` (defensive — Temurin's is UB there); a live
//!   non-Class tag panics loudly (foreign-side bug);
//! - a family/staticness mismatch (e.g. an instance ID through
//!   `CallStatic*MethodA`) is a LOUD panic — Temurin: UB, Java cannot
//!   express it;
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

/// The three Call*MethodA families (M5_JNI_CALLS.md §4) — all landed in
/// 11b (part 1 the virtual family, part 2 the static/nonvirtual ones);
/// one core, three resolution laws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallFamily {
    /// `Call<K>MethodA`: dispatch by the RECEIVER's runtime class (the
    /// ordinary override law).
    Virtual,
    /// `CallStatic<K>MethodA`: resolve on the jclass's class; no receiver.
    Static,
    /// `CallNonvirtual<K>MethodA`: dispatch on the DECLARED class — the
    /// `super.m()` shape. Implementation precision (recorded in the
    /// design note, 11b-2): the JNI spec ties the method ID to the jclass
    /// argument ("the method ID must be obtained by calling
    /// GetMethodID() on the class clazz"). The walk therefore STARTS at
    /// the jclass's own class and covers its ancestors — on every VALID
    /// combination that lands on exactly the body the ID names (the same
    /// first-declaring class GetMethodID's walk found, m.owner), while an
    /// invalid combination answers by the documented declared-class law
    /// instead of Temurin's UB.
    Nonvirtual,
}

impl CallFamily {
    fn describe(self) -> &'static str {
        match self {
            CallFamily::Virtual => "virtual",
            CallFamily::Static => "static",
            CallFamily::Nonvirtual => "nonvirtual",
        }
    }
}

/// The A-form invoke core, shared by all three families. NEVER fails:
/// every Java-observable failure pends a throwable and answers the zero
/// value — the JNI-faithful shape the wrappers rely on. Loud PANICS are
/// reserved for foreign-side bugs that Java cannot express (a corrupted
/// method ID, a family/staticness mismatch).
///
/// Resolution law (11b-2): the family picks the START class —
/// virtual: the receiver's runtime class; static/nonvirtual: the
/// declared class (see [`CallFamily::Nonvirtual`] for the precision) —
/// then the walk goes start → ancestors via `parent_of` until
/// `find_method` hits, the JVM's own resolution shape (invokevirtual/
/// invokestatic search the class and its superclasses). The walk is
/// what makes an ID minted through GetMethodID's ancestor walk (owner =
/// the declaring superclass) work for a receiver whose runtime class
/// only INHERITS the method.
fn invoke_a(
    family: CallFamily,
    mid: u64,
    receiver_tag: u64,
    declared_tag: u64,
    args_ptr: usize,
) -> Value {
    let m = resolve_method_id(mid);
    let (params, ret) = parse_method_desc(&m.sig).unwrap_or_else(|| {
        panic!(
            "j2r internal: method {}::{} has unparseable descriptor {}",
            m.owner, m.name, m.sig
        )
    });
    // The family and the ID's staticness must agree — a mismatch is a
    // foreign-side bug (Temurin: UB), so this is a loud panic, not a pend.
    match family {
        CallFamily::Static if !m.is_static => panic!(
            "j2r JNI bridge: CallStatic*MethodA given an INSTANCE method ID \
             for {}::{} — foreign-side bug",
            m.owner, m.name
        ),
        CallFamily::Static => {}
        _ if m.is_static => panic!(
            "j2r JNI bridge: Call*MethodA ({}) given a STATIC method ID \
             for {}::{} — foreign-side bug",
            family.describe(),
            m.owner,
            m.name
        ),
        _ => {}
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
    // Receiver law: virtual/nonvirtual dereference the receiver (null →
    // NPE pend + zero); static has none.
    let recv = match family {
        CallFamily::Static => None,
        _ if receiver_tag == 0 => {
            return pend_and_zero(
                &ret,
                crate::throw_new("java/lang/NullPointerException", None),
            );
        }
        _ => Some(super::bridge::resolve(receiver_tag)),
    };
    // Declared-class law: the static/nonvirtual families need the jclass.
    // JNI null pends NPE (defensive — Temurin's is UB there); a live tag
    // that is not a Class mirror panics inside `class_name_from_tag`
    // (foreign-side bug, never silent).
    let declared: Option<String> = match family {
        CallFamily::Virtual => None,
        _ if declared_tag == 0 => {
            return pend_and_zero(
                &ret,
                crate::throw_new("java/lang/NullPointerException", None),
            );
        }
        _ => Some(
            super::classes::class_name_from_tag(declared_tag)
                .expect("a live jclass tag resolves — null was handled above"),
        ),
    };
    // The start class per family, then the ancestor walk.
    let start: String = match family {
        CallFamily::Virtual => crate::heap::with_obj(
            *recv.as_ref().expect("virtual family has a receiver"),
            |o| o.jclass.to_string(),
        ),
        _ => declared.expect("static/nonvirtual resolved the declared class above"),
    };
    let ctx = super::host_dispatch();
    let mut cur = start.clone();
    loop {
        if let Some(f) = ctx.find_method(&cur, &m.name, &m.sig) {
            return match f(recv.as_ref(), &cooked) {
                Ok(v) => {
                    if matches!(ret, TypeDesc::Void) {
                        Value::Ref(None)
                    } else {
                        v
                    }
                }
                Err(t) => pend_and_zero(&ret, t),
            };
        }
        match ctx.parent_of(&cur) {
            Some(p) if p != cur => cur = p.to_string(),
            _ => break,
        }
    }
    pend_and_zero(
        &ret,
        crate::internal_error(&format!(
            "{} dispatch target {}.{}{} not in dispatch table",
            family.describe(),
            start,
            m.name,
            m.sig
        )),
    )
}

/// Virtual family core (the ten landed `Call<K>MethodA` slots).
fn call_method_a(mid: u64, obj_tag: u64, args_ptr: usize) -> Value {
    invoke_a(CallFamily::Virtual, mid, obj_tag, 0, args_ptr)
}

/// Static family core: the receiver ABI position carries the jclass
/// (the C signature is `(env, clazz, mid, args)`).
fn call_static_method_a(mid: u64, clazz_tag: u64, args_ptr: usize) -> Value {
    invoke_a(CallFamily::Static, mid, 0, clazz_tag, args_ptr)
}

/// Nonvirtual family core: BOTH the receiver and the declared class
/// (the C signature is `(env, obj, clazz, mid, args)`).
fn call_nonvirtual_method_a(mid: u64, obj_tag: u64, clazz_tag: u64, args_ptr: usize) -> Value {
    invoke_a(CallFamily::Nonvirtual, mid, obj_tag, clazz_tag, args_ptr)
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

// ------------------------------------------------- static family (11b-2)
//
// The C signature is `(env, clazz, mid, args)`: the RECEIVER position
// carries the jclass, there is no receiver, and the ID must have been
// minted by GetStaticMethodID (the staticness mismatch is loud).

pub fn slot_call_static_object_method_a(mid: u64, clazz_tag: u64, args_ptr: usize) -> u64 {
    tag_or_zero(call_static_method_a(mid, clazz_tag, args_ptr))
}

pub fn slot_call_static_boolean_method_a(mid: u64, clazz_tag: u64, args_ptr: usize) -> u8 {
    match call_static_method_a(mid, clazz_tag, args_ptr) {
        Value::Int(i) => (i != 0) as u8,
        _ => 0,
    }
}

pub fn slot_call_static_byte_method_a(mid: u64, clazz_tag: u64, args_ptr: usize) -> i8 {
    match call_static_method_a(mid, clazz_tag, args_ptr) {
        Value::Int(i) => i as i8,
        _ => 0,
    }
}

pub fn slot_call_static_char_method_a(mid: u64, clazz_tag: u64, args_ptr: usize) -> u16 {
    match call_static_method_a(mid, clazz_tag, args_ptr) {
        Value::Int(i) => i as u16,
        _ => 0,
    }
}

pub fn slot_call_static_short_method_a(mid: u64, clazz_tag: u64, args_ptr: usize) -> i16 {
    match call_static_method_a(mid, clazz_tag, args_ptr) {
        Value::Int(i) => i as i16,
        _ => 0,
    }
}

pub fn slot_call_static_int_method_a(mid: u64, clazz_tag: u64, args_ptr: usize) -> i32 {
    match call_static_method_a(mid, clazz_tag, args_ptr) {
        Value::Int(i) => i,
        _ => 0,
    }
}

pub fn slot_call_static_long_method_a(mid: u64, clazz_tag: u64, args_ptr: usize) -> i64 {
    match call_static_method_a(mid, clazz_tag, args_ptr) {
        Value::Long(j) => j,
        _ => 0,
    }
}

pub fn slot_call_static_float_method_a(mid: u64, clazz_tag: u64, args_ptr: usize) -> f32 {
    match call_static_method_a(mid, clazz_tag, args_ptr) {
        Value::Float(f) => f,
        _ => 0.0,
    }
}

pub fn slot_call_static_double_method_a(mid: u64, clazz_tag: u64, args_ptr: usize) -> f64 {
    match call_static_method_a(mid, clazz_tag, args_ptr) {
        Value::Double(d) => d,
        _ => 0.0,
    }
}

/// Void has no zero to answer — the unit return is the whole story.
pub fn slot_call_static_void_method_a(mid: u64, clazz_tag: u64, args_ptr: usize) {
    let _ = call_static_method_a(mid, clazz_tag, args_ptr);
}

// --------------------------------------------- nonvirtual family (11b-2)
//
// The C signature is `(env, obj, clazz, mid, args)`: BOTH the receiver
// and the declared class ride the call. Dispatch obeys the declared
// class (the `super.m()` law) — see [`CallFamily::Nonvirtual`].

pub fn slot_call_nonvirtual_object_method_a(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    args_ptr: usize,
) -> u64 {
    tag_or_zero(call_nonvirtual_method_a(mid, obj_tag, clazz_tag, args_ptr))
}

pub fn slot_call_nonvirtual_boolean_method_a(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    args_ptr: usize,
) -> u8 {
    match call_nonvirtual_method_a(mid, obj_tag, clazz_tag, args_ptr) {
        Value::Int(i) => (i != 0) as u8,
        _ => 0,
    }
}

pub fn slot_call_nonvirtual_byte_method_a(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    args_ptr: usize,
) -> i8 {
    match call_nonvirtual_method_a(mid, obj_tag, clazz_tag, args_ptr) {
        Value::Int(i) => i as i8,
        _ => 0,
    }
}

pub fn slot_call_nonvirtual_char_method_a(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    args_ptr: usize,
) -> u16 {
    match call_nonvirtual_method_a(mid, obj_tag, clazz_tag, args_ptr) {
        Value::Int(i) => i as u16,
        _ => 0,
    }
}

pub fn slot_call_nonvirtual_short_method_a(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    args_ptr: usize,
) -> i16 {
    match call_nonvirtual_method_a(mid, obj_tag, clazz_tag, args_ptr) {
        Value::Int(i) => i as i16,
        _ => 0,
    }
}

pub fn slot_call_nonvirtual_int_method_a(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    args_ptr: usize,
) -> i32 {
    match call_nonvirtual_method_a(mid, obj_tag, clazz_tag, args_ptr) {
        Value::Int(i) => i,
        _ => 0,
    }
}

pub fn slot_call_nonvirtual_long_method_a(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    args_ptr: usize,
) -> i64 {
    match call_nonvirtual_method_a(mid, obj_tag, clazz_tag, args_ptr) {
        Value::Long(j) => j,
        _ => 0,
    }
}

pub fn slot_call_nonvirtual_float_method_a(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    args_ptr: usize,
) -> f32 {
    match call_nonvirtual_method_a(mid, obj_tag, clazz_tag, args_ptr) {
        Value::Float(f) => f,
        _ => 0.0,
    }
}

pub fn slot_call_nonvirtual_double_method_a(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    args_ptr: usize,
) -> f64 {
    match call_nonvirtual_method_a(mid, obj_tag, clazz_tag, args_ptr) {
        Value::Double(d) => d,
        _ => 0.0,
    }
}

/// Void has no zero to answer — the unit return is the whole story.
pub fn slot_call_nonvirtual_void_method_a(mid: u64, obj_tag: u64, clazz_tag: u64, args_ptr: usize) {
    let _ = call_nonvirtual_method_a(mid, obj_tag, clazz_tag, args_ptr);
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

    // -- the mock-dispatch fixture (M5_JNI_CALLS.md §4/§5) --------------
    //
    // Base declares mark()I = 1, twice(I)I = arg*2, sm()I static = 10,
    // sq(I)I static = arg², boom()I = throws, mk()Ljava/lang/String;
    // static = a fresh mirror; Derived overrides mark()I = 2 and
    // inherits everything else. The lookup matches per DECLARING class —
    // exactly the generated registry's shape — so a receiver whose
    // runtime class is Derived only reaches Base's inherited bodies
    // through the ancestor walk, which IS the resolution under test.
    // Every test takes `runtime()` FIRST (the global RUNTIME_LOCK) and
    // keeps the guard alive: the host dispatch is process-global and the
    // classes.rs fixture must not race this one.

    static BASE_METHODS: [crate::UserMethod; 6] = [
        crate::UserMethod {
            name: "mark",
            desc: "()I",
            is_static: false,
        },
        crate::UserMethod {
            name: "twice",
            desc: "(I)I",
            is_static: false,
        },
        crate::UserMethod {
            name: "sm",
            desc: "()I",
            is_static: true,
        },
        crate::UserMethod {
            name: "sq",
            desc: "(I)I",
            is_static: true,
        },
        crate::UserMethod {
            name: "boom",
            desc: "()I",
            is_static: false,
        },
        crate::UserMethod {
            name: "mk",
            desc: "()Ljava/lang/String;",
            is_static: true,
        },
    ];
    static DERIVED_METHODS: [crate::UserMethod; 1] = [crate::UserMethod {
        name: "mark",
        desc: "()I",
        is_static: false,
    }];
    static CALL_DERIVED_PARENTS: [&str; 1] = ["j2r/test/CallBase"];

    fn base_mark(
        _recv: Option<&crate::heap::JRef>,
        _args: &[Value],
    ) -> Result<Value, crate::Throw> {
        Ok(Value::Int(1))
    }

    fn derived_mark(
        _recv: Option<&crate::heap::JRef>,
        _args: &[Value],
    ) -> Result<Value, crate::Throw> {
        Ok(Value::Int(2))
    }

    fn base_twice(
        _recv: Option<&crate::heap::JRef>,
        args: &[Value],
    ) -> Result<Value, crate::Throw> {
        match args.first() {
            Some(Value::Int(i)) => Ok(Value::Int(i * 2)),
            other => panic!("fixture twice expects one Int arg, got {other:?}"),
        }
    }

    fn base_sm(_recv: Option<&crate::heap::JRef>, _args: &[Value]) -> Result<Value, crate::Throw> {
        Ok(Value::Int(10))
    }

    fn base_sq(_recv: Option<&crate::heap::JRef>, args: &[Value]) -> Result<Value, crate::Throw> {
        match args.first() {
            Some(Value::Int(i)) => Ok(Value::Int(i * i)),
            other => panic!("fixture sq expects one Int arg, got {other:?}"),
        }
    }

    fn base_boom(
        _recv: Option<&crate::heap::JRef>,
        _args: &[Value],
    ) -> Result<Value, crate::Throw> {
        Err(crate::throw_new(
            "java/lang/RuntimeException",
            Some("callee threw"),
        ))
    }

    fn base_mk(_recv: Option<&crate::heap::JRef>, _args: &[Value]) -> Result<Value, crate::Throw> {
        Ok(Value::Ref(Some(crate::object::new_string("from static"))))
    }

    fn dispatch_lookup(class: &str, name: &str, desc: &str) -> Option<crate::MethodFn> {
        match (class, name, desc) {
            ("j2r/test/CallBase", "mark", "()I") => Some(base_mark),
            ("j2r/test/CallBase", "twice", "(I)I") => Some(base_twice),
            ("j2r/test/CallBase", "sm", "()I") => Some(base_sm),
            ("j2r/test/CallBase", "sq", "(I)I") => Some(base_sq),
            ("j2r/test/CallBase", "boom", "()I") => Some(base_boom),
            ("j2r/test/CallBase", "mk", "()Ljava/lang/String;") => Some(base_mk),
            ("j2r/test/CallDerived", "mark", "()I") => Some(derived_mark),
            _ => None,
        }
    }

    fn meta_of(c: &str) -> Option<&'static crate::UserClassInfo> {
        static BASE_INFO: crate::UserClassInfo = crate::UserClassInfo {
            is_interface: false,
            simple_name: Some("CallBase"),
            fields: &[],
            methods: &BASE_METHODS,
        };
        static DERIVED_INFO: crate::UserClassInfo = crate::UserClassInfo {
            is_interface: false,
            simple_name: Some("CallDerived"),
            fields: &[],
            methods: &DERIVED_METHODS,
        };
        match c {
            "j2r/test/CallBase" => Some(&BASE_INFO),
            "j2r/test/CallDerived" => Some(&DERIVED_INFO),
            _ => None,
        }
    }

    fn parents_of(c: &str) -> Option<&'static [&'static str]> {
        if c == "j2r/test/CallDerived" {
            Some(&CALL_DERIVED_PARENTS)
        } else {
            None
        }
    }

    fn install_dispatch_fixture() {
        super::super::install_host_dispatch(crate::Dispatch {
            lookup: Some(dispatch_lookup),
            parents: Some(parents_of),
            meta: Some(meta_of),
            static_get: None,
            static_set: None,
        });
    }

    fn class_tag(internal: &str) -> u64 {
        super::super::bridge::new_local(crate::reflect::class_ref(internal))
    }

    fn derived_receiver() -> u64 {
        super::super::bridge::new_local(crate::heap::alloc(crate::object::JObj {
            jclass: std::sync::Arc::from("j2r/test/CallDerived"),
            slots: vec![],
            prim: None,
        }))
    }

    fn int_arg_word(v: i32) -> [u8; 8] {
        // The jvalue union slot carries sub-word kinds in the LOW bytes
        // (little-endian) — exactly what decode_jvalue reads for Int.
        let mut w = [0u8; 8];
        w[0..4].copy_from_slice(&v.to_le_bytes());
        w
    }

    fn base_receiver() -> u64 {
        super::super::bridge::new_local(crate::heap::alloc(crate::object::JObj {
            jclass: std::sync::Arc::from("j2r/test/CallBase"),
            slots: vec![],
            prim: None,
        }))
    }

    fn clear_pending() {
        super::super::bridge::take_pending_exception();
    }

    #[test]
    fn virtual_calls_dispatch_by_the_runtime_class_and_walk_ancestors() {
        let _rt = crate::test_support::runtime();
        install_dispatch_fixture();
        let recv = derived_receiver();
        let base_cls = class_tag("j2r/test/CallBase");
        let derived_cls = class_tag("j2r/test/CallDerived");
        // The override: mark()I minted on Derived runs Derived's body.
        let m_derived =
            super::super::classes::slot_get_method_id(derived_cls, "mark", "()I", false)
                .expect("Derived declares mark");
        assert_eq!(
            super::super::classes::resolve_method_id(m_derived).owner,
            "j2r/test/CallDerived"
        );
        assert_eq!(
            slot_call_int_method_a(m_derived, recv, 0),
            2,
            "the receiver's runtime class decides — the override runs"
        );
        // THE override-vs-declared-class proof: the SAME call shape with
        // an ID minted on Base must STILL run the override (virtual law),
        // never Base's body.
        let m_base = super::super::classes::slot_get_method_id(base_cls, "mark", "()I", false)
            .expect("Base declares mark");
        assert_eq!(
            super::super::classes::resolve_method_id(m_base).owner,
            "j2r/test/CallBase"
        );
        assert_eq!(
            slot_call_int_method_a(m_base, recv, 0),
            2,
            "virtual dispatch ignores the ID's declaring class — the \
             receiver's override wins"
        );
        // The ancestor walk: twice(I)I is declared on Base only; the ID
        // minted through Derived must resolve through the walk and reach
        // Base's body with the decoded argument.
        let m_twice =
            super::super::classes::slot_get_method_id(derived_cls, "twice", "(I)I", false)
                .expect("the walk finds twice on Base");
        assert_eq!(
            super::super::classes::resolve_method_id(m_twice).owner,
            "j2r/test/CallBase",
            "Get*MethodID mints the ID against the DECLARING class"
        );
        let word = int_arg_word(21);
        assert_eq!(
            slot_call_int_method_a(m_twice, recv, word.as_ptr() as usize),
            42,
            "the runtime-class walk reaches the inherited body"
        );
        clear_pending();
        super::super::bridge::delete_local(recv);
        super::super::bridge::delete_local(base_cls);
        super::super::bridge::delete_local(derived_cls);
    }

    #[test]
    fn nonvirtual_calls_dispatch_by_the_declared_class() {
        let _rt = crate::test_support::runtime();
        install_dispatch_fixture();
        let recv = derived_receiver(); // a Derived INSTANCE that overrides mark
        let base_cls = class_tag("j2r/test/CallBase");
        let derived_cls = class_tag("j2r/test/CallDerived");
        let m_base = super::super::classes::slot_get_method_id(base_cls, "mark", "()I", false)
            .expect("Base declares mark");
        // THE super.m() law — the mirror image of the virtual proof:
        // same receiver (Derived, overriding), same Base-minted ID, but
        // the declared class is Base → Base's body (1), NOT the override.
        assert_eq!(
            slot_call_nonvirtual_int_method_a(m_base, recv, base_cls, 0),
            1,
            "nonvirtual dispatch obeys the DECLARED class, not the \
             receiver's runtime class"
        );
        // The declared class is Derived (its own override) → 2.
        let m_derived =
            super::super::classes::slot_get_method_id(derived_cls, "mark", "()I", false)
                .expect("Derived declares mark");
        assert_eq!(
            slot_call_nonvirtual_int_method_a(m_derived, recv, derived_cls, 0),
            2,
            "nonvirtual on the overriding declared class runs the override"
        );
        // The ancestor case: an ID minted on Base for an inherited
        // method, called nonvirtual with clazz=Derived — the declared-
        // class walk lands on Base's body (the JNI spec ties the ID to
        // the jclass, so this is the same body the ID names).
        let m_twice =
            super::super::classes::slot_get_method_id(derived_cls, "twice", "(I)I", false)
                .expect("the walk finds twice on Base");
        let word = int_arg_word(-4);
        assert_eq!(
            slot_call_nonvirtual_int_method_a(m_twice, recv, derived_cls, word.as_ptr() as usize),
            -8,
            "the declared-class walk covers the ancestors"
        );
        clear_pending();
        super::super::bridge::delete_local(recv);
        super::super::bridge::delete_local(base_cls);
        super::super::bridge::delete_local(derived_cls);
    }

    #[test]
    fn static_calls_resolve_on_the_declared_class_no_receiver() {
        let _rt = crate::test_support::runtime();
        install_dispatch_fixture();
        let base_cls = class_tag("j2r/test/CallBase");
        let derived_cls = class_tag("j2r/test/CallDerived");
        let m_sm = super::super::classes::slot_get_method_id(base_cls, "sm", "()I", true)
            .expect("Base declares static sm");
        // Static body runs with NO receiver and NO receiver effects.
        assert_eq!(
            slot_call_static_int_method_a(m_sm, base_cls, 0),
            10,
            "the static family resolves on the declared class"
        );
        // The same ID through the SUBclass: the declared-class walk
        // covers the ancestors (faithful static resolution).
        assert_eq!(
            slot_call_static_int_method_a(m_sm, derived_cls, 0),
            10,
            "the static walk resolves through the subclass"
        );
        // Arguments cross (static + descriptor-driven decode): sq(I)I.
        let m_sq = super::super::classes::slot_get_method_id(base_cls, "sq", "(I)I", true)
            .expect("Base declares static sq");
        let word = int_arg_word(7);
        assert_eq!(
            slot_call_static_int_method_a(m_sq, base_cls, word.as_ptr() as usize),
            49,
            "jvalue args decode per the descriptor and reach the callee"
        );
        // The Object kind on the static family: the answer is a rooted
        // local-ref tag carrying the mirror.
        let m_mk =
            super::super::classes::slot_get_method_id(base_cls, "mk", "()Ljava/lang/String;", true)
                .expect("Base declares static mk");
        let tag = slot_call_static_object_method_a(m_mk, base_cls, 0);
        assert_ne!(tag, 0, "a non-null answer is a live local-ref tag");
        let r = super::super::bridge::resolve(tag);
        assert_eq!(&*crate::object::string_value(&r), "from static");
        super::super::bridge::delete_local(tag);
        clear_pending();
        super::super::bridge::delete_local(base_cls);
        super::super::bridge::delete_local(derived_cls);
    }

    #[test]
    fn family_staticness_mismatches_are_loud() {
        let _rt = crate::test_support::runtime();
        install_dispatch_fixture();
        let base_cls = class_tag("j2r/test/CallBase");
        let instance_id = super::super::classes::slot_get_method_id(base_cls, "mark", "()I", false)
            .expect("instance mark");
        let static_id = super::super::classes::slot_get_method_id(base_cls, "sm", "()I", true)
            .expect("static sm");
        // instance ID through the static family → loud (Temurin: UB)
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = slot_call_static_int_method_a(instance_id, base_cls, 0);
        }))
        .is_err();
        assert!(panicked, "an instance ID in the static family must be loud");
        // static ID through the virtual family → loud
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = slot_call_int_method_a(static_id, 1, 0);
        }))
        .is_err();
        assert!(panicked, "a static ID in the virtual family must be loud");
        // static ID through the nonvirtual family → loud
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = slot_call_nonvirtual_int_method_a(static_id, 1, base_cls, 0);
        }))
        .is_err();
        assert!(
            panicked,
            "a static ID in the nonvirtual family must be loud"
        );
        clear_pending();
        super::super::bridge::delete_local(base_cls);
    }

    #[test]
    fn null_receiver_null_class_null_args_pend_npe_and_answer_zero() {
        let _rt = crate::test_support::runtime();
        install_dispatch_fixture();
        let base_cls = class_tag("j2r/test/CallBase");
        let m_mark = super::super::classes::slot_get_method_id(base_cls, "mark", "()I", false)
            .expect("instance mark");
        let m_twice = super::super::classes::slot_get_method_id(base_cls, "twice", "(I)I", false)
            .expect("instance twice");
        let m_sm = super::super::classes::slot_get_method_id(base_cls, "sm", "()I", true)
            .expect("static sm");
        let recv = base_receiver();
        // null receiver, virtual family → zero + pending NPE
        assert_eq!(slot_call_int_method_a(m_mark, 0, 0), 0, "zero return");
        assert!(super::super::bridge::has_pending_exception());
        let l = super::super::bridge::pending_exception_local().expect("pending");
        let r = super::super::bridge::resolve(l);
        assert_eq!(
            crate::heap::with_obj(r, |o| o.jclass.to_string()),
            "java/lang/NullPointerException"
        );
        super::super::bridge::delete_local(l);
        clear_pending();
        // null receiver, nonvirtual family → same law
        assert_eq!(slot_call_nonvirtual_int_method_a(m_mark, 0, base_cls, 0), 0);
        assert!(super::super::bridge::has_pending_exception());
        clear_pending();
        // null jclass, static family → same law (defensive; Temurin: UB)
        assert_eq!(slot_call_static_int_method_a(m_sm, 0, 0), 0);
        assert!(super::super::bridge::has_pending_exception());
        clear_pending();
        // null jclass, nonvirtual family → same law (a LIVE receiver so
        // the declared-class check is what fires)
        assert_eq!(slot_call_nonvirtual_int_method_a(m_mark, recv, 0, 0), 0);
        assert!(super::super::bridge::has_pending_exception());
        clear_pending();
        // null jvalue* with a demanding descriptor → same law (virtual)
        assert_eq!(slot_call_int_method_a(m_twice, recv, 0), 0);
        assert!(super::super::bridge::has_pending_exception());
        clear_pending();
        // …and the static family agrees
        assert_eq!(
            slot_call_static_int_method_a(m_sm, base_cls, 0),
            10,
            "a valid static call is unaffected"
        );
        super::super::bridge::delete_local(recv);
        super::super::bridge::delete_local(base_cls);
    }

    #[test]
    fn a_thrown_callee_pends_and_answers_the_zero_value() {
        let _rt = crate::test_support::runtime();
        install_dispatch_fixture();
        let base_cls = class_tag("j2r/test/CallBase");
        let recv = super::super::bridge::new_local(crate::heap::alloc(crate::object::JObj {
            jclass: std::sync::Arc::from("j2r/test/CallBase"),
            slots: vec![],
            prim: None,
        }));
        let m_boom = super::super::classes::slot_get_method_id(base_cls, "boom", "()I", false)
            .expect("Base declares boom");
        // The callee's Err is NOT propagated as a panic: the call answers
        // the zero value and the throwable waits in the cell.
        assert_eq!(slot_call_int_method_a(m_boom, recv, 0), 0, "zero return");
        let t = super::super::bridge::take_pending_exception().expect("pending throwable");
        assert_eq!(
            crate::heap::with_obj(t.obj, |o| o.jclass.to_string()),
            "java/lang/RuntimeException",
            "the callee's throwable is the pending one"
        );
        assert!(
            !super::super::bridge::has_pending_exception(),
            "take clears"
        );
        // The nonvirtual family obeys the same law (same core).
        assert_eq!(
            slot_call_nonvirtual_int_method_a(m_boom, recv, base_cls, 0),
            0
        );
        let t = super::super::bridge::take_pending_exception().expect("pending again");
        assert_eq!(
            crate::heap::with_obj(t.obj, |o| o.jclass.to_string()),
            "java/lang/RuntimeException"
        );
        super::super::bridge::delete_local(recv);
        super::super::bridge::delete_local(base_cls);
    }
}
