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
/// internal form (arrays allowed, e.g. `[[I`).
pub fn array_new_ref(elem_class: &str, len: i32) -> Result<JRef, Throw> {
    if len < 0 {
        return Err(throw_new(
            "java/lang/NegativeArraySizeException",
            Some(&len.to_string()),
        ));
    }
    let desc = format!("[L{elem_class};");
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
    // ArrayStoreException check for reference arrays.
    if cls.starts_with("[L") {
        let elem = cls
            .strip_prefix("[L")
            .and_then(|s| s.strip_suffix(';'))
            .expect("ref array descriptor");
        if let Value::Ref(Some(obj)) = &v {
            let obj_cls = obj.borrow().jclass.to_string();
            if !is_subtype(&obj_cls, elem, ctx.parents) {
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
    fn store_type_check() {
        let a = array_new_ref("java/lang/StringBuilder", 1).unwrap();
        let s = crate::object::new_string("str");
        let err = array_store(&a, 0, Value::Ref(Some(s)), Dispatch::default()).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/ArrayStoreException");
        let sb = crate::object::new_builtin("java/lang/StringBuilder").unwrap();
        array_store(&a, 0, Value::Ref(Some(sb)), Dispatch::default()).unwrap();
    }
}
