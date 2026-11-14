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

/// `Class.toString()`: `class X` / `interface X` / keyword for primitives /
/// `class […` for arrays (arrays report `class` regardless of element kind).
pub fn to_string_of(internal: &str, ctx: Dispatch) -> String {
    if let Some(p) = prim_display(internal) {
        return p.to_string();
    }
    if internal.starts_with('[') {
        return format!("class {}", binary_name(internal));
    }
    if ctx.is_interface(internal) {
        format!("interface {}", binary_name(internal))
    } else {
        format!("class {}", binary_name(internal))
    }
}

/// `Class.getSuperclass()`: interfaces → `None` (incl. builtin interfaces,
/// whose subtype parent deliberately connects them to `Object`);
/// primitives → `None`; arrays → `Object`; everything else walks the
/// compiled-first parent chain.
pub fn superclass_of(internal: &str, ctx: Dispatch) -> Option<String> {
    if prim_display(internal).is_some() {
        return None;
    }
    if internal.starts_with('[') {
        return Some("java/lang/Object".to_string());
    }
    if ctx.is_interface(internal) {
        return None;
    }
    // subtype::parent_of resolves compiled classes through the parents
    // table (superclass first) and falls back to the builtin chain.
    ctx.parent_of(internal).map(|p| p.to_string())
}

/// `Class.getComponentType()`: `[X` → element name (unwrapping `L…;` to
/// the bare internal name — the element of `[Ljava/lang/String;` IS
/// `java/lang/String`), anything else → `None`.
pub fn component_type_of(internal: &str) -> Option<String> {
    let elem = internal.strip_prefix('[')?;
    Some(
        match elem.strip_prefix('L').and_then(|s| s.strip_suffix(';')) {
            Some(cls) => cls.to_string(),
            None => elem.to_string(),
        },
    )
}

// ---------------------------------------------------------------------------
// Field / Method objects (fixed slot layouts, also mirrored in builtin.rs).
// ---------------------------------------------------------------------------

/// `Field` slots: 0 owner class (internal), 1 name, 2 descriptor,
/// 3 is_static, 4 instance slot, 5 static pool slot, 6 raw modifiers.
pub(crate) fn field_obj(
    owner: &str,
    name: &str,
    desc: &str,
    is_static: bool,
    instance_slot: usize,
    static_slot: u32,
    modifiers: u16,
) -> JRef {
    crate::heap::alloc(crate::object::JObj {
        jclass: Arc::from("java/lang/reflect/Field"),
        slots: vec![
            Value::Str(Arc::new(owner.to_string())),
            Value::Str(Arc::new(name.to_string())),
            Value::Str(Arc::new(desc.to_string())),
            Value::Int(is_static as i32),
            Value::Int(instance_slot as i32),
            Value::Int(static_slot as i32),
            Value::Int(modifiers as i32),
        ],
        prim: None,
    })
}

/// `Method` slots: 0 owner class (internal), 1 name, 2 descriptor,
/// 3 is_static. Freshly allocated per lookup — Temurin does NOT cache
/// Method copies (fixture oracle: `m1 == m2` is false), and `Method.equals`
/// is structural, implemented as a native over these slots.
pub(crate) fn method_obj(
    owner: &str,
    name: &str,
    desc: &str,
    is_static: bool,
    modifiers: u16,
) -> JRef {
    crate::heap::alloc(crate::object::JObj {
        jclass: Arc::from("java/lang/reflect/Method"),
        slots: vec![
            Value::Str(Arc::new(owner.to_string())),
            Value::Str(Arc::new(name.to_string())),
            Value::Str(Arc::new(desc.to_string())),
            Value::Int(is_static as i32),
            Value::Int(modifiers as i32),
        ],
        prim: None,
    })
}

pub(crate) fn field_slots(
    r: &JRef,
) -> (Arc<String>, Arc<String>, Arc<String>, bool, usize, u32, u16) {
    with_obj(*r, |o| {
        let s = |i: usize| match &o.slots[i] {
            Value::Str(v) => v.clone(),
            other => panic!("j2r internal: bad Field slot {i}: {other:?}"),
        };
        (
            s(0),
            s(1),
            s(2),
            o.slots[3].expect_int() != 0,
            o.slots[4].expect_int() as usize,
            o.slots[5].expect_int() as u32,
            o.slots[6].expect_int() as u16,
        )
    })
}

pub(crate) fn method_slots(r: &JRef) -> (Arc<String>, Arc<String>, Arc<String>, bool, u16) {
    with_obj(*r, |o| {
        let s = |i: usize| match &o.slots[i] {
            Value::Str(v) => v.clone(),
            other => panic!("j2r internal: bad Method slot {i}: {other:?}"),
        };
        (
            s(0),
            s(1),
            s(2),
            o.slots[3].expect_int() != 0,
            o.slots[4].expect_int() as u16,
        )
    })
}

fn str_arg(args: &[Value], who: &str) -> Result<Arc<String>, Throw> {
    match args.first() {
        Some(Value::Ref(Some(r))) => Ok(crate::object::string_value(r)),
        Some(Value::Ref(None)) => Err(crate::throw_new("java/lang/NullPointerException", None)),
        _ => Err(crate::internal_error(&format!(
            "{who}: expected string arg"
        ))),
    }
}

/// `Class.getDeclaredField(String)`. Compiled classes only (loud internal
/// error otherwise); miss → `NoSuchFieldException: <name>`, null name → NPE.
pub fn get_declared_field(ctx: Dispatch, owner: &str, args: &[Value]) -> Result<Value, Throw> {
    let name = str_arg(args, "getDeclaredField")?;
    let Some(info) = ctx.class_info(owner) else {
        return Err(crate::internal_error(
            "reflection on builtin classes is outside the M3 subset (getDeclaredField)",
        ));
    };
    match info.fields.iter().find(|f| f.name == name.as_str()) {
        Some(f) => Ok(Value::Ref(Some(field_obj(
            owner,
            &name,
            f.desc,
            f.is_static,
            f.instance_slot,
            f.static_slot,
            f.modifiers,
        )))),
        None => Err(crate::throw_new(
            "java/lang/NoSuchFieldException",
            Some(&name),
        )),
    }
}

