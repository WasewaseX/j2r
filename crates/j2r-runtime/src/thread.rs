//! Java threads — M4 Wave 2 (T06–T09, `docs/M4_PLAN.md` §3 Wave 2).
//!
//! This module turns the single-threaded runtime into a real
//! thread runtime:
//!
//! * **T06 — Thread natives.** `java/lang/Thread` exists as a builtin
//!   class (5 slots: runnable, name, daemon, started, tid). `start`
//!   spawns a native thread per Java thread; `join`/`sleep`/`yield`/
//!   `currentThread`/`setDaemon`/`isDaemon`/`getName`/`setName`/
//!   `isAlive`/`activeCount`/`run` follow the JDK contract (exact
//!   exception messages verified against Temurin 21.0.13 by fixtures).
//!   Thread *subclassing* is not supported (loud compile error, as with
//!   every unknown builtin shape); `Runnable` (lambda or compiled class)
//!   is the supported body.
//! * **T07 — thread registry.** Every thread that touches Java state
//!   auto-registers a [`ThreadRecord`]: its per-thread frame registry
//!   ([`crate::frame`]), its state machine, and (lazily) its Java
//!   `Thread` object. The collector scans every record (STW — T08), so
//!   registration is the rooting story for cross-thread state.
//! * **T08 — stop-the-world protocol.** [`GC_REQUESTED`] plus the
//!   [`ThreadRecord`] state machine implement cooperative stop-the-world:
//!   the initiating thread ([`StwGuard::begin`]) publishes the request and
//!   waits until every other record is `Parked` or `Exited`; collection
//!   then runs with no concurrent mutator; `drop` releases everyone.
//!   Threads park at `gc_point` polls (the generated `refresh; gc_point;`
//!   sequence means a parked thread's snapshot is exact by construction).
//!   An allocation that races the request window is born-survivor-marked
//!   by `heap::alloc` (heap module docs) — a fresh handle can never name a
//!   swept object.
//! * **T09 — cooperative scheduler.** `J2R_SCHED=coop` makes thread
//!   switches happen *only* at yield points (`gc_point`, `start`/`join`/
//!   `sleep`/`yield`; monitor ops join in Wave 3), round-robin in
//!   registration order with `J2R_SCHED_SEED` choosing the first turn. In
//!   coop mode a program's output is a pure function of (program, seed),
//!   which is what makes differential testing of threads meaningful
//!   (M4_PLAN §4). Default (`free`) mode is the deployment shape: no
//!   scheduler, OS scheduling.
//!
//! ## Honesty boundaries (same discipline as every milestone)
//!
//! * A thread inside a long native (or a pure-compute loop with no call
//!   and no allocation) does not reach a yield point and delays GC — the
//!   same granularity the plan documents for the JVM's own safepoint
//!   model. Coop mode cannot expose races that need interleaving inside
//!   one non-yielding native call; natives are single-owned (heap lock),
//!   so their internals are not racy (M4_PLAN §4).
//! * `interrupt` and daemon-shutdown edge cases are Wave 4 (T14/T15);
//!   `join`/`sleep` cannot be interrupted yet (documented gap).
//! * Uncaught child exceptions print the JDK line
//!   (`Exception in thread "name" ...`) and kill the thread only — the
//!   process exit code stays the main thread's, like the JVM. Frame lines
//!   are not rendered (no `LineNumberTable` yet), matching the harness
//!   normalization used since M1.
//! * When the Java `main` returns, the process joins all non-daemon
//!   threads first ([`await_non_daemons`]) — the basic JVM shutdown rule;
//!   T15 owns the remaining registry edge cases.
//! * Coop `sleep` is a yield point, not a wall-clock contract: coop mode
//!   is a test mode; timing behaviour is asserted only in free mode.
//!
//! ## Lock law additions (heap.rs L1–L3 extended)
//!
//! * **L4 — slot mutexes are leaves.** A frame-registry slot mutex is
//!   never held across another lock acquisition, a park, or user-code
//!   dispatch.
//! * **L5 — registry order is registry→slots.** The scheduler registry
//!   mutex may be taken while holding nothing else; a record's slot mutex
//!   is only ever taken alone (L4). Nothing takes the registry lock while
//!   holding a slot mutex.
//! * **L6 — never park holding a runtime lock** (M4_PLAN R2): both park
//!   sites (`safepoint_poll` and coop turn waits) run before any runtime
//!   lock acquisition in their call paths, and thread death notifies the
//!   registry without holding any other lock.

use crate::heap::JRef;
use crate::internal_error;
use crate::{throw_new, Throw, Value};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Instant;

// ----------------------------------------------------------------- states
/// Record states. `Running` = may mutate frame slots or the heap;
/// `Parked` = at a safepoint or coop turn wait (state frozen, scan-safe);
/// `Exited` = dead, registry root for its Java object only.
pub const STATE_RUNNING: u8 = 0;
pub const STATE_PARKED: u8 = 1;
pub const STATE_EXITED: u8 = 2;

