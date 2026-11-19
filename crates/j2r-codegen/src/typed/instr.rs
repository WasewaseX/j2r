//! `typed`::instr — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use crate::emit::{arg_count, f32_lit, f64_lit, next_pc_of};
use crate::ir::{Instr, Ldc, Ldc2};
use crate::readiness::{shuffle_slot_writes, Kind};

pub(crate) fn emit_instr_typed(ctx: &mut Ctx<'_>, pc: u32, instr: &Instr) {
    use Instr as I;
    let fault = format!("{pc}u32");
    let vs = ctx.vs(pc);
    let k = vs.len();
    match instr {
        I::Nop => {}
        I::AConstNull
        | I::IConst(_)
        | I::LConst(_)
        | I::FConst(_)
        | I::DConst(_)
        | I::Ldc(_)
        | I::Ldc2W(_)
        | I::Load(_)
        | I::Store(_) => {
            emit_const_local_typed(ctx, pc, &fault, instr, &vs, k);
        }
        I::ALoad(_) | I::ArrayLength => emit_array_load_typed(ctx, pc, &fault, instr, &vs, k),
        I::AStore(_) => emit_array_store_typed(ctx, pc, &fault, instr, &vs, k),
        I::Pop | I::Pop2 => {}
        I::Dup | I::DupX1 | I::DupX2 | I::Dup2 | I::Dup2X1 | I::Dup2X2 | I::Swap => {
            emit_shuffle_typed(ctx, pc, instr);
        }
        I::Arith { op, kind } => emit_arith_typed(ctx, pc, &fault, *op, *kind, &vs),
        I::Convert(conv) => emit_convert_typed(ctx, pc, *conv, &vs),
        I::IInc { idx, c } => emit_iinc_typed(ctx, pc, idx, c),
        I::Cmp(kind) => emit_cmp_typed(ctx, pc, *kind, &vs),
        I::IfZero(..)
        | I::IfICmp(..)
        | I::IfRefCmp { .. }
        | I::IfNull(_)
        | I::IfNonNull(_)
        | I::Goto(_)
        | I::Return(_) => emit_branch_typed(ctx, pc, instr, &vs, k),
        I::GetStatic { class, field, .. } => {
            emit_getstatic_typed(ctx, pc, &fault, class, field);
        }
        I::PutStatic { .. } | I::GetField { .. } | I::PutField { .. } => {
            emit_field_typed(ctx, pc, &fault, instr, &vs, k);
        }
        I::InvokeVirtual { .. }
        | I::InvokeInterface { .. }
        | I::InvokeSpecial { .. }
        | I::InvokeStatic { .. } => emit_invoke_typed(ctx, pc, &fault, instr, &vs, k),
        I::MapPutPrim { .. } | I::MapGetPrim { .. } => {
            emit_map_prim_typed(ctx, pc, &fault, instr, &vs, k);
        }
        I::MonitorEnter | I::MonitorExit => emit_monitor_typed(ctx, pc, &fault, instr, &vs, k),
        I::New { .. } | I::NewArray(_) | I::ANewArray { .. } | I::MultiNewArray { .. } => {
            emit_alloc_typed(ctx, pc, &fault, instr, &vs, k);
        }
        I::AThrow | I::CheckCast { .. } | I::InstanceOf { .. } => {
            emit_throw_cast_typed(ctx, pc, &fault, instr, &vs, k);
        }
        I::TableSwitch { .. } | I::LookupSwitch { .. } => {
            unreachable!("j2r internal: switch rejected by the typed plan")
        }
        I::NewIntArrayFilled { .. } => {
            unreachable!("j2r internal: array-literal fill rejected by the typed plan")
        }
        I::InvokeDynamic { .. } => {
            unreachable!("j2r internal: invokedynamic rejected by the typed plan")
        }
    }
}

