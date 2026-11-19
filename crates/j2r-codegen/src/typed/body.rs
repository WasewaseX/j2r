//! `typed`::body — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use crate::emit::{compute_leaders, is_terminator};
use crate::{ClassPlan, MethodPlan, Program};
use j2r_runtime::{parse_method_desc, type_slots};

/// The kind of an expression being assigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Src {
    /// A bare primitive of this kind.
    Prim(Prim),
    /// A `Value` (cell read, helper return).
    Value,
}

/// Plans and emits the typed body for one method, or names the blocker.
pub(crate) fn try_emit(
    prog: &Program,
    c: &ClassPlan,
    m: &MethodPlan,
) -> Result<String, Unsupported> {
    let shapes = crate::verify::verify_method_shapes(m, &c.concats, &c.lambdas)
        .map_err(|_| Unsupported::PlanSanity)?;
    let p = plan(m, &shapes)?;
    let ctx = Ctx {
        prog,
        c,
        m,
        p: &p,
        shapes: &shapes,
        pcs: m.instrs.iter().map(|(pc, _)| *pc).collect(),
        out: Vec::new(),
        sym: Default::default(),
        tmp_n: 0,
        sym_pushes: 0,
    };
    Ok(emit_body(ctx))
}

pub(crate) fn try_wrap(fault: &str, expr: String) -> String {
    format!("{RT}::j2r_try!('run, thrown, fault_pc, {fault}, {expr})")
}

/// The typed-body method header (verbatim from `emit_body`, round-5
/// reshaping): the banner comment, the fn signature, the FrameGuard
/// (identical to the default body, M2_PLAN §3.1), the argument unmarshal
/// through the transient Vec (the runtime's unmarshal law is unchanged),
/// and the instance receiver.
pub(crate) fn emit_typed_header(m: &MethodPlan, p: &Plan, out: &mut String) {
    let instance = !m.is_static;
    let unmarshal_start = if instance { 1usize } else { 0usize };
    let locals_len = m.max_locals.max(if instance { 1 } else { 0 });
    let _ = writeln!(
        out,
        "// j2r: typed-operand emission ({} typed vars, {} Value cells)",
        p.typed_vars, p.cell_vars
    );
    let _ = writeln!(
        out,
        "pub fn {}(recv: ::core::option::Option<&{RT}::JRef>, args: &[{RT}::Value]) -> ::core::result::Result<{RT}::Value, {RT}::Throw> {{",
        m.fn_name
    );
    let _ = writeln!(
        out,
        "    let __frame = match {RT}::frame::FrameGuard::enter() {{"
    );
    let _ = writeln!(out, "        ::core::result::Result::Ok(f) => f,");
    let _ = writeln!(
        out,
        "        ::core::result::Result::Err(t) => return ::core::result::Result::Err(t),"
    );
    let _ = writeln!(out, "    }};");
    let _ = writeln!(
        out,
        "    let mut __loc_tmp: ::std::vec::Vec<{RT}::Value> = ::std::vec![{RT}::Value::Void; {locals_len}usize];"
    );
    let _ = writeln!(
        out,
        "    {RT}::unmarshal_args(&mut __loc_tmp, args, {:?}, {unmarshal_start}usize).expect(\"j2r internal: argument unmarshal\");",
        m.desc
    );
    if instance {
        let _ = writeln!(
            out,
            "    if let ::core::option::Option::Some(this_ref) = recv {{"
        );
        let _ = writeln!(
            out,
            "        __loc_tmp[0usize] = {RT}::Value::Ref(::core::option::Option::Some(this_ref.clone()));"
        );
        let _ = writeln!(out, "    }}");
    }
}

