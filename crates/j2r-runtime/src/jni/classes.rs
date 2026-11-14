//! T10 slice 3 — the class/field surface of the JNI bridge (design of
//! record: `docs/M5_JNI_CLASSES.md`). ALL the semantics live here, in
//! safe Rust, testable without FFI; the generated slot wrappers in
//! [`crate::jni::tables`] are thin — null guards, tag/pointer
//! conversions, and the per-slot C narrowing/casting.
//!
//! The two representation decisions (M5_JNI_CLASSES.md §2/§3):
//!
//! * `jclass` IS a real `jobject` — the tag of the interned
//!   `java/lang/Class` mirror object ([`crate::reflect::class_ref`]);
//!   the internal class name rides in the mirror's `slots[0]` and is
//!   recovered with [`crate::reflect::class_name_of`] (loud on a
//!   non-Class object — a foreign-side bug, never silent).
//! * `jfieldID` is a tag into the field-ID table below, marked by bit 63
//!   so it can never be confused with a handle-table `jobject` tag. IDs
//!   are deduplicated (the same `(owner, name, desc, static?)` triple
//!   returns the same ID forever) and permanent — the JNI law: field IDs
//!   do not move, and v1 never frees them.

use crate::jni::bridge;
use crate::Value;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

// ---------------------------------------------------------------------------
// The field-ID table (M5_JNI_CLASSES.md §3)
// ---------------------------------------------------------------------------

/// Bit 63 marks a field-ID tag. Handle-table `jobject` tags are
/// `(generation << 32) | (slot + 1)` — they can reach bit 63 only after
/// ~2³² generations, and even then [`bridge::resolve`] would loudly miss
/// (the generation-pin law). The marker keeps the two spaces structurally
/// disjoint for every realistic program.
const FIELD_ID_MARK: u64 = 1 << 63;

/// One resolved field fact — everything an accessor needs.
#[derive(Debug, Clone)]
pub struct FieldId {
    /// Internal name of the DECLARING class (the walk's owner at mint
    /// time — an inherited field is owned by its declaring class, and the
    /// instance slot is absolute in the flattened layout).
    pub owner: String,
    pub name: String,
    /// Field descriptor (`"I"`, `"Ljava/lang/String;"`, `"[J"`).
    pub desc: String,
    pub is_static: bool,
    /// Absolute instance slot (meaningful when `!is_static`).
    pub instance_slot: usize,
    /// Static-pool slot (meaningful when `is_static`).
    pub static_slot: u32,
}

#[derive(Default)]
struct Registry {
    seq: u64,
    by_triple: HashMap<(String, String, String, bool), u64>,
    by_id: HashMap<u64, FieldId>,
}

static FIELD_IDS: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::default()));

/// Mints (or returns the existing) field ID for the triple. IDs are
/// permanent: no free, no reuse — matching the JNI oracle's semantics.
fn mint_field_id(owner: &str, name: &str, desc: &str, f: &crate::UserField) -> u64 {
    let mut reg = FIELD_IDS.lock().unwrap_or_else(|e| e.into_inner());
    let key = (
        owner.to_string(),
        name.to_string(),
        desc.to_string(),
        f.is_static,
    );
    if let Some(id) = reg.by_triple.get(&key) {
        return *id;
    }
    let id = FIELD_ID_MARK | reg.seq;
    reg.seq += 1;
    reg.by_triple.insert(key, id);
    reg.by_id.insert(
        id,
        FieldId {
            owner: owner.to_string(),
            name: name.to_string(),
            desc: desc.to_string(),
            is_static: f.is_static,
            instance_slot: f.instance_slot,
            static_slot: f.static_slot,
        },
    );
    id
}

/// The tag behind a `jfieldID` pointer (the safe int→ptr law: the
/// foreign side never dereferences our tags).
pub fn field_id_from_ptr(p: crate::jni::tables::jfieldID) -> u64 {
    p as usize as u64
}

/// The `jfieldID` pointer for a minted tag.
pub fn field_id_to_ptr(tag: u64) -> crate::jni::tables::jfieldID {
    tag as usize as *mut std::os::raw::c_void
}

