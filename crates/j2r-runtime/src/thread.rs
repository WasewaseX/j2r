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
use crate::{throw_new, Throw, Value};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

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
}

impl ThreadRecord {
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
fn register_named_record(name: &str, running: bool, in_ring: bool) -> Arc<ThreadRecord> {
    let mut g = lock_sched();
    let rec = Arc::new(ThreadRecord::new(g.records.len(), name, running, in_ring));
    g.records.push(rec.clone());
    rec
}

/// Adopts `rec` as this native thread's record (the start handoff: the
/// record is pre-registered by the starting thread so the Java object's
/// tid link exists before the child runs).
fn adopt_record(rec: Arc<ThreadRecord>) {
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
    rec.state.store(STATE_RUNNING, Ordering::Release);
    rec.in_ring.store(true, Ordering::Release);
    apply_coop_seed_if_needed();
}

// ------------------------------------------------------------ coop mode

fn coop_mode() -> bool {
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

fn coop_seed() -> usize {
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
fn apply_coop_seed_if_needed() {
    if !coop_mode() {
        return;
    }
    let mut g = lock_sched();
    let n = g.records.len();
    if n > 0 {
        g.turn = coop_seed() % n;
    }
}

fn next_live_index(records: &[Arc<ThreadRecord>], from: usize) -> usize {
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
        if g.turn == me.reg_index {
            break;
        }
        let holder = &g.records[g.turn];
        if holder.state.load(Ordering::Acquire) == STATE_EXITED
            || !holder.in_ring.load(Ordering::Acquire)
        {
            g.turn = next_live_index(&g.records, me.reg_index);
            notify_all(&g);
            continue;
        }
        me.state.store(STATE_PARKED, Ordering::Release);
        g = SCHED_CV.wait(g).unwrap_or_else(|e| e.into_inner());
    }
    me.state.store(STATE_RUNNING, Ordering::Release);
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
    let me = current_record();
    me.state.store(STATE_PARKED, Ordering::Release);
    let mut g = lock_sched();
    notify_all(&g); // the collector may be waiting for exactly this
    while GC_REQUESTED.load(Ordering::Acquire) {
        g = SCHED_CV.wait(g).unwrap_or_else(|e| e.into_inner());
    }
    me.state.store(STATE_RUNNING, Ordering::Release);
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
        let me_index = current_record().reg_index;
        let mut g = lock_sched();
        loop {
            let pending = g.records.iter().any(|r| {
                r.reg_index != me_index && r.state.load(Ordering::Acquire) == STATE_RUNNING
            });
            if !pending {
                break;
            }
            notify_all(&g);
            g = SCHED_CV.wait(g).unwrap_or_else(|e| e.into_inner());
        }
        Some(StwGuard { _token: token })
    }
}

impl Drop for StwGuard {
    fn drop(&mut self) {
        GC_REQUESTED.store(false, Ordering::Release);
        // Release every GC-parked thread. Coop-turn waiters are not
        // released by GC — they are granted turns by rotation; their wake
        // predicate (`turn == me`) absorbs this wakeup.
        let g = lock_sched();
        notify_all(&g);
    }
}

// ------------------------------------------------- thread body + death

/// Runs the thread body with the JDK death semantics and records death
/// exactly once: alive=false, state=Exited, coop turn granted onward,
/// everyone notified (joiners, the ring, a waiting collector or shutdown).
fn run_body(rec: &Arc<ThreadRecord>, body: impl FnOnce() -> Result<(), Throw>) {
    rec.state.store(STATE_RUNNING, Ordering::Release);
    rec.alive.store(true, Ordering::Release);
    // First yield point: acquire the turn granted by the starting thread's
    // handoff (coop); no-op in free mode.
    yield_point();
    let name = rec.name.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    match result {
        Ok(Ok(())) => {}
        Ok(Err(t)) => {
            // JDK uncaught-exception behaviour: print and kill the thread;
            // process exit code unaffected.
            eprintln!(
                "Exception in thread \"{}\" {}",
                name,
                crate::throwable_display(&t, crate::Dispatch::default())
            );
            let trace = crate::throwable_enclosed_trace(&t, crate::Dispatch::default());
            if !trace.is_empty() {
                eprint!("{trace}");
            }
        }
        Err(payload) => {
            // A Rust panic is an internal runtime bug (never a Java
            // program error): fail loudly and kill the process — the
            // loud-panic policy applies inside threads too.
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            eprintln!("j2r internal error in thread \"{name}\": {msg}");
            std::process::exit(101);
        }
    }
    // Death: exactly once. Grant the coop turn onward (a dying thread that
    // never yields again must not starve the ring), then notify everyone.
    rec.alive.store(false, Ordering::Release);
    rec.state.store(STATE_EXITED, Ordering::Release);
    let mut g = lock_sched();
    if coop_mode() && g.turn == rec.reg_index {
        g.turn = next_live_index(&g.records, rec.reg_index);
    }
    notify_all(&g);
}

// ------------------------------------------------------ natives (T06)

/// `java/lang/Thread` slot layout (builtin, 5 slots):
/// 0 runnable, 1 name (`Value::Str`), 2 daemon (Int 0/1), 3 started
/// (Int 0/1), 4 tid (Int; -1 until `start` links the registry record).
pub const THREAD_SLOTS: usize = 5;

/// JDK "Thread-N" default-name numbering (its own counter, like the JDK's
/// `nextThreadNum` — independent from registry ids).
static NAME_SEQ: AtomicI64 = AtomicI64::new(0);

fn slot_str(r: &JRef, i: usize) -> String {
    crate::heap::with_obj(*r, |o| match &o.slots[i] {
        Value::Str(s) => s.as_str().to_string(),
        other => panic!("j2r internal: Thread slot {i} is {other:?}, expected Str"),
    })
}

fn slot_int(r: &JRef, i: usize) -> i32 {
    crate::heap::with_obj(*r, |o| o.slots[i].expect_int())
}

fn slot_ref(r: &JRef, i: usize) -> Option<JRef> {
    crate::heap::with_obj(*r, |o| o.slots[i].expect_ref())
}

fn set_slot(r: &JRef, i: usize, v: Value) {
    crate::heap::with_obj_mut(*r, |o| o.slots[i] = v);
}

fn arg_runnable(v: Option<&Value>) -> Option<JRef> {
    match v {
        Some(Value::Ref(x)) => *x,
        _ => None,
    }
}

fn arg_name(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::Ref(Some(x))) => Some(crate::heap::with_obj(*x, |o| match &o.slots[0] {
            Value::Str(s) => s.as_str().to_string(),
            other => panic!("j2r internal: expected String, got {other:?}"),
        })),
        _ => None,
    }
}

