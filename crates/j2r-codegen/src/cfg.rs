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
use std::collections::{BTreeSet, HashMap, HashSet};

/// Why a method cannot be lifted (recorded in the lift report).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    EmptyBody,
    HasExceptionTable,
    HasSwitch,
    Irreducible,
    MultiExitLoop,
    ImpureLoopExit,
    ImpureJoin,
    BranchIntoEmittedCode,
    FallOffEnd,
}

// ---------------------------------------------------------------- CFG

struct Block {
    /// Index of the block's first instruction.
    start: usize,
    /// Index one past the block's last instruction.
    end: usize,
    /// Successor instruction indices (deduped, order preserved).
    succs: Vec<usize>,
}

struct Cfg {
    blocks: Vec<Block>,
    entry: usize,
}

impl Cfg {
    fn build(m: &MethodPlan) -> Cfg {
        let n = m.instrs.len();
        let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut is_term = vec![false; n];
        for (i, (_pc, instr)) in m.instrs.iter().enumerate() {
            let push = |target_pc: u32, out: &mut Vec<Vec<usize>>| {
                if let Some(j) = m.instrs.iter().position(|(p, _)| *p == target_pc) {
                    out[i].push(j);
                }
            };
            match instr {
                Instr::Goto(t) => {
                    push(*t, &mut succ);
                    is_term[i] = true;
                }
                Instr::IfZero(_, t)
                | Instr::IfICmp(_, t)
                | Instr::IfNull(t)
                | Instr::IfNonNull(t)
                | Instr::IfRefCmp { target: t, .. } => {
                    push(*t, &mut succ);
                    if i + 1 < n {
                        succ[i].push(i + 1);
                    }
                    is_term[i] = true;
                }
                Instr::TableSwitch {
                    default, targets, ..
                } => {
                    push(*default, &mut succ);
                    for t in targets {
                        push(*t, &mut succ);
                    }
                    is_term[i] = true;
                }
                Instr::LookupSwitch { default, pairs, .. } => {
                    push(*default, &mut succ);
                    for (_, t) in pairs {
                        push(*t, &mut succ);
                    }
                    is_term[i] = true;
                }
                Instr::Return(_) | Instr::AThrow => is_term[i] = true,
                _ => {}
            }
        }
        // branch-target instruction indices
        let mut targets: BTreeSet<usize> = BTreeSet::new();
        for s in succ.iter().flatten() {
            targets.insert(*s);
        }
        let mut blocks = Vec::new();
        let mut i = 0;
        while i < n {
            let start = i;
            let mut j = i;
            let mut blk_succ = Vec::new();
            loop {
                if is_term[j] {
                    blk_succ = succ[j].clone();
                    j += 1;
                    break;
                }
                if j + 1 < n && targets.contains(&(j + 1)) {
                    blk_succ = vec![j + 1];
                    j += 1;
                    break;
                }
                j += 1;
                if j >= n {
                    break;
                }
            }
            blocks.push(Block {
                start,
                end: j,
                succs: blk_succ,
            });
            i = j;
        }
        // Remap successors from instruction indices to block indices —
        // every consumer (DFS, preds, loops) works on blocks.
        let starts: Vec<usize> = blocks.iter().map(|b| b.start).collect();
        for b in blocks.iter_mut() {
            b.succs = b
                .succs
                .iter()
                .map(|&i| {
                    starts.binary_search(&i).unwrap_or_else(|_| {
                        starts
                            .iter()
                            .position(|&s| s <= i)
                            .map(|pos| {
                                let mut k = pos;
                                while k + 1 < starts.len() && starts[k + 1] <= i {
                                    k += 1;
                                }
                                k
                            })
                            .expect("instruction inside a block")
                    })
                })
                .collect();
        }
        Cfg { blocks, entry: 0 }
    }

    fn block_of(&self, instr_idx: usize) -> usize {
        self.blocks
            .iter()
            .position(|b| b.start <= instr_idx && instr_idx < b.end)
            .expect("instruction inside a block")
    }

    fn preds(&self) -> Vec<Vec<usize>> {
        let mut preds = vec![Vec::new(); self.blocks.len()];
        for (i, b) in self.blocks.iter().enumerate() {
            for &s in &b.succs {
                preds[s].push(i);
            }
        }
        preds
    }

    /// Sorted, deduped successor lists (block indices) — the DFS order
    /// used by both `analyse` and `idom`.
    fn sorted_succ_lists(&self) -> Vec<Vec<usize>> {
        self.blocks
            .iter()
            .map(|b| {
                let mut v: Vec<usize> = b.succs.clone();
                v.sort_unstable();
                v.dedup();
                v
            })
            .collect()
    }