/// The const/local arms (verbatim from `emit_instr_typed`, round-5 reshaping):
/// the pushes, the loads (the load snapshot law), the stores.
pub(crate) fn emit_const_local_typed(
    ctx: &mut Ctx<'_>,
    pc: u32,
    fault: &str,
    instr: &Instr,
    vs: &[usize],
    k: usize,
) {
    use Instr as I;
    match instr {
        I::AConstNull => {
            let slot = ctx.shape(pc).stack.len();
            let target = ctx.fresh_target(slot, pc);
            ctx.push_result(
                slot,
                target,
                format!("{RT}::Value::Ref(::core::option::Option::None)"),
                Src::Value,
                true,
                Kind::Ref,
            );
        }
        I::IConst(v) => {
            push_const(ctx, pc, fault, Src::Prim(Prim::I), v.to_string());
        }
        I::LConst(v) => {
            push_const(ctx, pc, fault, Src::Prim(Prim::L), format!("{v}i64"));
        }
        I::FConst(v) => {
            push_const(ctx, pc, fault, Src::Prim(Prim::F), f32_lit(*v));
        }
        I::DConst(v) => {
            push_const(ctx, pc, fault, Src::Prim(Prim::D), f64_lit(*v));
        }
        I::Ldc(Ldc::Int(v)) => {
            push_const(ctx, pc, fault, Src::Prim(Prim::I), v.to_string());
        }
        I::Ldc(Ldc::Float(v)) => {
            push_const(ctx, pc, fault, Src::Prim(Prim::F), f32_lit(*v));
        }
        I::Ldc(Ldc::String(s)) => {
            let slot = ctx.shape(pc).stack.len();
            let target = ctx.fresh_target(slot, pc);
            // intern_string is a side effect (the intern table) — the
            // def materializes into a temp so it runs exactly once.
            ctx.push_result(
                slot,
                target,
                format!(
                    "{RT}::Value::Ref(::core::option::Option::Some({RT}::intern_string({s:?})))"
                ),
                Src::Value,
                false,
                Kind::Ref,
            );
        }
        I::Ldc(Ldc::Class(name)) => {
            let slot = ctx.shape(pc).stack.len();
            let target = ctx.fresh_target(slot, pc);
            ctx.push_result(
                slot,
                target,
                format!("{RT}::reflect::class_value({name:?})"),
                Src::Value,
                false,
                Kind::Ref,
            );
        }
        I::Ldc2W(Ldc2::Long(v)) => {
            push_const(ctx, pc, fault, Src::Prim(Prim::L), format!("{v}i64"));
        }
        I::Ldc2W(Ldc2::Double(v)) => {
            push_const(ctx, pc, fault, Src::Prim(Prim::D), f64_lit(*v));
        }
        I::Load(idx) => {
            let expr = ctx.read_v(pc, *idx as usize);
            let (src, kind) = match ctx.site_v(pc, *idx as usize) {
                Site::Cell => (
                    Src::Value,
                    Kind::of(&ctx.shape(pc).locals[*idx as usize])
                        .expect("j2r internal: loaded local has no verified kind"),
                ),
                Site::Typed(sp) => {
                    let kind = match sp {
                        Prim::I => Kind::Int,
                        Prim::L => Kind::Long,
                        Prim::F => Kind::Float,
                        Prim::D => Kind::Double,
                    };
                    (Src::Prim(sp), kind)
                }
            };
            let slot = ctx.shape(pc).stack.len();
            let target = ctx.fresh_target(slot, pc);
            // Loads SNAPSHOT even though the read is side-effect-free: a
            // leaf read names the local's epoch variable, and a later
            // IInc/Store in the same block MUTATES that variable before
            // the pending entry is consumed (javac's post-increment
            // lowering: iload 7; iinc 7,1; use-the-old-7 — the def-time
            // temp is the value snapshot the slot write used to be).
            ctx.push_result(slot, target, expr, src, false, kind);
        }
        I::Store(idx) => {
            let top = vs[k - 1];
            let (expr, src) = ctx.read_for(pc, top);
            if let Some(t) = ctx.store_target(*idx as usize, pc) {
                ctx.assign(t, expr, src);
            }
        }
        _ => unreachable!("j2r internal: not a const_local arm"),
    }
}

/// The array read arms (verbatim from `emit_instr_typed`, round-5 reshaping):
/// aaload with the raw accessors for primitive element kinds, arraylength.
pub(crate) fn emit_array_load_typed(
    ctx: &mut Ctx<'_>,
    pc: u32,
    fault: &str,
    instr: &Instr,
    vs: &[usize],
    k: usize,
) {
    use Instr as I;
    match instr {
        I::ALoad(_) => {
            // pop the int index, then the array ref; the result lands on
            // the ref's slot (kind change → next epoch, or the cell).
            let idx_expr = ctx.read_prim(pc, vs[k - 1], Prim::I);
            ctx.out.push(format!("let __idx = {idx_expr};"));
            let arr_expr = ctx.try_ref_of(pc, vs[k - 2], fault);
            ctx.out.push(format!("let __arr = {arr_expr};"));
            let slot = vs[k - 2];
            let kind = elem_kind(instr);
            let target = ctx.refill_target(slot, pc, Some(kind));
            // Primitive element kinds take the RAW accessors — the typed
            // payload hands the number out directly (no Value clone, no
            // expect unwrap; the boxing-wave representation). Reference
            // kinds keep the Value-returning path.
            match kind {
                Kind::Int => {
                    let load = try_wrap(fault, format!("{RT}::array::array_load_i(&__arr, __idx)"));
                    ctx.push_result(slot, target, load, Src::Prim(Prim::I), false, Kind::Int);
                }
                Kind::Long => {
                    let load = try_wrap(fault, format!("{RT}::array::array_load_j(&__arr, __idx)"));
                    ctx.push_result(slot, target, load, Src::Prim(Prim::L), false, Kind::Long);
                }
                Kind::Float => {
                    let load = try_wrap(fault, format!("{RT}::array::array_load_f(&__arr, __idx)"));
                    ctx.push_result(slot, target, load, Src::Prim(Prim::F), false, Kind::Float);
                }
                Kind::Double => {
                    let load = try_wrap(fault, format!("{RT}::array::array_load_d(&__arr, __idx)"));
                    ctx.push_result(slot, target, load, Src::Prim(Prim::D), false, Kind::Double);
                }
                _ => {
                    let load = try_wrap(fault, format!("{RT}::array::array_load(&__arr, __idx)"));
                    ctx.push_result(slot, target, load, Src::Value, false, kind);
                }
            }
        }
        I::ArrayLength => {
            let arr_expr = ctx.try_ref_of(pc, vs[k - 1], fault);
            ctx.out.push(format!("let __arr = {arr_expr};"));
            let len = try_wrap(fault, format!("{RT}::array::array_length(&__arr)"));
            let slot = vs[k - 1];
            let target = ctx.refill_target(slot, pc, Some(Kind::Int));
            // __len is the raw i32 (try_wrap unwrapped the Result);
            // Src::Prim lets a Cell target wrap it in Value::Int.
            ctx.push_result(slot, target, len, Src::Prim(Prim::I), false, Kind::Int);
        }
        _ => unreachable!("j2r internal: not a array_load arm"),
    }
}

