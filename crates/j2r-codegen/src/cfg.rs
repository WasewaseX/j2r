//! CFG reconstruction prototype — Wave 4 of M2 (`docs/M2_PLAN.md` §6).
//!
//! **Investigation only, behind `--emit-cfg` (default OFF).** The approved
//! M1 architecture — `match pc` dispatch over basic blocks — remains the
//! default emission. This module produces a *structured* body for a
//! conservative subset of methods and reports [`Unsupported`] for
//! everything else, in which case the emitter silently uses the pc-dispatch
//! body. Supported subset:
//!
//! * no exception table (handler edges are not lifted);
//! * no `tableswitch`/`lookupswitch` (switch lowering not lifted);
//! * reducible control flow — every retreating edge targets a block that
//!   dominates its source (checked, not assumed);
//! * natural loops whose OPEN exits (targets that do not themselves
//!   terminate — early `return`/`throw` blocks are emitted inline at the
//!   edge and need no post-loop region) share ONE target block: javac's
//!   `break`/`continue` inside `while`/`do-while` loops always produce
//!   extra edges to the SAME after-loop block, and an inner loop's exit
//!   whose block is the enclosing loop's continue-target (the shared-exit
//!   shape) routes through the enclosing join label — the multi-TARGET
//!   case (Java labeled `break` to two different labels) still refuses;
//! * if/else diamonds whose join block has exactly the two arm-end
//!   predecessors (an arm walk that reaches a fresh loop header walks
//!   THROUGH it to the loop's open exit, so the sibling shape
//!   `if (c) { inner-loop } continue-to-its-exit` becomes a normal meet);
//! * no branch into the middle of already-emitted code.
//!
//! Semantics are preserved by construction: the function preamble
//! (FrameGuard, locals, unmarshal, operand stack, `thrown`, `'run` loop) is
//! byte-identical to the dispatch emission; non-terminator instructions are
//! emitted by the same machinery; and `j2r_try!`'s `continue` reaches the
//! loop top where `thrown.take()` finds no handler and returns `Err` —
//! exactly the dispatch body's handler-free behaviour.

use crate::ir::Instr;
use crate::MethodPlan;
use std::collections::HashSet;

/// Rust condition code for a conditional instruction (pops its operands).
fn cond_code(instr: &Instr, _pc: &u32) -> Vec<String> {
    const RT: &str = "::j2r_runtime";
    match instr {
        Instr::IfZero(op, _) => vec![
            "let __a = ::j2r_runtime::pop_int(&mut stack);".into(),
            format!("let __cond = __a {} 0;", op.rust_op()),
        ],
        Instr::IfICmp(op, _) => vec![
            "let __b = ::j2r_runtime::pop_int(&mut stack);".into(),
            "let __a = ::j2r_runtime::pop_int(&mut stack);".into(),
            format!("let __cond = __a {} __b;", op.rust_op()),
        ],
        Instr::IfRefCmp { eq, .. } => vec![
            format!("let __b = {RT}::stack::stack_pop(&mut stack);"),
            format!("let __a = {RT}::stack::stack_pop(&mut stack);"),
            if *eq {
                "let __cond = ::j2r_runtime::ref_eq(&__a, &__b);".into()
            } else {
                "let __cond = !::j2r_runtime::ref_eq(&__a, &__b);".into()
            },
        ],
        Instr::IfNull(_) => vec![
            format!("let __a = {RT}::stack::stack_pop(&mut stack);"),
            "let __cond = __a.is_null();".into(),
        ],
        Instr::IfNonNull(_) => vec![
            format!("let __a = {RT}::stack::stack_pop(&mut stack);"),
            "let __cond = !__a.is_null();".into(),
        ],
        other => unreachable!("cond_code on {other:?}"),
    }
}

