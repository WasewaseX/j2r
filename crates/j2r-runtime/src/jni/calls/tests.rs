use super::*;

/// The loud-abort guard shared by this file's panic tests: run the
/// closure, catch the unwind, report whether it panicked.
fn catch_panicked<F: FnOnce()>(f: F) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err()
}

#[test]
fn jvalue_words_decode_per_descriptor() {
    // Int-group sub-word kinds ride the LOW bytes (little-endian);
    // F occupies the low 4 bytes of its 8-byte slot.
    let mut w = [0u8; 8];
    w[0] = 1;
    assert_eq!(decode_jvalue(&w, &TypeDesc::Boolean), Value::Int(1));
    w[0] = 0xFB; // -5 as i8
    assert_eq!(decode_jvalue(&w, &TypeDesc::Byte), Value::Int(-5));
    w[0..2].copy_from_slice(&0xFFD6_u16.to_le_bytes()); // -42 as i16
    assert_eq!(decode_jvalue(&w, &TypeDesc::Short), Value::Int(-42));
    w[0..2].copy_from_slice(&0x263A_u16.to_le_bytes());
    assert_eq!(decode_jvalue(&w, &TypeDesc::Char), Value::Int(0x263A));
    w[0..4].copy_from_slice(&(-42_i32).to_le_bytes());
    assert_eq!(decode_jvalue(&w, &TypeDesc::Int), Value::Int(-42));
    w.copy_from_slice(&(-42_i64).to_le_bytes());
    assert_eq!(decode_jvalue(&w, &TypeDesc::Long), Value::Long(-42));
    w[0..4].copy_from_slice(&1.5_f32.to_le_bytes());
    assert_eq!(decode_jvalue(&w, &TypeDesc::Float), Value::Float(1.5));
    w.copy_from_slice(&2.5_f64.to_le_bytes());
    assert_eq!(decode_jvalue(&w, &TypeDesc::Double), Value::Double(2.5));
    // reference: the pointer word IS the tag; 0 = null
    assert_eq!(
        decode_jvalue(&[0; 8], &TypeDesc::Object("x".into())),
        Value::Ref(None)
    );
    let _rt = crate::runtime();
    let tag = super::super::bridge::new_local(crate::object::new_string("hi"));
    let mut ptr_word = [0u8; 8];
    ptr_word.copy_from_slice(&tag.to_le_bytes());
    match decode_jvalue(&ptr_word, &TypeDesc::Object("x".into())) {
        Value::Ref(Some(r)) => assert_eq!(
            &*crate::object::string_value(&r),
            "hi",
            "the tag crosses into a real object"
        ),
        other => panic!("expected ref, got {other:?}"),
    }
    delete_local(tag);
}

#[test]
fn zero_values_match_the_return_descriptor() {
    assert_eq!(zero_value(&TypeDesc::Int), Value::Int(0));
    assert_eq!(zero_value(&TypeDesc::Long), Value::Long(0));
    assert_eq!(zero_value(&TypeDesc::Float), Value::Float(0.0));
    assert_eq!(zero_value(&TypeDesc::Double), Value::Double(0.0));
    assert_eq!(zero_value(&TypeDesc::Object("x".into())), Value::Ref(None));
    assert_eq!(zero_value(&TypeDesc::Void), Value::Ref(None));
}

#[test]
fn method_ids_are_deduplicated_marked_and_disjoint_from_field_ids() {
    let _rt = crate::runtime();
    let id1 = super::super::classes::mint_for_test("a/B", "foo", "(I)V", false);
    let id2 = super::super::classes::mint_for_test("a/B", "foo", "(I)V", false);
    assert_eq!(id1, id2, "the same triple mints the same ID forever");
    let resolved = super::super::classes::resolve_method_id(id1);
    assert_eq!(resolved.owner, "a/B");
    assert_eq!(resolved.name, "foo");
    assert_eq!(resolved.sig, "(I)V");
    assert!(!resolved.is_static);
    // the mark is bit 62 — a FIELD id (bit 63) is loud at resolve
    let panicked =
        std::panic::catch_unwind(|| super::super::classes::resolve_method_id(1 << 63 | 1)).is_err();
    assert!(panicked, "a field ID passed as a method ID must be loud");
    // an unmarked foreign pointer is loud too
    let panicked =
        std::panic::catch_unwind(|| super::super::classes::resolve_method_id(0xBEEF)).is_err();
    assert!(panicked, "a foreign jmethodID must be loud");
}