/// The array-store arm (verbatim from `emit_instr_typed`, round-5 reshaping):
/// raw int-group stores and the boxed ref path.
pub(crate) fn emit_array_store_typed(
    ctx: &mut Ctx<'_>,
    pc: u32,
    fault: &str,
    instr: &Instr,
    vs: &[usize],
    k: usize,
) {
    use Instr as I;
    match instr {
        I::AStore(_) => {
            let idx_expr = ctx.read_prim(pc, vs[k - 2], Prim::I);
            let arr_expr = ctx.try_ref_of(pc, vs[k - 3], fault);
            let store = match instr {
                I::AStore(crate::ir::ElemKind::IntGroup) => {
                    // The raw int-group store: the runtime narrows by the
                    // array's own descriptor (the bastore/castore/sastore
                    // law) — no Value construction at all.
                    let v_expr = ctx.read_prim(pc, vs[k - 1], Prim::I);
                    ctx.out.push(format!("let __idx = {idx_expr};"));
                    ctx.out.push(format!("let __arr = {arr_expr};"));
                    ctx.out.push(format!("let __sv = {v_expr};"));
                    try_wrap(
                        fault,
                        format!("{RT}::array::array_store_i_raw(&__arr, __idx, __sv)"),
                    )
                }
                I::AStore(crate::ir::ElemKind::Long) => {
                    let v_expr = ctx.read_prim(pc, vs[k - 1], Prim::L);
                    ctx.out.push(format!("let __idx = {idx_expr};"));
                    ctx.out.push(format!("let __arr = {arr_expr};"));
                    ctx.out.push(format!("let __sv = {v_expr};"));
                    try_wrap(
                        fault,
                        format!("{RT}::array::array_store_j_raw(&__arr, __idx, __sv)"),
                    )
                }
                I::AStore(crate::ir::ElemKind::Float) => {
                    let v_expr = ctx.read_prim(pc, vs[k - 1], Prim::F);
                    ctx.out.push(format!("let __idx = {idx_expr};"));
                    ctx.out.push(format!("let __arr = {arr_expr};"));
                    ctx.out.push(format!("let __sv = {v_expr};"));
                    try_wrap(
                        fault,
                        format!("{RT}::array::array_store_f_raw(&__arr, __idx, __sv)"),
                    )
                }
                I::AStore(crate::ir::ElemKind::Double) => {
                    let v_expr = ctx.read_prim(pc, vs[k - 1], Prim::D);
                    ctx.out.push(format!("let __idx = {idx_expr};"));
                    ctx.out.push(format!("let __arr = {arr_expr};"));
                    ctx.out.push(format!("let __sv = {v_expr};"));
                    try_wrap(
                        fault,
                        format!("{RT}::array::array_store_d_raw(&__arr, __idx, __sv)"),
                    )
                }
                _ => {
                    // Reference stores keep the boxed path (the ASE check
                    // needs the stored object's class).
                    let v_expr = ctx.wrap_s(pc, vs[k - 1]);
                    ctx.out.push(format!("let __idx = {idx_expr};"));
                    ctx.out.push(format!("let __arr = {arr_expr};"));
                    ctx.out.push(format!("let __v = {v_expr};"));
                    try_wrap(
                        fault,
                        format!(
                            "{RT}::array::array_store(&__arr, __idx, __v, crate::j2r_dispatch())"
                        ),
                    )
                }
            };
            ctx.out.push(format!("{store};"));
        }
        _ => unreachable!("j2r internal: not a array_store arm"),
    }
}

/// The stack-shuffle arm (verbatim from `emit_instr_typed`, round-5 reshaping).
pub(crate) fn emit_shuffle_typed(ctx: &mut Ctx<'_>, pc: u32, instr: &Instr) {
    use Instr as I;
    match instr {
        I::Dup | I::DupX1 | I::DupX2 | I::Dup2 | I::Dup2X1 | I::Dup2X2 | I::Swap => {
            // A shuffle may consume a value TWICE (dup) — the one read
            // the symbolic model guarantees per def. Materialize the
            // whole prefix first; the shuffle then runs on the slot
            // variables exactly as the old emitter did.
            ctx.materialize_all();
            let writes = shuffle_slot_writes(instr, &ctx.shape(pc).stack)
                .expect("j2r internal: shuffle map for a shuffle instruction");
            // Read every distinct source first (the runtime pops the
            // region before pushing), then write — no aliasing. The
            // refill write's new kind is the SOURCE's kind.
            let mut temps: Vec<(usize, String, Site)> = Vec::new();
            let mut seen: Vec<usize> = Vec::new();
            for &(_, src) in writes.iter() {
                if !seen.contains(&src) {
                    seen.push(src);
                    let expr = ctx.read_s(pc, src);
                    let site = ctx.site_s(pc, src);
                    let t = format!("__m{}", seen.len() - 1);
                    ctx.out.push(format!("let {t} = {expr};"));
                    temps.push((src, t, site));
                }
            }
            let in_len = ctx.shape(pc).stack.len();
            // One source may fan out to SEVERAL destinations (dup2_x2 and
            // friends): a `Value` temp can be moved only into its LAST
            // destination — every earlier destination takes a clone
            // (primitive temps are Copy; no clone needed there).
            let last_use: Vec<bool> = writes
                .iter()
                .enumerate()
                .map(|(i, &(_, src))| !writes[i + 1..].iter().any(|&(_, s)| s == src))
                .collect();
            for (wi, &(out_slot, src_slot)) in writes.iter().enumerate() {
                if out_slot == src_slot {
                    continue; // identity move
                }
                let (expr, src) = temps
                    .iter()
                    .find(|(s, _, _)| *s == src_slot)
                    .map(|(_, t, site)| {
                        (
                            t.clone(),
                            match site {
                                Site::Cell => Src::Value,
                                Site::Typed(sp) => Src::Prim(*sp),
                            },
                        )
                    })
                    .expect("j2r internal: shuffle source temp");
                let expr = match (&src, last_use[wi]) {
                    (Src::Value, false) => format!("{expr}.clone()"),
                    _ => expr,
                };
                let target = if out_slot < in_len {
                    let new_kind = kind_at(ctx, pc, src_slot);
                    ctx.refill_target(out_slot, pc, new_kind)
                } else {
                    ctx.fresh_target(out_slot, pc)
                };
                if let Some(t) = target {
                    ctx.assign(t, expr, src);
                }
            }
        }
        _ => unreachable!("j2r internal: not a shuffle arm"),
    }
}

