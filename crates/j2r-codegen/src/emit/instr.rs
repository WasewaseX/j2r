//! `emit`::instr — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use crate::ir::{Instr, Ldc, Ldc2};
use crate::{CallTarget, ClassPlan, MethodPlan, Program};

// ---------------------------------------------------------------- instructions

pub(crate) fn emit_instr(
    prog: &Program,
    c: &ClassPlan,
    m: &MethodPlan,
    pc: u32,
    instr: &Instr,
    out: &mut Vec<String>,
) {
    let fault = format!("{pc}u32");
    match instr {
        Instr::Nop
        | Instr::AConstNull
        | Instr::IConst(_)
        | Instr::LConst(_)
        | Instr::FConst(_)
        | Instr::DConst(_)
        | Instr::Ldc(_)
        | Instr::Ldc2W(_)
        | Instr::Load(_)
        | Instr::Store(_)
        | Instr::IInc { .. } => emit_const_local(prog, c, m, pc, instr, &fault, out),
        Instr::ALoad(_) | Instr::AStore(_) | Instr::ArrayLength => {
            emit_array_ops(prog, c, m, pc, instr, &fault, out)
        }
        Instr::Pop
        | Instr::Pop2
        | Instr::Dup
        | Instr::DupX1
        | Instr::DupX2
        | Instr::Dup2
        | Instr::Dup2X1
        | Instr::Dup2X2
        | Instr::Swap => emit_shuffle(prog, c, m, pc, instr, &fault, out),
        Instr::Arith { op, kind } => emit_arith(*op, *kind, &fault, out),
        Instr::Convert(conv) => emit_convert(*conv, out),
        Instr::Cmp(kind) => emit_cmp(*kind, out),
        Instr::IfZero(..)
        | Instr::IfICmp(..)
        | Instr::IfRefCmp { .. }
        | Instr::IfNull(_)
        | Instr::IfNonNull(_)
        | Instr::Goto(_)
        | Instr::TableSwitch { .. }
        | Instr::LookupSwitch { .. }
        | Instr::Return(_) => emit_branch_ops(prog, c, m, pc, instr, &fault, out),
        Instr::GetStatic { .. }
        | Instr::PutStatic { .. }
        | Instr::GetField { .. }
        | Instr::PutField { .. } => emit_field_ops(prog, c, m, pc, instr, &fault, out),
        Instr::InvokeVirtual { .. } | Instr::InvokeInterface { .. } => {
            emit_invoke_virtual_ops(prog, c, m, pc, instr, &fault, out)
        }
        Instr::InvokeStatic { .. } | Instr::InvokeSpecial { .. } => {
            emit_invoke_static_ops(prog, c, m, pc, instr, &fault, out)
        }
        Instr::InvokeDynamic { .. } => emit_invoke_dynamic_ops(prog, c, m, pc, instr, &fault, out),
        Instr::MonitorEnter | Instr::MonitorExit => {
            emit_monitor_ops(prog, c, m, pc, instr, &fault, out)
        }
        Instr::New { .. }
        | Instr::NewArray(_)
        | Instr::NewIntArrayFilled { .. }
        | Instr::ANewArray { .. }
        | Instr::MultiNewArray { .. } => emit_alloc_ops(prog, c, m, pc, instr, &fault, out),
        Instr::MapPutPrim { .. } | Instr::MapGetPrim { .. } => {
            emit_map_prim_ops(prog, c, m, pc, instr, &fault, out)
        }
        Instr::AThrow | Instr::CheckCast { .. } | Instr::InstanceOf { .. } => {
            emit_throw_cast_ops(prog, c, m, pc, instr, &fault, out)
        }
    }
}

