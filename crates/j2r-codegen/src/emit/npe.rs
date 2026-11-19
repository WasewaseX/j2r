//! `emit`::npe — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use crate::ir::Instr;
use crate::MethodPlan;

/// The JVM's `print_klass_name`: internal name with `/` -> `.`, exactly
/// two classes bare.
pub(crate) fn npe_klass_name(class: &str) -> String {
    let dotted = class.replace('/', ".");
    match dotted.as_str() {
        "java.lang.Object" => "Object".to_string(),
        "java.lang.String" => "String".to_string(),
        _ => dotted,
    }
}

/// One parameter's external form (the OpenJDK `Symbol` printer law):
/// `N` leading `[`s print `[]` N times after the element (a dotted class
/// or a primitive keyword), then the JVM's two-name trimmer (per
/// parameter the exact-prefix form is equivalent for real classfiles).
pub(crate) fn npe_param_type(t: &j2r_runtime::TypeDesc) -> String {
    match t {
        j2r_runtime::TypeDesc::Object(name) => npe_klass_name(name),
        // `inner` is the full array descriptor (`[C`, `[Ljava/lang/String;`)
        j2r_runtime::TypeDesc::Array(inner) => npe_param_raw(inner),
        j2r_runtime::TypeDesc::Boolean => "boolean".to_string(),
        j2r_runtime::TypeDesc::Byte => "byte".to_string(),
        j2r_runtime::TypeDesc::Short => "short".to_string(),
        j2r_runtime::TypeDesc::Char => "char".to_string(),
        j2r_runtime::TypeDesc::Int => "int".to_string(),
        j2r_runtime::TypeDesc::Long => "long".to_string(),
        j2r_runtime::TypeDesc::Float => "float".to_string(),
        j2r_runtime::TypeDesc::Double => "double".to_string(),
        j2r_runtime::TypeDesc::Void => "void".to_string(),
    }
}

/// The array-parameter form: the element (dotted class or primitive
/// keyword) then `[]` per dimension, then the two-name trimmer.
pub(crate) fn npe_param_raw(desc: &str) -> String {
    let dims = desc.bytes().take_while(|&b| b == b'[').count();
    let elem = &desc[dims..];
    let mut out = match elem.as_bytes().first() {
        Some(b'L') => npe_klass_name(
            elem.strip_suffix(';')
                .and_then(|s| s.strip_prefix('L'))
                .unwrap_or(elem),
        ),
        Some(b'Z') => "boolean".to_string(),
        Some(b'B') => "byte".to_string(),
        Some(b'S') => "short".to_string(),
        Some(b'C') => "char".to_string(),
        Some(b'I') => "int".to_string(),
        Some(b'J') => "long".to_string(),
        Some(b'F') => "float".to_string(),
        Some(b'D') => "double".to_string(),
        _ => elem.to_string(),
    };
    for _ in 0..dims {
        out.push_str("[]");
    }
    if let Some(rest) = out.strip_prefix("java.lang.Object") {
        return format!("Object{rest}");
    }
    if let Some(rest) = out.strip_prefix("java.lang.String") {
        return format!("String{rest}");
    }
    out
}

