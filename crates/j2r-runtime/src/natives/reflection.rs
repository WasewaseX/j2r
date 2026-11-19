use super::*;

pub(crate) fn n_class_for_name(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    for_name_resolve(args, ctx, true)
}
/// `Class.forName(String, boolean, ClassLoader)` — the 3-arg overload:
/// resolution is identical (closed world); `initialize == false` must
/// NOT run the clinit (probe F2) and still resolves unknown-name errors
/// the same way; the loader is ignored (the system loader IS the closed
/// world).
pub(crate) fn n_class_for_name3(
    _recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    // javac passes the flag as the descriptor's raw boolean (Z) — the
    // boxed form only arrives through reflective invoke.
    let init = match args.get(1) {
        Some(Value::Int(i)) => *i != 0,
        Some(Value::Ref(Some(b))) => {
            let v = crate::heap::with_obj(*b, |o| o.slots[0].clone());
            v.expect_int() != 0
        }
        _ => {
            return Err(internal_error(
                "Class.forName: expected boolean initialize flag",
            ))
        }
    };
    for_name_resolve(&args[..1], ctx, init)
}
/// `ClassLoader.getSystemClassLoader()` — R2 slice 1: returns an inert
/// `java/lang/ClassLoader` node. The closed world has exactly one loader
/// (the system loader IS the world), so the node carries no state and no
/// loader-resolved lookup exists to hang behavior on; `Class.forName`'s
/// 3-arg overload accepts and ignores it (Temurin-observable behavior on
/// the closed world is identical).
pub(crate) fn n_classloader_get_system(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = crate::object::new_obj("java/lang/ClassLoader", 0)?;
    Ok(Value::Ref(Some(r)))
}
/// `Class.getClassLoader()` — the same inert `java/lang/ClassLoader`
/// node: the closed world has exactly one loader (the system loader IS
/// the world), so every class answers the same node, exactly like
/// `getSystemClassLoader` (FORNAME-INIT: the 3-arg `forName` needs the
/// loader expression to compile; the runtime accepts and ignores it).
pub(crate) fn n_class_get_class_loader(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = crate::object::new_obj("java/lang/ClassLoader", 0)?;
    Ok(Value::Ref(Some(r)))
}
pub(crate) fn n_class_get_name(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getName");
    let name = reflect::class_name_of(r);
    Ok(Value::Ref(Some(object::new_string(&reflect::binary_name(
        &name,
    )))))
}
pub(crate) fn n_class_get_simple_name(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getSimpleName");
    let name = reflect::class_name_of(r);
    Ok(Value::Ref(Some(object::new_string(&reflect::simple_name(
        &name, ctx,
    )))))
}
pub(crate) fn n_class_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.toString");
    let name = reflect::class_name_of(r);
    Ok(Value::Ref(Some(object::new_string(
        &reflect::to_string_of(&name, ctx),
    ))))
}
pub(crate) fn n_class_is_instance(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.isInstance");
    let name = reflect::class_name_of(r);
    let ok = match &arg(args, "Class.isInstance") {
        Value::Ref(None) => false,
        Value::Ref(Some(o)) => {
            let jc = crate::heap::with_obj(*o, |ob| ob.jclass.to_string());
            ctx.is_subtype(&jc, &name)
        }
        _ => return Err(internal_error("Class.isInstance needs a reference")),
    };
    Ok(Value::Int(ok as i32))
}
pub(crate) fn n_class_is_assignable_from(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.isAssignableFrom");
    let name = reflect::class_name_of(r);
    let argref = match &arg(args, "Class.isAssignableFrom") {
        Value::Ref(Some(o)) => o,
        // Temurin: isAssignableFrom(null) is a plain NPE.
        Value::Ref(None) => return Err(crate::npe()),
        _ => return Err(internal_error("Class.isAssignableFrom needs a Class")),
    };
    if &*crate::heap::with_obj(*argref, |o| o.jclass.clone()) != "java/lang/Class" {
        // Temurin casts; a non-Class arg is an internal dispatch bug here.
        return Err(internal_error("isAssignableFrom argument is not a Class"));
    }
    let other = reflect::class_name_of(argref);
    Ok(Value::Int(ctx.is_subtype(&other, &name) as i32))
}
pub(crate) fn n_class_is_interface(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.isInterface");
    let name = reflect::class_name_of(r);
    Ok(Value::Int(ctx.is_interface(&name) as i32))
}
pub(crate) fn n_class_is_array(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.isArray");
    Ok(Value::Int(reflect::class_name_of(r).starts_with('[') as i32))
}
pub(crate) fn n_class_is_primitive(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.isPrimitive");
    let name = reflect::class_name_of(r);
    // Temurin 21 oracle: void.class.isPrimitive() is TRUE (V.java capture).
    Ok(Value::Int(reflect::prim_display(&name).is_some() as i32))
}
pub(crate) fn n_class_get_superclass(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getSuperclass");
    let name = reflect::class_name_of(r);
    Ok(match reflect::superclass_of(&name, ctx) {
        Some(sup) => reflect::class_value(&sup),
        None => Value::Ref(None),
    })
}
pub(crate) fn n_class_get_component_type(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getComponentType");
    let name = reflect::class_name_of(r);
    Ok(match reflect::component_type_of(&name) {
        Some(elem) => reflect::class_value(&elem),
        None => Value::Ref(None),
    })
}
pub(crate) fn n_class_get_declared_field(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getDeclaredField");
    let owner = reflect::class_name_of(r);
    reflect::get_declared_field(ctx, &owner, args)
}
/// R2 slice 2: `Class.getField(String)` — the public-member search law
/// (own public, then the superclass chain; see reflect::get_field).
pub(crate) fn n_class_get_field(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getField");
    let owner = reflect::class_name_of(r);
    reflect::get_field(ctx, &owner, args)
}
// R2 slice 3: the array forms + the Constructor surface.
pub(crate) fn n_class_get_declared_fields(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getDeclaredFields");
    let owner = reflect::class_name_of(r);
    reflect::get_declared_fields(ctx, &owner)
}
pub(crate) fn n_class_get_fields(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getFields");
    let owner = reflect::class_name_of(r);
    reflect::get_fields(ctx, &owner)
}
pub(crate) fn n_class_get_declared_ctors(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getDeclaredConstructors");
    let owner = reflect::class_name_of(r);
    reflect::get_declared_constructors(ctx, &owner)
}
pub(crate) fn n_class_get_ctors(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getConstructors");
    let owner = reflect::class_name_of(r);
    reflect::get_constructors(ctx, &owner)
}
pub(crate) fn n_class_get_declared_ctor(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getDeclaredConstructor");
    let owner = reflect::class_name_of(r);
    reflect::get_declared_constructor(ctx, &owner, args)
}
pub(crate) fn n_class_get_ctor(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getConstructor");
    let owner = reflect::class_name_of(r);
    reflect::get_constructor(ctx, &owner, args)
}
pub(crate) fn n_ctor_get_name(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Constructor.getName");
    let (owner, _, _) = reflect::ctor_slots(r);
    Ok(Value::Ref(Some(crate::object::new_string(
        &reflect::binary_name(&owner),
    ))))
}
pub(crate) fn n_ctor_get_modifiers(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Constructor.getModifiers");
    let (_, _, mods) = reflect::ctor_slots(r);
    Ok(Value::Int(mods as i32))
}
pub(crate) fn n_ctor_get_parameter_types(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Constructor.getParameterTypes");
    let (_, desc, _) = reflect::ctor_slots(r);
    let items: Vec<Value> = reflect::desc_params(&desc)
        .iter()
        .map(|p| reflect::class_value(p))
        .collect();
    reflect::class_array(items, ctx)
}
pub(crate) fn n_ctor_get_declaring_class(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Constructor.getDeclaringClass");
    let (owner, _, _) = reflect::ctor_slots(r);
    Ok(reflect::class_value(&owner))
}
pub(crate) fn n_ctor_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Constructor.toString");
    let (owner, desc, mods) = reflect::ctor_slots(r);
    let modw = match mods & 0x0007 {
        0x0001 => "public ",
        0x0002 => "private ",
        0x0004 => "protected ",
        _ => "",
    };
    let params = reflect::desc_params(&desc)
        .iter()
        .map(|p| reflect::binary_name(p))
        .collect::<Vec<String>>()
        .join(",");
    Ok(Value::Ref(Some(crate::object::new_string(&format!(
        "{}{}({})",
        modw,
        reflect::binary_name(&owner),
        params
    )))))
}
/// `java.lang.reflect.Modifier.toString(int)` — the canonical bit order
/// (JDK source): public protected private abstract static final transient
/// volatile synchronized native strictfp interface. R2 slice 3 (the
/// fixture renders modifier masks through it).
pub(crate) fn n_modifier_to_string(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let mut mod_ = match args.first() {
        Some(Value::Int(i)) => *i as u32,
        other => {
            return Err(internal_error(&format!(
                "Modifier.toString: expected int arg, found {other:?}"
            )))
        }
    };
    // The JDK reads `mod` after masking the INTERFACE bit out of the
    // ordinary pass — replicated bit-for-bit from Modifier.toString.
    let mut has_interface = false;
    if (mod_ & 0x0200) != 0 {
        has_interface = true;
        mod_ &= !0x0200;
    }
    let mut sb = String::new();
    let push = |sb: &mut String, bit: u32, word: &str| {
        if (mod_ & bit) != 0 {
            sb.push_str(word);
            sb.push(' ');
        }
    };
    push(&mut sb, 0x0001, "public");
    push(&mut sb, 0x0004, "protected");
    push(&mut sb, 0x0002, "private");
    push(&mut sb, 0x0400, "abstract");
    push(&mut sb, 0x0008, "static");
    push(&mut sb, 0x0010, "final");
    push(&mut sb, 0x0080, "transient");
    push(&mut sb, 0x0040, "volatile");
    push(&mut sb, 0x0020, "synchronized");
    push(&mut sb, 0x0100, "native");
    push(&mut sb, 0x0800, "strictfp");
    if has_interface {
        push(&mut sb, 0x0200, "interface");
    }
    let out = sb.strip_suffix(' ').unwrap_or(&sb).to_string();
    Ok(crate::object::string_out(&out))
}