/// The JDK's primitive-widening table (jdk.internal.reflect canConvert):
/// which field descriptor kinds a typed getter accepts. `to Z/B` accept
/// only themselves; every integer family widens upward.
pub(crate) fn can_convert(from: char, to: char) -> bool {
    if from == to {
        return true;
    }
    match to {
        'C' => matches!(from, 'B' | 'S'),
        'S' => matches!(from, 'B'),
        'I' => matches!(from, 'B' | 'C' | 'S'),
        'J' => matches!(from, 'B' | 'C' | 'S' | 'I'),
        'F' => matches!(from, 'B' | 'C' | 'S' | 'I' | 'J'),
        'D' => matches!(from, 'B' | 'C' | 'S' | 'I' | 'J' | 'F'),
        _ => false,
    }
}

fn prim_display_of(kind: char) -> &'static str {
    match kind {
        'Z' => "boolean",
        'B' => "byte",
        'C' => "char",
        'S' => "short",
        'I' => "int",
        'J' => "long",
        'F' => "float",
        _ => "double",
    }
}

/// Narrows a raw pool/slot Value to the requested primitive kind — the
/// widening semantics are Java's own (I→F rounds, I→J sign-extends,
/// everything exact). The B/C/S/Z payloads already live in `Value::Int`
/// normalized (sign/zero-extended), so same-family narrows are no-ops.
pub(crate) fn narrow_value(v: Value, want: char) -> Value {
    match (v, want) {
        (Value::Int(i), 'J') => Value::Long(i as i64),
        (Value::Int(i), 'F') => Value::Float(i as f32),
        (Value::Int(i), 'D') => Value::Double(i as f64),
        (Value::Long(l), 'F') => Value::Float(l as f32),
        (Value::Long(l), 'D') => Value::Double(l as f64),
        (Value::Float(f), 'D') => Value::Double(f as f64),
        (v, _) => v,
    }
}

/// The typed getter core (`Field.getInt` and friends): the same
/// receiver/null laws as field_get, then the JDK's widening table, then
/// the raw-value narrow. Error messages are Temurin-exact (NarrowMsg
/// oracle): `Attempt to get double field "NarrowMsg.D" with illegal
/// data type conversion to int`.
pub fn field_get_typed(
    ctx: Dispatch,
    f: &JRef,
    obj: Option<&JRef>,
    want: char,
) -> Result<Value, Throw> {
    let (owner, name, desc, is_static, islot, sslot, _modifiers) = field_slots(f);
    let raw = if is_static {
        let Some(get) = ctx.static_get else {
            return Err(crate::internal_error("Field.get: no static pool access"));
        };
        get(sslot)
    } else {
        let Some(o) = obj else {
            return Err(throw_npe_null_recv());
        };
        let ok = with_obj(*o, |ob| ctx.is_subtype(&ob.jclass, &owner));
        if !ok {
            return Err(throw_ill_arg(&format!(
                "Can not get {} field {}.{} on {}",
                java_type_name(&desc),
                binary_name(&owner),
                name,
                dotted_class_of(obj)
            )));
        }
        crate::object::obj_get(o, islot)
    };
    let from = desc.chars().next().unwrap_or('?');
    if !can_convert(from, want) {
        return Err(throw_ill_arg(&format!(
            "Attempt to get {} field \"{}.{}\" with illegal data type conversion to {}",
            java_type_name(&desc),
            binary_name(&owner),
            name,
            prim_display_of(want)
        )));
    }
    Ok(narrow_value(raw, want))
}

/// The typed setter core: the value's payload kind must match the field
/// kind exactly (the JDK's setInt on a long field is an IAE — cross-kind
/// setter messages follow the JDK shape; the fixture pins exact-kind
/// sets, the cross-kind message is an honest residual).
pub fn field_set_typed(
    ctx: Dispatch,
    f: &JRef,
    obj: Option<&JRef>,
    v: &Value,
    want: char,
) -> Result<(), Throw> {
    let (owner, name, desc, _, _, _, _) = field_slots(f);
    let from = desc.chars().next().unwrap_or('?');
    if from != want {
        return Err(throw_ill_arg(&format!(
            "Attempt to put {} field \"{}.{}\" with illegal data type conversion",
            java_type_name(&desc),
            binary_name(&owner),
            name
        )));
    }
    field_set(ctx, f, obj, v)
}

/// `Class.getField(String)` — the JDK's public-member search (OpenJDK
/// getField0 shape): own PUBLIC fields first, then the SUPERCLASS chain
/// recursively. A non-public same-name field in a class does NOT block
/// the walk (the JDK quirk: the search continues upward). The
/// interface-constants leg of the JDK walk is a registered residual —
/// compiled-class metadata does not carry the interface list yet
/// (UserClassInfo has no interfaces table; adding one is the
/// getFields/getDeclaredConstructors slice's job).
/// Miss → `NoSuchFieldException: <name>`; null name → NPE (oracle:
/// Temurin throws plain NPE, no message — pinned by the fixture).
pub fn get_field(ctx: Dispatch, owner: &str, args: &[Value]) -> Result<Value, Throw> {
    if matches!(args.first(), Some(Value::Ref(None)) | None) {
        return Err(crate::throw_new("java/lang/NullPointerException", None));
    }
    let name = str_arg(args, "getField")?;
    match get_field0(ctx, owner, &name) {
        Some((decl_owner, f)) => Ok(Value::Ref(Some(field_obj(
            &decl_owner,
            f.name,
            f.desc,
            f.is_static,
            f.instance_slot,
            f.static_slot,
            f.modifiers,
        )))),
        None => Err(crate::throw_new(
            "java/lang/NoSuchFieldException",
            Some(&name),
        )),
    }
}