/// The `(...)` argument list of a method descriptor.
pub(crate) fn npe_params(desc: &str) -> String {
    let (params, _) = j2r_runtime::parse_method_desc(desc).expect("valid descriptor");
    params
        .iter()
        .map(npe_param_type)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The JVM's `print_method_name`: `<klass>.<name>(<params>)`.
pub(crate) fn npe_method_name(class: &str, name: &str, desc: &str) -> String {
    format!("{}.{}({})", npe_klass_name(class), name, npe_params(desc))
}

/// Is `slot` one of the method's incoming-parameter slots (slot 0 is the
/// receiver for instance methods)? The JVM's law prints `<parameterN>` /
/// `this` / the LVT name for a never-written parameter slot — forms j2r
/// keeps unprobed (bare NPE); a written non-parameter slot is the probed
/// `<localN>` form.
pub(crate) fn npe_slot_is_param(m: &MethodPlan, slot: u16) -> bool {
    // The instance receiver slot is the JVM's `this` branch (never null
    // in javac bytecode) — unprobed ground, treated like a parameter.
    if !m.is_static && slot == 0 {
        return true;
    }
    let (params, _) = j2r_runtime::parse_method_desc(&m.desc).expect("valid descriptor");
    let mut cur = if m.is_static { 0usize } else { 1usize };
    for p in &params {
        let size = j2r_runtime::type_slots(p);
        if (slot as usize) >= cur && (slot as usize) < cur + size {
            return true;
        }
        cur += size;
    }
    false
}

/// How many operand-stack values the receiver sits UNDER at the faulting
/// site (the values the site pops BEFORE the receiver): the invokes, the
/// arraylength, the getfield, the monitor ops and the athrow pop the
/// receiver FIRST (it is on top); aload pops the index first; astore pops
/// value+index; putfield pops the stored value first.
pub(crate) fn npe_operands_above(instr: &Instr) -> usize {
    match instr {
        Instr::AStore(_) => 2,
        Instr::ALoad(_) | Instr::PutField { .. } => 1,
        // the invoke receiver sits UNDER the call's arguments
        Instr::InvokeVirtual { desc, .. }
        | Instr::InvokeSpecial { desc, .. }
        | Instr::InvokeInterface { desc, .. } => arg_count(desc),
        _ => 0,
    }
}

/// Is this instruction a PURE operand producer (pops nothing, pushes
/// exactly one value)? The operand accounting above the receiver only
/// accepts these: a composed value (`q.ifld = obj.f` — the value is a
/// load-plus-getfield pair) has its producer deeper than one push, and
/// a backward one-push-per-operand walk would mis-attribute it — the
/// JVM's forward simulation handles it, j2r's bounded law refuses
/// (unprobed ground, registered residual).
pub(crate) fn npe_pure_producer(instr: &Instr) -> bool {
    matches!(
        instr,
        Instr::Load(_)
            | Instr::IConst(_)
            | Instr::LConst(_)
            | Instr::FConst(_)
            | Instr::DConst(_)
            | Instr::AConstNull
            | Instr::Ldc(_)
            | Instr::Ldc2W(_)
            | Instr::GetStatic { .. }
            | Instr::New { .. }
    )
}

/// The CAUSE look-back: the instruction that produced the null receiver,
/// skipping pass-through `checkcast`s and accounting for the operands
/// stacked above the receiver (the array/field-write sites pop them
/// first). Returns the `because ...` clause (without the trailing
/// `is null`), or None when the producer kind is outside the probed
/// matrix (bare NPE).
pub(crate) fn npe_cause(m: &MethodPlan, pc: u32, instr: &Instr) -> Option<String> {
    let idx = m.instrs.iter().position(|(p, _)| *p == pc)?;
    let mut pending = npe_operands_above(instr);
    let mut i = idx;
    while i > 0 {
        i -= 1;
        if pending > 0 {
            // The operands stacked above the receiver were pushed by the
            // straight-line run before the fault; each pure producer
            // accounts for one. Composed values (a push that itself
            // consumed operands), dup-shaped pushes and control flow are
            // unprobed provenance — refuse.
            if !npe_pure_producer(&m.instrs[i].1) {
                return None;
            }
            pending -= 1;
            continue;
        }
        match &m.instrs[i].1 {
            Instr::CheckCast { .. } => continue,
            // javac's monitor-save shape (`aload o; dup; astore_2;
            // monitorenter`): the store and the dup are pass-throughs
            // for the VALUE — the JVM's simulated stack lands on the
            // load beneath them (A8's oracle).
            Instr::Store(_) => continue,
            Instr::Dup => continue,
            Instr::Load(slot) => {
                if npe_slot_is_param(m, *slot) {
                    return None;
                }
                return Some(format!("because \"<local{slot}>\""));
            }
            Instr::GetStatic { class, field, .. } => {
                return Some(format!("because \"{}.{}\"", npe_klass_name(class), field));
            }
            Instr::InvokeVirtual { class, name, desc }
            | Instr::InvokeSpecial { class, name, desc }
            | Instr::InvokeStatic { class, name, desc }
            | Instr::InvokeInterface { class, name, desc } => {
                return Some(format!(
                    "because the return value of \"{}\"",
                    npe_method_name(class, name, desc)
                ));
            }
            _ => return None,
        }
    }
    None
}

/// The full null-detail message for a null-check site (the ACTION plus
/// the CAUSE), or None to keep the bare NPE. Both halves must be
/// computable — the JVM never emits the action alone in probed ground.
pub(crate) fn npe_message(m: &MethodPlan, pc: u32, instr: &Instr) -> Option<String> {
    let action = match instr {
        Instr::InvokeVirtual { class, name, desc }
        | Instr::InvokeSpecial { class, name, desc }
        | Instr::InvokeInterface { class, name, desc } => {
            format!("Cannot invoke \"{}\"", npe_method_name(class, name, desc))
        }
        Instr::GetField { field, .. } => format!("Cannot read field \"{field}\""),
        Instr::PutField { field, .. } => format!("Cannot assign field \"{field}\""),
        Instr::ArrayLength => "Cannot read the array length".to_string(),
        Instr::ALoad(kind) => format!("Cannot load from {} array", npe_array_kind(m, pc, kind)?),
        Instr::AStore(kind) => format!("Cannot store to {} array", npe_array_kind(m, pc, kind)?),
        Instr::MonitorEnter => "Cannot enter synchronized block".to_string(),
        Instr::AThrow => "Cannot throw exception".to_string(),
        _ => return None,
    };
    let cause = npe_cause(m, pc, instr)?;
    Some(format!("{action} {cause} is null"))
}

/// The JVM's per-opcode array kind (the message is driven by the opcode,
/// not the array's runtime type). iaload/baload/caload/saload (and the
/// matching stores) share `ElemKind::IntGroup` — the decoder records the
/// raw opcodes for the message layer.
pub(crate) fn npe_array_kind(
    m: &MethodPlan,
    pc: u32,
    kind: &crate::ir::ElemKind,
) -> Option<&'static str> {
    match kind {
        crate::ir::ElemKind::Long => Some("long"),
        crate::ir::ElemKind::Float => Some("float"),
        crate::ir::ElemKind::Double => Some("double"),
        crate::ir::ElemKind::Ref | crate::ir::ElemKind::Nest(_) => Some("object"),
        crate::ir::ElemKind::IntGroup => {
            let op = *m.array_ops.get(&pc)?;
            Some(match op {
                0x2e | 0x4f => "int",
                0x33 | 0x54 => "byte/boolean",
                0x34 | 0x55 => "char",
                0x35 | 0x56 => "short",
                _ => return None,
            })
        }
    }
}