    /// Immediate dominators (Cooper-Harvey-Kennedy): iterative RPO walk,
    /// a node's idom seeded with a processed PREDECESSOR NODE, intersects
    /// on reverse-postorder indices.
    ///
    /// The previous implementation seeded with `idoms[pred]` (the
    /// predecessor's own idom) and intersected on raw block indices — a
    /// node with a single predecessor then got its parent's idom instead
    /// of the parent, every dominator chain through a loop body skipped
    /// the header, and the back-edge dominance test in `analyse`
    /// mis-flagged EVERY loop whose header is not the entry block as
    /// `Irreducible` (found 2026-11-07 by the readiness wave: javac's
    /// plain `while`/`for` loops failed `cfg-report` while synthetic
    /// header==entry tests passed). Unsound only in the conservative
    /// direction — affected methods fell back to the exact dispatch
    /// body — but it silently zeroed the lift's real-loop coverage.
    fn idom(&self) -> Vec<Option<usize>> {
        let n = self.blocks.len();
        let succs = self.sorted_succ_lists();

        // Reverse postorder over the reachable graph (iterative DFS,
        // ascending successor order — same order as `analyse`'s).
        let mut state = vec![0u8; n]; // 0 unvisited, 1 on stack, 2 done
        let mut post: Vec<usize> = Vec::new();
        let mut stack: Vec<(usize, usize)> = vec![(self.entry, 0)];
        state[self.entry] = 1;
        while let Some(&mut (node, ref mut idx)) = stack.last_mut() {
            if *idx < succs[node].len() {
                let s = succs[node][*idx];
                *idx += 1;
                if state[s] == 0 {
                    state[s] = 1;
                    stack.push((s, 0));
                }
            } else {
                state[node] = 2;
                post.push(node);
                stack.pop();
            }
        }
        let rpo: Vec<usize> = post.iter().rev().copied().collect();
        let mut rpono = vec![0usize; n];
        for (k, &node) in rpo.iter().enumerate() {
            rpono[node] = k;
        }

        let preds = self.preds();
        let mut idoms: Vec<Option<usize>> = vec![None; n];
        idoms[self.entry] = Some(self.entry);
        let mut changed = true;
        while changed {
            changed = false;
            for &b in &rpo {
                if b == self.entry {
                    continue;
                }
                let mut new: Option<usize> = None;
                for &p in &preds[b] {
                    if idoms[p].is_some() {
                        new = Some(match new {
                            // THE FIX: seed with the predecessor node
                            // itself, not the predecessor's idom.
                            None => p,
                            Some(cur) => intersect(&idoms, &rpono, cur, p),
                        });
                    }
                }
                if new.is_some() && new != idoms[b] {
                    idoms[b] = new;
                    changed = true;
                }
            }
        }
        idoms
    }

    fn dominates(idoms: &[Option<usize>], a: usize, b: usize) -> bool {
        let mut cur = Some(b);
        while let Some(c) = cur {
            if c == a {
                return true;
            }
            match idoms[c] {
                Some(d) if d != c => cur = Some(d),
                _ => break,
            }
        }
        false
    }
}

/// CHK intersect on reverse-postorder indices: walk whichever finger is
/// RPO-deeper up its idom chain until the fingers meet.
fn intersect(idoms: &[Option<usize>], rpono: &[usize], mut a: usize, mut b: usize) -> usize {
    while a != b {
        while rpono[a] > rpono[b] {
            a = idoms[a].unwrap_or(a);
        }
        while rpono[b] > rpono[a] {
            b = idoms[b].unwrap_or(b);
        }
    }
    a
}

// ------------------------------------------------------- reducibility + loops

#[derive(Debug)]
struct Loop {
    header: usize,
    /// The single OPEN exit edge (from-block, to-block) — `None` when
    /// every exit target terminates (early `return`/`throw` blocks are
    /// emitted inline at the edge, so the loop needs no post-loop exit
    /// region at all).
    exit: Option<(usize, usize)>,
    /// Every block of the natural loop (header included) — used by the
    /// builder's join-purity check to recognise the loop's own back
    /// edges at a loop-header meet, and by the arm walk to cover a
    /// traversed loop's internal predecessors of a shared exit.
    nodes: HashSet<usize>,
}

/// Pure-`goto` trampoline blocks: a block of exactly one `goto t` whose
/// target is a different block. javac chains scope exits through them
/// (`break` → trampoline → after-loop), so two exit edges can reach the
/// SAME continuation via different blocks — the readiness wave's
/// multi-exit census measured exactly this (hdr_pc=99 → targets [126, 135]
/// where block 126 is just `goto 135`). Stops key on the RESOLVED target;
/// chains never walk through a loop header (edges to a header are the
/// loop top, not whatever follows it) and never through a self-jump.
fn trampoline_map(cfg: &Cfg, m: &MethodPlan, loops: &[Loop]) -> HashMap<usize, usize> {
    let headers: HashSet<usize> = loops.iter().map(|l| l.header).collect();
    let mut raw: HashMap<usize, usize> = HashMap::new();
    for (i, b) in cfg.blocks.iter().enumerate() {
        if b.end == b.start + 1 {
            if let Instr::Goto(t) = &m.instrs[b.start].1 {
                if let Some(ti) = m.instrs.iter().position(|(p, _)| *p == *t) {
                    let tb = cfg.block_of(ti);
                    if tb != i {
                        raw.insert(i, tb);
                    }
                }
            }
        }
    }
    let mut out: HashMap<usize, usize> = HashMap::new();
    for &from in raw.keys() {
        let mut cur = from;
        while let Some(&n) = raw.get(&cur) {
            if headers.contains(&cur) {
                break;
            }
            cur = n;
        }
        if !headers.contains(&from) {
            out.insert(from, cur);
        }
    }
    out
}