#[test]
fn pending_exception_cell_roundtrips_per_thread() {
    let _rt = crate::runtime();
    assert!(!super::super::bridge::has_pending_exception());
    assert_eq!(super::super::bridge::pending_exception_local(), None);
    let t = crate::throw_new("java/lang/RuntimeException", Some("boom"));
    super::super::bridge::pend_exception(t);
    assert!(super::super::bridge::has_pending_exception());
    // ExceptionOccurred is REPEATABLE: fresh local ref each call,
    // cell untouched
    let l1 = super::super::bridge::pending_exception_local().expect("pending");
    let l2 = super::super::bridge::pending_exception_local().expect("still pending");
    assert_ne!(l1, l2, "each ExceptionOccurred mints a FRESH local ref");
    assert!(super::super::bridge::has_pending_exception());
    // ExceptionClear takes it away
    let taken = super::super::bridge::take_pending_exception().expect("cleared");
    assert_eq!(
        crate::heap::with_obj(taken.obj, |o| o.jclass.to_string()),
        "java/lang/RuntimeException"
    );
    assert!(!super::super::bridge::has_pending_exception());
    assert!(super::super::bridge::take_pending_exception().is_none());
    delete_local(l1);
    delete_local(l2);
}

// -- the mock-dispatch fixture (M5_JNI_CALLS.md §4/§5) --------------
//
// Base declares mark()I = 1, twice(I)I = arg*2, sm()I static = 10,
// sq(I)I static = arg², boom()I = throws, mk()Ljava/lang/String;
// static = a fresh mirror; Derived overrides mark()I = 2 and
// inherits everything else. The lookup matches per DECLARING class —
// exactly the generated registry's shape — so a receiver whose
// runtime class is Derived only reaches Base's inherited bodies
// through the ancestor walk, which IS the resolution under test.
// Every test takes `runtime()` FIRST (the global RUNTIME_LOCK) and
// keeps the guard alive: the host dispatch is process-global and the
// classes.rs fixture must not race this one.

static BASE_METHODS: [crate::UserMethod; 9] = [
    crate::UserMethod {
        name: "mark",
        desc: "()I",
        is_static: false,
        modifiers: 0,
        is_native: false,
    },
    crate::UserMethod {
        name: "twice",
        desc: "(I)I",
        is_static: false,
        modifiers: 0,
        is_native: false,
    },
    crate::UserMethod {
        name: "sm",
        desc: "()I",
        is_static: true,
        modifiers: 0,
        is_native: false,
    },
    crate::UserMethod {
        name: "sq",
        desc: "(I)I",
        is_static: true,
        modifiers: 0,
        is_native: false,
    },
    crate::UserMethod {
        name: "boom",
        desc: "()I",
        is_static: false,
        modifiers: 0,
        is_native: false,
    },
    crate::UserMethod {
        name: "mk",
        desc: "()Ljava/lang/String;",
        is_static: true,
        modifiers: 0,
        is_native: false,
    },
    crate::UserMethod {
        name: "sum3",
        desc: "(III)I",
        is_static: true,
        modifiers: 0,
        is_native: false,
    },
    crate::UserMethod {
        name: "addD",
        desc: "(DD)D",
        is_static: true,
        modifiers: 0,
        is_native: false,
    },
    crate::UserMethod {
        name: "mix",
        desc: "(ID)I",
        is_static: false,
        modifiers: 0,
        is_native: false,
    },
];
static DERIVED_METHODS: [crate::UserMethod; 1] = [crate::UserMethod {
    name: "mark",
    desc: "()I",
    is_static: false,
    modifiers: 0,
    is_native: false,
}];
static CALL_DERIVED_PARENTS: [&str; 1] = ["j2r/test/CallBase"];

fn base_mark(_recv: Option<&crate::heap::JRef>, _args: &[Value]) -> Result<Value, crate::Throw> {
    Ok(Value::Int(1))
}

fn derived_mark(_recv: Option<&crate::heap::JRef>, _args: &[Value]) -> Result<Value, crate::Throw> {
    Ok(Value::Int(2))
}