/// The emitted `pop_ref` law line for a null-check site: with the
/// computed message when one is available, the plain law (unchanged
/// generated code) otherwise. `bind` names the binding (`__r` for the
/// invoke/field/monitor receivers, `__arr` for the array accesses).
pub(crate) fn npe_pop_ref_line(
    m: &MethodPlan,
    pc: u32,
    instr: &Instr,
    fault: &str,
    bind: &str,
) -> String {
    match npe_message(m, pc, instr) {
        Some(msg) => format!(
            "let {bind} = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::pop_ref_msg(&mut stack, ::core::option::Option::Some({msg:?})));"
        ),
        None => format!(
            "let {bind} = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::pop_ref(&mut stack));"
        ),
    }
}

/// The emitted `athrow` law line (`object::take_thrown_msg`): the same
/// static-message law on the thrown object itself.
pub(crate) fn npe_take_thrown_line(m: &MethodPlan, pc: u32, instr: &Instr, fault: &str) -> String {
    match npe_message(m, pc, instr) {
        Some(msg) => format!(
            "let __thr = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::object::take_thrown_msg(&mut stack, ::core::option::Option::Some({msg:?})));"
        ),
        None => format!(
            "let __thr = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::object::take_thrown(&mut stack));"
        ),
    }
}
#[cfg(test)]
pub(crate) mod npe_message_tests {
    use super::*;
    use crate::ir::Instr;
    use crate::ir::Ldc;