/// The const/local arms (verbatim from `emit_instr`, round-6 reshaping): the pushes, the loads, the stores and the iinc.
pub(crate) fn emit_const_local(
    _prog: &Program,
    _c: &ClassPlan,
    _m: &MethodPlan,
    _pc: u32,
    instr: &Instr,
    _fault: &str,
    out: &mut Vec<String>,
) {
    match instr {
        Instr::Nop => {}
        Instr::AConstNull => out.push(format!(
            "stack.push({RT}::Value::Ref(::core::option::Option::None));"
        )),
        Instr::IConst(v) => out.push(format!("stack.push({RT}::Value::Int({v}));")),
        Instr::LConst(v) => out.push(format!("stack.push({RT}::Value::Long({v}i64));")),
        Instr::FConst(v) => out.push(format!("stack.push({RT}::Value::Float({}));", f32_lit(*v))),
        Instr::DConst(v) => out.push(format!("stack.push({RT}::Value::Double({}));", f64_lit(*v))),
        Instr::Ldc(Ldc::Int(v)) => out.push(format!("stack.push({RT}::Value::Int({v}));")),
        Instr::Ldc(Ldc::Float(v)) => {
            out.push(format!("stack.push({RT}::Value::Float({}));", f32_lit(*v)))
        }
        Instr::Ldc(Ldc::String(s)) => out.push(format!(
            "stack.push({RT}::Value::Ref(::core::option::Option::Some({RT}::intern_string({s:?}))));"
        )),
        Instr::Ldc(Ldc::Class(name)) => {
            // M3 Wave 5 (M3_PLAN §8.1): `X.class` / `int.class` / array
            // literals resolve to the interned per-class `Class` object.
            out.push(format!(
                "stack.push({RT}::reflect::class_value({name:?}));"
            ))
        }
        Instr::Ldc2W(Ldc2::Long(v)) => {
            out.push(format!("stack.push({RT}::Value::Long({v}i64));"))
        }
        Instr::Ldc2W(Ldc2::Double(v)) => {
            out.push(format!("stack.push({RT}::Value::Double({}));", f64_lit(*v)))
        }
        Instr::Load(idx) => out.push(format!("stack.push(locals[{idx}usize].clone());")),
        Instr::Store(idx) => out.push(format!(
            "locals[{idx}usize] = {RT}::stack::stack_pop(&mut stack);"
        )),
        Instr::IInc { idx, c } => out.push(format!(
            "locals[{idx}usize] = {RT}::Value::Int(locals[{idx}usize].expect_int().wrapping_add({c}));"
        )),
        _ => unreachable!("j2r internal: not a const_local arm"),
    }
}