/// Attempts to lift `m`'s body into structured Rust.
///
/// `emit_fn(idx, out)` emits instruction `idx` exactly as the dispatch
/// emitter would (branch/switch instructions are never passed — the
/// builder handles them).
pub fn lift(
    m: &MethodPlan,
    emit_fn: impl FnMut(usize, &mut Vec<String>),
) -> Result<String, Unsupported> {
    let cfg = Cfg::build(m);
    if std::env::var("J2R_CFG_DEBUG").as_deref() == Ok("2") {
        // Block-table dump for join-shape diagnosis: id, pc range,
        // successors (as instruction indices), terminator class.
        eprintln!(
            "cfg-debug: block table for {}{} ({} blocks, entry {})",
            m.name,
            m.desc,
            cfg.blocks.len(),
            cfg.entry
        );
        for (i, b) in cfg.blocks.iter().enumerate() {
            let term =
                b.end
                    .checked_sub(1)
                    .and_then(|j| m.instrs.get(j))
                    .map(|(_, ins)| match ins {
                        Instr::Goto(_) => "goto".to_string(),
                        Instr::IfZero(_, _)
                        | Instr::IfICmp(_, _)
                        | Instr::IfNull(_)
                        | Instr::IfNonNull(_)
                        | Instr::IfRefCmp { .. } => "if".to_string(),
                        Instr::Return(_) => "ret".to_string(),
                        Instr::AThrow => "athrow".to_string(),
                        _ => "fall".to_string(),
                    });
            eprintln!(
                "cfg-debug:   block {i}: pc[{},{}] term={term:?} succs={:?}",
                m.instrs[b.start].0,
                m.instrs[b.end - 1].0,
                b.succs
            );
        }
    }
    let (loops, tramp) = analyse(&cfg, m)?;
    if std::env::var("J2R_CFG_DEBUG").as_deref() == Ok("2") {
        for l in &loops {
            eprintln!(
                "cfg-debug: loop header={} exit={:?} nodes={:?}",
                l.header,
                l.exit,
                {
                    let mut v: Vec<usize> = l.nodes.iter().copied().collect();
                    v.sort_unstable();
                    v
                }
            );
        }
    }
    let mut b = Builder::new(m, &cfg, loops, tramp.clone(), Box::new(emit_fn));
    let entry = cfg.entry;
    b.region(entry)?;
    // The G3.1 remainder drain: deferred joins registered outside any
    // loop body (depth 0 — method-level walks and inner-loop exit
    // regions) close their labels and emit their meet bodies here, at
    // the end of lift. Anything deeper remaining here would be a scope
    // bug; drain it anyway rather than drop it silently.
    while let Some(d) = b.deferred.pop() {
        b.out.push("}".to_string());
        b.stops.retain(|e| e.sid != d.sid);
        b.region(d.meet)?;
    }
    // Every block must be emitted — EXCEPT goto trampolines whose edges
    // were resolved to the chain's end (their single `goto` instruction is
    // structural and is never passed to the emitter, so skipping the block
    // loses no code).
    if cfg
        .blocks
        .iter()
        .enumerate()
        .any(|(i, _)| !tramp.contains_key(&i) && !b.emitted.contains(&i))
    {
        if cfg_debug() {
            let missing: Vec<usize> = cfg
                .blocks
                .iter()
                .enumerate()
                .filter(|(i, _)| !tramp.contains_key(i) && !b.emitted.contains(i))
                .map(|(i, _)| i)
                .collect();
            eprintln!(
                "cfg-debug: refuse[lift:unemitted] {}{} missing={:?}",
                m.name, m.desc, missing
            );
            if std::env::var("J2R_CFG_DEBUG").as_deref() == Ok("3") {
                eprintln!("cfg-debug: ---- emitted text at refuse[lift:unemitted] ----");
                for (i, line) in b.out.iter().enumerate() {
                    eprintln!("cfg-debug: {:>4}: {}", i, line);
                }
            }
        }
        return Err(Unsupported::BranchIntoEmittedCode);
    }
    // Structural safety net: a loop whose body is exactly a self-continue
    // can never terminate. That degenerate emission is the signature of a
    // body walk hijacked by a stale stop entry (found by cfg_equiv on
    // BigRpg.mergeSort when the lift-closure wave first unlocked these
    // shapes) — refuse the method rather than emit a hang.
    let out = b.out.join("\n");
    let mut lines = out.lines().peekable();
    while let Some(line) = lines.next() {
        if let Some(lbl) = line.strip_suffix(": loop {") {
            if let Some(next) = lines.peek() {
                if next.trim() == format!("continue {lbl};") {
                    return Err(Unsupported::BranchIntoEmittedCode);
                }
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- report

/// Why a method is (or is not) lifted — one row of the Wave-4 lift report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodCfg {
    pub class: String,
    pub method: String,
    /// True when `--emit-cfg` would emit a structured body for this method.
    pub lifted: bool,
    /// `None` when lifted; otherwise the exact unsupported-shape reason.
    pub reason: Option<Unsupported>,
}

/// Per-method lift decisions across a whole compiled program (the Wave-4
/// analogue of `specialize::plan_program`). The decision is the real one —
/// the full `lift` pipeline runs with a no-op instruction emitter — so the
/// report can never claim more than emission would deliver.
pub fn plan_program(prog: &crate::Program) -> Vec<MethodCfg> {
    let mut out = Vec::new();
    for c in &prog.classes {
        for m in &c.methods {
            let reason = lift(m, |_, _| {}).err();
            out.push(MethodCfg {
                class: c.name.clone(),
                method: m.name.clone(),
                lifted: reason.is_none(),
                reason,
            });
        }
    }
    out
}

/// Renders the lift-report summary table (per-method rows are appended by
/// the CLI, mirroring `specialize::render_report`).
pub fn render_report(rows: &[MethodCfg]) -> String {
    use std::fmt::Write as _;
    let methods = rows.len();
    let lifted = rows.iter().filter(|r| r.lifted).count();
    let mut reasons: Vec<(String, usize)> = Vec::new();
    for r in rows {
        if let Some(reason) = r.reason {
            let name = format!("{reason:?}");
            match reasons.iter_mut().find(|(n, _)| *n == name) {
                Some((_, c)) => *c += 1,
                None => reasons.push((name, 1)),
            }
        }
    }
    reasons.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut s = String::new();
    let _ = writeln!(s, "| methods | lifted | fallback |");
    let _ = writeln!(s, "| --- | --- | --- |");
    let _ = writeln!(s, "| {methods} | {lifted} | {} |", methods - lifted);
    let _ = writeln!(s);
    let _ = writeln!(s, "| fallback reason | methods |");
    let _ = writeln!(s, "| --- | --- |");
    for (name, count) in &reasons {
        let _ = writeln!(s, "| {name} | {count} |");
    }
    s
}

/// The deep-scan result type (the divergent arm's walk sets and
/// candidate joins, computed once and consumed by the attempt).
pub(crate) struct DeepScan {
    d1: HashSet<usize>,
    d2: HashSet<usize>,
    deep: HashSet<usize>,
    cands: Vec<usize>,
    inter: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MethodPlan;
    use j2r_runtime::TypeDesc;

    fn method(instrs: Vec<(u32, Instr)>) -> MethodPlan {
        MethodPlan {
            name: "t".into(),
            desc: "()V".into(),
            fn_name: "t".into(),
            is_static: true,
            is_native: false,
            is_sync: false,
            modifiers: 0,
            signature: None,
            lvtt: Vec::new(),
            array_ops: Default::default(),
            instrs,
            exception_table: vec![],
            max_locals: 2,
            max_stack: 4,
        }
    }

    #[test]
    fn straight_line_lifts() {
        let m = method(vec![
            (0, Instr::Nop),
            (1, Instr::Nop),
            (2, Instr::Return(TypeDesc::Void)),
        ]);
        let out = lift(&m, |_, _| {}).unwrap();
        assert!(!out.contains("__cond"));
    }

    #[test]
    fn simple_if_diamond_lifts() {
        // 0: IfZero(Eq, 8); 4: Return; 8: Return — both arms terminate.
        let m = method(vec![
            (0, Instr::IfZero(crate::ir::CmpOp::Eq, 8)),
            (4, Instr::Return(TypeDesc::Void)),
            (8, Instr::Return(TypeDesc::Void)),
        ]);
        let out = lift(&m, |_, _| {}).unwrap();
        assert!(out.contains("if __cond {"), "{out}");
        assert!(out.contains("} else {"), "{out}");
    }

    #[test]
    fn simple_while_loop_lifts() {
        // 0: IfZero(Ne, 16)   (loop guard → exit at 16)
        // 4: IfZero(Eq, 0)    (back edge → continue)
        // 8: Goto 0
        // 16: Return
        // Natural loop {0,4,8}, single exit → 16. The exit block TERMINATES
        // (return), so the lift-closure wave emits it INLINE in the guard's
        // true arm — the loop has no open exit and no `break` stop anymore.
        let m = method(vec![
            (0, Instr::IfZero(crate::ir::CmpOp::Ne, 16)),
            (4, Instr::IfZero(crate::ir::CmpOp::Eq, 0)),
            (8, Instr::Goto(0)),
            (16, Instr::Return(TypeDesc::Void)),
        ]);
        let out = lift(&m, |_, _| {}).unwrap();
        assert!(out.contains(": loop {"), "{out}");
        assert!(out.contains("continue 'loop_"), "{out}");
    }

    #[test]
    fn javac_while_loop_with_non_entry_header_lifts() {
        // The shape javac actually emits for `while (c) { body }` — the
        // loop header is a block with an entry-side predecessor AND the
        // back edge, i.e. NOT the entry block. Under the old idom (seed
        // = idom(pred)) the body's dominator chain skipped the header
        // and this lifted shape was mis-flagged Irreducible.
        // 0:  Goto 4            (entry → header)
        // 4:  IfZero(Ge, 16)    (header: guard → exit 16)
        // 8:  Goto 4            (body: back edge to the header)
        // 16: Return
        let m = method(vec![
            (0, Instr::Goto(4)),
            (4, Instr::IfZero(crate::ir::CmpOp::Ge, 16)),
            (8, Instr::Goto(4)),
            (16, Instr::Return(TypeDesc::Void)),
        ]);
        let out = lift(&m, |_, _| {}).unwrap();
        assert!(out.contains(": loop {"), "{out}");
    }

    #[test]
    fn loop_header_after_body_lifts() {
        // The goto-cond layout (old javac / do-while family): the guard's
        // false arm jumps BACK into a body block that sits BEFORE the
        // header in index order. The idom fix surfaced this shape (it
        // used to be swallowed by the false Irreducible before the
        // builder ever saw it). Header Bk2 = pc12; body = pc4-8 and the
        // pc16 jump back into it; single exit at pc20.
        //  0: Goto 12       (entry → cond)
        //  4: Nop           (body)
        //  8: Goto 12       (body end → cond)
        // 12: IfZero(Ge,20) (cond: exit 20 / fallthrough 16)
        // 16: Goto 4        (back edge into the body)
        // 20: Return
        let m = method(vec![
            (0, Instr::Goto(12)),
            (4, Instr::Nop),
            (8, Instr::Goto(12)),
            (12, Instr::IfZero(crate::ir::CmpOp::Ge, 20)),
            (16, Instr::Goto(4)),
            (20, Instr::Return(TypeDesc::Void)),
        ]);
        let out = lift(&m, |_, _| {}).unwrap();
        assert!(out.contains(": loop {"), "{out}");
        assert!(out.contains("continue 'loop_"), "{out}");
    }

    #[test]
    fn early_return_in_loop_lifts() {
        // The isFullRank shape (the MultiExitLoop the readiness census
        // actually measured): an early `return` block inside the loop is
        // EXCLUDED from the natural-loop node set (the node walk follows
        // predecessors from the back edge), so its incoming edge looked
        // like a second exit with a different target. The lift-closure
        // wave classifies terminator targets (return/athrow) as non-exits
        // — the builder emits them inline at the edge and that path ends.
        //  0: Nop                  (init)
        //  1: IfICmp(Ge, 11)       (guard → exit 11)
        //  2: IfZero(Ne, 6)        (body: skip the early return?)
        //  3: Return(Int)          (early return block — terminator target)
        //  6: Nop; 7: Goto(1)      (increment + back edge)
        // 11: Nop; 12: Return(Int) (after loop)
        let m = method(vec![
            (0, Instr::Nop),
            (1, Instr::IfICmp(crate::ir::CmpOp::Ge, 11)),
            (2, Instr::IfZero(crate::ir::CmpOp::Ne, 6)),
            (3, Instr::Return(TypeDesc::Int)),
            (6, Instr::Nop),
            (7, Instr::Goto(1)),
            (11, Instr::Nop),
            (12, Instr::Return(TypeDesc::Int)),
        ]);
        let out = lift(&m, |_, _| {}).unwrap();
        assert!(out.contains(": loop {"), "{out}");
        // the whole method lifted — no dispatch fallback
        assert!(!out.contains("match pc"), "{out}");
    }

    #[test]
    fn shared_exit_continue_target_lifts() {
        // The JaggedNew shape (the ImpureLoopExit the readiness census
        // actually measured): an inner loop whose exit block is the
        // ENCLOSING loop's continue-target. The inner loop's exit block
        // has an outside predecessor (the outer `continue` edge) — the
        // old purity check refused; the lift-closure wave routes the
        // shared exit through the enclosing join label and walks the
        // arm-walk THROUGH the fresh inner header to its exit.
        //  0: Goto(2)           entry → outer header
        //  2: IfICmp(Ge, 30)    outer guard → exit 30 (return — terminator)
        //  5: IfNonNull(9)      if (row != null) → inner setup 9, else → 7
        //  7: Goto(24)          outer `continue` → the shared increment block
        //  9: IfICmp(Ge, 24)    inner guard → exit 24 (the SHARED block)
        // 12: Nop; 13: Goto(9)  inner body + back edge
        // 24: Nop; 25: Goto(2)  shared increment + outer back edge
        // 30: Return
        let m = method(vec![
            (0, Instr::Goto(2)),
            (2, Instr::IfICmp(crate::ir::CmpOp::Ge, 30)),
            (5, Instr::IfNonNull(9)),
            (7, Instr::Goto(24)),
            (9, Instr::IfICmp(crate::ir::CmpOp::Ge, 24)),
            (12, Instr::Nop),
            (13, Instr::Goto(9)),
            (24, Instr::Nop),
            (25, Instr::Goto(2)),
            (30, Instr::Return(TypeDesc::Void)),
        ]);
        let out = lift(&m, |_, _| {}).unwrap();
        assert_eq!(out.matches(": loop {").count(), 2, "{out}");
        assert!(out.contains("break 'join_"), "{out}");
    }

    #[test]
    fn shared_exit_emits_every_block_exactly_once() {
        // The once-only emission invariant over the shared-exit shape:
        // every ordinary (non-branch) instruction reaches the emitter
        // exactly once even though the exit block is reachable from both
        // the inner loop's exit edge and the outer continue edge.
        let m = method(vec![
            (0, Instr::Goto(2)),
            (2, Instr::IfICmp(crate::ir::CmpOp::Ge, 30)),
            (5, Instr::IfNonNull(9)),
            (7, Instr::Goto(24)),
            (9, Instr::IfICmp(crate::ir::CmpOp::Ge, 24)),
            (12, Instr::Nop),
            (13, Instr::Goto(9)),
            (24, Instr::Nop),
            (25, Instr::Goto(2)),
            (30, Instr::Return(TypeDesc::Void)),
        ]);
        use std::cell::RefCell;
        let seen: RefCell<Vec<usize>> = RefCell::new(Vec::new());
        let out = {
            let seen = &seen;
            lift(&m, |idx, _| seen.borrow_mut().push(idx)).unwrap()
        };
        assert!(out.contains(": loop {"), "{out}");
        let seen = seen.borrow();
        for idx in 0..m.instrs.len() {
            let structural = matches!(
                &m.instrs[idx].1,
                Instr::IfZero(..)
                    | Instr::IfICmp(..)
                    | Instr::IfNull(..)
                    | Instr::IfNonNull(..)
                    | Instr::IfRefCmp { .. }
                    | Instr::Goto(_)
            );
            let n = seen.iter().filter(|&&s| s == idx).count();
            if structural {
                assert_eq!(n, 0, "structural instr {idx} passed to emitter");
            } else {
                assert_eq!(n, 1, "instr {idx} emitted {n} times (want 1)");
            }
        }
    }

    #[test]
    fn back_edge_uses_loop_label_not_stale_join_label() {
        // cfg_equiv caught this on BigJson$Parser: a diamond whose meet is
        // a FRESH loop header pushes (header, join-label) into the stops;
        // the loop is then emitted at the meet, and the body's back edge
        // to the header must `continue` the LOOP's own label — the old
        // find-first stop lookup emitted `continue 'join_...`, which is
        // illegal Rust (labeled blocks cannot be `continue`d). The
        // active-loop check now precedes the stop lookup in both
        // stop_transfer and jump_to_block.
        // 0: IfZero(Eq, 8)      diamond: true arm → the loop header
        // 4: Return             false arm terminates
        // 8: IfICmp(Ge, 20)     loop guard → exit 20 (a return — the loop
        //                       has NO open exit, so walk_meet returns the
        //                       header itself as the meet)
        // 12: Goto(8)           back edge
        // 16: Nop               (unreached padding keeps pcs distinct)
        // 20: Return
        let m = method(vec![
            (0, Instr::IfZero(crate::ir::CmpOp::Eq, 8)),
            (4, Instr::Return(TypeDesc::Void)),
            (8, Instr::IfICmp(crate::ir::CmpOp::Ge, 20)),
            (12, Instr::Goto(8)),
            (20, Instr::Return(TypeDesc::Void)),
        ]);
        let out = lift(&m, |_, _| {}).unwrap();
        assert!(out.contains("continue 'loop_"), "{out}");
        assert!(!out.contains("continue 'join_"), "{out}");
    }

    #[test]
    fn sequential_loops_lift() {
        // Two loop headers, both non-entry, sharing one trailing exit —
        // the second header's dominator chain must survive the first.
        let m = method(vec![
            (0, Instr::Goto(4)),
            (4, Instr::IfZero(crate::ir::CmpOp::Ge, 16)),
            (8, Instr::Goto(4)),
            (16, Instr::Goto(20)),
            (20, Instr::IfZero(crate::ir::CmpOp::Ge, 32)),
            (24, Instr::Goto(20)),
            (32, Instr::Return(TypeDesc::Void)),
        ]);
        let out = lift(&m, |_, _| {}).unwrap();
        assert_eq!(out.matches(": loop {").count(), 2, "{out}");
    }

    #[test]
    fn irreducible_falls_back() {
        // GENUINE irreducibility: a two-entry cycle. B0 branches into
        // B1 and B2, and B1 ⇄ B2 form a cycle entered from BOTH sides —
        // no single header dominates the cycle, so no natural loop
        // exists. (The old test used a single-entry cycle that is
        // actually reducible and only "failed" because of the idom bug.)
        let m = method(vec![
            (0, Instr::IfZero(crate::ir::CmpOp::Eq, 8)),
            (4, Instr::Goto(8)),
            (8, Instr::Goto(4)),
            (12, Instr::Return(TypeDesc::Void)),
        ]);
        assert_eq!(lift(&m, |_, _| {}), Err(Unsupported::Irreducible));
    }

    #[test]
    fn exception_table_falls_back() {
        let mut m = method(vec![(0, Instr::Return(TypeDesc::Void))]);
        m.exception_table = vec![(0, 1, 0, None)];
        assert_eq!(lift(&m, |_, _| {}), Err(Unsupported::HasExceptionTable));
    }

    #[test]
    fn switch_falls_back() {
        let m = method(vec![(
            0,
            Instr::TableSwitch {
                default: 0,
                low: 0,
                targets: vec![0],
            },
        )]);
        assert_eq!(lift(&m, |_, _| {}), Err(Unsupported::HasSwitch));
    }

    #[test]
    fn join_with_third_predecessor_lifts_via_owner_stop() {
        // The 2026-11-08 meet-already-stopped law: a nested diamond whose
        // meet is an ENCLOSING diamond's join does not re-join it — its
        // arms break with the owner's label and the owner emits the meet
        // exactly once. This shape previously refused ImpureJoin (the
        // nested join_via_label saw the outer root as a third edge); the
        // free-walk arms make it liftable with identical semantics:
        // c0 -> 12 -> 20; !c0 -> (c4 ? 20 : 8 -> 20).
        let m = method(vec![
            (0, Instr::IfZero(crate::ir::CmpOp::Eq, 12)),
            (4, Instr::IfZero(crate::ir::CmpOp::Eq, 20)),
            (8, Instr::Goto(20)),
            (12, Instr::Nop),
            (13, Instr::Goto(20)),
            (20, Instr::Return(TypeDesc::Void)),
        ]);
        let r = lift(&m, |_, _| {});
        let out = r.expect("the owner-stop nested join now lifts");
        assert!(
            out.contains("break 'join_"),
            "arms must transfer to the owner's join label, got:\n{out}"
        );
    }

    // NOTE on divergent-join refusals: a straight-line (loop-free) method
    // cannot produce a genuinely divergent both-Some-meets diamond — its
    // arm walks converge through single-predecessor chains to the same
    // final join, which is the shared-meet case above. Real divergence
    // needs loop contexts (transfer arms), covered by the free-walk law
    // in diamond(); unplaceable joins surface loudly via
    // BranchIntoEmittedCode at the end-of-lift all-blocks check.

    #[test]
    fn cfg_dominators_on_diamond() {
        let m = method(vec![
            (0, Instr::IfZero(crate::ir::CmpOp::Eq, 8)),
            (4, Instr::Nop),
            (5, Instr::Goto(12)),
            (8, Instr::Nop),
            (9, Instr::Goto(12)),
            (12, Instr::Return(TypeDesc::Void)),
        ]);
        let cfg = Cfg::build(&m);
        assert_eq!(cfg.blocks.len(), 4);
        let idoms = cfg.idom();
        let ret = cfg.block_of(5);
        assert_eq!(idoms[ret], Some(cfg.entry));
    }

    #[test]
    fn walk_meet_maps_instruction_indices_to_blocks() {
        // Regression for the index-type panic: walk_meet's nested-diamond
        // recursion passed the *instruction* index (last + 1) where a
        // *block* index was required — this method (9 instructions, 6
        // blocks) panicked with "index out of bounds" before the fix. The
        // shape (a nested diamond whose arms converge at block 4, and the
        // OUTER diamond's true arm branches STRAIGHT INTO the ret block 5
        // — a join of the arm edge and block 4's fall-through) was
        // refused conservatively (Err(ImpureJoin)) before the G3.1 deep
        // cross-meet attempt; the attempt recognizes the ARM-START meet
        // (block 5 == t_b is the convergence point, preds = the root's
        // branch edge + block 4's fall) and lowers the empty-arm join —
        // both joins label-scoped, every edge a break. The lift must
        // SUCCEED (the index mapping works: no panic, the blocks wire up)
        // and carry both join labels.
        let m = method(vec![
            (0, Instr::IfZero(crate::ir::CmpOp::Eq, 28)),
            (4, Instr::IfZero(crate::ir::CmpOp::Eq, 20)),
            (8, Instr::Nop),
            (12, Instr::Nop),
            (16, Instr::Goto(24)),
            (20, Instr::Goto(24)),
            (24, Instr::Nop),
            (25, Instr::Nop),
            (28, Instr::Return(TypeDesc::Void)),
        ]);
        let r = lift(&m, |_, _| {});
        let out = r.expect("the empty-arm nested join now lifts");
        assert!(
            out.contains("'join_5_1: {") && out.contains("'join_4_2: {"),
            "both joins must be label-scoped, got:\n{out}"
        );
    }

    #[test]
    fn empty_arm_diamond_lifts() {
        // The branch target IS the join (javac's shape for
        // `if (c) { } else { ... }`): the diamond's own root is a
        // legitimate predecessor of the meet. 11 instructions in 3 blocks.
        // The counting recorder also pins the dropped-last-instruction
        // regression: a block ending at a leader boundary had its final
        // instruction skipped by the region walker (the `iconst_0` arm of
        // a boolean branch vanished, corrupting the operand stack — caught
        // by the output-equivalence test on StringOps). Every ordinary
        // instruction must reach the emitter exactly once; branch
        // instructions never do (the builder handles them structurally).
        let m = method(vec![
            (0, Instr::IfZero(crate::ir::CmpOp::Ne, 40)),
            (4, Instr::Nop),
            (8, Instr::Nop),
            (12, Instr::Nop),
            (16, Instr::Nop),
            (20, Instr::Nop),
            (24, Instr::Nop),
            (28, Instr::Nop),
            (32, Instr::Nop),
            (36, Instr::Nop),
            (40, Instr::Return(TypeDesc::Void)),
        ]);
        use std::cell::RefCell;
        let seen: RefCell<Vec<usize>> = RefCell::new(Vec::new());
        let out = {
            let seen = &seen;
            lift(&m, |idx, _| seen.borrow_mut().push(idx)).unwrap()
        };
        assert!(out.contains("if __cond {"), "{out}");
        let seen = seen.borrow();
        for idx in 0..m.instrs.len() {
            let structural = matches!(&m.instrs[idx].1, Instr::IfZero(..));
            let n = seen.iter().filter(|&&s| s == idx).count();
            if structural {
                assert_eq!(n, 0, "structural instr {idx} passed to emitter");
            } else {
                assert_eq!(n, 1, "instr {idx} emitted {n} times (want 1)");
            }
        }
    }
}

// --- round-7 file-split pass: verbatim children below ---
mod graph;
pub use graph::Unsupported;
pub(crate) use graph::*;
mod loops;
pub(crate) use loops::*;
mod builder;
pub(crate) use builder::*;
mod builder2;