/// The typed/cell local declarations (verbatim from `emit_body`,
/// round-5 reshaping). Cells clone out; typed epochs take the entry value
/// from the unmarshal Vec when the epoch covers entry, defaults else.
/// ONLY incoming argument slots may be read from the unmarshal Vec:
/// slot 0 is `this` for instance methods, then the descriptor's
/// parameters (long/double take two slots). A non-argument local whose
/// epoch covers entry must take the typed default — the JVM verifier
/// guarantees a store before any read — because the unmarshal Vec holds
/// Void there. Live catch: BigJson's skipWs()V has a loop-live `c`
/// local that is not an argument; reading __loc_tmp[1] panicked
/// "expected int value, got Void" at runtime.
pub(crate) fn emit_typed_locals(
    m: &MethodPlan,
    p: &Plan,
    shapes_max_locals: usize,
    out: &mut String,
) {
    let instance = !m.is_static;
    let locals_len = m
        .max_locals
        .max(shapes_max_locals)
        .max(if instance { 1 } else { 0 });
    let arg_slots = {
        let (params, _) = parse_method_desc(&m.desc)
            .unwrap_or_else(|| panic!("j2r internal: bad descriptor {:?}", m.desc));
        let mut n = if instance { 1usize } else { 0usize };
        for t in &params {
            n += type_slots(t);
        }
        n
    };
    let first_pc = m.instrs.first().map(|(pc, _)| *pc).unwrap_or(0);
    for (j, var) in p.locals.iter().enumerate() {
        match var {
            Var::Dead => {}
            Var::Cell => {
                let read = j < arg_slots && j < locals_len;
                if read {
                    let _ = writeln!(
                        out,
                        "    let mut v{j}: {RT}::Value = __loc_tmp[{j}usize].clone();"
                    );
                } else {
                    let _ = writeln!(out, "    let mut v{j}: {RT}::Value = {RT}::Value::Void;");
                }
            }
            Var::Typed(epochs) => {
                for (e, (start, prim)) in epochs.iter().enumerate() {
                    let name = p.local_name(j, e);
                    let ty = prim.rust_ty();
                    if e == 0 && *start <= first_pc && j < arg_slots && j < locals_len {
                        let f = prim.expect_fn();
                        let _ =
                            writeln!(out, "    let mut {name}: {ty} = __loc_tmp[{j}usize].{f}();");
                    } else {
                        let default = prim.default_lit();
                        let _ = writeln!(out, "    let mut {name}: {ty} = {default};");
                    }
                }
            }
        }
    }
}

/// The ACC_SYNCHRONIZED prologue (verbatim from `emit_body`, round-5
/// reshaping) — byte-identical to the default body.
pub(crate) fn emit_typed_sync(m: &MethodPlan, c: &ClassPlan, out: &mut String) {
    if m.is_sync {
        let instance = !m.is_static;
        if instance {
            out.push_str("    if let ::core::option::Option::Some(r) = recv {\n");
            out.push_str(&format!(
                "        if let ::core::result::Result::Err(t) = {RT}::monitor::monitor_enter(r) {{ return ::core::result::Result::Err(t); }}\n"
            ));
            out.push_str("        __frame.record_monitor(*r);\n    }\n");
        } else {
            let idx = c.static_monitor.expect("validated static monitor index");
            out.push_str(&format!(
                "    let __cm = crate::statics::class_monitor({idx}u32);\n"
            ));
            out.push_str(&format!(
                "    if let ::core::result::Result::Err(t) = {RT}::monitor::monitor_enter(&__cm) {{ return ::core::result::Result::Err(t); }}\n"
            ));
            out.push_str("    __frame.record_monitor(__cm);\n");
        }
    }
}

/// The operand-stack variable declarations + the pc/thrown/fault_pc
/// decls (verbatim from `emit_body`, round-5 reshaping): one Value cell
/// or one typed var per epoch.
pub(crate) fn emit_typed_stack(p: &Plan, out: &mut String) {
    for (n, var) in p.stack.iter().enumerate() {
        match var {
            Var::Dead => {}
            Var::Cell => {
                let _ = writeln!(out, "    let mut s{n}: {RT}::Value = {RT}::Value::Void;");
            }
            Var::Typed(epochs) => {
                for (e, (_, prim)) in epochs.iter().enumerate() {
                    let _ = writeln!(
                        out,
                        "    let mut {name}: {ty} = {default};",
                        name = p.stack_name(n, e),
                        ty = prim.rust_ty(),
                        default = prim.default_lit(),
                    );
                }
            }
        }
    }
    let _ = writeln!(out, "    let mut pc: u32 = 0u32;");
    let _ = writeln!(
        out,
        "    let mut thrown: ::core::option::Option<{RT}::Throw> = ::core::option::Option::None;"
    );
    let _ = writeln!(out, "    let mut fault_pc: u32 = 0u32;");
}

