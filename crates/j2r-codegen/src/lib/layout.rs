//! `lib`::layout — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use j2r_runtime::{builtin, parse_method_desc, TypeDesc};
use std::collections::BTreeMap;

pub(crate) fn compute_layouts(prog: &mut Program) -> Result<()> {
    // memoized total_slots over user classes + builtin parents
    fn total_slots(prog: &Program, name: &str, depth: usize) -> Result<usize> {
        if depth > 64 {
            return Err(CodegenError::Semantic {
                class: name.to_string(),
                message: "class hierarchy too deep (cycle?)".to_string(),
            });
        }
        if let Some(idx) = prog.user_index.get(name) {
            let sup = prog.classes[*idx].super_name.clone();
            let base = match &sup {
                Some(s) => {
                    if prog.is_user(s) {
                        total_slots(prog, s, depth + 1)?
                    } else {
                        builtin::builtin_total_slots(s).ok_or_else(|| CodegenError::Semantic {
                            class: name.to_string(),
                            message: format!("unknown superclass {s}"),
                        })?
                    }
                }
                None => 0,
            };
            let own = prog.classes[*idx]
                .fields
                .iter()
                .filter(|f| !f.is_static)
                .count();
            Ok(base + own)
        } else {
            builtin::builtin_total_slots(name).ok_or_else(|| CodegenError::Semantic {
                class: name.to_string(),
                message: format!("unknown class {name} in layout computation"),
            })
        }
    }

    let names: Vec<String> = prog.classes.iter().map(|c| c.name.clone()).collect();
    for name in &names {
        let idx = prog.user_index[name];
        let sup = prog.classes[idx].super_name.clone();
        let base = match &sup {
            Some(s) => {
                if prog.is_user(s) {
                    total_slots(prog, s, 0)?
                } else {
                    builtin::builtin_total_slots(s).ok_or_else(|| CodegenError::Semantic {
                        class: name.clone(),
                        message: format!("unknown superclass {s}"),
                    })?
                }
            }
            None => 0,
        };
        let total = total_slots(prog, name, 0)?;
        let plan = &mut prog.classes[idx];
        plan.base_slots = base;
        plan.total_slots = total;
        // assign absolute instance slots
        let mut slot = base;
        for f in plan.fields.iter_mut() {
            if !f.is_static {
                f.instance_slot = Some(slot);
                slot += 1;
            }
        }
    }
    // Second pass — the default-slot spec (NEW-INSTANCE). Runs after every
    // base_slots is settled so the root user class's base is final. The
    // chain walk mirrors new_obj_lines' (root upward, subclass overrides
    // last); the layouts are dense and non-overlapping.
    let names: Vec<String> = prog.classes.iter().map(|c| c.name.clone()).collect();
    for name in &names {
        let idx = prog.user_index[name];
        let mut chain: Vec<String> = Vec::new();
        let mut cur = name.clone();
        loop {
            chain.push(cur.clone());
            let sup = prog.classes[prog.user_index[&cur]].super_name.clone();
            match sup {
                Some(s) if prog.is_user(&s) => cur = s,
                _ => break,
            }
        }
        let root_name = chain.last().unwrap().clone();
        let spec_base = prog.classes[prog.user_index[&root_name]].base_slots;
        let total = prog.classes[idx].total_slots;
        let mut spec = vec![b'i'; total - spec_base];
        for cls in chain.iter().rev() {
            let ci = prog.user_index[cls];
            let slots: Vec<(usize, String)> = prog.classes[ci]
                .fields
                .iter()
                .filter_map(|f| f.instance_slot.map(|s| (s, f.desc.clone())))
                .collect();
            for (abs, d) in slots {
                spec[abs - spec_base] = match d.as_str() {
                    "B" | "C" | "S" | "I" | "Z" => b'i',
                    "J" => b'j',
                    "F" => b'f',
                    "D" => b'd',
                    _ => b'r',
                };
            }
        }
        prog.classes[idx].default_spec = spec.iter().map(|&c| c as char).collect();
    }
    Ok(())
}

