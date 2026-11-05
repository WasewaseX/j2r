//! Java arrays on the heap.
//!
//! Arrays are ordinary [`JObj`]s whose class is the array descriptor
//! (`[I`, `[Ljava/lang/String;`, ...). Slot 0 holds the length (as
//! `Value::Int`); slots `1..=len` hold the elements. Bounds violations raise
//! `ArrayIndexOutOfBoundsException` with the exact OpenJDK message; negative
//! sizes raise `NegativeArraySizeException`; storing the wrong reference type
//! raises `ArrayStoreException`.
//!
//! Callers obtain array references through `pop_ref`, which already converts
//! `null` into `NullPointerException`, so these functions only deal with
//! non-null references.

use crate::heap::{alloc, with_obj, with_obj_mut, JRef};
use crate::object::JObj;
use crate::subtype::is_subtype;
use crate::Value;
use crate::{internal_error, throw_new, Dispatch, Throw};
use std::sync::Arc;

pub const BOOLEAN: &str = "[Z";
pub const CHAR: &str = "[C";
pub const FLOAT: &str = "[F";
pub const DOUBLE: &str = "[D";
pub const BYTE: &str = "[B";
pub const SHORT: &str = "[S";
pub const INT: &str = "[I";
pub const LONG: &str = "[J";

fn default_element(desc: &str) -> Value {
    match desc {
        FLOAT => Value::Float(0.0),
        DOUBLE => Value::Double(0.0),
        LONG => Value::Long(0),
        BOOLEAN | CHAR | BYTE | SHORT | INT => Value::Int(0),
        _ => Value::Ref(None),
    }
}

fn check_array(r: &JRef) -> Result<(i32, Arc<str>), Throw> {
    // M4 Wave 1 lock law L3: no heap re-entry — the error object is built
    // AFTER the guard ends, from the class name the closure carried out.
    let checked: Result<(i32, Arc<str>), Arc<str>> = with_obj(*r, |b| {
        let cls = b.jclass.clone();
        if !cls.starts_with('[') {
            return Err(cls);
        }
        let len = b.slots[0].expect_int();
        Ok((len, cls))
    });
    checked.map_err(|cls| internal_error(&format!("array operation on non-array object {cls}")))
}

fn bounds_ok(len: i32, idx: i32) -> Result<(), Throw> {
    if idx < 0 || idx >= len {
        return Err(throw_new(
            "java/lang/ArrayIndexOutOfBoundsException",
            Some(&format!("Index {idx} out of bounds for length {len}")),
        ));
    }
    Ok(())
}

/// Shallow copy of any array — the JVM's array `clone()` (corpus wave 2,
/// JZlib: javac's enum `values()` compiles to `$VALUES.clone()`). Same
/// descriptor, same length, element-wise copy through load/store; for
/// reference arrays the ELEMENT REFERENCES are copied (identity is kept),
/// exactly like the JVM.
pub fn array_shallow_copy(src: &JRef, ctx: Dispatch) -> Result<JRef, Throw> {
    let (len, cls) = check_array(src)?;
    let out = if let Some(stripped) = cls.strip_prefix("[L").and_then(|s| s.strip_suffix(';')) {
        array_new_ref(stripped, len)?
    } else if cls.starts_with("[[") {
        array_new_ref(&cls[1..], len)?
    } else {
        array_new_prim(&cls, len)?
    };
    for i in 0..len {
        let v = array_load(src, i)?;
        array_store(&out, i, v, ctx)?;
    }
    Ok(out)
}

/// `newarray` (primitive arrays). `desc` is one of the `[*` constants above.
pub fn array_new_prim(desc: &str, len: i32) -> Result<JRef, Throw> {
    if len < 0 {
        return Err(throw_new(
            "java/lang/NegativeArraySizeException",
            Some(&len.to_string()),
        ));
    }
    let mut slots = Vec::with_capacity(len as usize + 1);
    slots.push(Value::Int(len));
    let dflt = default_element(desc);
    for _ in 0..len {
        slots.push(dflt.clone());
    }
    Ok(alloc(JObj {
        jclass: Arc::from(desc),
        slots,
    }))
}