    fn plan(is_static: bool, desc: &str, instrs: Vec<(u32, Instr)>) -> MethodPlan {
        MethodPlan {
            name: "t".into(),
            desc: desc.into(),
            is_static,
            is_native: false,
            is_sync: false,
            modifiers: 0x0009,
            signature: None,
            lvtt: Vec::new(),
            array_ops: Default::default(),
            fn_name: "t".into(),
            instrs,
            exception_table: Vec::new(),
            max_locals: 5,
            max_stack: 3,
        }
    }

    // print_klass_name: the two bare names and the dotted law.
    #[test]
    fn klass_name_two_bare_names_and_dotted_law() {
        assert_eq!(npe_klass_name("java/lang/Double"), "java.lang.Double");
        assert_eq!(npe_klass_name("java/lang/String"), "String");
        assert_eq!(npe_klass_name("java/lang/Object"), "Object");
        assert_eq!(npe_klass_name("java/util/Map"), "java.util.Map");
        assert_eq!(npe_klass_name("NpeShapeProbe2"), "NpeShapeProbe2");
        assert_eq!(npe_klass_name("[I"), "[I");
        // the bare law is EXACT — no prefix-family drift
        assert_eq!(
            npe_klass_name("java/lang/StringBuilder"),
            "java.lang.StringBuilder"
        );
    }

    // The parameter list law: keywords, dotted names, arrays, the trimmer.
    #[test]
    fn params_keywords_arrays_and_the_two_name_trimmer() {
        assert_eq!(npe_params("()V"), "");
        assert_eq!(
            npe_params("(Ljava/lang/Object;)Ljava/lang/Object;"),
            "Object"
        );
        assert_eq!(npe_params("(I)V"), "int");
        assert_eq!(npe_params("(II)V"), "int, int");
        assert_eq!(npe_params("(Ljava/lang/String;)V"), "String");
        assert_eq!(
            npe_params("(Ljava/lang/CharSequence;)V"),
            "java.lang.CharSequence"
        );
        // A10's oracle: `String.getChars(int, int, char[], int)`
        assert_eq!(npe_params("(II[CI)V"), "int, int, char[], int");
        // the JVM's 16-char prefix trimmer trims the array form too
        assert_eq!(npe_params("([Ljava/lang/String;)V"), "String[]");
        // the trimmer covers the array form too
        assert_eq!(npe_params("([Ljava/lang/Object;)V"), "Object[]");
        // the 2-arg producer list (A11)
        assert_eq!(
            npe_params("(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"),
            "Object, Object"
        );
    }

    // print_method_name: the producer-chain forms from the oracle.
    #[test]
    fn method_name_forms_match_the_oracle() {
        assert_eq!(
            npe_method_name(
                "java/util/Map",
                "get",
                "(Ljava/lang/Object;)Ljava/lang/Object;"
            ),
            "java.util.Map.get(Object)"
        );
        assert_eq!(
            npe_method_name("NpeShapeProbe2", "getS2", "()Ljava/lang/String;"),
            "NpeShapeProbe2.getS2()"
        );
        assert_eq!(
            npe_method_name(
                "java/lang/String",
                "compareToIgnoreCase",
                "(Ljava/lang/String;)I"
            ),
            "String.compareToIgnoreCase(String)"
        );
        assert_eq!(
            npe_method_name(
                "java/lang/StringBuilder",
                "insert",
                "(ILjava/lang/CharSequence;II)Ljava/lang/StringBuilder;"
            ),
            "java.lang.StringBuilder.insert(int, java.lang.CharSequence, int, int)"
        );
    }