/// Analyse the CFG; `Err(reason)` means the method falls back to dispatch.
/// On success returns the loops plus the trampoline-resolution map (the
/// builder keys every stop lookup on resolved targets).
fn analyse(cfg: &Cfg, m: &MethodPlan) -> Result<(Vec<Loop>, HashMap<usize, usize>), Unsupported> {
    if m.instrs.is_empty() {
        return Err(Unsupported::EmptyBody);
    }
    if !m.exception_table.is_empty() {
        return Err(Unsupported::HasExceptionTable);
    }
    if m.instrs
        .iter()
        .any(|(_, i)| matches!(i, Instr::TableSwitch { .. } | Instr::LookupSwitch { .. }))
    {
        return Err(Unsupported::HasSwitch);
    }

    // Retreating edges via iterative DFS.
    let n = cfg.blocks.len();
    let succs: Vec<Vec<usize>> = cfg
        .blocks
        .iter()
        .map(|b| {
            let mut v: Vec<usize> = b.succs.clone();
            v.sort_unstable();
            v.dedup();
            v
        })
        .collect();
    let mut state = vec![0u8; n]; // 0 unvisited, 1 on stack, 2 done
    let mut retreating: Vec<(usize, usize)> = Vec::new();
    let mut stack: Vec<(usize, usize)> = vec![(cfg.entry, 0)];
    state[cfg.entry] = 1;
    while let Some(&mut (node, ref mut idx)) = stack.last_mut() {
        if *idx < succs[node].len() {
            let s = succs[node][*idx];
            *idx += 1;
            match state[s] {
                0 => {
                    state[s] = 1;
                    stack.push((s, 0));
                }
                1 => retreating.push((node, s)),
                _ => {}
            }
        } else {
            state[node] = 2;
            stack.pop();
        }
    }

    let idoms = cfg.idom();
    for &(from, to) in &retreating {
        if !Cfg::dominates(&idoms, to, from) {
            return Err(Unsupported::Irreducible);
        }
    }

    // Natural loops (unioned across all back edges of a header). Exit
    // handling (revised by the lift-closure wave, 2026-11-07):
    //
    // * Edges are grouped by TARGET BLOCK, not by (from, to) pair —
    //   javac's `break` inside a `while`/`do-while` adds a second edge to
    //   the SAME after-loop block, and the builder already routes every
    //   body→exit edge through one `break` stop. Comparing (from, to)
    //   pairs refused exactly those loops.
    // * A target block whose terminator is `return`/`athrow` is NOT an
    //   open exit: the natural-loop node set is built by walking
    //   PREDECESSORS from the back edge, so early-return blocks inside
    //   the loop body are excluded from `nodes` and their incoming edges
    //   looked like exits with a second target. The builder emits such
    //   blocks inline at the edge (the walk emits the return and that
    //   path ends) — no post-loop region needed.
    // * What remains must be ONE open exit target; a genuine multi-target
    //   loop (Java labeled `break` to two different labels) still refuses
    //   as MultiExitLoop.
    // * The old "exit block has no outside predecessors" purity check is
    //   GONE: the builder routes a shared exit through the enclosing
    //   stop (the JaggedNew continue-target shape) or falls back loudly
    //   (BranchIntoEmittedCode) on shapes it cannot structure — the
    //   conservative direction is unchanged, nothing wrong can be
    //   emitted (cfg_equiv is the proof obligation).
    let preds = cfg.preds();
    let mut back_edges: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(from, header) in &retreating {
        back_edges.entry(header).or_default().push(from);
    }
    // Pass 1: loop node sets (headers known afterwards — the trampoline
    // map needs them: chains never walk through a loop top).
    let mut bodies: Vec<(usize, HashSet<usize>)> = Vec::new();
    for (header, sources) in back_edges {
        let mut nodes: HashSet<usize> = HashSet::new();
        nodes.insert(header);
        for from in &sources {
            let mut work = vec![*from];
            while let Some(x) = work.pop() {
                if nodes.insert(x) {
                    for &p in &preds[x] {
                        work.push(p);
                    }
                }
            }
        }
        bodies.push((header, nodes));
    }
    let tramp = trampoline_map(
        cfg,
        m,
        &bodies
            .iter()
            .map(|(h, ns)| Loop {
                header: *h,
                exit: None,
                nodes: ns.clone(),
            })
            .collect::<Vec<_>>(),
    );
    // Pass 2: exit classification per loop.
    let mut loops: Vec<Loop> = Vec::new();
    for (header, nodes) in bodies {
        let mut open_exits: Vec<usize> = Vec::new();
        let node_list: Vec<usize> = nodes.iter().copied().collect();
        for &node in &node_list {
            for &s in &succs[node] {
                // Resolve goto trampolines first: javac routes `break`
                // through single-goto blocks, so two edges can reach the
                // same continuation via different blocks (confluence).
                let mut t = s;
                while let Some(&n) = tramp.get(&t) {
                    t = n;
                }
                if nodes.contains(&t) || open_exits.contains(&t) {
                    continue;
                }
                let last = cfg.blocks[t].end - 1;
                if matches!(&m.instrs[last].1, Instr::Return(_) | Instr::AThrow) {
                    continue; // emitted inline at the edge — not an open exit
                }
                open_exits.push(t);
            }
        }
        let exit = match open_exits.len() {
            0 => None,
            1 => Some((header, open_exits[0])),
            _ => return Err(Unsupported::MultiExitLoop),
        };
        loops.push(Loop {
            header,
            exit,
            nodes,
        });
    }
    Ok((loops, tramp))
}

