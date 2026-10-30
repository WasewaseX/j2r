//! Heap object model.
//!
//! Every Java object (including strings, boxed values, arrays and native
//! JDK objects) is a [`JObj`] behind `Rc<RefCell<..>>` ([`JRef`]). Instance
//! fields live in flattened `slots`, one slot per field regardless of the
//! field's JVM width; the code generator assigns absolute slot indices
//! across whole class hierarchies, so field access is a direct index.

use crate::Value;
use crate::{throw_new, Throw};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, PartialEq)]
pub struct JObj {
    /// Runtime class in internal form (`a/b/C`); arrays use descriptors
    /// like `[I` or `[Ljava/lang/String;`.
    pub jclass: Rc<str>,
    /// Flattened field slots. Slot semantics are owned by the code
    /// generator's layout; natives use fixed slots documented per class.
    pub slots: Vec<Value>,
}

pub type JRef = Rc<RefCell<JObj>>;

/// Allocates a new object with `slots` default (Void) slots.
pub fn new_obj(class: &str, slots: usize) -> Result<JRef, Throw> {
    Ok(JRef::new(RefCell::new(JObj {
        jclass: Rc::from(class),
        slots: vec![Value::Void; slots],
    })))
}

/// Allocates an object of a builtin JDK class with the right slot count.
pub fn new_builtin(class: &str) -> Result<JRef, Throw> {
    match crate::builtin::builtin_total_slots(class) {
        Some(n) => new_obj(class, n),
        None => Err(crate::internal_error(&format!(
            "cannot instantiate builtin class {class}"
        ))),
    }
}

/// Reads a field slot (panics on out-of-range: a codegen bug, never a Java
/// program error).
pub fn obj_get(r: &JRef, idx: usize) -> Value {
    r.borrow().slots[idx].clone()
}

/// Writes a field slot.
pub fn obj_set(r: &JRef, idx: usize, v: Value) {
    r.borrow_mut().slots[idx] = v;
}

/// Creates a `java/lang/String` object.
pub fn new_string(s: &str) -> JRef {
    JRef::new(RefCell::new(JObj {
        jclass: Rc::from("java/lang/String"),
        slots: vec![Value::Str(Rc::new(s.to_string()))],
    }))
}

/// Creates a `java/lang/String` object sharing an existing character buffer
/// (used by `StringBuilder.toString`, which is safe because `StringBuilder`
/// mutations copy-on-write through `Rc::make_mut`).
pub fn string_from_rc(s: Rc<String>) -> JRef {
    JRef::new(RefCell::new(JObj {
        jclass: Rc::from("java/lang/String"),
        slots: vec![Value::Str(s)],
    }))
}

thread_local! {
    static INTERNED: RefCell<HashMap<Rc<str>, JRef>> = RefCell::new(HashMap::new());
}

/// Interns a string exactly like the JVM constant pool does: two `ldc`s of
/// the same literal yield the same reference (`==` identity).
pub fn intern_string(s: &str) -> JRef {
    INTERNED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(existing) = cache.get(s) {
            return existing.clone();
        }
        let r = new_string(s);
        cache.insert(Rc::from(s), r.clone());
        r
    })
}

/// True when `r` is a `java/lang/String` object.
pub fn is_string(r: &JRef) -> bool {
    &*r.borrow().jclass == "java/lang/String"
}

/// Extracts the character data of a String object (panics otherwise).
pub fn string_value(r: &JRef) -> Rc<String> {
    let b = r.borrow();
    match &b.slots[0] {
        Value::Str(s) => s.clone(),
        other => panic!(
            "j2r internal: object {} is not a String: {other:?}",
            b.jclass
        ),
    }
}

/// Builds a `String[]` for `main(String[] args)`.
pub fn new_string_array(items: &[String]) -> JRef {
    let mut slots = Vec::with_capacity(items.len() + 1);
    slots.push(Value::Int(items.len() as i32));
    for item in items {
        slots.push(Value::Ref(Some(new_string(item))));
    }
    JRef::new(RefCell::new(JObj {
        jclass: Rc::from("[Ljava/lang/String;"),
        slots,
    }))
}

/// `athrow`: pops the reference and turns it into a pending throw.
/// Throwing `null` raises a `NullPointerException`.
pub fn take_thrown(stack: &mut Vec<Value>) -> Result<JRef, Throw> {
    let v = stack.pop().expect("j2r internal: athrow on empty stack");
    match v {
        Value::Ref(Some(r)) => Ok(r),
        _ => Err(throw_new("java/lang/NullPointerException", None)),
    }
}

thread_local! {
    static OUT_REF: RefCell<Option<JRef>> = const { RefCell::new(None) };
}

fn native_stream(sink: i32) -> JRef {
    JRef::new(RefCell::new(JObj {
        jclass: Rc::from("java/io/PrintStream"),
        slots: vec![Value::Int(sink)],
    }))
}

/// `java/lang/System.out` (sink id 1). Lazily created, process-wide.
pub fn system_out_ref() -> JRef {
    OUT_REF.with(|r| {
        let mut r = r.borrow_mut();
        if r.is_none() {
            *r = Some(native_stream(1));
        }
        r.clone().expect("just initialized")
    })
}

/// `java/lang/System.err` (sink id 2).
pub fn system_err_ref() -> JRef {
    native_stream(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_returns_same_object() {
        let a = intern_string("hello-intern");
        let b = intern_string("hello-intern");
        assert!(Rc::ptr_eq(&a, &b));
        let c = intern_string("other");
        assert!(!Rc::ptr_eq(&a, &c));
    }

    #[test]
    fn string_roundtrip() {
        let r = new_string("héllo ✓");
        assert!(is_string(&r));
        assert_eq!(&*string_value(&r), "héllo ✓");
    }

    #[test]
    fn string_array_layout() {
        let arr = new_string_array(&["a".into(), "b".into()]);
        assert_eq!(&*arr.borrow().jclass, "[Ljava/lang/String;");
        assert_eq!(arr.borrow().slots[0].expect_int(), 2);
    }
}
