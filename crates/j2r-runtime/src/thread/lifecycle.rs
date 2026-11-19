use super::*;

/// Testing instrument, never a runtime feature: `J2R_CHAOS_US` (0-2000,
/// default 0) injects a randomized micro-sleep at the concurrency
/// protocol seams below, widening the race windows that only CI's slow
/// runners hit naturally (gc-probe evidence: MonitorContention 2/2,
/// WaitNotify 1/2 hangs at a collection per allocation on 2-vCPU). The
/// stress suites (T17) run with chaos enabled; generated programs never
/// set it, and the cost when unset is one Acquire load per seam.
static CHAOS_US: AtomicU32 = AtomicU32::new(0);
static CHAOS_INIT: AtomicBool = AtomicBool::new(false);
static CHAOS_SEED: AtomicU32 = AtomicU32::new(0x9E3779B9);
fn chaos_us() -> u64 {
    if !CHAOS_INIT.load(Ordering::Acquire) {
        let v: u64 = std::env::var("J2R_CHAOS_US")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
            .min(2000);
        CHAOS_US.store(v as u32, Ordering::Release);
        CHAOS_INIT.store(true, Ordering::Release);
    }
    CHAOS_US.load(Ordering::Acquire) as u64
}
/// Test-only forcing: set the chaos budget directly (bypasses the env
/// read, which is cached process-wide and racy under parallel tests).
#[cfg(test)]
pub(crate) fn chaos_set(us: u32) {
    CHAOS_INIT.store(true, Ordering::Release);
    CHAOS_US.store(us.min(2000), Ordering::Release);
}
/// A randomized micro-sleep (xorshift, lock-free) — the wider-window
/// stand-in for CI's slow scheduler.
pub(crate) fn chaos() {
    let us = chaos_us();
    if us == 0 {
        return;
    }
    let mut x = CHAOS_SEED.load(Ordering::Relaxed);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    CHAOS_SEED.store(x, Ordering::Relaxed);
    let roll = (x % (us as u32 + 1)) as u64;
    if roll > 0 {
        std::thread::sleep(std::time::Duration::from_micros(roll));
    }
}

// ------------------------------------------------- shutdown + roots
/// Joins every registered non-daemon thread still alive. Called by the
/// generated `main_run` right before returning its exit code — the basic
/// JVM shutdown rule (T15 owns the registry edge cases).
pub fn await_non_daemons() {
    loop {
        let me = current_record().reg_index;
        let targets: Vec<Arc<ThreadRecord>> = {
            let g = lock_sched();
            g.records
                .iter()
                .filter(|t| {
                    // Only STARTED JAVA threads take part in shutdown:
                    // ring membership is what `start` grants (T09), so a
                    // bystander (embedder helper, test harness thread) is
                    // not a Java thread and must never block the JVM exit
                    // — and in a unit-test registry it never exits.
                    t.reg_index != me
                        && t.in_ring.load(Ordering::Acquire)
                        && !t.daemon.load(Ordering::Acquire)
                        && t.state.load(Ordering::Acquire) != STATE_EXITED
                })
                .cloned()
                .collect()
        };
        if targets.is_empty() {
            return;
        }
        for t in targets {
            join_record(&t);
        }
    }
}
fn join_record(t: &Arc<ThreadRecord>) {
    if coop_mode() {
        while t.state.load(Ordering::Acquire) != STATE_EXITED {
            yield_point();
        }
        return;
    }
    // Same Parked discipline as `n_thread_join`: a shutdown-joining thread
    // mutates no Java state while it waits.
    let me = current_record();
    me.state.store(STATE_PARKED, Ordering::Release);
    let mut g = lock_sched();
    notify_all(&g);
    while t.state.load(Ordering::Acquire) != STATE_EXITED {
        g = SCHED_CV.wait(g).unwrap_or_else(|e| e.into_inner());
    }
    me.set_running();
}
/// Collector hook (T08): the registry's own roots — every record's Java
/// `Thread` object. A thread's *frame* slots are scanned separately via
/// `frame::for_each_live_slot`, which walks this same registry.
pub(crate) fn scan_registry_roots(out: &mut Vec<u32>) {
    for t in lock_sched().records.iter() {
        if let Some(r) = t
            .java_object
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            out.push(r.index());
        }
    }
}
/// Test-only hooks: a thread waiting for (or holding between operations
/// of) the test-serialization lock cannot mutate Java state while it
/// waits — marking it `Parked` is the truthful state and keeps the STW
/// collector from waiting on a lock-chain (collector → lock holder →
/// lock waiter → collector).
#[cfg(test)]
pub(crate) fn mark_test_lock_wait() {
    let rec = current_record();
    rec.state.store(STATE_PARKED, Ordering::Release);
    let g = lock_sched();
    notify_all(&g);
}
#[cfg(test)]
pub(crate) fn mark_test_lock_acquired() {
    current_record()
        .state
        .store(STATE_RUNNING, Ordering::Release);
}
/// Test-only: the thread released the test-serialization lock (guard
/// dropped) — it stopped mutating Java state, so `Parked` is the
/// truthful state. Without this, libtest's per-test threads die with
/// `Running` records and every later STW begin waits for the dead.
#[cfg(test)]
pub(crate) fn mark_test_lock_released() {
    current_record()
        .state
        .store(STATE_PARKED, Ordering::Release);
    let g = lock_sched();
    notify_all(&g);
}

// ------------------------------------------------- frame slot storage (T07)
