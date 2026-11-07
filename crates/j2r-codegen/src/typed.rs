//! Typed-operand emission behind `--emit-typed` — the registered
//! performance lever (docs/BENCHMARKS.md "First steady-state comparison":
//! the 86.2x steady-state gap is attributed to the `Vec<Value>` operand
//! model, and the readiness wave proved structuring alone is not the
//! payload; docs/TYPED_REGISTER_READINESS.md §5 verdict: GO, sequenced,
//! step 2).
//!
//! **Investigation emission, off by default.** The approved default —
//! `match pc` dispatch over a `Vec<Value>` operand stack — is untouched;
//! a method this module cannot plan falls back to it whole, so
//! `--emit-typed` can never change semantics, only the shape of the
//! bodies it accepts (the same discipline as `--emit-cfg`).
//!
//! Model (docs/TYPED_REGISTER_READINESS.md, the readiness wave's live
//! range analysis):
//!
//! * The verifier's per-pc shapes are decomposed into per-position
//!   EPOCHS (maximal occupied runs between kills). Kinds may differ
//!   ACROSS epochs of one position — the emitter declares one typed
//!   variable per (position, epoch), re-typing per epoch exactly as the
//!   analysis allows.
//! * Wave-1 scope: PRIMITIVE positions (every epoch Int/Long/Float/
//!   Double) get typed variables; reference or mixed positions get a
//!   plain `Value` cell — the census's payload is primitive traffic
//!   (83–93% occupancy in the hot JAMA kernels), and ref positions carry
//!   the null/NPE/checkcast plumbing the cells already provide.
//! * Reads use the epoch CONTAINING the pc; writes target the epoch the
//!   value flows to — the first epoch opening AFTER the pc for stores
//!   and kind-changing refills, the containing epoch for same-kind
//!   refills (arithmetic chains overwrite their operand's variable after
//!   the operand reads — sound because the popped value is dead the
//!   moment both operands are read; the verifier's join consistency is
//!   what makes the linear epoch model agree with every control path).
//! * GC rooting is exact-parity: every `refresh` site materializes the
//!   same root set as the default body — locals as the latest value per
//!   slot (the last epoch opened at or before the pc), stack as the live
//!   prefix `0..depth` — as slice literals; no runtime change beyond the
//!   pure `stack::ref_of` law extraction.

use crate::emit::{
    arg_count, compute_leaders, f32_lit, f64_lit, is_terminator, next_pc_of, returns_void,
};
use crate::ir::{ArithOp, Conv, Instr, Ldc, Ldc2, NumKind};
use crate::readiness::{position_reports, shuffle_slot_writes, Kind};
use crate::verify::{MethodShapes, PcShape};
use crate::{ClassPlan, MethodPlan, Program};
use j2r_runtime::{parse_method_desc, type_slots};
use std::fmt::Write as _;

/// Why a method cannot be emitted typed (recorded in the fallback
/// comment; the method silently uses the approved default body).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// A `switch` — not lifted in wave 1.
    HasSwitch,
    /// An invokedynamic call site (lambdas/concat) — not lifted in wave 1.
    HasInvokeDynamic,
    /// Every position is reference-typed or dead: typed variables buy
    /// nothing (the registered `≥ 1 primitive epoch` rule).
    NoPrimitiveEpoch,
    /// The plan's own soundness net failed: an occupied (pc, slot) with
    /// no containing epoch. Never observed; any hit falls back loudly.
    PlanSanity,
}

impl Unsupported {
    pub fn name(self) -> &'static str {
        match self {
            Unsupported::HasSwitch => "HasSwitch",
            Unsupported::HasInvokeDynamic => "HasInvokeDynamic",
            Unsupported::NoPrimitiveEpoch => "NoPrimitiveEpoch",
            Unsupported::PlanSanity => "PlanSanity",
        }
    }
}

/// A primitive storage kind (the Rust variable type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prim {
    I,
    L,
    F,
    D,
}

impl Prim {
    fn of(k: Kind) -> Option<Prim> {
        match k {
            Kind::Int => Some(Prim::I),
            Kind::Long => Some(Prim::L),
            Kind::Float => Some(Prim::F),
            Kind::Double => Some(Prim::D),
            Kind::Ref => None,
        }
    }

    fn rust_ty(self) -> &'static str {
        match self {
            Prim::I => "i32",
            Prim::L => "i64",
            Prim::F => "f32",
            Prim::D => "f64",
        }
    }

    fn default_lit(self) -> &'static str {
        match self {
            Prim::I => "0i32",
            Prim::L => "0i64",
            Prim::F => "0f32",
            Prim::D => "0f64",
        }
    }

    fn value_ctor(self) -> &'static str {
        match self {
            Prim::I => "Int",
            Prim::L => "Long",
            Prim::F => "Float",
            Prim::D => "Double",
        }
    }

    fn expect_fn(self) -> &'static str {
        match self {
            Prim::I => "expect_int",
            Prim::L => "expect_long",
            Prim::F => "expect_float",
            Prim::D => "expect_double",
        }
    }
}

/// The storage plan for one operand-stack slot or local slot.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Var {
    /// Never occupied — no variable, no reads, no writes.
    Dead,
    /// One `Value` cell for the whole method (references, mixed epochs).
    Cell,
    /// One typed variable per epoch: `(start_pc, prim)`, ascending.
    Typed(Vec<(u32, Prim)>),
}

/// The per-method typed-emission plan.
pub struct Plan {
    /// Per stack SLOT index (JVMS slots; HighHalf never materializes).
    stack: Vec<Var>,
    /// Per local slot index.
    locals: Vec<Var>,
    pub typed_vars: usize,
    pub cell_vars: usize,
}

fn var_of_report(epochs: Vec<(u32, Kind)>) -> Var {
    if epochs.is_empty() {
        return Var::Dead;
    }
    let mut prims = Vec::with_capacity(epochs.len());
    for (start, k) in &epochs {
        match Prim::of(*k) {
            Some(p) => prims.push((*start, p)),
            None => return Var::Cell, // any Ref epoch → the wave-1 cell law
        }
    }
    Var::Typed(prims)
}

