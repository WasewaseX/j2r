//! Typed-register readiness analysis — the registered follow-up to the
//! M2 3a shape report (docs/M2_SHAPE_REPORT.md, "Recommendation for a
//! future milestone") and the measurement the typed-operand GO/NO-GO is
//! decided on (docs/BENCHMARKS.md, "First steady-state comparison": the
//! registered performance lever is the typed-register / typed-operand
//! emission model, with CFG structuring as prerequisite rather than
//! payload).
//!
//! The M2 shape pass was deliberately conservative: a stack position or
//! local slot had to keep ONE kind across the WHOLE method (all-or-
//! nothing) to enter a typed pool. Its own report identified the fix —
//! track kinds per LIVE RANGE, not per method — and measured that 72 of
//! 161 fixture methods fell back with `KindVaries`, dominated by
//! position reuse across disjoint live ranges. This module implements
//! that refinement on the verifier's exported per-pc shapes plus the
//! instruction stream:
//!
//! * A position (operand-stack slot or local slot) is *occupied* at a pc
//!   when the verifier's state gives it a concrete kind; kinds collapse
//!   to Int/Long/Float/Double/Ref (references, arrays, null and the
//!   defensive `Top` all collapse to Ref; `Unset`/`Poison`/`HighHalf`
//!   mean the position is dead — the verifier rejects every load of
//!   them).
//! * A maximal run of occupied pcs is one *epoch*. A LOCAL slot's epoch
//!   also ends at every store to that slot (`xstore`/`iinc`): a store
//!   redefines the slot, so the old value is dead from there on even
//!   though the verifier's merged state still shows it (the verifier
//!   itself documents javac's disjoint-range slot reuse — see
//!   `VTy::merge_local`). A STACK position's kills are the depth
//!   dropping to or below it AND — the typed wave's refinement (2026-11-07)
//!   — a pop-and-REFILL whose KIND CHANGES: an `xXaload`/arithmetic/convert/
//!   compare/field/call result lands on the slot its popped operand
//!   occupied, redefining it in place exactly like a store redefines a
//!   local (the old operand is consumed; every later read on every path
//!   reads the new value — the verifier's join consistency is what makes
//!   the linear redefinition sound). Same-kind refills (`iadd` feeding an
//!   `iadd`) are NOT boundaries — the refill overwrites the epoch's own
//!   variable after the operand reads, mirroring a same-kind local store
//!   chain. Stack shuffles (`dup_x1` …) can still change a position's
//!   kind without any kill; the analysis counts those AGAINST readiness
//!   (mixed epoch), never for it — conservative by construction.
//! * A position is EPOCH-CLEAN when every epoch holds a single kind.
//!   Kinds may differ ACROSS epochs: the verifier guarantees the
//!   position is dead between them (a load in between would have failed
//!   verification — `Poison` locals and stack underflow are both
//!   verification errors), so a typed-register emitter may re-type each
//!   epoch independently.
//!
//! A method is TYPED-READY when (a) its body can be CFG-lifted — the
//! registered prerequisite, measured by the real `cfg::lift` pipeline
//! with a no-op emitter — (b) every position is epoch-clean, and (c) at
//! least one epoch is primitive-typed (typed locals buy nothing in an
//! all-reference method, though the structured body still removes the
//! match-pc dispatch). Like `specialize` and `cfg`, this pass observes
//! and decides; it changes no emission.

use crate::ir::Instr;
use crate::verify::{MethodShapes, VTy};

/// The storage kind of one position at one pc, collapsed from the
/// verifier lattice. `None` = the position is dead at that pc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Int,
    Long,
    Float,
    Double,
    /// Any reference (classes, arrays, null, and the defensive `Top`).
    Ref,
}

impl Kind {
    pub(crate) fn of(v: &VTy) -> Option<Kind> {
        match v {
            VTy::Int => Some(Kind::Int),
            VTy::Long => Some(Kind::Long),
            VTy::Float => Some(Kind::Float),
            VTy::Double => Some(Kind::Double),
            VTy::Ref | VTy::Null | VTy::Array(_) | VTy::Top => Some(Kind::Ref),
            // Dead: never legal to load, so a live range cannot span it.
            VTy::Unset | VTy::Poison | VTy::HighHalf => None,
        }
    }

    fn is_prim(self) -> bool {
        !matches!(self, Kind::Ref)
    }
}

/// One occupied run (epoch) of a position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Epoch {
    pub kind: Kind,
    /// True when two different kinds met inside this run with no kill
    /// between them — the one shape per-epoch typing cannot serve.
    pub mixed: bool,
}

