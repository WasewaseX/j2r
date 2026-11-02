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
//! * single-exit natural loops whose exit block has no outside predecessors;
//! * if/else diamonds whose join block has exactly the two arm-end
//!   predecessors;
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

    fn idom(&self) -> Vec<Option<usize>> {
        let n = self.blocks.len();
        let preds = self.preds();
        let mut idoms: Vec<Option<usize>> = vec![None; n];
        idoms[self.entry] = Some(self.entry);
        let mut changed = true;
        while changed {
            changed = false;
            for i in 0..n {
                if i == self.entry {
                    continue;
                }
                let mut new: Option<usize> = None;
                for &p in &preds[i] {
                    if let Some(pd) = idoms[p] {
                        new = Some(match new {
                            None => pd,
                            Some(cur) => intersect(&idoms, cur, pd),
                        });
                    }
                }
                if new.is_some() && new != idoms[i] {
                    idoms[i] = new;
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

fn intersect(idoms: &[Option<usize>], mut a: usize, mut b: usize) -> usize {
    while a != b {
        while a > b {
            a = idoms[a].unwrap_or(a);
        }
        while b > a {
            b = idoms[b].unwrap_or(b);
        }
    }
    a
}

// ------------------------------------------------------- reducibility + loops

#[derive(Debug)]
struct Loop {
    header: usize,
    /// The single exit edge (from-block, to-block).
    exit: (usize, usize),
}

/// Analyse the CFG; `Err(reason)` means the method falls back to dispatch.
fn analyse(cfg: &Cfg, m: &MethodPlan) -> Result<Vec<Loop>, Unsupported> {
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

    // Natural loops (unioned across all back edges of a header); single
    // exit per loop, pure exit block.
    let preds = cfg.preds();
    let mut back_edges: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(from, header) in &retreating {
        back_edges.entry(header).or_default().push(from);
    }
    let mut loops: Vec<Loop> = Vec::new();
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
        let mut exit: Option<(usize, usize)> = None;
        let node_list: Vec<usize> = nodes.iter().copied().collect();
        for &node in &node_list {
            for &s in &succs[node] {
                if !nodes.contains(&s) {
                    match exit {
                        None => exit = Some((node, s)),
                        Some(e) if e == (node, s) => {}
                        Some(_) => return Err(Unsupported::MultiExitLoop),
                    }
                }
            }
        }
        let e = exit.ok_or(Unsupported::MultiExitLoop)?;
        // purity: the exit block's predecessors must all be loop nodes
        for &p in &preds[e.1] {
            if !nodes.contains(&p) {
                return Err(Unsupported::ImpureLoopExit);
            }
        }
        loops.push(Loop { header, exit: e });
    }
    Ok(loops)
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
    emit: EmitFn<'a>,
    emitted: HashSet<usize>,
    /// Active loop headers (innermost last): (block, label) — `continue`.
    active_loops: Vec<(usize, String)>,
    out: Vec<String>,
    label_counter: usize,
}

type Res<T> = Result<T, Unsupported>;

/// The per-instruction emitter callback handed to [`Builder`] (the dispatch
/// emitter's `emit_instr`, boxed). Factored out for clippy type_complexity.
type EmitFn<'a> = Box<dyn FnMut(usize, &mut Vec<String>) + 'a>;

impl<'a> Builder<'a> {
    fn new(m: &'a MethodPlan, cfg: &'a Cfg, loops: Vec<Loop>, emit: EmitFn<'a>) -> Builder<'a> {
        Builder {
            m,
            cfg,
            loops,
            emit,
            emitted: HashSet::new(),
            active_loops: Vec::new(),
            out: Vec::new(),
            label_counter: 0,
        }
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

    /// Emits the transfer to a reached stop target: `continue` when the
    /// stop is an active loop header, `break` when it is a join.
    fn stop_transfer(&mut self, cur: usize, stops: &Stops) -> Res<()> {
        let (_, l) = stops
            .iter()
            .find(|(b, _)| *b == cur)
            .ok_or(Unsupported::ImpureJoin)?;
        if self.active_loops.iter().any(|(h, _)| *h == cur) {
            self.out.push(format!("continue {l};"));
        } else {
            self.out.push(format!("break {l};"));
        }
        Ok(())
    }

    /// Transfer of control to block `tb`: a stop transfer, a continue into
    /// an active loop, or a fresh region.
    fn jump_to_block(&mut self, tb: usize, stops: &Stops) -> Res<()> {
        if stops.iter().any(|(b, _)| *b == tb) {
            return self.stop_transfer(tb, stops);
        }
        for (h, l) in self.active_loops.iter().rev() {
            if *h == tb {
                let l = l.clone();
                self.out.push(format!("continue {l};"));
                return Ok(());
            }
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
    fn emit_loop(&mut self, hb: usize, stops: &Stops) -> Res<()> {
        let exit_block = self
            .loops
            .iter()
            .find(|l| l.header == hb)
            .ok_or(Unsupported::Irreducible)?
            .exit
            .1;
        if self.emitted.contains(&exit_block) {
            return Err(Unsupported::BranchIntoEmittedCode);
        }
        let lbl = self.fresh_label("loop", hb);
        self.out.push(format!("{lbl}: loop {{"));
        self.active_loops.push((hb, lbl.clone()));
        let mut body_stops: Stops = stops.to_vec();
        body_stops.push((exit_block, lbl.clone()));
        self.region(hb, &body_stops)?;
        self.active_loops.pop();
        self.out.push("}".to_string());
        if self.emitted.contains(&exit_block) {
            return Err(Unsupported::BranchIntoEmittedCode);
        }
        self.region(exit_block, stops)
    }

    /// Emits blocks from `b` along the fall-through/goto chain, stopping at
    /// the first conditional (a diamond), terminator, stop target, loop
    /// header or already-emitted block.
    fn region(&mut self, b: usize, stops: &Stops) -> Res<()> {
        // a fresh loop header is a loop construct, not a plain block
        if self.loop_at(b) && !self.active_loops.iter().any(|(h, _)| *h == b) {
            return self.emit_loop(b, stops);
        }
        let mut cur = b;
        loop {
            if stops.iter().any(|(s, _)| *s == cur) {
                self.stop_transfer(cur, stops)?;
                return Ok(());
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
                    if stops.iter().any(|(s, _)| *s == tb) {
                        self.stop_transfer(tb, stops)?;
                        return Ok(());
                    }
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
        let mut cur = Some(start);
        loop {
            let Some(b) = cur else { return (None, visited) };
            if !visited.insert(b) {
                return (None, visited); // cycle through non-joins: bail
            }
            if self.is_join(b) || self.loop_at(b) {
                return (Some(b), visited);
            }
            let last = self.cfg.blocks[b].end - 1;
            match &self.m.instrs[last].1 {
                Instr::Return(_) | Instr::AThrow => return (None, visited),
                Instr::Goto(t) => {
                    let ti = self.instr_index(*t);
                    cur = Some(self.block_of(ti));
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
                    cur = Some(self.block_of(last + 1));
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

        match (m1, m2) {
            (Some(meet), Some(_)) if meet == m2.unwrap() => {
                self.join_via_label(meet, t_b, f_b, root, &arm_blocks, stops)?;
                self.region(meet, stops)
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
            _ => Err(Unsupported::ImpureJoin),
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
        // Any third edge from outside the diamond falls back.
        let preds = self.cfg.preds();
        for &p in &preds[meet] {
            if !arm_blocks.contains(&p) && p != root {
                return Err(Unsupported::ImpureJoin);
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
    let loops = analyse(&cfg, m)?;
    let mut b = Builder::new(m, &cfg, loops, Box::new(emit_fn));
    let entry = cfg.entry;
    b.region(entry, &Vec::new())?;
    if b.emitted.len() != cfg.blocks.len() {
        return Err(Unsupported::BranchIntoEmittedCode);
    }
    Ok(b.out.join("\n"))
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
        // Natural loop {0,4,8}, single exit → 16.
        let m = method(vec![
            (0, Instr::IfZero(crate::ir::CmpOp::Ne, 16)),
            (4, Instr::IfZero(crate::ir::CmpOp::Eq, 0)),
            (8, Instr::Goto(0)),
            (16, Instr::Return(TypeDesc::Void)),
        ]);
        let out = lift(&m, |_, _| {}).unwrap();
        assert!(out.contains(": loop {"), "{out}");
        assert!(out.contains("continue 'loop_"), "{out}");
        assert!(out.contains("break 'loop_"), "{out}");
    }

    #[test]
    fn irreducible_falls_back() {
        let m = method(vec![
            (0, Instr::Goto(8)),
            (4, Instr::Goto(12)),
            (8, Instr::Goto(4)),
            (12, Instr::Goto(8)),
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
    fn join_with_third_predecessor_falls_back() {
        let m = method(vec![
            (0, Instr::IfZero(crate::ir::CmpOp::Eq, 12)),
            (4, Instr::IfZero(crate::ir::CmpOp::Eq, 20)),
            (8, Instr::Goto(20)),
            (12, Instr::Nop),
            (13, Instr::Goto(20)),
            (20, Instr::Return(TypeDesc::Void)),
        ]);
        let r = lift(&m, |_, _| {});
        assert!(matches!(r, Err(Unsupported::ImpureJoin)));
    }

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