/// The branch/return arms (verbatim from `emit_instr_typed`, round-5 reshaping):
/// materialize-below + backward-poll + the pc update.
pub(crate) fn emit_branch_typed(ctx: &mut Ctx<'_>, pc: u32, instr: &Instr, vs: &[usize], k: usize) {
    use Instr as I;
    match instr {
        I::IfZero(op, t) => {
            ctx.materialize_below(vs[k - 1]);
            ctx.backward_poll(pc, &[*t]);
            let a = ctx.read_prim(pc, vs[k - 1], Prim::I);
            ctx.out.push(format!("let __a = {a};"));
            ctx.out.push(format!(
                "if __a {} 0 {{ pc = {t}u32; }} else {{ pc = {}u32; }}",
                op.rust_op(),
                next_pc_of(ctx.m, pc)
            ));
        }
        I::IfICmp(op, t) => {
            ctx.materialize_below(vs[k - 2]);
            ctx.backward_poll(pc, &[*t]);
            let b = ctx.read_prim(pc, vs[k - 1], Prim::I);
            let a = ctx.read_prim(pc, vs[k - 2], Prim::I);
            ctx.out.push(format!("let __b = {b}; let __a = {a};"));
            ctx.out.push(format!(
                "if __a {} __b {{ pc = {t}u32; }} else {{ pc = {}u32; }}",
                op.rust_op(),
                next_pc_of(ctx.m, pc)
            ));
        }
        I::IfRefCmp { eq, target } => {
            ctx.materialize_below(vs[k - 2]);
            ctx.backward_poll(pc, &[*target]);
            let b = ctx.read_s(pc, vs[k - 1]);
            let a = ctx.read_s(pc, vs[k - 2]);
            ctx.out.push(format!("let __b = {b}; let __a = {a};"));
            let cond = if *eq {
                format!("{RT}::ref_eq(&__a, &__b)")
            } else {
                format!("!{RT}::ref_eq(&__a, &__b)")
            };
            ctx.out.push(format!(
                "if {cond} {{ pc = {target}u32; }} else {{ pc = {}u32; }}",
                next_pc_of(ctx.m, pc)
            ));
        }
        I::IfNull(t) => {
            ctx.materialize_below(vs[k - 1]);
            ctx.backward_poll(pc, &[*t]);
            let a = ctx.read_s(pc, vs[k - 1]);
            ctx.out.push(format!("let __a = {a};"));
            ctx.out.push(format!(
                "if __a.is_null() {{ pc = {t}u32; }} else {{ pc = {}u32; }}",
                next_pc_of(ctx.m, pc)
            ));
        }
        I::IfNonNull(t) => {
            ctx.materialize_below(vs[k - 1]);
            ctx.backward_poll(pc, &[*t]);
            let a = ctx.read_s(pc, vs[k - 1]);
            ctx.out.push(format!("let __a = {a};"));
            ctx.out.push(format!(
                "if !__a.is_null() {{ pc = {t}u32; }} else {{ pc = {}u32; }}",
                next_pc_of(ctx.m, pc)
            ));
        }
        I::Goto(t) => {
            // A goto has no operands of its own — the whole prefix flows
            // past the branch.
            ctx.materialize_all();
            ctx.backward_poll(pc, &[*t]);
            ctx.out.push(format!("pc = {t}u32;"));
        }
        I::Return(t) => {
            if *t == j2r_runtime::TypeDesc::Void {
                ctx.out
                    .push("return ::core::result::Result::Ok(::j2r_runtime::Value::Void);".into());
            } else {
                let v = ctx.wrap_s(pc, vs[k - 1]);
                ctx.out
                    .push(format!("return ::core::result::Result::Ok({v});"));
            }
        }
        _ => unreachable!("j2r internal: not a branch arm"),
    }
}