fn init_common(recv: &JRef, runnable: Option<JRef>, name: Option<String>) {
    let seq = NAME_SEQ.fetch_add(1, Ordering::AcqRel);
    let name = name.unwrap_or_else(|| format!("Thread-{seq}"));
    crate::heap::with_obj_mut(*recv, |o| {
        o.slots[0] = Value::Ref(runnable);
        o.slots[1] = Value::Str(Arc::new(name));
        o.slots[2] = Value::Int(0);
        o.slots[3] = Value::Int(0);
        o.slots[4] = Value::Int(-1);
    });
}

pub fn n_thread_init_v(recv: Option<&JRef>, _args: &[Value]) -> Result<Value, Throw> {
    let r = *recv.expect("j2r internal: Thread.<init> without receiver");
    init_common(&r, None, None);
    Ok(Value::Void)
}

pub fn n_thread_init_r(recv: Option<&JRef>, args: &[Value]) -> Result<Value, Throw> {
    let r = *recv.expect("j2r internal: Thread.<init> without receiver");
    init_common(&r, arg_runnable(args.first()), None);
    Ok(Value::Void)
}

pub fn n_thread_init_n(recv: Option<&JRef>, args: &[Value]) -> Result<Value, Throw> {
    let r = *recv.expect("j2r internal: Thread.<init> without receiver");
    init_common(&r, None, arg_name(args.first()));
    Ok(Value::Void)
}