pub(crate) fn emit_body(mut ctx: Ctx<'_>) -> String {
    let m = ctx.m;
    let c = ctx.c;
    let p = ctx.p;
    let mut out = String::new();

    emit_typed_header(m, p, &mut out);
    let shapes_max_locals = ctx.shapes.max_locals;
    emit_typed_locals(m, p, shapes_max_locals, &mut out);
    emit_typed_sync(m, c, &mut out);
    emit_typed_stack(p, &mut out);
    let _ = writeln!(out, "    let mut pc: u32 = 0u32;");
    let _ = writeln!(
        &mut out,
        "    let mut thrown: ::core::option::Option<{RT}::Throw> = ::core::option::Option::None;"
    );
    let _ = writeln!(out, "    let mut fault_pc: u32 = 0u32;");

    let has_table = !m.exception_table.is_empty();
    if has_table {
        // byte-identical to the default body's handler (shared fn)
        crate::emit::emit_default_handlers(m, &mut out);
    }

    let _ = writeln!(out, "    'run: loop {{");
    if has_table {
        let _ = writeln!(
            out,
            "        if let ::core::option::Option::Some(t) = thrown.take() {{"
        );
        let _ = writeln!(
            out,
            "            match __j2r_find_handler(&t.java_class(), fault_pc) {{"
        );
        let _ = writeln!(out, "                ::core::option::Option::Some(h) => {{");
        // The default body's `stack.clear(); stack.push(exception);` — the
        // typed model has no depth to clear; the handler arm overwrites
        // position 0 (a Value cell by law: the exception is a Ref).
        let _ = writeln!(
            out,
            "                    s0 = {RT}::Value::Ref(::core::option::Option::Some(t.obj));"
        );
        let _ = writeln!(out, "                    pc = h;");
        let _ = writeln!(out, "                }}");
        let _ = writeln!(
            out,
            "                ::core::option::Option::None => return ::core::result::Result::Err(t),"
        );
        let _ = writeln!(out, "            }}");
        let _ = writeln!(out, "        }}");
    } else {
        let _ = writeln!(
            out,
            "        if let ::core::option::Option::Some(t) = thrown.take() {{"
        );
        let _ = writeln!(out, "            return ::core::result::Result::Err(t);");
        let _ = writeln!(out, "        }}");
    }

    // Dispatch over leaders — same walk as the default body's emit_block,
    // with the typed per-instruction emitter.
    let leaders = compute_leaders(m);
    let mut sorted: Vec<u32> = leaders.iter().copied().collect();
    sorted.sort_unstable();
    let _ = writeln!(out, "        match pc {{");
    for (i, &leader) in sorted.iter().enumerate() {
        let next_leader = sorted.get(i + 1).copied();
        let block = emit_block_typed(&mut ctx, leader, next_leader);
        let _ = writeln!(out, "            {leader}u32 => {{");
        for line in block {
            let _ = writeln!(out, "                {line}");
        }
        let _ = writeln!(out, "            }}");
    }
    let _ = writeln!(
        out,
        "            _ => ::core::unreachable!(\"j2r internal: pc out of range\"),"
    );
    let _ = writeln!(out, "        }}");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "}}");
    let _ = writeln!(
        out,
        "// j2r: operand/register unification — {} symbolic stack pushes elided",
        ctx.sym_pushes
    );
    out
}

pub(crate) fn emit_block_typed(
    ctx: &mut Ctx<'_>,
    leader: u32,
    next_leader: Option<u32>,
) -> Vec<String> {
    let m = ctx.m;
    let start = match m.instrs.iter().position(|(pc, _)| *pc == leader) {
        Some(s) => s,
        None => {
            return match next_leader {
                Some(n) => vec![format!("pc = {n}u32;")],
                None => vec!["::core::panic!(\"j2r internal: pc out of range\")".to_string()],
            }
        }
    };
    // Each block starts from the slot-variable model: values live across
    // the block boundary were materialized by the previous block's
    // handoff (or the throw dispatch); the unification layer carries
    // only intra-block defs.
    ctx.sym.clear();
    let mut lines = Vec::new();
    let mut terminated = false;
    for (pc, instr) in &m.instrs[start..] {
        if next_leader == Some(*pc) && *pc != leader {
            break;
        }
        emit_instr_typed(ctx, *pc, instr);
        if is_terminator(instr) {
            terminated = true;
            break;
        }
    }
    if !terminated {
        // The block handoff: pending entries land in their slot variables
        // before the pc transfer, so the successor block reads the same
        // storage the old emitter maintained at its push sites.
        ctx.materialize_all();
    }
    // Swap FIRST: the instruction lines live in ctx.out. Pushing the
    // fall-through before the swap would strand `pc = n` in ctx.out — it
    // leaks into the NEXT block's head and THIS block loses its pc
    // update (pc never advances: an infinite dispatch loop, the
    // LoopCounter hang).
    std::mem::swap(&mut lines, &mut ctx.out);
    if !terminated {
        match next_leader {
            Some(n) => lines.push(format!("pc = {n}u32;")),
            None => lines.push(
                "::core::panic!(\"j2r internal: method fell off the end without a return\")"
                    .to_string(),
            ),
        }
    }
    lines
}
