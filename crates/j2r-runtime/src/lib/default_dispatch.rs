use super::*;

fn body_a(_recv: Option<&JRef>, _args: &[Value]) -> Result<Value, Throw> {
    Ok(Value::Int(1))
}
fn body_b(_recv: Option<&JRef>, _args: &[Value]) -> Result<Value, Throw> {
    Ok(Value::Int(2))
}
fn body_top(_recv: Option<&JRef>, _args: &[Value]) -> Result<Value, Throw> {
    Ok(Value::Int(10))
}
fn body_mid(_recv: Option<&JRef>, _args: &[Value]) -> Result<Value, Throw> {
    Ok(Value::Int(20))
}
fn body_class(_recv: Option<&JRef>, _args: &[Value]) -> Result<Value, Throw> {
    Ok(Value::Int(7))
}

// The Probe5 shape: Impl implements WithDefault; the default twice()
// body carries sentinel 42.
fn p5_parents(class: &str) -> Option<&'static [&'static str]> {
    match class {
        "P5$Impl" => Some(&["java/lang/Object", "P5$WithDefault"]),
        "P5$WithDefault" => Some(&["java/lang/Object"]),
        _ => None,
    }
}
fn p5_lookup(class: &str, name: &str, desc: &str) -> Option<MethodFn> {
    match (class, name, desc) {
        ("P5$Impl", "m", "()I") => Some(body_class),
        ("P5$WithDefault", "twice", "()I") => Some(body_a),
        _ => None,
    }
}
const P5_IFACE: UserClassInfo = UserClassInfo {
    id: 1,
    is_interface: true,
    is_abstract: false,
    simple_name: Some("WithDefault"),
    nest_host: None,
    total_slots: 0,
    defaults_spec: "",
    fields: &[],
    methods: &[
        UserMethod {
            name: "m",
            desc: "()I",
            is_static: false,
            modifiers: 0x0401,
            is_native: false,
        },
        UserMethod {
            name: "twice",
            desc: "()I",
            is_static: false,
            modifiers: 0x0001,
            is_native: false,
        },
    ],
    ctors: &[],
    clinit: None,
};
const P5_IMPL: UserClassInfo = UserClassInfo {
    id: 2,
    is_interface: false,
    is_abstract: false,
    simple_name: Some("Impl"),
    nest_host: None,
    total_slots: 0,
    defaults_spec: "",
    fields: &[],
    methods: &[UserMethod {
        name: "m",
        desc: "()I",
        is_static: false,
        modifiers: 0x0001,
        is_native: false,
    }],
    ctors: &[],
    clinit: None,
};
fn p5_meta(class: &str) -> Option<&'static UserClassInfo> {
    match class {
        "P5$WithDefault" => Some(&P5_IFACE),
        "P5$Impl" => Some(&P5_IMPL),
        _ => None,
    }
}
fn p5_dispatch() -> Dispatch {
    let mut d = Dispatch::new(Some(p5_lookup), Some(p5_parents));
    d.meta = Some(p5_meta);
    d
}
fn recv_of(class: &str) -> JRef {
    crate::heap::alloc(JObj {
        jclass: Arc::from(class),
        slots: Vec::new(),
        prim: None,
    })
}

#[test]
fn single_default_dispatches() {
    let _rt = crate::runtime();
    let recv = recv_of("P5$Impl");
    let ctx = p5_dispatch();
    let v = invoke_virtual(&recv, "P5$WithDefault", "twice", "()I", &[], ctx)
        .expect("the interface default must run");
    assert_eq!(
        v.expect_int(),
        1,
        "the WithDefault default body (sentinel 1) runs"
    );
    // The concrete sibling still wins through the ordinary chain law.
    let v = invoke_virtual(&recv, "P5$WithDefault", "m", "()I", &[], ctx)
        .expect("the class method must run");
    assert_eq!(v.expect_int(), 7);
}