/// The field access arms (verbatim from `emit_instr_typed`, round-5 reshaping):
/// putstatic with ensure, getfield/putfield with the boundary markers.
pub(crate) fn emit_field_typed(
    ctx: &mut Ctx<'_>,
    pc: u32,
    fault: &str,
    instr: &Instr,
    vs: &[usize],
    k: usize,
) {
    use Instr as I;
    match instr {
        I::PutStatic { class, field, .. } => {
            let f = crate::resolve_field_pub(ctx.prog, class, field, true)
                .expect("validated static field");
            let slot = f.static_slot.expect("static field");
            if let Some(marker) = crate::emit::jmm_boundary_marker(f, class) {
                ctx.out.push(marker);
            }
            // ensure → possible <clinit> dispatch; refresh while the value
            // is still on the stack so the snapshot holds it.
            ctx.refresh(pc);
            let v = ctx.wrap_s(pc, vs[k - 1]);
            ctx.out.push(format!("let __v = {v};"));
            for (id, expr) in crate::emit::ensure_calls(ctx.prog, class) {
                // CLINIT-THROW: the clinit throw rides the exception table
                // in typed bodies too (same j2r_try lowering).
                ctx.out.push(format!(
                    "{};",
                    try_wrap(fault, format!("crate::statics::ensure({id}u32, {expr})"))
                ));
            }
            ctx.out
                .push(format!("crate::statics::set({slot}u32, __v);"));
        }
        I::GetField { class, field, .. } => {
            let slot = crate::resolve_field_slot_pub(ctx.prog, class, field, false)
                .expect("validated field");
            if let Ok(f) = crate::resolve_field_pub(ctx.prog, class, field, false) {
                if let Some(marker) = crate::emit::jmm_boundary_marker(f, class) {
                    ctx.out.push(marker);
                }
            }
            let r = ctx.try_ref_of(pc, vs[k - 1], fault);
            ctx.out.push(format!("let __r = {r};"));
            // obj_get returns a plain `Value` (the NPE is already raised
            // by ref_of above) — exactly the default body's law; no
            // j2r_try wrap here. A typed target keeps the raw Value in
            // the def-time temp (the expect hoists here); a cell target
            // stores as before.
            let get = format!("{RT}::object::obj_get(&__r, {slot}usize)");
            let top = vs[k - 1];
            // The kind comes from the USER field's descriptor; a
            // builtin-ancestor field (not a FieldPlan) is an object ref —
            // the corpus builtin fields (FilterInputStream.in et al) are
            // refs by construction.
            let kind = match crate::resolve_field_pub(ctx.prog, class, field, false) {
                Ok(f) => desc_kind_of(&f.desc),
                Err(_) => Kind::Ref,
            };
            let target = ctx.refill_target(top, pc, Some(kind));
            ctx.push_result(top, target, get, Src::Value, false, kind);
        }
        I::PutField { class, field, .. } => {
            let slot = crate::resolve_field_slot_pub(ctx.prog, class, field, false)
                .expect("validated field");
            if let Ok(f) = crate::resolve_field_pub(ctx.prog, class, field, false) {
                if let Some(marker) = crate::emit::jmm_boundary_marker(f, class) {
                    ctx.out.push(marker);
                }
            }
            let v = ctx.wrap_s(pc, vs[k - 1]);
            ctx.out.push(format!("let __v = {v};"));
            let r = ctx.try_ref_of(pc, vs[k - 2], fault);
            ctx.out.push(format!("let __r = {r};"));
            ctx.out
                .push(format!("{RT}::object::obj_set(&__r, {slot}usize, __v);"));
        }
        _ => unreachable!("j2r internal: not a field arm"),
    }
}

/// The invoke arms (verbatim from `emit_instr_typed`, round-5 reshaping): the
/// call-boundary refresh, the G3.2 slice-2 stack-array args, and the
/// User/Native call targets.
pub(crate) fn emit_invoke_typed(
    ctx: &mut Ctx<'_>,
    pc: u32,
    fault: &str,
    instr: &Instr,
    vs: &[usize],
    k: usize,
) {
    use Instr as I;
    match instr {
        I::InvokeVirtual { class, name, desc } | I::InvokeInterface { class, name, desc } => {
            // Call boundary: refresh the root snapshot before the args
            // leave the operand positions (M2_PLAN §4.4).
            ctx.refresh(pc);
            let n = arg_count(desc);
            let args: Vec<String> = ((k - n)..k).map(|i| ctx.wrap_s(pc, vs[i])).collect();
            // G3.2 slice 2: the arg vec is one-shot into invoke_exact / a
            // direct compiled-method call — both take &[Value], so a
            // fixed-size stack array coerces identically and kills the
            // per-call heap allocation + Vec drop glue (228 sites in the
            // JAMA typed crate). Clones are unchanged (GC-root parity).
            if args.is_empty() {
                ctx.out.push(format!("let __a: [{RT}::Value; 0] = [];"));
            } else {
                ctx.out.push(format!("let __a = [{}];", args.join(", ")));
            }
            let r = ctx.try_ref_of(pc, vs[k - n - 1], fault);
            ctx.out.push(format!("let __r = {r};"));
            let call = try_wrap(
                fault,
                format!(
                    "{RT}::invoke_virtual(&__r, {class:?}, {name:?}, {desc:?}, &__a, crate::j2r_dispatch())"
                ),
            );
            write_invoke_result(ctx, pc, vs[k - n - 1], desc, call);
        }
        I::InvokeSpecial { class, name, desc } => {
            ctx.refresh(pc);
            let n = arg_count(desc);
            let args: Vec<String> = ((k - n)..k).map(|i| ctx.wrap_s(pc, vs[i])).collect();
            // G3.2 slice 2: the arg vec is one-shot into invoke_exact / a
            // direct compiled-method call — both take &[Value], so a
            // fixed-size stack array coerces identically and kills the
            // per-call heap allocation + Vec drop glue (228 sites in the
            // JAMA typed crate). Clones are unchanged (GC-root parity).
            if args.is_empty() {
                ctx.out.push(format!("let __a: [{RT}::Value; 0] = [];"));
            } else {
                ctx.out.push(format!("let __a = [{}];", args.join(", ")));
            }
            let r = ctx.try_ref_of(pc, vs[k - n - 1], fault);
            ctx.out.push(format!("let __r = {r};"));
            match crate::resolve_call_pub(ctx.prog, class, name, desc).expect("validated call") {
                crate::CallTarget::User { module, fn_name } => {
                    let call = try_wrap(
                        fault,
                        format!(
                            "crate::classes::{module}::{fn_name}(::core::option::Option::Some(&__r), &__a)"
                        ),
                    );
                    write_invoke_result(ctx, pc, vs[k - n - 1], desc, call);
                }
                crate::CallTarget::Native { class: ncls } => {
                    let call = try_wrap(
                        fault,
                        format!(
                            "{RT}::natives::invoke_exact({ncls:?}, {name:?}, {desc:?}, ::core::option::Option::Some(&__r), &__a, crate::j2r_dispatch())"
                        ),
                    );
                    write_invoke_result(ctx, pc, vs[k - n - 1], desc, call);
                }
            }
        }
        I::InvokeStatic { class, name, desc } => {
            ctx.refresh(pc);
            let n = arg_count(desc);
            let args: Vec<String> = ((k - n)..k).map(|i| ctx.wrap_s(pc, vs[i])).collect();
            // G3.2 slice 2: the arg vec is one-shot into invoke_exact / a
            // direct compiled-method call — both take &[Value], so a
            // fixed-size stack array coerces identically and kills the
            // per-call heap allocation + Vec drop glue (228 sites in the
            // JAMA typed crate). Clones are unchanged (GC-root parity).
            if args.is_empty() {
                ctx.out.push(format!("let __a: [{RT}::Value; 0] = [];"));
            } else {
                ctx.out.push(format!("let __a = [{}];", args.join(", ")));
            }
            match crate::resolve_call_pub(ctx.prog, class, name, desc).expect("validated call") {
                crate::CallTarget::User { module, fn_name } => {
                    for (id, expr) in crate::emit::ensure_calls(ctx.prog, class) {
                        // CLINIT-THROW: same lowering as the default body.
                        ctx.out.push(format!(
                            "{};",
                            try_wrap(fault, format!("crate::statics::ensure({id}u32, {expr})"))
                        ));
                    }
                    let call = try_wrap(
                        fault,
                        format!(
                            "crate::classes::{module}::{fn_name}(::core::option::Option::None, &__a)"
                        ),
                    );
                    write_invoke_result_static(ctx, pc, vs, k, n, desc, call);
                }
                crate::CallTarget::Native { class: ncls } => {
                    let call = try_wrap(
                        fault,
                        format!(
                            "{RT}::natives::invoke_exact({ncls:?}, {name:?}, {desc:?}, ::core::option::Option::None, &__a, crate::j2r_dispatch())"
                        ),
                    );
                    write_invoke_result_static(ctx, pc, vs, k, n, desc, call);
                }
            }
        }
        _ => unreachable!("j2r internal: not a invoke arm"),
    }
}