/// The getField0 walk: own public field, else recurse into the
/// superclass (compiled classes only — builtin parents have no fields
/// the closed world can reflect on).
fn get_field0(
    ctx: Dispatch,
    owner: &str,
    name: &str,
) -> Option<(String, &'static crate::UserField)> {
    if let Some(info) = ctx.class_info(owner) {
        if let Some(f) = info
            .fields
            .iter()
            .find(|f| f.name == name && (f.modifiers & 0x0001) != 0)
        {
            return Some((owner.to_string(), f));
        }
    }
    let parent = ctx.parent_of(owner)?;
    if parent == "java/lang/Object" {
        return None;
    }
    get_field0(ctx, parent, name)
}

// ---------------------------------------------------------------------------
// R2 slice 3: the array forms (getDeclaredFields/getFields) and the
// Constructor surface. Oracle: ReflectProbe on Temurin 21.0.13+9.
// ---------------------------------------------------------------------------

/// `Constructor` slots: 0 owner class (internal), 1 descriptor, 2 the raw
/// ACC_* modifiers. Freshly allocated per lookup — Temurin does NOT cache
/// reflective objects (identity across calls is unpinned).
pub(crate) fn ctor_obj(owner: &str, desc: &str, modifiers: u16) -> JRef {
    crate::heap::alloc(crate::object::JObj {
        jclass: Arc::from("java/lang/reflect/Constructor"),
        slots: vec![
            Value::Str(Arc::new(owner.to_string())),
            Value::Str(Arc::new(desc.to_string())),
            Value::Int(modifiers as i32),
        ],
        prim: None,
    })
}

pub(crate) fn ctor_slots(r: &JRef) -> (Arc<String>, Arc<String>, u16) {
    with_obj(*r, |o| {
        let s = |i: usize| match &o.slots[i] {
            Value::Str(v) => v.clone(),
            other => panic!("j2r internal: bad Constructor slot {i}: {other:?}"),
        };
        (s(0), s(1), o.slots[2].expect_int() as u16)
    })
}

/// Builds a `Field[]` / `Constructor[]` / `Class[]` holding exactly
/// `items` (array layout: slot 0 = length). `elem_class` is the BARE
/// element (array_new_ref adds the `[L…;` wrapper itself). L3 note:
/// every store takes the heap lock on its own — no with_obj closure
/// wraps the loop.
fn ref_array(elem_class: &str, items: Vec<Value>, ctx: Dispatch) -> Result<Value, Throw> {
    let arr = crate::array::array_new_ref(elem_class, items.len() as i32)?;
    for (i, v) in items.into_iter().enumerate() {
        crate::array::array_store(&arr, i as i32, v, ctx)?;
    }
    Ok(Value::Ref(Some(arr)))
}

/// `Class[]` builder (Constructor.getParameterTypes).
pub(crate) fn class_array(items: Vec<Value>, ctx: Dispatch) -> Result<Value, Throw> {
    ref_array("java/lang/Class", items, ctx)
}

/// `Class.getDeclaredFields()` — ALL declared fields (any modifier, no
/// inheritance) in DECLARATION order — oracle: Temurin lists Child's
/// [pubChild, privChild, pubBase] in source order. Compiled classes only.
pub fn get_declared_fields(ctx: Dispatch, owner: &str) -> Result<Value, Throw> {
    let Some(info) = ctx.class_info(owner) else {
        return Err(crate::internal_error(
            "reflection on builtin classes is outside the R2 subset (getDeclaredFields)",
        ));
    };
    let items: Vec<Value> = info
        .fields
        .iter()
        .map(|f| {
            Value::Ref(Some(field_obj(
                owner,
                f.name,
                f.desc,
                f.is_static,
                f.instance_slot,
                f.static_slot,
                f.modifiers,
            )))
        })
        .collect();
    ref_array("java/lang/reflect/Field", items, ctx)
}

/// `Class.getFields()` — the JDK's public walk: OWN public fields in
/// declaration order, then the superclass chain's publics AFTER them
/// (oracle: Child lists [own pubs…, then Base's pubs…]), NO dedupe of
/// shadowed names (Child.pubBase and Base.pubBase BOTH appear). The
/// interface-constants leg is the registered residual — the metadata
/// carries no interfaces table (the same gap as `getField`'s walk).
pub fn get_fields(ctx: Dispatch, owner: &str) -> Result<Value, Throw> {
    let mut items: Vec<Value> = Vec::new();
    get_fields_walk(ctx, owner, &mut items);
    ref_array("java/lang/reflect/Field", items, ctx)
}

fn get_fields_walk(ctx: Dispatch, owner: &str, out: &mut Vec<Value>) {
    if let Some(info) = ctx.class_info(owner) {
        for f in info.fields.iter().filter(|f| (f.modifiers & 0x0001) != 0) {
            out.push(Value::Ref(Some(field_obj(
                owner,
                f.name,
                f.desc,
                f.is_static,
                f.instance_slot,
                f.static_slot,
                f.modifiers,
            ))));
        }
    }
    if let Some(parent) = ctx.parent_of(owner) {
        if parent != "java/lang/Object" {
            get_fields_walk(ctx, parent, out);
        }
    }
}

/// `Class.getDeclaredConstructors()` — the metadata's CLASSFILE order
/// REVERSED: Temurin lists the LAST-declared ctor first (oracle: of
/// Ctorz's four ctors declared ()/(int)/(String,long)/(double,boolean,
/// char), Temurin prints protected(double,boolean,char) first). The
/// reversal is applied HERE; the metadata stays in classfile order.
/// Compiled classes only.
pub fn get_declared_constructors(ctx: Dispatch, owner: &str) -> Result<Value, Throw> {
    let Some(info) = ctx.class_info(owner) else {
        return Err(crate::internal_error(
            "reflection on builtin classes is outside the R2 subset (getDeclaredConstructors)",
        ));
    };
    let items: Vec<Value> = info
        .ctors
        .iter()
        .rev()
        .map(|k| Value::Ref(Some(ctor_obj(owner, k.desc, k.modifiers))))
        .collect();
    ref_array("java/lang/reflect/Constructor", items, ctx)
}