    // The slot law: main's args occupy slot 0 only; a static no-arg
    // method has no parameter slots; an instance method's slot 0 is the
    // receiver (`this` — unprobed ground, treated like a parameter).
    #[test]
    fn slot_law_params_receiver_and_written_locals() {
        let m = plan(true, "([Ljava/lang/String;)V", vec![]);
        assert!(npe_slot_is_param(&m, 0));
        assert!(!npe_slot_is_param(&m, 1));
        assert!(!npe_slot_is_param(&m, 2));
        let no_args = plan(true, "()V", vec![]);
        assert!(!npe_slot_is_param(&no_args, 0));
        // wide params occupy two slots
        let wide = plan(true, "(JD)V", vec![]);
        assert!(npe_slot_is_param(&wide, 0));
        assert!(npe_slot_is_param(&wide, 1));
        assert!(npe_slot_is_param(&wide, 2));
        assert!(npe_slot_is_param(&wide, 3));
        assert!(!npe_slot_is_param(&wide, 4));
        // the instance receiver slot
        let ctor = plan(false, "()V", vec![]);
        assert!(npe_slot_is_param(&ctor, 0));
    }

    // The S1 unbox chain: get (interface) -> checkcast -> unbox; the
    // checkcast is a pass-through and the producer names the get with
    // the ERASED argument list.
    #[test]
    fn s1_unbox_chain_through_checkcast() {
        let m = plan(
            true,
            "([Ljava/lang/String;)V",
            vec![
                (0, Instr::Ldc(Ldc::String("missing".into()))),
                (8, Instr::Load(1)),
                (9, Instr::Ldc(Ldc::String("missing".into()))),
                (
                    11,
                    Instr::InvokeInterface {
                        class: "java/util/Map".into(),
                        name: "get".into(),
                        desc: "(Ljava/lang/Object;)Ljava/lang/Object;".into(),
                    },
                ),
                (
                    16,
                    Instr::CheckCast {
                        class: "java/lang/Double".into(),
                    },
                ),
                (
                    19,
                    Instr::InvokeVirtual {
                        class: "java/lang/Double".into(),
                        name: "doubleValue".into(),
                        desc: "()D".into(),
                    },
                ),
            ],
        );
        let msg = npe_message(&m, 19, &m.instrs[5].1).expect("S1 is computable");
        assert_eq!(
            msg,
            "Cannot invoke \"java.lang.Double.doubleValue()\" because the return value of \"java.util.Map.get(Object)\" is null"
        );
    }

    // The S2 null-local law: the producing Load names the SLOT (the
    // LVT-absent fallback); S4 names the producing static call.
    #[test]
    fn s2_local_and_s4_producer_forms() {
        let s2 = plan(
            true,
            "([Ljava/lang/String;)V",
            vec![
                (54, Instr::AConstNull),
                (55, Instr::Store(1)),
                (56, Instr::Load(1)),
                (
                    57,
                    Instr::InvokeVirtual {
                        class: "java/lang/Double".into(),
                        name: "doubleValue".into(),
                        desc: "()D".into(),
                    },
                ),
            ],
        );
        assert_eq!(
            npe_message(&s2, 57, &s2.instrs[3].1).expect("S2 is computable"),
            "Cannot invoke \"java.lang.Double.doubleValue()\" because \"<local1>\" is null"
        );
        let s4 = plan(
            true,
            "([Ljava/lang/String;)V",
            vec![
                (
                    130,
                    Instr::InvokeStatic {
                        class: "NpeShapeProbe2".into(),
                        name: "getS2".into(),
                        desc: "()Ljava/lang/String;".into(),
                    },
                ),
                (
                    133,
                    Instr::InvokeVirtual {
                        class: "java/lang/String".into(),
                        name: "length".into(),
                        desc: "()I".into(),
                    },
                ),
            ],
        );
        assert_eq!(
            npe_message(&s4, 133, &s4.instrs[1].1).expect("S4 is computable"),
            "Cannot invoke \"String.length()\" because the return value of \"NpeShapeProbe2.getS2()\" is null"
        );
    }

