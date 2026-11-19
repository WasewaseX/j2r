//! `cfg`::builder2 — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use std::collections::HashSet;

impl<'a> Builder<'a> {
    /// The diamond lowering decisions (called from [`Self::diamond`],
    /// which owns the mark-and-drain bracket around deferred joins). The
    /// argument count is the diamond's own decision context (condition,
    /// root, both arm blocks, the arm-walk sets' meet results) — all
    /// needed, none optional.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn diamond_match(
        &mut self,
        ci: usize,
        root: usize,
        t_b: usize,
        f_b: usize,
        arm_blocks: &HashSet<usize>,
        m1: Option<usize>,
        m2: Option<usize>,
    ) -> Res<()> {
        if self.diamond_active_meet(ci, root, t_b, f_b, m1, m2)? {
            return Ok(());
        }
        if self.diamond_already_stopped(ci, root, t_b, f_b, m1, m2)? {
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
            (Some(m1v), Some(m2v)) if m1v != m2v => {
                self.diamond_divergent(ci, root, t_b, f_b, arm_blocks, m1v, m2v)
            }
            (Some(meet), None) | (None, Some(meet)) => {
                self.diamond_single_meet(ci, root, t_b, f_b, meet, arm_blocks, m1, m2)
            }
            (None, None) => self.diamond_both_terminate(ci, root, t_b, f_b, m1, m2),
            _ => unreachable!("diamond: both-equal meet handled above"),
        }
    }

    /// The free-walk if/else (byte-identical at all six diamond sites):
    /// each arm's region consumes its own side under the current stops.
    pub(crate) fn free_walk_if_else(&mut self, t_b: usize, f_b: usize) -> Res<()> {
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

    /// The loop-header meet case (verbatim from `diamond_match`, round-6
    /// reshaping): the arms transfer back to the enclosing loop's top.
    /// Returns true when this diamond was handled here.
    pub(crate) fn diamond_active_meet(
        &mut self,
        ci: usize,
        root: usize,
        t_b: usize,
        f_b: usize,
        m1: Option<usize>,
        m2: Option<usize>,
    ) -> Res<bool> {
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
            self.free_walk_if_else(t_b, f_b)?;
            self.stops.retain(|e| e.sid != meet_sid);
            return Ok(true);
        }
        Ok(false)
    }

    /// The already-stopped meet case (verbatim): an enclosing structure
    /// owns the meet; free-walk if/else under the current stops.
    /// Returns true when this diamond was handled here.
    pub(crate) fn diamond_already_stopped(
        &mut self,
        ci: usize,
        root: usize,
        t_b: usize,
        f_b: usize,
        m1: Option<usize>,
        m2: Option<usize>,
    ) -> Res<bool> {
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
            self.free_walk_if_else(t_b, f_b)?;
            return Ok(true);
        }
        Ok(false)
    }

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
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn diamond_divergent(
        &mut self,
        ci: usize,
        root: usize,
        t_b: usize,
        f_b: usize,
        arm_blocks: &HashSet<usize>,
        m1: usize,
        m2: usize,
    ) -> Res<()> {
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
            // deep scan finds the unique cross-arm join and hands it
            // to the battle-tested join_via_label; on refusal the
            // shallow logic below still runs (fallback, never silent).
            let scan = self.deep_cross_candidates(root, t_b, f_b);
            if self.diamond_deep_cross_meet(ci, root, t_b, f_b, scan)? {
                return Ok(());
            }
            if self.diamond_deferred_joins(ci, root, t_b, f_b, arm_blocks, m1, m2)? {
                return Ok(());
            }
            return Err(Unsupported::ImpureJoin);
        }
        self.free_walk_if_else(t_b, f_b)?;
        Ok(())
    }

    /// The G3.1 deep cross-arm scan (verbatim from the divergent arm): both
    /// arms' deep walk sets, the union, the arm-start join candidates and
    /// their sorted intersection.
    pub(crate) fn deep_cross_candidates(&self, root: usize, t_b: usize, f_b: usize) -> DeepScan {
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
        DeepScan {
            d1,
            d2,
            deep,
            cands,
            inter,
        }
    }

    /// The deep cross-meet attempt (verbatim): arm-start meet selection,
    /// the cross-arm pre-registration loop and the optimistic
    /// join_via_label attempt with full rollback. Returns true when the
    /// diamond was handled here.
    pub(crate) fn diamond_deep_cross_meet(
        &mut self,
        ci: usize,
        root: usize,
        t_b: usize,
        f_b: usize,
        scan: DeepScan,
    ) -> Res<bool> {
        let DeepScan {
            d1,
            d2,
            deep,
            cands,
            inter,
        } = scan;
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
                    (d1.contains(&p) && !d2.contains(&p)) || (d2.contains(&p) && !d1.contains(&p))
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
                Ok(true) => return Ok(true), // deferred: drained by the mark bracket
                Ok(false) => {
                    // the label form: the CALLER emits the meet
                    // body (join_via_label's contract — the
                    // equal-meet path does the same)
                    self.region(meet)?;
                    return Ok(true);
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
        Ok(false)
    }

    /// The G3.1 deferred-join mechanism (verbatim): defer ONLY the meet(s)
    /// that have OUTSIDE predecessors, emit the free-walk if/else under the
    /// registered stops, and keep the loud refusal when nothing defers.
    /// Returns true when this diamond was handled here.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn diamond_deferred_joins(
        &mut self,
        ci: usize,
        root: usize,
        t_b: usize,
        f_b: usize,
        arm_blocks: &HashSet<usize>,
        m1: usize,
        m2: usize,
    ) -> Res<bool> {
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
        let t_defer = self.meet_has_outside_pred(m1, root, arm_blocks) && self.deferrable(m1);
        let f_defer = self.meet_has_outside_pred(m2, root, arm_blocks) && self.deferrable(m2);
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
        if cfg_debug() {
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
            self.free_walk_if_else(t_b, f_b)?;
            return Ok(true);
        }
        if cfg_debug() {
            eprintln!(
                "cfg-debug: divergent joins unresolved {}{} ci={} (block {}) \
                             t_b={} f_b={} m1={:?} m2={:?}",
                self.m.name, self.m.desc, ci, root, t_b, f_b, m1, m2
            );
        }
        Ok(false)
    }

    /// The single-meet arm (verbatim): the G3.1 guard-shape refinement —
    /// when the MEET-side arm is itself a TRANSFER, free-walk if/else;
    /// otherwise the meet joins via its label.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn diamond_single_meet(
        &mut self,
        ci: usize,
        root: usize,
        t_b: usize,
        f_b: usize,
        meet: usize,
        arm_blocks: &HashSet<usize>,
        m1: Option<usize>,
        m2: Option<usize>,
    ) -> Res<()> {
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
            self.free_walk_if_else(t_b, f_b)?;
            return Ok(());
        }
        if self.join_via_label(meet, t_b, f_b, root, arm_blocks)? {
            // the meet was DEFERRED: its body drains later
        } else {
            self.region(meet)?;
        }
        Ok(())
    }

    /// The both-terminate arm (verbatim): plain if/else — the surrounding
    /// stop context carries into the arms.
    pub(crate) fn diamond_both_terminate(
        &mut self,
        ci: usize,
        root: usize,
        t_b: usize,
        f_b: usize,
        m1: Option<usize>,
        m2: Option<usize>,
    ) -> Res<()> {
        if std::env::var_os("J2R_CFG_TRACE").is_some() {
            eprintln!(
                "cfg-trace: diamond {}{} ci={} root={} m1={:?} m2={:?} -> both_terminate",
                self.m.name, self.m.desc, ci, root, m1, m2
            );
        }
        // both arms terminate — plain if/else; the surrounding
        // stop context carries into the arms
        self.free_walk_if_else(t_b, f_b)?;
        Ok(())
    }

    /// Emits a labeled join block around the two diamond arms. Returns
    /// `true` when the meet was DEFERRED (G3.1: an outside predecessor
    /// made the inline join impure and the meet was deferrable — the arms
    /// break to the deferred label and the meet body drains later),
    /// `false` when the meet is emitted inline once by the caller.
    pub(crate) fn join_via_label(
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
                        if cfg_debug() {
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
    pub(crate) fn register_deferred(&mut self, meet: usize) {
        let lbl = self.fresh_label("djoin", meet);
        if cfg_debug() {
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
    pub(crate) fn deferrable(&self, meet: usize) -> bool {
        !self.emitted.contains(&meet)
            && !self.active_loops.iter().any(|(h, _)| *h == meet)
            && !self.loop_at(meet)
            && !self.stops.iter().any(|e| e.block == meet)
    }

    /// Whether `meet` has a predecessor OUTSIDE the diamond's arm walks
    /// (and is not the diamond's own root) — the G3.1 deferral guard: only
    /// such meets defer; pure meets (all preds inside the arms) are
    /// emitted inline by their owning arm.
    pub(crate) fn meet_has_outside_pred(
        &self,
        meet: usize,
        root: usize,
        arm_blocks: &HashSet<usize>,
    ) -> bool {
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
    pub(crate) fn drain_deferred_since(&mut self, mark: usize) -> Res<()> {
        while self.deferred.len() > mark {
            let d = self.deferred.pop().expect("mark invariant");
            self.out.push("}".to_string());
            self.btrace("}");
            self.stops.retain(|e| e.sid != d.sid);
            if cfg_debug() {
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