fn base_twice(_recv: Option<&crate::heap::JRef>, args: &[Value]) -> Result<Value, crate::Throw> {
    match args.first() {
        Some(Value::Int(i)) => Ok(Value::Int(i * 2)),
        other => panic!("fixture twice expects one Int arg, got {other:?}"),
    }
}

fn base_sm(_recv: Option<&crate::heap::JRef>, _args: &[Value]) -> Result<Value, crate::Throw> {
    Ok(Value::Int(10))
}

fn base_sq(_recv: Option<&crate::heap::JRef>, args: &[Value]) -> Result<Value, crate::Throw> {
    match args.first() {
        Some(Value::Int(i)) => Ok(Value::Int(i * i)),
        other => panic!("fixture sq expects one Int arg, got {other:?}"),
    }
}

fn base_boom(_recv: Option<&crate::heap::JRef>, _args: &[Value]) -> Result<Value, crate::Throw> {
    Err(crate::throw_new(
        "java/lang/RuntimeException",
        Some("callee threw"),
    ))
}

fn base_mk(_recv: Option<&crate::heap::JRef>, _args: &[Value]) -> Result<Value, crate::Throw> {
    Ok(Value::Ref(Some(crate::object::new_string("from static"))))
}

fn base_sum3(_recv: Option<&crate::heap::JRef>, args: &[Value]) -> Result<Value, crate::Throw> {
    let mut sum = 0;
    for a in args {
        match a {
            Value::Int(i) => sum += i,
            other => panic!("fixture sum3 expects Int args, got {other:?}"),
        }
    }
    Ok(Value::Int(sum))
}

fn base_add_d(_recv: Option<&crate::heap::JRef>, args: &[Value]) -> Result<Value, crate::Throw> {
    match (args.first(), args.get(1)) {
        (Some(Value::Double(a)), Some(Value::Double(b))) => Ok(Value::Double(a + b)),
        other => panic!("fixture addD expects two Double args, got {other:?}"),
    }
}

fn base_mix(_recv: Option<&crate::heap::JRef>, args: &[Value]) -> Result<Value, crate::Throw> {
    match (args.first(), args.get(1)) {
        (Some(Value::Int(i)), Some(Value::Double(d))) => Ok(Value::Int(i * 100 + *d as i32)),
        other => panic!("fixture mix expects (Int, Double), got {other:?}"),
    }
}

fn dispatch_lookup(class: &str, name: &str, desc: &str) -> Option<crate::MethodFn> {
    match (class, name, desc) {
        ("j2r/test/CallBase", "mark", "()I") => Some(base_mark),
        ("j2r/test/CallBase", "twice", "(I)I") => Some(base_twice),
        ("j2r/test/CallBase", "sm", "()I") => Some(base_sm),
        ("j2r/test/CallBase", "sq", "(I)I") => Some(base_sq),
        ("j2r/test/CallBase", "boom", "()I") => Some(base_boom),
        ("j2r/test/CallBase", "mk", "()Ljava/lang/String;") => Some(base_mk),
        ("j2r/test/CallBase", "sum3", "(III)I") => Some(base_sum3),
        ("j2r/test/CallBase", "addD", "(DD)D") => Some(base_add_d),
        ("j2r/test/CallBase", "mix", "(ID)I") => Some(base_mix),
        ("j2r/test/CallDerived", "mark", "()I") => Some(derived_mark),
        _ => None,
    }
}

fn meta_of(c: &str) -> Option<&'static crate::UserClassInfo> {
    static BASE_INFO: crate::UserClassInfo = crate::UserClassInfo {
        id: 0,
        clinit: None,
        is_abstract: false,
        total_slots: 0,
        defaults_spec: "",
        is_interface: false,
        simple_name: Some("CallBase"),
        nest_host: None,
        fields: &[],
        methods: &BASE_METHODS,
        ctors: &[],
    };
    static DERIVED_INFO: crate::UserClassInfo = crate::UserClassInfo {
        id: 0,
        clinit: None,
        is_abstract: false,
        total_slots: 0,
        defaults_spec: "",
        is_interface: false,
        simple_name: Some("CallDerived"),
        nest_host: None,
        fields: &[],
        methods: &DERIVED_METHODS,
        ctors: &[],
    };
    match c {
        "j2r/test/CallBase" => Some(&BASE_INFO),
        "j2r/test/CallDerived" => Some(&DERIVED_INFO),
        _ => None,
    }
}

