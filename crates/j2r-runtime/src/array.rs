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

use crate::object::{JObj, JRef};
use crate::subtype::is_subtype;
use crate::Value;
use crate::{internal_error, throw_new, Dispatch, Throw};
use std::cell::RefCell;
use std::rc::Rc;

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

fn check_array(r: &JRef) -> Result<(i32, Rc<str>), Throw> {
    let b = r.borrow();
    let cls = b.jclass.clone();
    if !cls.starts_with('[') {
        return Err(internal_error(&format!(
            "array operation on non-array object {cls}"
        )));
    }
    let len = b.slots[0].expect_int();
    Ok((len, cls))
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
    Ok(JRef::new(RefCell::new(JObj {
        jclass: Rc::from(desc),
        slots,
    })))
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
    Ok(JRef::new(RefCell::new(JObj {
        jclass: Rc::from(desc.as_str()),
        slots,
    })))
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
        Ok(JRef::new(RefCell::new(JObj {
            jclass: Rc::from(desc),
            slots,
        })))
    }
}

pub fn array_length(r: &JRef) -> Result<i32, Throw> {
    Ok(check_array(r)?.0)
}

pub fn array_load(r: &JRef, idx: i32) -> Result<Value, Throw> {
    let (len, _cls) = check_array(r)?;
    bounds_ok(len, idx)?;
    Ok(r.borrow().slots[idx as usize + 1].clone())
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
            let obj_cls = obj.borrow().jclass.to_string();
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
    r.borrow_mut().slots[idx as usize + 1] = v;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::string_value;

    #[test]
    fn prim_array_roundtrip() {
        let a = array_new_prim(INT, 3).unwrap();
        assert_eq!(array_length(&a).unwrap(), 3);
        array_store(&a, 1, Value::Int(42), Dispatch::default()).unwrap();
        assert_eq!(array_load(&a, 1).unwrap().expect_int(), 42);
        assert_eq!(array_load(&a, 0).unwrap().expect_int(), 0);
    }

    #[test]
    fn bounds_message_matches_jdk() {
        let a = array_new_prim(INT, 3).unwrap();
        let err = array_load(&a, 5).unwrap_err();
        let msg = err.obj.borrow().slots[0].clone().expect_ref().unwrap();
        assert_eq!(&*string_value(&msg), "Index 5 out of bounds for length 3");
    }

    #[test]
    fn negative_size() {
        let err = array_new_prim(INT, -1).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/NegativeArraySizeException");
    }

    #[test]
    fn ref_array_of_array_descriptor_is_canonical() {
        // `anewarray [I` (javac's lowering of `new int[3][]`) must produce
        // the array class `[[I`, not `[L[I;`.
        let a = array_new_ref("[I", 3).unwrap();
        assert_eq!(&*a.borrow().jclass, "[[I");
        assert_eq!(array_length(&a).unwrap(), 3);
    }

    #[test]
    fn multi_fully_allocated() {
        let grid = array_new_multi("[[I", &[2, 3]).unwrap();
        assert_eq!(&*grid.borrow().jclass, "[[I");
        let row0 = match array_load(&grid, 0).unwrap() {
            Value::Ref(Some(r)) => r,
            other => panic!("expected ref, got {other:?}"),
        };
        assert_eq!(&*row0.borrow().jclass, "[I");
        assert_eq!(array_length(&row0).unwrap(), 3);
        array_store(&grid, 1, Value::Int(9), Dispatch::default()).unwrap();
    }

    #[test]
    fn multi_partially_allocated_rows_are_null() {
        let grid = array_new_multi("[[I", &[2]).unwrap();
        assert_eq!(&*grid.borrow().jclass, "[[I");
        assert!(matches!(array_load(&grid, 0).unwrap(), Value::Ref(None)));
    }

    #[test]
    fn multi_negative_dim_raises() {
        let err = array_new_multi("[[I", &[2, -5]).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/NegativeArraySizeException");
    }

    #[test]
    fn multi_ref_bottom() {
        let m = array_new_multi("[[Ljava/lang/String;", &[1, 2]).unwrap();
        assert_eq!(&*m.borrow().jclass, "[[Ljava/lang/String;");
        let row = match array_load(&m, 0).unwrap() {
            Value::Ref(Some(r)) => r,
            other => panic!("expected ref, got {other:?}"),
        };
        assert_eq!(&*row.borrow().jclass, "[Ljava/lang/String;");
        let s = crate::object::new_string("x");
        array_store(&row, 0, Value::Ref(Some(s)), Dispatch::default()).unwrap();
    }

    #[test]
    fn store_into_nested_prim_array_rejects_wrong_ref() {
        let m = array_new_multi("[[I", &[1, 1]).unwrap();
        let alien = crate::object::new_builtin("java/lang/StringBuilder").unwrap();
        let err = array_store(&m, 0, Value::Ref(Some(alien)), Dispatch::default()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/ArrayStoreException");
    }

    #[test]
    fn store_type_check() {
        let a = array_new_ref("java/lang/StringBuilder", 1).unwrap();
        let s = crate::object::new_string("str");
        let err = array_store(&a, 0, Value::Ref(Some(s)), Dispatch::default()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/ArrayStoreException");
        let sb = crate::object::new_builtin("java/lang/StringBuilder").unwrap();
        array_store(&a, 0, Value::Ref(Some(sb)), Dispatch::default()).unwrap();
    }
}