/// The G3.3 specialized-map core arms (verbatim from `emit_instr_typed`,
/// round-5 reshaping).
pub(crate) fn emit_map_prim_typed(
    ctx: &mut Ctx<'_>,
    pc: u32,
    fault: &str,
    instr: &Instr,
    vs: &[usize],
    k: usize,
) {
    use Instr as I;
    match instr {
        // G3.3 slice 2 (peephole.rs): the specialized-map cores. The
        // prim value flows straight from the typed stack read into the
        // node's slot — no Value::Ref box, no valueOf dispatch, no unbox
        // dispatch. The put's return is discarded (the core's own Pop);
        // the get's result refills the map operand's slot under the
        // kind-change law, with the expect_* unwrap hoisted by the
        // unified push law (Src::Value with the prim kind).
        I::MapPutPrim { kind, prim } => {
            ctx.refresh(pc);
            let (ctor, tp) = boxed_ctor(*prim);
            let val = format!(
                "{RT}::Value::{}({})",
                ctor,
                ctx.read_prim(pc, vs[k - 1], tp)
            );
            let key = ctx.wrap_s(pc, vs[k - 2]);
            let r = ctx.try_ref_of(pc, vs[k - 3], fault);
            ctx.out.push(format!("let __k = {key};"));
            ctx.out.push(format!("let __val = {val};"));
            ctx.out.push(format!("let __r = {r};"));
            // G3.3 slice 4: the storage kind routes the native — the
            // hash family shares hm_put_prim (node-kind dispatch on the
            // map's runtime class; LinkedHashMap rides with the
            // tail-link law), TreeMap routes to the red-black walk.
            let put_fn = match kind {
                crate::ir::MapKind::TreeMap => "tm_put_prim",
                _ => "hm_put_prim",
            };
            let call = try_wrap(
                fault,
                format!("{RT}::collections::{put_fn}(&__r, __k, __val, crate::j2r_dispatch())"),
            );
            ctx.out.push(format!("let __v = {call};"));
        }
        I::MapGetPrim { kind, prim } => {
            ctx.refresh(pc);
            let (_, tp) = boxed_ctor(*prim);
            // The fused unbox law: the expected box class rides the
            // synthetic — a foreign box or a mismatched prim tag throws
            // the faithful checkcast's exact CCE (G3.3 slice 3).
            let expect = match prim {
                crate::ir::PrimType::Double => "java/lang/Double",
                crate::ir::PrimType::Float => "java/lang/Float",
                crate::ir::PrimType::Long => "java/lang/Long",
                _ => "java/lang/Integer",
            };
            let key = ctx.wrap_s(pc, vs[k - 1]);
            let r = ctx.try_ref_of(pc, vs[k - 2], fault);
            ctx.out.push(format!("let __k = {key};"));
            ctx.out.push(format!("let __r = {r};"));
            // The storage kind routes the native (G3.3 slice 4).
            let get_fn = match kind {
                crate::ir::MapKind::TreeMap => "tm_get_prim",
                _ => "hm_get_prim",
            };
            let call = try_wrap(
                fault,
                format!(
                    "{RT}::collections::{get_fn}(&__r, __k, {expect:?}, crate::j2r_dispatch())"
                ),
            );
            let kind = match tp {
                Prim::D => Kind::Double,
                Prim::F => Kind::Float,
                Prim::L => Kind::Long,
                Prim::I => Kind::Int,
            };
            let slot = vs[k - 2];
            let target = ctx.refill_target(slot, pc, Some(kind));
            ctx.push_result(slot, target, call, Src::Value, false, kind);
        }
        _ => unreachable!("j2r internal: not a map_prim arm"),
    }
}

