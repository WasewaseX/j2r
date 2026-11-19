use super::*;

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
pub(crate) fn str_arg(args: &[Value], who: &str) -> Result<Arc<String>, Throw> {
    match args.first() {
        Some(Value::Ref(Some(r))) => Ok(crate::object::string_value(r)),
        Some(Value::Ref(None)) => Err(crate::throw_new("java/lang/NullPointerException", None)),
        _ => Err(internal_error(&format!("{who}: expected string arg"))),
    }
}
/// `Class.getDeclaredField(String)`. Compiled classes only (loud internal
/// error otherwise); miss → `NoSuchFieldException: <name>`, null name → NPE.
pub fn get_declared_field(ctx: Dispatch, owner: &str, args: &[Value]) -> Result<Value, Throw> {
    let name = str_arg(args, "getDeclaredField")?;
    let Some(info) = ctx.class_info(owner) else {
        return Err(internal_error(
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
            return Err(internal_error("Field.get: no static pool access"));
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
