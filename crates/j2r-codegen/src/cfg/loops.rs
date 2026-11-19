//! `cfg`::loops — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use crate::ir::Instr;
use crate::MethodPlan;
use std::collections::{HashMap, HashSet};

/// CHK intersect on reverse-postorder indices: walk whichever finger is
/// RPO-deeper up its idom chain until the fingers meet.
pub(crate) fn intersect(
    idoms: &[Option<usize>],
    rpono: &[usize],
    mut a: usize,
    mut b: usize,
) -> usize {
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
pub(crate) struct Loop {
    pub(crate) header: usize,
    /// The single OPEN exit edge (from-block, to-block) — `None` when
    /// every exit target terminates (early `return`/`throw` blocks are
    /// emitted inline at the edge, so the loop needs no post-loop exit
    /// region at all).
    pub(crate) exit: Option<(usize, usize)>,
    /// Every block of the natural loop (header included) — used by the
    /// builder's join-purity check to recognise the loop's own back
    /// edges at a loop-header meet, and by the arm walk to cover a
    /// traversed loop's internal predecessors of a shared exit.
    pub(crate) nodes: HashSet<usize>,
}

/// Pure-`goto` trampoline blocks: a block of exactly one `goto t` whose
/// target is a different block. javac chains scope exits through them
/// (`break` → trampoline → after-loop), so two exit edges can reach the
/// SAME continuation via different blocks — the readiness wave's
/// multi-exit census measured exactly this (hdr_pc=99 → targets [126, 135]
/// where block 126 is just `goto 135`). Stops key on the RESOLVED target;
/// chains never walk through a loop header (edges to a header are the
/// loop top, not whatever follows it) and never through a self-jump.
pub(crate) fn trampoline_map(cfg: &Cfg, m: &MethodPlan, loops: &[Loop]) -> HashMap<usize, usize> {
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

/// Successor lists, sorted+deduped (verbatim from `analyse`'s DFS prep,
/// round-5 reshaping; the same lists serve the DFS and the exit pass).
pub(crate) fn sorted_succ_lists(cfg: &Cfg) -> Vec<Vec<usize>> {
    cfg.blocks
        .iter()
        .map(|b| {
            let mut v: Vec<usize> = b.succs.clone();
            v.sort_unstable();
            v.dedup();
            v
        })
        .collect()
}

/// Retreating edges via iterative DFS (verbatim from `analyse`,
/// round-5 reshaping).
pub(crate) fn retreating_edges(cfg: &Cfg, succs: &[Vec<usize>]) -> Vec<(usize, usize)> {
    let n = cfg.blocks.len();
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
    retreating
}

/// Natural-loop node sets (unioned across all back edges of a header):
/// every node that can reach the back-edge source without going through
/// the header (verbatim from `analyse` pass 1, round-5 reshaping).
pub(crate) fn loop_bodies(
    cfg: &Cfg,
    retreating: &[(usize, usize)],
) -> Vec<(usize, HashSet<usize>)> {
    let preds = cfg.preds();
    let mut back_edges: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(from, header) in retreating {
        back_edges.entry(header).or_default().push(from);
    }
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
    bodies
}

/// Exit classification per loop (verbatim from `analyse` pass 2,
/// round-5 reshaping). Exit handling (revised by the lift-closure wave,
/// 2026-11-07):
///
/// * Edges are grouped by TARGET BLOCK, not by (from, to) pair —
///   javac's `break` inside a `while`/`do-while` adds a second edge to
///   the SAME after-loop block, and the builder already routes every
///   body→exit edge through one `break` stop. Comparing (from, to)
///   pairs refused exactly those loops.
/// * A target block whose terminator is `return`/`athrow` is NOT an
///   open exit: the natural-loop node set is built by walking
///   PREDECESSORS from the back edge, so early-return blocks inside
///   the loop body are excluded from `nodes` and their incoming edges
///   looked like exits with a second target. The builder emits such
///   blocks inline at the edge (the walk emits the return and that
///   path ends) — no post-loop region needed.
/// * What remains must be ONE open exit target; a genuine multi-target
///   loop (Java labeled `break` to two different labels) still refuses
///   as MultiExitLoop.
/// * The old "exit block has no outside predecessors" purity check is
///   GONE: the builder routes a shared exit through the enclosing
///   stop (the JaggedNew continue-target shape) or falls back loudly
///   (BranchIntoEmittedCode) on shapes it cannot structure — the
///   conservative direction is unchanged, nothing wrong can be
///   emitted (cfg_equiv is the proof obligation).
pub(crate) fn classify_loops(
    cfg: &Cfg,
    m: &MethodPlan,
    succs: &[Vec<usize>],
    tramp: &HashMap<usize, usize>,
    bodies: Vec<(usize, HashSet<usize>)>,
) -> Result<Vec<Loop>, Unsupported> {
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
    Ok(loops)
}

/// Analyse the CFG; `Err(reason)` means the method falls back to dispatch.
/// On success returns the loops plus the trampoline-resolution map (the
/// builder keys every stop lookup on resolved targets).
pub(crate) fn analyse(
    cfg: &Cfg,
    m: &MethodPlan,
) -> Result<(Vec<Loop>, HashMap<usize, usize>), Unsupported> {
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

    let succs = sorted_succ_lists(cfg);
    let retreating = retreating_edges(cfg, &succs);
    let idoms = cfg.idom();
    for &(from, to) in &retreating {
        if !Cfg::dominates(&idoms, to, from) {
            return Err(Unsupported::Irreducible);
        }
    }

    // Natural loops (unioned across all back edges of a header). Pass 1:
    // loop node sets (headers known afterwards — the trampoline map needs
    // them: chains never walk through a loop top).
    let bodies = loop_bodies(cfg, &retreating);
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
    let loops = classify_loops(cfg, m, &succs, &tramp, bodies)?;
    Ok((loops, tramp))
}