#[test]
fn most_specific_default_wins() {
    let _rt = crate::runtime();
    // Mid extends Top; BOTH declare the default; Impl implements Mid;
    // the site is declared on Top (the L2 probe shape). Mid is the
    // strict subtype candidate and dominates Top.
    fn parents(class: &str) -> Option<&'static [&'static str]> {
        match class {
            "MS$Impl" => Some(&["java/lang/Object", "MS$Mid"]),
            "MS$Mid" => Some(&["java/lang/Object", "MS$Top"]),
            "MS$Top" => Some(&["java/lang/Object"]),
            _ => None,
        }
    }
    fn lookup(class: &str, name: &str, desc: &str) -> Option<MethodFn> {
        match (class, name, desc) {
            ("MS$Top", "t", "()I") => Some(body_top),
            ("MS$Mid", "t", "()I") => Some(body_mid),
            _ => None,
        }
    }
    const TOP: UserClassInfo = UserClassInfo {
        id: 3,
        is_interface: true,
        is_abstract: false,
        simple_name: Some("Top"),
        nest_host: None,
        total_slots: 0,
        defaults_spec: "",
        fields: &[],
        methods: &[UserMethod {
            name: "t",
            desc: "()I",
            is_static: false,
            modifiers: 0x0001,
            is_native: false,
        }],
        ctors: &[],
        clinit: None,
    };
    const MID: UserClassInfo = UserClassInfo {
        id: 4,
        is_interface: true,
        is_abstract: false,
        simple_name: Some("Mid"),
        nest_host: None,
        total_slots: 0,
        defaults_spec: "",
        fields: &[],
        methods: &[UserMethod {
            name: "t",
            desc: "()I",
            is_static: false,
            modifiers: 0x0001,
            is_native: false,
        }],
        ctors: &[],
        clinit: None,
    };
    const IMPL: UserClassInfo = UserClassInfo {
        id: 5,
        is_interface: false,
        is_abstract: false,
        simple_name: Some("Impl"),
        nest_host: None,
        total_slots: 0,
        defaults_spec: "",
        fields: &[],
        methods: &[],
        ctors: &[],
        clinit: None,
    };
    fn meta(class: &str) -> Option<&'static UserClassInfo> {
        match class {
            "MS$Top" => Some(&TOP),
            "MS$Mid" => Some(&MID),
            "MS$Impl" => Some(&IMPL),
            _ => None,
        }
    }
    let mut ctx = Dispatch::new(Some(lookup), Some(parents));
    ctx.meta = Some(meta);
    let recv = recv_of("MS$Impl");
    let v = invoke_virtual(&recv, "MS$Top", "t", "()I", &[], ctx)
        .expect("the most-specific default must run");
    assert_eq!(
        v.expect_int(),
        20,
        "Mid's body (sentinel 20) dominates Top's"
    );
}

#[test]
fn superinterface_default_reached_through_non_redeclaring_sub() {
    let _rt = crate::runtime();
    // Mid2 extends Top2 but does NOT redeclare; Impl implements Mid2;
    // the site is declared on Mid2 (the L4 probe shape).
    fn parents(class: &str) -> Option<&'static [&'static str]> {
        match class {
            "SI$Impl" => Some(&["java/lang/Object", "SI$Mid2"]),
            "SI$Mid2" => Some(&["java/lang/Object", "SI$Top2"]),
            "SI$Top2" => Some(&["java/lang/Object"]),
            _ => None,
        }
    }
    fn lookup(class: &str, name: &str, desc: &str) -> Option<MethodFn> {
        match (class, name, desc) {
            ("SI$Top2", "t2", "()I") => Some(body_top),
            _ => None,
        }
    }
    const TOP2: UserClassInfo = UserClassInfo {
        id: 6,
        is_interface: true,
        is_abstract: false,
        simple_name: Some("Top2"),
        nest_host: None,
        total_slots: 0,
        defaults_spec: "",
        fields: &[],
        methods: &[UserMethod {
            name: "t2",
            desc: "()I",
            is_static: false,
            modifiers: 0x0001,
            is_native: false,
        }],
        ctors: &[],
        clinit: None,
    };
    const MID2: UserClassInfo = UserClassInfo {
        id: 7,
        is_interface: true,
        is_abstract: true,
        simple_name: Some("Mid2"),
        nest_host: None,
        total_slots: 0,
        defaults_spec: "",
        fields: &[],
        methods: &[UserMethod {
            name: "t2",
            desc: "()I",
            is_static: false,
            modifiers: 0x0401,
            is_native: false,
        }],
        ctors: &[],
        clinit: None,
    };
    const IMPL2: UserClassInfo = UserClassInfo {
        id: 8,
        is_interface: false,
        is_abstract: false,
        simple_name: Some("Impl"),
        nest_host: None,
        total_slots: 0,
        defaults_spec: "",
        fields: &[],
        methods: &[],
        ctors: &[],
        clinit: None,
    };
    fn meta(class: &str) -> Option<&'static UserClassInfo> {
        match class {
            "SI$Top2" => Some(&TOP2),
            "SI$Mid2" => Some(&MID2),
            "SI$Impl" => Some(&IMPL2),
            _ => None,
        }
    }
    let mut ctx = Dispatch::new(Some(lookup), Some(parents));
    ctx.meta = Some(meta);
    let recv = recv_of("SI$Impl");
    let v = invoke_virtual(&recv, "SI$Mid2", "t2", "()I", &[], ctx)
        .expect("the super-interface default must run through the closure");
    assert_eq!(v.expect_int(), 10, "Top2's body (sentinel 10) runs");
}

