//! `cfg`::graph — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use crate::ir::Instr;
use crate::MethodPlan;
use std::collections::BTreeSet;

/// The cfg-debug gate: the diagnostic dump behind `J2R_CFG_DEBUG` —
/// off in production, one env read per dump site.
pub(crate) fn cfg_debug() -> bool {
    std::env::var_os("J2R_CFG_DEBUG").is_some()
}

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

pub(crate) struct Block {
    /// Index of the block's first instruction.
    pub(crate) start: usize,
    /// Index one past the block's last instruction.
    pub(crate) end: usize,
    /// Successor instruction indices (deduped, order preserved).
    pub(crate) succs: Vec<usize>,
}

pub(crate) struct Cfg {
    pub(crate) blocks: Vec<Block>,
    pub(crate) entry: usize,
}

impl Cfg {
    pub(crate) fn build(m: &MethodPlan) -> Cfg {
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

    pub(crate) fn block_of(&self, instr_idx: usize) -> usize {
        self.blocks
            .iter()
            .position(|b| b.start <= instr_idx && instr_idx < b.end)
            .expect("instruction inside a block")
    }

    pub(crate) fn preds(&self) -> Vec<Vec<usize>> {
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
    pub(crate) fn sorted_succ_lists(&self) -> Vec<Vec<usize>> {
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
    pub(crate) fn idom(&self) -> Vec<Option<usize>> {
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

    pub(crate) fn dominates(idoms: &[Option<usize>], a: usize, b: usize) -> bool {
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