// ---------------------------------------------------------------- emission

type Stops = Vec<(usize, String)>;

/// The structured-body builder. `emit_fn(idx, out)` emits instruction
/// `idx`'s code exactly as the dispatch emitter would (the caller never
/// passes branch/switch instructions here — the builder handles them).
struct Builder<'a> {
    m: &'a MethodPlan,
    cfg: &'a Cfg,
    loops: Vec<Loop>,
    /// Pure-goto trampoline blocks → resolved target. Every stop lookup
    /// keys on `resolve(block)` so an edge through a javac goto-chain
    /// finds the stop pushed for the chain's end (and vice versa).
    tramp: HashMap<usize, usize>,
    emit: EmitFn<'a>,
    emitted: HashSet<usize>,
    /// Active loop headers (innermost last): (block, label) — `continue`.
    active_loops: Vec<(usize, String)>,
    /// Set by `emit_loop` around its body-walk `region` call: the entry
    /// block is being EMITTED, not transferred to. `region` clears the
    /// flag for its own recursion so only the immediate body-walk entry
    /// is exempt from the stop check.
    body_walk: bool,
    out: Vec<String>,
    label_counter: usize,
}

type Res<T> = Result<T, Unsupported>;

/// The per-instruction emitter callback handed to [`Builder`] (the dispatch
/// emitter's `emit_instr`, boxed). Factored out for clippy type_complexity.
type EmitFn<'a> = Box<dyn FnMut(usize, &mut Vec<String>) + 'a>;