/// Every branch edge of a method as (branch pc, target pc) — FORWARD
/// joins and BACK edges alike. Switches never reach the analysis (plan
/// refuses them first), so these six forms are the only branchers.
fn branch_edges(m: &MethodPlan) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for (pc, ins) in &m.instrs {
        let targets: &[u32] = match ins {
            Instr::Goto(t)
            | Instr::IfZero(_, t)
            | Instr::IfICmp(_, t)
            | Instr::IfNull(t)
            | Instr::IfNonNull(t) => std::slice::from_ref(t),
            Instr::IfRefCmp { target, .. } => std::slice::from_ref(target),
            _ => &[],
        };
        for &t in targets {
            out.push((*pc, t));
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The result of the loop-epoch merge: a rewritten epoch list, or a
/// conflict that demotes the whole position to a `Value` cell.
enum MergedEpochs {
    List(Vec<(u32, Kind)>),
    Conflict,
}

/// Merges the epochs a back edge re-enters into the epoch covering the
/// back-edge target (the loop header). The linear epoch model is only
/// locally sound: a boundary inside a loop (an `iinc`, a kind-changing
/// refill) opens a NEW epoch whose variable the header's reads never see
/// — the back edge re-enters pcs that read the OLD epoch's variable, so
/// the written value never arrives (a hung loop counter is the observed
/// form). Merging the loop span's epochs into ONE variable is always
/// sound when the merged kind is uniform: one variable per span is
/// exactly the cell model's behavior restricted to the span (last write
/// wins per path). A kind conflict inside a merged group — or a back
/// edge whose target pc has no covering epoch while the loop span opens
/// one (an occupancy the verifier's join cannot have produced) — demotes
/// the position to a cell.
fn merge_branch_epochs(epochs: Vec<(u32, Kind)>, bes: &[(u32, u32)]) -> MergedEpochs {
    if epochs.is_empty() || bes.is_empty() {
        return MergedEpochs::List(epochs);
    }
    let n = epochs.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut Vec<usize>, mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    for &(b, g) in bes {
        // The unified branch law, forward and backward alike: the values
        // live in the branch's epoch and flow to every path the branch
        // opens, so ALL epochs opening inside (min, max] are ONE variable
        // with the epoch containing min. Back edge (t → h): the loop
        // span's epochs merge into the header's (the counter law). Forward
        // join (b → J): the diamond arms' epochs merge into the branch's
        // (the `if (p<0) r = -r;` law — the skip path must see the arm's
        // write and the arm must see the pre-branch value at the join).
        let lo = b.min(g);
        let hi = b.max(g);
        let lo_idx = epochs.iter().rposition(|(s, _)| *s <= lo);
        for (i, &(s, _)) in epochs.iter().enumerate() {
            if s > lo && s <= hi {
                match lo_idx {
                    Some(base) => {
                        let (a, c) = (find(&mut parent, i), find(&mut parent, base));
                        if a != c {
                            parent[a] = c;
                        }
                    }
                    // The branch span opens an epoch the source pc's state
                    // does not have — not a shape the verifier's join
                    // produces; refuse rather than guess.
                    None => return MergedEpochs::Conflict,
                }
            }
        }
    }
    // Group epochs by merge root; each group keeps its minimum start and
    // must hold a single kind.
    let mut groups: std::collections::BTreeMap<usize, (u32, Option<Kind>)> = Default::default();
    for (i, &(s, k)) in epochs.iter().enumerate() {
        let r = find(&mut parent, i);
        let g = groups.entry(r).or_insert((s, Some(k)));
        g.0 = g.0.min(s);
        if g.1 != Some(k) {
            g.1 = None;
        }
    }
    let mut out = Vec::with_capacity(groups.len());
    for (_, (s, k)) in groups {
        match k {
            Some(k) => out.push((s, k)),
            None => return MergedEpochs::Conflict,
        }
    }
    out.sort_unstable_by_key(|&(s, _)| s);
    MergedEpochs::List(out)
}

/// The plan-time epoch rewrite for one position: linear epochs from the
/// readiness analysis, branch-merged across every branch edge.
/// `Conflict` (any kind divergence inside a merged span) becomes a cell.
fn epochs_to_var(epochs: Vec<(u32, Kind)>, bes: &[(u32, u32)]) -> Var {
    match merge_branch_epochs(epochs, bes) {
        MergedEpochs::Conflict => Var::Cell,
        MergedEpochs::List(merged) => var_of_report(merged),
    }
}

/// Builds the typed plan for one method, or names the blocker.
pub fn plan(m: &MethodPlan, shapes: &MethodShapes) -> Result<Plan, Unsupported> {
    // Instruction coverage first: the whole method falls back on any
    // unsupported shape — there is no per-instruction fallback.
    for (_, ins) in &m.instrs {
        match ins {
            Instr::TableSwitch { .. } | Instr::LookupSwitch { .. } => {
                return Err(Unsupported::HasSwitch)
            }
            Instr::InvokeDynamic { .. } => return Err(Unsupported::HasInvokeDynamic),
            _ => {}
        }
    }

    let (stack_reports, local_reports) = position_reports(&m.instrs, shapes);
    let to_epochs = |r: &crate::readiness::PositionReport| -> Vec<(u32, Kind)> {
        r.epochs
            .iter()
            .zip(r.epoch_starts.iter())
            .map(|(e, &s)| (s, e.kind))
            .collect()
    };
    let bes = branch_edges(m);
    let stack: Vec<Var> = stack_reports
        .iter()
        .map(|r| epochs_to_var(to_epochs(r), &bes))
        .collect();
    let locals: Vec<Var> = local_reports
        .iter()
        .map(|r| epochs_to_var(to_epochs(r), &bes))
        .collect();

    let count = |vars: &[Var]| -> (usize, usize) {
        vars.iter().fold((0, 0), |(t, c), v| match v {
            Var::Typed(_) => (t + 1, c),
            Var::Cell => (t, c + 1),
            Var::Dead => (t, c),
        })
    };
    let (st, sc) = count(&stack);
    let (lt, lc) = count(&locals);
    let p = Plan {
        stack,
        locals,
        typed_vars: st + lt,
        cell_vars: sc + lc,
    };
    if p.typed_vars == 0 {
        return Err(Unsupported::NoPrimitiveEpoch);
    }

    // Soundness net: every occupied (pc, slot) must resolve to a
    // containing epoch (Typed) or a Cell. A Dead var hit by an occupied
    // slot, or a Typed var with no epoch covering an occupied pc, means
    // the emitter would read an undeclared variable — fall back loudly.
    for (pc, shape) in &shapes.per_pc {
        for (i, v) in shape.stack.iter().enumerate() {
            if Kind::of(*v).is_none() {
                continue;
            }
            match p.stack.get(i) {
                Some(Var::Cell) => {}
                Some(Var::Typed(epochs)) => {
                    if !epochs.iter().any(|(s, _)| *s <= *pc) {
                        return Err(Unsupported::PlanSanity);
                    }
                }
                _ => return Err(Unsupported::PlanSanity),
            }
        }
        for (j, v) in shape.locals.iter().enumerate() {
            if Kind::of(*v).is_none() {
                continue;
            }
            match p.locals.get(j) {
                Some(Var::Cell) => {}
                Some(Var::Typed(epochs)) => {
                    if !epochs.iter().any(|(s, _)| *s <= *pc) {
                        return Err(Unsupported::PlanSanity);
                    }
                }
                _ => return Err(Unsupported::PlanSanity),
            }
        }
    }
    Ok(p)
}

impl Plan {
    fn stack_name(&self, slot: usize, epoch: usize) -> String {
        format!("s{slot}e{epoch}")
    }

    fn local_name(&self, slot: usize, epoch: usize) -> String {
        format!("v{slot}e{epoch}")
    }
}

// ---------------------------------------------------------------- emitter

const RT: &str = "::j2r_runtime";

/// One storage target/read site: the variable name and its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Site {
    /// A `Value` cell (references, mixed epochs).
    Cell,
    /// A typed primitive variable of this epoch's kind.
    Typed(Prim),
}

struct Ctx<'a> {
    prog: &'a Program,
    c: &'a ClassPlan,
    m: &'a MethodPlan,
    p: &'a Plan,
    shapes: &'a MethodShapes,
    /// The method's instruction pcs, ascending — the occupancy walk of
    /// the unified write law scans this.
    pcs: Vec<u32>,
    out: Vec<String>,
}

