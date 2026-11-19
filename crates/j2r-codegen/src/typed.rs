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
//!   same ROOT SET as the default body — since the G3.2 ref-only
//!   projection (2026-11-12) the slice literals carry only the
//!   positions that can contribute a handle (`Var::Cell` locals,
//!   REF-kind live stack positions); the runtime's refresh discards
//!   every primitive entry, so the prim wraps were pure construction +
//!   scan waste. The handle list written into the frame slot is
//!   IDENTICAL to the old emission — locals as the latest value per
//!   slot (the last epoch opened at or before the pc), stack as the live
//!   prefix `0..depth`; no runtime change beyond the
//!   pure `stack::ref_of` law extraction.
//! * Operand-stack/register unification (2026-11-08, the readiness §5
//!   lever): within one block, a stack value whose target is a TYPED
//!   slot variable need not be WRITTEN there — a pure def (constants,
//!   local loads, non-trapping arithmetic, conversions, comparisons)
//!   keeps its expression tree (composed through let-bindings, so trees
//!   stay one level deep) and an impure def (array/field loads, calls,
//!   trapping division) keeps the temp variable holding its
//!   already-computed result, with the `expect_*` unwrap hoisted from
//!   the slot write to the def site. The entry lands in its slot
//!   variable — exactly where the old emitter wrote it, same target —
//!   when a GC refresh, a shuffle (dup family), or the block handoff
//!   needs the storage snapshot. Branch operands stay symbolic on
//!   forward branches (the branch consumes them itself) and materialize
//!   under the backward-branch refresh (the snapshot must hold them —
//!   the default body refreshes before the branch pops). GC-root parity
//!   is by construction: the refresh reads the same slot variables the
//!   old emitter maintained, at the same sites, with the same values.
//!   Reference positions keep the cell path unchanged (their slots are
//!   always `Value` cells; the null/NPE/checkcast plumbing is untouched).

use crate::ir::Instr;
use crate::readiness::{position_reports, Kind};
use crate::verify::{MethodShapes, PcShape};
use crate::{ClassPlan, MethodPlan, Program};
use std::fmt::Write as _;

