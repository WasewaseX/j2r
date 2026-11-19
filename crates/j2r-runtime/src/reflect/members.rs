use super::*;

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
        return Err(internal_error(
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
        return Err(internal_error(
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
        return Err(internal_error(
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
        _ => return Err(internal_error("getDeclaredConstructor: bad param array")),
    };
    let params: Vec<Arc<String>> = param_refs.iter().map(class_name_of).collect();
    let Some(info) = ctx.class_info(owner) else {
        return Err(internal_error(
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
        _ => return Err(internal_error("getDeclaredMethod: bad param array")),
    };
    let params: Vec<Arc<String>> = param_refs.iter().map(class_name_of).collect();
    let Some(info) = ctx.class_info(owner) else {
        return Err(internal_error(
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
