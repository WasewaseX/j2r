//! `typed`::plan — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use crate::ir::Instr;
use crate::readiness::Kind;
use crate::MethodPlan;

/// Why a method cannot be emitted typed (recorded in the fallback
/// comment; the method silently uses the approved default body).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// A `switch` — not lifted in wave 1.
    HasSwitch,
    /// A collapsed array-literal fill (G2.6) — the default body emits the
    /// literal form; the typed path simply falls back for it.
    HasArrayLiteralFill,
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
            Unsupported::HasArrayLiteralFill => "HasArrayLiteralFill",
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
    pub(crate) fn of(k: Kind) -> Option<Prim> {
        match k {
            Kind::Int => Some(Prim::I),
            Kind::Long => Some(Prim::L),
            Kind::Float => Some(Prim::F),
            Kind::Double => Some(Prim::D),
            Kind::Ref => None,
        }
    }

    pub(crate) fn rust_ty(self) -> &'static str {
        match self {
            Prim::I => "i32",
            Prim::L => "i64",
            Prim::F => "f32",
            Prim::D => "f64",
        }
    }

    pub(crate) fn default_lit(self) -> &'static str {
        match self {
            Prim::I => "0i32",
            Prim::L => "0i64",
            Prim::F => "0f32",
            Prim::D => "0f64",
        }
    }

    pub(crate) fn value_ctor(self) -> &'static str {
        match self {
            Prim::I => "Int",
            Prim::L => "Long",
            Prim::F => "Float",
            Prim::D => "Double",
        }
    }

    pub(crate) fn expect_fn(self) -> &'static str {
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
pub(crate) enum Var {
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
    pub(crate) stack: Vec<Var>,
    /// Per local slot index.
    pub(crate) locals: Vec<Var>,
    pub typed_vars: usize,
    pub cell_vars: usize,
}

pub(crate) fn var_of_report(epochs: Vec<(u32, Kind)>) -> Var {
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
pub(crate) fn branch_edges(m: &MethodPlan) -> Vec<(u32, u32)> {
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
pub(crate) enum MergedEpochs {
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
pub(crate) fn merge_branch_epochs(epochs: Vec<(u32, Kind)>, bes: &[(u32, u32)]) -> MergedEpochs {
    if epochs.is_empty() || bes.is_empty() {
        return MergedEpochs::List(epochs);
    }
    let n = epochs.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
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
pub(crate) fn epochs_to_var(epochs: Vec<(u32, Kind)>, bes: &[(u32, u32)]) -> Var {
    match merge_branch_epochs(epochs, bes) {
        MergedEpochs::Conflict => Var::Cell,
        MergedEpochs::List(merged) => var_of_report(merged),
    }
}