impl<'a> Ctx<'a> {
    fn shape(&self, pc: u32) -> &'a PcShape {
        self.shapes
            .per_pc
            .get(&pc)
            .expect("j2r internal: no verified shape for pc")
    }

    /// Occupied stack slots at `pc`, bottom → top (HighHalf skipped).
    fn vs(&self, pc: u32) -> Vec<usize> {
        self.shape(pc)
            .stack
            .iter()
            .enumerate()
            .filter(|(_, v)| Kind::of(**v).is_some())
            .map(|(i, _)| i)
            .collect()
    }

    fn stack_var(&self, slot: usize) -> &'a Var {
        self.p.stack.get(slot).unwrap_or(&Var::Dead)
    }

    fn local_var(&self, slot: usize) -> &'a Var {
        self.p.locals.get(slot).unwrap_or(&Var::Dead)
    }

    /// The index of the epoch containing `pc` for a Typed var, and its prim.
    fn containing(&self, epochs: &[(u32, Prim)], pc: u32) -> (usize, Prim) {
        let idx = epochs
            .iter()
            .rposition(|(s, _)| *s <= pc)
            .expect("j2r internal: no epoch contains pc (plan sanity passed)");
        (idx, epochs[idx].1)
    }

    /// The latest epoch opened at or before `pc` (the value a GC refresh
    /// must see — the Vec's stale content in the default body).
    fn latest(&self, epochs: &[(u32, Prim)], pc: u32) -> Option<(usize, Prim)> {
        epochs
            .iter()
            .rposition(|(s, _)| *s <= pc)
            .map(|idx| (idx, epochs[idx].1))
    }

    /// Read expression for stack `slot` at `pc` (typed var or cell clone).
    fn read_s(&self, pc: u32, slot: usize) -> String {
        match self.stack_var(slot) {
            Var::Cell => format!("s{slot}.clone()"),
            Var::Typed(epochs) => {
                let (idx, _) = self.containing(epochs, pc);
                self.p.stack_name(slot, idx)
            }
            Var::Dead => unreachable!("j2r internal: read of dead stack slot (plan sanity)"),
        }
    }

    /// Site of stack `slot` at `pc`.
    fn site_s(&self, pc: u32, slot: usize) -> Site {
        match self.stack_var(slot) {
            Var::Cell => Site::Cell,
            Var::Typed(epochs) => Site::Typed(self.containing(epochs, pc).1),
            Var::Dead => unreachable!("j2r internal: site of dead stack slot"),
        }
    }

    /// The Value-typed expression for stack `slot`'s content at `pc`
    /// (call arguments, refresh snapshots, returns).
    fn wrap_s(&self, pc: u32, slot: usize) -> String {
        match self.stack_var(slot) {
            Var::Cell => format!("s{slot}.clone()"),
            Var::Typed(epochs) => {
                let (idx, prim) = self.containing(epochs, pc);
                format!(
                    "{RT}::Value::{}({})",
                    prim.value_ctor(),
                    self.p.stack_name(slot, idx)
                )
            }
            Var::Dead => unreachable!("j2r internal: wrap of dead stack slot"),
        }
    }

    /// The `pop_ref` law (`stack::ref_of`) applied to a stack slot,
    /// wrapped for `j2r_try!` exactly like the default body's pop.
    fn try_ref_of(&self, pc: u32, slot: usize, fault: &str) -> String {
        let name = match self.stack_var(slot) {
            Var::Cell => format!("&s{slot}"),
            Var::Typed(epochs) => {
                let (idx, _) = self.containing(epochs, pc);
                format!("&{}", self.p.stack_name(slot, idx))
            }
            Var::Dead => unreachable!("j2r internal: ref of dead stack slot"),
        };
        format!(
            "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::stack::ref_of({name}))"
        )
    }

    /// Read expression for local `slot` at `pc`.
    fn read_v(&self, pc: u32, slot: usize) -> String {
        match self.local_var(slot) {
            Var::Cell => format!("v{slot}.clone()"),
            Var::Typed(epochs) => {
                let (idx, _) = self.containing(epochs, pc);
                self.p.local_name(slot, idx)
            }
            Var::Dead => unreachable!("j2r internal: read of dead local slot (plan sanity)"),
        }
    }

    fn site_v(&self, pc: u32, slot: usize) -> Site {
        match self.local_var(slot) {
            Var::Cell => Site::Cell,
            Var::Typed(epochs) => Site::Typed(self.containing(epochs, pc).1),
            Var::Dead => unreachable!("j2r internal: site of dead local slot"),
        }
    }

    /// The first instruction pc after `pc` at which the position (stack
    /// slot or local slot) is occupied — the first reader of a value
    /// written at `pc`.
    fn first_occupied_after(&self, pc: u32, stack: bool, idx: usize) -> Option<u32> {
        let occ = |q: u32| -> bool {
            let Some(shape) = self.shapes.per_pc.get(&q) else {
                return false;
            };
            let v = if stack {
                shape.stack.get(idx)
            } else {
                shape.locals.get(idx)
            };
            v.is_some_and(|v| Kind::of(*v).is_some())
        };
        // `pcs` is ascending; scan past pc and take the first occupied pc.
        let start = self.pcs.partition_point(|&q| q <= pc);
        self.pcs[start..].iter().copied().find(|&q| occ(q))
    }

    /// Write target for a FRESH stack slot (a pure push at `pc`).
    fn fresh_target(&self, slot: usize, pc: u32) -> Option<(String, Site)> {
        match self.stack_var(slot) {
            Var::Cell => Some((format!("s{slot}"), Site::Cell)),
            Var::Typed(epochs) => {
                let r = self.first_occupied_after(pc, true, slot)?;
                let (idx, prim) = self.containing(epochs, r);
                Some((self.p.stack_name(slot, idx), Site::Typed(prim)))
            }
            Var::Dead => None,
        }
    }

    /// Write target for a REFILL of stack `slot` at `pc` with a value of
    /// kind `new_kind` (the incoming value's kind — asserted against the
    /// target epoch's kind when the target is typed).
    fn refill_target(
        &self,
        slot: usize,
        pc: u32,
        new_kind: Option<Kind>,
    ) -> Option<(String, Site)> {
        match self.stack_var(slot) {
            Var::Cell => Some((format!("s{slot}"), Site::Cell)),
            Var::Typed(epochs) => {
                let r = self.first_occupied_after(pc, true, slot)?;
                let (idx, prim) = self.containing(epochs, r);
                if let Some(k) = new_kind {
                    assert_eq!(
                        Some(prim),
                        Prim::of(k),
                        "j2r internal: refill kind vs target epoch"
                    );
                }
                Some((self.p.stack_name(slot, idx), Site::Typed(prim)))
            }
            Var::Dead => None,
        }
    }

    /// Write target for a STORE to local `slot` at `pc`. Same unified law.
    fn store_target(&self, slot: usize, pc: u32) -> Option<(String, Site)> {
        match self.local_var(slot) {
            Var::Cell => Some((format!("v{slot}"), Site::Cell)),
            Var::Typed(epochs) => {
                let r = self.first_occupied_after(pc, false, slot)?;
                let (idx, prim) = self.containing(epochs, r);
                Some((self.p.local_name(slot, idx), Site::Typed(prim)))
            }
            Var::Dead => None,
        }
    }

    /// Emit `target = expr;` adapting the expression to the target's kind.
    /// `expr_is_value` = the expression is already a `Value` (cell reads,
    /// helper returns); otherwise it is a primitive of `src`.
    fn assign(&mut self, target: (String, Site), expr: String, src: Src) {
        let (name, site) = target;
        let line = match (site, src) {
            (Site::Cell, Src::Prim(prim)) => {
                format!("{name} = {RT}::Value::{}({expr});", prim.value_ctor())
            }
            (Site::Cell, Src::Value) => format!("{name} = {expr};"),
            (Site::Typed(tp), Src::Prim(sp)) => {
                assert_eq!(tp, sp, "j2r internal: typed kind mismatch");
                format!("{name} = {expr};")
            }
            (Site::Typed(tp), Src::Value) => {
                format!("{name} = {expr}.{}();", tp.expect_fn())
            }
        };
        self.out.push(line);
    }

    /// The GC refresh at `pc`: the same root set as the default body —
    /// every local slot's latest value (the Vec's stale content) and the
    /// live stack prefix `0..depth` — as slice literals.
    fn refresh(&mut self, pc: u32) {
        let mut locals_v = Vec::new();
        for j in 0..self.p.locals.len().max(self.shape(pc).locals.len()) {
            match self.local_var(j) {
                Var::Cell => locals_v.push(format!("v{j}.clone()")),
                Var::Typed(epochs) => match self.latest(epochs, pc) {
                    Some((idx, prim)) => locals_v.push(format!(
                        "{RT}::Value::{}({})",
                        prim.value_ctor(),
                        self.p.local_name(j, idx)
                    )),
                    None => locals_v.push(format!("{RT}::Value::Void")),
                },
                Var::Dead => locals_v.push(format!("{RT}::Value::Void")),
            }
        }
        let mut stack_v = Vec::new();
        for (i, v) in self.shape(pc).stack.iter().enumerate() {
            if Kind::of(*v).is_none() {
                continue; // HighHalf / dead: one Vec entry per VALUE
            }
            stack_v.push(self.wrap_s(pc, i));
        }
        self.out.push(format!(
            "__frame.refresh(&[{}], &[{}], &thrown);",
            locals_v.join(", "),
            stack_v.join(", ")
        ));
    }

    /// The backward-branch GC poll (same guard as the default body's
    /// `backward_poll`, with the typed refresh).
    fn backward_poll(&mut self, pc: u32, targets: &[u32]) {
        if !targets.iter().any(|t| *t <= pc) {
            return;
        }
        self.refresh(pc);
        self.out
            .push(format!("{RT}::heap::gc_point();"));
    }

    /// int/long read of an operand slot (typed var bare, or cell unwrap).
    fn read_prim(&self, pc: u32, slot: usize, prim: Prim) -> String {
        match self.site_s(pc, slot) {
            Site::Typed(tp) => {
                assert_eq!(tp, prim, "j2r internal: operand kind");
                self.read_s(pc, slot)
            }
            Site::Cell => format!("s{slot}.{}()", prim.expect_fn()),
        }
    }
}

/// The kind of an expression being assigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Src {
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
    };
    Ok(emit_body(ctx))
}

fn try_wrap(fault: &str, expr: String) -> String {
    format!("{RT}::j2r_try!('run, thrown, fault_pc, {fault}, {expr})")
}