// ------------------------------------------------------- global GC request
/// Stop-the-world request flag (T08). Published by the collecting thread
/// inside [`StwGuard::begin`]; observed by every thread at its next
/// `gc_point` poll (`safepoint_poll`) and, for allocations that race the
/// request window, by `heap::alloc` under the heap lock (born-survivor
/// marking — see the heap module docs).
pub static GC_REQUESTED: AtomicBool = AtomicBool::new(false);

// ------------------------------------------------------------ thread record
/// One registered Java thread. Shared via `Arc`: the owning thread reaches
/// it through its thread-local handle, the collector through the global
/// registry.
pub struct ThreadRecord {
    /// Registry identity (immutable after registration); also the Java
    /// thread id stamped into the object's slot 4 by `start`.
    pub reg_index: usize,
    /// Java-visible name. Behind a mutex because `setName` mutates it.
    pub name: Mutex<String>,
    /// Daemon flag (`setDaemon` before `start`).
    pub daemon: AtomicBool,
    /// Registry-side alive flag: set when the child adopts the record,
    /// cleared exactly once at death.
    pub alive: AtomicBool,
    /// The state machine above.
    pub state: AtomicU8,
    /// Coop-ring participation (T09): only started Java threads rotate the
    /// turn. Threads that merely registered (test threads, native helpers)
    /// are invisible to the scheduler — otherwise a blocked non-participant
    /// could own the turn and starve the ring.
    pub in_ring: AtomicBool,
    /// The thread's frame-registry slot storage (T07 — the Wave-1
    /// thread_local state, now one instance per Java thread, reachable by
    /// the collector). See `crate::frame` for the slot protocol.
    pub slots: Mutex<frame_slots::SlotState>,
    /// Lazily created Java `java/lang/Thread` object for
    /// `Thread.currentThread()`; a registry root while present.
    pub java_object: Mutex<Option<JRef>>,
    /// Interrupt status (T14, Wave 4). The JDK-exact story (pinned on
    /// Temurin 21.0.13 with InterruptProbe/EdgeProbe):
    /// * `interrupt()` sets the flag (records AND the object's slot 5, so
    ///   a never-started or already-dead thread's flag is observable);
    /// * `sleep`/`join`/`wait` deliver `InterruptedException` and CLEAR
    ///   the flag ("sleep interrupted" / message-less for join and wait);
    /// * a pre-set flag makes the next blocking call throw immediately;
    /// * monitor-enter ignores it (the flag stays set after entering);
    /// * `isInterrupted` reads without clearing, `interrupted` (static)
    ///   reads AND clears.
    pub interrupted: AtomicBool,
    /// True while this thread is parked INSIDE a monitor wait (T10/T17):
    /// it cannot use the coop turn even if the rotation points at it —
    /// its pair-mate, who would notify it, may itself be waiting for the
    /// turn, which is a ring deadlock (found by the T17 coop stress and
    /// identical in shape to the CI-timing race the gc-probe localized).
    /// The turn-grant loop skips such holders; the flag is cleared by
    /// monitor_wake_prep, whose yield_point re-acquires the turn after
    /// the wake.
    pub in_monitor_wait: AtomicBool,
    /// Debug progress probe (T17.1): bumped on every monitor enter/exit —
    /// a thread whose counter freezes is stuck inside one runtime call;
    /// a thread whose counter grows is looping but missing its polls.
    pub mon_ops: AtomicU64,
    /// The per-thread wake channel (T14). `interrupt()` delivers through
    /// it so a thread parked on a monitor wait set (or on `sleep`/`join`,
    /// which also poll their record flag at each wake) wakes instantly.
    /// `notify`/`notifyAll` deliver through the SAME channel for monitor
    /// waiters: the flag is only a wake event — the meaning is decided by
    /// re-checking the wait set (`in_set`) and `interrupted` after waking.
    /// Leaf lock: nothing takes `wakeup_mu` while holding another runtime
    /// lock except under the monitor lock, and nothing waits on any other
    /// lock while holding `wakeup_mu`.
    pub wakeup_mu: Mutex<bool>,
    pub wakeup_cv: Condvar,
}
impl ThreadRecord {
    /// The RUNNING transition the scheduler hand-offs make (Wave 3): one
    /// spelling for the state store, never a bare atomic at the call site.
    fn set_running(&self) {
        self.state.store(STATE_RUNNING, Ordering::Release);
    }

    fn new(reg_index: usize, name: &str, running: bool, in_ring: bool) -> Self {
        ThreadRecord {
            reg_index,
            name: Mutex::new(name.to_string()),
            daemon: AtomicBool::new(false),
            alive: AtomicBool::new(false),
            state: AtomicU8::new(if running { STATE_RUNNING } else { STATE_PARKED }),
            in_ring: AtomicBool::new(in_ring),
            slots: Mutex::new(frame_slots::SlotState::new()),
            java_object: Mutex::new(None),
            interrupted: AtomicBool::new(false),
            in_monitor_wait: AtomicBool::new(false),
            mon_ops: AtomicU64::new(0),
            wakeup_mu: Mutex::new(false),
            wakeup_cv: Condvar::new(),
        }
    }