/// Decodes all methods of class `ci`, synthesizing `<clinit>` bodies for
/// `ConstantValue` fields and resolving `invokedynamic` concat sites.
pub(crate) fn plan_methods(prog: &Program, ci: usize) -> Result<MethodPlans> {
    let c = &prog.classes[ci];
    let mut methods = Vec::new();
    let mut clinit_fn = None;
    let mut concats = BTreeMap::new();
    let mut lambdas = BTreeMap::new();

    for m in &c.cf.methods {
        // ACC_SYNCHRONIZED on a method (0x0020). Since the monitor wave the
        // emitter wraps such methods with a monitor acquisition — instance
        // methods on the receiver, static methods on the class's synthetic
        // monitor object (M3_PLAN §7.1). Abstract/interface methods ignore
        // the flag per JVMS.
        let is_sync = !c.is_interface && m.access & j2r_classfile::flags::ACC_SYNCHRONIZED != 0;
        if m.code.is_none() {
            if m.is_abstract() {
                continue; // abstract: no body to compile
            }
            if m.access & j2r_classfile::flags::ACC_NATIVE != 0 {
                // T12 12c (docs/M5_JNI_NATIVES.md §4/§5): a native method
                // has no bytecode body; its implementation arrives via
                // the binding registry. Plan it WITHOUT instrs (the
                // emitter's native branch emits the call-through body;
                // the lookup table omits it) and record the metadata row.
                methods.push(MethodPlan {
                    name: m.name.clone(),
                    desc: m.descriptor.clone(),
                    is_static: m.is_static(),
                    is_native: true,
                    is_sync: false,
                    modifiers: m.access,
                    signature: m.signature.clone(),
                    lvtt: Vec::new(),
                    array_ops: std::collections::BTreeMap::new(),
                    fn_name: method_fn_name(&m.name, &m.descriptor),
                    instrs: Vec::new(),
                    exception_table: Vec::new(),
                    max_locals: 0,
                    max_stack: 0,
                });
                continue;
            }
            return Err(CodegenError::Unsupported {
                class: c.name.clone(),
                method: format!("{}{}", m.name, m.descriptor),
                pc: 0,
                what: "method has no Code attribute and is neither abstract nor native".to_string(),
            });
        }
        let code = m.code().map_err(cf_err)?;
        let (instrs, array_ops) =
            ir::decode(&code.code, &c.cf.constant_pool).map_err(|e| CodegenError::Unsupported {
                class: c.name.clone(),
                method: format!("{}{}", m.name, m.descriptor),
                pc: e.pc,
                what: e.message,
            })?;

        // invokedynamic sites, dispatched on the bootstrap class
        for (pc, instr) in &instrs {
            if let ir::Instr::InvokeDynamic { index } = instr {
                let what = |what: String| CodegenError::Unsupported {
                    class: c.name.clone(),
                    method: format!("{}{}", m.name, m.descriptor),
                    pc: *pc,
                    what,
                };
                let bcls = crate::lambda::bootstrap_class_of(c, *index).map_err(what)?;
                match bcls.as_str() {
                    crate::lambda::LMF_CLASS => {
                        let plan = crate::lambda::resolve_lambda(prog, ci, *index).map_err(what)?;
                        lambdas.insert(*index, plan);
                    }
                    "java/lang/invoke/StringConcatFactory" => {
                        let plan = resolve_concat(&prog.classes[ci], *index).map_err(what)?;
                        concats.insert(*index, plan);
                    }
                    other => {
                        return Err(what(format!("unsupported invokedynamic bootstrap {other}")));
                    }
                }
            }
        }

        let is_clinit = m.name == "<clinit>";
        if is_clinit {
            clinit_fn = Some(method_fn_name(&m.name, &m.descriptor));
        }
        methods.push(MethodPlan {
            name: m.name.clone(),
            desc: m.descriptor.clone(),
            is_static: m.is_static(),
            is_native: false,
            is_sync,
            modifiers: m.access,
            signature: m.signature.clone(),
            lvtt: code.lvtt.clone(),
            array_ops,
            fn_name: method_fn_name(&m.name, &m.descriptor),
            instrs,
            exception_table: code
                .exception_table
                .iter()
                .map(|e| {
                    (
                        e.start_pc as u32,
                        e.end_pc as u32,
                        e.handler_pc as u32,
                        e.catch_type.clone(),
                    )
                })
                .collect(),
            max_locals: code.max_locals as usize,
            max_stack: code.max_stack as usize,
        });
    }

    // ConstantValue static finals are NOT compiled into <clinit> code.
    // javac records their value in the ConstantValue attribute and inlines
    // reads; the JVM sets them during class preparation, before any
    // <clinit> code runs. J2R mirrors that by placing the values in the
    // generated crate's static pool at startup (see `statics::set_constants`
    // in the emitter). Synthesizing clinit *code* here was incorrect: the
    // appended stores landed after the method's `return` (dead code), and
    // ConstantValue-only classes produced a clinit without a terminator.

    Ok((methods, clinit_fn, concats, lambdas))
}