fn emit_body(mut ctx: Ctx<'_>) -> String {
    let m = ctx.m;
    let c = ctx.c;
    let p = ctx.p;
    let mut out = String::new();
    let instance = !m.is_static;
    let unmarshal_start = if instance { 1usize } else { 0usize };
    let locals_len = m
        .max_locals
        .max(ctx.shapes.max_locals)
        .max(if instance { 1 } else { 0 });

    let _ = writeln!(
        &mut out,
        "// j2r: typed-operand emission ({} typed vars, {} Value cells)",
        p.typed_vars, p.cell_vars
    );
    let _ = writeln!(
        &mut out,
        "pub fn {}(recv: ::core::option::Option<&{RT}::JRef>, args: &[{RT}::Value]) -> ::core::result::Result<{RT}::Value, {RT}::Throw> {{",
        m.fn_name
    );
    // FrameGuard first — identical to the default body (M2_PLAN §3.1).
    let _ = writeln!(
        &mut out,
        "    let __frame = match {RT}::frame::FrameGuard::enter() {{"
    );
    let _ = writeln!(out, "        ::core::result::Result::Ok(f) => f,");
    let _ = writeln!(
        &mut out,
        "        ::core::result::Result::Err(t) => return ::core::result::Result::Err(t),"
    );
    let _ = writeln!(out, "    }};");
    // Argument unmarshal goes through a transient Vec (the runtime's
    // unmarshal law is unchanged), then the typed/cell locals extract
    // from it — entry-time only.
    let _ = writeln!(
        &mut out,
        "    let mut __loc_tmp: ::std::vec::Vec<{RT}::Value> = ::std::vec![{RT}::Value::Void; {locals_len}usize];"
    );
    let _ = writeln!(
        &mut out,
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
    // Local variables: cells clone out; typed epochs take the entry value
    // from the unmarshal Vec when the epoch covers entry, defaults else.
    // ONLY incoming argument slots may be read from the unmarshal Vec:
    // slot 0 is `this` for instance methods, then the descriptor's
    // parameters (long/double take two slots). A non-argument local whose
    // epoch covers entry must take the typed default — the JVM verifier
    // guarantees a store before any read — because the unmarshal Vec holds
    // Void there. Live catch: BigJson's skipWs()V has a loop-live `c`
    // local that is not an argument; reading __loc_tmp[1] panicked
    // "expected int value, got Void" at runtime.
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
                    let _ = writeln!(
                        out,
                        "    let mut v{j}: {RT}::Value = {RT}::Value::Void;"
                    );
                }
            }
            Var::Typed(epochs) => {
                for (e, (start, prim)) in epochs.iter().enumerate() {
                    let name = p.local_name(j, e);
                    let ty = prim.rust_ty();
                    if e == 0 && *start <= first_pc && j < arg_slots && j < locals_len {
                        let f = prim.expect_fn();
                        let _ = writeln!(
                            out,
                            "    let mut {name}: {ty} = __loc_tmp[{j}usize].{f}();"
                        );
                    } else {
                        let default = prim.default_lit();
                        let _ = writeln!(
                            out,
                            "    let mut {name}: {ty} = {default};"
                        );
                    }
                }
            }
        }
    }
    // ACC_SYNCHRONIZED prologue — byte-identical to the default body.
    if m.is_sync {
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
    // Operand-stack variables: one Value cell or one typed var per epoch.
    for (n, var) in p.stack.iter().enumerate() {
        match var {
            Var::Dead => {}
            Var::Cell => {
                let _ = writeln!(
                    out,
                    "    let mut s{n}: {RT}::Value = {RT}::Value::Void;"
                );
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
        &mut out,
        "    let mut thrown: ::core::option::Option<{RT}::Throw> = ::core::option::Option::None;"
    );
    let _ = writeln!(out, "    let mut fault_pc: u32 = 0u32;");

    let has_table = !m.exception_table.is_empty();
    if has_table {
        let _ = writeln!(
            out,
            "    fn __j2r_find_handler(thrown_class: &str, fault: u32) -> ::core::option::Option<u32> {{"
        );
        for (start, end, handler, catch) in &m.exception_table {
            let cond = match catch {
                Some(cls) => format!(
                    " && {RT}::is_subtype(thrown_class, {cls:?}, ::core::option::Option::Some(crate::registry::parents_of))"
                ),
                None => String::new(),
            };
            let _ = writeln!(
                out,
                "        if ({start}u32..{end}u32).contains(&fault){cond} {{ return ::core::option::Option::Some({handler}u32); }}"
            );
        }
        let _ = writeln!(out, "        ::core::option::Option::None");
        let _ = writeln!(out, "    }}");
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
        let _ = writeln!(
            out,
            "            return ::core::result::Result::Err(t);"
        );
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
    let _ = writeln!(out, "            _ => ::core::unreachable!(\"j2r internal: pc out of range\"),");
    let _ = writeln!(out, "        }}");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "}}");
    out
}

fn emit_block_typed(
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

fn emit_instr_typed(ctx: &mut Ctx<'_>, pc: u32, instr: &Instr) {
    use Instr as I;
    let fault = format!("{pc}u32");
    let vs = ctx.vs(pc);
    let k = vs.len();
    match instr {
        I::Nop => {}
        I::AConstNull => {
            if let Some(t) = ctx.fresh_target(ctx.shape(pc).stack.len(), pc) {
                ctx.assign(
                    t,
                    format!("{RT}::Value::Ref(::core::option::Option::None)"),
                    Src::Value,
                );
            }
        }
        I::IConst(v) => {
            push_const(ctx, pc, &fault, Src::Prim(Prim::I), v.to_string());
        }
        I::LConst(v) => {
            push_const(ctx, pc, &fault, Src::Prim(Prim::L), format!("{v}i64"));
        }
        I::FConst(v) => {
            push_const(ctx, pc, &fault, Src::Prim(Prim::F), f32_lit(*v));
        }
        I::DConst(v) => {
            push_const(ctx, pc, &fault, Src::Prim(Prim::D), f64_lit(*v));
        }
        I::Ldc(Ldc::Int(v)) => {
            push_const(ctx, pc, &fault, Src::Prim(Prim::I), v.to_string());
        }
        I::Ldc(Ldc::Float(v)) => {
            push_const(ctx, pc, &fault, Src::Prim(Prim::F), f32_lit(*v));
        }
        I::Ldc(Ldc::String(s)) => {
            if let Some(t) = ctx.fresh_target(ctx.shape(pc).stack.len(), pc) {
                ctx.assign(
                    t,
                    format!("{RT}::Value::Ref(::core::option::Option::Some({RT}::intern_string({s:?})))"),
                    Src::Value,
                );
            }
        }
        I::Ldc(Ldc::Class(name)) => {
            if let Some(t) = ctx.fresh_target(ctx.shape(pc).stack.len(), pc) {
                ctx.assign(
                    t,
                    format!("{RT}::reflect::class_value({name:?})"),
                    Src::Value,
                );
            }
        }
        I::Ldc2W(Ldc2::Long(v)) => {
            push_const(ctx, pc, &fault, Src::Prim(Prim::L), format!("{v}i64"));
        }
        I::Ldc2W(Ldc2::Double(v)) => {
            push_const(ctx, pc, &fault, Src::Prim(Prim::D), f64_lit(*v));
        }
        I::Load(idx) => {
            let expr = ctx.read_v(pc, *idx as usize);
            let src = match ctx.site_v(pc, *idx as usize) {
                Site::Cell => Src::Value,
                Site::Typed(sp) => Src::Prim(sp),
            };
            if let Some(t) = ctx.fresh_target(ctx.shape(pc).stack.len(), pc) {
                ctx.assign(t, expr, src);
            }
        }
        I::Store(idx) => {
            let top = vs[k - 1];
            let expr = ctx.read_s(pc, top);
            let src = match ctx.site_s(pc, top) {
                Site::Cell => Src::Value,
                Site::Typed(sp) => Src::Prim(sp),
            };
            if let Some(t) = ctx.store_target(*idx as usize, pc) {
                ctx.assign(t, expr, src);
            }
        }
        I::ALoad(_) => {
            // pop the int index, then the array ref; the result lands on
            // the ref's slot (kind change → next epoch, or the cell).
            let idx_expr = ctx.read_prim(pc, vs[k - 1], Prim::I);
            ctx.out.push(format!("let __idx = {idx_expr};"));
            let arr_expr = ctx.try_ref_of(pc, vs[k - 2], &fault);
            ctx.out.push(format!("let __arr = {arr_expr};"));
            let load = try_wrap(
                &fault,
                format!("{RT}::array::array_load(&__arr, __idx)"),
            );
            ctx.out.push(format!("let __v = {load};"));
            if let Some(t) = ctx.refill_target(vs[k - 2], pc, Some(elem_kind(instr))) {
                ctx.assign(t, "__v".to_string(), Src::Value);
            }
        }
        I::AStore(_) => {
            let v_expr = ctx.wrap_s(pc, vs[k - 1]);
            ctx.out.push(format!("let __v = {v_expr};"));
            let idx_expr = ctx.read_prim(pc, vs[k - 2], Prim::I);
            ctx.out.push(format!("let __idx = {idx_expr};"));
            let arr_expr = ctx.try_ref_of(pc, vs[k - 3], &fault);
            ctx.out.push(format!("let __arr = {arr_expr};"));
            let store = try_wrap(
                &fault,
                format!("{RT}::array::array_store(&__arr, __idx, __v, crate::j2r_dispatch())"),
            );
            ctx.out.push(format!("{store};"));
        }
        I::ArrayLength => {
            let arr_expr = ctx.try_ref_of(pc, vs[k - 1], &fault);
            ctx.out.push(format!("let __arr = {arr_expr};"));
            let len = try_wrap(&fault, format!("{RT}::array::array_length(&__arr)"));
            ctx.out.push(format!("let __len = {len};"));
            if let Some(t) = ctx.refill_target(vs[k - 1], pc, Some(Kind::Int)) {
                // __len is the raw i32 (try_wrap unwrapped the Result);
                // Src::Prim lets a Cell target wrap it in Value::Int.
                ctx.assign(t, "__len".to_string(), Src::Prim(Prim::I));
            }
        }
        I::Pop | I::Pop2 => {}
        I::Dup | I::DupX1 | I::DupX2 | I::Dup2 | I::Dup2X1 | I::Dup2X2 | I::Swap => {
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
                    .map(|(_, t, site)| (t.clone(), match site {
                        Site::Cell => Src::Value,
                        Site::Typed(sp) => Src::Prim(*sp),
                    }))
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
        I::Arith { op, kind } => emit_arith_typed(ctx, pc, &fault, *op, *kind, &vs),
        I::Convert(conv) => emit_convert_typed(ctx, pc, *conv, &vs),
        I::IInc { idx, c } => {
            let j = *idx as usize;
            let expr = ctx.read_v(pc, j);
            let line = match ctx.site_v(pc, j) {
                Site::Typed(Prim::I) => {
                    let (target, _) = ctx
                        .store_target(j, pc)
                        .expect("j2r internal: iinc target");
                    format!("{target} = {expr}.wrapping_add({c});")
                }
                Site::Cell => {
                    let (target, _) = ctx
                        .store_target(j, pc)
                        .expect("j2r internal: iinc target");
                    format!("{target} = {RT}::Value::Int(v{j}.expect_int().wrapping_add({c}));")
                }
                Site::Typed(other) => unreachable!("j2r internal: iinc on {other:?}"),
            };
            ctx.out.push(line);
        }
        I::Cmp(kind) => emit_cmp_typed(ctx, pc, *kind, &vs),
        I::IfZero(op, t) => {
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
            ctx.backward_poll(pc, &[*t]);
            let a = ctx.read_s(pc, vs[k - 1]);
            ctx.out.push(format!("let __a = {a};"));
            ctx.out.push(format!(
                "if __a.is_null() {{ pc = {t}u32; }} else {{ pc = {}u32; }}",
                next_pc_of(ctx.m, pc)
            ));
        }
        I::IfNonNull(t) => {
            ctx.backward_poll(pc, &[*t]);
            let a = ctx.read_s(pc, vs[k - 1]);
            ctx.out.push(format!("let __a = {a};"));
            ctx.out.push(format!(
                "if !__a.is_null() {{ pc = {t}u32; }} else {{ pc = {}u32; }}",
                next_pc_of(ctx.m, pc)
            ));
        }
        I::Goto(t) => {
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
        I::GetStatic { class, field, .. } => {
            emit_getstatic_typed(ctx, pc, &fault, class, field);
        }
        I::PutStatic { class, field, .. } => {
            let f = crate::resolve_field_pub(ctx.prog, class, field, true)
                .expect("validated static field");
            let slot = f.static_slot.expect("static field");
            // ensure → possible <clinit> dispatch; refresh while the value
            // is still on the stack so the snapshot holds it.
            ctx.refresh(pc);
            let v = ctx.wrap_s(pc, vs[k - 1]);
            ctx.out.push(format!("let __v = {v};"));
            for (id, expr) in crate::emit::ensure_calls(ctx.prog, class) {
                ctx.out
                    .push(format!("crate::statics::ensure({id}u32, {expr});"));
            }
            ctx.out
                .push(format!("crate::statics::set({slot}u32, __v);"));
        }
        I::GetField { class, field, .. } => {
            let f = crate::resolve_field_pub(ctx.prog, class, field, false)
                .expect("validated field");
            let slot = f.instance_slot.expect("instance field");
            let r = ctx.try_ref_of(pc, vs[k - 1], &fault);
            ctx.out.push(format!("let __r = {r};"));
            // obj_get returns a plain `Value` (the NPE is already raised
            // by ref_of above) — exactly the default body's law; no
            // j2r_try wrap here.
            ctx.out
                .push(format!("let __v = {RT}::object::obj_get(&__r, {slot}usize);"));
            if let Some(t) = ctx.refill_target(vs[k - 1], pc, Some(desc_kind_of(&f.desc))) {
                ctx.assign(t, "__v".to_string(), Src::Value);
            }
        }
        I::PutField { class, field, .. } => {
            let slot = crate::resolve_field_slot_pub(ctx.prog, class, field, false)
                .expect("validated field");
            let v = ctx.wrap_s(pc, vs[k - 1]);
            ctx.out.push(format!("let __v = {v};"));
            let r = ctx.try_ref_of(pc, vs[k - 2], &fault);
            ctx.out.push(format!("let __r = {r};"));
            ctx.out
                .push(format!("{RT}::object::obj_set(&__r, {slot}usize, __v);"));
        }
        I::InvokeVirtual { class, name, desc }
        | I::InvokeInterface { class, name, desc } => {
            // Call boundary: refresh the root snapshot before the args
            // leave the operand positions (M2_PLAN §4.4).
            ctx.refresh(pc);
            let n = arg_count(desc);
            let args: Vec<String> = ((k - n)..k).map(|i| ctx.wrap_s(pc, vs[i])).collect();
            ctx.out.push(format!("let __a = vec![{}];", args.join(", ")));
            let r = ctx.try_ref_of(pc, vs[k - n - 1], &fault);
            ctx.out.push(format!("let __r = {r};"));
            let call = try_wrap(
                &fault,
                format!(
                    "{RT}::invoke_virtual(&__r, {class:?}, {name:?}, {desc:?}, &__a, crate::j2r_dispatch())"
                ),
            );
            ctx.out.push(format!("let __v = {call};"));
            write_invoke_result(ctx, pc, vs[k - n - 1], desc);
        }
        I::InvokeSpecial { class, name, desc } => {
            ctx.refresh(pc);
            let n = arg_count(desc);
            let args: Vec<String> = ((k - n)..k).map(|i| ctx.wrap_s(pc, vs[i])).collect();
            ctx.out.push(format!("let __a = vec![{}];", args.join(", ")));
            let r = ctx.try_ref_of(pc, vs[k - n - 1], &fault);
            ctx.out.push(format!("let __r = {r};"));
            match crate::resolve_call_pub(ctx.prog, class, name, desc).expect("validated call") {
                crate::CallTarget::User { module, fn_name } => {
                    let call = try_wrap(
                        &fault,
                        format!(
                            "crate::classes::{module}::{fn_name}(::core::option::Option::Some(&__r), &__a)"
                        ),
                    );
                    ctx.out.push(format!("let __v = {call};"));
                }
                crate::CallTarget::Native { class: ncls } => {
                    let call = try_wrap(
                        &fault,
                        format!(
                            "{RT}::natives::invoke_exact({ncls:?}, {name:?}, {desc:?}, ::core::option::Option::Some(&__r), &__a, crate::j2r_dispatch())"
                        ),
                    );
                    ctx.out.push(format!("let __v = {call};"));
                }
            }
            write_invoke_result(ctx, pc, vs[k - n - 1], desc);
        }
        I::InvokeStatic { class, name, desc } => {
            ctx.refresh(pc);
            let n = arg_count(desc);
            let args: Vec<String> = ((k - n)..k).map(|i| ctx.wrap_s(pc, vs[i])).collect();
            ctx.out.push(format!("let __a = vec![{}];", args.join(", ")));
            match crate::resolve_call_pub(ctx.prog, class, name, desc).expect("validated call") {
                crate::CallTarget::User { module, fn_name } => {
                    for (id, expr) in crate::emit::ensure_calls(ctx.prog, class) {
                        ctx.out
                            .push(format!("crate::statics::ensure({id}u32, {expr});"));
                    }
                    let call = try_wrap(
                        &fault,
                        format!(
                            "crate::classes::{module}::{fn_name}(::core::option::Option::None, &__a)"
                        ),
                    );
                    ctx.out.push(format!("let __v = {call};"));
                }
                crate::CallTarget::Native { class: ncls } => {
                    let call = try_wrap(
                        &fault,
                        format!(
                            "{RT}::natives::invoke_exact({ncls:?}, {name:?}, {desc:?}, ::core::option::Option::None, &__a, crate::j2r_dispatch())"
                        ),
                    );
                    ctx.out.push(format!("let __v = {call};"));
                }
            }
            if !returns_void(desc) {
                if n >= 1 {
                    // The result replaces the first popped arg's slot.
                    write_invoke_result(ctx, pc, vs[k - n], desc);
                } else {
                    // A no-arg non-void static call (System.nanoTime,
                    // Math.random, ...) pushes onto a FRESH slot — there
                    // is no popped slot to refill.
                    if let Some(t) = ctx.fresh_target(ctx.shape(pc).stack.len(), pc) {
                        ctx.assign(t, "__v".to_string(), Src::Value);
                    }
                }
            }
        }
        I::MonitorEnter => {
            let r = ctx.try_ref_of(pc, vs[k - 1], &fault);
            ctx.out.push(format!("let __r = {r};"));
            let enter = try_wrap(&fault, format!("{RT}::monitor::monitor_enter(&__r)"));
            ctx.out.push(format!("{enter};"));
        }
        I::MonitorExit => {
            let r = ctx.try_ref_of(pc, vs[k - 1], &fault);
            ctx.out.push(format!("let __r = {r};"));
            let exit = try_wrap(&fault, format!("{RT}::monitor::monitor_exit(&__r)"));
            ctx.out.push(format!("{exit};"));
        }
        I::New { class } => {
            // The one collection point shape (M2_PLAN §4.3).
            ctx.refresh(pc);
            ctx.out.push(format!("{RT}::heap::gc_point();"));
            crate::emit::new_obj_lines(ctx.prog, class, &fault, &mut ctx.out);
            if let Some(t) = ctx.fresh_target(ctx.shape(pc).stack.len(), pc) {
                ctx.assign(
                    t,
                    format!("{RT}::Value::Ref(::core::option::Option::Some(__nobj))"),
                    Src::Value,
                );
            }
        }
        I::NewArray(prim) => {
            ctx.refresh(pc);
            ctx.out.push(format!("{RT}::heap::gc_point();"));
            let desc = prim.array_desc();
            let len = ctx.read_prim(pc, vs[k - 1], Prim::I);
            ctx.out.push(format!("let __len = {len};"));
            let new = try_wrap(
                &fault,
                format!("{RT}::array::array_new_prim({desc:?}, __len)"),
            );
            ctx.out.push(format!("let __narr = {new};"));
            // The result REFILLS the popped length operand's slot (the
            // value_effect law: array construction pushes in place) — a
            // fresh stack.len() target would be beyond the stack and the
            // write would be skipped entirely.
            if let Some(t) = ctx.refill_target(vs[k - 1], pc, Some(Kind::Ref)) {
                ctx.assign(
                    t,
                    format!("{RT}::Value::Ref(::core::option::Option::Some(__narr))"),
                    Src::Value,
                );
            }
        }
        I::ANewArray { elem } => {
            ctx.refresh(pc);
            ctx.out.push(format!("{RT}::heap::gc_point();"));
            let len = ctx.read_prim(pc, vs[k - 1], Prim::I);
            ctx.out.push(format!("let __len = {len};"));
            let new = try_wrap(
                &fault,
                format!("{RT}::array::array_new_ref({elem:?}, __len)"),
            );
            ctx.out.push(format!("let __narr = {new};"));
            // In-place refill of the popped length slot (see NewArray).
            if let Some(t) = ctx.refill_target(vs[k - 1], pc, Some(Kind::Ref)) {
                ctx.assign(
                    t,
                    format!("{RT}::Value::Ref(::core::option::Option::Some(__narr))"),
                    Src::Value,
                );
            }
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
                &fault,
                format!("{RT}::array::array_new_multi({desc:?}, &[{}])", list.join(", ")),
            );
            ctx.out.push(format!("let __narr = {new};"));
            // In-place refill of the FIRST popped dimension slot (the
            // deepest of the dims) — the result lands exactly there.
            if let Some(t) = ctx.refill_target(vs[k - *dims as usize], pc, Some(Kind::Ref)) {
                ctx.assign(
                    t,
                    format!("{RT}::Value::Ref(::core::option::Option::Some(__narr))"),
                    Src::Value,
                );
            }
        }
        I::AThrow => {
            let thr = ctx.try_ref_of(pc, vs[k - 1], &fault);
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
                &fault,
                format!("{RT}::checkcast({v}, {class:?}, crate::j2r_dispatch())"),
            );
            ctx.out.push(format!("let __v = {cc};"));
            if let Some(t) = ctx.refill_target(vs[k - 1], pc, Some(Kind::Ref)) {
                ctx.assign(t, "__v".to_string(), Src::Value);
            }
        }
        I::InstanceOf { class } => {
            let v = ctx.read_s(pc, vs[k - 1]);
            ctx.out.push(format!("let __v = {v};"));
            let io = format!(
                "{RT}::j2r_instance_of(&__v, {class:?}, crate::j2r_dispatch()) as i32"
            );
            if let Some(t) = ctx.refill_target(vs[k - 1], pc, Some(Kind::Int)) {
                ctx.assign(t, io, Src::Prim(Prim::I));
            }
        }
        I::TableSwitch { .. } | I::LookupSwitch { .. } => {
            unreachable!("j2r internal: switch rejected by the typed plan")
        }
        I::InvokeDynamic { .. } => {
            unreachable!("j2r internal: invokedynamic rejected by the typed plan")
        }
    }
}

/// The `Kind` of a stack slot's verified type at `pc`.
fn kind_at(ctx: &Ctx<'_>, pc: u32, slot: usize) -> Option<Kind> {
    Kind::of(ctx.shape(pc).stack[slot])
}

/// A pure push of a primitive constant (or a `Value`-typed one).
fn push_const(ctx: &mut Ctx<'_>, pc: u32, _fault: &str, src: Src, lit: String) {
    if let Some(t) = ctx.fresh_target(ctx.shape(pc).stack.len(), pc) {
        ctx.assign(t, lit, src);
    }
}

/// The element `Kind` of an `xXaload` instruction (the refill's kind).
fn elem_kind(instr: &Instr) -> Kind {
    match instr {
        Instr::ALoad(crate::ir::ElemKind::IntGroup) => Kind::Int,
        Instr::ALoad(crate::ir::ElemKind::Long) => Kind::Long,
        Instr::ALoad(crate::ir::ElemKind::Float) => Kind::Float,
        Instr::ALoad(crate::ir::ElemKind::Double) => Kind::Double,
        _ => Kind::Ref,
    }
}

/// The `Kind` of a field/call-result descriptor's type.
fn desc_kind_of(desc: &str) -> Kind {
    match desc.as_bytes().last() {
        Some(b'I') | Some(b'B') | Some(b'C') | Some(b'S') | Some(b'Z') => Kind::Int,
        Some(b'J') => Kind::Long,
        Some(b'F') => Kind::Float,
        Some(b'D') => Kind::Double,
        _ => Kind::Ref,
    }
}

/// The invoke result's refill write: lands on the slot the receiver (or,
/// for statics, the deepest argument) occupied — the kind-change law of
/// the analysis applies unchanged. Void returns write nothing.
fn write_invoke_result(ctx: &mut Ctx<'_>, pc: u32, slot: usize, desc: &str) {
    if returns_void(desc) {
        return;
    }
    let k = desc_kind_of(&return_desc(desc));
    if let Some(t) = ctx.refill_target(slot, pc, Some(k)) {
        ctx.assign(t, "__v".to_string(), Src::Value);
    }
}

/// The return-type descriptor of a method descriptor (`(II)D` → `D`).
fn return_desc(desc: &str) -> String {
    let close = desc.rfind(')').expect("valid descriptor");
    desc[close + 1..].to_string()
}

fn emit_arith_typed(
    ctx: &mut Ctx<'_>,
    pc: u32,
    fault: &str,
    op: ArithOp,
    kind: NumKind,
    vs: &[usize],
) {
    use NumKind as NK;
    let k = vs.len();
    match kind {
        NK::Int | NK::Long => {
            let prim = if kind == NK::Long { Prim::L } else { Prim::I };
            let (result_slot, a_slot, b_slot) = match op {
                ArithOp::Neg => (vs[k - 1], vs[k - 1], vs[k - 1]),
                _ => (vs[k - 2], vs[k - 2], vs[k - 1]),
            };
            // Reads first (the same-kind refill overwrites the operand's
            // variable), then write.
            let a = ctx.read_prim(pc, a_slot, prim);
            let b = if op == ArithOp::Neg {
                String::new()
            } else if op == ArithOp::Shl || op == ArithOp::Shr || op == ArithOp::UShr {
                // The shift amount is always an int, even for long shifts.
                ctx.read_prim(pc, b_slot, Prim::I)
            } else {
                ctx.read_prim(pc, b_slot, prim)
            };
            let new_kind = match kind {
                NK::Long => Kind::Long,
                _ => Kind::Int,
            };
            match op {
                ArithOp::Neg => {
                    ctx.out.push(format!("let __a = {a};"));
                    if let Some(t) = ctx.refill_target(result_slot, pc, Some(new_kind)) {
                        ctx.assign(t, "__a.wrapping_neg()".to_string(), Src::Prim(prim));
                    }
                }
                ArithOp::Add | ArithOp::Sub | ArithOp::Mul | ArithOp::And | ArithOp::Or
                | ArithOp::Xor => {
                    let rust_op = match op {
                        ArithOp::Add => "wrapping_add",
                        ArithOp::Sub => "wrapping_sub",
                        ArithOp::Mul => "wrapping_mul",
                        ArithOp::And => "&",
                        ArithOp::Or => "|",
                        _ => "^",
                    };
                    ctx.out.push(format!("let __b = {b}; let __a = {a};"));
                    let expr = if rust_op.starts_with("wrapping") {
                        format!("__a.{rust_op}(__b)")
                    } else {
                        format!("__a {rust_op} __b")
                    };
                    if let Some(t) = ctx.refill_target(result_slot, pc, Some(new_kind)) {
                        ctx.assign(t, expr, Src::Prim(prim));
                    }
                }
                ArithOp::Div | ArithOp::Rem => {
                    let helper = match (op, kind) {
                        (ArithOp::Div, NK::Long) => "long_div",
                        (ArithOp::Div, _) => "int_div",
                        (ArithOp::Rem, NK::Long) => "long_rem",
                        _ => "int_rem",
                    };
                    ctx.out.push(format!("let __b = {b}; let __a = {a};"));
                    let div = try_wrap(fault, format!("{RT}::{helper}(__a, __b)"));
                    ctx.out.push(format!("let __r = {div};"));
                    if let Some(t) = ctx.refill_target(result_slot, pc, Some(new_kind)) {
                        ctx.assign(t, "__r".to_string(), Src::Prim(prim));
                    }
                }
                ArithOp::Shl | ArithOp::Shr | ArithOp::UShr => {
                    let mask = if kind == NK::Long { 63 } else { 31 };
                    let shift_fn = if op == ArithOp::Shl {
                        "wrapping_shl"
                    } else {
                        "wrapping_shr"
                    };
                    ctx.out.push(format!("let __b = {b}; let __a = {a};"));
                    let expr = if op == ArithOp::UShr {
                        // logical shift on the unsigned reinterpretation
                        let uty = if kind == NK::Long { "u64" } else { "u32" };
                        format!(
                            " ((__a as {uty}).{shift_fn}((__b & {mask}) as u32)) as {}",
                            if kind == NK::Long { "i64" } else { "i32" }
                        )
                        .trim_start()
                        .to_string()
                    } else {
                        format!("__a.{shift_fn}((__b & {mask}) as u32)")
                    };
                    if let Some(t) = ctx.refill_target(result_slot, pc, Some(new_kind)) {
                        ctx.assign(t, expr, Src::Prim(prim));
                    }
                }
            }
        }
        NK::Float | NK::Double => {
            let prim = if kind == NK::Float { Prim::F } else { Prim::D };
            let (result_slot, a_slot, b_slot) = match op {
                ArithOp::Neg => (vs[k - 1], vs[k - 1], vs[k - 1]),
                _ => (vs[k - 2], vs[k - 2], vs[k - 1]),
            };
            let a = ctx.read_prim(pc, a_slot, prim);
            let b = ctx.read_prim(pc, b_slot, prim);
            let new_kind = if kind == NK::Float { Kind::Float } else { Kind::Double };
            let rust_op = match op {
                ArithOp::Neg => {
                    ctx.out.push(format!("let __a = {a};"));
                    if let Some(t) = ctx.refill_target(result_slot, pc, Some(new_kind)) {
                        ctx.assign(t, "-__a".to_string(), Src::Prim(prim));
                    }
                    return;
                }
                ArithOp::Add => "+",
                ArithOp::Sub => "-",
                ArithOp::Mul => "*",
                ArithOp::Div => "/",
                _ => "%",
            };
            ctx.out.push(format!("let __b = {b}; let __a = {a};"));
            if let Some(t) = ctx.refill_target(result_slot, pc, Some(new_kind)) {
                ctx.assign(t, format!("__a {rust_op} __b"), Src::Prim(prim));
            }
        }
        _ => unreachable!("arith on narrow kind"),
    }
}

fn emit_convert_typed(ctx: &mut Ctx<'_>, pc: u32, conv: Conv, vs: &[usize]) {
    use NumKind as NK;
    let k = vs.len();
    // `to_prim` is deliberately unused: the CAST uses the descriptor kinds
    // (the match below) and the storage kind is the from-kind for narrow
    // targets, decided by `refill_target` — the pair match pins legality.
    let (from_prim, _to_prim) = match (conv.from, conv.to) {
        (NK::Int, NK::Long) => (Prim::I, Prim::L),
        (NK::Int, NK::Float) => (Prim::I, Prim::F),
        (NK::Int, NK::Double) => (Prim::I, Prim::D),
        (NK::Int, NK::Byte) | (NK::Int, NK::Char) | (NK::Int, NK::Short) => (Prim::I, Prim::I),
        (NK::Long, NK::Int) | (NK::Long, NK::Float) | (NK::Long, NK::Double) => {
            (Prim::L, Prim::L)
        }
        (NK::Float, NK::Int) | (NK::Float, NK::Long) => (Prim::F, Prim::F),
        (NK::Float, NK::Double) => (Prim::F, Prim::D),
        (NK::Double, NK::Int) | (NK::Double, NK::Long) => (Prim::D, Prim::D),
        (NK::Double, NK::Float) => (Prim::D, Prim::F),
        _ => unreachable!("conversion pair"),
    };
    // Rust's `as` casts match Java exactly here (emit_convert's law):
    // float/double → int/long saturate with NaN → 0; int → narrow wraps;
    // int → char zero-extends. The CAST uses the descriptor kinds; the
    // variable kind is the from-kind for narrow targets.
    let cast: &str = match (conv.from, conv.to) {
        (NK::Int, NK::Long) => " as i64",
        (NK::Int, NK::Float) => " as f32",
        (NK::Int, NK::Double) => " as f64",
        (NK::Int, NK::Byte) => " as i8 as i32",
        (NK::Int, NK::Char) => " as u16 as i32",
        (NK::Int, NK::Short) => " as i16 as i32",
        (NK::Long, NK::Int) => " as i32",
        (NK::Long, NK::Float) => " as f32",
        (NK::Long, NK::Double) => " as f64",
        (NK::Float, NK::Int) => " as i32",
        (NK::Float, NK::Long) => " as i64",
        (NK::Float, NK::Double) => " as f64",
        (NK::Double, NK::Int) => " as i32",
        (NK::Double, NK::Long) => " as i64",
        (NK::Double, NK::Float) => " as f32",
        _ => unreachable!("conversion pair"),
    };
    let (result_slot, a_slot) = (vs[k - 1], vs[k - 1]);
    let a = ctx.read_prim(pc, a_slot, from_prim);
    ctx.out.push(format!("let __a = {a};"));
    let new_kind = match conv.to {
        NK::Long => Kind::Long,
        NK::Float => Kind::Float,
        NK::Double => Kind::Double,
        _ => Kind::Int,
    };
    // The result variable's type: the TARGET kind for widening to
    // long/float/double, the from-kind (int) for narrow targets.
    let target = ctx.refill_target(result_slot, pc, Some(new_kind));
    if let Some((name, site)) = target {
        let expr = format!("__a{cast}");
        let src = match (site, conv.to) {
            (Site::Typed(_), NK::Byte) | (Site::Typed(_), NK::Char) | (Site::Typed(_), NK::Short) => {
                Src::Prim(Prim::I)
            }
            (Site::Typed(_), NK::Long) => Src::Prim(Prim::L),
            (Site::Typed(_), NK::Float) => Src::Prim(Prim::F),
            (Site::Typed(_), NK::Double) => Src::Prim(Prim::D),
            (Site::Typed(_), NK::Int) => Src::Prim(Prim::I),
            (Site::Cell, _) => Src::Prim(Prim::I), // wrapped below by kind
        };
        // For Cell targets the wrap kind must be the DESCRIPTOR kind.
        if site == Site::Cell {
            let ctor = match new_kind {
                Kind::Long => "Long",
                Kind::Float => "Float",
                Kind::Double => "Double",
                _ => "Int",
            };
            ctx.out
                .push(format!("{name} = {RT}::Value::{ctor}({expr});"));
        } else {
            ctx.assign((name, site), expr, src);
        }
    }
}

fn emit_cmp_typed(ctx: &mut Ctx<'_>, pc: u32, kind: crate::ir::CmpKind, vs: &[usize]) {
    use crate::ir::CmpKind as CK;
    let k = vs.len();
    let (a_slot, b_slot) = (vs[k - 2], vs[k - 1]);
    let (prim, nan_val): (Prim, i32) = match kind {
        CK::LCmp => (Prim::L, 0),
        CK::FCmpL => (Prim::F, -1),
        CK::FCmpG => (Prim::F, 1),
        CK::DCmpL => (Prim::D, -1),
        CK::DCmpG => (Prim::D, 1),
    };
    let b = ctx.read_prim(pc, b_slot, prim);
    let a = ctx.read_prim(pc, a_slot, prim);
    ctx.out.push(format!("let __b = {b}; let __a = {a};"));
    let expr = if kind == CK::LCmp {
        "match __a.cmp(&__b) { ::core::cmp::Ordering::Less => -1i32, ::core::cmp::Ordering::Greater => 1i32, ::core::cmp::Ordering::Equal => 0i32 }".to_string()
    } else {
        format!(
            "if __a.is_nan() || __b.is_nan() {{ {nan_val}i32 }} else {{ match __a.partial_cmp(&__b) {{ ::core::option::Option::Some(::core::cmp::Ordering::Less) => -1i32, ::core::option::Option::Some(::core::cmp::Ordering::Greater) => 1i32, _ => 0i32 }} }}"
        )
    };
    if let Some(t) = ctx.refill_target(a_slot, pc, Some(Kind::Int)) {
        ctx.assign(t, expr, Src::Prim(Prim::I));
    }
}

fn emit_getstatic_typed(ctx: &mut Ctx<'_>, pc: u32, _fault: &str, class: &str, field: &str) {
    fn push_value(ctx: &mut Ctx<'_>, pc: u32, expr: String) {
        // Every call site pushes a REF (System.out/err, Locale.US, TYPE,
        // any ref-typed static): a ref epoch means the position is a Cell,
        // so the target is a Cell and `Src::Value` stores the Value
        // directly. A Typed target here would take `assign`'s
        // `expect_fn` unwrap — unreachable for refs by the verifier.
        if let Some(t) = ctx.fresh_target(ctx.shape(pc).stack.len(), pc) {
            match t.1 {
                Site::Cell => ctx.assign(t, expr, Src::Value),
                Site::Typed(_) => ctx.assign(t, expr, Src::Value),
            }
        }
    }
    if class == "java/lang/System" && field == "out" {
        push_value(ctx, pc, format!("{RT}::natives::system_out()"));
    } else if class == "java/lang/System" && field == "err" {
        push_value(ctx, pc, format!("{RT}::natives::system_err()"));
    } else if class == "java/util/Locale" && field == "US" {
        push_value(ctx, pc, format!("{RT}::natives::builtin_locale_us()"));
    } else if field == "TYPE" {
        if let Some(desc) = crate::primitive_type_field(class) {
            push_value(ctx, pc, format!("{RT}::reflect::class_value({desc:?})"));
        } else {
            let f = crate::resolve_field_pub(ctx.prog, class, field, true)
                .expect("validated static field");
            let slot = f.static_slot.expect("static field");
            push_value(ctx, pc, format!("crate::statics::get({slot}u32)"));
        }
    } else {
        let f = crate::resolve_field_pub(ctx.prog, class, field, true)
            .expect("validated static field");
        let slot = f.static_slot.expect("static field");
        let ensures = crate::emit::ensure_calls(ctx.prog, class);
        // `ensure` may dispatch into user code (<clinit>): refresh first.
        if !ensures.is_empty() {
            ctx.refresh(pc);
        }
        for (id, expr) in ensures {
            ctx.out
                .push(format!("crate::statics::ensure({id}u32, {expr});"));
        }
        let kind = desc_kind_of(&f.desc);
        if let Some(t) = ctx.fresh_target(ctx.shape(pc).stack.len(), pc) {
            match t.1 {
                Site::Cell => ctx.assign(
                    t,
                    format!("crate::statics::get({slot}u32)"),
                    Src::Value,
                ),
                Site::Typed(tp) => {
                    assert_eq!(Some(tp), Prim::of(kind), "j2r internal: static kind");
                    ctx.assign(
                        t,
                        format!("crate::statics::get({slot}u32)"),
                        Src::Value,
                    );
                }
            }
        }
    }
}



// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    // --- merge_branch_epochs: the loop-carried + join laws -------------

    #[test]
    fn back_edge_merges_loop_span_into_one_epoch() {
        // The LoopCounter law: istore opens an epoch at 2, iinc inside the
        // loop opens another at 11, the goto back to 4 re-enters reads of
        // the first — the two MUST merge into one variable.
        let epochs = vec![(2, Kind::Int), (11, Kind::Int)];
        let bes = vec![(16, 4)];
        match merge_branch_epochs(epochs, &bes) {
            MergedEpochs::List(m) => assert_eq!(m, vec![(2, Kind::Int)]),
            MergedEpochs::Conflict => panic!("same-kind loop span must merge, not conflict"),
        }
    }

    #[test]
    fn back_edge_kind_conflict_demotes_position_to_cell() {
        // A merged span holding two kinds cannot be one variable.
        let epochs = vec![(2, Kind::Int), (11, Kind::Long)];
        let bes = vec![(16, 4)];
        assert!(matches!(
            merge_branch_epochs(epochs, &bes),
            MergedEpochs::Conflict
        ));
        assert_eq!(
            epochs_to_var(vec![(2, Kind::Int), (11, Kind::Long)], &[(16, 4)]),
            Var::Cell
        );
    }

    #[test]
    fn back_edge_without_header_epoch_conflicts() {
        // The loop span opens an epoch the header state does not have —
        // refuse (cell) rather than guess.
        assert!(matches!(
            merge_branch_epochs(vec![(6, Kind::Int)], &[(12, 4)]),
            MergedEpochs::Conflict
        ));
    }

    #[test]
    fn no_back_edges_keeps_linear_epochs() {
        let epochs = vec![(2, Kind::Int), (11, Kind::Long)];
        match merge_branch_epochs(epochs.clone(), &[]) {
            MergedEpochs::List(m) => assert_eq!(m, epochs),
            MergedEpochs::Conflict => panic!("no back edges: nothing to merge"),
        }
    }

    #[test]
    fn disjoint_slot_reuse_outside_the_loop_span_survives() {
        // javac's disjoint-range slot reuse (the LoopCounter v2 slot: int
        // epoch then long epoch) — a back edge AFTER both starts must not
        // merge them; two typed epochs remain.
        let epochs = vec![(2, Kind::Int), (11, Kind::Long)];
        let bes = vec![(47, 31)];
        match merge_branch_epochs(epochs, &bes) {
            MergedEpochs::List(m) => {
                assert_eq!(m, vec![(2, Kind::Int), (11, Kind::Long)])
            }
            MergedEpochs::Conflict => panic!("disjoint reuse is not loop-carried"),
        }
    }

    #[test]
    fn nested_loop_back_edges_merge_transitively() {
        // Two back edges (inner 16→9, outer 20→4) over epochs at 5, 11, 18:
        // all three merge with the epoch covering 4.
        let epochs = vec![(2, Kind::Int), (5, Kind::Int), (11, Kind::Int), (18, Kind::Int)];
        let bes = vec![(16, 9), (20, 4)];
        match merge_branch_epochs(epochs, &bes) {
            MergedEpochs::List(m) => assert_eq!(m, vec![(2, Kind::Int)]),
            MergedEpochs::Conflict => panic!("uniform-kind nested loops must merge"),
        }
    }

    // --- back_edges: the branch census ---------------------------------

    #[test]
    fn branch_edges_collect_forward_and_backward() {
        // Forward Goto, self-loop IfNull, backward IfICmp; Return is not
        // a brancher. Edges come out sorted, deduped.
        let m = MethodPlan {
            name: "t".into(),
            desc: "()V".into(),
            is_static: true,
            is_sync: false,
            fn_name: "t".into(),
            instrs: vec![
                (0, Instr::Goto(9)),                                  // forward
                (1, Instr::IfNull(1)),                                // self-loop
                (4, Instr::IfICmp(crate::ir::CmpOp::Ge, 0)),          // backward
                (9, Instr::Return(j2r_runtime::TypeDesc::Void)),
            ],
            exception_table: vec![],
            max_locals: 0,
            max_stack: 2,
        };
        assert_eq!(branch_edges(&m), vec![(0, 9), (1, 1), (4, 0)]);
    }

    #[test]
    fn forward_join_merges_diamond_arm_into_branch_epoch() {
        // The tql2 `if (p < 0) r = -r;` law: local 14's pre-branch epoch
        // (192), the negate arm's write epoch (199), and the join read —
        // branch 194 → 206 merges BOTH into one variable, so the skip
        // path sees the arm's write and the arm sees the pre-branch value.
        let epochs = vec![(2, Kind::Int), (192, Kind::Int), (199, Kind::Int)];
        let bes = vec![(194, 206)];
        match merge_branch_epochs(epochs, &bes) {
            // The pre-branch epoch (192) and the arm's write epoch (199)
            // unify; the unrelated earlier live range (2) stays separate.
            MergedEpochs::List(m) => assert_eq!(m, vec![(2, Kind::Int), (192, Kind::Int)]),
            MergedEpochs::Conflict => panic!("uniform-kind diamond must merge"),
        }
    }

    #[test]
    fn if_else_diamond_merges_both_arms_transitively() {
        // javac layout: if_icmp L2; THEN; goto L3; L2: ELSE; L3:. The
        // if_icmp merges THEN's epochs into the branch epoch; the goto
        // merges ELSE's epochs into the (already merged) THEN group —
        // transitively one variable for the whole diamond.
        let epochs = vec![(2, Kind::Int), (10, Kind::Int), (20, Kind::Int)];
        let bes = vec![(8, 18), (16, 30)];
        match merge_branch_epochs(epochs, &bes) {
            MergedEpochs::List(m) => assert_eq!(m, vec![(2, Kind::Int)]),
            MergedEpochs::Conflict => panic!("transitive diamond merge failed"),
        }
    }

    // --- Unsupported::name: the fallback marker is machine-counted -----

    #[test]
    fn fallback_reason_names_are_stable() {
        assert_eq!(Unsupported::HasSwitch.name(), "HasSwitch");
        assert_eq!(Unsupported::HasInvokeDynamic.name(), "HasInvokeDynamic");
        assert_eq!(Unsupported::NoPrimitiveEpoch.name(), "NoPrimitiveEpoch");
        assert_eq!(Unsupported::PlanSanity.name(), "PlanSanity");
    }

    // --- var_of_report / Plan Var laws ----------------------------------

    #[test]
    fn var_of_report_laws() {
        assert_eq!(var_of_report(vec![]), Var::Dead);
        assert_eq!(var_of_report(vec![(0, Kind::Ref)]), Var::Cell);
        assert_eq!(
            var_of_report(vec![(0, Kind::Int), (4, Kind::Long)]),
            Var::Typed(vec![(0, Prim::I), (4, Prim::L)])
        );
    }
}