// R2 slice 2: the typed getter/setter family (`Field.getInt` and
// friends). Each delegates to field_get/field_set (the JDK's
// receiver-mismatch IAE and NPE laws already live there) and then
// narrows the boxed Value to the requested primitive kind. A narrow
// mismatch (getInt on a double field) throws IllegalArgumentException
// with a named message — the fixture pins the correct-narrow shapes;
// the wrong-narrow message follows the JDK shape but is not pinned.
macro_rules! field_typed_getter {
    ($name:ident, $want:expr) => {
        pub fn $name(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
            let f = this(recv, "Field.typed get");
            let obj = match args.first() {
                Some(Value::Ref(r)) => r.as_ref(),
                _ => return Err(internal_error("Field typed get needs a receiver argument")),
            };
            reflect::field_get_typed(ctx, f, obj, $want)
        }
    };
}

macro_rules! field_typed_setter {
    ($name:ident, $want:expr) => {
        pub fn $name(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
            let f = this(recv, "Field.typed set");
            let obj = match args.first() {
                Some(Value::Ref(r)) => r.as_ref(),
                _ => return Err(internal_error("Field typed set needs a receiver argument")),
            };
            let value = match args.get(1) {
                Some(v) => v.clone(),
                None => return Err(internal_error("Field typed set needs a value argument")),
            };
            reflect::field_set_typed(ctx, f, obj, &value, $want)?;
            Ok(Value::Void)
        }
    };
}