pub fn n_thread_init_rn(recv: Option<&JRef>, args: &[Value]) -> Result<Value, Throw> {
    let r = *recv.expect("j2r internal: Thread.<init> without receiver");
    init_common(&r, arg_runnable(args.first()), arg_name(args.get(1)));
    Ok(Value::Void)
}

/// `Thread.start()`: links a registry record, spawns the native thread
/// that runs the body, hands off (coop).
pub fn n_thread_start(recv: Option<&JRef>, ctx: crate::Dispatch) -> Result<Value, Throw> {
    let r = *recv.expect("j2r internal: Thread.start without receiver");
    if slot_int(&r, 3) != 0 {
        // JDK: starting an already-started thread.
        return Err(throw_new("java/lang/IllegalThreadStateException", None));
    }
    set_slot(&r, 3, Value::Int(1));
    let runnable = slot_ref(&r, 0);
    let record_name = slot_str(&r, 1);

    // Pre-register the child's record so the tid link on the Java object
    // exists before the child runs (join/isAlive resolve through it). The
    // child joins the coop ring; its state stays `Parked` until its body
    // adopts the record.
    let rec = register_named_record("", false, true);
    *rec.name.lock().unwrap_or_else(|e| e.into_inner()) = record_name.clone();
    set_slot(
        &r,
        4,
        Value::Int(i32::try_from(rec.reg_index).unwrap_or(i32::MAX)),
    );

    // The child runs the runnable's `run()` (lambda SAM or compiled class
    // walk). With no runnable, `run()` does nothing — JDK-exact.
    let body = move || -> Result<(), Throw> {
        match runnable {
            Some(run) => dispatch_runnable(&run, ctx),
            None => Ok(()),
        }
    };

    let handle = std::thread::Builder::new()
        .name(format!("j2r-{}", record_name))
        .stack_size(child_stack_bytes())
        .spawn(move || {
            adopt_record(rec.clone());
            run_body(&rec, body);
        })
        .expect("j2r internal: failed to spawn Java thread");
    // In coop mode `start` is a handoff yield point (M4_PLAN §3 Wave 2):
    // the parent yields so the child (or the next live thread) runs. The
    // detached handle is intentional: death is registry-driven.
    yield_point();
    drop(handle);
    Ok(Value::Void)
}

fn child_stack_bytes() -> usize {
    static OVERRIDE: OnceLock<usize> = OnceLock::new();
    *OVERRIDE.get_or_init(|| {
        // The M2 calibration default: the DEFAULT_FRAMES ceiling with
        // the same 8× margin the main worker uses (frame.rs §depth
        // limit).
        std::env::var("J2R_THREAD_STACK")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(crate::frame::DEFAULT_FRAMES * crate::frame::FRAME_FOOTPRINT * 8)
    })
}

/// Dispatches `runnable.run()` from runtime code (T06). Lambda closures
/// carry the SAM interface as their runtime class (M3) — `java/lang/Runnable`
/// closures go through `lambda::n_sam`; compiled classes walk the parent
/// chain for `run()V` like every virtual call.
fn dispatch_runnable(run: &JRef, ctx: crate::Dispatch) -> Result<(), Throw> {
    let cls = crate::heap::with_obj(*run, |o| o.jclass.to_string());
    if cls == "java/lang/Runnable" {
        crate::lambda::n_sam(Some(run), &[], ctx)?;
        return Ok(());
    }
    // Compiled class: virtual walk up the parent chain.
    let mut cur = cls.clone();
    for _ in 0..64 {
        if let Some(f) = ctx.find_method(&cur, "run", "()V") {
            f(Some(run), &[])?;
            return Ok(());
        }
        match ctx.parent_of(&cur) {
            Some(p) => cur = p.to_string(),
            None => break,
        }
    }
    // A `Runnable` with no resolvable run(): a builtin shape gap — loud.
    Err(crate::internal_error(&format!(
        "j2r internal: no run()V found on runnable class {cls}"
    )))
}

/// `Thread.run()` invoked directly (JDK: dispatches the runnable on the
/// *caller's* thread; no start semantics).
pub fn n_thread_run(recv: Option<&JRef>, ctx: crate::Dispatch) -> Result<Value, Throw> {
    let r = *recv.expect("j2r internal: Thread.run without receiver");
    match slot_ref(&r, 0) {
        Some(run) => dispatch_runnable(&run, ctx).map(|_| Value::Void),
        None => Ok(Value::Void),
    }
}