/// The array-access arms (verbatim): aload's pop-int + S5 law site, astore's S7 shape and arraylength.
pub(crate) fn emit_array_ops(
    _prog: &Program,
    _c: &ClassPlan,
    m: &MethodPlan,
    pc: u32,
    instr: &Instr,
    fault: &str,
    out: &mut Vec<String>,
) {
    match instr {
        Instr::ALoad(_) => {
            out.push("let __idx = ::j2r_runtime::pop_int(&mut stack);".into());
            // JEP 358 slice b: the array law sites (S5-family; the kind
            // is the raw opcode's law via the decode side-map).
            out.push(npe_pop_ref_line(m, pc, instr, fault, "__arr"));
            out.push(format!(
                "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::array::array_load(&__arr, __idx));"
            ));
            out.push("stack.push(__v);".into());
        }
        Instr::AStore(_) => {
            out.push("let __v = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            out.push("let __idx = ::j2r_runtime::pop_int(&mut stack);".into());
            // JEP 358 slice b: S7's shape (the store kind per opcode).
            out.push(npe_pop_ref_line(m, pc, instr, fault, "__arr"));
            out.push(format!(
                "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::array::array_store(&__arr, __idx, __v, crate::j2r_dispatch()));"
            ));
        }
        Instr::ArrayLength => {
            // JEP 358 slice b: S5's shape.
            out.push(npe_pop_ref_line(m, pc, instr, fault, "__arr"));
            out.push(format!(
                "stack.push({RT}::Value::Int({RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::array::array_length(&__arr))));"
            ));
        }
        _ => unreachable!("j2r internal: not a array arm"),
    }
}

/// The operand-stack shuffle arms (verbatim): pop/pop2 and the dup family.
pub(crate) fn emit_shuffle(
    _prog: &Program,
    _c: &ClassPlan,
    _m: &MethodPlan,
    _pc: u32,
    instr: &Instr,
    _fault: &str,
    out: &mut Vec<String>,
) {
    match instr {
        Instr::Pop => out.push("let _ = stack.pop();".into()),
        Instr::Pop2 => out.push("::j2r_runtime::stack::stack_pop2(&mut stack);".into()),
        Instr::Dup => out.push("::j2r_runtime::stack::stack_dup(&mut stack);".into()),
        Instr::DupX1 => out.push("::j2r_runtime::stack::stack_dup_x1(&mut stack);".into()),
        Instr::DupX2 => out.push("::j2r_runtime::stack::stack_dup_x2(&mut stack);".into()),
        Instr::Dup2 => out.push("::j2r_runtime::stack::stack_dup2(&mut stack);".into()),
        Instr::Dup2X1 => out.push("::j2r_runtime::stack::stack_dup2_x1(&mut stack);".into()),
        Instr::Dup2X2 => out.push("::j2r_runtime::stack::stack_dup2_x2(&mut stack);".into()),
        Instr::Swap => out.push("::j2r_runtime::stack::stack_swap(&mut stack);".into()),
        _ => unreachable!("j2r internal: not a shuffle arm"),
    }
}

/// The branch, switch and return arms (verbatim): the conditional pairs, the ref comparisons, both switches, goto and the typed returns.
pub(crate) fn emit_branch_ops(
    _prog: &Program,
    _c: &ClassPlan,
    m: &MethodPlan,
    pc: u32,
    instr: &Instr,
    _fault: &str,
    out: &mut Vec<String>,
) {
    match instr {
        Instr::IfZero(op, t) => {
            backward_poll(out, pc, &[*t]);
            out.push("let __a = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push(format!(
                "if __a {} 0 {{ pc = {t}u32; }} else {{ pc = {}u32; }}",
                op.rust_op(),
                next_pc_of(m, pc)
            ));
        }
        Instr::IfICmp(op, t) => {
            backward_poll(out, pc, &[*t]);
            out.push("let __b = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push("let __a = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push(format!(
                "if __a {} __b {{ pc = {t}u32; }} else {{ pc = {}u32; }}",
                op.rust_op(),
                next_pc_of(m, pc)
            ));
        }
        Instr::IfRefCmp { eq, target } => {
            backward_poll(out, pc, &[*target]);
            out.push("let __b = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            out.push("let __a = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            let cond = if *eq {
                format!("{RT}::ref_eq(&__a, &__b)")
            } else {
                format!("!{RT}::ref_eq(&__a, &__b)")
            };
            out.push(format!(
                "if {cond} {{ pc = {target}u32; }} else {{ pc = {}u32; }}",
                next_pc_of(m, pc)
            ));
        }
        Instr::IfNull(t) => {
            backward_poll(out, pc, &[*t]);
            out.push("let __a = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            out.push(format!(
                "if __a.is_null() {{ pc = {t}u32; }} else {{ pc = {}u32; }}",
                next_pc_of(m, pc)
            ));
        }
        Instr::IfNonNull(t) => {
            backward_poll(out, pc, &[*t]);
            out.push("let __a = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            out.push(format!(
                "if !__a.is_null() {{ pc = {t}u32; }} else {{ pc = {}u32; }}",
                next_pc_of(m, pc)
            ));
        }
        Instr::Goto(t) => {
            backward_poll(out, pc, &[*t]);
            out.push(format!("pc = {t}u32;"));
        }
        Instr::TableSwitch {
            default,
            low,
            targets,
        } => {
            let mut ts = targets.clone();
            ts.push(*default);
            backward_poll(out, pc, &ts);
            out.push("let __k = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push("match __k {".into());
            for (i, t) in targets.iter().enumerate() {
                let v = *low as i64 + i as i64;
                out.push(format!("    {v}i32 => {{ pc = {t}u32; }}"));
            }
            out.push(format!("    _ => {{ pc = {default}u32; }}"));
            out.push("}".into());
        }
        Instr::LookupSwitch { default, pairs } => {
            let mut ts: Vec<u32> = pairs.iter().map(|(_, t)| *t).collect();
            ts.push(*default);
            backward_poll(out, pc, &ts);
            out.push("let __k = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push("match __k {".into());
            for (v, t) in pairs {
                out.push(format!("    {v}i32 => {{ pc = {t}u32; }}"));
            }
            out.push(format!("    _ => {{ pc = {default}u32; }}"));
            out.push("}".into());
        }
        Instr::Return(t) => {
            if *t == j2r_runtime::TypeDesc::Void {
                out.push("return ::core::result::Result::Ok(::j2r_runtime::Value::Void);".into());
            } else {
                out.push(
                    "return ::core::result::Result::Ok(::j2r_runtime::stack::stack_pop(&mut stack));"
                        .into(),
                );
            }
        }
        _ => unreachable!("j2r internal: not a branch arm"),
    }
}

/// The field-access arms (verbatim): getstatic's System.out/err/Locale.US/TYPE special cases and the generic slot law, putstatic's clinit ensure walk, getfield/putfield's S6/A7 law sites.
pub(crate) fn emit_field_ops(
    prog: &Program,
    _c: &ClassPlan,
    m: &MethodPlan,
    pc: u32,
    instr: &Instr,
    fault: &str,
    out: &mut Vec<String>,
) {
    match instr {
        Instr::GetStatic { class, field, .. } => {
            if class == "java/lang/System" && field == "out" {
                out.push(format!("stack.push({RT}::natives::system_out());"));
            } else if class == "java/lang/System" && field == "err" {
                out.push(format!("stack.push({RT}::natives::system_err());"));
            } else if class == "java/util/Locale" && field == "US" {
                // Corpus wave 3 (JAMA): the Locale.US constant object.
                out.push(format!("stack.push({RT}::natives::builtin_locale_us());"));
            } else if field == "TYPE" {
                if let Some(desc) = crate::primitive_type_field(class) {
                    out.push(format!("stack.push({RT}::reflect::class_value({desc:?}));"));
                } else {
                    let f = crate::resolve_field_pub(prog, class, field, true)
                        .expect("validated static field");
                    let slot = f.static_slot.expect("static field");
                    if let Some(marker) = jmm_boundary_marker(f, class) {
                        out.push(marker);
                    }
                    out.push(format!("stack.push(crate::statics::get({slot}u32));"));
                }
            } else {
                let f = crate::resolve_field_pub(prog, class, field, true)
                    .expect("validated static field");
                let slot = f.static_slot.expect("static field");
                if let Some(marker) = jmm_boundary_marker(f, class) {
                    out.push(marker);
                }
                let ensures = ensure_calls(prog, class);
                // `ensure` may dispatch into user code (<clinit>): refresh
                // the frame snapshot first (M2_PLAN §4.4).
                if !ensures.is_empty() {
                    out.push(FRAME_REFRESH.into());
                }
                for (id, expr) in ensures {
                    // CLINIT-THROW: a throwing clinit surfaces HERE as an
                    // Err — the EIIE wrapper (or the raw Error) must ride
                    // the method's exception table like any other throw.
                    out.push(format!(
                        "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, crate::statics::ensure({id}u32, {expr}));"
                    ));
                }
                out.push(format!("stack.push(crate::statics::get({slot}u32));"));
            }
        }
        Instr::PutStatic { class, field, .. } => {
            let f =
                crate::resolve_field_pub(prog, class, field, true).expect("validated static field");
            let slot = f.static_slot.expect("static field");
            if let Some(marker) = jmm_boundary_marker(f, class) {
                out.push(marker);
            }
            // ensure → possible <clinit> dispatch; refresh while the value
            // is still on the stack so the snapshot holds it.
            out.push(FRAME_REFRESH.into());
            out.push("let __v = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            for (id, expr) in ensure_calls(prog, class) {
                // CLINIT-THROW: the putstatic's clinit throw propagates.
                out.push(format!(
                    "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, crate::statics::ensure({id}u32, {expr}));"
                ));
            }
            out.push(format!("crate::statics::set({slot}u32, __v);"));
        }
        Instr::GetField { class, field, .. } => {
            let slot =
                crate::resolve_field_slot_pub(prog, class, field, false).expect("validated field");
            // Marker only for USER fields — a builtin-ancestor access
            // (corpus wave 2: FilterInputStream.in) resolves to a slot,
            // not a FieldPlan, and the checker's scope is the input
            // bytecode's own fields.
            if let Ok(f) = crate::resolve_field_pub(prog, class, field, false) {
                if let Some(marker) = jmm_boundary_marker(f, class) {
                    out.push(marker);
                }
            }
            // JEP 358 slice b: S6's shape (the field-read law).
            out.push(npe_pop_ref_line(m, pc, instr, fault, "__r"));
            out.push(format!(
                "stack.push({RT}::object::obj_get(&__r, {slot}usize));"
            ));
        }
        Instr::PutField { class, field, .. } => {
            let slot =
                crate::resolve_field_slot_pub(prog, class, field, false).expect("validated field");
            if let Ok(f) = crate::resolve_field_pub(prog, class, field, false) {
                if let Some(marker) = jmm_boundary_marker(f, class) {
                    out.push(marker);
                }
            }
            out.push("let __v = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            // JEP 358 slice b: the field-write law (A7's oracle).
            out.push(npe_pop_ref_line(m, pc, instr, fault, "__r"));
            out.push(format!("{RT}::object::obj_set(&__r, {slot}usize, __v);"));
        }
        _ => unreachable!("j2r internal: not a field arm"),
    }
}

/// The invokevirtual/interface arms (verbatim): the closed-world target, the vtable dispatch, the caller-tagged Constructor.newInstance law and the dynamic fallback walk.
pub(crate) fn emit_invoke_virtual_ops(
    prog: &Program,
    c: &ClassPlan,
    m: &MethodPlan,
    pc: u32,
    instr: &Instr,
    fault: &str,
    out: &mut Vec<String>,
) {
    match instr {
        Instr::InvokeVirtual { class, name, desc }
        | Instr::InvokeInterface { class, name, desc } => {
            // Call boundary: refresh the root snapshot before popping args
            // so the receiver and arguments stay rooted during the call
            // (M2_PLAN §4.4).
            out.push(FRAME_REFRESH.into());
            let n = arg_count(desc);
            out.push(format!(
                "let __a = {RT}::stack::pop_n(&mut stack, {n}usize);"
            ));
            // JEP 358: the receiver's null law carries the site's static
            // message when both halves are computable (slice a — the
            // invoke law sites, oracle NpeShapeProbe2 S1-S4+S8).
            out.push(npe_pop_ref_line(m, pc, instr, fault, "__r"));
            // R5 slice 1: when the closed-world resolution proves every
            // possible runtime receiver walks to ONE target, place that
            // target directly — the InvokeSpecial forms with the receiver.
            // The refresh, the pop_ref null law and the try-wrap are
            // byte-identical; only the dispatch walk is elided. Anything
            // unresolved keeps the dynamic path below.
            // R5 slice 2: when the resolution diverges (the polymorphic
            // residue), the codegen-time VTABLE places each receiver
            // class's own target behind ONE class read — the same
            // `jclass` field the dynamic walk starts from — with the full
            // dynamic walk as the fallback arm (byte-identical for every
            // receiver outside the closed-world arms).
            match crate::resolve_virtual_target_pub(prog, class, name, desc) {
                Some(crate::CallTarget::User { module, fn_name }) => {
                    out.push(format!(
                        "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, crate::classes::{module}::{fn_name}(::core::option::Option::Some(&__r), &__a));"
                    ));
                }
                Some(crate::CallTarget::Native { class: ncls }) => {
                    out.push(format!(
                        "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::natives::invoke_exact({ncls:?}, {name:?}, {desc:?}, ::core::option::Option::Some(&__r), &__a, crate::j2r_dispatch()));"
                    ));
                }
                None => match crate::resolve_vtable_pub(prog, class, name, desc) {
                    Some(arms) => {
                        out.push("let __v = {".into());
                        out.push(format!("let __cls = {RT}::object_class(&__r);"));
                        out.push("match &*__cls {".into());
                        for arm in &arms {
                            let pats = arm
                                .classes
                                .iter()
                                .map(|c| format!("{c:?}"))
                                .collect::<Vec<_>>()
                                .join(" | ");
                            match &arm.target {
                                crate::CallTarget::User { module, fn_name } => {
                                    out.push(format!(
                                        "{pats} => {RT}::j2r_try!('run, thrown, fault_pc, {fault}, crate::classes::{module}::{fn_name}(::core::option::Option::Some(&__r), &__a)),"
                                    ));
                                }
                                crate::CallTarget::Native { class: ncls } => {
                                    out.push(format!(
                                        "{pats} => {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::natives::invoke_exact({ncls:?}, {name:?}, {desc:?}, ::core::option::Option::Some(&__r), &__a, crate::j2r_dispatch())),"
                                    ));
                                }
                            }
                        }
                        out.push(format!(
                            "_ => {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::invoke_virtual(&__r, {class:?}, {name:?}, {desc:?}, &__a, crate::j2r_dispatch())),"
                        ));
                        // The match is the block's tail expression — no
                        // semicolon, or `__v` collapses to `()`.
                        out.push("}".into());
                        out.push("};".into());
                    }
                    None => {
                        // The reflection CALLER law (the different-package
                        // private-ctor residual, closed 2026-11-16): a
                        // closed-world compile knows the caller
                        // statically — the emitting class IS the JDK's
                        // Reflection.getCallerClass() for every site —
                        // so the builtin Constructor.newInstance invoke
                        // is emitted CALLER-TAGGED and the runtime
                        // applies the probed access matrix (probe
                        // S1-S8, scripts/evidence/private_ctor/). Builtin
                        // classes cannot be subclassed by user code in
                        // the closed world, so no other receiver class
                        // can reach this shape through a vtable arm.
                        if class == "java/lang/reflect/Constructor" && name == "newInstance" {
                            out.push(format!(
                                "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::reflect::ctor_new_instance_caller(crate::j2r_dispatch(), &__r, &__a, {caller:?}));",
                                caller = c.name,
                            ));
                        } else {
                            out.push(format!(
                                "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::invoke_virtual(&__r, {class:?}, {name:?}, {desc:?}, &__a, crate::j2r_dispatch()));"
                            ));
                        }
                    }
                },
            }
            if !returns_void(desc) {
                out.push("stack.push(__v);".into());
            }
        }
        _ => unreachable!("j2r internal: not a invoke_virtual arm"),
    }
}

/// The invokestatic + invokespecial arms (verbatim): the clinit ensure walk, the static target and the special receiver law.
pub(crate) fn emit_invoke_static_ops(
    prog: &Program,
    _c: &ClassPlan,
    m: &MethodPlan,
    pc: u32,
    instr: &Instr,
    fault: &str,
    out: &mut Vec<String>,
) {
    match instr {
        Instr::InvokeStatic { class, name, desc } => {
            out.push(FRAME_REFRESH.into());
            let n = arg_count(desc);
            out.push(format!(
                "let __a = {RT}::stack::pop_n(&mut stack, {n}usize);"
            ));
            match crate::resolve_call_pub(prog, class, name, desc).expect("validated call") {
                CallTarget::User { module, fn_name } => {
                    for (id, expr) in ensure_calls(prog, class) {
                        // CLINIT-THROW: the invokestatic's clinit throw
                        // propagates before the call.
                        out.push(format!(
                            "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, crate::statics::ensure({id}u32, {expr}));"
                        ));
                    }
                    out.push(format!(
                        "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, crate::classes::{module}::{fn_name}(::core::option::Option::None, &__a));"
                    ));
                }
                CallTarget::Native { class: ncls } => {
                    out.push(format!(
                        "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::natives::invoke_exact({ncls:?}, {name:?}, {desc:?}, ::core::option::Option::None, &__a, crate::j2r_dispatch()));"
                    ));
                }
            }
            if !returns_void(desc) {
                out.push("stack.push(__v);".into());
            }
        }
        Instr::InvokeSpecial { class, name, desc } => {
            out.push(FRAME_REFRESH.into());
            let n = arg_count(desc);
            out.push(format!(
                "let __a = {RT}::stack::pop_n(&mut stack, {n}usize);"
            ));
            // JEP 358: the same law on the invokespecial receiver.
            out.push(npe_pop_ref_line(m, pc, instr, fault, "__r"));
            match crate::resolve_call_pub(prog, class, name, desc).expect("validated call") {
                CallTarget::User { module, fn_name } => {
                    out.push(format!(
                        "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, crate::classes::{module}::{fn_name}(::core::option::Option::Some(&__r), &__a));"
                    ));
                }
                CallTarget::Native { class: ncls } => {
                    out.push(format!(
                        "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::natives::invoke_exact({ncls:?}, {name:?}, {desc:?}, ::core::option::Option::Some(&__r), &__a, crate::j2r_dispatch()));"
                    ));
                }
            }
            if !returns_void(desc) {
                out.push("stack.push(__v);".into());
            }
        }
        _ => unreachable!("j2r internal: not a invoke_static arm"),
    }
}

/// The invokedynamic arm (verbatim): the closure-construction and string-concat sites.
pub(crate) fn emit_invoke_dynamic_ops(
    _prog: &Program,
    c: &ClassPlan,
    _m: &MethodPlan,
    _pc: u32,
    instr: &Instr,
    fault: &str,
    out: &mut Vec<String>,
) {
    match instr {
        Instr::InvokeDynamic { index } => {
            out.push(FRAME_REFRESH.into());
            if let Some(plan) = c.lambdas.get(index) {
                // Closure construction (M3_PLAN §6.1): the captures are the
                // indy site's dynamic arguments; the refresh above rooted
                // them through the frame snapshot before the pop.
                let n = plan.capture_kinds.len();
                out.push(format!(
                    "let __a = {RT}::stack::pop_n(&mut stack, {n}usize);"
                ));
                out.push(format!(
                    "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::lambda::make_closure({}u8, {:?}, {:?}, {:?}, {:?}, {}u32, &__a));",
                    plan.shape, plan.interface, plan.target_class, plan.target_name, plan.target_desc, plan.ctor_slots
                ));
            } else {
                let plan = c.concats.get(index).expect("validated concat site");
                let n = plan.kinds.len();
                out.push(format!(
                    "let __a = {RT}::stack::pop_n(&mut stack, {n}usize);"
                ));
                let consts = plan
                    .constants
                    .iter()
                    .map(constant_literal)
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push(format!(
                    "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::concat::string_concat({:?}, &[{}], &__a, {:?}, crate::j2r_dispatch()));",
                    plan.recipe, consts, plan.kinds
                ));
            }
            out.push("stack.push(__v);".into());
        }
        _ => unreachable!("j2r internal: not a invoke_dynamic arm"),
    }
}

/// The monitor arms (verbatim): A8's enter oracle with the frame record/unrecord bookkeeping and the bare exit law.
pub(crate) fn emit_monitor_ops(
    _prog: &Program,
    _c: &ClassPlan,
    m: &MethodPlan,
    pc: u32,
    instr: &Instr,
    fault: &str,
    out: &mut Vec<String>,
) {
    match instr {
        Instr::MonitorEnter => {
            // JEP 358 slice b: A8's oracle ("Cannot enter synchronized
            // block"); MonitorExit keeps the bare law (unreachable from
            // javac with a null monitor — registered residual).
            out.push(npe_pop_ref_line(m, pc, instr, fault, "__r"));
            out.push(format!(
                "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::monitor::monitor_enter(&__r));"
            ));
            out.push("__frame.record_monitor(__r);".into());
        }
        Instr::MonitorExit => {
            out.push(format!(
                "let __r = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::pop_ref(&mut stack));"
            ));
            out.push(format!(
                "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::monitor::monitor_exit(&__r));"
            ));
            out.push("__frame.unrecord_monitor(&__r);".into());
        }
        _ => unreachable!("j2r internal: not a monitor arm"),
    }
}

/// The allocation arms (verbatim): the gc-point collection shapes for new/newarray/anewarray/multianewarray and the G2.6 collapsed array-literal fill.
pub(crate) fn emit_alloc_ops(
    prog: &Program,
    _c: &ClassPlan,
    _m: &MethodPlan,
    pc: u32,
    instr: &Instr,
    fault: &str,
    out: &mut Vec<String>,
) {
    match instr {
        Instr::New { class } => {
            // The one collection point shape (M2_PLAN §4.3): refresh the
            // frame snapshot, then gc_point — before the allocation and
            // before the <clinit> ensure dispatch.
            out.push(FRAME_REFRESH.into());
            out.push(format!("{RT}::heap::gc_point();"));
            new_obj_lines(prog, class, fault, out);
            out.push(format!(
                "stack.push({RT}::Value::Ref(::core::option::Option::Some(__nobj)));"
            ));
        }
        Instr::NewArray(prim) => {
            out.push(FRAME_REFRESH.into());
            out.push(format!("{RT}::heap::gc_point();"));
            let desc = prim.array_desc();
            out.push("let __len = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push(format!(
                "stack.push({RT}::Value::Ref(::core::option::Option::Some({RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::array::array_new_prim({desc:?}, __len)))));"
            ));
        }
        Instr::NewIntArrayFilled { prim, vals } => {
            // G2.6 arm a (peephole.rs): the collapsed array-literal fill.
            // The element source is a NESTED static — a separate
            // const-eval item, so the enclosing fn's MIR sees ONE call
            // instead of N aggregate operands (the measured
            // borrow-check/MIR wall). Fresh JRef per evaluation: same
            // newarray semantics as the uncollapsed form. The runtime
            // constructor applies the same per-prim narrowing as
            // array_store_i_raw, so the payload is byte-identical.
            out.push(FRAME_REFRESH.into());
            out.push(format!("{RT}::heap::gc_point();"));
            let sname = format!("__J2R_AFILL_{pc}");
            let lit = vals
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let desc = prim.array_desc();
            out.push(format!(
                "static {sname}: [i32; {}] = [{}];",
                vals.len(),
                lit
            ));
            out.push(format!(
                "stack.push({RT}::Value::Ref(::core::option::Option::Some({RT}::array::array_int_group_filled({desc:?}, &{sname}))));"
            ));
        }
        Instr::ANewArray { elem } => {
            out.push(FRAME_REFRESH.into());
            out.push(format!("{RT}::heap::gc_point();"));
            out.push("let __len = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push(format!(
                "stack.push({RT}::Value::Ref(::core::option::Option::Some({RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::array::array_new_ref({elem:?}, __len)))));"
            ));
        }
        Instr::MultiNewArray { desc, dims } => {
            out.push(FRAME_REFRESH.into());
            out.push(format!("{RT}::heap::gc_point();"));
            // The dimension sizes were pushed outermost-first, so pop them
            // back-to-front into __dim0..__dim(n-1).
            for i in (0..*dims).rev() {
                out.push(format!(
                    "let __dim{i} = ::j2r_runtime::pop_int(&mut stack);"
                ));
            }
            let list: Vec<String> = (0..*dims).map(|i| format!("__dim{i}")).collect();
            out.push(format!(
                "stack.push({RT}::Value::Ref(::core::option::Option::Some({RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::array::array_new_multi({desc:?}, &[{}])))));",
                list.join(", ")
            ));
        }
        _ => unreachable!("j2r internal: not a alloc arm"),
    }
}

/// The specialized-map arms (verbatim, G3.3 slices 2-4): the raw-prim put/get cores with the fused unbox law.
pub(crate) fn emit_map_prim_ops(
    _prog: &Program,
    _c: &ClassPlan,
    _m: &MethodPlan,
    _pc: u32,
    instr: &Instr,
    fault: &str,
    out: &mut Vec<String>,
) {
    match instr {
        // G3.3 slice 2 (peephole.rs): the specialized-map cores. The
        // value lands in the node's slot as the RAW prim — the box never
        // existed on this path; the put's return is discarded by the
        // core's own Pop and the get's result replaces the map operand
        // exactly like the invoke-result law.
        Instr::MapPutPrim { kind, .. } => {
            out.push(FRAME_REFRESH.into());
            out.push(format!("let __val = {RT}::stack::stack_pop(&mut stack);"));
            // The key pops as a plain Value — a null key is legal for
            // HashMap (the faithful put's args pop unchecked); only the
            // RECEIVER carries the pop_ref null check, like every invoke.
            out.push(format!("let __key = {RT}::stack::stack_pop(&mut stack);"));
            out.push(format!(
                "let __r = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::pop_ref(&mut stack));"
            ));
            // G3.3 slice 4: the storage kind routes the native — the
            // hash family shares hm_put_prim (its node-kind dispatch
            // follows the map's runtime class, so LinkedHashMap rides
            // with the tail-link law), TreeMap routes to the red-black
            // walk's own pair.
            let put_fn = match kind {
                crate::ir::MapKind::TreeMap => "tm_put_prim",
                _ => "hm_put_prim",
            };
            out.push(format!(
                "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::collections::{put_fn}(&__r, __key, __val, crate::j2r_dispatch()));"
            ));
        }
        Instr::MapGetPrim { kind, prim } => {
            out.push(FRAME_REFRESH.into());
            out.push(format!("let __key = {RT}::stack::stack_pop(&mut stack);"));
            out.push(format!(
                "let __r = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::pop_ref(&mut stack));"
            ));
            // The fused unbox law: the expected box class rides the
            // synthetic — a foreign box or a mismatched prim tag throws
            // the faithful checkcast's exact CCE (G3.3 slice 3). The
            // storage kind routes the native (G3.3 slice 4).
            let expect = match prim {
                crate::ir::PrimType::Double => "java/lang/Double",
                crate::ir::PrimType::Float => "java/lang/Float",
                crate::ir::PrimType::Long => "java/lang/Long",
                _ => "java/lang/Integer",
            };
            let get_fn = match kind {
                crate::ir::MapKind::TreeMap => "tm_get_prim",
                _ => "hm_get_prim",
            };
            out.push(format!(
                "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::collections::{get_fn}(&__r, __key, {expect:?}, crate::j2r_dispatch()));"
            ));
            out.push("stack.push(__v);".into());
        }
        _ => unreachable!("j2r internal: not a map_prim arm"),
    }
}

/// The throw and type-test arms (verbatim): athrow's pending-throw record, checkcast and instanceof.
pub(crate) fn emit_throw_cast_ops(
    _prog: &Program,
    _c: &ClassPlan,
    m: &MethodPlan,
    pc: u32,
    instr: &Instr,
    fault: &str,
    out: &mut Vec<String>,
) {
    match instr {
        Instr::AThrow => {
            // pop the exception object; on success record the pending throw
            // (the Ok path of j2r_try! would otherwise fall through!)
            // JEP 358 slice b: A9's oracle ("Cannot throw exception").
            out.push(npe_take_thrown_line(m, pc, instr, fault));
            out.push(format!(
                "thrown = ::core::option::Option::Some({RT}::Throw {{ obj: __thr }});"
            ));
            // the faulting pc for this throw is the athrow itself — without
            // this, a rethrow inside its own catch handler loops forever
            out.push(format!("fault_pc = {fault};"));
        }
        Instr::CheckCast { class } => {
            out.push(format!(
                "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::checkcast(::j2r_runtime::stack::stack_pop(&mut stack), {class:?}, crate::j2r_dispatch()));"
            ));
            out.push("stack.push(__v);".into());
        }
        Instr::InstanceOf { class } => {
            out.push("let __v = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            out.push(format!(
                "stack.push({RT}::Value::Int({RT}::j2r_instance_of(&__v, {class:?}, crate::j2r_dispatch()) as i32));"
            ));
        }
        _ => unreachable!("j2r internal: not a throw_cast arm"),
    }
}