/// Resolves an `invokedynamic` call site. Milestone 1 supports exactly one
/// bootstrap: `StringConcatFactory.makeConcatWithConstants`.
pub(crate) fn resolve_concat(
    c: &ClassPlan,
    indy_index: u16,
) -> std::result::Result<ConcatPlan, String> {
    let cp = &c.cf.constant_pool;
    let (bsm_index, _name, desc) = cp.dynamic(indy_index).map_err(|e| e.message)?;
    let bsm =
        c.cf.bootstrap_methods
            .get(bsm_index as usize)
            .ok_or_else(|| format!("bootstrap method #{bsm_index} missing"))?;
    // method handle -> methodref
    let handle = cp.entry(bsm.method_handle).map_err(|e| e.message)?;
    let mh_index = match handle {
        j2r_classfile::CpEntry::MethodHandle { kind: _, index } => *index,
        other => return Err(format!("bootstrap handle is {}", other.tag_name())),
    };
    let (cls, meth, _mdesc) = cp.member_ref(mh_index).map_err(|e| e.message)?;
    if cls != "java/lang/invoke/StringConcatFactory" || meth != "makeConcatWithConstants" {
        return Err(format!(
            "unsupported invokedynamic bootstrap {cls}.{meth} (only StringConcatFactory.makeConcatWithConstants is supported)"
        ));
    }
    let recipe = match bsm.arguments.first() {
        Some(j2r_classfile::BootstrapArgument::String(s)) => s.clone(),
        Some(other) => {
            return Err(format!(
                "makeConcatWithConstants recipe must be a String, found {other:?}"
            ))
        }
        None => return Err("makeConcatWithConstants without recipe".to_string()),
    };
    let mut constants = Vec::new();
    for arg in &bsm.arguments[1..] {
        constants.push(match arg {
            j2r_classfile::BootstrapArgument::Int(v) => j2r_runtime::Value::Int(*v),
            j2r_classfile::BootstrapArgument::Long(v) => j2r_runtime::Value::Long(*v),
            j2r_classfile::BootstrapArgument::Float(v) => j2r_runtime::Value::Float(*v),
            j2r_classfile::BootstrapArgument::Double(v) => j2r_runtime::Value::Double(*v),
            j2r_classfile::BootstrapArgument::String(s) => {
                j2r_runtime::Value::Str(std::sync::Arc::new(s.clone()))
            }
            // MethodType/MethodHandle/Class constants never appear as concat
            // recipe constants in javac output; they belong to the
            // LambdaMetafactory path, which is dispatched earlier.
            j2r_classfile::BootstrapArgument::MethodType(_)
            | j2r_classfile::BootstrapArgument::MethodHandle { .. }
            | j2r_classfile::BootstrapArgument::Class(_) => {
                return Err("unsupported concat bootstrap constant".to_string())
            }
            j2r_classfile::BootstrapArgument::Opaque { tag } => {
                return Err(format!("unsupported concat bootstrap constant ({tag})"))
            }
        });
    }
    let (params, ret) =
        parse_method_desc(&desc).ok_or_else(|| format!("bad indy descriptor {desc:?}"))?;
    if !matches!(ret, TypeDesc::Object(ref o) if o == "java/lang/String") {
        return Err(format!("concat must return String, got {desc:?}"));
    }
    let kinds: String = params
        .iter()
        .map(|t| match t {
            TypeDesc::Int => 'I',
            TypeDesc::Long => 'J',
            TypeDesc::Float => 'F',
            TypeDesc::Double => 'D',
            TypeDesc::Char => 'C',
            TypeDesc::Boolean => 'Z',
            TypeDesc::Byte => 'B',
            TypeDesc::Short => 'S',
            TypeDesc::Object(_) | TypeDesc::Array(_) => 'L',
            TypeDesc::Void => 'V',
        })
        .collect();
    Ok(ConcatPlan {
        recipe,
        constants,
        kinds,
    })
}