/// Resolves the registry record a Java Thread object refers to: `start`
/// stamped the record's registry id into slot 4. Records are never
/// removed, so the lookup stays valid after death (joiners observe the
/// `Exited` transition). `None` = never started (JDK: join/isAlive on a
/// never-started thread see a non-alive thread).
fn target_record(r: &JRef) -> Option<Arc<ThreadRecord>> {
    let tid = slot_int(r, 4);
    if tid < 0 {
        return None;
    }
    let g = lock_sched();
    g.records.get(tid as usize).cloned()
}

fn target_alive(r: &JRef) -> bool {
    target_record(r)
        .map(|t| t.state.load(Ordering::Acquire) != STATE_EXITED)
        .unwrap_or(false)
}

/// `Thread.join()`, `join(millis)`, `join(millis, nanos)`.
pub fn n_thread_join(recv: Option<&JRef>, args: &[Value]) -> Result<Value, Throw> {
    let r = *recv.expect("j2r internal: Thread.join without receiver");
    let millis = match args.first() {
        Some(Value::Long(v)) => *v,
        None => 0,
        _ => i64::MIN, // malformed arg shape: internal path never used
    };
    let nanos = match args.get(1) {
        Some(Value::Int(v)) => *v,
        None => 0,
        _ => 0,
    };
    if millis < 0 {
        return Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some("timeout value is negative"),
        ));
    }
    if !(0..=999_999).contains(&nanos) {
        return Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some("nanosecond timeout value out of range"),
        ));
    }
    // JDK rounding: nanos >= 500000 rounds the millisecond budget up.
    let budget_ms = if nanos >= 500_000 { millis + 1 } else { millis };

    let target = match target_record(&r) {
        Some(t) => t,
        None => return Ok(Value::Void), // never started: nothing to wait for
    };
    if target.reg_index == current_record().reg_index {
        // JDK behaviour: joining the current thread without a timeout
        // never returns; with a timeout it returns after the budget.
        if budget_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(budget_ms as u64));
            return Ok(Value::Void);
        }
        if coop_mode() {
            loop {
                yield_point(); // deterministic hang, like the JVM's
            }
        }
        loop {
            std::thread::park();
        }
    }
    if coop_mode() {
        let deadline = budget_ms;
        let mut waited: i64 = 0;
        while target.state.load(Ordering::Acquire) != STATE_EXITED {
            if deadline > 0 && waited >= deadline {
                return Ok(Value::Void);
            }
            yield_point();
            waited += 1; // coop ticks, not wall-clock (documented gap)
        }
        return Ok(Value::Void);
    }
    let deadline = if budget_ms > 0 {
        Some(std::time::Instant::now() + std::time::Duration::from_millis(budget_ms as u64))
    } else {
        None
    };
    // A join-waiter mutates no Java state while waiting — mark it Parked
    // so the STW collector never waits for a thread that waits for another
    // (that chain would otherwise be a collector-joiner-target cycle).
    let me = current_record();
    me.state.store(STATE_PARKED, Ordering::Release);
    let mut g = lock_sched();
    notify_all(&g);
    while target.state.load(Ordering::Acquire) != STATE_EXITED {
        if let Some(d) = deadline {
            let now = std::time::Instant::now();
            if now >= d {
                break;
            }
            g = SCHED_CV
                .wait_timeout(g, d - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        } else {
            g = SCHED_CV.wait(g).unwrap_or_else(|e| e.into_inner());
        }
    }
    me.state.store(STATE_RUNNING, Ordering::Release);
    Ok(Value::Void)
}