    // The array kinds are the RAW OPCODE's law (S7/A1/A3/A6): iastore
    // "int", baload "byte/boolean", caload "char", saload "short" — all
    // ElemKind::IntGroup, so the side-map decides.
    #[test]
    fn array_kinds_come_from_the_raw_opcode() {
        let cases = [
            (0x2e, "int"),          // iaload
            (0x4f, "int"),          // iastore
            (0x33, "byte/boolean"), // baload
            (0x54, "byte/boolean"), // bastore
            (0x34, "char"),         // caload
            (0x55, "char"),         // castore
            (0x35, "short"),        // saload
            (0x56, "short"),        // sastore
        ];
        for (op, kind) in cases {
            let store = op >= 0x4f;
            // loads: aload arr, iconst_0, <op> — ONE operand above the
            // receiver; stores: aload arr, iconst_0, iconst_1, <op>.
            let (instrs, fault) = if store {
                (
                    vec![
                        (0, Instr::Load(1)),
                        (1, Instr::IConst(0)),
                        (2, Instr::IConst(1)),
                        (3, Instr::AStore(crate::ir::ElemKind::IntGroup)),
                    ],
                    3,
                )
            } else {
                (
                    vec![
                        (0, Instr::Load(1)),
                        (1, Instr::IConst(0)),
                        (2, Instr::ALoad(crate::ir::ElemKind::IntGroup)),
                    ],
                    2,
                )
            };
            let mut m = plan(true, "([Ljava/lang/String;)V", instrs);
            m.array_ops.insert(fault, op);
            let instr = &m.instrs.last().unwrap().1;
            let msg = npe_message(&m, fault, instr).expect("the probed array kinds compute");
            let expected = if store {
                format!("Cannot store to {kind} array because \"<local1>\" is null")
            } else {
                format!("Cannot load from {kind} array because \"<local1>\" is null")
            };
            assert_eq!(msg, expected, "opcode {op:#x}");
        }
    }

    // The non-IntGroup kinds are ElemKind's own law (A2/A4).
    #[test]
    fn ref_and_long_array_kinds() {
        let instrs = vec![
            (0, Instr::Load(1)),
            (1, Instr::IConst(0)),
            (2, Instr::ALoad(crate::ir::ElemKind::Ref)),
        ];
        let m = plan(true, "([Ljava/lang/String;)V", instrs);
        let msg = npe_message(&m, 2, &m.instrs[2].1).expect("aaload computes");
        assert_eq!(
            msg,
            "Cannot load from object array because \"<local1>\" is null"
        );
        let instrs = vec![
            (0, Instr::Load(1)),
            (1, Instr::IConst(0)),
            (2, Instr::ALoad(crate::ir::ElemKind::Long)),
        ];
        let m = plan(true, "([Ljava/lang/String;)V", instrs);
        let msg = npe_message(&m, 2, &m.instrs[2].1).expect("laload computes");
        assert_eq!(
            msg,
            "Cannot load from long array because \"<local1>\" is null"
        );
    }

    // S7's operand accounting: the array sits UNDER the index and the
    // stored value; A7's putfield shape sits under the stored value;
    // A10's invoke receiver sits under its four arguments.
    #[test]
    fn operands_above_the_receiver_are_accounted() {
        // S7: aload_1, iconst_0, iconst_1, iastore
        let s7 = plan(
            true,
            "([Ljava/lang/String;)V",
            vec![
                (255, Instr::Load(1)),
                (256, Instr::IConst(0)),
                (257, Instr::IConst(1)),
                (258, Instr::AStore(crate::ir::ElemKind::IntGroup)),
            ],
        );
        let mut s7 = s7;
        s7.array_ops.insert(258, 0x4f);
        assert_eq!(
            npe_message(&s7, 258, &s7.instrs[3].1).expect("S7 computes"),
            "Cannot store to int array because \"<local1>\" is null"
        );
        // A7: aload_2, iconst_1, putfield
        let a7 = plan(
            false,
            "()V",
            vec![
                (0, Instr::Load(2)),
                (1, Instr::IConst(1)),
                (
                    2,
                    Instr::PutField {
                        class: "NpeShapes3".into(),
                        field: "ifld".into(),
                        desc: "I".into(),
                    },
                ),
            ],
        );
        assert_eq!(
            npe_message(&a7, 2, &a7.instrs[2].1).expect("A7 computes"),
            "Cannot assign field \"ifld\" because \"<local2>\" is null"
        );
        // A10: aload_1, iconst_0, iconst_1, aload_3, iconst_0, invokevirtual
        let a10 = plan(
            true,
            "([Ljava/lang/String;)V",
            vec![
                (0, Instr::Load(1)),
                (1, Instr::IConst(0)),
                (2, Instr::IConst(1)),
                (3, Instr::Load(3)),
                (4, Instr::IConst(0)),
                (
                    5,
                    Instr::InvokeVirtual {
                        class: "java/lang/String".into(),
                        name: "getChars".into(),
                        desc: "(II[CI)V".into(),
                    },
                ),
            ],
        );
        assert_eq!(
            npe_message(&a10, 5, &a10.instrs[5].1).expect("A10 computes"),
            "Cannot invoke \"String.getChars(int, int, char[], int)\" because \"<local1>\" is null"
        );
    }