fn parents_of(c: &str) -> Option<&'static [&'static str]> {
    if c == "j2r/test/CallDerived" {
        Some(&CALL_DERIVED_PARENTS)
    } else {
        None
    }
}

fn install_dispatch_fixture() {
    super::super::install_host_dispatch(crate::Dispatch {
        lookup: Some(dispatch_lookup),
        parents: Some(parents_of),
        meta: Some(meta_of),
        static_get: None,
        static_set: None,
    });
}

fn class_tag(internal: &str) -> u64 {
    super::super::bridge::new_local(crate::reflect::class_ref(internal))
}

fn derived_receiver() -> u64 {
    super::super::bridge::new_local(crate::heap::alloc(crate::object::JObj {
        jclass: std::sync::Arc::from("j2r/test/CallDerived"),
        slots: vec![],
        prim: None,
    }))
}

fn int_arg_word(v: i32) -> [u8; 8] {
    // The jvalue union slot carries sub-word kinds in the LOW bytes
    // (little-endian) — exactly what decode_jvalue reads for Int.
    let mut w = [0u8; 8];
    w[0..4].copy_from_slice(&v.to_le_bytes());
    w
}

fn base_receiver() -> u64 {
    super::super::bridge::new_local(crate::heap::alloc(crate::object::JObj {
        jclass: std::sync::Arc::from("j2r/test/CallBase"),
        slots: vec![],
        prim: None,
    }))
}

fn clear_pending() {
    super::super::bridge::take_pending_exception();
}

/// A hand-built SysV V-form record set (the 11c acceptance: the walk
/// tested against records with KNOWN layouts). `consumed_gp`/
/// `consumed_fp` = the named-argument slots the (simulated) foreign
/// variadic caller already burned; `gp_words`/`fp_words` = the vararg
/// payload; `overflow_words` rides the overflow (stack) area. All
/// three buffers are boxed and kept together so every address pinned
/// in the record outlives the call.
struct VFormArea {
    rec: Box<[u8; 24]>,
    _save: Box<[u8]>,
    _ovf: Box<[u8]>,
}

impl VFormArea {
    /// The `va_list` value: the record's address (what a C caller
    /// passes as the ap argument).
    fn ap(&self) -> usize {
        self.rec.as_ptr() as usize
    }
}

fn build_save_area(
    consumed_gp: usize,
    consumed_fp: usize,
    gp_words: &[u64],
    fp_words: &[f64],
    overflow_words: &[u64],
) -> VFormArea {
    let mut save = vec![0u8; 48 + 8 * 16].into_boxed_slice();
    let mut gp = consumed_gp * 8;
    for w in gp_words {
        save[gp..gp + 8].copy_from_slice(&w.to_le_bytes());
        gp += 8;
    }
    let mut fp = 48 + consumed_fp * 16;
    for v in fp_words {
        save[fp..fp + 8].copy_from_slice(&v.to_le_bytes());
        fp += 16;
    }
    let mut ovf = vec![0u8; overflow_words.len() * 8 + 8].into_boxed_slice();
    for (i, w) in overflow_words.iter().enumerate() {
        ovf[i * 8..i * 8 + 8].copy_from_slice(&w.to_le_bytes());
    }
    let mut rec = Box::new([0u8; 24]);
    rec[0..4].copy_from_slice(&((consumed_gp * 8) as u32).to_le_bytes());
    rec[4..8].copy_from_slice(&((48 + consumed_fp * 16) as u32).to_le_bytes());
    rec[8..16].copy_from_slice(&(ovf.as_ptr() as usize).to_le_bytes());
    rec[16..24].copy_from_slice(&(save.as_ptr() as usize).to_le_bytes());
    VFormArea {
        rec,
        _save: save,
        _ovf: ovf,
    }
}