/// The monitor arms (verbatim from `emit_instr_typed`, round-5 reshaping).
pub(crate) fn emit_monitor_typed(
    ctx: &mut Ctx<'_>,
    pc: u32,
    fault: &str,
    instr: &Instr,
    vs: &[usize],
    k: usize,
) {
    use Instr as I;
    match instr {
        I::MonitorEnter => {
            let r = ctx.try_ref_of(pc, vs[k - 1], fault);
            ctx.out.push(format!("let __r = {r};"));
            let enter = try_wrap(fault, format!("{RT}::monitor::monitor_enter(&__r)"));
            ctx.out.push(format!("{enter};"));
        }
        I::MonitorExit => {
            let r = ctx.try_ref_of(pc, vs[k - 1], fault);
            ctx.out.push(format!("let __r = {r};"));
            let exit = try_wrap(fault, format!("{RT}::monitor::monitor_exit(&__r)"));
            ctx.out.push(format!("{exit};"));
        }
        _ => unreachable!("j2r internal: not a monitor arm"),
    }
}

/// The allocation arms (verbatim from `emit_instr_typed`, round-5 reshaping):
/// the gc_point collection shapes and the in-place refills.
pub(crate) fn emit_alloc_typed(
    ctx: &mut Ctx<'_>,
    pc: u32,
    fault: &str,
    instr: &Instr,
    vs: &[usize],
    k: usize,
) {
    use Instr as I;
    match instr {
        I::New { class } => {
            // The one collection point shape (M2_PLAN §4.3).
            ctx.refresh(pc);
            ctx.out.push(format!("{RT}::heap::gc_point();"));
            crate::emit::new_obj_lines(ctx.prog, class, fault, &mut ctx.out);
            let slot = ctx.shape(pc).stack.len();
            let target = ctx.fresh_target(slot, pc);
            ctx.push_result(
                slot,
                target,
                format!("{RT}::Value::Ref(::core::option::Option::Some(__nobj))"),
                Src::Value,
                false,
                Kind::Ref,
            );
        }
        I::NewArray(prim) => {
            ctx.refresh(pc);
            ctx.out.push(format!("{RT}::heap::gc_point();"));
            let desc = prim.array_desc();
            let len = ctx.read_prim(pc, vs[k - 1], Prim::I);
            ctx.out.push(format!("let __len = {len};"));
            let new = try_wrap(
                fault,
                format!("{RT}::array::array_new_prim({desc:?}, __len)"),
            );
            ctx.out.push(format!("let __narr = {new};"));
            // The result REFILLS the popped length operand's slot (the
            // value_effect law: array construction pushes in place) — a
            // fresh stack.len() target would be beyond the stack and the
            // write would be skipped entirely.
            let slot = vs[k - 1];
            let target = ctx.refill_target(slot, pc, Some(Kind::Ref));
            ctx.push_result(
                slot,
                target,
                format!("{RT}::Value::Ref(::core::option::Option::Some(__narr))"),
                Src::Value,
                false,
                Kind::Ref,
            );
        }
        I::ANewArray { elem } => {
            ctx.refresh(pc);
            ctx.out.push(format!("{RT}::heap::gc_point();"));
            let len = ctx.read_prim(pc, vs[k - 1], Prim::I);
            ctx.out.push(format!("let __len = {len};"));
            let new = try_wrap(
                fault,
                format!("{RT}::array::array_new_ref({elem:?}, __len)"),
            );
            ctx.out.push(format!("let __narr = {new};"));
            // In-place refill of the popped length slot (see NewArray).
            let slot = vs[k - 1];
            let target = ctx.refill_target(slot, pc, Some(Kind::Ref));
            ctx.push_result(
                slot,
                target,
                format!("{RT}::Value::Ref(::core::option::Option::Some(__narr))"),
                Src::Value,
                false,
                Kind::Ref,
            );
        }
        I::MultiNewArray { desc, dims } => {
            ctx.refresh(pc);
            ctx.out.push(format!("{RT}::heap::gc_point();"));
            // Dimension sizes pushed outermost-first: __dim0 is the
            // deepest of the top `dims` values (the runtime pops
            // back-to-front into __dim0..__dim(n-1)).
            for i in (0..*dims).rev() {
                let d = ctx.read_prim(pc, vs[k - (*dims as usize) + i as usize], Prim::I);
                ctx.out.push(format!("let __dim{i} = {d};"));
            }
            let list: Vec<String> = (0..*dims).map(|i| format!("__dim{i}")).collect();
            let new = try_wrap(
                fault,
                format!(
                    "{RT}::array::array_new_multi({desc:?}, &[{}])",
                    list.join(", ")
                ),
            );
            ctx.out.push(format!("let __narr = {new};"));
            // In-place refill of the FIRST popped dimension slot (the
            // deepest of the dims) — the result lands exactly there.
            let slot = vs[k - *dims as usize];
            let target = ctx.refill_target(slot, pc, Some(Kind::Ref));
            ctx.push_result(
                slot,
                target,
                format!("{RT}::Value::Ref(::core::option::Option::Some(__narr))"),
                Src::Value,
                false,
                Kind::Ref,
            );
        }
        _ => unreachable!("j2r internal: not a alloc arm"),
    }
}