/// Looks the ID up — a foreign or corrupted pointer is a LOUD registry
/// miss (the T09 law): across the `extern "system"` boundary the panic
/// aborts the process (pinned by the slice-2 child-process test).
pub fn resolve_field_id(id: u64) -> FieldId {
    if id & FIELD_ID_MARK == 0 {
        panic!(
            "j2r JNI bridge: jfieldID 0x{id:x} lacks the field-ID marker — \
             not a pointer this bridge minted"
        );
    }
    FIELD_IDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .by_id
        .get(&id)
        .cloned()
        .unwrap_or_else(|| {
            panic!("j2r JNI bridge: jfieldID 0x{id:x} is not registered — foreign-side bug")
        })
}

// ---------------------------------------------------------------------------
// jclass — FindClass / GetObjectClass (M5_JNI_CLASSES.md §2)
// ---------------------------------------------------------------------------

/// The bare primitive descriptors a field/array descriptor can end in.
fn is_prim_desc(s: &str) -> bool {
    matches!(s, "I" | "J" | "F" | "D" | "Z" | "B" | "C" | "S")
}

/// The element name of an array descriptor (`"[I"` → `"I"`,
/// `"[Ljava/lang/String;"` → `"java/lang/String"`), `None` when the
/// shape is not an array descriptor.
fn array_element_name(s: &str) -> Option<&str> {
    let rest = s.strip_prefix('[')?;
    Some(match rest.strip_prefix('L') {
        // the L-form carries the trailing `;` — strip it
        Some(cls) => cls.strip_suffix(';').unwrap_or(cls),
        None => rest,
    })
}

/// Does `name` (JNI form: internal `a/b/C`, array descriptor, or a bare
/// primitive after array stripping) name a class this runtime can
/// serve? Compiled classes via the host dispatch, builtins via the
/// builtin tables, plus the two structural classes the runtime always
/// carries (`java/lang/Object`, `java/lang/Class`).
fn class_name_exists(name: &str) -> bool {
    if let Some(elem) = array_element_name(name) {
        return class_name_exists(elem);
    }
    if is_prim_desc(name) {
        return true;
    }
    if matches!(name, "java/lang/Object" | "java/lang/Class") {
        return true;
    }
    let ctx = crate::jni::host_dispatch();
    ctx.class_info(name).is_some()
        || crate::builtin::builtin_total_slots(name).is_some()
        || crate::builtin::builtin_is_interface(name)
        || crate::builtin::builtin_parent_of(name).is_some()
}

/// `FindClass` body: the class must EXIST (a miss returns JNI null — the
/// spec's `NoClassDefFoundError` shape, v1 gap, T13-registered) BEFORE
/// the mirror is interned: no garbage names ever enter the Class table.
/// Bare primitive names (`"int"`) are not classes in JNI → null.
pub fn slot_find_class(name: &str) -> Option<u64> {
    if !class_name_exists(name) {
        return None;
    }
    Some(bridge::new_local(crate::reflect::class_ref(name)))
}

/// `GetObjectClass` body: null → JNI null (v1 gap, T13-registered);
/// otherwise the object's own runtime class — arrays included, since an
/// array `JObj`'s `jclass` IS its descriptor (`"[I"`).
pub fn slot_get_object_class(tag: u64) -> Option<u64> {
    if tag == 0 {
        return None;
    }
    let r = bridge::resolve(tag);
    let name = crate::heap::with_obj(r, |o| o.jclass.to_string());
    Some(bridge::new_local(crate::reflect::class_ref(&name)))
}

