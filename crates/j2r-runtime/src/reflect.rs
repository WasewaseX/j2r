//! Reflection subset (M3_PLAN §8): `Class` objects, `Field.get/set` and
//! `Method.invoke` over compiled classes, with Temurin-exact failure
//! messages.
//!
//! Design (per plan §8.1):
//! * A `java/lang/Class` object is a heap object whose slot 0 carries the
//!   class's **internal name** (`a/b/C`, `[I`, `[Ljava/lang/String;`, or a
//!   bare primitive descriptor like `I`). Lazily built, interned per name
//!   in a permanent table and scanned as a root set, so `X.class == X.class`
//!   identity survives any number of collections.
//! * `Field`/`Method` objects are plain heap objects with a documented slot
//!   layout (see the `field_obj`/`method_obj` constructors, mirrored in
//!   builtin.rs).
//! * Field/method *tables* come from the generated crate through
//!   [`Dispatch::class_info`] — reflection only sees what the compiler
//!   proved exists. Builtin classes answer metadata queries from the
//!   builtin hierarchy tables; `getDeclaredField`/`getDeclaredMethod` on
//!   them are a loud internal error (outside the M3 subset, documented in
//!   M3_EVIDENCE.md).
//! * All reflective failures reuse the exact strings Temurin 21 printed
//!   during the oracle capture (M3_EVIDENCE.md wave-5 section).

use crate::desc::{parse_method_desc, TypeDesc};
use crate::heap::{with_obj, JRef};
use crate::internal_error;
use crate::{natives, Dispatch, Throw, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
/// Temurin's exact NPE message when a reflective accessor is handed a null
/// receiver for an instance member: the generated accessor's null check on
/// `obj.getClass()` produces this helpful-message NPE (oracle capture,
/// M3_EVIDENCE.md wave-5).
pub(crate) const NULL_RECV_NPE: &str =
    "Cannot invoke \"Object.getClass()\" because \"obj\" is null";
/// Per-name interned `java/lang/Class` objects — pinned forever via
/// [`scan_roots`] so `X.class` identity is stable across GC cycles.
/// Process-global since M4 Wave 1 (T04); `OnceLock` because `HashMap::new`
/// is not const. Allocation under its own lock is heap-lock law L2
/// (cache→heap nesting, never the reverse).
static CLASS_OBJECTS: OnceLock<Mutex<HashMap<Arc<str>, JRef>>> = OnceLock::new();
fn class_objects() -> &'static Mutex<HashMap<Arc<str>, JRef>> {
    CLASS_OBJECTS.get_or_init(|| Mutex::new(HashMap::new()))
}
/// Root scanner: the interned class objects are permanent roots (plan §8.1
/// "scanned as static roots"). Wired into `heap::collect` next to the
/// interned-string / boxed-cache scanners.
pub fn scan_roots(out: &mut Vec<u32>) {
    for r in class_objects()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
    {
        out.push(r.index());
    }
}
/// The internal class name carried by a `java/lang/Class` object (panics on
/// non-Class objects: a native dispatch bug, never a Java program error).
pub(crate) fn class_name_of(r: &JRef) -> Arc<String> {
    with_obj(*r, |o| match &o.slots[0] {
        Value::Str(s) => s.clone(),
        other => panic!(
            "j2r internal: object {} is not a Class: {other:?}",
            o.jclass
        ),
    })
}
/// Interns and returns the `Class` object for an internal class name.
pub fn class_ref(internal: &str) -> JRef {
    if let Some(r) = class_objects()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(internal)
        .copied()
    {
        return r;
    }
    let r = crate::heap::alloc(crate::object::JObj {
        jclass: Arc::from("java/lang/Class"),
        slots: vec![Value::Str(Arc::new(internal.to_string()))],
        prim: None,
    });
    let mut b = class_objects().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = b.get(internal) {
        return *existing;
    }
    b.insert(Arc::from(internal), r);
    r
}
/// `Class` object as a `Value` (the shape `ldc CONST_Class` pushes).
pub fn class_value(internal: &str) -> Value {
    Value::Ref(Some(class_ref(internal)))
}

// ---------------------------------------------------------------------------
// Name formatting (oracle-verified against Temurin 21, M3_EVIDENCE.md).
// ---------------------------------------------------------------------------
/// `boolean`…`void` display names for bare primitive descriptors.
pub(crate) fn prim_display(desc: &str) -> Option<&'static str> {
    Some(match desc {
        "Z" => "boolean",
        "B" => "byte",
        "S" => "short",
        "C" => "char",
        "I" => "int",
        "J" => "long",
        "F" => "float",
        "D" => "double",
        "V" => "void",
        _ => return None,
    })
}
/// `Class.getName()`: internal name → binary name. Primitives display as
/// keywords; arrays stay in descriptor form with only the `L…;` element
/// dotted (`[Ljava/lang/String;` → `[Ljava.lang.String;`, but `[I` stays
/// `[I` — oracle O.java).
pub fn binary_name(internal: &str) -> String {
    if let Some(p) = prim_display(internal) {
        return p.to_string();
    }
    if let Some(rest) = internal.strip_prefix('L').and_then(|s| s.strip_suffix(';')) {
        return rest.replace('/', ".");
    }
    internal.replace('/', ".")
}
/// The canonical name used in the `Enum.valueOf` miss message
/// (`getCanonicalName()`): nested member classes join with `.` all the way
/// down, so `EnumProbe$Color` prints as `EnumProbe.Color` — Temurin-pinned
/// by corpus/EnumProbe.java. Enum types are never anonymous or local (JLS
/// 8.9), so the canonical name always exists on this path.
pub fn canonical_name(internal: &str) -> String {
    binary_name(internal).replace('$', ".")
}
/// The dotted field-type name used in `Field` failure messages
/// (`getFieldType().getName()`): primitives as keywords, references and
/// arrays dotted through [`binary_name`].
pub(crate) fn java_type_name(desc: &str) -> String {
    binary_name(desc)
}
/// `Class.getSimpleName()`. Compiled classes use the `InnerClasses` name
/// exported in `UserClassInfo`; builtin and otherwise-unlisted classes fall
/// back to binary-name derivation (top-level suffix after `$`).
pub fn simple_name(internal: &str, ctx: Dispatch) -> String {
    if let Some(p) = prim_display(internal) {
        return p.to_string();
    }
    if let Some(rest) = internal.strip_prefix('[') {
        return format!("{}[]", simple_name(rest, ctx));
    }
    if let Some(rest) = internal.strip_prefix('L').and_then(|s| s.strip_suffix(';')) {
        return simple_name(rest, ctx);
    }
    if let Some(info) = ctx.class_info(internal) {
        if let Some(name) = info.simple_name {
            return name.to_string();
        }
    }
    // Derivation fallback: last path segment, then last `$` segment when
    // that suffix is not anonymous-looking (anonymous classes arrive here
    // as `Some("")` from the attribute, not through this branch).
    let binary = internal.replace('/', ".");
    let after_dot = binary.rsplit('.').next().unwrap_or(&binary);
    match after_dot.rsplit_once('$') {
        Some((_, suffix)) if !suffix.is_empty() && !suffix.bytes().all(|b| b.is_ascii_digit()) => {
            suffix.to_string()
        }
        _ => after_dot.to_string(),
    }
}

// --- scanaislop split: the items below live verbatim in the child modules ---
mod class;
mod invoke;
mod members;
#[cfg(test)]
mod tests;
pub(crate) use class::*;
pub use invoke::ctor_new_instance_caller;
pub(crate) use invoke::*;
pub(crate) use members::*;
