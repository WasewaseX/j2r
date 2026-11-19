use super::*;

/// The current thread's record, auto-registering on first touch (T07).
/// Auto-registration is the rooting story: any thread that can hold Java
/// state is visible to the collector from that moment on. Auto-registered
/// threads run (state `Running`) but do NOT join the coop ring — ring
/// participation is a property of started Java threads (`begin_main_...
/// thread` / `start`), so a blocked bystander can never own the turn.
pub(crate) fn current_record() -> Arc<ThreadRecord> {
    CURRENT.with(|c| {
        if let Some(r) = c.borrow().as_ref() {
            return r.clone();
        }
        let rec = register_named_record("", true, false);
        adopt_record(rec.clone());
        rec
    })
}
/// Registers a new record in the global registry. `running` selects the
/// initial state: an auto-registered thread is executing right now;
/// a pre-registered child is `Parked` (holds no Java state, scan-safe
/// from the first instant) until its body adopts it.
pub(crate) fn register_named_record(name: &str, running: bool, in_ring: bool) -> Arc<ThreadRecord> {
    let mut g = lock_sched();
    let rec = Arc::new(ThreadRecord::new(g.records.len(), name, running, in_ring));
    g.records.push(rec.clone());
    rec
}
/// Adopts `rec` as this native thread's record (the start handoff: the
/// record is pre-registered by the starting thread so the Java object's
/// tid link exists before the child runs).
pub(crate) fn adopt_record(rec: Arc<ThreadRecord>) {
    CURRENT.with(|c| *c.borrow_mut() = Some(rec));
}
/// Registers the generated worker thread as the *main* Java thread.
/// Called by the generated `main_run` as its very first statement, before
/// statics init — so every allocation afterwards happens on a registered,
/// correctly named thread.
pub fn begin_main_java_thread() {
    let rec = current_record();
    *rec.name.lock().unwrap_or_else(|e| e.into_inner()) = "main".to_string();
    rec.daemon.store(false, Ordering::Release);
    rec.alive.store(true, Ordering::Release);
    rec.set_running();
    rec.in_ring.store(true, Ordering::Release);
    apply_coop_seed_if_needed();
}

// ------------------------------------------------------------ coop mode
pub(crate) fn coop_mode() -> bool {
    #[cfg(test)]
    {
        match COOP_TEST_OVERRIDE.load(Ordering::Acquire) {
            0 => return false,
            1 => return true,
            _ => {}
        }
    }
    static COOP: OnceLock<bool> = OnceLock::new();
    *COOP.get_or_init(|| matches!(std::env::var("J2R_SCHED").ok().as_deref(), Some("coop")))
}
#[cfg(test)]
static COOP_TEST_OVERRIDE: AtomicI64 = AtomicI64::new(-1);
#[cfg(test)]
pub(crate) fn set_coop_for_test(on: bool) {
    COOP_TEST_OVERRIDE.store(if on { 1 } else { 0 }, Ordering::Release);
}
pub(crate) fn coop_seed() -> usize {
    static SEED: OnceLock<usize> = OnceLock::new();
    *SEED.get_or_init(|| {
        std::env::var("J2R_SCHED_SEED")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    })
}
/// Applies the coop seed once, at main-thread registration (deterministic
/// rotation start; no-op in free mode).
pub(crate) fn apply_coop_seed_if_needed() {
    if !coop_mode() {
        return;
    }
    let mut g = lock_sched();
    let n = g.records.len();
    if n > 0 {
        g.turn = coop_seed() % n;
    }
}
pub(crate) fn next_live_index(records: &[Arc<ThreadRecord>], from: usize) -> usize {
    let n = records.len();
    for step in 1..=n {
        let i = (from + step) % n;
        let r = &records[i];
        if r.in_ring.load(Ordering::Acquire) && r.state.load(Ordering::Acquire) != STATE_EXITED {
            return i;
        }
    }
    from
}
/// A yield point (T09): in coop mode, if the thread HOLDS the turn it
/// passes it on (the actual yield); then it waits until the turn comes
/// back before resuming. Free mode: no-op.
///
/// The pass-only-if-held shape is what makes coop output a pure function
/// of (program, seed): a thread that does not hold the turn can never
/// act at a yield point, so a record's log position is pinned by turn
/// ownership rather than by who reached the yield point first.
///
/// Every switch in the runtime happens here or through the start handoff —
/// never preemptively. Callers: `gc_point` polls, `start`, `join`, `sleep`,
/// `yield` (and monitor ops from Wave 3).
pub(crate) fn yield_point() {
    if !coop_mode() {
        return;
    }
    let me = current_record();
    let mut g = lock_sched();
    // Non-ring threads never participate: they cannot hold, pass, or wait
    // for the turn (a blocked bystander must never own or starve it).
    if !me.in_ring.load(Ordering::Acquire) {
        return;
    }
    // Fast path: no OTHER ring-live thread — nothing to rotate with.
    let other_ring_live = g.records.iter().any(|r| {
        r.reg_index != me.reg_index
            && r.in_ring.load(Ordering::Acquire)
            && r.state.load(Ordering::Acquire) != STATE_EXITED
    });
    if !other_ring_live {
        return;
    }
    // Phase 1 — if I arrived holding the turn, pass it on (the yield).
    if g.turn == me.reg_index {
        g.turn = next_live_index(&g.records, me.reg_index);
        notify_all(&g);
    }
    // Phase 2 — acquire: wait until the turn is mine. A turn-holder that
    // died or left the ring while I wait is recovered from: the turn is
    // claimed onward from me (this is what makes a finished scheduler
    // phase re-usable by the next one — the same way apply_coop_seed
    // pins a fresh ring's start in generated programs).
    loop {
        chaos();
        if g.turn == me.reg_index {
            break;
        }
        let holder = &g.records[g.turn];
        if holder.state.load(Ordering::Acquire) == STATE_EXITED
            || !holder.in_ring.load(Ordering::Acquire)
            // A monitor-parked holder cannot use the turn: its pair-mate,
            // who would notify it, may itself be waiting for the turn —
            // grant onward so the ring keeps moving (T17 race fix). The
            // holder re-acquires through monitor_wake_prep's yield_point
            // when its notify arrives.
            // Scan origin is the HOLDER, not me: skipping from `me`
            // deterministically re-selects the same monitor-parked
            // candidate and spins forever while holding the SCHED lock
            // (found by the coop stress round itself — the freeze was
            // total: even the stall-dump bystander blocked on
            // lock_sched). From the holder, every skip strictly
            // progresses around the ring and terminates on a holder
            // that can actually act (a waiter, or me).
            || (holder.state.load(Ordering::Acquire) == STATE_PARKED
                && holder.in_monitor_wait.load(Ordering::Acquire))
        {
            g.turn = next_live_index(&g.records, g.turn);
            notify_all(&g);
            continue;
        }
        me.state.store(STATE_PARKED, Ordering::Release);
        g = SCHED_CV.wait(g).unwrap_or_else(|e| e.into_inner());
    }
    me.set_running();
}