#[test]
fn virtual_calls_dispatch_by_the_runtime_class_and_walk_ancestors() {
    let _rt = crate::runtime();
    install_dispatch_fixture();
    let recv = derived_receiver();
    let base_cls = class_tag(CALL_BASE);
    let derived_cls = class_tag("j2r/test/CallDerived");
    // The override: mark()I minted on Derived runs Derived's body.
    let m_derived = super::super::classes::slot_get_method_id(derived_cls, "mark", "()I", false)
        .expect("Derived declares mark");
    assert_eq!(
        super::super::classes::resolve_method_id(m_derived).owner,
        "j2r/test/CallDerived"
    );
    assert_eq!(
        slot_call_int_method_a(m_derived, recv, 0),
        2,
        "the receiver's runtime class decides — the override runs"
    );
    // THE override-vs-declared-class proof: the SAME call shape with
    // an ID minted on Base must STILL run the override (virtual law),
    // never Base's body.
    let m_base = super::super::classes::slot_get_method_id(base_cls, "mark", "()I", false)
        .expect("Base declares mark");
    assert_eq!(
        super::super::classes::resolve_method_id(m_base).owner,
        "j2r/test/CallBase"
    );
    assert_eq!(
        slot_call_int_method_a(m_base, recv, 0),
        2,
        "virtual dispatch ignores the ID's declaring class — the \
         receiver's override wins"
    );
    // The ancestor walk: twice(I)I is declared on Base only; the ID
    // minted through Derived must resolve through the walk and reach
    // Base's body with the decoded argument.
    let m_twice = super::super::classes::slot_get_method_id(derived_cls, "twice", "(I)I", false)
        .expect("the walk finds twice on Base");
    assert_eq!(
        super::super::classes::resolve_method_id(m_twice).owner,
        "j2r/test/CallBase",
        "Get*MethodID mints the ID against the DECLARING class"
    );
    let word = int_arg_word(21);
    assert_eq!(
        slot_call_int_method_a(m_twice, recv, word.as_ptr() as usize),
        42,
        "the runtime-class walk reaches the inherited body"
    );
    clear_pending();
    delete_local(recv);
    delete_local(base_cls);
    delete_local(derived_cls);
}

#[test]
fn nonvirtual_calls_dispatch_by_the_declared_class() {
    let _rt = crate::runtime();
    install_dispatch_fixture();
    let recv = derived_receiver(); // a Derived INSTANCE that overrides mark
    let base_cls = class_tag(CALL_BASE);
    let derived_cls = class_tag("j2r/test/CallDerived");
    let m_base = super::super::classes::slot_get_method_id(base_cls, "mark", "()I", false)
        .expect("Base declares mark");
    // THE super.m() law — the mirror image of the virtual proof:
    // same receiver (Derived, overriding), same Base-minted ID, but
    // the declared class is Base → Base's body (1), NOT the override.
    assert_eq!(
        slot_call_nonvirtual_int_method_a(m_base, recv, base_cls, 0),
        1,
        "nonvirtual dispatch obeys the DECLARED class, not the \
         receiver's runtime class"
    );
    // The declared class is Derived (its own override) → 2.
    let m_derived = super::super::classes::slot_get_method_id(derived_cls, "mark", "()I", false)
        .expect("Derived declares mark");
    assert_eq!(
        slot_call_nonvirtual_int_method_a(m_derived, recv, derived_cls, 0),
        2,
        "nonvirtual on the overriding declared class runs the override"
    );
    // The ancestor case: an ID minted on Base for an inherited
    // method, called nonvirtual with clazz=Derived — the declared-
    // class walk lands on Base's body (the JNI spec ties the ID to
    // the jclass, so this is the same body the ID names).
    let m_twice = super::super::classes::slot_get_method_id(derived_cls, "twice", "(I)I", false)
        .expect("the walk finds twice on Base");
    let word = int_arg_word(-4);
    assert_eq!(
        slot_call_nonvirtual_int_method_a(m_twice, recv, derived_cls, word.as_ptr() as usize),
        -8,
        "the declared-class walk covers the ancestors"
    );
    clear_pending();
    delete_local(recv);
    delete_local(base_cls);
    delete_local(derived_cls);
}