/// The instruction-stack effect the boundary rule needs, in VALUE space:
/// how many operand values the instruction consumes and what it pushes
/// back onto the stack (`None` = nothing, or a push onto a FRESH slot —
/// fresh slots need no boundary; their epoch opens on first occupancy).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PushKind {
    None,
    /// One value of this kind pushed back onto a popped slot (refill).
    Val(Kind),
    /// `swap`: the two top cat-1 slots exchange — a mutual refill.
    Swap,
}

/// Value-space stack effect of one instruction: how many operand VALUES
/// it consumes (cat-2 values are one value) and what it pushes back
/// (`None` = nothing, or a push onto a FRESH slot — fresh slots need no
/// boundary; their epoch opens on first occupancy). `swap` is special: a
/// mutual refill of the two top slots.
pub(crate) fn value_effect(instr: &Instr) -> (usize, PushKind) {
    use crate::ir::{ArithOp, Instr as I, NumKind as NK};
    let val = |k: NK| match k {
        NK::Int | NK::Byte | NK::Char | NK::Short => Kind::Int,
        NK::Long => Kind::Long,
        NK::Float => Kind::Float,
        NK::Double => Kind::Double,
    };
    let desc_kind = |desc: &str| match desc.as_bytes().last() {
        Some(b'I') | Some(b'B') | Some(b'C') | Some(b'S') | Some(b'Z') => Kind::Int,
        Some(b'J') => Kind::Long,
        Some(b'F') => Kind::Float,
        Some(b'D') => Kind::Double,
        _ => Kind::Ref,
    };
    match instr {
        I::ALoad(e) => {
            let k = match e {
                crate::ir::ElemKind::IntGroup => Kind::Int,
                crate::ir::ElemKind::Long => Kind::Long,
                crate::ir::ElemKind::Float => Kind::Float,
                crate::ir::ElemKind::Double => Kind::Double,
                _ => Kind::Ref,
            };
            (2, PushKind::Val(k))
        }
        I::Arith { op, kind } => match op {
            ArithOp::Neg => (1, PushKind::Val(val(*kind))),
            _ => (2, PushKind::Val(val(*kind))),
        },
        I::Convert(conv) => (1, PushKind::Val(val(conv.to))),
        I::Cmp(_) => (2, PushKind::Val(Kind::Int)),
        I::GetField { desc, .. } => (1, PushKind::Val(desc_kind(desc))),
        I::InvokeVirtual { desc, .. }
        | I::InvokeSpecial { desc, .. }
        | I::InvokeInterface { desc, .. }
        | I::InvokeStatic { desc, .. } => {
            let (params, ret) = j2r_runtime::parse_method_desc(desc).expect("valid descriptor");
            let recv = if matches!(instr, I::InvokeStatic { .. }) {
                0
            } else {
                1
            };
            let push = match ret {
                j2r_runtime::TypeDesc::Void => PushKind::None,
                j2r_runtime::TypeDesc::Int => PushKind::Val(Kind::Int),
                j2r_runtime::TypeDesc::Long => PushKind::Val(Kind::Long),
                j2r_runtime::TypeDesc::Float => PushKind::Val(Kind::Float),
                j2r_runtime::TypeDesc::Double => PushKind::Val(Kind::Double),
                _ => PushKind::Val(Kind::Ref),
            };
            (params.len() + recv, push)
        }
        // The report's boundary walk may still see an indy call before the
        // emitter rejects the method: an under-counted pop can only MERGE
        // epochs (mixed or same-kind), never split one — conservative.
        I::InvokeDynamic { .. } => (0, PushKind::None),
        I::ArrayLength => (1, PushKind::Val(Kind::Int)),
        I::InstanceOf { .. } => (1, PushKind::Val(Kind::Int)),
        I::CheckCast { .. } => (1, PushKind::Val(Kind::Ref)),
        I::Swap => (2, PushKind::Swap),
        I::Pop => (1, PushKind::None),
        // Pop2 in value space: two SLOTS (one cat-2 value or two cat-1s) —
        // the caller special-cases it before the slot walk.
        I::Pop2 => (2, PushKind::None),
        I::Return(t) => (
            if matches!(t, j2r_runtime::TypeDesc::Void) {
                0
            } else {
                1
            },
            PushKind::None,
        ),
        I::PutField { .. } => (2, PushKind::None),
        I::PutStatic { .. } => (1, PushKind::None),
        I::MonitorEnter | I::MonitorExit | I::AThrow => (1, PushKind::None),
        I::IfZero(..) | I::IfNull(_) | I::IfNonNull(_) => (1, PushKind::None),
        I::IfICmp(..) | I::IfRefCmp { .. } => (2, PushKind::None),
        // Array construction POPS its dimension operands and pushes the
        // array in their place — a REFILL of the first popped slot, not a
        // fresh push (the Matrix.<init> NPE: the missing Ref epoch left
        // the result write untargeted and the field stored garbage).
        I::NewArray(_) | I::ANewArray { .. } => (1, PushKind::Val(Kind::Ref)),
        I::MultiNewArray { dims, .. } => (*dims as usize, PushKind::Val(Kind::Ref)),
        // Zero-pop, fresh-slot push or no stack effect at all.
        _ => (0, PushKind::None),
    }
}

