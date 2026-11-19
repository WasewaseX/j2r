use super::*;

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
pub(crate) fn tag_or_zero(v: Value) -> u64 {
    match v {
        Value::Ref(Some(r)) => crate::jni::bridge::new_local(r),
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

// ---------------------------------------------------- V-form families (11c)
//
// The C signatures mirror the A-forms with the jvalue* replaced by a
// `va_list` (a POINTER to the caller's __va_list_tag record on this
// ABI): virtual `(env, obj, mid, ap)`, static `(env, clazz, mid, ap)`,
// nonvirtual `(env, obj, clazz, mid, ap)`. The walk reads the record
// through the audited primitives and consumes it into a LOCAL cursor
// (the caller's record is left untouched — the documented v1 law).
