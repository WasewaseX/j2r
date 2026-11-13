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

type Stops = Vec<StopEntry>;

/// One entry on the builder-owned stop stack. `sid` is a globally unique
/// frame id (monotonic counter) — frames pop BY SID, never by position or
/// by label: a positional `truncate` goes stale when an inner frame's pop
/// removes entries BELOW the mark (the retain-by-label pops shift the
/// stack — transpose's exit stop survived its own truncate and the block
/// was never emitted), and a label-retain catches OTHER entries sharing
/// the label (an active-meet diamond's frame label is the enclosing
/// loop's label, which is also the loop's exit-stop entry label — the
/// retain killed the exit stop mid-body).
struct StopEntry {
    block: usize,
    label: String,
    sid: u64,
}

/// A join meet registered as a DEFERRED join (the G3.1 mechanism, the
/// named `ImpureJoin` lowering): its (meet, label) sits on the
/// builder-owned stop stack until the enclosing drain point (the
/// emitting loop's body end, or the end of `lift`), its body then emits
/// ONCE after the label closes. Every edge to the meet — diamond arms
/// AND later outside predecessors — `break`s to the label while it is
/// open, so the meet needs no inline position in the walk.
struct DeferredJoin {
    meet: usize,
    label: String,
    sid: u64,
}

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
    /// The builder-OWNED stop stack (G3.1): the old call-threaded
    /// `&Stops` snapshots, promoted so a DEFERRED join registered during
    /// a diamond's arm walk stays visible to the later outside-predecessor
    /// walks in the same enclosing region. Frames push/pop symmetrically
    /// (pop-BY-SID, so deferred entries survive them); only deferred
    /// registrations push asymmetrically, and only a drain point consumes
    /// those.
    stops: Stops,
    /// Monotonic frame-id source: every stop push gets a globally unique
    /// sid; the pop retains by that sid (see StopEntry).
    stop_seq: u64,
    /// Registered-but-undrained deferred joins, in registration order
    /// (the LAST entry is the innermost open label and drains first).
    deferred: Vec<DeferredJoin>,
    /// Set by `emit_loop` around its body-walk `region` call: the entry
    /// block is being EMITTED, not transferred to. `region` clears the
    /// flag for its own recursion so only the immediate body-walk entry
    /// is exempt from the stop check.
    body_walk: bool,
    out: Vec<String>,
    label_counter: usize,
    /// cfg-trace brace tracker: the open-construct kinds in emission
    /// order. Debug-only (J2R_CFG_TRACE); every `}` must close the top
    /// opener and every `} else {` must close an `if`.
    brace_trace: Vec<&'static str>,
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
            stops: Vec::new(),
            stop_seq: 0,
            deferred: Vec::new(),
            body_walk: false,
            out: Vec::new(),
            label_counter: 0,
            brace_trace: Vec::new(),
        }
    }

    /// Debug-only brace accounting (J2R_CFG_TRACE). `push` is the raw
    /// out-line just appended; kind is the construct it opens/closes.
    fn btrace(&mut self, line: &str) {
        if std::env::var_os("J2R_CFG_TRACE").is_none() {
            return;
        }
        eprintln!(
            "cfg-trace: push out.len()={} depth={} kind={} line={}",
            self.out.len(),
            self.brace_trace.len(),
            self.brace_trace.last().unwrap_or(&""),
            line.trim_end()
        );
        let t = line.trim_end();
        if t.ends_with(": {") || t.ends_with(": loop {") || t == "loop {" {
            self.brace_trace.push("block");
        } else if t == "if __cond {" {
            self.brace_trace.push("if");
        } else if t == "} else {" {
            match self.brace_trace.pop() {
                Some("if") => {
                    self.brace_trace.push("if-else");
                }
                other => panic!(
                    "cfg-trace: brace bug: `}} else {{` closed {other:?} (top of stack, out.len()={})",
                    self.out.len()
                ),
            }
        } else if t == "}" && self.brace_trace.pop().is_none() {
            panic!(
                "cfg-trace: brace bug: `}}` with empty stack at out.len()={}",
                self.out.len()
            );
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

    /// Pushes a stop entry with a fresh globally-unique frame id and
    /// returns it — the pop site retains by this sid, never by position
    /// or label (see StopEntry).
    fn push_stop(&mut self, block: usize, label: String) -> u64 {
        let sid = self.stop_seq;
        self.stop_seq += 1;
        self.stops.push(StopEntry { block, label, sid });
        sid
    }

    /// cfg-debug diagnostic for a loud refusal site: names the block the
    /// walk refused on, so a census trace can point at the exact shape.
    fn dbg_refuse(&self, site: &str, b: usize) {
        if std::env::var("J2R_CFG_DEBUG").as_deref() == Ok("3") {
            eprintln!(
                "cfg-debug: ---- emitted text at refuse[{}] (block {}) ----",
                site, b
            );
            for (i, line) in self.out.iter().enumerate() {
                eprintln!("cfg-debug: {:>4}: {}", i, line);
            }
        }
        if std::env::var_os("J2R_CFG_DEBUG").is_some() {
            eprintln!(
                "cfg-debug: refuse[{}] {}{} block={} emitted={} stops={:?} active={:?}",
                site,
                self.m.name,
                self.m.desc,
                b,
                self.emitted.contains(&b),
                self.stops.iter().map(|e| e.block).collect::<Vec<_>>(),
                self.active_loops
                    .iter()
                    .map(|(x, _)| *x)
                    .collect::<Vec<_>>()
            );
        }
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
    fn stop_transfer(&mut self, cur: usize) -> Res<()> {
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
        let l = self
            .stops
            .iter()
            .find(|e| e.block == cur)
            .ok_or(Unsupported::ImpureJoin)?
            .label
            .clone();
        self.out.push(format!("break {l};"));
        Ok(())
    }

    /// Transfer of control to block `tb`: an active-loop continue, a stop
    /// transfer, or a fresh region. `tb` is resolved through goto
    /// trampolines first — the destination is the chain's end. The
    /// active-loop check precedes the stop lookup (see stop_transfer).
    fn jump_to_block(&mut self, tb: usize) -> Res<()> {
        let tb = self.resolve(tb);
        for (h, l) in self.active_loops.iter().rev() {
            if *h == tb {
                let l = l.clone();
                self.out.push(format!("continue {l};"));
                return Ok(());
            }
        }
        if self.stops.iter().any(|e| e.block == tb) {
            return self.stop_transfer(tb);
        }
        if self.emitted.contains(&tb) {
            self.dbg_refuse("jump_to_block", tb);
            return Err(Unsupported::BranchIntoEmittedCode);
        }
        if self.loop_at(tb) {
            return self.emit_loop(tb);
        }
        self.region(tb)
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
    fn emit_loop(&mut self, hb: usize) -> Res<()> {
        let exit_block = self
            .loops
            .iter()
            .find(|l| l.header == hb)
            .ok_or(Unsupported::Irreducible)?
            .exit
            .map(|(_, t)| t);
        // Delegated exit FIRST (the shared-exit shape): when the exit
        // block is a stop of the enclosing walk, its owner emits it —
        // and that owner may well be the ENCLOSING loop whose header is
        // legitimately already in `emitted` (hqr2: inner loop(72)'s exit
        // IS the active loop(15) header, i.e. the inner loop's exit edge
        // is a `continue` of the outer loop). Refusing on
        // exit_emitted before the delegated check rejected exactly that
        // sound shape; the emitted check below only fires for an exit
        // block that is emitted AND unowned.
        let delegated = matches!(exit_block, Some(x) if self.stops.iter().any(|e| e.block == x));
        if !delegated
            && exit_block
                .map(|x| self.emitted.contains(&x))
                .unwrap_or(false)
        {
            if std::env::var_os("J2R_CFG_DEBUG").is_some() {
                eprintln!(
                    "cfg-debug: refuse[emit_loop:exit_emitted-header] {}{} header={} hb_emitted={}",
                    self.m.name,
                    self.m.desc,
                    hb,
                    self.emitted.contains(&hb)
                );
            }
            self.dbg_refuse("emit_loop:exit_emitted", exit_block.unwrap());
            return Err(Unsupported::BranchIntoEmittedCode);
        }
        if std::env::var_os("J2R_CFG_DEBUG").is_some() {
            eprintln!(
                "cfg-debug: emit_loop {}{} header={} active={:?}",
                self.m.name,
                self.m.desc,
                hb,
                self.active_loops
                    .iter()
                    .map(|(x, _)| *x)
                    .collect::<Vec<_>>()
            );
        }
        let lbl = self.fresh_label("loop", hb);
        self.out.push(format!("{lbl}: loop {{"));
        let __t = self.out.last().unwrap().clone();
        self.btrace(&__t);
        self.active_loops.push((hb, lbl.clone()));
        // Frame mark: the exit-stop entry pushed for the body walk is
        // popped after the body (and its drain) — the exit region sees the
        // stops as of THIS call, matching the old snapshot semantics.
        let exit_sid = match exit_block {
            Some(x) if !delegated => Some(self.push_stop(x, lbl.clone())),
            _ => None,
        };
        // Mark the body walk so `region` treats its active-header entry as
        // emission (see the field doc); restore afterwards — nested loops
        // each mark their own walk.
        let saved_body_walk = self.body_walk;
        self.body_walk = true;
        let body = self.region(hb);
        self.body_walk = saved_body_walk;
        body?;
        // Deferred joins registered during this body walk were already
        // drained by their OWNING diamonds (each diamond closes its own
        // labels before its emission ends), so a meet body that continues
        // the loop ran while the loop was still active. Nothing to drain
        // here — the stop stack only needs its frame popped.
        // Frame pop BY SID (not by position: inner frames' pops may have
        // shifted the stack below `saved`; not by label: the active-meet
        // diamonds inside the body share this loop's label) — the exit
        // region must see the stops as of THIS call, exactly.
        if let Some(sid) = exit_sid {
            self.stops.retain(|e| e.sid != sid);
        }
        self.active_loops.pop();
        let __l = self.out.len();
        self.out.push("}".to_string());
        self.btrace("}");
        if delegated {
            return Ok(()); // the enclosing stop's owner emits the exit region
        }
        match exit_block {
            None => Ok(()), // every exit terminated inline; the loop diverges
            Some(x) => {
                if self.emitted.contains(&x) {
                    self.dbg_refuse("emit_loop:exit_after_body", x);
                    return Err(Unsupported::BranchIntoEmittedCode);
                }
                self.region(x)
            }
        }
    }

    /// Emits blocks from `b` along the fall-through/goto chain, stopping at
    /// the first conditional (a diamond), terminator, stop target, loop
    /// header or already-emitted block.
    fn region(&mut self, b: usize) -> Res<()> {
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
        if !entry_is_body_walk && self.stops.iter().any(|e| e.block == self.resolve(b)) {
            return self.stop_transfer(b);
        }
        // a fresh loop header is a loop construct, not a plain block
        if self.loop_at(b) && !self.active_loops.iter().any(|(h, _)| *h == b) {
            return self.emit_loop(b);
        }
        let mut cur = b;
        loop {
            if cur != b && self.stops.iter().any(|e| e.block == self.resolve(cur)) {
                self.stop_transfer(cur)?;
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
                return self.emit_loop(cur);
            }
            if self.emitted.contains(&cur) {
                self.dbg_refuse("region:emitted", cur);
                return Err(Unsupported::BranchIntoEmittedCode);
            }
            if std::env::var_os("J2R_CFG_DEBUG").is_some() {
                eprintln!(
                    "cfg-debug: emit block {}{} block={} out.len={}",
                    self.m.name,
                    self.m.desc,
                    cur,
                    self.out.len()
                );
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
                    self.jump_to_block(tb)?;
                    return Ok(());
                }
                Instr::IfZero(_, _)
                | Instr::IfICmp(_, _)
                | Instr::IfNull(_)
                | Instr::IfNonNull(_)
                | Instr::IfRefCmp { .. } => {
                    self.diamond(last)?;
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

    /// The DEEP arm scan (G3.1): the full set of blocks reachable from
    /// `start` within this arm's walk — descending through INTERNAL joins
    /// (walk_meet stops at the first one, which hides the arm's true
    /// convergence point: hqr2's diamond(76) reports m1=83/m2=80 while
    /// both arms actually converge at the loop latch 105, via the
    /// notlast diamond nested inside the true arm). Pure inspection; the
    /// set feeds the cross-meet candidate filter and join_via_label's
    /// purity check. Both the raw and resolved form of every visited
    /// block are recorded so raw trampoline-chain predecessors pass the
    /// purity check. Stops: stop targets (transfer), loop headers (an
    /// active/emitted header transfers; a fresh open-exit header walks
    /// through like walk_meet), emitted blocks, terminators, switches,
    /// and the visited set itself (cycles).
    fn walk_deep_set(&self, start: usize) -> HashSet<usize> {
        let mut seen: HashSet<usize> = HashSet::new();
        let mut work: Vec<usize> = vec![start];
        while let Some(raw) = work.pop() {
            let b = self.resolve(raw);
            if !seen.insert(b) {
                continue;
            }
            seen.insert(raw);
            // Loop headers: an ACTIVE (or emitted) header is a transfer
            // boundary — add-only (the arm's region walk emits/transfers
            // the loop). A FRESH open-exit header WALKS THROUGH (add the
            // nodes, continue at the exit — walk_meet's semantics): the
            // arm's region walk will emit the whole loop, so the loop's
            // INTERNAL predecessors of its exit belong to this arm and
            // must pass join_via_label's purity check — hqr2's latch 105
            // has the pred 101 INSIDE the fresh loop(91); without the
            // walk-through the scan stopped at the header 91, 101 never
            // entered `deep`, and 105 was filtered out of the candidates.
            if self.loop_at(b) {
                let active = self.active_loops.iter().any(|(h, _)| *h == b);
                if !active && !self.emitted.contains(&b) {
                    if let Some(l) = self.loops.iter().find(|l| l.header == b) {
                        match l.exit {
                            Some((_, exit_t)) if !l.nodes.contains(&exit_t) => {
                                for &n in &l.nodes {
                                    seen.insert(n);
                                }
                                work.push(exit_t);
                            }
                            _ => {}
                        }
                    }
                }
                continue;
            }
            if self.stops.iter().any(|e| e.block == b) || self.emitted.contains(&b) {
                continue;
            }
            let last = self.cfg.blocks[b].end - 1;
            match &self.m.instrs[last].1 {
                Instr::Return(_) | Instr::AThrow => {}
                Instr::Goto(t) => {
                    let ti = self.instr_index(*t);
                    work.push(self.block_of(ti));
                }
                Instr::IfZero(_, t)
                | Instr::IfICmp(_, t)
                | Instr::IfNull(t)
                | Instr::IfNonNull(t)
                | Instr::IfRefCmp { target: t, .. } => {
                    work.push(self.block_of(self.instr_index(*t)));
                    if last + 1 < self.m.instrs.len() {
                        work.push(self.block_of(last + 1));
                    }
                }
                Instr::TableSwitch { .. } | Instr::LookupSwitch { .. } => {}
                _ => {
                    if last + 1 < self.m.instrs.len() {
                        work.push(self.block_of(last + 1));
                    }
                }
            }
        }
        seen
    }

    /// Emits an if/else diamond rooted at conditional instruction `ci`.
    /// Both arms are walked to their common meet; the meet is emitted once
    /// behind a labeled block (or the diamond is plain if/else when both
    /// arms terminate). A meet that is the enclosing loop's header turns
    /// the arms into `continue`s.
    fn diamond(&mut self, ci: usize) -> Res<()> {
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

        // G3.1 diamond-scoped drain bookkeeping: every deferred join this
        // diamond's arms register closes at THIS diamond's end (see
        // drain_deferred_since).
        let dj_mark = self.deferred.len();
        self.diamond_match(ci, root, t_b, f_b, &arm_blocks, m1, m2)?;
        self.drain_deferred_since(dj_mark)?;
        Ok(())
    }

    /// The diamond lowering decisions (called from [`Self::diamond`],
    /// which owns the mark-and-drain bracket around deferred joins). The
    /// argument count is the diamond's own decision context (condition,
    /// root, both arm blocks, the arm-walk sets' meet results) — all
    /// needed, none optional.
    #[allow(clippy::too_many_arguments)]
    fn diamond_match(
        &mut self,
        ci: usize,
        root: usize,
        t_b: usize,
        f_b: usize,
        arm_blocks: &HashSet<usize>,
        m1: Option<usize>,
        m2: Option<usize>,
    ) -> Res<()> {
        // A meet that is the enclosing loop's header: the arms transfer
        // back to the loop top (`continue`); a terminating arm simply ends
        // inside its branch. The outer stops stay in force.
        let active_meet = |b: Option<usize>| -> Option<usize> {
            b.filter(|m| self.active_loops.iter().any(|(h, _)| *h == *m))
        };
        if std::env::var_os("J2R_CFG_TRACE").is_some() {
            eprintln!(
                "cfg-trace: diamond {}{} ci={} root={} m1={:?} m2={:?} -> active_meet",
                self.m.name, self.m.desc, ci, root, m1, m2
            );
        }
        if let Some(meet) = active_meet(m1).or(active_meet(m2)) {
            let (_, l) = self
                .active_loops
                .iter()
                .rev()
                .find(|(h, _)| *h == meet)
                .unwrap()
                .clone();
            // Frame: the meet entry is the arms' transfer target only.
            // Pop-by-label (not truncate) — an arm may have registered a
            // DEFERRED join whose stop entry must outlive this frame.
            let frame_label = l.clone();
            let meet_sid = self.push_stop(meet, frame_label.clone());
            self.out.push("if __cond {".into());
            self.btrace("if __cond {");
            self.region(t_b)?;
            self.out.push("} else {".into());
            self.btrace("} else {");
            self.region(f_b)?;
            self.out.push("}".into());
            self.btrace("}");
            self.stops.retain(|e| e.sid != meet_sid);
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
            (Some(m), Some(n)) if m == n => self.stops.iter().any(|e| e.block == m),
            (Some(m), None) | (None, Some(m)) => self.stops.iter().any(|e| e.block == m),
            _ => false,
        };
        if std::env::var_os("J2R_CFG_TRACE").is_some() {
            eprintln!(
                "cfg-trace: diamond {}{} ci={} root={} m1={:?} m2={:?} -> already_stopped",
                self.m.name, self.m.desc, ci, root, m1, m2
            );
        }
        if meet_already_stopped {
            self.out.push("if __cond {".into());
            self.btrace("if __cond {");
            self.region(t_b)?;
            self.out.push("} else {".into());
            self.btrace("} else {");
            self.region(f_b)?;
            self.out.push("}".into());
            self.btrace("}");
            return Ok(());
        }

        match (m1, m2) {
            (Some(meet), Some(_)) if meet == m2.unwrap() => {
                if self.join_via_label(meet, t_b, f_b, root, arm_blocks)? {
                    // the meet was DEFERRED: its body drains later — no
                    // inline emission here
                } else {
                    self.region(meet)?;
                }
                Ok(())
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
                if std::env::var_os("J2R_CFG_TRACE").is_some() {
                    eprintln!(
                        "cfg-trace: diamond {}{} ci={} root={} m1={:?} m2={:?} -> divergent",
                        self.m.name, self.m.desc, ci, root, m1, m2
                    );
                }
                let t_is_transfer = self.stops.iter().any(|e| e.block == t_b || e.block == m1);
                let f_is_transfer = self.stops.iter().any(|e| e.block == f_b || e.block == m2);
                if !(t_is_transfer || f_is_transfer) {
                    // G3.1 deep cross-meet attempt: walk_meet stops at the
                    // FIRST join of each arm, which hides the arms' TRUE
                    // convergence point when internal diamonds sit between
                    // (hqr2's diamond(76): shallow m1=83/m2=80, but both
                    // arms reach the loop latch 105 — the f-arm via the
                    // 81 fall-through, the t-arm through the notlast
                    // diamond 85 whose own arms converge at 105 too). The
                    // shallow divergent lowering then spuriously defers
                    // 83 (its "outside pred" is its own chain successor)
                    // and emits 105 inline in the f-arm, so the t-arm's
                    // diamond(85) hits emitted code. The deep scan finds
                    // the unique cross-arm join and hands it to the
                    // battle-tested join_via_label; on refusal the shallow
                    // logic below still runs (fallback, never silent).
                    let d1 = self.walk_deep_set(t_b);
                    let d2 = self.walk_deep_set(f_b);
                    let mut deep = d1.clone();
                    deep.extend(d2.iter().copied());
                    deep.insert(t_b);
                    deep.insert(f_b);
                    let cands: Vec<usize> = d1
                        .intersection(&d2)
                        .copied()
                        .filter(|&c| {
                            c != root
                                && self.is_join(c)
                                && !self.emitted.contains(&c)
                                && !self.loop_at(c)
                                && !self.active_loops.iter().any(|(h, _)| *h == c)
                                && !self.stops.iter().any(|e| e.block == c)
                                && self.cfg.preds()[c]
                                    .iter()
                                    .all(|&p| p == root || deep.contains(&p))
                        })
                        .collect();
                    let inter: Vec<usize> = {
                        let mut v: Vec<usize> = d1.intersection(&d2).copied().collect();
                        v.sort_unstable();
                        v
                    };
                    if std::env::var_os("J2R_CFG_TRACE").is_some() {
                        eprintln!(
                            "cfg-trace: deep cross-meet {}{} ci={} root={} \
                             |d1|={} |d2|={} inter={:?} cands={:?} stops={:?} \
                             active={:?}",
                            self.m.name,
                            self.m.desc,
                            ci,
                            root,
                            d1.len(),
                            d2.len(),
                            inter,
                            cands,
                            self.stops.iter().map(|e| e.block).collect::<Vec<_>>(),
                            self.active_loops
                                .iter()
                                .map(|(x, _)| *x)
                                .collect::<Vec<_>>()
                        );
                    }
                    // Meet selection: ONLY the ARM-START (empty-arm) shape
                    // — the branch target itself is the convergence point
                    // (hqr2's diamond(76): block 82 FALLS INTO the true
                    // arm's block 83, so 83 is both the arm start and the
                    // join; its preds are the root's branch edge and the
                    // else arm's fall-through). A general unique-candidate
                    // fallback would intercept PURE-meet divergent shapes
                    // that the shallow logic deliberately refuses — the
                    // pinned walk_meet law test's shape (nested diamond,
                    // both meets pure) lowered incompletely and refused
                    // with the wrong class — so non-arm-start candidates
                    // keep the shallow path, never the deep one.
                    let meet = if cands.contains(&t_b) {
                        Some(t_b)
                    } else if cands.contains(&f_b) {
                        Some(f_b)
                    } else {
                        None
                    };
                    if let Some(meet) = meet {
                        // Pre-register the REMAINING cross-arm joins
                        // (hqr2's latch 105: one pred is the else arm's
                        // own `goto 105` (81), the others live inside the
                        // meet body — the label must open BEFORE the
                        // if/else so every break site is inside it; the
                        // diamond-scoped drain closes it after the arms).
                        // A candidate qualifies only when some pred
                        // originates from ONE-SIDE-ONLY blocks (d1\d2 or
                        // d2\d1) — t-chain-internal joins (85, 90, ...)
                        // have all preds inside the meet body and stay
                        // inline.
                        for &c in cands.iter() {
                            if c == meet {
                                continue;
                            }
                            let cross_pred = self.cfg.preds()[c].iter().any(|&p| {
                                (d1.contains(&p) && !d2.contains(&p))
                                    || (d2.contains(&p) && !d1.contains(&p))
                            });
                            if cross_pred {
                                if std::env::var_os("J2R_CFG_TRACE").is_some() {
                                    eprintln!(
                                        "cfg-trace: deep pre-register {}{} \
                                         ci={} cross-join={}",
                                        self.m.name, self.m.desc, ci, c
                                    );
                                }
                                self.register_deferred(c);
                            }
                        }
                        // OPTIMISTIC attempt with full rollback: the deep
                        // shape may still refuse inside join_via_label's
                        // arm walks (the shallow walks' guarantees don't
                        // extend to the deep sets). On refusal, restore
                        // EVERY piece of builder state the attempt touched
                        // and fall through to the shallow logic below —
                        // its refusal class (ImpureJoin) is the pinned
                        // law for shapes the deep lowering cannot finish.
                        let out_mark = self.out.len();
                        let brace_mark = self.brace_trace.len();
                        let def_mark = self.deferred.len();
                        let sid_mark = self.stop_seq;
                        let loops_mark = self.active_loops.len();
                        let emitted_before = self.emitted.clone();
                        let bw = self.body_walk;
                        let attempt = self.join_via_label(meet, t_b, f_b, root, &deep);
                        match attempt {
                            Ok(true) => return Ok(()), // deferred: drained by the mark bracket
                            Ok(false) => {
                                // the label form: the CALLER emits the meet
                                // body (join_via_label's contract — the
                                // equal-meet path does the same)
                                self.region(meet)?;
                                return Ok(());
                            }
                            Err(e) => {
                                self.out.truncate(out_mark);
                                self.brace_trace.truncate(brace_mark);
                                self.deferred.truncate(def_mark);
                                self.stops.retain(|s| s.sid < sid_mark);
                                self.active_loops.truncate(loops_mark);
                                self.emitted = emitted_before;
                                self.body_walk = bw;
                                if std::env::var_os("J2R_CFG_TRACE").is_some() {
                                    eprintln!(
                                        "cfg-trace: deep attempt rolled back \
                                         {}{} ci={} meet={} err={:?}",
                                        self.m.name, self.m.desc, ci, meet, e
                                    );
                                }
                                // fall through to the shallow divergent logic
                            }
                        }
                    }
                    // G3.1 — the deferred-join mechanism (the named
                    // ImpureJoin lowering). Defer ONLY the meet(s) that
                    // have OUTSIDE predecessors (the design's guard): a
                    // meet whose predecessors all lie inside the arms is
                    // pure — the owning arm walks THROUGH it and emits it
                    // inline, no deferral, no drain-order hazards. A meet
                    // with an outside predecessor defers (must also be
                    // deferrable: not emitted, not a loop header, not an
                    // owned stop); its edges — arms AND the later outside
                    // preds — break to the label, and the body drains
                    // once at the enclosing loop-body end / lift end. A
                    // shape where nothing defers keeps the loud refusal —
                    // never a silent miscompile.
                    let t_defer =
                        self.meet_has_outside_pred(m1, root, arm_blocks) && self.deferrable(m1);
                    let f_defer =
                        self.meet_has_outside_pred(m2, root, arm_blocks) && self.deferrable(m2);
                    let def_dbg = |b: Option<usize>| -> String {
                        match b {
                            None => "None".into(),
                            Some(m) => format!(
                                "emitted={} active_hdr={} loop_at={} stopped={}",
                                self.emitted.contains(&m),
                                self.active_loops.iter().any(|(h, _)| *h == m),
                                self.loop_at(m),
                                self.stops.iter().any(|e| e.block == m)
                            ),
                        }
                    };
                    if std::env::var_os("J2R_CFG_DEBUG").is_some() {
                        eprintln!(
                            "cfg-debug: divergent defer check {}{} ci={} root={} \
                             t_b={} f_b={} m1={:?} m2={:?} t_out={} f_out={} t_def={} f_def={} \
                             m1[{}] m2[{}]",
                            self.m.name,
                            self.m.desc,
                            ci,
                            root,
                            t_b,
                            f_b,
                            m1,
                            m2,
                            self.meet_has_outside_pred(m1, root, arm_blocks),
                            self.meet_has_outside_pred(m2, root, arm_blocks),
                            t_defer,
                            f_defer,
                            def_dbg(Some(m1)),
                            def_dbg(Some(m2))
                        );
                    }
                    if t_defer || f_defer {
                        if t_defer {
                            self.register_deferred(m1);
                        }
                        if f_defer {
                            self.register_deferred(m2);
                        }
                        self.out.push("if __cond {".into());
                        self.btrace("if __cond {");
                        self.region(t_b)?;
                        self.out.push("} else {".into());
                        self.btrace("} else {");
                        self.region(f_b)?;
                        self.out.push("}".into());
                        self.btrace("}");
                        return Ok(());
                    }
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
                self.btrace("if __cond {");
                self.region(t_b)?;
                self.out.push("} else {".into());
                self.btrace("} else {");
                self.region(f_b)?;
                self.out.push("}".into());
                self.btrace("}");
                Ok(())
            }
            (Some(meet), None) | (None, Some(meet)) => {
                // G3.1 guard-shape refinement (a latent pre-existing bug
                // the deferral unmasked): when the MEET-side arm is itself
                // a TRANSFER — its start block or its meet is already an
                // owned stop — the arm's region breaks to that stop and
                // never reaches the meet; claiming the meet via
                // join_via_label would hoist the meet's code (which lives
                // on the stop owner's side, e.g. the loop EXIT) into this
                // diamond, and the owner's later walk would hit it
                // emitted. tred2's loop(4) guard: true arm starts at the
                // loop's exit-stop 40, meet 60 lies on the exit side —
                // free-walk if/else instead, and the loop's own exit
                // region emits the meet side once, after the loop.
                if std::env::var_os("J2R_CFG_TRACE").is_some() {
                    eprintln!(
                        "cfg-trace: diamond {}{} ci={} root={} m1={:?} m2={:?} -> single_meet",
                        self.m.name, self.m.desc, ci, root, m1, m2
                    );
                }
                let meet_side_is_transfer = if m1.is_some() {
                    self.stops.iter().any(|e| e.block == t_b || e.block == meet)
                } else {
                    self.stops.iter().any(|e| e.block == f_b || e.block == meet)
                };
                if meet_side_is_transfer {
                    self.out.push("if __cond {".into());
                    self.btrace("if __cond {");
                    self.region(t_b)?;
                    self.out.push("} else {".into());
                    self.btrace("} else {");
                    self.region(f_b)?;
                    self.out.push("}".into());
                    self.btrace("}");
                    return Ok(());
                }
                if self.join_via_label(meet, t_b, f_b, root, arm_blocks)? {
                    // the meet was DEFERRED: its body drains later
                } else {
                    self.region(meet)?;
                }
                Ok(())
            }
            (None, None) => {
                if std::env::var_os("J2R_CFG_TRACE").is_some() {
                    eprintln!(
                        "cfg-trace: diamond {}{} ci={} root={} m1={:?} m2={:?} -> both_terminate",
                        self.m.name, self.m.desc, ci, root, m1, m2
                    );
                }
                // both arms terminate — plain if/else; the surrounding
                // stop context carries into the arms
                self.out.push("if __cond {".into());
                self.btrace("if __cond {");
                self.region(t_b)?;
                self.out.push("} else {".into());
                self.btrace("} else {");
                self.region(f_b)?;
                self.out.push("}".into());
                self.btrace("}");
                Ok(())
            }
            _ => unreachable!("diamond: both-equal meet handled above"),
        }
    }

    /// Emits a labeled join block around the two diamond arms. Returns
    /// `true` when the meet was DEFERRED (G3.1: an outside predecessor
    /// made the inline join impure and the meet was deferrable — the arms
    /// break to the deferred label and the meet body drains later),
    /// `false` when the meet is emitted inline once by the caller.
    fn join_via_label(
        &mut self,
        meet: usize,
        t_b: usize,
        f_b: usize,
        root: usize,
        arm_blocks: &HashSet<usize>,
    ) -> Res<bool> {
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
                        // G3.1 — an outside predecessor makes the inline
                        // join impure. DEFER instead of refusing when the
                        // meet is deferrable: the (meet, label) entry goes
                        // on the builder-owned stop stack, the arms break
                        // to the label, the later outside predecessor's
                        // edge breaks to it too, and the meet body emits
                        // ONCE at the enclosing drain point. A meet that
                        // fails the deferrable guard keeps the loud
                        // refusal — never a silent miscompile.
                        if std::env::var_os("J2R_CFG_TRACE").is_some() {
                            eprintln!("cfg-trace: join purity ci-meet={} -> defer", meet);
                        }
                        if self.deferrable(meet) {
                            self.register_deferred(meet);
                            self.out.push("if __cond {".into());
                            self.btrace("if __cond {");
                            self.region(t_b)?;
                            self.out.push("} else {".into());
                            self.btrace("} else {");
                            self.region(f_b)?;
                            self.out.push("}".into());
                            self.btrace("}");
                            // no trailing break, no close: the deferred
                            // label stays open until its drain point
                            return Ok(true);
                        }
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
            self.dbg_refuse("join_via_label", meet);
            return Err(Unsupported::BranchIntoEmittedCode);
        }
        let lbl = self.fresh_label("join", meet);
        self.out.push(format!("{lbl}: {{"));
        let __t = self.out.last().unwrap().clone();
        self.btrace(&__t);
        self.out.push("if __cond {".into());
        self.btrace("if __cond {");
        // Frame: the meet entry is the arms' transfer target only; pop by
        // SID so a DEFERRED join registered inside an arm survives.
        let join_sid = self.push_stop(meet, lbl.clone());
        self.region(t_b)?;
        self.out.push("} else {".into());
        self.btrace("} else {");
        self.region(f_b)?;
        self.out.push("}".into());
        self.btrace("}");
        self.stops.retain(|e| e.sid != join_sid);
        self.out.push(format!("break {lbl};"));
        let __l = self.out.len();
        self.out.push("}".to_string());
        self.btrace("}");
        Ok(false)
    }

    /// G3.1 — registers `meet` as a DEFERRED join: opens its labeled
    /// block (closed later at the drain point), pushes (meet, label) on
    /// the builder-owned stop stack (NOT popped by any frame — every
    /// edge to the meet breaks to the label while it is open), and
    /// records it for the drain.
    fn register_deferred(&mut self, meet: usize) {
        let lbl = self.fresh_label("djoin", meet);
        if std::env::var_os("J2R_CFG_DEBUG").is_some() {
            eprintln!(
                "cfg-debug: deferred join registered {}{} meet={} label={lbl}",
                self.m.name, self.m.desc, meet
            );
        }
        self.out.push(format!("{lbl}: {{"));
        let __t = self.out.last().unwrap().clone();
        self.btrace(&__t);
        let d_sid = self.push_stop(meet, lbl.clone());
        self.deferred.push(DeferredJoin {
            meet,
            label: lbl,
            sid: d_sid,
        });
    }

    /// Whether `meet` may be registered as a deferred join: not already
    /// emitted, not an active loop header, not a fresh loop header
    /// (conservative v1 — the census shapes have meet_loop=None), and not
    /// already an owned stop. A fall-through predecessor is ALLOWED (the
    /// design's original no-fall-through guard was relaxed after the
    /// three hazard cases were analyzed): a falling predecessor's region
    /// walk stops at the (meet, label) stop entry and `break`s forward to
    /// the drain position — the same target the fall-through reached; a
    /// fall-through pred emitted BEFORE registration would have emitted
    /// the meet inline, making it not-deferrable via the emitted guard;
    /// and the meet body itself only ever falls INTO loudly-refused
    /// shapes (BranchIntoEmittedCode / FallOffEnd), never silently.
    fn deferrable(&self, meet: usize) -> bool {
        !self.emitted.contains(&meet)
            && !self.active_loops.iter().any(|(h, _)| *h == meet)
            && !self.loop_at(meet)
            && !self.stops.iter().any(|e| e.block == meet)
    }

    /// Whether `meet` has a predecessor OUTSIDE the diamond's arm walks
    /// (and is not the diamond's own root) — the G3.1 deferral guard: only
    /// such meets defer; pure meets (all preds inside the arms) are
    /// emitted inline by their owning arm.
    fn meet_has_outside_pred(&self, meet: usize, root: usize, arm_blocks: &HashSet<usize>) -> bool {
        self.cfg.preds()[meet]
            .iter()
            .any(|&p| !arm_blocks.contains(&p) && p != root)
    }

    /// The G3.1 drain, DIAMOND-SCOPED: closes every deferred join
    /// registered at or after `mark` (innermost label first) and emits
    /// its meet body ONCE after the close — `break <label>` lands exactly
    /// at the body. Draining at the OWNING diamond's end is what keeps
    /// the emission lexically correct: the label's braces open and close
    /// inside the diamond's own if/else, so a sibling arm (the enclosing
    /// diamond's `else`) is never emitted while a label is still open —
    /// the loop-body-end/lift-end drain points of the first draft left
    /// the label open across the sibling arm and cfg_equiv caught the
    /// invalid `} else {` (BigJson's escape chain). Each drained meet's
    /// stop entry is consumed first so the body walk emits it instead of
    /// breaking to itself; bodies may register further deferred joins
    /// (drained after, by the same loop) and may break to STILL-open
    /// deferred labels of enclosing diamonds.
    fn drain_deferred_since(&mut self, mark: usize) -> Res<()> {
        while self.deferred.len() > mark {
            let d = self.deferred.pop().expect("mark invariant");
            self.out.push("}".to_string());
            self.btrace("}");
            self.stops.retain(|e| e.sid != d.sid);
            if std::env::var_os("J2R_CFG_DEBUG").is_some() {
                eprintln!(
                    "cfg-debug: deferred join drained {}{} meet={} label={}",
                    self.m.name, self.m.desc, d.meet, d.label
                );
            }
            self.region(d.meet)?;
        }
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
        if std::env::var_os("J2R_CFG_DEBUG").is_some() {
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