/// `anewarray` (reference arrays); `elem_class` is the element class in
/// internal form — a plain class name (`java/lang/String`) or, when the
/// element is itself an array type, the array descriptor (`[I`,
/// `[[Ljava/lang/String;`, as javac emits for `new int[3][]`). The created
/// array's class is the canonical descriptor: `[{elem}` for array elements,
/// `[L{elem};` for class elements.
pub fn array_new_ref(elem_class: &str, len: i32) -> Result<JRef, Throw> {
    if len < 0 {
        return Err(throw_new(
            "java/lang/NegativeArraySizeException",
            Some(&len.to_string()),
        ));
    }
    let desc = if elem_class.starts_with('[') {
        format!("[{elem_class}")
    } else {
        format!("[L{elem_class};")
    };
    let mut slots = Vec::with_capacity(len as usize + 1);
    slots.push(Value::Int(len));
    for _ in 0..len {
        slots.push(Value::Ref(None));
    }
    Ok(alloc(JObj {
        jclass: Arc::from(desc.as_str()),
        slots,
    }))
}

/// `multianewarray`. `desc` is the full array-class descriptor from the
/// constant pool (e.g. `[[I`, `[[Ljava/lang/String;`); `dims` holds the
/// runtime dimension sizes outermost-first, and exactly `dims.len()` levels
/// are allocated — any deeper level stays `null` (JVMS §6.5 multianewarray).
/// Every allocated level carries its exact array class, so `aastore`/`checkcast`
/// subtype checks on the result see canonical descriptors.
pub fn array_new_multi(desc: &str, dims: &[i32]) -> Result<JRef, Throw> {
    let depth = desc.chars().take_while(|&c| c == '[').count();
    // The verifier rejects anything else before this runs.
    debug_assert!(!dims.is_empty(), "multianewarray with no dimensions");
    debug_assert!(
        dims.len() <= depth,
        "multianewarray {desc} with {} dims > depth {depth}",
        dims.len()
    );
    build_level(desc, dims)
}

fn build_level(desc: &str, dims: &[i32]) -> Result<JRef, Throw> {
    let len = dims[0];
    if len < 0 {
        return Err(throw_new(
            "java/lang/NegativeArraySizeException",
            Some(&len.to_string()),
        ));
    }
    if dims.len() == 1 {
        // Bottom allocated level: element type comes from desc[1..].
        let elem = &desc[1..];
        let mut chars = elem.chars();
        let tag = chars.next().unwrap_or('?');
        match tag {
            'Z' | 'C' | 'B' | 'S' | 'I' | 'J' | 'F' | 'D' => {
                // array_new_prim takes the array descriptor (`[I`), not the tag
                let arr_desc = format!("[{tag}");
                array_new_prim(&arr_desc, len)
            }
            'L' => {
                let class_name = elem
                    .strip_prefix('L')
                    .and_then(|s| s.strip_suffix(';'))
                    .ok_or_else(|| internal_error(&format!("malformed array descriptor {desc}")))?;
                array_new_ref(class_name, len)
            }
            // `[` — element is itself an array class; array_new_ref accepts
            // descriptors and builds `[{elem}`.
            '[' => array_new_ref(elem, len),
            other => Err(internal_error(&format!(
                "malformed array descriptor {desc} (elem tag {other:?})"
            ))),
        }
    } else {
        // Intermediate level: elements are sub-arrays (or null when the
        // caller allocated fewer levels than the descriptor's depth).
        let mut slots = Vec::with_capacity(len as usize + 1);
        slots.push(Value::Int(len));
        for _ in 0..len {
            let sub = build_level(&desc[1..], &dims[1..])?;
            slots.push(Value::Ref(Some(sub)));
        }
        Ok(alloc(JObj {
            jclass: Arc::from(desc),
            slots,
        }))
    }
}

pub fn array_length(r: &JRef) -> Result<i32, Throw> {
    Ok(check_array(r)?.0)
}

