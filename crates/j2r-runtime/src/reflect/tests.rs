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
    let _rt = crate::runtime();
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
    let _rt = crate::runtime();
    // Enum.valueOf's miss message: nested enums print dotted all the way
    // down (Temurin: `No enum constant EnumProbe.Color.red`).
    assert_eq!(canonical_name("EnumProbe$Color"), "EnumProbe.Color");
    assert_eq!(canonical_name("a/b/C$D$E"), "a.b.C.D.E");
    assert_eq!(canonical_name("Color"), "Color");
}

#[test]
fn simple_names_derive_and_override() {
    let _rt = crate::runtime();
    assert_eq!(simple_name("java/lang/String", ctx()), "String");
    assert_eq!(simple_name("java/util/HashMap$Itr", ctx()), "Itr");
    assert_eq!(simple_name("I", ctx()), "int");
    assert_eq!(simple_name("[I", ctx()), "int[]");
    assert_eq!(simple_name("[Ljava/lang/String;", ctx()), "String[]");
    assert_eq!(simple_name("[[J", ctx()), "long[][]");
}

#[test]
fn to_string_shapes() {
    let _rt = crate::runtime();
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
    let _rt = crate::runtime();
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
    let _rt = crate::runtime();
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
    let _rt = crate::runtime();
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

// ---- The reflection CALLER law (the different-package private-ctor
// residual, registered 2026-11-14, closed 2026-11-16) --------------
// The probe matrix (scripts/evidence/private_ctor/, Temurin
// 21.0.13+9): S1 diff-package private refused; S2 same-package
// NON-nestmate private refused (JEP 181: the law is NEST-based, not
// package-based); S3 diff-package protected refused; S4 the
// diff-package protected SUBCLASS refused too (newInstance passes
// obj=null — no instance context); S5 diff-package public allowed;
// S6 nestmate private allowed; S8 package-private refused with the
// `with package access` message tail.
static PC_INFO: crate::UserClassInfo = crate::UserClassInfo {
    id: 0,
    is_interface: false,
    is_abstract: false,
    simple_name: None,
    nest_host: None,
    total_slots: 0,
    defaults_spec: "",
    fields: &[],
    methods: &[],
    ctors: &[],
    clinit: None,
};
static PC_NESTED: crate::UserClassInfo = crate::UserClassInfo {
    id: 0,
    is_interface: false,
    is_abstract: false,
    simple_name: None,
    nest_host: Some("pB/Caller"),
    total_slots: 0,
    defaults_spec: "",
    fields: &[],
    methods: &[],
    ctors: &[],
    clinit: None,
};
static PC_HOST2_NESTED: crate::UserClassInfo = crate::UserClassInfo {
    id: 0,
    is_interface: false,
    is_abstract: false,
    simple_name: None,
    nest_host: Some("pA/Holder"),
    total_slots: 0,
    defaults_spec: "",
    fields: &[],
    methods: &[],
    ctors: &[],
    clinit: None,
};
fn pc_meta(class: &str) -> Option<&'static crate::UserClassInfo> {
    match class {
        "pA/Private" | "pA/Same" | "pA/Prot" | "pA/Package" | "pA/Public" | "pB/Caller"
        | "pB/Sub" | "pA/Holder" => Some(&PC_INFO),
        "pB/Caller$NestedPrivate" => Some(&PC_NESTED),
        "pA/Holder$NestedPrivate" => Some(&PC_HOST2_NESTED),
        _ => None,
    }
}
fn pc_ctx() -> Dispatch {
    Dispatch {
        meta: Some(pc_meta),
        ..Dispatch::default()
    }
}

#[test]
fn caller_law_matrix() {
    let ctx = pc_ctx();
    // S1: private, different package -> REFUSED.
    assert!(!ctor_access_allowed(
        &ctx,
        "pB/Caller",
        "pA/Private",
        0x0002
    ));
    // S2: private, SAME package but a non-nestmate -> REFUSED.
    assert!(!ctor_access_allowed(&ctx, "pA/Same", "pA/Private", 0x0002));
    // S6: private, nestmates (the NestedPrivate META carries the
    // NestHost attribute) -> allowed.
    assert!(ctor_access_allowed(
        &ctx,
        "pB/Caller",
        "pB/Caller$NestedPrivate",
        0x0002
    ));
    // Nest-mateship keys on the HOST, not the simple name: two
    // same-simple-name nested classes of different hosts refuse.
    assert!(!ctor_access_allowed(
        &ctx,
        "pA/Holder$NestedPrivate",
        "pB/Caller$NestedPrivate",
        0x0002
    ));
    // The same class calling its own private ctor passes (its host
    // resolves to itself).
    assert!(ctor_access_allowed(
        &ctx,
        "pA/Private",
        "pA/Private",
        0x0002
    ));
    // S3/S4: protected, different package -> REFUSED even for the
    // subclass caller (newInstance passes obj=null).
    assert!(!ctor_access_allowed(&ctx, "pB/Caller", "pA/Prot", 0x0004));
    assert!(!ctor_access_allowed(&ctx, "pB/Sub", "pA/Prot", 0x0004));
    // protected, same package -> allowed.
    assert!(ctor_access_allowed(&ctx, "pA/Same", "pA/Prot", 0x0004));
    // S8: package-private (zero bits), different package -> REFUSED.
    assert!(!ctor_access_allowed(
        &ctx,
        "pB/Caller",
        "pA/Package",
        0x0000
    ));
    // package-private, same package -> allowed.
    assert!(ctor_access_allowed(&ctx, "pA/Same", "pA/Package", 0x0000));
    // S5: public, different package -> allowed.
    assert!(ctor_access_allowed(&ctx, "pB/Caller", "pA/Public", 0x0001));
}

#[test]
fn refusal_message_laws() {
    let _rt = crate::runtime();
    // The message reads OUTSIDE the with_obj closure — a nested
    // heap read on the same object deadlocks the heap lock (the
    // registered never-repeat rule).
    let msg = |t: Throw| {
        let slot = with_obj(t.obj, |o| o.slots[0].clone());
        match slot {
            Value::Ref(Some(r)) => crate::object::string_value(&r).to_string(),
            other => panic!("bad detailMessage slot {other:?}"),
        }
    };
    // Probe S1's exact bytes (dotted names both sides).
    assert_eq!(
        msg(throw_illegal_access("pB/Caller", "pA/Private", 0x0002)),
        "class pB.Caller cannot access a member of class pA.Private with modifiers \"private\""
    );
    // Probe S3/S4's protected word.
    assert_eq!(
        msg(throw_illegal_access("pB/Sub", "pA/Prot", 0x0004)),
        "class pB.Sub cannot access a member of class pA.Prot with modifiers \"protected\""
    );
    // Probe S8's package-access tail (the zero-bit set has NO
    // modifiers word).
    assert_eq!(
        msg(throw_illegal_access("pB/PkgCall", "pA/Package", 0x0000)),
        "class pB.PkgCall cannot access a member of class pA.Package with package access"
    );
}