/// The internal class name behind a `jclass` tag; `None` for JNI null.
/// The boundary guard is on the OBJECT'S OWN CLASS — a `Class` mirror
/// carries `jclass == "java/lang/Class"` — because the name slot alone
/// cannot tell a mirror from a Java `String` (both hold a `Value::Str`).
/// Anything else is a foreign-side bug: loud abort, never silent.
/// Recovers the internal class name behind a `jclass` tag (the interned
/// Class mirror's law, M5_JNI_CLASSES.md §2). `None` only for JNI null;
/// a non-Class object PANICS inside (`class_name_of`'s loud law — a
/// foreign-side bug). Shared with `jni::calls` (the static/nonvirtual
/// families resolve their declared class through it).
pub(super) fn class_name_from_tag(tag: u64) -> Option<String> {
    if tag == 0 {
        return None;
    }
    let r = bridge::resolve(tag);
    let is_class = crate::heap::with_obj(r, |o| o.jclass.as_ref() == "java/lang/Class");
    if !is_class {
        panic!("j2r JNI bridge: a jclass argument was not a Class object — foreign-side bug");
    }
    Some(crate::reflect::class_name_of(&r).to_string())
}

// ---------------------------------------------------------------------------
// Field-ID resolution — the inherited-field walk (M5_JNI_CLASSES.md §3)
// ---------------------------------------------------------------------------

/// `GetFieldID`/`GetStaticFieldID` body: resolve `(owner, name, sig)` by
/// walking the owner, then its superclass chain (JNI searches INHERITED
/// fields, unlike `Class.getDeclaredField`), first match wins; the
/// field's own `is_static` must agree with the accessor family asked
/// for. Miss (unknown class, unknown name/desc, or only the OTHER kind)
/// → `None` → the wrapper returns JNI null (the spec's
/// `NoSuchFieldError` shape — v1 gap, T13-registered). Builtin classes
/// have no field facts in v1 → null (registered gap).
pub fn slot_get_field_id(class_tag: u64, name: &str, sig: &str, want_static: bool) -> Option<u64> {
    let owner = class_name_from_tag(class_tag)?;
    let ctx = crate::jni::host_dispatch();
    let mut cur = Some(owner);
    while let Some(c) = cur {
        if let Some(info) = ctx.class_info(&c) {
            if let Some(f) = info
                .fields
                .iter()
                .find(|f| f.name == name && f.desc == sig && f.is_static == want_static)
            {
                return Some(mint_field_id(&c, name, sig, f));
            }
        }
        cur = ctx.parent_of(&c).map(|s| s.to_string());
    }
    None
}

// ---------------------------------------------------------------------------
// The accessors (M5_JNI_CLASSES.md §4)
// ---------------------------------------------------------------------------

/// A wrong-kind ID passed to an accessor is a foreign-side bug (Temurin:
/// UB) — here it is a loud abort with the offending fact named.
fn kind_guard(f: &FieldId, want_static: bool, slot: &str) {
    if f.is_static != want_static {
        panic!(
            "j2r JNI bridge: {slot} given a {} field ID for {}::{} — foreign-side bug",
            if f.is_static { "static" } else { "instance" },
            f.owner,
            f.name
        );
    }
}

/// A receiver whose class is not a subtype of the ID's declaring class
/// is a foreign-side bug (Temurin: UB) — loud abort, the
/// `reflect::field_set` precedent hardened across the FFI boundary.
fn receiver_guard(r: crate::heap::JRef, f: &FieldId, slot: &str) {
    let ctx = crate::jni::host_dispatch();
    let jc = crate::heap::with_obj(r, |o| o.jclass.to_string());
    if !ctx.is_subtype(&jc, &f.owner) {
        panic!(
            "j2r JNI bridge: {slot} receiver of class {jc} is not a subtype of {} \
             — foreign-side bug",
            f.owner
        );
    }
}

/// The value shape a field descriptor demands (the int-group law:
/// `Z/B/C/S` ride `Value::Int`). A mismatch here is OUR bug (the
/// wrapper's narrowing), so it is an internal error, not a loud foreign
/// miss.
fn shape_guard(desc: &str, v: &Value) {
    let ok = match v {
        Value::Ref(_) => desc.starts_with('L') || desc.starts_with('['),
        Value::Long(_) => desc == "J",
        Value::Float(_) => desc == "F",
        Value::Double(_) => desc == "D",
        Value::Int(_) => matches!(desc, "I" | "Z" | "B" | "C" | "S"),
        _ => false,
    };
    if !ok {
        crate::internal_error(&format!(
            "j2r internal: a field accessor wrapper cooked {v:?} for a \"{desc}\" field"
        ));
    }
}