impl<'a> Builder<'a> {
    fn new(
        m: &'a MethodPlan,
        cfg: &'a Cfg,
        loops: Vec<Loop>,
        tramp: HashMap<usize, usize>,
        emit: EmitFn<'a>,
    ) -> Builder<'a> {
        Builder {
            m,
            cfg,
            loops,
            tramp,
            emit,
            emitted: HashSet::new(),
            active_loops: Vec::new(),
            body_walk: false,
            out: Vec::new(),
            label_counter: 0,
        }
    }

    /// The stop-key for a block: follow pure-goto trampolines to the
    /// chain's end. Loop headers are never trampolines (excluded when the
    /// map is built), so `continue` routing is untouched.
    fn resolve(&self, b: usize) -> usize {
        let mut cur = b;
        while let Some(&n) = self.tramp.get(&cur) {
            cur = n;
        }
        cur
    }

    fn fresh_label(&mut self, kind: &str, b: usize) -> String {
        self.label_counter += 1;
        format!("'{}_{}_{}", kind, b, self.label_counter)
    }

    fn loop_at(&self, b: usize) -> bool {
        self.loops.iter().any(|l| l.header == b)
    }

    fn is_join(&self, b: usize) -> bool {
        self.cfg.preds()[b].len() >= 2
    }

    fn block_of(&self, instr_idx: usize) -> usize {
        self.cfg.block_of(instr_idx)
    }

    fn instr_index(&self, pc: u32) -> usize {
        self.m
            .instrs
            .iter()
            .position(|(p, _)| *p == pc)
            .expect("branch target is an instruction boundary")
    }

    /// Emits the transfer to a reached stop target: `continue` into the
    /// active loop's own label, or `break` to the stop's label.
    fn stop_transfer(&mut self, cur: usize, stops: &Stops) -> Res<()> {
        let cur = self.resolve(cur);
        // The ACTIVE-LOOP check runs BEFORE the stop lookup and uses the
        // loop's OWN label: a block can be both an active header and a
        // stale stop entry (a join meet pushed while the header was still
        // fresh, whose loop is now emitted inside the join's arm) — such
        // an edge must `continue` the loop label, because
        // `continue <block-label>` is illegal Rust (cfg_equiv caught it
        // on BigJson$Parser). Only non-header targets take the stop's
        // label as `break`, legal for both loop and block labels.
        for (h, l) in self.active_loops.iter().rev() {
            if *h == cur {
                let l = l.clone();
                self.out.push(format!("continue {l};"));
                return Ok(());
            }
        }
        let (_, l) = stops
            .iter()
            .find(|(b, _)| *b == cur)
            .ok_or(Unsupported::ImpureJoin)?;
        self.out.push(format!("break {l};"));
        Ok(())
    }

    /// Transfer of control to block `tb`: an active-loop continue, a stop
    /// transfer, or a fresh region. `tb` is resolved through goto
    /// trampolines first — the destination is the chain's end. The
    /// active-loop check precedes the stop lookup (see stop_transfer).
    fn jump_to_block(&mut self, tb: usize, stops: &Stops) -> Res<()> {
        let tb = self.resolve(tb);
        for (h, l) in self.active_loops.iter().rev() {
            if *h == tb {
                let l = l.clone();
                self.out.push(format!("continue {l};"));
                return Ok(());
            }
        }
        if stops.iter().any(|(b, _)| *b == tb) {
            return self.stop_transfer(tb, stops);
        }
        if self.emitted.contains(&tb) {
            return Err(Unsupported::BranchIntoEmittedCode);
        }
        if self.loop_at(tb) {
            return self.emit_loop(tb, stops);
        }
        self.region(tb, stops)
    }

    /// Emits a loop rooted at header block `hb`, then its exit region.
    /// Every edge from the body to the exit block becomes `break 'loop;`,
    /// so the exit code is emitted exactly once, after the loop.
    ///
    /// Two lift-closure refinements (2026-11-07): when the loop has NO
    /// open exit (every exit target terminates — early returns emitted
    /// inline), no exit stop is pushed and nothing follows the loop. When
    /// the open exit block is ALREADY a stop of the enclosing walk (the
    /// shared-exit shape: an inner loop's exit is the enclosing loop's
    /// continue-target, or a diamond's join), the exit edges break the
    /// ENCLOSING label and the exit region is emitted by whoever owns
    /// that stop — emitting it here too would duplicate the block (the
    /// outside predecessor would then hit BranchIntoEmittedCode).
    fn emit_loop(&mut self, hb: usize, stops: &Stops) -> Res<()> {
        let exit_block = self
            .loops
            .iter()
            .find(|l| l.header == hb)
            .ok_or(Unsupported::Irreducible)?
            .exit
            .map(|(_, t)| t);
        if exit_block
            .map(|x| self.emitted.contains(&x))
            .unwrap_or(false)
        {
            return Err(Unsupported::BranchIntoEmittedCode);
        }
        // Delegated exit: the enclosing walk already owns this block.
        let delegated = matches!(exit_block, Some(x) if stops.iter().any(|(s, _)| *s == x));
        let lbl = self.fresh_label("loop", hb);
        self.out.push(format!("{lbl}: loop {{"));
        self.active_loops.push((hb, lbl.clone()));
        let mut body_stops: Stops = stops.to_vec();
        if let (false, Some(x)) = (delegated, exit_block) {
            body_stops.push((x, lbl.clone()));
        }
        // Mark the body walk so `region` treats its active-header entry as
        // emission (see the field doc); restore afterwards — nested loops
        // each mark their own walk.
        let saved_body_walk = self.body_walk;
        self.body_walk = true;
        let body = self.region(hb, &body_stops);
        self.body_walk = saved_body_walk;
        body?;
        self.active_loops.pop();
        self.out.push("}".to_string());
        if delegated {
            return Ok(()); // the enclosing stop's owner emits the exit region
        }
        match exit_block {
            None => Ok(()), // every exit terminated inline; the loop diverges
            Some(x) => {
                if self.emitted.contains(&x) {
                    return Err(Unsupported::BranchIntoEmittedCode);
                }
                self.region(x, stops)
            }
        }
    }

    /// Emits blocks from `b` along the fall-through/goto chain, stopping at
    /// the first conditional (a diamond), terminator, stop target, loop
    /// header or already-emitted block.
    fn region(&mut self, b: usize, stops: &Stops) -> Res<()> {
        // Only the IMMEDIATE emit_loop body walk treats an active-header
        // entry as emission (a stale stop entry for it — the block doubling
        // as an enclosing join's meet or an enclosing loop's exit — must
        // not hijack the body into `continue <its own label>`, a tight
        // infinite loop; cfg_equiv caught it on BigRpg.mergeSort). Any
        // OTHER region call with a header entry is a TRANSFER to that
        // header (a diamond arm reaching an active-header meet) and the
        // stop check must fire.
        let entry_is_body_walk = std::mem::take(&mut self.body_walk);
        // ENTRY stop check comes BEFORE loop construction: a fresh header
        // that is also a pending stop (a diamond's meet, an enclosing
        // loop's exit target) is a TRANSFER target — the arms break to it
        // and the loop construct is emitted by the stop's owner at the
        // join's meet position. Emitting it inline here instead left the
        // join's own trailing region(meet) hitting an emitted block.
        if !entry_is_body_walk && stops.iter().any(|(s, _)| *s == self.resolve(b)) {
            return self.stop_transfer(b, stops);
        }
        // a fresh loop header is a loop construct, not a plain block
        if self.loop_at(b) && !self.active_loops.iter().any(|(h, _)| *h == b) {
            return self.emit_loop(b, stops);
        }
        let mut cur = b;
        loop {
            if cur != b && stops.iter().any(|(s, _)| *s == self.resolve(cur)) {
                self.stop_transfer(cur, stops)?;
                return Ok(());
            }
            // Fall-through into a loop header — only for blocks REACHED by
            // the walk (not the region's own entry block, which is either
            // the header itself being emitted for emit_loop, or a plain
            // first block). A FRESH header must become its loop construct
            // here: javac peels the first-iteration test to the top, so
            // the pre-loop block FALLS THROUGH into the guard (the header
            // is not reached by a jump) — emitting the header inline
            // instead turned its guard into a diamond whose meet is the
            // header itself, and the join-purity check then saw the
            // entry-side predecessor as a third edge (ImpureJoin) — every
            // javac `for`/`while` loop fell back, with the idom bug
            // masking the builder gap. An ACTIVE header reached by
            // fall-through is a back edge without a goto (the do-while
            // cond shape) → `continue`.
            if cur != b && self.loop_at(cur) {
                if let Some((_, l)) = self.active_loops.iter().find(|(h, _)| *h == cur).cloned() {
                    self.out.push(format!("continue {l};"));
                    return Ok(());
                }
                return self.emit_loop(cur, stops);
            }
            if self.emitted.contains(&cur) {
                return Err(Unsupported::BranchIntoEmittedCode);
            }
            self.emitted.insert(cur);
            let span_start = self.cfg.blocks[cur].start;
            let span_end = self.cfg.blocks[cur].end;
            let last = span_end - 1;
            for idx in span_start..last {
                let mut tmp: Vec<String> = Vec::new();
                (self.emit)(idx, &mut tmp);
                self.out.extend(tmp);
            }
            let term = &self.m.instrs[last].1;
            match term {
                Instr::Return(_) => {
                    let mut tmp: Vec<String> = Vec::new();
                    (self.emit)(last, &mut tmp);
                    self.out.extend(tmp);
                    return Ok(());
                }
                Instr::AThrow => {
                    let mut tmp: Vec<String> = Vec::new();
                    (self.emit)(last, &mut tmp);
                    self.out.extend(tmp);
                    // rethrow reaches the loop top → no handler → Err(t)
                    self.out.push("continue 'run;".to_string());
                    return Ok(());
                }
                Instr::Goto(t) => {
                    let ti = self.instr_index(*t);
                    let tb = self.block_of(ti);
                    self.jump_to_block(tb, stops)?;
                    return Ok(());
                }
                Instr::IfZero(_, _)
                | Instr::IfICmp(_, _)
                | Instr::IfNull(_)
                | Instr::IfNonNull(_)
                | Instr::IfRefCmp { .. } => {
                    self.diamond(last, stops)?;
                    return Ok(());
                }
                Instr::TableSwitch { .. } | Instr::LookupSwitch { .. } => {
                    return Err(Unsupported::HasSwitch);
                }
                _ => {
                    if last + 1 >= self.m.instrs.len() {
                        return Err(Unsupported::FallOffEnd);
                    }
                    // The block ended at a leader boundary, not at a
                    // terminator: `last` is ordinary code and must be
                    // emitted before falling through (the dispatch emitter
                    // emits it; dropping it silently corrupts the operand
                    // stack — caught by the output-equivalence test).
                    let mut tmp: Vec<String> = Vec::new();
                    (self.emit)(last, &mut tmp);
                    self.out.extend(tmp);
                    cur = self.block_of(last + 1);
                }
            }
        }
    }

    /// Walks from block `b` along single-predecessor chains to find the
    /// meet (first join / loop header / divergence point). Returns the meet
    /// block (or `None` when the walk terminates) plus every block visited.
    fn walk_meet(&self, start: usize) -> (Option<usize>, HashSet<usize>) {
        let mut visited: HashSet<usize> = HashSet::new();
        // Record the resolved-from trampoline chain FIRST: the chain's
        // edges belong to this arm (they will route through this arm's
        // stop label), so the join-purity check must see them as arm
        // blocks. The resolved end itself is inserted by the walk below.
        let end = self.resolve(start);
        let mut s = start;
        while s != end {
            visited.insert(s);
            match self.tramp.get(&s) {
                Some(&n) => s = n,
                None => break,
            }
        }
        let mut cur = Some(end);
        loop {
            let Some(b) = cur else { return (None, visited) };
            if !visited.insert(b) {
                return (None, visited); // cycle through non-joins: bail
            }
            if self.loop_at(b) {
                // An ACTIVE (or already-emitted) header is a genuine meet:
                // the enclosing loop's top, re-entered by this arm. A FRESH
                // header is not a meet — the arm will emit the WHOLE loop,
                // and control continues at the loop's open exit: walk
                // through (covering the loop's nodes so the join-purity
                // check accepts its internal predecessors of the exit).
                // This is the shared-exit sibling shape (JaggedNew: the
                // outer `continue` target doubles as the inner loop's
                // exit) — without the walk-through, the two arms' walks
                // diverged (header vs join) and every such method fell
                // back with ImpureJoin/ImpureLoopExit.
                let active = self.active_loops.iter().any(|(h, _)| *h == b);
                if !active && !self.emitted.contains(&b) {
                    if let Some(l) = self.loops.iter().find(|l| l.header == b) {
                        match l.exit {
                            Some((_, exit_t)) if !l.nodes.contains(&exit_t) => {
                                visited.insert(b);
                                for &n in &l.nodes {
                                    visited.insert(n);
                                }
                                cur = Some(exit_t);
                                continue;
                            }
                            _ => return (Some(b), visited),
                        }
                    }
                }
                return (Some(b), visited);
            }
            if self.is_join(b) {
                return (Some(b), visited);
            }
            let last = self.cfg.blocks[b].end - 1;
            match &self.m.instrs[last].1 {
                Instr::Return(_) | Instr::AThrow => return (None, visited),
                Instr::Goto(t) => {
                    let ti = self.instr_index(*t);
                    cur = Some(self.resolve(self.block_of(ti)));
                }
                Instr::IfZero(_, t)
                | Instr::IfICmp(_, t)
                | Instr::IfNull(t)
                | Instr::IfNonNull(t)
                | Instr::IfRefCmp { target: t, .. } => {
                    // nested diamond: both of ITS arms must reach the same
                    // meet, which is this chain's meet. Both arms are
                    // *instruction* positions and must be mapped to blocks
                    // before recursing (walk_meet works on block indices).
                    if last + 1 >= self.m.instrs.len() {
                        return (None, visited);
                    }
                    let ti = self.instr_index(*t);
                    let (m1, v1) = self.walk_meet(self.block_of(ti));
                    let (m2, v2) = self.walk_meet(self.block_of(last + 1));
                    visited.extend(v1);
                    visited.extend(v2);
                    return match (m1, m2) {
                        (Some(a), Some(c)) if a == c => (Some(a), visited),
                        _ => (None, visited),
                    };
                }
                Instr::TableSwitch { .. } | Instr::LookupSwitch { .. } => {
                    return (None, visited); // analysed() already bailed
                }
                _ => {
                    if last + 1 >= self.m.instrs.len() {
                        return (None, visited);
                    }
                    cur = Some(self.resolve(self.block_of(last + 1)));
                }
            }
        }
    }

    /// Emits an if/else diamond rooted at conditional instruction `ci`.
    /// Both arms are walked to their common meet; the meet is emitted once
    /// behind a labeled block (or the diamond is plain if/else when both
    /// arms terminate). A meet that is the enclosing loop's header turns
    /// the arms into `continue`s.
    fn diamond(&mut self, ci: usize, stops: &Stops) -> Res<()> {
        let (t_pc, f_pc) = match &self.m.instrs[ci].1 {
            Instr::IfZero(_, t) | Instr::IfICmp(_, t) | Instr::IfNull(t) | Instr::IfNonNull(t) => {
                (*t, self.m.instrs[ci + 1].0)
            }
            Instr::IfRefCmp { target, .. } => (*target, self.m.instrs[ci + 1].0),
            other => unreachable!("diamond on {other:?}"),
        };
        self.out
            .extend(cond_code(&self.m.instrs[ci].1, &self.m.instrs[ci].0));

        let t_b = self.block_of(self.instr_index(t_pc));
        let f_b = self.block_of(self.instr_index(f_pc));
        // the block containing the conditional — one legitimate predecessor
        // of the meet (an *empty arm*: the branch target IS the join)
        let root = self.block_of(ci);

        // arm walks (pure: they inspect, they do not emit)
        let (m1, v1) = self.walk_meet(t_b);
        let (m2, v2) = self.walk_meet(f_b);
        let mut arm_blocks: HashSet<usize> = v1;
        arm_blocks.extend(v2);
        arm_blocks.insert(t_b);
        arm_blocks.insert(f_b);

        // A meet that is the enclosing loop's header: the arms transfer
        // back to the loop top (`continue`); a terminating arm simply ends
        // inside its branch. The outer stops stay in force.
        let active_meet = |b: Option<usize>| -> Option<usize> {
            b.filter(|m| self.active_loops.iter().any(|(h, _)| *h == *m))
        };
        if let Some(meet) = active_meet(m1).or(active_meet(m2)) {
            let (_, l) = self
                .active_loops
                .iter()
                .rev()
                .find(|(h, _)| *h == meet)
                .unwrap()
                .clone();
            let mut arm_stops: Stops = stops.to_vec();
            arm_stops.push((meet, l));
            self.out.push("if __cond {".into());
            self.region(t_b, &arm_stops)?;
            self.out.push("} else {".into());
            self.region(f_b, &arm_stops)?;
            self.out.push("}".into());
            return Ok(());
        }

        // The meet is ALREADY a stop target: an enclosing structure owns
        // it (an outer diamond's labeled join, the loop exit). The arms
        // walk to it and break with the OWNER's label; re-joining here
        // would emit the meet twice (cfg-debug census: tred2's nested
        // diamond 16 meeting at 18, diamond 6's join). Free-walk if/else
        // with the CURRENT stops — no join_via_label, no trailing region:
        // the owner emits the meet after its labeled block.
        let meet_already_stopped = match (m1, m2) {
            (Some(m), Some(n)) if m == n => {
                stops.iter().any(|(s, _)| *s == m)
            }
            (Some(m), None) | (None, Some(m)) => stops.iter().any(|(s, _)| *s == m),
            _ => false,
        };
        if meet_already_stopped {
            self.out.push("if __cond {".into());
            self.region(t_b, stops)?;
            self.out.push("} else {".into());
            self.region(f_b, stops)?;
            self.out.push("}".into());
            return Ok(());
        }

        match (m1, m2) {
            (Some(meet), Some(_)) if meet == m2.unwrap() => {
                self.join_via_label(meet, t_b, f_b, root, &arm_blocks, stops)?;
                self.region(meet, stops)
            }
            (Some(m1), Some(m2)) if m1 != m2 => {
                // Divergent joins (readiness census: tql2/tred2/orthes
                // loop-header diamonds). At least one arm is a TRANSFER
                // arm: it starts at, or its walk meets, a block that is
                // already a stop target (the enclosing loop's exit or an
                // enclosing join) — its region ends in that stop's own
                // `break`/`continue`, never falls through. Lower as a
                // free-walk if/else: each arm's region consumes its own
                // side (nested diamonds own their meets). An arm walk that
                // is NOT a transfer must not fall past its meet — the
                // m1==m2 shared-meet shape is the join_via_label case
                // above; a walk that runs into emitted code or off the
                // end fails LOUDLY (BranchIntoEmittedCode / FallOffEnd),
                // never silently. Without a transfer arm the diamond is
                // genuinely divergent (two live meets the outer walk
                // still has to place) — refused as before.
                let t_is_transfer =
                    stops.iter().any(|(s, _)| *s == t_b || *s == m1);
                let f_is_transfer =
                    stops.iter().any(|(s, _)| *s == f_b || *s == m2);
                if !(t_is_transfer || f_is_transfer) {
                    if std::env::var_os("J2R_CFG_DEBUG").is_some() {
                        eprintln!(
                            "cfg-debug: divergent joins unresolved {}{} ci={} (block {}) \
                             t_b={} f_b={} m1={:?} m2={:?}",
                            self.m.name, self.m.desc, ci, root, t_b, f_b, m1, m2
                        );
                    }
                    return Err(Unsupported::ImpureJoin);
                }
                self.out.push("if __cond {".into());
                self.region(t_b, stops)?;
                self.out.push("} else {".into());
                self.region(f_b, stops)?;
                self.out.push("}".into());
                Ok(())
            }
            (Some(meet), None) | (None, Some(meet)) => {
                self.join_via_label(meet, t_b, f_b, root, &arm_blocks, stops)?;
                self.region(meet, stops)
            }
            (None, None) => {
                // both arms terminate — plain if/else; the surrounding
                // stop context carries into the arms
                self.out.push("if __cond {".into());
                self.region(t_b, stops)?;
                self.out.push("} else {".into());
                self.region(f_b, stops)?;
                self.out.push("}".into());
                Ok(())
            }
            _ => unreachable!("diamond: both-equal meet handled above"),
        }
    }

    /// Emits a labeled join block around the two diamond arms; the meet is
    /// emitted once by the caller afterwards.
    fn join_via_label(
        &mut self,
        meet: usize,
        t_b: usize,
        f_b: usize,
        root: usize,
        arm_blocks: &HashSet<usize>,
        stops: &Stops,
    ) -> Res<()> {
        // meet purity: every predecessor of the meet must lie inside the
        // two arm walks, or be the diamond's own root — the root's branch
        // edge into the meet is exactly the empty-arm shape
        // (`if (c) {} else { ... }`), where the branch target IS the join.
        // Any third edge from outside the diamond falls back — EXCEPT the
        // meet's own loop back edges when the meet is a (fresh) loop
        // header: a nested loop's header is met by the enclosing diamond's
        // arm AND re-entered by its body's back edge, which the loop
        // emission itself lowers to `continue` (the join label never sees
        // that edge). Found by the readiness census: every javac
        // nested-`for` numeric kernel refused here.
        let preds = self.cfg.preds();
        let meet_loop_nodes =
            if self.loop_at(meet) && !self.active_loops.iter().any(|(h, _)| *h == meet) {
                self.loops
                    .iter()
                    .find(|l| l.header == meet)
                    .map(|l| l.nodes.clone())
            } else {
                None
            };
        for &p in &preds[meet] {
            if !arm_blocks.contains(&p) && p != root {
                match &meet_loop_nodes {
                    Some(nodes) if nodes.contains(&p) => {}
                    _ => {
                        if std::env::var_os("J2R_CFG_DEBUG").is_some() {
                            eprintln!(
                                "cfg-debug: join purity refused {}{} meet={} root={} \
                                 outside_pred={} meet_loop={:?}",
                                self.m.name,
                                self.m.desc,
                                meet,
                                root,
                                p,
                                meet_loop_nodes.as_ref().map(|n| n.len())
                            );
                        }
                        return Err(Unsupported::ImpureJoin);
                    }
                }
            }
        }
        if self.emitted.contains(&meet) {
            return Err(Unsupported::BranchIntoEmittedCode);
        }
        let lbl = self.fresh_label("join", meet);
        self.out.push(format!("{lbl}: {{"));
        self.out.push("if __cond {".into());
        let mut arm_stops: Stops = stops.to_vec();
        arm_stops.push((meet, lbl.clone()));
        self.region(t_b, &arm_stops)?;
        self.out.push("} else {".into());
        self.region(f_b, &arm_stops)?;
        self.out.push("}".into());
        self.out.push(format!("break {lbl};"));
        self.out.push("}".to_string());
        Ok(())
    }
}

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
            let term = b.end.checked_sub(1).and_then(|j| m.instrs.get(j)).map(
                |(_, ins)| match ins {
                    Instr::Goto(_) => "goto".to_string(),
                    Instr::IfZero(_, _)
                    | Instr::IfICmp(_, _)
                    | Instr::IfNull(_)
                    | Instr::IfNonNull(_)
                    | Instr::IfRefCmp { .. } => "if".to_string(),
                    Instr::Return(_) => "ret".to_string(),
                    Instr::AThrow => "athrow".to_string(),
                    _ => "fall".to_string(),
                },
            );
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
    b.region(entry, &Vec::new())?;
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
            is_sync: false,
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
        // *block* index was required — this method (9 instructions, 7
        // blocks) panicked with "index out of bounds: len 7, index 9"
        // before the fix. The shape itself (a nested diamond joining
        // before the outer diamond joins) is refused conservatively —
        // refusal, never a panic, is the designed behaviour (M2_PLAN §6.1:
        // anything unclear falls back to the dispatch body).
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
        assert_eq!(lift(&m, |_, _| {}), Err(Unsupported::ImpureJoin));
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