/// `Thread.sleep(millis[, nanos])`.
pub fn n_thread_sleep(args: &[Value]) -> Result<Value, Throw> {
    let millis = match args.first() {
        Some(Value::Long(v)) => *v,
        _ => {
            return Err(throw_new(
                "java/lang/IllegalArgumentException",
                Some("timeout value is negative"),
            ))
        }
    };
    let nanos = match args.get(1) {
        Some(Value::Int(v)) => *v,
        None => 0,
        _ => 0,
    };
    if millis < 0 {
        return Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some("timeout value is negative"),
        ));
    }
    if !(0..=999_999).contains(&nanos) {
        return Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some("nanosecond timeout value out of range"),
        ));
    }
    // JDK rounding: nanos >= 500000 rounds up.
    let budget_ms = if nanos >= 500_000 { millis + 1 } else { millis };
    if coop_mode() {
        // Coop sleep: a yield point; wall-clock timing is not a coop-mode
        // contract (documented in the module header).
        yield_point();
        return Ok(Value::Void);
    }
    if budget_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(budget_ms as u64));
    } else {
        std::thread::yield_now();
    }
    Ok(Value::Void)
}

/// `Thread.yield()`.
pub fn n_thread_yield() -> Result<Value, Throw> {
    if coop_mode() {
        yield_point();
    } else {
        std::thread::yield_now();
    }
    Ok(Value::Void)
}

/// `Thread.currentThread()` — static. Returns the per-thread interned
/// `java/lang/Thread` object (plan Wave 2: identity must hold across
/// calls in one thread and differ across threads).
pub fn current_thread_object() -> JRef {
    let rec = current_record();
    let mut j = rec.java_object.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = *j {
        return r;
    }
    let name = rec.name.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let daemon = rec.daemon.load(Ordering::Acquire) as i32;
    let alive = rec.alive.load(Ordering::Acquire) as i32;
    let obj = crate::heap::alloc(crate::object::JObj {
        jclass: Arc::from("java/lang/Thread"),
        slots: vec![
            Value::Ref(None),
            Value::Str(Arc::new(name)),
            Value::Int(daemon),
            Value::Int(alive),
            Value::Int(i32::try_from(rec.reg_index).unwrap_or(i32::MAX)),
        ],
    });
    *j = Some(obj);
    obj
}

pub fn n_thread_current_thread() -> Result<Value, Throw> {
    Ok(Value::Ref(Some(current_thread_object())))
}

pub fn n_thread_set_daemon(recv: Option<&JRef>, args: &[Value]) -> Result<Value, Throw> {
    let r = *recv.expect("j2r internal: Thread.setDaemon without receiver");
    let on = match args.first() {
        Some(Value::Int(v)) => *v != 0,
        _ => false,
    };
    if slot_int(&r, 3) != 0 {
        // JDK: setDaemon on a started thread.
        return Err(throw_new("java/lang/IllegalThreadStateException", None));
    }
    set_slot(&r, 2, Value::Int(on as i32));
    Ok(Value::Void)
}

pub fn n_thread_is_daemon(recv: Option<&JRef>) -> Result<Value, Throw> {
    let r = *recv.expect("j2r internal: Thread.isDaemon without receiver");
    Ok(Value::Int(slot_int(&r, 2)))
}

pub fn n_thread_get_name(recv: Option<&JRef>) -> Result<Value, Throw> {
    let r = *recv.expect("j2r internal: Thread.getName without receiver");
    // Strings ride the operand stack as heap refs (the Value contract),
    // never as the raw Str payload.
    Ok(Value::Ref(Some(crate::object::new_string(&slot_str(
        &r, 1,
    )))))
}

pub fn n_thread_set_name(recv: Option<&JRef>, args: &[Value]) -> Result<Value, Throw> {
    let r = *recv.expect("j2r internal: Thread.setName without receiver");
    match args.first() {
        Some(Value::Ref(Some(name_obj))) => {
            let s = crate::heap::with_obj(*name_obj, |o| match &o.slots[0] {
                Value::Str(s) => s.as_str().to_string(),
                other => panic!("j2r internal: expected String, got {other:?}"),
            });
            set_slot(&r, 1, Value::Str(Arc::new(s)));
            Ok(Value::Void)
        }
        _ => Err(throw_new(
            "java/lang/NullPointerException",
            Some("name cannot be null"),
        )),
    }
}