/// Slot-space stack effect of one instruction at one pc. The verifier's
/// operand stack is 1-slot-per-value (`State::push` appends one entry;
/// `check_max` counts entries) — so the slot count IS the value count,
/// and category-2 values occupy one stack slot even though a cat-2 LOCAL
/// occupies two local slots.
pub(crate) fn slot_effect(instr: &Instr, _in_stack: &[VTy]) -> (usize, PushKind) {
    let (pv, push) = value_effect(instr);
    (pv, push)
}

/// The epoch decomposition of one position (stack slot or local slot),
/// with the two stability verdicts:
///
/// * `epoch_clean` — every epoch is single-kind (the live-range model
///   can type this position, re-typing per epoch).
/// * `pool_stable` — ONE kind across ALL occupied pcs (the M2 3a pool
///   rule; reported for continuity with docs/M2_SHAPE_REPORT.md).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionReport {
    pub epochs: Vec<Epoch>,
    /// The pc each epoch OPENS at (first occupied pc of the run), parallel
    /// to `epochs` — the typed emitter's per-epoch variable map.
    pub epoch_starts: Vec<u32>,
    pub epoch_clean: bool,
    pub pool_stable: bool,
}

fn analyze_position(
    pcs: &[u32],
    occupied: impl Fn(u32) -> Option<Kind>,
    boundary: impl Fn(u32) -> bool,
) -> PositionReport {
    let mut done: Vec<Epoch> = Vec::new();
    let mut starts: Vec<u32> = Vec::new();
    // The currently-open epoch: (first kind, any kind mix seen so far).
    let mut cur: Option<(Kind, bool)> = None;
    for &pc in pcs {
        if boundary(pc) {
            // The pre-state at a store still belongs to the closing
            // epoch (that value is what the store overwrites); the next
            // occupied pc opens a fresh epoch.
            match (occupied(pc), cur.take()) {
                (Some(k), Some((k0, mixed))) => {
                    done.push(Epoch {
                        kind: k0,
                        mixed: mixed || k != k0,
                    });
                }
                (Some(k), None) => {
                    // The closing epoch existed only at this pc (the value
                    // was pushed by the previous instruction and is being
                    // consumed here) — its start is this pc.
                    starts.push(pc);
                    done.push(Epoch {
                        kind: k,
                        mixed: false,
                    });
                }
                (None, Some((k0, mixed))) => done.push(Epoch { kind: k0, mixed }),
                (None, None) => {}
            }
        } else {
            match (occupied(pc), cur.take()) {
                (Some(k), Some((k0, mixed))) => {
                    // No kill between the previous occupied pc and this
                    // one: the kind must agree or the run is mixed.
                    cur = Some((k0, mixed || k != k0));
                }
                (Some(k), None) => {
                    // First occupancy opens a fresh epoch at this pc.
                    starts.push(pc);
                    cur = Some((k, false));
                }
                (None, Some((k0, mixed))) => done.push(Epoch { kind: k0, mixed }),
                (None, None) => {}
            }
        }
    }
    if let Some((k0, mixed)) = cur {
        done.push(Epoch { kind: k0, mixed });
    }
    let epoch_clean = done.iter().all(|e| !e.mixed);
    let first = done.first().map(|e| e.kind);
    let pool_stable = !done.is_empty() && done.iter().all(|e| e.kind == first.unwrap());
    PositionReport {
        epochs: done,
        epoch_starts: starts,
        epoch_clean,
        pool_stable,
    }
}

/// Per-method readiness rollup — the row of the readiness report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Liftable + every position epoch-clean + at least one primitive
    /// epoch: the typed-register/typed-operand model applies as-is.
    TypedReady,
    /// Liftable + epoch-clean, but every epoch is reference-typed —
    /// typed locals buy nothing here (the structured body still removes
    /// the match-pc dispatch).
    LiftableAllRef,
    /// The CFG lift is the blocker (the registered prerequisite); the
    /// `cfg::Unsupported` reason names the shape.
    NeedsCfgLift,
    /// Liftable, but some position changes kind with no kill between —
    /// the one shape per-epoch typing cannot serve.
    MixedEpoch,
}