    // The composed-value refusal: `q.ifld = obj.f` — the value is a
    // load-plus-getfield pair, so the bounded backward walk refuses
    // rather than mis-attribute (the JVM's forward simulation lands on
    // local2; j2r keeps the bare NPE — registered residual).
    #[test]
    fn composed_value_operands_refuse() {
        let composed = plan(
            false,
            "()V",
            vec![
                (0, Instr::Load(2)),
                (1, Instr::Load(3)),
                (
                    2,
                    Instr::GetField {
                        class: "NpeShapes3".into(),
                        field: "ifld".into(),
                        desc: "I".into(),
                    },
                ),
                (
                    5,
                    Instr::PutField {
                        class: "NpeShapes3".into(),
                        field: "ifld".into(),
                        desc: "I".into(),
                    },
                ),
            ],
        );
        assert!(npe_message(&composed, 5, &composed.instrs[3].1).is_none());
    }

    // A8's monitor-save passthrough: aload_1, dup, astore_2,
    // monitorenter — the store and the dup are value pass-throughs.
    #[test]
    fn monitor_save_shape_walks_to_the_load() {
        let a8 = plan(
            true,
            "([Ljava/lang/String;)V",
            vec![
                (0, Instr::AConstNull),
                (1, Instr::Store(1)),
                (2, Instr::Load(1)),
                (3, Instr::Dup),
                (4, Instr::Store(2)),
                (5, Instr::MonitorEnter),
            ],
        );
        assert_eq!(
            npe_message(&a8, 5, &a8.instrs[5].1).expect("A8 computes"),
            "Cannot enter synchronized block because \"<local1>\" is null"
        );
    }

    // The conservative boundary: a never-written parameter slot (the
    // JVM's <parameterN>/this branch), a field-read producer (the chain
    // cause), a dup-shaped producer and the specialized map cores all
    // keep the bare NPE.
    #[test]
    fn unprobed_producers_keep_the_bare_npe() {
        let param = plan(
            true,
            "(Ljava/lang/String;)V",
            vec![
                (0, Instr::Load(0)),
                (
                    1,
                    Instr::InvokeVirtual {
                        class: "java/lang/String".into(),
                        name: "length".into(),
                        desc: "()I".into(),
                    },
                ),
            ],
        );
        assert!(npe_message(&param, 1, &param.instrs[1].1).is_none());
        let field_chain = plan(
            true,
            "([Ljava/lang/String;)V",
            vec![
                (0, Instr::Load(1)),
                (
                    1,
                    Instr::GetField {
                        class: "NpeShapeProbe2".into(),
                        field: "ifield".into(),
                        desc: "Ljava/lang/Double;".into(),
                    },
                ),
                (
                    4,
                    Instr::InvokeVirtual {
                        class: "java/lang/Double".into(),
                        name: "doubleValue".into(),
                        desc: "()D".into(),
                    },
                ),
            ],
        );
        assert!(npe_message(&field_chain, 4, &field_chain.instrs[2].1).is_none());
        let dup = plan(
            true,
            "()V",
            vec![
                (0, Instr::AConstNull),
                (1, Instr::Dup),
                (
                    2,
                    Instr::InvokeVirtual {
                        class: "java/lang/Double".into(),
                        name: "doubleValue".into(),
                        desc: "()D".into(),
                    },
                ),
            ],
        );
        assert!(npe_message(&dup, 2, &dup.instrs[2].1).is_none());
    }
}
