//! Heap object model.
//!
//! Every Java object (including strings, boxed values, arrays and native
//! JDK objects) is a [`JObj`] living in the arena heap ([`crate::heap`]),
//! addressed by generation-tagged `Copy` handles ([`JRef`], defined in
//! `crate::heap`). Instance fields live in flattened `slots`, one slot per
//! field regardless of the field's JVM width; the code generator assigns
//! absolute slot indices across whole class hierarchies, so field access is
//! a direct index.
//!
//! Object access goes through `crate::heap::with_obj` / `with_obj_mut` —
//! the arena is a process-global `Mutex` (M4 Wave 1), so a `Ref` must
//! never escape the lock guard (M2_PLAN §4.7; heap lock law L1–L3).

use crate::heap::{alloc, with_obj, with_obj_mut, JRef};
use crate::Value;
use crate::{throw_new, Throw};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, PartialEq)]
pub struct JObj {
    /// Runtime class in internal form (`a/b/C`); arrays use descriptors
    /// like `[I` or `[Ljava/lang/String;`.
    pub jclass: Arc<str>,
    /// Flattened field slots. Slot semantics are owned by the code
    /// generator's layout; natives use fixed slots documented per class.
    pub slots: Vec<Value>,
}

/// Allocates a new object with `slots` default (Void) slots.
pub fn new_obj(class: &str, slots: usize) -> Result<JRef, Throw> {
    Ok(alloc(JObj {
        jclass: Arc::from(class),
        slots: vec![Value::Void; slots],
    }))
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
    with_obj(*r, |o| o.slots[idx].clone())
}

/// Writes a field slot.
pub fn obj_set(r: &JRef, idx: usize, v: Value) {
    with_obj_mut(*r, |o| o.slots[idx] = v);
}

/// Fills the user-owned slot region with JVM default values (JLS §4.12.5:
/// numeric fields are 0, booleans false, references null) — the same
/// defaults a real JVM writes between allocation and the constructor body.
///
/// The spec string covers the LAST `spec.len()` slots (the user-owned
/// region above the builtin-parent base): one kind char per slot —
/// `i`/`j`/`f`/`d` for the numeric families, `r` for references. A bad
/// char is a codegen bug and panics loudly, like every other internal
/// invariant break. Generated `new` calls this right after allocation;
/// nothing else needs it (runtime-internal allocations own every slot
/// they read).
pub fn set_defaults(r: &JRef, spec: &str) {
    with_obj_mut(*r, |o| {
        let base = o.slots.len().saturating_sub(spec.len());
        for (i, c) in spec.bytes().enumerate() {
            o.slots[base + i] = match c {
                b'i' => Value::Int(0),
                b'j' => Value::Long(0),
                b'f' => Value::Float(0.0),
                b'd' => Value::Double(0.0),
                b'r' => Value::Ref(None),
                other => panic!(
                    "j2r internal: bad default spec char {other:?} for {}",
                    o.jclass
                ),
            };
        }
    });
}

/// Creates a `java/lang/String` object.
pub fn new_string(s: &str) -> JRef {
    alloc(JObj {
        jclass: Arc::from("java/lang/String"),
        slots: vec![Value::Str(Arc::new(s.to_string()))],
    })
}

/// Creates a `java/lang/String` object sharing an existing character buffer
/// (used by `StringBuilder.toString`, which is safe because `StringBuilder`
/// mutations copy-on-write through `Arc::make_mut`).
pub fn string_from_rc(s: Arc<String>) -> JRef {
    alloc(JObj {
        jclass: Arc::from("java/lang/String"),
        slots: vec![Value::Str(s)],
    })
}

/// The string-intern cache (JLS constant-pool identity: two `ldc`s of the
/// same literal yield the same reference). Process-global since M4 Wave 1
/// (T04). `OnceLock` because `HashMap::new` is not const. May allocate
/// under its own lock — heap-lock law L2 allows cache→heap nesting, never
/// the reverse.
static INTERNED: OnceLock<Mutex<HashMap<Arc<str>, JRef>>> = OnceLock::new();