/// `Get<prim>Field` body: `None` = JNI null receiver (v1 gap,
/// T13-registered); `Some(value)` is RAW (the wrapper narrows by its own
/// slot descriptor).
pub fn slot_get_instance_field(obj_tag: u64, fid: u64) -> Option<Value> {
    if obj_tag == 0 {
        return None;
    }
    let f = resolve_field_id(fid);
    kind_guard(&f, false, "an instance accessor");
    let r = bridge::resolve(obj_tag);
    receiver_guard(r, &f, "an instance accessor");
    Some(crate::object::obj_get(&r, f.instance_slot))
}

/// `Set<prim>Field` body: null receiver → no-op (v1 gap); the wrapper
/// already cooked the C argument into the descriptor's `Value` shape —
/// re-asserted here, then stored.
pub fn slot_set_instance_field(obj_tag: u64, fid: u64, v: Value) {
    if obj_tag == 0 {
        return;
    }
    let f = resolve_field_id(fid);
    kind_guard(&f, false, "an instance accessor");
    shape_guard(&f.desc, &v);
    let r = bridge::resolve(obj_tag);
    receiver_guard(r, &f, "an instance accessor");
    crate::object::obj_set(&r, f.instance_slot, v);
}

/// `GetStatic<prim>Field` body: the ID is the sole authority (the jclass
/// argument is not re-validated in v1 — registered note,
/// M5_JNI_CLASSES.md §4); the static pool comes from the host dispatch.
pub fn slot_get_static_field(fid: u64) -> Option<Value> {
    let f = resolve_field_id(fid);
    kind_guard(&f, true, "a static accessor");
    let ctx = crate::jni::host_dispatch();
    let get = ctx.static_get.unwrap_or_else(|| {
        panic!(
            "j2r JNI bridge: static read of {}::{} before the host dispatch \
             was installed",
            f.owner, f.name
        )
    });
    Some(get(f.static_slot))
}

/// `SetStatic<prim>Field` body.
pub fn slot_set_static_field(fid: u64, v: Value) {
    let f = resolve_field_id(fid);
    kind_guard(&f, true, "a static accessor");
    shape_guard(&f.desc, &v);
    let ctx = crate::jni::host_dispatch();
    let set = ctx.static_set.unwrap_or_else(|| {
        panic!(
            "j2r JNI bridge: static write of {}::{} before the host dispatch \
             was installed",
            f.owner, f.name
        )
    });
    set(f.static_slot, &v);
}

/// The generated wrappers call this on a `Some(value)` whose shape does
/// not match the slot's own descriptor — a foreign-side bug (Temurin:
/// UB), here a loud abort naming the slot and the value.
pub fn loud_field_mismatch(slot: &str, v: &Value) -> ! {
    panic!(
        "j2r JNI bridge: {slot} received a value of the wrong shape ({v:?}) \
         for its slot descriptor — foreign-side bug"
    )
}

// ---------------------------------------------------------------------------
// The method-ID table (T11 slice 11b, design of record: M5_JNI_CALLS.md §3/§4)
// ---------------------------------------------------------------------------

/// Bit 62 marks a method-ID tag — disjoint from the field-ID mark
/// (bit 63), from handle-table `jobject` tags, and from each other for
/// every realistic program (same argument as the field-ID mark).
const METHOD_ID_MARK: u64 = 1 << 62;

/// One resolved method fact — everything the Call*MethodA core needs.
#[derive(Debug, Clone)]
pub struct MethodId {
    /// Internal name of the class the walk FOUND the method in (the
    /// GetMethodID search walks owner + ancestors, JNI-faithful).
    pub owner: String,
    pub name: String,
    /// Method descriptor `"(II)V"` — arity and kinds ride it.
    pub sig: String,
    pub is_static: bool,
}

#[derive(Default)]
struct MethodRegistry {
    seq: u64,
    by_triple: HashMap<(String, String, String, bool), u64>,
    by_id: HashMap<u64, MethodId>,
}