// ------------------------------------------------- monitor park (Wave 3)
/// Prepares a monitor park (M4 Wave 3, T10). Called by `crate::monitor`
/// while holding the monitor's `inner` guard — safe ONLY because this
/// never waits: it flips the state to `Parked` (the collector stops
/// waiting for the thread) and, in coop mode, rotates the turn away so
/// the monitor's owner can run and release. A blocked thread that held
/// the turn would deadlock the ring: the owner waits for the turn, the
/// holder waits for the owner's release.
pub(crate) fn monitor_park_prep(rec: &Arc<ThreadRecord>) {
    chaos();
    sched_trace("park", rec.reg_index);
    rec.in_monitor_wait.store(true, Ordering::Release);
    rec.state.store(STATE_PARKED, Ordering::Release);
    if coop_mode() && rec.in_ring.load(Ordering::Acquire) {
        let mut g = lock_sched();
        if g.turn == rec.reg_index {
            g.turn = next_live_index(&g.records, rec.reg_index);
        }
        notify_all(&g);
    } else {
        // Free mode: a park is a safepoint-equivalent arrival. The STW
        // collector's quiescence loop SLEEPS on the SCHED condvar — a
        // state flip without a notify is invisible to it, and a
        // monitor/clinit-parking thread (which will never reach a
        // safepoint or a backedge poll while parked) made collections
        // wait forever (the second half of the T17.1 freeze: the
        // clinit-gate fix alone still froze ~3/4 debug runs because the
        // collector slept through the waiters' flips). The scheduler's
        // own law is "every state flip notifies" — comply in free mode
        // too.
        let g = lock_sched();
        notify_all(&g);
    }
}
/// Completes a monitor wake (M4 Wave 3, T10): called by `crate::monitor`
/// with NO runtime lock held (L7) when a thread leaves a monitor park.
/// Absorbs an in-flight STW (`safepoint_poll`), rejoins the coop ring
/// (`yield_point` — pass-if-held, then wait for the turn), and marks the
/// thread `Running`.
pub(crate) fn monitor_wake_prep(rec: &Arc<ThreadRecord>) {
    chaos();
    sched_trace("wake", rec.reg_index);
    rec.in_monitor_wait.store(false, Ordering::Release);
    safepoint_poll();
    yield_point();
    rec.set_running();
}