#[test]
fn static_calls_resolve_on_the_declared_class_no_receiver() {
    let _rt = crate::runtime();
    install_dispatch_fixture();
    let base_cls = class_tag(CALL_BASE);
    let derived_cls = class_tag("j2r/test/CallDerived");
    let m_sm = super::super::classes::slot_get_method_id(base_cls, "sm", "()I", true)
        .expect("Base declares static sm");
    // Static body runs with NO receiver and NO receiver effects.
    assert_eq!(
        slot_call_static_int_method_a(m_sm, base_cls, 0),
        10,
        "the static family resolves on the declared class"
    );
    // The same ID through the SUBclass: the declared-class walk
    // covers the ancestors (faithful static resolution).
    assert_eq!(
        slot_call_static_int_method_a(m_sm, derived_cls, 0),
        10,
        "the static walk resolves through the subclass"
    );
    // Arguments cross (static + descriptor-driven decode): sq(I)I.
    let m_sq = super::super::classes::slot_get_method_id(base_cls, "sq", "(I)I", true)
        .expect("Base declares static sq");
    let word = int_arg_word(7);
    assert_eq!(
        slot_call_static_int_method_a(m_sq, base_cls, word.as_ptr() as usize),
        49,
        "jvalue args decode per the descriptor and reach the callee"
    );
    // The Object kind on the static family: the answer is a rooted
    // local-ref tag carrying the mirror.
    let m_mk =
        super::super::classes::slot_get_method_id(base_cls, "mk", "()Ljava/lang/String;", true)
            .expect("Base declares static mk");
    let tag = slot_call_static_object_method_a(m_mk, base_cls, 0);
    assert_ne!(tag, 0, "a non-null answer is a live local-ref tag");
    let r = super::super::bridge::resolve(tag);
    assert_eq!(&*crate::object::string_value(&r), "from static");
    delete_local(tag);
    clear_pending();
    delete_local(base_cls);
    delete_local(derived_cls);
}

#[test]
fn family_staticness_mismatches_are_loud() {
    let _rt = crate::runtime();
    install_dispatch_fixture();
    let base_cls = class_tag(CALL_BASE);
    let instance_id = super::super::classes::slot_get_method_id(base_cls, "mark", "()I", false)
        .expect("instance mark");
    let static_id =
        super::super::classes::slot_get_method_id(base_cls, "sm", "()I", true).expect("static sm");
    // instance ID through the static family → loud (Temurin: UB)
    let panicked = catch_panicked(|| {
        let _ = slot_call_static_int_method_a(instance_id, base_cls, 0);
    });
    assert!(panicked, "an instance ID in the static family must be loud");
    // static ID through the virtual family → loud
    let panicked = catch_panicked(|| {
        let _ = slot_call_int_method_a(static_id, 1, 0);
    });
    assert!(panicked, "a static ID in the virtual family must be loud");
    // static ID through the nonvirtual family → loud
    let panicked = catch_panicked(|| {
        let _ = slot_call_nonvirtual_int_method_a(static_id, 1, base_cls, 0);
    });
    assert!(
        panicked,
        "a static ID in the nonvirtual family must be loud"
    );
    clear_pending();
    delete_local(base_cls);
}

#[test]
fn null_receiver_null_class_null_args_pend_npe_and_answer_zero() {
    let _rt = crate::runtime();
    install_dispatch_fixture();
    let base_cls = class_tag(CALL_BASE);
    let m_mark = super::super::classes::slot_get_method_id(base_cls, "mark", "()I", false)
        .expect("instance mark");
    let m_twice = super::super::classes::slot_get_method_id(base_cls, "twice", "(I)I", false)
        .expect("instance twice");
    let m_sm =
        super::super::classes::slot_get_method_id(base_cls, "sm", "()I", true).expect("static sm");
    let recv = base_receiver();
    // null receiver, virtual family → zero + pending NPE
    assert_eq!(slot_call_int_method_a(m_mark, 0, 0), 0, "zero return");
    assert!(super::super::bridge::has_pending_exception());
    let l = super::super::bridge::pending_exception_local().expect("pending");
    let r = super::super::bridge::resolve(l);
    assert_eq!(
        crate::heap::with_obj(r, |o| o.jclass.to_string()),
        "java/lang/NullPointerException"
    );
    delete_local(l);
    clear_pending();
    // null receiver, nonvirtual family → same law
    assert_eq!(slot_call_nonvirtual_int_method_a(m_mark, 0, base_cls, 0), 0);
    assert!(super::super::bridge::has_pending_exception());
    clear_pending();
    // null jclass, static family → same law (defensive; Temurin: UB)
    assert_eq!(slot_call_static_int_method_a(m_sm, 0, 0), 0);
    assert!(super::super::bridge::has_pending_exception());
    clear_pending();
    // null jclass, nonvirtual family → same law (a LIVE receiver so
    // the declared-class check is what fires)
    assert_eq!(slot_call_nonvirtual_int_method_a(m_mark, recv, 0, 0), 0);
    assert!(super::super::bridge::has_pending_exception());
    clear_pending();
    // null jvalue* with a demanding descriptor → same law (virtual)
    assert_eq!(slot_call_int_method_a(m_twice, recv, 0), 0);
    assert!(super::super::bridge::has_pending_exception());
    clear_pending();
    // …and the static family agrees
    assert_eq!(
        slot_call_static_int_method_a(m_sm, base_cls, 0),
        10,
        "a valid static call is unaffected"
    );
    delete_local(recv);
    delete_local(base_cls);
}