static METHOD_IDS: LazyLock<Mutex<MethodRegistry>> =
    LazyLock::new(|| Mutex::new(MethodRegistry::default()));

fn mint_method_id(owner: &str, name: &str, sig: &str, is_static: bool) -> u64 {
    let mut reg = METHOD_IDS.lock().unwrap_or_else(|e| e.into_inner());
    let key = (
        owner.to_string(),
        name.to_string(),
        sig.to_string(),
        is_static,
    );
    if let Some(id) = reg.by_triple.get(&key) {
        return *id;
    }
    let id = METHOD_ID_MARK | reg.seq;
    reg.seq += 1;
    reg.by_triple.insert(key.clone(), id);
    reg.by_id.insert(
        id,
        MethodId {
            owner: owner.to_string(),
            name: name.to_string(),
            sig: sig.to_string(),
            is_static,
        },
    );
    id
}

/// Test-only: mint through the same registry the slot fns use.
#[cfg(test)]
pub fn mint_for_test(owner: &str, name: &str, sig: &str, is_static: bool) -> u64 {
    mint_method_id(owner, name, sig, is_static)
}

/// The tag behind a `jmethodID` pointer (the safe int→ptr law).
pub fn method_id_from_ptr(p: crate::jni::tables::jmethodID) -> u64 {
    p as usize as u64
}

/// The `jmethodID` pointer for a minted tag.
pub fn method_id_to_ptr(tag: u64) -> crate::jni::tables::jmethodID {
    tag as usize as *mut std::os::raw::c_void
}

/// Looks the ID up — a foreign or corrupted pointer is a LOUD registry
/// miss (the T09 law); a FIELD id passed where a METHOD id belongs is
/// loud too (the marker disambiguates the two spaces by construction).
pub fn resolve_method_id(id: u64) -> MethodId {
    if id & METHOD_ID_MARK == 0 {
        panic!(
            "j2r JNI bridge: jmethodID 0x{id:x} lacks the method-ID marker — \
             not a pointer this bridge minted"
        );
    }
    METHOD_IDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .by_id
        .get(&id)
        .cloned()
        .unwrap_or_else(|| {
            panic!("j2r JNI bridge: jmethodID 0x{id:x} is not registered — foreign-side bug")
        })
}

/// `GetMethodID` (slot 33) / `GetStaticMethodID` (slot 113) body: walk
/// the class and its ancestors for a declared method with the exact
/// name+descriptor and the requested staticness (the JNI search law);
/// mint the permanent, deduplicated ID. `None` = not found — the wrapper
/// returns JNI null with a pending `NoSuchMethodError` (the oracle's
/// shape, now that the pending-exception cell exists). Builtin classes
/// have no compiled method table — v1 gap (T12 RegisterNatives owns
/// native bindings; registered in M5_JNI_CALLS.md §4).
pub fn slot_get_method_id(class_tag: u64, name: &str, sig: &str, want_static: bool) -> Option<u64> {
    let owner = class_name_from_tag(class_tag)?;
    let ctx = crate::jni::host_dispatch();
    let mut cur = Some(owner);
    while let Some(c) = cur {
        if let Some(info) = ctx.class_info(&c) {
            if info
                .methods
                .iter()
                .any(|mm| mm.name == name && mm.desc == sig && mm.is_static == want_static)
            {
                return Some(mint_method_id(&c, name, sig, want_static));
            }
        }
        cur = ctx.parent_of(&c).map(|s| s.to_string());
    }
    // Not found (including builtin classes — no compiled method table):
    // the oracle pends NoSuchMethodError and answers null (T11 11b —
    // the first T10-era NPE-shaped gap to migrate to the faithful shape).
    crate::jni::bridge::pend_exception(crate::throw_new(
        "java/lang/NoSuchMethodError",
        Some(&format!("{name} {sig}")),
    ));
    None
}