/// `System.arraycopy` — JDK semantics with messages verified against
/// Temurin 21.0.13 (corpus wave 1 probe, `corpus/AcpyProbe.java`):
///
/// 1. null src or dst → `NullPointerException` with message `null`;
/// 2. negative length → `ArrayIndexOutOfBoundsException`
///    `arraycopy: length {n} is negative`;
/// 3. `srcPos < 0` → `arraycopy: source index {p} out of bounds for {src}[{len}]`;
/// 4. `srcPos+len > srcLen` → `arraycopy: last source index {p+l} out of bounds for {src}[{len}]`;
/// 5. mirrored destination checks;
/// 6. type mismatch → `ArrayStoreException`
///    `arraycopy: type mismatch: can not copy {src}[] into {dst}[]`
///    (prim→prim different kinds, prim→ref; ref-element names print as
///    `object array` for `java.lang.Object`, dotted class otherwise);
/// 7. the copy itself is temp-buffered — overlapping regions behave as if
///    the source were snapshotted first, which is the documented JDK
///    behavior and exactly what the overlap probe observed;
/// 8. ref→ref copies run the per-element `array_store` subtype check in
///    destination order, so a failing element throws after its
///    predecessors were already stored (the JDK's partial-copy behavior;
///    the *message* in that path is the aastore-style one, which is a
///    known, documented drift until a JDK vector pins arraycopy's own).
pub fn system_arraycopy(args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (src, src_pos, dst, dst_pos, len) = match (
        args.first(),
        args.get(1),
        args.get(2),
        args.get(3),
        args.get(4),
    ) {
        (
            Some(Value::Ref(Some(src))),
            Some(Value::Int(sp)),
            Some(Value::Ref(Some(dst))),
            Some(Value::Int(dp)),
            Some(Value::Int(l)),
        ) => (src, *sp, dst, *dp, *l),
        // null src/dst or malformed arg shape → NPE with message `null`,
        // exactly like Temurin
        _ => return Err(throw_new("java/lang/NullPointerException", None)),
    };
    if len < 0 {
        return Err(throw_new(
            "java/lang/ArrayIndexOutOfBoundsException",
            Some(&format!("arraycopy: length {len} is negative")),
        ));
    }
    let (src_len, src_cls) = check_array(src)?;
    let (dst_len, dst_cls) = check_array(dst)?;
    if src_pos < 0 {
        return Err(throw_new(
            "java/lang/ArrayIndexOutOfBoundsException",
            Some(&format!(
                "arraycopy: source index {src_pos} out of bounds for {}[{src_len}]",
                java_array_name(&src_cls)
            )),
        ));
    }
    if src_pos.checked_add(len).is_none_or(|last| last > src_len) {
        return Err(throw_new(
            "java/lang/ArrayIndexOutOfBoundsException",
            Some(&format!(
                "arraycopy: last source index {} out of bounds for {}[{src_len}]",
                src_pos + len,
                java_array_name(&src_cls)
            )),
        ));
    }
    if dst_pos < 0 {
        return Err(throw_new(
            "java/lang/ArrayIndexOutOfBoundsException",
            Some(&format!(
                "arraycopy: destination index {dst_pos} out of bounds for {}[{dst_len}]",
                java_array_name(&dst_cls)
            )),
        ));
    }
    if dst_pos.checked_add(len).is_none_or(|last| last > dst_len) {
        return Err(throw_new(
            "java/lang/ArrayIndexOutOfBoundsException",
            Some(&format!(
                "arraycopy: last destination index {} out of bounds for {}[{dst_len}]",
                dst_pos + len,
                java_array_name(&dst_cls)
            )),
        ));
    }
    // Type-compatibility: primitive kinds must agree exactly; a primitive
    // source into a reference destination is an upfront ArrayStoreException.
    let src_ref_component = ref_elem(&src_cls);
    let dst_ref_component = ref_elem(&dst_cls);
    match (src_ref_component, dst_ref_component) {
        (None, None) => {
            if src_cls != dst_cls {
                return Err(type_mismatch(&src_cls, &dst_cls));
            }
        }
        (None, Some(_)) => return Err(type_mismatch(&src_cls, &dst_cls)),
        (Some(_), None) => return Err(type_mismatch(&src_cls, &dst_cls)),
        // ref→ref: element-by-element store checks happen during the copy.
        (Some(_), Some(_)) => {}
    }
    // Temp-buffer the source range (overlap-safe, JDK-documented).
    let mut temp: Vec<Value> = Vec::with_capacity(len as usize);
    for i in 0..len {
        temp.push(array_load(src, src_pos + i)?);
    }
    for (i, v) in temp.into_iter().enumerate() {
        array_store(dst, dst_pos + i as i32, v, ctx)?;
    }
    Ok(Value::Void)
}