field_typed_getter!(n_field_get_int, 'I');
field_typed_getter!(n_field_get_long, 'J');
field_typed_getter!(n_field_get_float, 'F');
field_typed_getter!(n_field_get_double, 'D');
field_typed_getter!(n_field_get_boolean, 'Z');
field_typed_getter!(n_field_get_char, 'C');
field_typed_getter!(n_field_get_short, 'S');
field_typed_getter!(n_field_get_byte, 'B');

field_typed_setter!(n_field_set_int, 'I');
field_typed_setter!(n_field_set_long, 'J');
field_typed_setter!(n_field_set_float, 'F');
field_typed_setter!(n_field_set_double, 'D');
field_typed_setter!(n_field_set_boolean, 'Z');
field_typed_setter!(n_field_set_char, 'C');
field_typed_setter!(n_field_set_short, 'S');
field_typed_setter!(n_field_set_byte, 'B');
/// R2 slice 2: `Field.getModifiers()` — the raw ACC_* mask from the
/// classfile, carried in the Field object's slot 6.
pub(crate) fn n_field_get_modifiers(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let f = this(recv, "Field.getModifiers");
    let (_, _, _, _, _, _, modifiers) = reflect::field_slots(f);
    Ok(Value::Int(modifiers as i32))
}
/// R2 slice 2: `Field.getDeclaringClass()` — the owner class from slot 0.
pub(crate) fn n_field_get_declaring_class(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let f = this(recv, "Field.getDeclaringClass");
    let (owner, _, _, _, _, _, _) = reflect::field_slots(f);
    Ok(Value::Ref(Some(reflect::class_ref(&owner))))
}
/// R2 slice 2: `Method.getModifiers()` — the raw ACC_* mask (slot 4).
pub(crate) fn n_method_get_modifiers(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let m = this(recv, "Method.getModifiers");
    let (_, _, _, _, modifiers) = reflect::method_slots(m);
    Ok(Value::Int(modifiers as i32))
}
pub(crate) fn n_class_get_declared_method(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = this(recv, "Class.getDeclaredMethod");
    let owner = reflect::class_name_of(r);
    reflect::get_declared_method(ctx, &owner, args)
}
pub(crate) fn n_field_get(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let f = this(recv, "Field.get");
    let obj = match args.first() {
        Some(Value::Ref(r)) => r.as_ref(),
        _ => return Err(internal_error("Field.get needs a receiver argument")),
    };
    reflect::field_get(ctx, f, obj)
}
pub(crate) fn n_field_set(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let f = this(recv, "Field.set");
    let obj = match args.first() {
        Some(Value::Ref(r)) => r.as_ref(),
        _ => return Err(internal_error("Field.set needs a receiver argument")),
    };
    let value = args.get(1).cloned().unwrap_or(Value::Ref(None));
    reflect::field_set(ctx, f, obj, &value)?;
    Ok(Value::Void)
}
pub(crate) fn n_method_invoke(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let m = this(recv, "Method.invoke");
    let target = match args.first() {
        Some(Value::Ref(r)) => r.as_ref(),
        _ => return Err(internal_error("Method.invoke needs a receiver argument")),
    };
    let argv = match args.get(1) {
        Some(Value::Ref(r)) => r.as_ref(),
        _ => return Err(internal_error("Method.invoke needs an args array")),
    };
    reflect::method_invoke(ctx, m, target, argv)
}
pub(crate) fn n_field_get_name(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let f = this(recv, "Field.getName");
    let (_, name, _, _, _, _, _) = reflect::field_slots(f);
    Ok(Value::Ref(Some(object::new_string(&name))))
}
/// `Field.getType`: map the stored field descriptor to the interned
/// `Class` object. Primitives are keyed by their bare descriptor (`I` ->
/// `int.class`), object descriptors drop the `L...;` wrapper, and array
/// descriptors (`[I`, `[Ljava/lang/String;`) intern as-is — exactly the
/// keys `component_type_of` hands back, so `int[].class.getComponentType()
/// == int.class` holds end to end (BigCache oracle).
pub(crate) fn n_field_get_type(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let f = this(recv, "Field.getType");
    let (_, _, desc, _, _, _, _) = reflect::field_slots(f);
    let d = desc.as_str();
    let key: &str = if d.starts_with('[') {
        d
    } else if d.starts_with('L') && d.len() >= 2 && d.ends_with(';') {
        &d[1..d.len() - 1]
    } else {
        d
    };
    let r = reflect::class_ref(key);
    Ok(Value::Ref(Some(r)))
}
pub(crate) fn n_method_get_name(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let m = this(recv, "Method.getName");
    let (_, name, _, _, _) = reflect::method_slots(m);
    Ok(Value::Ref(Some(object::new_string(&name))))
}
/// `Method.equals`: structural (same declaring class, name and descriptor),
/// not identity — Temurin hands back fresh Method objects per lookup
/// (fixture oracle: `m1 == m2` false but `m1.equals(m2)` true).
pub(crate) fn n_method_equals(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let m = this(recv, "Method.equals");
    // M4 Wave 1 lock law L3: the class check and the slot reads are
    // separate flat heap accesses, never nested inside one guard.
    let same = match &arg(args, "Method.equals") {
        Value::Ref(Some(other)) => {
            let is_method =
                crate::heap::with_obj(*other, |o| o.jclass.as_ref() == "java/lang/reflect/Method");
            is_method && {
                let (o0, o1, o2, _, _) = reflect::method_slots(other);
                let (s0, s1, s2, _, _) = reflect::method_slots(m);
                o0 == s0 && o1 == s1 && o2 == s2
            }
        }
        _ => false,
    };
    Ok(Value::Int(same as i32))
}
pub(crate) fn n_system_gc(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::heap::gc_now();
    Ok(Value::Void)
}
/// `Constructor.newInstance(Object...)` — NEW-INSTANCE: the args array
/// rides unmarshaled into the `<init>` through the dispatch table (the
/// implementation and the probed laws live in `reflect::ctor_new_instance`).
pub(crate) fn n_ctor_new_instance(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let k = this(recv, "Constructor.newInstance");
    let argv = match args.first() {
        Some(Value::Ref(r)) => r.as_ref(),
        _ => {
            return Err(internal_error(
                "Constructor.newInstance needs the args array",
            ))
        }
    };
    reflect::ctor_new_instance(ctx, k, argv)
}

// ---------------------------------------------------------------- Table
// ---------------------------------------------------------- java/lang/Thread
// M4 Wave 2 (T06): thin wrappers onto `crate::thread` (implementation,
// JDK contract and honesty notes live there).