#[test]
fn a_thrown_callee_pends_and_answers_the_zero_value() {
    let _rt = crate::runtime();
    install_dispatch_fixture();
    let base_cls = class_tag(CALL_BASE);
    let recv = super::super::bridge::new_local(crate::heap::alloc(crate::object::JObj {
        jclass: std::sync::Arc::from("j2r/test/CallBase"),
        slots: vec![],
        prim: None,
    }));
    let m_boom = super::super::classes::slot_get_method_id(base_cls, "boom", "()I", false)
        .expect("Base declares boom");
    // The callee's Err is NOT propagated as a panic: the call answers
    // the zero value and the throwable waits in the cell.
    assert_eq!(slot_call_int_method_a(m_boom, recv, 0), 0, "zero return");
    let t = super::super::bridge::take_pending_exception().expect("pending throwable");
    assert_eq!(
        crate::heap::with_obj(t.obj, |o| o.jclass.to_string()),
        "java/lang/RuntimeException",
        "the callee's throwable is the pending one"
    );
    assert!(
        !super::super::bridge::has_pending_exception(),
        "take clears"
    );
    // The nonvirtual family obeys the same law (same core).
    assert_eq!(
        slot_call_nonvirtual_int_method_a(m_boom, recv, base_cls, 0),
        0
    );
    let t = super::super::bridge::take_pending_exception().expect("pending again");
    assert_eq!(
        crate::heap::with_obj(t.obj, |o| o.jclass.to_string()),
        "java/lang/RuntimeException"
    );
    delete_local(recv);
    delete_local(base_cls);
}

// -- the V-form save-area walk (11c) --------------------------------

#[test]
fn vform_walk_reads_integer_save_areas() {
    let _rt = crate::runtime();
    install_dispatch_fixture();
    let base_cls = class_tag(CALL_BASE);
    let m_sum3 = super::super::classes::slot_get_method_id(base_cls, "sum3", "(III)I", true)
        .expect("static sum3");
    // A realistic static-V record: a 3-word payload needs all three
    // GP register slots, so the walk starts at gp_offset 16 (the
    // simulated foreign caller burned 2 named slots): words at
    // save+16/24/32. The register→overflow split is its own test.
    let area = build_save_area(2, 0, &[3, 4, 5], &[], &[]);
    let ap = area.ap();
    assert_eq!(
        slot_call_static_int_method_v(m_sum3, base_cls, ap),
        12,
        "three GP words walk the save area and reach the callee"
    );
    // The Object kind on the V-form static family: the answer is a
    // rooted local-ref tag (mk takes NO args — nothing to read).
    let m_mk =
        super::super::classes::slot_get_method_id(base_cls, "mk", "()Ljava/lang/String;", true)
            .expect("static mk");
    let tag = slot_call_static_object_method_v(m_mk, base_cls, ap);
    assert_ne!(tag, 0, "a zero-arg V-call needs no walk at all");
    let r = super::super::bridge::resolve(tag);
    assert_eq!(&*crate::object::string_value(&r), "from static");
    delete_local(tag);
    clear_pending();
    delete_local(base_cls);
}