    /// Delivers a wake event through the ticket channel (T14). Used by
    /// `interrupt()` and by `notify`/`notifyAll` for each removed waiter.
    pub(crate) fn deliver_wakeup(&self) {
        chaos();
        let mut flag = self.wakeup_mu.lock().unwrap_or_else(|e| e.into_inner());
        *flag = true;
        self.wakeup_cv.notify_all();
    }

    /// Parks on the ticket channel until a wake event (or the deadline).
    /// The caller re-derives the MEANING afterwards (wait-set membership,
    /// interrupt flag, deadline) — this is a wake primitive, not policy.
    pub(crate) fn ticket_wait(&self, deadline: Option<Instant>) {
        let mut flag = self.wakeup_mu.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if *flag {
                *flag = false; // consume exactly one wake event
                return;
            }
            flag = match deadline {
                Some(d) => {
                    let now = Instant::now();
                    if now >= d {
                        return; // deadline reached: caller re-checks it
                    }
                    self.wakeup_cv
                        .wait_timeout(flag, d - now)
                        .unwrap_or_else(|e| e.into_inner())
                        .0
                }
                None => self.wakeup_cv.wait(flag).unwrap_or_else(|e| e.into_inner()),
            };
        }
    }
}

// -------------------------------------------------------- global registry
struct Sched {
    records: Vec<Arc<ThreadRecord>>,
    /// Coop round-robin turn (index into `records`).
    turn: usize,
}
static SCHED: Mutex<Sched> = Mutex::new(Sched {
    records: Vec::new(),
    turn: 0,
});
/// The one condvar for every wait in the module: GC park/release, coop
/// turn grants, `join`, non-daemon shutdown. Every state flip notifies.
static SCHED_CV: Condvar = Condvar::new();
fn lock_sched() -> std::sync::MutexGuard<'static, Sched> {
    SCHED.lock().unwrap_or_else(|e| e.into_inner())
}
fn notify_all(_: &std::sync::MutexGuard<'_, Sched>) {
    SCHED_CV.notify_all();
}
fn sched_trace(what: &str, tid: usize) {
    if std::env::var("J2R_SCHED_TRACE").is_ok() {
        let g = lock_sched();
        eprintln!(
            "sched {} tid{} turn={} holder_state={}",
            what,
            tid,
            g.turn,
            g.records
                .get(g.turn)
                .map(|h| h.state.load(Ordering::Acquire))
                .unwrap_or(9)
        );
    }
}
/// STW handshake trace (J2R_STW_TRACE): the observability twin of
/// sched_trace for the collector protocol — safepoint arrivals/exits,
/// quiescence waits, and releases. Debugging instrument, one Acquire
/// load when unset.
fn stw_trace(what: &str, tid: usize) {
    if std::env::var("J2R_STW_TRACE").is_ok() {
        eprintln!("stw {} tid{}", what, tid);
    }
}
/// Snapshot of the registry for the collector (T08) — used by
/// `frame::for_each_live_slot` to scan every thread's frame slots.
pub(crate) fn registry_records() -> Vec<Arc<ThreadRecord>> {
    lock_sched().records.clone()
}

// --------------------------------------------------- per-thread handle

thread_local! {
    // M4-SEAM (M4_PLAN §3 Wave 2 / T07): this is a *handle*, not state —
    // the thread's own `Arc<ThreadRecord>`. The state it points to lives
    // in the global registry and is scan-safe by construction. frame.rs
    // keeps two owner-only scalar thread_locals (depth/limit); no other
    // thread_locals exist in the crate (gate-12 census).
    static CURRENT: std::cell::RefCell<Option<Arc<ThreadRecord>>> =
        const { std::cell::RefCell::new(None) };
}
/// The per-thread frame-slot state, extracted from the Wave-1 thread_local
/// (frame.rs) so the collector can scan every thread's registry (T08).
/// Same LIFO + pool protocol as before; now behind one mutex per record.
pub(crate) mod frame_slots {
    #[derive(Default)]
    pub struct SlotState {
        pub slots: Vec<Option<Vec<u32>>>,
        pub free: Vec<usize>,
        pub pool: Vec<Vec<u32>>,
    }

    impl SlotState {
        pub fn new() -> Self {
            SlotState::default()
        }
    }
}

// ------------------------------------------------------------------ tests

// --- scanaislop split: the items below live verbatim in the child modules ---
mod lifecycle;
mod native;
mod sched;
#[cfg(test)]
mod tests;
pub use lifecycle::*;
pub use native::*;
pub use sched::*;