/// The element descriptor of a reference-component array (`None` for
/// primitive arrays).
fn ref_elem(cls: &str) -> Option<String> {
    let b = cls.as_bytes();
    if b.len() >= 2 && (b[1] == b'L' || b[1] == b'[') {
        if b[1] == b'L' {
            cls.strip_prefix("[L")
                .and_then(|s| s.strip_suffix(';'))
                .map(|s| s.to_string())
        } else {
            Some(cls[1..].to_string())
        }
    } else {
        None
    }
}

fn type_mismatch(src_cls: &str, dst_cls: &str) -> Throw {
    throw_new(
        "java/lang/ArrayStoreException",
        Some(&format!(
            "arraycopy: type mismatch: can not copy {}[] into {}[]",
            java_elem_name(src_cls),
            java_elem_name(dst_cls)
        )),
    )
}

/// The Java-visible array name Temurin prints in arraycopy messages:
/// `int` for `[I`, `object array` for `[Ljava/lang/Object;`, dotted class
/// for other reference components.
fn java_array_name(cls: &str) -> String {
    match cls {
        INT => "int".to_string(),
        LONG => "long".to_string(),
        CHAR => "char".to_string(),
        BOOLEAN => "boolean".to_string(),
        BYTE => "byte".to_string(),
        SHORT => "short".to_string(),
        FLOAT => "float".to_string(),
        DOUBLE => "double".to_string(),
        "[Ljava/lang/Object;" => "object array".to_string(),
        other => other
            .strip_prefix("[L")
            .and_then(|s| s.strip_suffix(';'))
            .unwrap_or(other)
            .replace('/', "."),
    }
}

fn java_elem_name(cls: &str) -> String {
    match cls {
        INT => "int".to_string(),
        LONG => "long".to_string(),
        CHAR => "char".to_string(),
        BOOLEAN => "boolean".to_string(),
        BYTE => "byte".to_string(),
        SHORT => "short".to_string(),
        FLOAT => "float".to_string(),
        DOUBLE => "double".to_string(),
        "[Ljava/lang/Object;" => "object array".to_string(),
        other => other
            .strip_prefix("[L")
            .and_then(|s| s.strip_suffix(';'))
            .unwrap_or(other)
            .replace('/', "."),
    }
}

pub fn array_load(r: &JRef, idx: i32) -> Result<Value, Throw> {
    let (len, _cls) = check_array(r)?;
    bounds_ok(len, idx)?;
    Ok(with_obj(*r, |o| o.slots[idx as usize + 1].clone()))
}