/// The readiness row for one method.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodReadiness {
    pub class: String,
    pub method: String,
    /// The real lift decision (`cfg::lift` with a no-op emitter).
    pub lifted: bool,
    pub lift_reason: Option<crate::cfg::Unsupported>,
    /// Occupancy-weighted primitive value traffic (pc × position).
    pub prim_traffic: u64,
    /// Occupancy-weighted reference value traffic (pc × position).
    pub ref_traffic: u64,
    /// Positions (stack slots + local slots) occupied somewhere.
    pub positions_total: usize,
    /// Of those, epoch-clean.
    pub positions_epoch_clean: usize,
    /// Of those, pool-stable under the M2 3a rule.
    pub positions_pool_stable: usize,
    /// Epochs across all positions, and how many are primitive-typed.
    pub epochs_total: usize,
    pub epochs_prim: usize,
    pub verdict: Verdict,
}

/// The slot write-map of a stack-shuffle instruction (`swap`, `dup`,
/// `dup_x1`, `dup_x2`, `dup2`, `dup2_x1`, `dup2_x2`): `(out_slot,
/// src_slot)` pairs — the value whose LOW slot is `src_slot` is copied to
/// `out_slot`. Derived from the runtime's own pop-then-push forms (the
/// differentially-pinned semantics in `j2r_runtime::stack`); the form is
/// decided by the categories of the in-shape values exactly as the runtime
/// decides them from the values. HighHalf slots never appear (dead by
/// law); same-slot writes are skipped by the caller. `None` for
/// non-shuffle instructions.
pub(crate) fn shuffle_slot_writes(instr: &Instr, in_stack: &[VTy]) -> Option<Vec<(usize, usize)>> {
    use Instr as I;
    // Occupied value slots, bottom → top (HighHalf skipped).
    let vs: Vec<usize> = in_stack
        .iter()
        .enumerate()
        .filter(|(_, v)| Kind::of(v).is_some())
        .map(|(i, _)| i)
        .collect();
    let k = vs.len();
    if k == 0 {
        return None;
    }
    let cat2 = |i: usize| in_stack[vs[i]].is_cat2();
    // The affected region (deepest first) and the out sequence as indices
    // into it, per the runtime's pop-then-push forms.
    let (n, out): (usize, &[usize]) = match instr {
        I::Swap => (2, &[1, 0]),
        I::Dup => (1, &[0, 0]),
        I::DupX1 => (2, &[1, 0, 1]),
        I::DupX2 => {
            if cat2(k - 2) {
                (2, &[1, 0, 1])
            } else {
                (3, &[2, 0, 1, 2])
            }
        }
        I::Dup2 => {
            if cat2(k - 1) {
                (1, &[0, 0])
            } else {
                (2, &[0, 1, 0, 1])
            }
        }
        I::Dup2X1 => {
            if cat2(k - 1) {
                (2, &[1, 0, 1])
            } else {
                (3, &[1, 2, 0, 1, 2])
            }
        }
        I::Dup2X2 => match (cat2(k - 1), cat2(k - 2)) {
            (true, true) => (2, &[1, 0, 1]),
            (true, false) => (3, &[2, 0, 1, 2]),
            (false, true) => (3, &[1, 2, 0, 1, 2]),
            (false, false) => {
                if cat2(k - 3) {
                    (3, &[1, 2, 0, 1, 2])
                } else {
                    (4, &[2, 3, 0, 1, 2, 3])
                }
            }
        },
        _ => return None,
    };
    if k < n {
        return None; // malformed — the verifier rejects it anyway
    }
    // Region value slots (deepest first). The stack is 1-slot-per-value:
    // each out value advances the cursor by exactly one slot; the cat-2
    // checks above decide the FORM only (the runtime's own rule).
    let region: Vec<usize> = vs[k - n..].to_vec();
    let mut writes = Vec::with_capacity(out.len());
    for (i, &src) in out.iter().enumerate() {
        writes.push((region[0] + i, region[src]));
    }
    Some(writes)
}