// ---------------------------------------------------------------------------
// Tests — safe, no FFI: the wrappers are thin casts over these bodies.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UserClassInfo;
    use std::sync::Mutex as StdMutex;

    // -- the two-class fixture (flattened instance slots: Base owns 0,
    //    Derived adds 1) ----------------------------------------------
    static BASE_FIELDS: [crate::UserField; 2] = [
        crate::UserField {
            name: "s",
            desc: "Ljava/lang/String;",
            is_static: false,
            instance_slot: 0,
            static_slot: 0,
            modifiers: 0,
        },
        crate::UserField {
            name: "counter",
            desc: "I",
            is_static: true,
            instance_slot: 0,
            static_slot: 3,
            modifiers: 0,
        },
    ];
    static DERIVED_FIELDS: [crate::UserField; 1] = [crate::UserField {
        name: "extra",
        desc: "J",
        is_static: false,
        instance_slot: 1,
        static_slot: 0,
        modifiers: 0,
    }];
    static DERIVED_PARENTS: [&str; 1] = ["j2r/test/JniBase"];

    fn meta_of(c: &str) -> Option<&'static UserClassInfo> {
        static BASE_INFO: UserClassInfo = UserClassInfo {
            id: 0,
            clinit: None,
            is_abstract: false,
            total_slots: 0,
            defaults_spec: "",
            is_interface: false,
            simple_name: Some("JniBase"),
            fields: &BASE_FIELDS,
            methods: &[],
            ctors: &[],
        };
        static DERIVED_INFO: UserClassInfo = UserClassInfo {
            id: 0,
            clinit: None,
            is_abstract: false,
            total_slots: 0,
            defaults_spec: "",
            is_interface: false,
            simple_name: Some("JniDerived"),
            fields: &DERIVED_FIELDS,
            methods: &[],
            ctors: &[],
        };
        match c {
            "j2r/test/JniBase" => Some(&BASE_INFO),
            "j2r/test/JniDerived" => Some(&DERIVED_INFO),
            _ => None,
        }
    }

    fn parents_of(c: &str) -> Option<&'static [&'static str]> {
        if c == "j2r/test/JniDerived" {
            Some(&DERIVED_PARENTS)
        } else {
            None
        }
    }

    static POOL: StdMutex<Vec<Value>> = StdMutex::new(Vec::new());

    fn pool_get(idx: u32) -> Value {
        POOL.lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(idx as usize)
            .cloned()
            .unwrap_or(Value::Int(0))
    }

    fn pool_set(idx: u32, v: &Value) {
        let mut p = POOL.lock().unwrap_or_else(|e| e.into_inner());
        while p.len() <= idx as usize {
            p.push(Value::Int(0));
        }
        p[idx as usize] = v.clone();
    }

    fn install_fixture() {
        crate::jni::install_host_dispatch(crate::Dispatch {
            lookup: None,
            parents: Some(parents_of),
            meta: Some(meta_of),
            static_get: Some(pool_get),
            static_set: Some(pool_set),
        });
    }

    fn class_tag(internal: &str) -> u64 {
        bridge::new_local(crate::reflect::class_ref(internal))
    }

    #[test]
    fn find_class_validates_existence_and_interns() {
        let _rt = crate::test_support::runtime();
        install_fixture();
        // compiled + builtin + structural + array + primitive-descriptor
        assert!(slot_find_class("j2r/test/JniBase").is_some());
        assert!(slot_find_class("java/lang/String").is_some());
        assert!(slot_find_class("java/lang/Object").is_some());
        assert!(slot_find_class("java/lang/Class").is_some());
        assert!(slot_find_class("[I").is_some());
        assert!(slot_find_class("[Ljava/lang/String;").is_some());
        // misses: unknown, malformed arrays, bare primitive names
        assert!(slot_find_class("no/such/Class").is_none());
        assert!(slot_find_class("[Lno/such/Class;").is_none());
        assert!(slot_find_class("[;").is_none());
        assert!(slot_find_class("int").is_none());
        assert!(slot_find_class("").is_none());
        // interning: two finds return the SAME Class mirror object
        let a = slot_find_class("j2r/test/JniBase").unwrap();
        let b = slot_find_class("j2r/test/JniBase").unwrap();
        let ra = bridge::resolve(a);
        let rb = bridge::resolve(b);
        assert!(crate::heap::JRef::ptr_eq(&ra, &rb));
    }

    #[test]
    fn get_object_class_reads_the_runtime_class_including_arrays() {
        let _rt = crate::test_support::runtime();
        install_fixture();
        let obj = bridge::new_local(crate::heap::alloc(crate::object::JObj {
            jclass: std::sync::Arc::from("j2r/test/JniDerived"),
            slots: vec![Value::Ref(None), Value::Long(7)],
            prim: None,
        }));
        let cls = slot_get_object_class(obj).unwrap();
        let name = class_name_from_tag(cls).unwrap();
        assert_eq!(name, "j2r/test/JniDerived");
        // an array object's class IS its descriptor
        let arr = crate::array::array_new_prim("[I", 3)
            .expect("fixture array with a valid length always allocates");
        let arr_cls = slot_get_object_class(bridge::new_local(arr)).unwrap();
        assert_eq!(class_name_from_tag(arr_cls).unwrap(), "[I");
        // null → null (v1 gap)
        assert!(slot_get_object_class(0).is_none());
    }

    #[test]
    fn field_id_walks_superclasses_and_dedups() {
        let _rt = crate::test_support::runtime();
        install_fixture();
        let derived = class_tag("j2r/test/JniDerived");
        let base = class_tag("j2r/test/JniBase");
        // "s" is declared on Base, resolved THROUGH Derived
        let via_derived = slot_get_field_id(derived, "s", "Ljava/lang/String;", false).unwrap();
        let via_base = slot_get_field_id(base, "s", "Ljava/lang/String;", false).unwrap();
        assert_eq!(via_derived, via_base, "same triple → same ID");
        let again = slot_get_field_id(derived, "s", "Ljava/lang/String;", false).unwrap();
        assert_eq!(again, via_derived, "dedup: the registry returns the ID");
        let f = resolve_field_id(via_derived);
        assert_eq!(f.owner, "j2r/test/JniBase");
        assert_eq!(f.instance_slot, 0);
        // kind mismatch → null (GetStaticFieldID for an instance field)
        assert!(slot_get_field_id(base, "s", "Ljava/lang/String;", true).is_none());
        // wrong desc → null
        assert!(slot_get_field_id(base, "s", "I", false).is_none());
        // unknown field → null
        assert!(slot_get_field_id(base, "nope", "I", false).is_none());
        // null jclass → null
        assert!(slot_get_field_id(0, "s", "Ljava/lang/String;", false).is_none());
        // static field of the derived class resolves to Base's pool slot
        let stat = slot_get_field_id(derived, "counter", "I", true).unwrap();
        let sf = resolve_field_id(stat);
        assert_eq!(sf.owner, "j2r/test/JniBase");
        assert_eq!(sf.static_slot, 3);
        // the ID is marked — a jobject tag can never be a valid field ID
        assert_eq!(via_derived & FIELD_ID_MARK, FIELD_ID_MARK);
    }

    #[test]
    fn foreign_field_id_is_loud() {
        let _rt = crate::test_support::runtime();
        // an unmarked tag is rejected before the registry lookup
        let result = std::panic::catch_unwind(|| resolve_field_id(0x1234));
        assert!(result.is_err(), "unmarked tag must panic");
        // a marked but never-minted tag is a loud miss too
        let result = std::panic::catch_unwind(|| resolve_field_id(FIELD_ID_MARK | 999));
        assert!(result.is_err(), "unminted tag must panic");
    }

    #[test]
    fn instance_accessors_read_and_write_through_slots() {
        let _rt = crate::test_support::runtime();
        install_fixture();
        let derived = class_tag("j2r/test/JniDerived");
        let s_id = slot_get_field_id(derived, "s", "Ljava/lang/String;", false).unwrap();
        let obj = crate::heap::alloc(crate::object::JObj {
            jclass: std::sync::Arc::from("j2r/test/JniDerived"),
            slots: vec![Value::Ref(None), Value::Long(0)],
            prim: None,
        });
        let obj_tag = bridge::new_local(obj);
        let extra_id = slot_get_field_id(derived, "extra", "J", false).unwrap();
        // write via the JNI body, read back through the heap
        slot_set_instance_field(obj_tag, extra_id, Value::Long(0x0102_0304_0506_0708));
        match slot_get_instance_field(obj_tag, extra_id) {
            Some(Value::Long(l)) => assert_eq!(l, 0x0102_0304_0506_0708),
            other => panic!("expected Long, got {other:?}"),
        }
        // an object field: store a String mirror JRef, read it back
        let str_obj = crate::object::new_string("hello jni");
        slot_set_instance_field(obj_tag, s_id, Value::Ref(Some(str_obj)));
        match slot_get_instance_field(obj_tag, s_id) {
            Some(Value::Ref(Some(r))) => {
                assert!(crate::heap::JRef::ptr_eq(&r, &str_obj));
            }
            other => panic!("expected the stored ref, got {other:?}"),
        }
        // null receiver: get → None, set → no-op (v1 gap law)
        assert!(slot_get_instance_field(0, extra_id).is_none());
        slot_set_instance_field(0, extra_id, Value::Long(1));
        match slot_get_instance_field(obj_tag, extra_id) {
            Some(Value::Long(l)) => assert_ne!(l, 1),
            other => panic!("expected Long, got {other:?}"),
        }
    }

    #[test]
    fn static_accessors_use_the_pool_slots() {
        let _rt = crate::test_support::runtime();
        install_fixture();
        let base = class_tag("j2r/test/JniBase");
        let counter = slot_get_field_id(base, "counter", "I", true).unwrap();
        slot_set_static_field(counter, Value::Int(41));
        match slot_get_static_field(counter) {
            Some(Value::Int(i)) => assert_eq!(i, 41),
            other => panic!("expected Int, got {other:?}"),
        }
        // pool slot 3 was addressed (the fixture's static_slot), not 0
        assert_eq!(pool_get(3), Value::Int(41));
    }

    #[test]
    fn wrong_kind_and_wrong_receiver_are_loud() {
        let _rt = crate::test_support::runtime();
        install_fixture();
        let base = class_tag("j2r/test/JniBase");
        let static_id = slot_get_field_id(base, "counter", "I", true).unwrap();
        let instance_id = slot_get_field_id(base, "s", "Ljava/lang/String;", false).unwrap();
        // instance accessor + static ID → loud
        let r = std::panic::catch_unwind(|| slot_get_instance_field(1, static_id));
        assert!(r.is_err());
        // static accessor + instance ID → loud
        let r = std::panic::catch_unwind(|| slot_get_static_field(instance_id));
        assert!(r.is_err());
        // receiver of an unrelated class → loud (the subtype guard)
        let stranger = bridge::new_local(crate::heap::alloc(crate::object::JObj {
            jclass: std::sync::Arc::from("java/lang/Object"),
            slots: vec![],
            prim: None,
        }));
        let r = std::panic::catch_unwind(|| {
            slot_get_instance_field(
                bridge::jobject_to_tag(bridge::tag_to_jobject(stranger)),
                instance_id,
            )
        });
        assert!(r.is_err(), "a java/lang/Object receiver is not a JniBase");
        // wrong value shape for the descriptor → internal error (our
        // bug, the shape guard fires before the receiver guard) — loud
        // either way.
        let r = std::panic::catch_unwind(|| {
            slot_set_instance_field(
                bridge::jobject_to_tag(bridge::tag_to_jobject(stranger)),
                instance_id,
                Value::Int(3),
            )
        });
        assert!(r.is_err());
    }

    #[test]
    fn class_name_from_tag_is_loud_on_non_class() {
        let _rt = crate::test_support::runtime();
        install_fixture();
        let str_obj = bridge::new_local(crate::object::new_string("not a class"));
        let r = std::panic::catch_unwind(|| {
            class_name_from_tag(str_obj).expect("resolve succeeds; name_of panics")
        });
        assert!(r.is_err(), "class_name_of on a non-Class must panic");
        assert!(class_name_from_tag(0).is_none());
    }
}