/// The athrow/checkcast/instanceof arms (verbatim from `emit_instr_typed`,
/// round-5 reshaping).
pub(crate) fn emit_throw_cast_typed(
    ctx: &mut Ctx<'_>,
    pc: u32,
    fault: &str,
    instr: &Instr,
    vs: &[usize],
    k: usize,
) {
    use Instr as I;
    match instr {
        I::AThrow => {
            let thr = ctx.try_ref_of(pc, vs[k - 1], fault);
            ctx.out.push(format!("let __thr = {thr};"));
            ctx.out.push(format!(
                "thrown = ::core::option::Option::Some({RT}::Throw {{ obj: __thr }});"
            ));
            // The faulting pc for this throw is the athrow itself.
            ctx.out.push(format!("fault_pc = {fault};"));
        }
        I::CheckCast { class } => {
            let v = ctx.read_s(pc, vs[k - 1]);
            let cc = try_wrap(
                fault,
                format!("{RT}::checkcast({v}, {class:?}, crate::j2r_dispatch())"),
            );
            let slot = vs[k - 1];
            let target = ctx.refill_target(slot, pc, Some(Kind::Ref));
            ctx.push_result(slot, target, cc, Src::Value, false, Kind::Ref);
        }
        I::InstanceOf { class } => {
            let v = ctx.read_s(pc, vs[k - 1]);
            ctx.out.push(format!("let __v = {v};"));
            let io =
                format!("{RT}::j2r_instance_of(&__v, {class:?}, crate::j2r_dispatch()) as i32");
            let slot = vs[k - 1];
            let target = ctx.refill_target(slot, pc, Some(Kind::Int));
            ctx.push_result(slot, target, io, Src::Prim(Prim::I), false, Kind::Int);
        }
        _ => unreachable!("j2r internal: not a throw_cast arm"),
    }
}

/// The iinc arm (verbatim from `emit_instr_typed`, round-5 reshaping).
pub(crate) fn emit_iinc_typed(ctx: &mut Ctx<'_>, pc: u32, idx: &u16, c: &i32) {
    let j = *idx as usize;
    let expr = ctx.read_v(pc, j);
    let line = match ctx.site_v(pc, j) {
        Site::Typed(Prim::I) => {
            let (target, _) = ctx.store_target(j, pc).expect("j2r internal: iinc target");
            format!("{target} = {expr}.wrapping_add({c});")
        }
        Site::Cell => {
            let (target, _) = ctx.store_target(j, pc).expect("j2r internal: iinc target");
            format!("{target} = {RT}::Value::Int(v{j}.expect_int().wrapping_add({c}));")
        }
        Site::Typed(other) => unreachable!("j2r internal: iinc on {other:?}"),
    };
    ctx.out.push(line);
}

/// The `Kind` of a stack slot's verified type at `pc`.
pub(crate) fn kind_at(ctx: &Ctx<'_>, pc: u32, slot: usize) -> Option<Kind> {
    Kind::of(&ctx.shape(pc).stack[slot])
}

/// A pure push of a primitive constant (or a `Value`-typed one).
pub(crate) fn push_const(ctx: &mut Ctx<'_>, pc: u32, _fault: &str, src: Src, lit: String) {
    let kind = match src {
        Src::Prim(Prim::I) => Kind::Int,
        Src::Prim(Prim::L) => Kind::Long,
        Src::Prim(Prim::F) => Kind::Float,
        _ => Kind::Double,
    };
    let slot = ctx.shape(pc).stack.len();
    let target = ctx.fresh_target(slot, pc);
    ctx.push_result(slot, target, lit, src, true, kind);
}

/// The element `Kind` of an `xXaload` instruction (the refill's kind).
pub(crate) fn elem_kind(instr: &Instr) -> Kind {
    match instr {
        Instr::ALoad(crate::ir::ElemKind::IntGroup) => Kind::Int,
        Instr::ALoad(crate::ir::ElemKind::Long) => Kind::Long,
        Instr::ALoad(crate::ir::ElemKind::Float) => Kind::Float,
        Instr::ALoad(crate::ir::ElemKind::Double) => Kind::Double,
        _ => Kind::Ref,
    }
}

/// The G3.3 map-core prim mapping: the `Value` constructor name and the
/// typed `Prim` of a boxed class's payload. Only the four boxable prims
/// reach the synthetic instructions (the peephole matches the four
/// `valueOf`/`xValue()` pairs).
pub(crate) fn boxed_ctor(p: crate::ir::PrimType) -> (&'static str, Prim) {
    match p {
        crate::ir::PrimType::Double => ("Double", Prim::D),
        crate::ir::PrimType::Float => ("Float", Prim::F),
        crate::ir::PrimType::Long => ("Long", Prim::L),
        _ => ("Int", Prim::I),
    }
}

/// The `Kind` of a field/call-result descriptor's type. The FIRST byte
/// decides: field and return descriptors are single types, and `[[D`
/// ends in 'D' while being a REFERENCE (the last-byte heuristic
/// misclassified array-of-double fields and call results — latent in the
/// slot-write law, where Cell targets carry no kind assert, and LOUD in
/// the unification layer's def-time expect). B/C/S/Z all narrow to Int
/// (the runtime represents them as Value::Int).
pub(crate) fn desc_kind_of(desc: &str) -> Kind {
    match desc.as_bytes().first() {
        Some(b'[') | Some(b'L') => Kind::Ref,
        Some(b'J') => Kind::Long,
        Some(b'F') => Kind::Float,
        Some(b'D') => Kind::Double,
        _ => Kind::Int,
    }
}
