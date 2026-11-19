//! `cfg`::builder — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use crate::ir::Instr;
use crate::MethodPlan;
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------- emission

pub(crate) type Stops = Vec<StopEntry>;

/// One entry on the builder-owned stop stack. `sid` is a globally unique
/// frame id (monotonic counter) — frames pop BY SID, never by position or
/// by label: a positional `truncate` goes stale when an inner frame's pop
/// removes entries BELOW the mark (the retain-by-label pops shift the
/// stack — transpose's exit stop survived its own truncate and the block
/// was never emitted), and a label-retain catches OTHER entries sharing
/// the label (an active-meet diamond's frame label is the enclosing
/// loop's label, which is also the loop's exit-stop entry label — the
/// retain killed the exit stop mid-body).
pub(crate) struct StopEntry {
    pub(crate) block: usize,
    pub(crate) label: String,
    pub(crate) sid: u64,
}

/// A join meet registered as a DEFERRED join (the G3.1 mechanism, the
/// named `ImpureJoin` lowering): its (meet, label) sits on the
/// builder-owned stop stack until the enclosing drain point (the
/// emitting loop's body end, or the end of `lift`), its body then emits
/// ONCE after the label closes. Every edge to the meet — diamond arms
/// AND later outside predecessors — `break`s to the label while it is
/// open, so the meet needs no inline position in the walk.
pub(crate) struct DeferredJoin {
    pub(crate) meet: usize,
    pub(crate) label: String,
    pub(crate) sid: u64,
}

/// The structured-body builder. `emit_fn(idx, out)` emits instruction
/// `idx`'s code exactly as the dispatch emitter would (the caller never
/// passes branch/switch instructions here — the builder handles them).
pub(crate) struct Builder<'a> {
    pub(crate) m: &'a MethodPlan,
    pub(crate) cfg: &'a Cfg,
    pub(crate) loops: Vec<Loop>,
    /// Pure-goto trampoline blocks → resolved target. Every stop lookup
    /// keys on `resolve(block)` so an edge through a javac goto-chain
    /// finds the stop pushed for the chain's end (and vice versa).
    pub(crate) tramp: HashMap<usize, usize>,
    pub(crate) emit: EmitFn<'a>,
    pub(crate) emitted: HashSet<usize>,
    /// Active loop headers (innermost last): (block, label) — `continue`.
    pub(crate) active_loops: Vec<(usize, String)>,
    /// The builder-OWNED stop stack (G3.1): the old call-threaded
    /// `&Stops` snapshots, promoted so a DEFERRED join registered during
    /// a diamond's arm walk stays visible to the later outside-predecessor
    /// walks in the same enclosing region. Frames push/pop symmetrically
    /// (pop-BY-SID, so deferred entries survive them); only deferred
    /// registrations push asymmetrically, and only a drain point consumes
    /// those.
    pub(crate) stops: Stops,
    /// Monotonic frame-id source: every stop push gets a globally unique
    /// sid; the pop retains by that sid (see StopEntry).
    pub(crate) stop_seq: u64,
    /// Registered-but-undrained deferred joins, in registration order
    /// (the LAST entry is the innermost open label and drains first).
    pub(crate) deferred: Vec<DeferredJoin>,
    /// Set by `emit_loop` around its body-walk `region` call: the entry
    /// block is being EMITTED, not transferred to. `region` clears the
    /// flag for its own recursion so only the immediate body-walk entry
    /// is exempt from the stop check.
    pub(crate) body_walk: bool,
    pub(crate) out: Vec<String>,
    pub(crate) label_counter: usize,
    /// cfg-trace brace tracker: the open-construct kinds in emission
    /// order. Debug-only (J2R_CFG_TRACE); every `}` must close the top
    /// opener and every `} else {` must close an `if`.
    pub(crate) brace_trace: Vec<&'static str>,
}

pub(crate) type Res<T> = Result<T, Unsupported>;

/// The per-instruction emitter callback handed to [`Builder`] (the dispatch
/// emitter's `emit_instr`, boxed). Factored out for clippy type_complexity.
pub(crate) type EmitFn<'a> = Box<dyn FnMut(usize, &mut Vec<String>) + 'a>;

impl<'a> Builder<'a> {
    pub(crate) fn new(
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
    pub(crate) fn btrace(&mut self, line: &str) {
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
    pub(crate) fn resolve(&self, b: usize) -> usize {
        let mut cur = b;
        while let Some(&n) = self.tramp.get(&cur) {
            cur = n;
        }
        cur
    }

    pub(crate) fn fresh_label(&mut self, kind: &str, b: usize) -> String {
        self.label_counter += 1;
        format!("'{}_{}_{}", kind, b, self.label_counter)
    }

    /// Pushes a stop entry with a fresh globally-unique frame id and
    /// returns it — the pop site retains by this sid, never by position
    /// or label (see StopEntry).
    pub(crate) fn push_stop(&mut self, block: usize, label: String) -> u64 {
        let sid = self.stop_seq;
        self.stop_seq += 1;
        self.stops.push(StopEntry { block, label, sid });
        sid
    }

    /// cfg-debug diagnostic for a loud refusal site: names the block the
    /// walk refused on, so a census trace can point at the exact shape.
    pub(crate) fn dbg_refuse(&self, site: &str, b: usize) {
        if std::env::var("J2R_CFG_DEBUG").as_deref() == Ok("3") {
            eprintln!(
                "cfg-debug: ---- emitted text at refuse[{}] (block {}) ----",
                site, b
            );
            for (i, line) in self.out.iter().enumerate() {
                eprintln!("cfg-debug: {:>4}: {}", i, line);
            }
        }
        if cfg_debug() {
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

    pub(crate) fn loop_at(&self, b: usize) -> bool {
        self.loops.iter().any(|l| l.header == b)
    }

    pub(crate) fn is_join(&self, b: usize) -> bool {
        self.cfg.preds()[b].len() >= 2
    }

    pub(crate) fn block_of(&self, instr_idx: usize) -> usize {
        self.cfg.block_of(instr_idx)
    }

    pub(crate) fn instr_index(&self, pc: u32) -> usize {
        self.m
            .instrs
            .iter()
            .position(|(p, _)| *p == pc)
            .expect("branch target is an instruction boundary")
    }

    /// Emits the transfer to a reached stop target: `continue` into the
    /// active loop's own label, or `break` to the stop's label.
    pub(crate) fn stop_transfer(&mut self, cur: usize) -> Res<()> {
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
    pub(crate) fn jump_to_block(&mut self, tb: usize) -> Res<()> {
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
    pub(crate) fn emit_loop(&mut self, hb: usize) -> Res<()> {
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
            if cfg_debug() {
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
        if cfg_debug() {
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
    pub(crate) fn region(&mut self, b: usize) -> Res<()> {
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
            if cfg_debug() {
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
    pub(crate) fn walk_meet(&self, start: usize) -> (Option<usize>, HashSet<usize>) {
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
    pub(crate) fn walk_deep_set(&self, start: usize) -> HashSet<usize> {
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
    pub(crate) fn diamond(&mut self, ci: usize) -> Res<()> {
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
}