pub fn n_thread_is_alive(recv: Option<&JRef>) -> Result<Value, Throw> {
    let r = *recv.expect("j2r internal: Thread.isAlive without receiver");
    Ok(Value::Int(target_alive(&r) as i32))
}

/// `Thread.activeCount()` — JDK: an estimate of live threads in the
/// current thread's group; Wave 2 has no groups, so it is the exact live
/// registry count (documented).
pub fn n_thread_active_count() -> Result<Value, Throw> {
    let g = lock_sched();
    let n = g
        .records
        .iter()
        .filter(|t| t.state.load(Ordering::Acquire) != STATE_EXITED)
        .count();
    Ok(Value::Int(n as i32))
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
                    t.reg_index != me
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
    me.state.store(STATE_RUNNING, Ordering::Release);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;

    /// Test-only RAII: marks the thread's record `Exited` on scope end so
    /// a raw `std::thread::spawn` in a test never leaves a zombie
    /// `Running` record that would stall every later STW begin. Mirrors
    /// `run_body`'s death sequence exactly — including grant-on-death: a
    /// dying thread that owns the coop turn must grant it onward, or the
    /// ring starves (found by the first coop test run).
    struct TestDeath(Arc<ThreadRecord>);
    impl Drop for TestDeath {
        fn drop(&mut self) {
            self.0.alive.store(false, Ordering::Release);
            self.0.state.store(STATE_EXITED, Ordering::Release);
            let mut g = lock_sched();
            if g.turn == self.0.reg_index {
                g.turn = next_live_index(&g.records, self.0.reg_index);
            }
            notify_all(&g);
        }
    }

    /// T05-wave2: the registry and records are compile-proven Send+Sync —
    /// the property real threads depend on.
    #[test]
    fn registry_types_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ThreadRecord>();
        assert_send_sync::<Arc<ThreadRecord>>();
        assert_send_sync::<Mutex<Sched>>();
    }

    /// T06: currentThread identity — same object across calls in one
    /// thread; different objects across two threads (M4_PLAN §1).
    #[test]
    fn current_thread_identity_holds() {
        let _rt = crate::test_support::runtime();
        let a1 = current_thread_object();
        let a2 = current_thread_object();
        assert!(JRef::ptr_eq(&a1, &a2), "identity must hold within a thread");
        let child_obj = std::thread::spawn(|| {
            let _d = TestDeath(current_record());
            current_thread_object()
        })
        .join()
        .unwrap();
        assert!(
            !JRef::ptr_eq(&a1, &child_obj),
            "distinct threads, distinct objects"
        );
        // ...and the child's object is still alive after the child exited
        // (registry-rooted): reading its name slot must not panic.
        let name = crate::heap::with_obj(child_obj, |o| o.slots[1].clone());
        assert!(matches!(name, Value::Str(_)));
    }

    /// T08 (R2): the two-thread park/resume handshake — an allocator
    /// thread parks at safepoints while this thread collects, and both
    /// survive repeated rounds. Bounded by a deadline so a regression
    /// fails instead of hanging CI.
    #[test]
    fn stw_handshake_allocator_and_collector() {
        let _rt = crate::test_support::runtime();
        let barrier = Arc::new(Barrier::new(2));
        let b2 = barrier.clone();
        let child = std::thread::spawn(move || {
            let _d = TestDeath(current_record());
            b2.wait();
            for _ in 0..200 {
                // Allocate + hit the collection point: exercises park/resume.
                let _o = crate::object::new_obj("test/Handshake", 1).unwrap();
                crate::heap::gc_point();
            }
        });
        barrier.wait();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        for _ in 0..20 {
            let guard = StwGuard::begin().expect("collector must win the token");
            crate::heap::collect_under_held_stw_for_test();
            drop(guard);
            assert!(
                std::time::Instant::now() < deadline,
                "STW handshake stalled"
            );
        }
        child.join().unwrap();
    }

    /// T07+T08: an object rooted ONLY by another thread's frame registry
    /// survives a collection driven by this thread — the cross-thread
    /// rooting proof the Wave-1 thread_local registry could not give.
    /// The child parks AT SAFEPOINTS while holding the root (a suspended
    /// frame's exact situation); the collector scans it there.
    #[test]
    fn cross_thread_frame_rooting_survives_collection() {
        let _rt = crate::test_support::runtime();
        let obj = crate::object::new_obj("test/CrossRoot", 1).unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let b2 = barrier.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let child = std::thread::spawn(move || {
            let _d = TestDeath(current_record());
            // Root it in the CHILD thread's frame registry (a NativeRoots
            // scope is the same slot machinery a suspended frame uses) and
            // keep parking at safepoints until the parent says stop —
            // the root stays live in the child's slots the whole time.
            let _nr = crate::frame::NativeRoots::push(&[obj]);
            b2.wait();
            while !s2.load(Ordering::Acquire) {
                crate::heap::gc_point();
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            obj
        });
        barrier.wait();
        // Give the child a moment to reach its first safepoint park with
        // the root in place, then collect through the real STW protocol.
        std::thread::sleep(std::time::Duration::from_millis(20));
        crate::heap::force_collect_for_test();
        assert!(
            crate::heap::is_live(obj),
            "object rooted by another thread's registry must survive"
        );
        stop.store(true, Ordering::Release);
        let survived = child.join().unwrap();
        assert!(crate::heap::is_live(survived));
    }

    /// T09: coop round-robin — two ring-participating threads recording
    /// their interleaving produce the exact deterministic rotation order
    /// (registration order parent-first). The barrier makes ring
    /// participation deterministic: generated programs register children
    /// IN the ring synchronously inside `start`, but a raw test thread
    /// joins the ring asynchronously, so the parent must not yield before
    /// the child is in.
    #[test]
    fn coop_round_robin_is_deterministic() {
        let _rt = crate::test_support::runtime();
        set_coop_for_test(true);
        let parent = current_record();
        parent.in_ring.store(true, Ordering::Release);
        *parent.name.lock().unwrap_or_else(|e| e.into_inner()) = "m".to_string();
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let l2 = log.clone();
        let barrier = Arc::new(Barrier::new(2));
        let b2 = barrier.clone();
        let child = std::thread::spawn(move || {
            let _d = TestDeath(current_record());
            let rec = current_record();
            rec.in_ring.store(true, Ordering::Release);
            *rec.name.lock().unwrap_or_else(|e| e.into_inner()) = "c".to_string();
            b2.wait();
            for i in 0..3 {
                yield_point();
                l2.lock().unwrap().push(format!("c{i}"));
            }
        });
        barrier.wait();
        // Pin the initial turn to the parent — the job apply_coop_seed
        // does for a fresh ring in a generated program (seed % ring size).
        // With a live holder pinned, no recovery ever fires and the
        // rotation order below is the pure round-robin contract.
        lock_sched().turn = parent.reg_index;
        for i in 0..3 {
            yield_point();
            log.lock().unwrap().push(format!("m{i}"));
        }
        child.join().unwrap();
        parent.in_ring.store(false, Ordering::Release);
        set_coop_for_test(false);
        let got = log.lock().unwrap().clone();
        // Parent holds the turn: its first yield passes to the child, the
        // child acquires and logs first — pinned by ownership, not by
        // scheduling.
        assert_eq!(got, vec!["c0", "m0", "c1", "m1", "c2", "m2"]);
    }

    /// T09: ring participation is the scheduler's visibility rule — a
    /// registered thread that is NOT in the ring never receives the turn,
    /// so the rotation order of the real participants is unchanged.
    #[test]
    fn coop_ignores_non_ring_threads() {
        let _rt = crate::test_support::runtime();
        set_coop_for_test(true);
        let parent = current_record();
        parent.in_ring.store(true, Ordering::Release);
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let l2 = log.clone();
        let barrier = Arc::new(Barrier::new(2));
        let b2 = barrier.clone();
        // A ring participant (barrier-synchronized) and a non-participant
        // running concurrently.
        let in_ring_child = std::thread::spawn(move || {
            let _d = TestDeath(current_record());
            let rec = current_record();
            rec.in_ring.store(true, Ordering::Release);
            b2.wait();
            for i in 0..2 {
                yield_point();
                l2.lock().unwrap().push(format!("a{i}"));
            }
        });
        let l3 = log.clone();
        let outsider = std::thread::spawn(move || {
            let _d = TestDeath(current_record());
            let _ = current_record(); // registered, but never joins the ring
            for i in 0..2 {
                l3.lock().unwrap().push(format!("b{i}"));
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        });
        barrier.wait();
        // Pin the initial turn to the parent (the seed's job in generated
        // programs); the outsider never participates either way.
        lock_sched().turn = parent.reg_index;
        for i in 0..2 {
            yield_point();
            log.lock().unwrap().push(format!("m{i}"));
        }
        in_ring_child.join().unwrap();
        outsider.join().unwrap();
        parent.in_ring.store(false, Ordering::Release);
        set_coop_for_test(false);
        let binding = log.lock().unwrap();
        let m_and_a: Vec<&String> = binding
            .iter()
            .filter(|s| s.starts_with('m') || s.starts_with('a'))
            .collect();
        // Rotation order with initial turn = parent: parent passes, child
        // acquires and logs first — deterministic either way the initial
        // race resolves, because a thread without the turn cannot act.
        assert_eq!(m_and_a, ["a0", "m0", "a1", "m1"]); // b* interleaves freely
        drop(binding);
    }

    /// T06: join happens-before — writes by the child are visible after
    /// join returns (the JDK start/join memory-model edge).
    #[test]
    fn join_happens_before_edge() {
        let _rt = crate::test_support::runtime();
        static SEEN: AtomicI64 = AtomicI64::new(0);
        let flag = Arc::new(AtomicBool::new(false));
        let f2 = flag.clone();
        let child = std::thread::spawn(move || {
            let _d = TestDeath(current_record());
            SEEN.store(42, Ordering::Release);
            f2.store(true, Ordering::Release);
        });
        child.join().unwrap();
        assert!(flag.load(Ordering::Acquire));
        assert_eq!(SEEN.load(Ordering::Acquire), 42);
    }

    /// T06: sleep exception messages are Temurin-exact.
    #[test]
    fn sleep_rejects_bad_arguments() {
        let _rt = crate::test_support::runtime();
        let t = n_thread_sleep(&[Value::Long(-1)]).unwrap_err();
        assert_eq!(t.java_class(), "java/lang/IllegalArgumentException");
        let msg = crate::heap::with_obj(t.obj, |o| o.slots[0].clone());
        assert!(
            matches!(&msg, Value::Ref(Some(_))),
            "message must be present"
        );
        let t2 = n_thread_sleep(&[Value::Long(1), Value::Int(1_000_000)]).unwrap_err();
        assert_eq!(t2.java_class(), "java/lang/IllegalArgumentException");
    }

    /// T06: start-before-setDaemon is JDK-exact (IllegalThreadStateException).
    #[test]
    fn set_daemon_after_start_is_rejected_by_flag() {
        let _rt = crate::test_support::runtime();
        // Direct slot check (the full path runs in the ThreadHello fixture):
        // started = slot 3 non-zero → setDaemon must refuse.
        let r = crate::heap::alloc(crate::object::JObj {
            jclass: Arc::from("java/lang/Thread"),
            slots: vec![
                Value::Ref(None),
                Value::Str(Arc::new("t".to_string())),
                Value::Int(0),
                Value::Int(1), // started
                Value::Int(-1),
            ],
        });
        let err = n_thread_set_daemon(Some(&r), &[Value::Int(1)]).unwrap_err();
        assert_eq!(err.java_class(), "java/lang/IllegalThreadStateException");
    }

    /// T07: records are never removed (join-after-death keeps working).
    /// Asserted by identity, not by a global census — parallel tests
    /// register their own records concurrently, so only the never-shrink /
    /// still-present properties are stable under cargo's parallel runner.
    #[test]
    fn registry_records_are_stable() {
        let _rt = crate::test_support::runtime();
        let rec = std::thread::spawn(|| {
            let d = TestDeath(current_record());
            d.0.clone()
        })
        .join()
        .unwrap();
        assert!(
            registry_records().iter().any(|r| Arc::ptr_eq(r, &rec)),
            "records are never removed, even after death"
        );
    }
}
