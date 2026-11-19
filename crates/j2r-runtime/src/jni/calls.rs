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

#[cfg(test)]
use super::bridge::delete_local;
use crate::desc::{parse_method_desc, TypeDesc};
use crate::internal_error;
use crate::jni::classes::{resolve_method_id, MethodId};
use crate::{Throw, Value};
/// The CallBase fixture class the reflective-call tables repeat row
/// after row.
#[cfg(test)]
const CALL_BASE: &str = "j2r/test/CallBase";
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
pub(crate) enum CallFamily {
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
/// Resolves the method ID and checks the family/staticness agreement —
/// the shared head of the A-form and V-form invoke cores. A corrupted ID
/// or a staticness mismatch is a loud PANIC (foreign-side bugs, Temurin:
/// UB — Java cannot express either).
fn resolve_checked(
    family: CallFamily,
    mid: u64,
    form: &str,
) -> (MethodId, TypeDesc, Vec<TypeDesc>) {
    let m = resolve_method_id(mid);
    let (params, ret) = parse_method_desc(&m.sig).unwrap_or_else(|| {
        panic!(
            "j2r internal: method {}::{} has unparseable descriptor {}",
            m.owner, m.name, m.sig
        )
    });
    match family {
        CallFamily::Static if !m.is_static => panic!(
            "j2r JNI bridge: CallStatic*Method{form} given an INSTANCE method \
             ID for {}::{} — foreign-side bug",
            m.owner, m.name
        ),
        CallFamily::Static => {}
        _ if m.is_static => panic!(
            "j2r JNI bridge: Call*Method{form} ({}) given a STATIC method ID \
             for {}::{} — foreign-side bug",
            family.describe(),
            m.owner,
            m.name
        ),
        _ => {}
    }
    (m, ret, params)
}
/// The shared dispatch tail of both invoke cores: receiver + declared
/// class laws, the per-family start class, the ancestor walk, the invoke,
/// zero-return-on-throw, and the loud miss pend.
fn dispatch(
    family: CallFamily,
    m: &MethodId,
    ret: &TypeDesc,
    cooked: Vec<Value>,
    receiver_tag: u64,
    declared_tag: u64,
) -> Value {
    // Receiver law: virtual/nonvirtual dereference the receiver (null →
    // NPE pend + zero); static has none.
    let recv = match family {
        CallFamily::Static => None,
        _ if receiver_tag == 0 => {
            return pend_and_zero(
                ret,
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
                ret,
                crate::throw_new("java/lang/NullPointerException", None),
            );
        }
        _ => Some(
            super::classes::class_name_from_tag(declared_tag)
                .expect("a live jclass tag resolves — null was handled above"),
        ),
    };
    // The start class per family, then the ancestor walk (the JVM's own
    // resolution shape: start → parent_of until find_method hits — what
    // makes an ID minted through GetMethodID's ancestor walk work for a
    // receiver whose runtime class only INHERITS the method).
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
                Err(t) => pend_and_zero(ret, t),
            };
        }
        // The native-binding consult (T12, M5_JNI_NATIVES.md §4): a
        // native method has no lookup entry, so BEFORE advancing past
        // this class, ask the registry — the walk consults at EVERY
        // class it visits, which makes bind-on-Base resolve for a
        // Derived receiver exactly like an inherited compiled method.
        // Rust bindings invoke like any other method fn; FOREIGN fn
        // pointers go through the audited call thunk (primitive 11,
        // 13b) — the C native ABI served inside the SysV register
        // window, loud beyond it.
        if let Some(b) = super::natives::lookup(&cur, &m.name, &m.sig) {
            return match super::natives::invoke_binding(
                &cur,
                &m.name,
                &m.sig,
                b,
                recv.as_ref(),
                &cooked,
            ) {
                Ok(v) => {
                    if matches!(ret, TypeDesc::Void) {
                        Value::Ref(None)
                    } else {
                        v
                    }
                }
                Err(t) => pend_and_zero(ret, t),
            };
        }
        match ctx.parent_of(&cur) {
            Some(p) if p != cur => cur = p.to_string(),
            _ => break,
        }
    }
    pend_and_zero(
        ret,
        internal_error(&format!(
            "{} dispatch target {}.{}{} not in dispatch table",
            family.describe(),
            start,
            m.name,
            m.sig
        )),
    )
}
fn call_method_v(mid: u64, obj_tag: u64, ap: usize) -> Value {
    invoke_v(CallFamily::Virtual, mid, obj_tag, 0, ap)
}
fn call_static_method_v(mid: u64, clazz_tag: u64, ap: usize) -> Value {
    invoke_v(CallFamily::Static, mid, 0, clazz_tag, ap)
}
fn call_nonvirtual_method_v(mid: u64, obj_tag: u64, clazz_tag: u64, ap: usize) -> Value {
    invoke_v(CallFamily::Nonvirtual, mid, obj_tag, clazz_tag, ap)
}

// --- scanaislop split: the items below live verbatim in the child modules ---
mod slots_a;
mod slots_v;
#[cfg(test)]
mod tests;
mod vform;
pub(crate) use slots_a::*;
pub(crate) use slots_v::*;
pub(crate) use vform::*;