fn interned() -> &'static Mutex<HashMap<Arc<str>, JRef>> {
    INTERNED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Interns a string exactly like the JVM constant pool does: two `ldc`s of
/// the same literal yield the same reference (`==` identity).
pub fn intern_string(s: &str) -> JRef {
    let mut cache = interned().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = cache.get(s) {
        return *existing;
    }
    let r = new_string(s);
    cache.insert(Arc::from(s), r);
    r
}

/// True when `r` is a `java/lang/String` object.
pub fn is_string(r: &JRef) -> bool {
    with_obj(*r, |o| &*o.jclass == "java/lang/String")
}

/// Extracts the character data of a String object (panics otherwise).
pub fn string_value(r: &JRef) -> Arc<String> {
    with_obj(*r, |b| match &b.slots[0] {
        Value::Str(s) => s.clone(),
        other => panic!(
            "j2r internal: object {} is not a String: {other:?}",
            b.jclass
        ),
    })
}

/// Builds a `String[]` for `main(String[] args)`.
pub fn new_string_array(items: &[String]) -> JRef {
    let mut slots = Vec::with_capacity(items.len() + 1);
    slots.push(Value::Int(items.len() as i32));
    for item in items {
        slots.push(Value::Ref(Some(new_string(item))));
    }
    alloc(JObj {
        jclass: Arc::from("[Ljava/lang/String;"),
        slots,
    })
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

/// The lazily-created `System.out` object (permanent root; M4 Wave 1, T04).
static OUT_REF: Mutex<Option<JRef>> = Mutex::new(None);

fn native_stream(sink: i32) -> JRef {
    alloc(JObj {
        jclass: Arc::from("java/io/PrintStream"),
        slots: vec![Value::Int(sink)],
    })
}

/// `java/lang/System.out` (sink id 1). Lazily created, process-wide.
pub fn system_out_ref() -> JRef {
    let mut r = OUT_REF.lock().unwrap_or_else(|e| e.into_inner());
    if r.is_none() {
        *r = Some(native_stream(1));
    }
    r.expect("just initialized")
}

/// `java/lang/System.err` (sink id 2).
pub fn system_err_ref() -> JRef {
    native_stream(2)
}

/// Collector hook: pushes the indices of every object held by the runtime's
/// own permanent structures (intern cache, `System.out`) into the root
/// vector. Interned literals and the shared stdout stream are permanent
/// roots, matching the JVM's constant-pool lifetime.
pub fn scan_runtime_roots(out: &mut Vec<u32>) {
    for r in interned()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
    {
        out.push(r.index());
    }
    if let Some(x) = OUT_REF.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        out.push(x.index());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::{with_obj, with_obj_mut};

    #[test]
    fn intern_returns_same_object() {
        let _rt = crate::test_support::runtime();
        let a = intern_string("hello-intern");
        let b = intern_string("hello-intern");
        assert!(JRef::ptr_eq(&a, &b));
        let c = intern_string("other");
        assert!(!JRef::ptr_eq(&a, &c));
    }

    #[test]
    fn string_roundtrip() {
        let _rt = crate::test_support::runtime();
        let r = new_string("héllo ✓");
        assert!(is_string(&r));
        assert_eq!(&*string_value(&r), "héllo ✓");
    }

    #[test]
    fn string_array_layout() {
        let _rt = crate::test_support::runtime();
        let arr = new_string_array(&["a".into(), "b".into()]);
        let cls = with_obj(arr, |o| o.jclass.to_string());
        assert_eq!(cls, "[Ljava/lang/String;");
        assert_eq!(with_obj(arr, |o| o.slots[0].clone()).expect_int(), 2);
    }

    #[test]
    fn obj_mut_writes_are_visible() {
        let _rt = crate::test_support::runtime();
        let r = new_obj("test/M", 2).unwrap();
        obj_set(&r, 0, Value::Int(7));
        with_obj_mut(r, |o| o.slots[1] = Value::Int(9));
        assert_eq!(obj_get(&r, 0).expect_int(), 7);
        assert_eq!(obj_get(&r, 1).expect_int(), 9);
    }
}