// ------------------------------------------------------- STW (T08)
/// Parks the current thread while a stop-the-world collection is in
/// flight. Called first thing in `gc_point` — *before* any runtime lock —
/// so a parked thread's frame snapshot is the one its generated code just
/// refreshed (`refresh; gc_point;` emission order guarantees it), and no
/// runtime lock is ever held across the park (L6).
pub(crate) fn safepoint_poll() {
    if !GC_REQUESTED.load(Ordering::Acquire) {
        return;
    }
    chaos();
    let me = current_record();
    stw_trace("sfp-enter", me.reg_index);
    me.state.store(STATE_PARKED, Ordering::Release);
    let mut g = lock_sched();
    notify_all(&g); // the collector may be waiting for exactly this
    while GC_REQUESTED.load(Ordering::Acquire) {
        g = SCHED_CV.wait(g).unwrap_or_else(|e| e.into_inner());
    }
    stw_trace("sfp-exit", me.reg_index);
    me.set_running();
}
/// RAII stop-the-world token (T08). `begin` publishes the GC request and
/// waits for every *other* record to leave `Running`; `drop` clears the
/// request and releases everyone. Returns `None` when a collection is
/// already in flight (defensive; the collect token serializes the normal
/// path) — the caller then parks through it via `safepoint_poll`.
pub struct StwGuard {
    _token: std::sync::MutexGuard<'static, ()>,
}
static COLLECT_TOKEN: Mutex<()> = Mutex::new(());
impl StwGuard {
    pub fn begin() -> Option<StwGuard> {
        // try_lock, NOT lock: a thread blocked on this token would be
        // invisible to the wait loop below (it reaches no poll while
        // blocked) — collector waits for blocker, blocker waits for token:
        // a cycle. Losers park at a poll instead (L6), where the owner's
        // release reaches them.
        let token = match COLLECT_TOKEN.try_lock() {
            Ok(t) => t,
            Err(_) => {
                safepoint_poll();
                return None;
            }
        };
        if GC_REQUESTED.swap(true, Ordering::AcqRel) {
            // A collection is already in flight: park exactly like any
            // other safepoint arrival until its owner releases us.
            drop(token);
            safepoint_poll();
            return None;
        }
        // Publish the request; wait for every other thread to stop mutating.
        // Only STARTED JAVA THREADS (ring participants) take part in the
        // quiescence handshake (M4 Wave 3): an auto-registered bystander
        // (test harness thread, embedder helper) never polls, so waiting
        // for it would hang the collector forever. Bystanders are still
        // safe: every heap mutation they make goes through the heap lock,
        // which the collector holds across mark+sweep, allocations are
        // born-survivor-marked under that lock, and their frame slots are
        // still scanned below.
        let me_index = current_record().reg_index;
        stw_trace("begin", me_index);
        let trace_on = std::env::var("J2R_STW_TRACE").is_ok();
        let mut g = lock_sched();
        loop {
            let pending = g.records.iter().any(|r| {
                r.reg_index != me_index
                    && r.in_ring.load(Ordering::Acquire)
                    && r.state.load(Ordering::Acquire) == STATE_RUNNING
            });
            if !pending {
                break;
            }
            if trace_on {
                let states: Vec<String> = g
                    .records
                    .iter()
                    .map(|r| {
                        format!(
                            "{}:{}{}#{}",
                            r.reg_index,
                            r.state.load(Ordering::Acquire),
                            if r.in_ring.load(Ordering::Acquire) {
                                "r"
                            } else {
                                "-"
                            },
                            r.mon_ops.load(Ordering::Relaxed)
                        )
                    })
                    .collect();
                eprintln!(
                    "stw wait me={} cls_owner={} [{}]",
                    me_index,
                    crate::clinit::STATES_OWNER.load(Ordering::Relaxed),
                    states.join(" ")
                );
            }
            notify_all(&g);
            g = SCHED_CV.wait(g).unwrap_or_else(|e| e.into_inner());
        }
        stw_trace("quiescent", me_index);
        Some(StwGuard { _token: token })
    }
}
impl Drop for StwGuard {
    fn drop(&mut self) {
        chaos();
        stw_trace("release", 0);
        GC_REQUESTED.store(false, Ordering::Release);
        // Release every GC-parked thread. Coop-turn waiters are not
        // released by GC — they are granted turns by rotation; their wake
        // predicate (`turn == me`) absorbs this wakeup.
        let g = lock_sched();
        notify_all(&g);
    }
}

// ------------------------------------------------- thread body + death