/// Builds the typed plan for one method, or names the blocker.
pub fn plan(m: &MethodPlan, shapes: &MethodShapes) -> Result<Plan, Unsupported> {
    // Instruction coverage first: the whole method falls back on any
    // unsupported shape — there is no per-instruction fallback.
    for (_, ins) in &m.instrs {
        match ins {
            Instr::TableSwitch { .. } | Instr::LookupSwitch { .. } => {
                return Err(Unsupported::HasSwitch)
            }
            Instr::NewIntArrayFilled { .. } => return Err(Unsupported::HasArrayLiteralFill),
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
            if Kind::of(v).is_none() {
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
            if Kind::of(v).is_none() {
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
pub(crate) enum Site {
    /// A `Value` cell (references, mixed epochs).
    Cell,
    /// A typed primitive variable of this epoch's kind.
    Typed(Prim),
}

pub(crate) struct Ctx<'a> {
    prog: &'a Program,
    c: &'a ClassPlan,
    m: &'a MethodPlan,
    p: &'a Plan,
    shapes: &'a MethodShapes,
    /// The method's instruction pcs, ascending — the occupancy walk of
    /// the unified write law scans this.
    pcs: Vec<u32>,
    out: Vec<String>,
    /// Symbolic stack entries keyed by SLOT INDEX — the unification
    /// layer's pending defs (see `SymEntry`). Cleared at every block
    /// start; materialized at refresh sites, shuffles, and the block
    /// handoff. Typed targets only.
    sym: std::collections::BTreeMap<usize, SymEntry>,
    /// Counter for the unification layer's `__t{n}` temps (unique per
    /// method; arm-scoped lets would suffice, unique names are simpler).
    tmp_n: usize,
    /// How many pushes the layer elided (the emitted header comment
    /// reports it — the observability census for the lever).
    sym_pushes: usize,
}

/// A stack value whose slot-variable write the emitter ELIDED. The
/// value is carried as a Rust expression instead of storage: a pure def
/// keeps its expression tree, an impure def keeps the name of the
/// method-unique temp (`__t{n}`) holding its already-computed result —
/// fresh temps because a pending def can live across a later def of the
/// same helper binding (`__v`, `__r` shadowing). The entry lands in its
/// slot variable — same target, same value, exactly where the old
/// emitter wrote it — when the storage snapshot is needed.
pub(crate) struct SymEntry {
    /// The read expression. `Some(prim)` defs read as a prim-typed
    /// expression; ref defs (`None`) read as a `Value`-typed expression.
    read: String,
    /// The def's verified kind — the read expression's type law.
    prim: Option<Prim>,
    /// The slot-variable target the materialization write uses — the
    /// same target the old emitter computed at push time.
    target: (String, Site),
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
            .filter(|(_, v)| Kind::of(v).is_some())
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

    /// Read expression for stack `slot` at `pc`. A symbolic ref entry
    /// (`prim: None`) reads as a cloned `Value` expression; a symbolic
    /// prim entry must never reach this method (its callers adapt via
    /// `read_for`) — the plan's slot variables are the fallback.
    fn read_s(&self, pc: u32, slot: usize) -> String {
        if let Some(e) = self.sym.get(&slot) {
            if e.prim.is_none() {
                return format!("{}.clone()", e.read);
            }
        }
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
    /// (call arguments, refresh snapshots, returns). A symbolic prim
    /// entry wraps its prim read in the def's ctor — the same Value the
    /// slot variable would hold; a symbolic ref entry clones its temp.
    fn wrap_s(&self, pc: u32, slot: usize) -> String {
        if let Some(e) = self.sym.get(&slot) {
            return match e.prim {
                Some(tp) => format!("{RT}::Value::{}({})", tp.value_ctor(), e.read),
                None => format!("{}.clone()", e.read),
            };
        }
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
    /// wrapped for `j2r_try!` exactly like the default body's pop. A
    /// symbolic ref entry borrows its temp instead of the slot variable.
    fn try_ref_of(&self, pc: u32, slot: usize, fault: &str) -> String {
        if let Some(e) = self.sym.get(&slot) {
            if e.prim.is_none() {
                return format!(
                    "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::stack::ref_of(&{}))",
                    e.read
                );
            }
        }
        let name = match self.stack_var(slot) {
            Var::Cell => format!("&s{slot}"),
            Var::Typed(epochs) => {
                let (idx, _) = self.containing(epochs, pc);
                format!("&{}", self.p.stack_name(slot, idx))
            }
            Var::Dead => unreachable!("j2r internal: ref of dead stack slot"),
        };
        format!("{RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::stack::ref_of({name}))")
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
            v.is_some_and(|v| Kind::of(v).is_some())
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
    /// live stack prefix `0..depth` — as slice literals. The unification
    /// layer's pending entries land in their slot variables FIRST, so
    /// the snapshot reads the same storage the old emitter maintained.
    ///
    /// G3.2 slice (2026-11-12, the REF-ONLY PROJECTION): the runtime's
    /// `FrameGuard::refresh` consumes ONLY `Value::Ref(Some(_))` entries;
    /// every primitive entry is scanned, matched and discarded, so the
    /// prim wraps (and the `Void` HighHalf fills) are pure construction
    /// and scan waste at the hottest sites (a call boundary in JAMA pays
    /// a full locals array per invoke). The typed plan knows exactly
    /// which positions can root: `Var::Cell` locals (ref or mixed; a
    /// mixed position may hold a ref at this pc) and REF-kind live stack
    /// positions. Typed local positions are ALL-PRIM by plan
    /// construction and Dead positions hold no value; neither can
    /// contribute a handle. The ROOT SET is unchanged byte-for-byte
    /// (the handle list refresh writes is identical); only the array
    /// literals shrink. Sites unchanged: the safepoint discipline is
    /// per-site, and the sites are not moved.
    fn refresh(&mut self, pc: u32) {
        self.materialize_all();
        let mut locals_v = Vec::new();
        for j in 0..self.p.locals.len().max(self.shape(pc).locals.len()) {
            if let Var::Cell = self.local_var(j) {
                locals_v.push(format!("v{j}.clone()"));
            }
        }
        let mut stack_v = Vec::new();
        for (i, v) in self.shape(pc).stack.iter().enumerate() {
            if Kind::of(v) != Some(Kind::Ref) {
                continue; // HighHalf / prim / dead: no rooting value
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
        self.out.push(format!("{RT}::heap::gc_point();"));
    }

    /// int/long/float/double read of an operand slot (symbolic entry,
    /// typed var bare, or cell unwrap).
    fn read_prim(&self, pc: u32, slot: usize, prim: Prim) -> String {
        if let Some(e) = self.sym.get(&slot) {
            match e.prim {
                Some(tp) => {
                    assert_eq!(tp, prim, "j2r internal: symbolic operand kind");
                    return e.read.clone();
                }
                None => unreachable!("j2r internal: ref entry read as prim"),
            }
        }
        match self.site_s(pc, slot) {
            Site::Typed(tp) => {
                assert_eq!(tp, prim, "j2r internal: operand kind");
                self.read_s(pc, slot)
            }
            Site::Cell => format!("s{slot}.{}()", prim.expect_fn()),
        }
    }

    /// The (expression, Src) pair for a stack read that adapts to the
    /// CONSUMER's needs: a symbolic prim entry yields its bare read (no
    /// expect, no slot round trip); a symbolic ref entry clones its temp
    /// (Value); no entry falls back to the site law. Used by consumers
    /// that push the value onward (Store into locals).
    fn read_for(&self, pc: u32, slot: usize) -> (String, Src) {
        if let Some(e) = self.sym.get(&slot) {
            return match e.prim {
                Some(tp) => (e.read.clone(), Src::Prim(tp)),
                None => (format!("{}.clone()", e.read), Src::Value),
            };
        }
        let expr = match self.stack_var(slot) {
            Var::Cell => format!("s{slot}.clone()"),
            Var::Typed(epochs) => {
                let (idx, _) = self.containing(epochs, pc);
                self.p.stack_name(slot, idx)
            }
            Var::Dead => unreachable!("j2r internal: read of dead stack slot (plan sanity)"),
        };
        let src = match self.site_s(pc, slot) {
            Site::Cell => Src::Value,
            Site::Typed(sp) => Src::Prim(sp),
        };
        (expr, src)
    }

    /// The unified stack-push law (operand-stack/register unification).
    /// Records a symbolic entry instead of writing the slot variable:
    /// the def's VERIFIED kind (`kind`) decides the read law, the slot's
    /// storage kind (Typed or Cell) only decides the materialization
    /// write. Pure defs keep their expression tree (composed through
    /// let-bindings, so trees stay one level deep); impure defs
    /// materialize into a method-unique `__t{n}` temp at the def site
    /// with the `expect_*` unwrap hoisted from the slot write to the
    /// def. Dead targets drop the value exactly as the old emitter did.
    fn push_result(
        &mut self,
        slot: usize,
        target: Option<(String, Site)>,
        expr: String,
        src: Src,
        pure: bool,
        kind: Kind,
    ) {
        let Some((name, site)) = target else { return };
        let prim = match kind {
            Kind::Int => Some(Prim::I),
            Kind::Long => Some(Prim::L),
            Kind::Float => Some(Prim::F),
            Kind::Double => Some(Prim::D),
            Kind::Ref => None,
        };
        // The read expression's type must follow the DEF's kind.
        let mut read = match (prim, src) {
            (Some(_), Src::Prim(_)) => expr,
            (Some(tp), Src::Value) => format!("{expr}.{}()", tp.expect_fn()),
            (None, Src::Value) => expr,
            (None, Src::Prim(_)) => unreachable!("j2r internal: prim expr for a ref def"),
        };
        if let (Some(tp), Some(sp)) = (
            prim,
            match src {
                Src::Prim(sp) => Some(sp),
                Src::Value => None,
            },
        ) {
            assert_eq!(tp, sp, "j2r internal: push kind");
        }
        if !pure {
            // Method-unique temps: a pending def can live across a later
            // def of a shared helper binding (`__v`, `__r` shadowing —
            // `foo(f(x)); g(y); use(x_result)`), so reuse is unsound.
            let t = format!("__t{}", self.tmp_n);
            self.tmp_n += 1;
            let ty = match prim {
                Some(tp) => tp.rust_ty().to_string(),
                None => format!("{RT}::Value"),
            };
            self.out.push(format!("let {t}: {ty} = {read};"));
            read = t;
        }
        self.sym.insert(
            slot,
            SymEntry {
                read,
                prim,
                target: (name, site),
            },
        );
        self.sym_pushes += 1;
    }

    /// Writes every pending symbolic entry into its slot variable and
    /// clears the map — the storage snapshot a GC refresh, a shuffle, or
    /// a block handoff needs. The write targets and values are exactly
    /// the old emitter's push-time writes, so the snapshot content is
    /// unchanged (GC-root parity by construction).
    fn materialize_all(&mut self) {
        let entries: Vec<SymEntry> = std::mem::take(&mut self.sym).into_values().collect();
        for e in entries {
            self.materialize_write(e);
        }
    }

    /// Materializes only the entries BELOW `min_slot` — a branch's own
    /// operands stay symbolic (a forward branch consumes them itself;
    /// a backward branch's refresh materializes them into the snapshot).
    fn materialize_below(&mut self, min_slot: usize) {
        let keys: Vec<usize> = self.sym.range(..min_slot).map(|(k, _)| *k).collect();
        for k in keys {
            if let Some(e) = self.sym.remove(&k) {
                self.materialize_write(e);
            }
        }
    }

    /// One materialization write, adapted to the entry's def kind and
    /// the slot variable's storage kind: a prim def stores its bare read
    /// into a typed slot, wraps in the ctor for a cell slot; a ref def
    /// stores its (cloned) Value temp.
    fn materialize_write(&mut self, e: SymEntry) {
        let (name, site) = e.target;
        let line = match (e.prim, site) {
            (Some(tp), Site::Typed(tsp)) => {
                assert_eq!(tp, tsp, "j2r internal: materialize kind");
                format!("{name} = {};", e.read)
            }
            (Some(tp), Site::Cell) => {
                format!("{name} = {RT}::Value::{}({});", tp.value_ctor(), e.read)
            }
            (None, Site::Cell) => format!("{name} = {}.clone();", e.read),
            (None, Site::Typed(_)) => {
                unreachable!("j2r internal: ref def into a typed slot")
            }
        };
        self.out.push(line);
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
        let epochs = vec![
            (2, Kind::Int),
            (5, Kind::Int),
            (11, Kind::Int),
            (18, Kind::Int),
        ];
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
            is_native: false,
            is_sync: false,
            modifiers: 0,
            signature: None,
            lvtt: Vec::new(),
            array_ops: Default::default(),
            fn_name: "t".into(),
            instrs: vec![
                (0, Instr::Goto(9)),                         // forward
                (1, Instr::IfNull(1)),                       // self-loop
                (4, Instr::IfICmp(crate::ir::CmpOp::Ge, 0)), // backward
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

// --- round-7 file-split pass: verbatim children below ---
mod plan;
pub use plan::Plan;
pub use plan::Prim;
pub use plan::Unsupported;
pub(crate) use plan::*;
mod body;
pub(crate) use body::*;
mod instr;
pub(crate) use instr::*;
mod arith;
pub(crate) use arith::*;