#[test]
fn vform_walk_reads_fp_and_mixed_save_areas() {
    let _rt = crate::runtime();
    install_dispatch_fixture();
    let base_cls = class_tag(CALL_BASE);
    let recv = base_receiver();
    // FP-only: addD(DD)D — two SSE-class words at fp_offset 48 and 64
    // (a static V-call burns ZERO fp slots for its named args: they
    // are all GP). The 16-byte slot stride is the ABI law.
    let m_add_d = super::super::classes::slot_get_method_id(base_cls, "addD", "(DD)D", true)
        .expect("static addD");
    let area = build_save_area(4, 0, &[], &[1.5, 2.25], &[]);
    let ap = area.ap();
    match slot_call_static_double_method_v(m_add_d, base_cls, ap) {
        d if (d - 3.75).abs() < 1e-9 => {}
        other => panic!("expected 3.75, got {other:?}"),
    }
    // MIXED: mix(I D)I — the Int rides GP (gp_offset 32), the Double
    // rides the FIRST FP slot (fp_offset 48); each family's cursor
    // advances independently.
    let m_mix = super::super::classes::slot_get_method_id(base_cls, "mix", "(ID)I", false)
        .expect("instance mix");
    let area2 = build_save_area(4, 0, &[21], &[0.5], &[]);
    let ap2 = area2.ap();
    assert_eq!(
        slot_call_int_method_v(m_mix, recv, ap2),
        2100,
        "the GP and FP cursors advance independently"
    );
    clear_pending();
    delete_local(recv);
    delete_local(base_cls);
}

#[test]
fn vform_walk_falls_back_to_the_overflow_area() {
    let _rt = crate::runtime();
    install_dispatch_fixture();
    let base_cls = class_tag(CALL_BASE);
    let m_sum3 = super::super::classes::slot_get_method_id(base_cls, "sum3", "(III)I", true)
        .expect("static sum3");
    // gp_offset exhausted (48 = 6 GP slots burned): every word now
    // comes from the overflow (stack) area, 8 bytes per word.
    let area = build_save_area(6, 0, &[], &[], &[6, 7, 8]);
    let ap = area.ap();
    assert_eq!(
        slot_call_static_int_method_v(m_sum3, base_cls, ap),
        21,
        "exhausted registers read from the overflow area"
    );
    clear_pending();
    delete_local(base_cls);
}

#[test]
fn vform_record_is_left_untouched() {
    let _rt = crate::runtime();
    install_dispatch_fixture();
    let base_cls = class_tag(CALL_BASE);
    let m_sum3 = super::super::classes::slot_get_method_id(base_cls, "sum3", "(III)I", true)
        .expect("static sum3");
    let area = build_save_area(4, 0, &[3, 4, 5], &[], &[]);
    let ap = area.ap();
    let before: [u8; 24] = *area.rec;
    let _ = slot_call_static_int_method_v(m_sum3, base_cls, ap);
    assert_eq!(
        &*area.rec, &before,
        "the LOCAL-cursor law: the caller's \
        record is consumed into our cursor, never written back"
    );
    clear_pending();
    delete_local(base_cls);
}

#[test]
fn vform_null_ap_and_unreadable_record_are_loud_not_silent() {
    let _rt = crate::runtime();
    install_dispatch_fixture();
    let base_cls = class_tag(CALL_BASE);
    let m_sum3 = super::super::classes::slot_get_method_id(base_cls, "sum3", "(III)I", true)
        .expect("static sum3");
    // null ap with a demanding descriptor → NPE pend + zero (the
    // A-form null-args law's V-form twin; Temurin's is UB there).
    assert_eq!(slot_call_static_int_method_v(m_sum3, base_cls, 0), 0);
    assert!(super::super::bridge::has_pending_exception());
    clear_pending();
    // An unreadable record (reg_save_area = 0, gp_offset < 48 → the
    // first read addresses NULL) → the primitives answer None → a
    // loud internal-error pend, NEVER a silent mis-marshal.
    let mut rec: Box<[u8; 24]> = Box::new([0u8; 24]);
    rec[8..16].copy_from_slice(&1usize.to_le_bytes()); // overflow area (unread, must be non-null-shaped)
                                                       // reg_save_area stays 0; gp_offset stays 0.
    let ap = rec.as_ptr() as usize;
    assert_eq!(slot_call_static_int_method_v(m_sum3, base_cls, ap), 0);
    assert!(super::super::bridge::has_pending_exception());
    let t = super::super::bridge::take_pending_exception().expect("pending");
    assert_eq!(
        crate::heap::with_obj(t.obj, |o| o.jclass.to_string()),
        "java/lang/Error",
        "an unreadable record is OUR loud internal error, not NPE"
    );
    delete_local(base_cls);
}