/// `Class.getConstructors()` — the PUBLIC subset of the declared walk
/// (same reversal; oracle: only `public Ctorz()` listed).
pub fn get_constructors(ctx: Dispatch, owner: &str) -> Result<Value, Throw> {
    let Some(info) = ctx.class_info(owner) else {
        return Err(crate::internal_error(
            "reflection on builtin classes is outside the R2 subset (getConstructors)",
        ));
    };
    let items: Vec<Value> = info
        .ctors
        .iter()
        .rev()
        .filter(|k| (k.modifiers & 0x0001) != 0)
        .map(|k| Value::Ref(Some(ctor_obj(owner, k.desc, k.modifiers))))
        .collect();
    ref_array("java/lang/reflect/Constructor", items, ctx)
}

/// `Class.getDeclaredConstructor(Class...)` — exact parameter match
/// against the descriptor's params portion. The `getConstructor` form is
/// the public-only subset (Temurin throws NoSuchMethodException for a
/// non-public-only match, same class). Miss message is Temurin's
/// `Owner.<init>(float)` shape (binary owner, `<init>`, dotted params,
/// no return type).
pub fn get_declared_constructor(
    ctx: Dispatch,
    owner: &str,
    args: &[Value],
) -> Result<Value, Throw> {
    ctor_lookup(ctx, owner, args, false)
}

/// `Class.getConstructor(Class...)` — the public-only form.
pub fn get_constructor(ctx: Dispatch, owner: &str, args: &[Value]) -> Result<Value, Throw> {
    ctor_lookup(ctx, owner, args, true)
}

fn ctor_lookup(
    ctx: Dispatch,
    owner: &str,
    args: &[Value],
    public_only: bool,
) -> Result<Value, Throw> {
    // Same param-array shape as getDeclaredMethod (arrays keep their
    // length in slot 0; elements start at slot 1; L3: one flat walk).
    let param_refs: Vec<JRef> = match args.first() {
        Some(Value::Ref(Some(arr))) => with_obj(*arr, |o| {
            o.slots
                .iter()
                .skip(1)
                .map(|v| match v {
                    Value::Ref(Some(c)) => Some(*c),
                    other => panic!("j2r internal: bad param class {other:?}"),
                })
                .collect::<Vec<Option<JRef>>>()
                .into_iter()
                .collect::<Option<Vec<JRef>>>()
                .expect("j2r internal: null param class in getDeclaredConstructor")
        }),
        Some(Value::Ref(None)) | None => Vec::new(),
        _ => {
            return Err(crate::internal_error(
                "getDeclaredConstructor: bad param array",
            ))
        }
    };
    let params: Vec<Arc<String>> = param_refs.iter().map(class_name_of).collect();
    let Some(info) = ctx.class_info(owner) else {
        return Err(crate::internal_error(
            "reflection on builtin classes is outside the R2 subset (getDeclaredConstructor)",
        ));
    };
    // The table stores full descriptors `(...)V`; match on the params
    // portion only (the requested shape carries no return type).
    let want_params = format!(
        "({}",
        params.iter().map(|p| param_desc(p)).collect::<String>()
    );
    for k in info.ctors.iter().rev() {
        if public_only && (k.modifiers & 0x0001) == 0 {
            continue;
        }
        // The params portion must match EXACTLY up to the closing paren —
        // a bare "(" (the no-arg shape) must not prefix-match every
        // descriptor's opening paren (the ReflectDeclared no-arg pin
        // caught exactly that).
        if k.desc.starts_with(&want_params) && k.desc[want_params.len()..].starts_with(')') {
            return Ok(Value::Ref(Some(ctor_obj(owner, k.desc, k.modifiers))));
        }
    }
    let dotted: Vec<String> = params.iter().map(|p| binary_name(p)).collect();
    Err(crate::throw_new(
        "java/lang/NoSuchMethodException",
        Some(&format!(
            "{}.<init>({})",
            binary_name(owner),
            dotted.join(",")
        )),
    ))
}

/// The RAW parameter descriptor list of a descriptor
/// (`(ILjava/lang/String;[J)V` → `["I", "Ljava/lang/String;", "[J]") —
/// Constructor.toString (dots each via binary_name: Temurin's shape is
/// dotted params, comma WITHOUT a trailing space — oracle
/// `(double,boolean,char)`) and getParameterTypes (class_value each)
/// share this parse.
pub(crate) fn desc_params(desc: &str) -> Vec<String> {
    let Some(open) = desc.find('(') else {
        return Vec::new();
    };
    let Some(close) = desc[open..].find(')') else {
        return Vec::new();
    };
    let raw = &desc[open + 1..open + close];
    let mut out = Vec::new();
    let mut rest = raw;
    while !rest.is_empty() {
        let (one, next) = one_param(rest);
        out.push(one.to_string());
        rest = next;
    }
    out
}

/// Slices one leading parameter descriptor off `s` (primitives, `L…;`,
/// and arrays with any bracket depth).
fn one_param(s: &str) -> (&str, &str) {
    let brackets = s.len() - s.trim_start_matches('[').len();
    let rest = &s[brackets..];
    if let Some(r) = rest.strip_prefix('L') {
        let end = r.find(';').expect("j2r internal: unterminated L desc");
        let end = brackets + 1 + end + 1;
        (&s[..end], &s[end..])
    } else {
        (&s[..brackets + 1], &s[brackets + 1..])
    }
}