/// The epoch decomposition of every stack slot and every local slot of a
/// method — the shared walk behind `analyze` (the report) and the typed
/// emitter's plan (the emission). Stack-slot kills: the depth dropping to
/// or below the slot, plus a pop-and-refill whose KIND CHANGES (the
/// in-place redefinition law — see the module docs; shuffles included via
/// `shuffle_slot_writes`). Local-slot kills: every store to the slot
/// (`xstore`/`iinc`).
pub(crate) fn position_reports(
    instrs: &[(u32, Instr)],
    shapes: &MethodShapes,
) -> (Vec<PositionReport>, Vec<PositionReport>) {
    let mut pcs: Vec<u32> = shapes.per_pc.keys().copied().collect();
    pcs.sort_unstable();

    let instr_by_pc: std::collections::HashMap<u32, &Instr> =
        instrs.iter().map(|(pc, i)| (*pc, i)).collect();

    let max_stack_seen = shapes
        .per_pc
        .values()
        .map(|s| s.stack.len())
        .max()
        .unwrap_or(0);
    let max_locals_seen = shapes
        .per_pc
        .values()
        .map(|s| s.locals.len())
        .max()
        .unwrap_or(0);

    // Store boundaries per local slot: `xstore` and `iinc` both redefine
    // the slot. (Wide forms are decoded to the same `Instr` variants.)
    let mut store_pcs: std::collections::BTreeMap<u16, Vec<u32>> = Default::default();
    for (pc, ins) in instrs {
        match ins {
            Instr::Store(idx) => store_pcs.entry(*idx).or_default().push(*pc),
            Instr::IInc { idx, .. } => store_pcs.entry(*idx).or_default().push(*pc),
            _ => {}
        }
    }
    let no_bounds: Vec<u32> = Vec::new();

    let mut stack_reports = Vec::with_capacity(max_stack_seen);
    for i in 0..max_stack_seen {
        let occupied = |pc: u32| -> Option<Kind> {
            shapes
                .per_pc
                .get(&pc)
                .and_then(|s| s.stack.get(i))
                .and_then(Kind::of)
        };
        let slot = i;
        let boundary = |pc: u32| -> bool {
            let Some(shape) = shapes.per_pc.get(&pc) else {
                return false;
            };
            let in_len = shape.stack.len();
            if in_len <= slot {
                return false; // slot not occupied at pc — nothing to redefine
            }
            let Some(instr) = instr_by_pc.get(&pc) else {
                return false;
            };
            let (popped, push) = slot_effect(instr, &shape.stack);
            // Shuffle refills: the write-map's existing-slot entries whose
            // kind changes across the move.
            if matches!(
                instr,
                Instr::Swap
                    | Instr::Dup
                    | Instr::DupX1
                    | Instr::DupX2
                    | Instr::Dup2
                    | Instr::Dup2X1
                    | Instr::Dup2X2
            ) {
                let Some(writes) = shuffle_slot_writes(instr, &shape.stack) else {
                    return false;
                };
                return writes.iter().any(|&(out_slot, src_slot)| {
                    out_slot < in_len
                        && out_slot != src_slot
                        && Kind::of(&shape.stack[out_slot]).is_some()
                        && Kind::of(&shape.stack[out_slot]) != Kind::of(&shape.stack[src_slot])
                }) && writes.iter().any(|&(out_slot, _)| out_slot == slot);
            }
            match push {
                PushKind::None => false, // shrink: unoccupancy closes the epoch
                PushKind::Swap => false, // swap handled by the shuffle map above
                PushKind::Val(k) => {
                    // The stack is 1-slot-per-value: the result lands on
                    // the slot `popped` below the top. Boundary iff the
                    // kind changes across the refill.
                    let r = in_len.checked_sub(popped).unwrap_or(usize::MAX);
                    r == slot && Kind::of(&shape.stack[slot]) != Some(k)
                }
            }
        };
        stack_reports.push(analyze_position(&pcs, occupied, boundary));
    }

    let mut local_reports = Vec::with_capacity(max_locals_seen);
    for j in 0..max_locals_seen {
        let occupied = |pc: u32| -> Option<Kind> {
            shapes
                .per_pc
                .get(&pc)
                .and_then(|s| s.locals.get(j))
                .and_then(Kind::of)
        };
        let bounds: &[u32] = store_pcs.get(&(j as u16)).map_or(&no_bounds, |v| v);
        local_reports.push(analyze_position(&pcs, occupied, |pc| bounds.contains(&pc)));
    }

    (stack_reports, local_reports)
}

/// Decides one method from its instruction stream and verified shapes.
/// `lifted`/`lift_reason` come from `cfg::lift` (the caller runs the real
/// pipeline once); this function never re-decides it.
pub fn analyze(
    class: &str,
    method: &str,
    instrs: &[(u32, Instr)],
    shapes: &MethodShapes,
    lifted: bool,
    lift_reason: Option<crate::cfg::Unsupported>,
) -> MethodReadiness {
    let mut prim_traffic: u64 = 0;
    let mut ref_traffic: u64 = 0;
    for shape in shapes.per_pc.values() {
        for v in &shape.stack {
            match Kind::of(v) {
                Some(k) if k.is_prim() => prim_traffic += 1,
                Some(_) => ref_traffic += 1,
                None => {}
            }
        }
        for v in &shape.locals {
            match Kind::of(v) {
                Some(k) if k.is_prim() => prim_traffic += 1,
                Some(_) => ref_traffic += 1,
                None => {}
            }
        }
    }

    let (stack_reports, local_reports) = position_reports(instrs, shapes);
    let mut reports: Vec<PositionReport> = Vec::new();
    reports.extend(stack_reports);
    reports.extend(local_reports);

    // Only positions occupied at least once roll up (a slot never seen
    // with a concrete kind has nothing to type).
    let occupied_positions: Vec<&PositionReport> =
        reports.iter().filter(|r| !r.epochs.is_empty()).collect();
    let positions_total = occupied_positions.len();
    let positions_epoch_clean = occupied_positions.iter().filter(|r| r.epoch_clean).count();
    let positions_pool_stable = occupied_positions.iter().filter(|r| r.pool_stable).count();
    let epochs_total: usize = occupied_positions.iter().map(|r| r.epochs.len()).sum();
    let epochs_prim: usize = occupied_positions
        .iter()
        .map(|r| r.epochs.iter().filter(|e| e.kind.is_prim()).count())
        .sum();
    let any_mixed = occupied_positions.iter().any(|r| !r.epoch_clean);

    let verdict = if !lifted {
        Verdict::NeedsCfgLift
    } else if any_mixed {
        Verdict::MixedEpoch
    } else if epochs_prim == 0 {
        Verdict::LiftableAllRef
    } else {
        Verdict::TypedReady
    };

    MethodReadiness {
        class: class.to_string(),
        method: method.to_string(),
        lifted,
        lift_reason,
        prim_traffic,
        ref_traffic,
        positions_total,
        positions_epoch_clean,
        positions_pool_stable,
        epochs_total,
        epochs_prim,
        verdict,
    }
}