pub fn array_store(r: &JRef, idx: i32, v: Value, ctx: Dispatch) -> Result<(), Throw> {
    let (len, cls) = check_array(r)?;
    bounds_ok(len, idx)?;
    // ArrayStoreException check for reference arrays. A reference-component
    // descriptor starts with `[L` (class element) or `[[` (array element,
    // e.g. `[[I` holding `[I` rows); anything else is a primitive array.
    let ref_component: Option<String> = {
        let b = cls.as_bytes();
        if b.len() >= 2 && (b[1] == b'L' || b[1] == b'[') {
            if b[1] == b'L' {
                cls.strip_prefix("[L")
                    .and_then(|s| s.strip_suffix(';'))
                    .map(|s| s.to_string())
            } else {
                Some(cls[1..].to_string())
            }
        } else {
            None
        }
    };
    if let Some(elem) = ref_component {
        if let Value::Ref(Some(obj)) = &v {
            let obj_cls = with_obj(*obj, |o| o.jclass.to_string());
            if !is_subtype(&obj_cls, &elem, ctx.parents) {
                return Err(throw_new(
                    "java/lang/ArrayStoreException",
                    Some(&format!(
                        "java.lang.{} stored into {}",
                        obj_cls.replace('/', "."),
                        cls
                    )),
                ));
            }
        }
    } else {
        // Primitive arrays must hold the matching primitive kind.
        let kind_ok = match cls.as_ref() {
            LONG => matches!(v, Value::Long(_)),
            FLOAT => matches!(v, Value::Float(_)),
            DOUBLE => matches!(v, Value::Double(_)),
            BOOLEAN | CHAR | BYTE | SHORT | INT => matches!(v, Value::Int(_)),
            _ => false,
        };
        if !kind_ok {
            return Err(internal_error(&format!("storing {v:?} into {cls}")));
        }
    }
    with_obj_mut(*r, |o| o.slots[idx as usize + 1] = v);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::string_value;

    #[test]
    fn prim_array_roundtrip() {
        let _rt = crate::test_support::runtime();
        let a = array_new_prim(INT, 3).unwrap();
        assert_eq!(array_length(&a).unwrap(), 3);
        array_store(&a, 1, Value::Int(42), Dispatch::default()).unwrap();
        assert_eq!(array_load(&a, 1).unwrap().expect_int(), 42);
        assert_eq!(array_load(&a, 0).unwrap().expect_int(), 0);
    }

    #[test]
    fn bounds_message_matches_jdk() {
        let _rt = crate::test_support::runtime();
        let a = array_new_prim(INT, 3).unwrap();
        let err = array_load(&a, 5).unwrap_err();
        let msg = crate::heap::with_obj(err.obj, |o| o.slots[0].clone())
            .expect_ref()
            .unwrap();
        assert_eq!(&*string_value(&msg), "Index 5 out of bounds for length 3");
    }

    #[test]
    fn negative_size() {
        let _rt = crate::test_support::runtime();
        let err = array_new_prim(INT, -1).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/NegativeArraySizeException");
    }

    #[test]
    fn ref_array_of_array_descriptor_is_canonical() {
        let _rt = crate::test_support::runtime();
        // `anewarray [I` (javac's lowering of `new int[3][]`) must produce
        // the array class `[[I`, not `[L[I;`.
        let a = array_new_ref("[I", 3).unwrap();
        assert_eq!(&*with_obj(a, |o| o.jclass.clone()), "[[I");
        assert_eq!(array_length(&a).unwrap(), 3);
    }

    #[test]
    fn multi_fully_allocated() {
        let _rt = crate::test_support::runtime();
        let grid = array_new_multi("[[I", &[2, 3]).unwrap();
        assert_eq!(&*with_obj(grid, |o| o.jclass.clone()), "[[I");
        let row0 = match array_load(&grid, 0).unwrap() {
            Value::Ref(Some(r)) => r,
            other => panic!("expected ref, got {other:?}"),
        };
        assert_eq!(&*with_obj(row0, |o| o.jclass.clone()), "[I");
        assert_eq!(array_length(&row0).unwrap(), 3);
        array_store(&grid, 1, Value::Int(9), Dispatch::default()).unwrap();
    }

    #[test]
    fn multi_partially_allocated_rows_are_null() {
        let _rt = crate::test_support::runtime();
        let grid = array_new_multi("[[I", &[2]).unwrap();
        assert_eq!(&*with_obj(grid, |o| o.jclass.clone()), "[[I");
        assert!(matches!(array_load(&grid, 0).unwrap(), Value::Ref(None)));
    }

    #[test]
    fn multi_negative_dim_raises() {
        let _rt = crate::test_support::runtime();
        let err = array_new_multi("[[I", &[2, -5]).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/NegativeArraySizeException");
    }

    #[test]
    fn multi_ref_bottom() {
        let _rt = crate::test_support::runtime();
        let m = array_new_multi("[[Ljava/lang/String;", &[1, 2]).unwrap();
        assert_eq!(&*with_obj(m, |o| o.jclass.clone()), "[[Ljava/lang/String;");
        let row = match array_load(&m, 0).unwrap() {
            Value::Ref(Some(r)) => r,
            other => panic!("expected ref, got {other:?}"),
        };
        assert_eq!(&*with_obj(row, |o| o.jclass.clone()), "[Ljava/lang/String;");
        let s = crate::object::new_string("x");
        array_store(&row, 0, Value::Ref(Some(s)), Dispatch::default()).unwrap();
    }

    #[test]
    fn store_into_nested_prim_array_rejects_wrong_ref() {
        let _rt = crate::test_support::runtime();
        let m = array_new_multi("[[I", &[1, 1]).unwrap();
        let alien = crate::object::new_builtin("java/lang/StringBuilder").unwrap();
        let err = array_store(&m, 0, Value::Ref(Some(alien)), Dispatch::default()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/ArrayStoreException");
    }

    #[test]
    fn store_type_check() {
        let _rt = crate::test_support::runtime();
        let a = array_new_ref("java/lang/StringBuilder", 1).unwrap();
        let s = crate::object::new_string("str");
        let err = array_store(&a, 0, Value::Ref(Some(s)), Dispatch::default()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/ArrayStoreException");
        let sb = crate::object::new_builtin("java/lang/StringBuilder").unwrap();
        array_store(&a, 0, Value::Ref(Some(sb)), Dispatch::default()).unwrap();
    }
}