/// Builds the method descriptor from parameter `Class` objects: a bare
/// primitive descriptor (`I`) and array descriptors (`[I`) pass through; a
/// class internal name (`java/lang/String`) is wrapped as `Ljava/lang/String;`.
fn param_desc(class_internal: &str) -> String {
    if class_internal.starts_with('[') || prim_display(class_internal).is_some() {
        class_internal.to_string()
    } else {
        format!("L{class_internal};")
    }
}

/// `Class.getDeclaredMethod(String, Class…)`. Miss message is Temurin's
/// `NoSuchMethodException: O2.t(int)` shape (binary owner, dotted params).
pub fn get_declared_method(ctx: Dispatch, owner: &str, args: &[Value]) -> Result<Value, Throw> {
    let name = str_arg(args, "getDeclaredMethod")?;
    // Arrays keep their length in slot 0; elements start at slot 1.
    // M4 Wave 1 lock law L3: the array walk and the per-class lookups are
    // separate flat heap accesses — refs first, names after the guard.
    let param_refs: Vec<JRef> = match args.get(1) {
        Some(Value::Ref(Some(arr))) => with_obj(*arr, |o| {
            o.slots
                .iter()
                .skip(1)
                .map(|v| match v {
                    Value::Ref(Some(c)) => Some(*c),
                    other => panic!("j2r internal: bad param class {other:?}"),
                })
                .collect::<Vec<Option<JRef>>>()
                .into_iter()
                .collect::<Option<Vec<JRef>>>()
                .expect("j2r internal: null param class in getDeclaredMethod")
        }),
        Some(Value::Ref(None)) | None => Vec::new(),
        _ => return Err(crate::internal_error("getDeclaredMethod: bad param array")),
    };
    let params: Vec<Arc<String>> = param_refs.iter().map(class_name_of).collect();
    let Some(info) = ctx.class_info(owner) else {
        return Err(crate::internal_error(
            "reflection on builtin classes is outside the M3 subset (getDeclaredMethod)",
        ));
    };
    // The table stores full descriptors `(...)Ret`; `split_once(')')` yields
    // the params portion *without* the closing paren, so build the wanted
    // shape the same way.
    let want_params = format!(
        "({}",
        params.iter().map(|p| param_desc(p)).collect::<String>()
    );
    // The table stores full descriptors `(...)Ret`; match on the params
    // portion only (the requested shape carries no return type).
    let matches = |m: &crate::UserMethod| {
        m.name == name.as_str()
            && match m.desc.split_once(')') {
                Some((mp, _)) => mp == want_params.as_str(),
                None => false,
            }
    };
    match info.methods.iter().find(|m| matches(m)) {
        Some(m) => Ok(Value::Ref(Some(method_obj(
            owner,
            m.name,
            m.desc,
            m.is_static,
            m.modifiers,
        )))),
        None => {
            let dotted = params
                .iter()
                .map(|p| binary_name(p))
                .collect::<Vec<_>>()
                .join(",");
            Err(crate::throw_new(
                "java/lang/NoSuchMethodException",
                Some(&format!("{}.{}({})", binary_name(owner), name, dotted)),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Boxing / unboxing for Field.get/set and Method.invoke.
// ---------------------------------------------------------------------------

/// Boxes a primitive `Value` according to a primitive field descriptor,
/// routing through the same `valueOf` natives as user code (so the
/// -128..127 identity caches apply — oracle: `f.get(o) == Integer.valueOf(5)`
/// is true on Temurin).
pub(crate) fn box_by_desc(v: Value, desc: &str) -> Result<Value, Throw> {
    let noop_ctx = Dispatch::default();
    match desc {
        "I" => natives::n_integer_value_of(None, &[v], noop_ctx),
        "J" => natives::n_long_value_of(None, &[v], noop_ctx),
        "F" => natives::n_float_value_of(None, &[v], noop_ctx),
        "D" => natives::n_double_value_of(None, &[v], noop_ctx),
        "B" => natives::n_byte_value_of(None, &[v], noop_ctx),
        "S" => natives::n_short_value_of(None, &[v], noop_ctx),
        "Z" => natives::n_boolean_value_of(None, &[v], noop_ctx),
        "C" => natives::n_character_value_of(None, &[v], noop_ctx),
        d if d.starts_with('L') || d.starts_with('[') => Ok(v),
        _ => Err(crate::internal_error(&format!("bad field desc {desc:?}"))),
    }
}

/// The exact box class a primitive descriptor demands.
fn expected_box_class(desc: &str) -> &'static str {
    match desc {
        "I" => "java/lang/Integer",
        "J" => "java/lang/Long",
        "F" => "java/lang/Float",
        "D" => "java/lang/Double",
        "B" => "java/lang/Byte",
        "S" => "java/lang/Short",
        "Z" => "java/lang/Boolean",
        "C" => "java/lang/Character",
        _ => "java/lang/Object",
    }
}

/// Unboxes a boxed argument for a primitive parameter. Exact box class or
/// `argument type mismatch` (Temurin's reflective-conversion failure).
pub(crate) fn unbox_by_desc(v: &Value, desc: &str) -> Result<Value, Throw> {
    let want = expected_box_class(desc);
    match v {
        Value::Ref(Some(r)) => {
            let ok = with_obj(*r, |o| &*o.jclass == want);
            if !ok {
                return Err(throw_ill_arg("argument type mismatch"));
            }
            Ok(with_obj(*r, |o| o.slots[0].clone()))
        }
        Value::Ref(None) => Err(throw_ill_arg("argument type mismatch")),
        // Direct primitives appear only from internal callers; accept them.
        prim @ (Value::Int(_) | Value::Long(_) | Value::Float(_) | Value::Double(_)) => {
            Ok(prim.clone())
        }
        other => Err(crate::internal_error(&format!("bad unbox arg {other:?}"))),
    }
}

fn throw_ill_arg(msg: &str) -> Throw {
    crate::throw_new("java/lang/IllegalArgumentException", Some(msg))
}

fn dotted_class_of(v: Option<&JRef>) -> String {
    match v {
        Some(r) => binary_name(&with_obj(*r, |o| o.jclass.to_string())),
        None => "null".to_string(),
    }
}

fn throw_npe_null_recv() -> Throw {
    crate::throw_new("java/lang/NullPointerException", Some(NULL_RECV_NPE))
}

// ---------------------------------------------------------------------------
// Field.get / Field.set
// ---------------------------------------------------------------------------

/// `Field.get(Object)`. Static fields ignore the receiver; instance fields
/// null-check it (Temurin NPE message), type-check it
/// (`Can not get int field O.pubf on java.lang.String`), then read and box.
pub fn field_get(ctx: Dispatch, f: &JRef, obj: Option<&JRef>) -> Result<Value, Throw> {
    let (owner, name, desc, is_static, islot, sslot, _modifiers) = field_slots(f);
    if is_static {
        let Some(get) = ctx.static_get else {
            return Err(crate::internal_error("Field.get: no static pool access"));
        };
        return box_by_desc(get(sslot), &desc);
    }
    let Some(o) = obj else {
        return Err(throw_npe_null_recv());
    };
    let ok = with_obj(*o, |ob| ctx.is_subtype(&ob.jclass, &owner));
    if !ok {
        return Err(throw_ill_arg(&format!(
            "Can not get {} field {}.{} on {}",
            java_type_name(&desc),
            binary_name(&owner),
            name,
            dotted_class_of(obj)
        )));
    }
    let raw = crate::object::obj_get(o, islot);
    box_by_desc(raw, &desc)
}

/// `Field.set(Object, Object)`. Messages mirror Temurin's accessor family:
/// receiver mismatch reports the *receiver* class as the `to` part (the JDK
/// passes `obj` as the value there), value mismatch the value class,
/// null-on-primitive `null value` (oracle O2/O3).
pub fn field_set(ctx: Dispatch, f: &JRef, obj: Option<&JRef>, v: &Value) -> Result<(), Throw> {
    let (owner, name, desc, is_static, islot, sslot, _modifiers) = field_slots(f);
    let set_to = |cls: String| {
        throw_ill_arg(&format!(
            "Can not set {} field {}.{} to {}",
            java_type_name(&desc),
            binary_name(&owner),
            name,
            cls
        ))
    };
    let is_ref = desc.starts_with('L') || desc.starts_with('[');
    if is_static {
        let cooked = if is_ref {
            if let Value::Ref(Some(r)) = v {
                let jc = with_obj(*r, |ob| ob.jclass.to_string());
                if !ctx.is_subtype(&jc, &desc) {
                    let cls = dotted_class_of(Some(r));
                    return Err(set_to(cls));
                }
            }
            v.clone()
        } else {
            prim_cook(v, &desc, &set_to)?
        };
        let Some(set) = ctx.static_set else {
            return Err(crate::internal_error("Field.set: no static pool access"));
        };
        set(sslot, &cooked);
        return Ok(());
    }
    let Some(o) = obj else {
        return Err(throw_npe_null_recv());
    };
    if !with_obj(*o, |ob| ctx.is_subtype(&ob.jclass, &owner)) {
        let cls = dotted_class_of(obj);
        return Err(set_to(cls));
    }
    let cooked = if is_ref {
        match v {
            Value::Ref(None) => Value::Ref(None),
            Value::Ref(Some(r)) => {
                let jc = with_obj(*r, |ob| ob.jclass.to_string());
                if !ctx.is_subtype(&jc, &desc) {
                    let cls = dotted_class_of(Some(r));
                    return Err(set_to(cls));
                }
                v.clone()
            }
            other => return Err(crate::internal_error(&format!("bad field value {other:?}"))),
        }
    } else {
        prim_cook(v, &desc, &set_to)?
    };
    crate::object::obj_set(o, islot, cooked);
    Ok(())
}

/// Primitive-field set: exact wrapper class required; failures carry
/// Temurin's `Can not set … to <class>` / `… to null value` wording
/// (oracle O3). `Method.invoke` keeps `argument type mismatch` instead.
fn prim_cook(v: &Value, desc: &str, set_to: &dyn Fn(String) -> Throw) -> Result<Value, Throw> {
    match v {
        Value::Ref(None) => Err(set_to("null value".to_string())),
        Value::Ref(Some(r)) => {
            let want = expected_box_class(desc);
            let jc = with_obj(*r, |ob| ob.jclass.to_string());
            if jc != want {
                let cls = dotted_class_of(Some(r));
                return Err(set_to(cls));
            }
            Ok(with_obj(*r, |o| o.slots[0].clone()))
        }
        other => Err(crate::internal_error(&format!("bad field value {other:?}"))),
    }
}

// ---------------------------------------------------------------------------
// Method.invoke
// ---------------------------------------------------------------------------

/// `Method.invoke(Object, Object…)`. Compiled classes only. Wraps every
/// exception the target throws in `InvocationTargetException` (message
/// `null`, like Temurin; `getCause` is outside the M3 subset).
pub fn method_invoke(
    ctx: Dispatch,
    m: &JRef,
    recv: Option<&JRef>,
    args: Option<&JRef>,
) -> Result<Value, Throw> {
    let (owner, name, desc, is_static, _modifiers) = method_slots(m);
    if ctx.class_info(&owner).is_none() {
        return Err(crate::internal_error(
            "reflection on builtin classes is outside the M3 subset (Method.invoke)",
        ));
    }
    let Some(f) = ctx.find_method(&owner, &name, &desc) else {
        return Err(crate::internal_error(&format!(
            "reflective target {owner}.{name}{desc} not in dispatch table"
        )));
    };
    // Arrays keep their length in slot 0; elements start at slot 1.
    let actual: Vec<Value> = match args {
        None => Vec::new(),
        Some(arr) => with_obj(*arr, |o| o.slots.iter().skip(1).cloned().collect()),
    };
    let Some((params, ret)) = parse_method_desc(&desc) else {
        return Err(crate::internal_error(&format!("bad method desc {desc:?}")));
    };
    if actual.len() != params.len() {
        return Err(throw_ill_arg(&format!(
            "wrong number of arguments: {} expected: {}",
            actual.len(),
            params.len()
        )));
    }
    let cooked = unmarshal_args(ctx, &params, &actual)?;
    let recv_for_call: Option<&JRef> = if is_static {
        // Temurin ignores the receiver on static invokes (oracle: a non-null
        // receiver of the wrong class still succeeds).
        None
    } else {
        match recv {
            None => return Err(throw_npe_null_recv()),
            Some(r) => {
                let jc = with_obj(*r, |ob| ob.jclass.to_string());
                if !ctx.is_subtype(&jc, &owner) {
                    return Err(throw_ill_arg(
                        "object is not an instance of declaring class",
                    ));
                }
                Some(r)
            }
        }
    };
    match f(recv_for_call, &cooked) {
        Ok(v) => {
            if ret == TypeDesc::Void {
                Ok(Value::Ref(None))
            } else {
                box_by_desc(v, &ret.field_desc())
            }
        }
        Err(t) => Err(wrap_in_ite(t)),
    }
}

/// Type-check + unmarshal reflective arguments (Temurin:
/// `argument type mismatch` / the IllegalArgumentException family) — the
/// one law Method.invoke and Constructor.newInstance share.
fn unmarshal_args(
    ctx: Dispatch,
    params: &[TypeDesc],
    actual: &[Value],
) -> Result<Vec<Value>, Throw> {
    let mut cooked: Vec<Value> = Vec::with_capacity(params.len());
    for (t, a) in params.iter().zip(actual.iter()) {
        match t {
            TypeDesc::Object(internal) => match a {
                Value::Ref(None) => cooked.push(Value::Ref(None)),
                Value::Ref(Some(r)) => {
                    let jc = with_obj(*r, |ob| ob.jclass.to_string());
                    if !ctx.is_subtype(jc.as_str(), internal) {
                        return Err(throw_ill_arg("argument type mismatch"));
                    }
                    cooked.push(a.clone());
                }
                _ => return Err(throw_ill_arg("argument type mismatch")),
            },
            TypeDesc::Array(d) => match a {
                Value::Ref(None) => cooked.push(Value::Ref(None)),
                Value::Ref(Some(r)) => {
                    let jc = with_obj(*r, |ob| ob.jclass.to_string());
                    if !ctx.is_subtype(jc.as_str(), d) {
                        return Err(throw_ill_arg("argument type mismatch"));
                    }
                    cooked.push(a.clone());
                }
                _ => return Err(throw_ill_arg("argument type mismatch")),
            },
            _ => cooked.push(unbox_by_desc(a, &t.field_desc())?),
        }
    }
    Ok(cooked)
}

/// `Constructor.newInstance` — NEW-INSTANCE (registered 2026-11-13,
/// fixed 2026-11-14, probes NewProbe/NewProbe2 vs Temurin 21.0.13+9):
///   P1 the clinit runs (once) before the ctor, and the ctor sees the
///      JLS §4.12.5 defaults (x=0 before assignment) — the layout
///      defaults ride the metadata (defaults_spec, total_slots);
///   P4/P7 the clinit's EIIE propagates UNWRAPPED (bare
///      ExceptionInInitializerError with the original as cause), and
///      the errant NCDFE with the fresh-EIIE cause on later accesses —
///      the ensure throws are NOT InvocationTargetException-wrapped;
///   P5 a same-package private ctor succeeds (the caller-sensitivity
///      residual: j2r has no caller concept, so the invoke ALWAYS runs;
///      the DIFFERENT-package IllegalAccessException is the registered
///      residual);
///   P6 an abstract class throws the message-null
///      InstantiationException (NOT wrapped);
///   P9 a ctor's own throw wraps in InvocationTargetException with the
///      original as cause (exactly Method.invoke's law).
pub fn ctor_new_instance(ctx: Dispatch, ctor: &JRef, args: Option<&JRef>) -> Result<Value, Throw> {
    let (owner, desc, _modifiers) = ctor_slots(ctor);
    let Some(info) = ctx.class_info(&owner) else {
        return Err(crate::internal_error(
            "reflection on builtin classes is outside the R2 subset (Constructor.newInstance)",
        ));
    };
    // The clinit law: the ensure's EIIE / errant NCDFE propagate UNWRAPPED
    // (probes P7/P8 — the class-init exception escapes the reflection
    // wrap).
    crate::clinit::ensure(info.id, info.clinit)?;
    if info.is_abstract {
        return Err(crate::throw_new("java/lang/InstantiationException", None));
    }
    let Some(f) = ctx.find_method(&owner, "<init>", &desc) else {
        return Err(crate::internal_error(&format!(
            "reflective target {owner}.<init>{desc} not in dispatch table"
        )));
    };
    // Arrays keep their length in slot 0; elements start at slot 1.
    let actual: Vec<Value> = match args {
        None => Vec::new(),
        Some(arr) => with_obj(*arr, |o| o.slots.iter().skip(1).cloned().collect()),
    };
    let Some((params, _ret)) = parse_method_desc(&desc) else {
        return Err(crate::internal_error(&format!("bad ctor desc {desc:?}")));
    };
    if actual.len() != params.len() {
        return Err(throw_ill_arg(&format!(
            "wrong number of arguments: {} expected: {}",
            actual.len(),
            params.len()
        )));
    }
    let cooked = unmarshal_args(ctx, &params, &actual)?;
    // The JLS §4.12.5 default-slot fill over the WHOLE flattened layout
    // (the ctor sees x=0 before assignment — probe P1). set_defaults
    // applies the spec from the tail: the builtin base region stays Void
    // exactly like the emitted `new` sites.
    let obj = crate::object::new_obj(&owner, info.total_slots)?;
    crate::object::set_defaults(&obj, info.defaults_spec);
    // The <init> runs ON the fresh object (the receiver rides the call,
    // exactly like the generated `new` sequence's invokespecial).
    match f(Some(&obj), &cooked) {
        Ok(_) => Ok(Value::Ref(Some(obj))),
        Err(t) => Err(wrap_in_ite(t)),
    }
}

/// Wraps a target exception in `java/lang/reflect/InvocationTargetException`
/// with a `null` message (Temurin prints the bare class name). The wrapped
/// target rides in the cause slot (audit A2 added `Throwable.cause`), so
/// `getCause()` returns it exactly like the JVM.
fn wrap_in_ite(t: Throw) -> Throw {
    let obj = crate::heap::alloc(crate::object::JObj {
        jclass: Arc::from("java/lang/reflect/InvocationTargetException"),
        slots: vec![Value::Ref(None), Value::Ref(Some(t.obj)), Value::Ref(None)],
        prim: None,
    });
    Throw { obj }
}

/// Unit tests: name formatting and the intern table (identity + GC rooting).
#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Dispatch {
        Dispatch::default()
    }

    /// R2 slice 3: the constructor-descriptor parameter parse —
    /// primitives, `L…;` classes, arrays at any depth, and the empty
    /// list. The `getDeclaredConstructor` exact-match law (the closing
    /// paren must land where the wanted shape ends) rides the same
    /// parse and is pinned differentially by ReflectDeclared.
    #[test]
    fn ctor_desc_params_parse() {
        assert!(desc_params("()V").is_empty());
        assert_eq!(desc_params("(I)V"), vec!["I"]);
        assert_eq!(
            desc_params("(ILjava/lang/String;[J)V"),
            vec!["I", "Ljava/lang/String;", "[J"]
        );
        assert_eq!(
            desc_params("([[Ljava/lang/String;I[D)V"),
            vec!["[[Ljava/lang/String;", "I", "[D"]
        );
    }

    #[test]
    fn binary_names_match_temurin() {
        let _rt = crate::test_support::runtime();
        assert_eq!(binary_name("java/lang/String"), "java.lang.String");
        assert_eq!(binary_name("O$Nested"), "O$Nested");
        assert_eq!(binary_name("I"), "int");
        assert_eq!(binary_name("[I"), "[I");
        assert_eq!(binary_name("[Ljava/lang/String;"), "[Ljava.lang.String;");
        assert_eq!(binary_name("[[J"), "[[J");
        assert_eq!(binary_name("V"), "void");
    }

    #[test]
    fn canonical_names_dots_nesting() {
        let _rt = crate::test_support::runtime();
        // Enum.valueOf's miss message: nested enums print dotted all the way
        // down (Temurin: `No enum constant EnumProbe.Color.red`).
        assert_eq!(canonical_name("EnumProbe$Color"), "EnumProbe.Color");
        assert_eq!(canonical_name("a/b/C$D$E"), "a.b.C.D.E");
        assert_eq!(canonical_name("Color"), "Color");
    }

    #[test]
    fn simple_names_derive_and_override() {
        let _rt = crate::test_support::runtime();
        assert_eq!(simple_name("java/lang/String", ctx()), "String");
        assert_eq!(simple_name("java/util/HashMap$Itr", ctx()), "Itr");
        assert_eq!(simple_name("I", ctx()), "int");
        assert_eq!(simple_name("[I", ctx()), "int[]");
        assert_eq!(simple_name("[Ljava/lang/String;", ctx()), "String[]");
        assert_eq!(simple_name("[[J", ctx()), "long[][]");
    }

    #[test]
    fn to_string_shapes() {
        let _rt = crate::test_support::runtime();
        assert_eq!(
            to_string_of("java/lang/String", ctx()),
            "class java.lang.String"
        );
        assert_eq!(
            to_string_of("java/lang/Runnable", ctx()),
            "interface java.lang.Runnable"
        );
        assert_eq!(to_string_of("I", ctx()), "int");
        assert_eq!(
            to_string_of("[Ljava/lang/String;", ctx()),
            "class [Ljava.lang.String;"
        );
    }

    #[test]
    fn class_objects_are_interned_and_rooted() {
        let _rt = crate::test_support::runtime();
        let a = class_ref("j2r/test/ReflIntern");
        let b = class_ref("j2r/test/ReflIntern");
        assert!(JRef::ptr_eq(&a, &b));
        let mut roots = Vec::new();
        scan_roots(&mut roots);
        assert!(roots.contains(&a.index()));
        assert_eq!(class_name_of(&a).as_str(), "j2r/test/ReflIntern");
    }

    #[test]
    fn superclass_edges() {
        let _rt = crate::test_support::runtime();
        assert!(superclass_of("I", ctx()).is_none());
        assert_eq!(
            superclass_of("[I", ctx()).as_deref(),
            Some("java/lang/Object")
        );
        assert!(superclass_of("java/lang/Runnable", ctx()).is_none());
        assert_eq!(
            superclass_of("java/lang/Integer", ctx()).as_deref(),
            Some("java/lang/Number")
        );
    }

    #[test]
    fn component_types() {
        let _rt = crate::test_support::runtime();
        // Component types come back as *intern keys* (internal names /
        // primitive descriptors), never raw field descriptors — the caller
        // feeds them straight into `class_ref` for identity equality with
        // `X.class` (ClassMeta fixture: `getComponentType() == String.class`
        // proven live against Temurin).
        assert_eq!(component_type_of("[I").as_deref(), Some("I"));
        assert_eq!(
            component_type_of("[Ljava/lang/String;").as_deref(),
            Some("java/lang/String")
        );
        // One strip only: `String[][]`'s component is `String[]`, which
        // keeps its own descriptor (still an array, still internable).
        assert_eq!(
            component_type_of("[[Ljava/lang/String;").as_deref(),
            Some("[Ljava/lang/String;")
        );
        assert!(component_type_of("java/lang/String").is_none());
    }
}