/// Per-method readiness across a whole compiled program (the readiness
/// analogue of `specialize::plan_program` / `cfg::plan_program`). The
/// lift decision is the real one — `cfg::lift` runs with a no-op
/// emitter — so the report can never claim more than emission would
/// deliver. Methods that fail verification are compile errors anyway
/// and are skipped, mirroring `specialize::plan_program`.
pub fn plan_program(prog: &crate::Program) -> Vec<MethodReadiness> {
    let mut out = Vec::new();
    for c in &prog.classes {
        for m in &c.methods {
            let Ok(shapes) = crate::verify::verify_method_shapes(m, &c.concats, &c.lambdas) else {
                continue;
            };
            let reason = crate::cfg::lift(m, |_, _| {}).err();
            out.push(analyze(
                &c.name,
                &m.name,
                &m.instrs,
                &shapes,
                reason.is_none(),
                reason,
            ));
        }
    }
    out
}

/// Renders the readiness aggregate as markdown (per-method rows are
/// appended by the CLI, mirroring `specialize::render_report`).
pub fn render_report(rows: &[MethodReadiness]) -> String {
    use std::fmt::Write as _;
    let methods = rows.len();
    let ready = rows
        .iter()
        .filter(|r| r.verdict == Verdict::TypedReady)
        .count();
    let all_ref = rows
        .iter()
        .filter(|r| r.verdict == Verdict::LiftableAllRef)
        .count();
    let needs_lift = rows
        .iter()
        .filter(|r| r.verdict == Verdict::NeedsCfgLift)
        .count();
    let mixed = rows
        .iter()
        .filter(|r| r.verdict == Verdict::MixedEpoch)
        .count();
    let prim: u64 = rows.iter().map(|r| r.prim_traffic).sum();
    let total: u64 = rows.iter().map(|r| r.prim_traffic + r.ref_traffic).sum();
    let epochs: usize = rows.iter().map(|r| r.epochs_total).sum();
    let epochs_prim: usize = rows.iter().map(|r| r.epochs_prim).sum();

    let mut s = String::new();
    let _ = writeln!(
        s,
        "| methods | typed-ready | liftable-all-ref | needs-cfg-lift | mixed-epoch |"
    );
    let _ = writeln!(s, "| --- | --- | --- | --- | --- |");
    let _ = writeln!(
        s,
        "| {methods} | {ready} | {all_ref} | {needs_lift} | {mixed} |"
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "| primitive occupancy | total occupancy | prim share | epochs (prim/total) |"
    );
    let _ = writeln!(s, "| --- | --- | --- | --- |");
    let _ = writeln!(
        s,
        "| {prim} | {total} | {:.1}% | {epochs_prim}/{epochs} |",
        if total == 0 {
            0.0
        } else {
            prim as f64 * 100.0 / total as f64
        }
    );
    let _ = writeln!(s);
    // Lift-reason breakdown over the needs-cfg-lift rows (the registered
    // prerequisite's exact shape census).
    let mut reasons: Vec<(String, usize)> = Vec::new();
    for r in rows {
        if r.verdict == Verdict::NeedsCfgLift {
            let name = r
                .lift_reason
                .as_ref()
                .map(|x| format!("{x:?}"))
                .unwrap_or_else(|| "Unknown".to_string());
            match reasons.iter_mut().find(|(n, _)| *n == name) {
                Some((_, c)) => *c += 1,
                None => reasons.push((name, 1)),
            }
        }
    }
    if !reasons.is_empty() {
        let _ = writeln!(s, "| needs-cfg-lift reason | methods |");
        let _ = writeln!(s, "| --- | --- |");
        for (name, count) in &reasons {
            let _ = writeln!(s, "| {name} | {count} |");
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::PcShape;
    use std::collections::BTreeMap;

    fn shapes(entries: &[(u32, Vec<VTy>, Vec<VTy>)]) -> MethodShapes {
        let mut per_pc = BTreeMap::new();
        for (pc, stack, locals) in entries {
            per_pc.insert(
                *pc,
                PcShape {
                    stack: stack.clone(),
                    locals: locals.clone(),
                },
            );
        }
        MethodShapes {
            per_pc,
            max_locals: entries.iter().map(|e| e.2.len()).max().unwrap_or(0),
            max_stack: entries.iter().map(|e| e.1.len()).max().unwrap_or(0),
        }
    }

    fn pos(r: &PositionReport) -> Vec<(Option<Kind>, bool)> {
        r.epochs.iter().map(|e| (Some(e.kind), e.mixed)).collect()
    }

    #[test]
    fn single_kind_position_is_one_clean_epoch() {
        let s = shapes(&[
            (0, vec![VTy::Int, VTy::Int], vec![VTy::Int]),
            (4, vec![VTy::Int], vec![VTy::Int]),
        ]);
        let r = analyze_position(&[0, 4], |pc| Kind::of(&s.per_pc[&pc].stack[0]), |_| false);
        assert_eq!(pos(&r), vec![(Some(Kind::Int), false)]);
        assert!(r.epoch_clean && r.pool_stable);
    }

    #[test]
    fn poison_separated_local_epochs_are_clean() {
        // The javac disjoint-range idiom the M2 report named: slot 0 is
        // Int in one range, Ref in a later one, and the merge poisons
        // the slot in between. Two clean epochs — the M2 whole-method
        // rule called this KindVaries; the live-range refinement types
        // each epoch independently.
        let s = shapes(&[
            (0, vec![], vec![VTy::Int]),
            (4, vec![], vec![VTy::Int]),
            (8, vec![], vec![VTy::Poison]),
            (12, vec![], vec![VTy::Ref]),
        ]);
        let r = analyze_position(
            &[0, 4, 8, 12],
            |pc| Kind::of(&s.per_pc[&pc].locals[0]),
            |_| false,
        );
        assert_eq!(
            pos(&r),
            vec![(Some(Kind::Int), false), (Some(Kind::Ref), false)]
        );
        assert!(r.epoch_clean);
        assert!(
            !r.pool_stable,
            "kinds differ across epochs: the M2 rule fell back here"
        );
    }

    #[test]
    fn store_retyped_local_is_two_clean_epochs() {
        // istore_0 at pc 8 redefines the slot: the Int epoch ends at the
        // store even though the merged state still shows the old value.
        let s = shapes(&[
            (0, vec![], vec![VTy::Int]),
            (4, vec![], vec![VTy::Int]),
            (8, vec![], vec![VTy::Int]),  // pre-state at the istore
            (12, vec![], vec![VTy::Ref]), // post-store: the Ref epoch
        ]);
        let instrs = [(8u32, Instr::Store(0u16))];
        let bounds: Vec<u32> = instrs
            .iter()
            .filter(|(_, i)| matches!(i, Instr::Store(0)))
            .map(|(pc, _)| *pc)
            .collect();
        let r = analyze_position(
            &[0, 4, 8, 12],
            |pc| Kind::of(&s.per_pc[&pc].locals[0]),
            |pc| bounds.contains(&pc),
        );
        assert_eq!(
            pos(&r),
            vec![(Some(Kind::Int), false), (Some(Kind::Ref), false)]
        );
        assert!(r.epoch_clean && !r.pool_stable);
    }

    #[test]
    fn iinc_is_a_store_boundary() {
        // iinc both reads and rewrites the slot: boundary. The Int kind
        // on both sides yields two clean Int epochs.
        let s = shapes(&[
            (0, vec![], vec![VTy::Int]),
            (4, vec![], vec![VTy::Int]),
            (8, vec![], vec![VTy::Int]),
        ]);
        let bounds = [4u32];
        let r = analyze_position(
            &[0, 4, 8],
            |pc| Kind::of(&s.per_pc[&pc].locals[0]),
            |pc| bounds.contains(&pc),
        );
        assert_eq!(
            pos(&r),
            vec![(Some(Kind::Int), false), (Some(Kind::Int), false)]
        );
        assert!(r.epoch_clean && r.pool_stable);
    }

    #[test]
    fn kind_change_without_kill_is_mixed() {
        // Position 0 goes Int -> Ref with no dead pc and no store: the
        // shuffle-shaped case the analysis must count AGAINST readiness.
        let s = shapes(&[
            (0, vec![VTy::Int], vec![]),
            (4, vec![VTy::Ref], vec![]),
            (8, vec![VTy::Ref], vec![]),
        ]);
        let r = analyze_position(
            &[0, 4, 8],
            |pc| Kind::of(&s.per_pc[&pc].stack[0]),
            |_| false,
        );
        // The run keeps its FIRST kind as the representative and marks
        // itself mixed — the mix, not the kind, is what blocks typing.
        assert_eq!(pos(&r), vec![(Some(Kind::Int), true)]);
        assert!(!r.epoch_clean);
    }

    #[test]
    fn depth_drop_splits_stack_epochs() {
        // Position 1 exists only at pc 0 (depth 2) and again at pc 8;
        // the depth drop at pc 4 is the kill.
        let s = shapes(&[
            (0, vec![VTy::Ref, VTy::Int], vec![]),
            (4, vec![VTy::Int], vec![]),
            (8, vec![VTy::Long, VTy::Int], vec![]),
        ]);
        let r = analyze_position(
            &[0, 4, 8],
            |pc| s.per_pc[&pc].stack.get(1).and_then(Kind::of),
            |_| false,
        );
        assert_eq!(
            pos(&r),
            vec![(Some(Kind::Int), false), (Some(Kind::Int), false)]
        );
        assert!(
            r.epoch_clean && r.pool_stable,
            "same kind in both epochs: pool-stable despite the split"
        );
        // Position 0 is occupied continuously: Ref -> Int -> Long, no
        // kill anywhere — mixed, counted against readiness.
        let r0 = analyze_position(
            &[0, 4, 8],
            |pc| s.per_pc[&pc].stack.first().and_then(Kind::of),
            |_| false,
        );
        assert!(!r0.epoch_clean);
    }

    #[test]
    fn top_null_and_arrays_collapse_to_ref_highhalf_is_dead() {
        assert_eq!(Kind::of(&VTy::Top), Some(Kind::Ref));
        assert_eq!(Kind::of(&VTy::Null), Some(Kind::Ref));
        assert_eq!(
            Kind::of(&VTy::Array(crate::ir::ElemKind::IntGroup)),
            Some(Kind::Ref)
        );
        assert_eq!(Kind::of(&VTy::HighHalf), None);
        assert_eq!(Kind::of(&VTy::Unset), None);
        assert_eq!(Kind::of(&VTy::Poison), None);
    }

    fn ready_row(r: &MethodReadiness) -> Verdict {
        r.verdict.clone()
    }

    fn analyze_with(
        instrs: &[(u32, Instr)],
        shapes: &MethodShapes,
        lifted: bool,
    ) -> MethodReadiness {
        analyze("T", "m", instrs, shapes, lifted, None)
    }

    #[test]
    fn verdicts_follow_lift_mixed_and_prims() {
        // Lift is the first gate: an unliftable method is NeedsCfgLift
        // even when its shapes are perfectly typed.
        let s = shapes(&[(0, vec![VTy::Int], vec![VTy::Int])]);
        let r = analyze_with(&[], &s, false);
        assert_eq!(ready_row(&r), Verdict::NeedsCfgLift);

        // Liftable + mixed epoch -> MixedEpoch.
        let s = shapes(&[(0, vec![VTy::Int], vec![]), (4, vec![VTy::Ref], vec![])]);
        let r = analyze_with(&[], &s, true);
        assert_eq!(ready_row(&r), Verdict::MixedEpoch);

        // Liftable + clean + all-reference epochs -> LiftableAllRef.
        let s = shapes(&[(0, vec![VTy::Ref], vec![VTy::Ref])]);
        let r = analyze_with(&[], &s, true);
        assert_eq!(ready_row(&r), Verdict::LiftableAllRef);

        // Liftable + clean + a primitive epoch -> TypedReady.
        let s = shapes(&[(0, vec![VTy::Double], vec![VTy::Double])]);
        let r = analyze_with(&[], &s, true);
        assert_eq!(ready_row(&r), Verdict::TypedReady);
        assert_eq!(r.epochs_prim, 2);
        assert_eq!(r.prim_traffic, 2);
    }

    #[test]
    fn traffic_counts_occupancy_and_skips_dead() {
        let s = shapes(&[
            (0, vec![VTy::Ref, VTy::Int], vec![VTy::Long, VTy::Unset]),
            (4, vec![VTy::Double], vec![VTy::Ref, VTy::Poison]),
        ]);
        let r = analyze_with(&[], &s, true);
        // prim: pc0 stack Int + locals Long = 2; pc4 stack Double = 1.
        assert_eq!(r.prim_traffic, 3);
        // ref: pc0 stack Ref = 1; pc4 locals Ref = 1.
        assert_eq!(r.ref_traffic, 2);
        // Unset/HighHalf/Poison occupancy is dead and counts nowhere.
        assert_eq!(r.prim_traffic + r.ref_traffic, 5);
    }
}