#[test]
fn diamond_conflict_is_loud_icerror() {
    let _rt = crate::runtime();
    // Two UNRELATED defaults — javac refuses this at compile time, so
    // the shape only arrives in a hand-crafted classfile; the law is
    // the loud IncompatibleClassChangeError, never a silent pick.
    fn parents(class: &str) -> Option<&'static [&'static str]> {
        match class {
            "DD$Impl" => Some(&["java/lang/Object", "DD$DA", "DD$DB"]),
            "DD$DA" => Some(&["java/lang/Object"]),
            "DD$DB" => Some(&["java/lang/Object"]),
            _ => None,
        }
    }
    fn lookup(class: &str, name: &str, desc: &str) -> Option<MethodFn> {
        match (class, name, desc) {
            ("DD$DA", "d", "()I") => Some(body_a),
            ("DD$DB", "d", "()I") => Some(body_b),
            _ => None,
        }
    }
    const DA: UserClassInfo = UserClassInfo {
        id: 9,
        is_interface: true,
        is_abstract: false,
        simple_name: Some("DA"),
        nest_host: None,
        total_slots: 0,
        defaults_spec: "",
        fields: &[],
        methods: &[UserMethod {
            name: "d",
            desc: "()I",
            is_static: false,
            modifiers: 0x0001,
            is_native: false,
        }],
        ctors: &[],
        clinit: None,
    };
    const DB: UserClassInfo = UserClassInfo {
        id: 10,
        is_interface: true,
        is_abstract: false,
        simple_name: Some("DB"),
        nest_host: None,
        total_slots: 0,
        defaults_spec: "",
        fields: &[],
        methods: &[UserMethod {
            name: "d",
            desc: "()I",
            is_static: false,
            modifiers: 0x0001,
            is_native: false,
        }],
        ctors: &[],
        clinit: None,
    };
    const IMPL3: UserClassInfo = UserClassInfo {
        id: 11,
        is_interface: false,
        is_abstract: false,
        simple_name: Some("Impl"),
        nest_host: None,
        total_slots: 0,
        defaults_spec: "",
        fields: &[],
        methods: &[],
        ctors: &[],
        clinit: None,
    };
    fn meta(class: &str) -> Option<&'static UserClassInfo> {
        match class {
            "DD$DA" => Some(&DA),
            "DD$DB" => Some(&DB),
            "DD$Impl" => Some(&IMPL3),
            _ => None,
        }
    }
    let mut ctx = Dispatch::new(Some(lookup), Some(parents));
    ctx.meta = Some(meta);
    let recv = recv_of("DD$Impl");
    let err = invoke_virtual(&recv, "DD$DA", "d", "()I", &[], ctx)
        .expect_err("the diamond conflict must stay loud");
    assert_eq!(err.java_class(), "java/lang/IncompatibleClassChangeError");
    let msg = crate::heap::with_obj(err.obj, |o| o.slots[0].clone());
    match msg {
        Value::Ref(Some(s)) => {
            let m = object::string_value(&s).to_string();
            assert!(
                m.contains("DD$DA") && m.contains("DD$DB"),
                "the message names both conflicting interfaces: {m}"
            );
        }
        other => panic!("unexpected message {other:?}"),
    }
}

#[test]
fn no_law_change_when_nothing_matches() {
    let _rt = crate::runtime();
    // No interface declares the method: the loud internal Error stays
    // the fallthrough (never a silent null).
    let ctx = p5_dispatch();
    let recv = recv_of("P5$Impl");
    let err = invoke_virtual(&recv, "P5$WithDefault", "absent", "()I", &[], ctx)
        .expect_err("an absent method must stay loud");
    assert_eq!(err.java_class(), "java/lang/Error");
}
