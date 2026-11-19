use super::*;

pub fn slot_call_object_method_v(mid: u64, obj_tag: u64, ap: usize) -> u64 {
    tag_or_zero(call_method_v(mid, obj_tag, ap))
}
pub fn slot_call_boolean_method_v(mid: u64, obj_tag: u64, ap: usize) -> u8 {
    match call_method_v(mid, obj_tag, ap) {
        Value::Int(i) => (i != 0) as u8,
        _ => 0,
    }
}
pub fn slot_call_byte_method_v(mid: u64, obj_tag: u64, ap: usize) -> i8 {
    match call_method_v(mid, obj_tag, ap) {
        Value::Int(i) => i as i8,
        _ => 0,
    }
}
pub fn slot_call_char_method_v(mid: u64, obj_tag: u64, ap: usize) -> u16 {
    match call_method_v(mid, obj_tag, ap) {
        Value::Int(i) => i as u16,
        _ => 0,
    }
}
pub fn slot_call_short_method_v(mid: u64, obj_tag: u64, ap: usize) -> i16 {
    match call_method_v(mid, obj_tag, ap) {
        Value::Int(i) => i as i16,
        _ => 0,
    }
}
pub fn slot_call_int_method_v(mid: u64, obj_tag: u64, ap: usize) -> i32 {
    match call_method_v(mid, obj_tag, ap) {
        Value::Int(i) => i,
        _ => 0,
    }
}
pub fn slot_call_long_method_v(mid: u64, obj_tag: u64, ap: usize) -> i64 {
    match call_method_v(mid, obj_tag, ap) {
        Value::Long(j) => j,
        _ => 0,
    }
}
pub fn slot_call_float_method_v(mid: u64, obj_tag: u64, ap: usize) -> f32 {
    match call_method_v(mid, obj_tag, ap) {
        Value::Float(f) => f,
        _ => 0.0,
    }
}
pub fn slot_call_double_method_v(mid: u64, obj_tag: u64, ap: usize) -> f64 {
    match call_method_v(mid, obj_tag, ap) {
        Value::Double(d) => d,
        _ => 0.0,
    }
}
/// Void has no zero to answer — the unit return is the whole story.
pub fn slot_call_void_method_v(mid: u64, obj_tag: u64, ap: usize) {
    let _ = call_method_v(mid, obj_tag, ap);
}
pub fn slot_call_static_object_method_v(mid: u64, clazz_tag: u64, ap: usize) -> u64 {
    tag_or_zero(call_static_method_v(mid, clazz_tag, ap))
}
pub fn slot_call_static_boolean_method_v(mid: u64, clazz_tag: u64, ap: usize) -> u8 {
    match call_static_method_v(mid, clazz_tag, ap) {
        Value::Int(i) => (i != 0) as u8,
        _ => 0,
    }
}
pub fn slot_call_static_byte_method_v(mid: u64, clazz_tag: u64, ap: usize) -> i8 {
    match call_static_method_v(mid, clazz_tag, ap) {
        Value::Int(i) => i as i8,
        _ => 0,
    }
}
pub fn slot_call_static_char_method_v(mid: u64, clazz_tag: u64, ap: usize) -> u16 {
    match call_static_method_v(mid, clazz_tag, ap) {
        Value::Int(i) => i as u16,
        _ => 0,
    }
}
pub fn slot_call_static_short_method_v(mid: u64, clazz_tag: u64, ap: usize) -> i16 {
    match call_static_method_v(mid, clazz_tag, ap) {
        Value::Int(i) => i as i16,
        _ => 0,
    }
}
pub fn slot_call_static_int_method_v(mid: u64, clazz_tag: u64, ap: usize) -> i32 {
    match call_static_method_v(mid, clazz_tag, ap) {
        Value::Int(i) => i,
        _ => 0,
    }
}
pub fn slot_call_static_long_method_v(mid: u64, clazz_tag: u64, ap: usize) -> i64 {
    match call_static_method_v(mid, clazz_tag, ap) {
        Value::Long(j) => j,
        _ => 0,
    }
}
pub fn slot_call_static_float_method_v(mid: u64, clazz_tag: u64, ap: usize) -> f32 {
    match call_static_method_v(mid, clazz_tag, ap) {
        Value::Float(f) => f,
        _ => 0.0,
    }
}
pub fn slot_call_static_double_method_v(mid: u64, clazz_tag: u64, ap: usize) -> f64 {
    match call_static_method_v(mid, clazz_tag, ap) {
        Value::Double(d) => d,
        _ => 0.0,
    }
}
/// Void has no zero to answer — the unit return is the whole story.
pub fn slot_call_static_void_method_v(mid: u64, clazz_tag: u64, ap: usize) {
    let _ = call_static_method_v(mid, clazz_tag, ap);
}
pub fn slot_call_nonvirtual_object_method_v(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    ap: usize,
) -> u64 {
    tag_or_zero(call_nonvirtual_method_v(mid, obj_tag, clazz_tag, ap))
}
pub fn slot_call_nonvirtual_boolean_method_v(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    ap: usize,
) -> u8 {
    match call_nonvirtual_method_v(mid, obj_tag, clazz_tag, ap) {
        Value::Int(i) => (i != 0) as u8,
        _ => 0,
    }
}
pub fn slot_call_nonvirtual_byte_method_v(mid: u64, obj_tag: u64, clazz_tag: u64, ap: usize) -> i8 {
    match call_nonvirtual_method_v(mid, obj_tag, clazz_tag, ap) {
        Value::Int(i) => i as i8,
        _ => 0,
    }
}
pub fn slot_call_nonvirtual_char_method_v(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    ap: usize,
) -> u16 {
    match call_nonvirtual_method_v(mid, obj_tag, clazz_tag, ap) {
        Value::Int(i) => i as u16,
        _ => 0,
    }
}
pub fn slot_call_nonvirtual_short_method_v(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    ap: usize,
) -> i16 {
    match call_nonvirtual_method_v(mid, obj_tag, clazz_tag, ap) {
        Value::Int(i) => i as i16,
        _ => 0,
    }
}
pub fn slot_call_nonvirtual_int_method_v(mid: u64, obj_tag: u64, clazz_tag: u64, ap: usize) -> i32 {
    match call_nonvirtual_method_v(mid, obj_tag, clazz_tag, ap) {
        Value::Int(i) => i,
        _ => 0,
    }
}
pub fn slot_call_nonvirtual_long_method_v(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    ap: usize,
) -> i64 {
    match call_nonvirtual_method_v(mid, obj_tag, clazz_tag, ap) {
        Value::Long(j) => j,
        _ => 0,
    }
}
pub fn slot_call_nonvirtual_float_method_v(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    ap: usize,
) -> f32 {
    match call_nonvirtual_method_v(mid, obj_tag, clazz_tag, ap) {
        Value::Float(f) => f,
        _ => 0.0,
    }
}
pub fn slot_call_nonvirtual_double_method_v(
    mid: u64,
    obj_tag: u64,
    clazz_tag: u64,
    ap: usize,
) -> f64 {
    match call_nonvirtual_method_v(mid, obj_tag, clazz_tag, ap) {
        Value::Double(d) => d,
        _ => 0.0,
    }
}
/// Void has no zero to answer — the unit return is the whole story.
pub fn slot_call_nonvirtual_void_method_v(mid: u64, obj_tag: u64, clazz_tag: u64, ap: usize) {
    let _ = call_nonvirtual_method_v(mid, obj_tag, clazz_tag, ap);
}
