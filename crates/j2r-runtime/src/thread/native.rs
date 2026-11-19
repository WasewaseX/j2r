use super::*;

/// Runs the thread body with the JDK death semantics and records death
/// exactly once: alive=false, state=Exited, coop turn granted onward,
/// everyone notified (joiners, the ring, a waiting collector or shutdown).
fn run_body(rec: &Arc<ThreadRecord>, body: impl FnOnce() -> Result<(), Throw>) {
    rec.set_running();
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
/// `java/lang/Thread` slot layout (builtin, 6 slots since T14):
/// 0 runnable, 1 name (`Value::Str`), 2 daemon (Int 0/1), 3 started
/// (Int 0/1), 4 tid (Int; -1 until `start` links the registry record),
/// 5 pendingInterrupt (Int 0/1; the never-started interrupt carrier).
pub const THREAD_SLOTS: usize = 6;
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
        o.slots[5] = Value::Int(0); // pendingInterrupt (T14, Wave 4)
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
    // Migrate a pre-start interrupt into the record (T14): the object-side
    // flag (slot 5) was the only observable carrier before start; the
    // blocking paths (sleep/join/wait) poll the record flag, so a thread
    // interrupted between construction and start must still deliver. The
    // flag MOVES: after start the record is the single carrier, so two
    // views of one thread can never report different values.
    if slot_int(&r, 5) != 0 {
        rec.interrupted.store(true, Ordering::Release);
        set_slot(&r, 5, Value::Int(0));
    }
    // Migrate the daemon flag the same way (T15): setDaemon is only legal
    // before start, so slot 2 was the carrier; without this migration the
    // registry's shutdown rule (await_non_daemons) would wait on a daemon
    // — found by the DaemonExit fixture, which hung the process for the
    // daemon's full sleep budget before this fix.
    rec.daemon.store(slot_int(&r, 2) != 0, Ordering::Release);

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
    Err(internal_error(&format!(
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
        return join_self(budget_ms);
    }
    if coop_mode() {
        return join_coop(&target, budget_ms);
    }
    join_wait(&target, budget_ms)
}
/// The self-join phase (verbatim from `n_thread_join`, round-6
/// reshaping): joining the current thread never returns without a
/// timeout, returns after the budget with one, and is interruptible
/// in every form (T14).
fn join_self(budget_ms: i64) -> Result<Value, Throw> {
    // JDK behaviour: joining the current thread without a timeout
    // never returns; with a timeout it returns after the budget.
    // Interruptible in every form (T14): the JDK delivers the
    // message-less InterruptedException (JoinInterruptProbe).
    let me = current_record();
    if budget_ms > 0 {
        let deadline = Instant::now() + std::time::Duration::from_millis(budget_ms as u64);
        me.state.store(STATE_PARKED, Ordering::Release);
        let mut g = lock_sched();
        notify_all(&g);
        loop {
            if interrupted_now(&me) {
                me.set_running();
                // L7: no SCHED guard across the heap lock (slot 5).
                drop(g);
                clear_interrupt(&me);
                return Err(throw_new("java/lang/InterruptedException", None));
            }
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            g = SCHED_CV
                .wait_timeout(g, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        me.set_running();
        return Ok(Value::Void);
    }
    if coop_mode() {
        loop {
            if interrupted_now(&me) {
                clear_interrupt(&me);
                return Err(throw_new("java/lang/InterruptedException", None));
            }
            yield_point(); // deterministic hang, like the JVM's
        }
    }
    me.state.store(STATE_PARKED, Ordering::Release);
    // Every state flip notifies (T17.1): the STW collector's
    // quiescence loop sleeps on this condvar.
    {
        let g = lock_sched();
        notify_all(&g);
    }
    loop {
        if interrupted_now(&me) {
            me.set_running();
            clear_interrupt(&me);
            return Err(throw_new("java/lang/InterruptedException", None));
        }
        // thread::park() alone is not woken by interrupt()'s condvar
        // notify, so poll on a short timeout (50 ms granularity).
        std::thread::park_timeout(std::time::Duration::from_millis(50));
    }
}
/// The coop-ring phase (verbatim continuation): interrupt check every
/// tick, deterministic hang like the JVM's, coop ticks as the budget
/// currency (documented gap).
fn join_coop(target: &Arc<ThreadRecord>, budget_ms: i64) -> Result<Value, Throw> {
    let deadline = budget_ms;
    let mut waited: i64 = 0;
    let me = current_record();
    while target.state.load(Ordering::Acquire) != STATE_EXITED {
        // Interrupt check every tick (T14): the flag can only have
        // been set by another thread's turn, which is exactly one
        // yield ago — deterministic delivery.
        if interrupted_now(&me) {
            clear_interrupt(&me);
            return Err(throw_new("java/lang/InterruptedException", None));
        }
        if deadline > 0 && waited >= deadline {
            return Ok(Value::Void);
        }
        yield_point();
        waited += 1; // coop ticks, not wall-clock (documented gap)
    }
    Ok(Value::Void)
}
/// The final wait phase (verbatim continuation): the join-waiter parks
/// as Parked (never a collector-joiner-target cycle) and wakes on the
/// sched condvar — interruptible, deadline-bounded when set.
fn join_wait(target: &Arc<ThreadRecord>, budget_ms: i64) -> Result<Value, Throw> {
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
        // Interruptible join (T14): the flag is checked before the first
        // park (pre-interrupt) and at every wake (interrupt() notifies
        // this condvar). Delivery clears the flag and throws the
        // message-less InterruptedException the probes pinned.
        if interrupted_now(&me) {
            me.set_running();
            // L7: drop the SCHED guard before the heap lock (slot 5).
            drop(g);
            clear_interrupt(&me);
            return Err(throw_new("java/lang/InterruptedException", None));
        }
        if let Some(d) = deadline {
            let now = Instant::now();
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
    me.set_running();
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
        // contract (documented in the module header). Interrupt delivery
        // stays deterministic (T14): the flag is checked BEFORE the yield
        // (pre-interrupt) and after it (the interrupter can only have run
        // during the yielded turn), so exactly one of the two checks sees
        // the flag.
        let me = current_record();
        if interrupted_now(&me) {
            clear_interrupt(&me);
            return Err(throw_new(
                "java/lang/InterruptedException",
                Some("sleep interrupted"),
            ));
        }
        yield_point();
        if interrupted_now(&me) {
            clear_interrupt(&me);
            return Err(throw_new(
                "java/lang/InterruptedException",
                Some("sleep interrupted"),
            ));
        }
        return Ok(Value::Void);
    }
    if budget_ms > 0 {
        // Interruptible free-running sleep (T14): park on the scheduler
        // condvar with a deadline instead of an unwakeable
        // std::thread::sleep — `interrupt()` notifies that condvar, so the
        // wake is immediate and the delivered exception is the JDK's
        // pinned "sleep interrupted". Marked Parked first: the STW
        // collector must not wait for a sleeping thread (join's
        // discipline; a sleeper mutates no Java state while parked).
        let me = current_record();
        let deadline = Instant::now() + std::time::Duration::from_millis(budget_ms as u64);
        me.state.store(STATE_PARKED, Ordering::Release);
        let mut g = lock_sched();
        notify_all(&g);
        loop {
            if interrupted_now(&me) {
                me.set_running();
                // L7: the SCHED guard must be gone before clear_interrupt
                // touches the heap (slot 5) — heap paths take the SCHED
                // lock, so SCHED-then-heap would close a cycle.
                drop(g);
                clear_interrupt(&me);
                return Err(throw_new(
                    "java/lang/InterruptedException",
                    Some("sleep interrupted"),
                ));
            }
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            g = SCHED_CV
                .wait_timeout(g, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        me.set_running();
        return Ok(Value::Void);
    }
    std::thread::yield_now();
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
            Value::Int(0), // pendingInterrupt (T14)
        ],
        prim: None,
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
    // setDaemon is only legal BEFORE start, so there is normally no
    // record yet — the flag rides slot 2 and `start` migrates it (the
    // interrupt migration's pattern). If a record exists anyway (a
    // start-linked alias), keep it in sync too.
    if let Some(rec) = target_record(&r) {
        rec.daemon.store(on, Ordering::Release);
    }
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

// ---------------------------------------------- interrupt (T14, Wave 4)
// JDK-exact semantics pinned on Temurin 21.0.13 (InterruptProbe /
// JoinInterruptProbe / EdgeProbe, 2026-11-05):
// * `sleep` interruption throws InterruptedException("sleep interrupted");
// * `join` and `Object.wait` interruptions throw a MESSAGE-LESS
//   InterruptedException (probes: msg=null in every blocked case);
// * a pre-set flag makes the next blocking call throw immediately, and
//   the flag is CLEARED by the delivery (C-probe: flagAfterSleepIE=false);
// * a thread interrupted while blocked on monitor-enter does not throw;
//   the flag stays set after the enter completes;
// * `isInterrupted` reads without clearing; the static `interrupted`
//   reads and clears;
// * `interrupt()` on a dead thread sets the flag (post-death
//   `isInterrupted` reads true — dead-probe); `interrupt()` on a
//   never-started thread is observable via the object flag (D-probe).
/// The record for a registry id (used by `object_notify` to reach the
/// removed waiters' ticket channels).
pub(crate) fn record_by_index(i: usize) -> Option<Arc<ThreadRecord>> {
    lock_sched().records.get(i).cloned()
}
/// Sets the interrupt flag on a thread. The flag's carrier depends on the
/// thread's state:
/// * started (record exists): the RECORD flag — shared by every view of
///   the thread, like the JDK's per-thread status. The object slot is NOT
///   touched, so two Thread-object views of one thread can never diverge.
/// * never started (no record): object slot 5 — the only observable
///   carrier (D-probe); `start` migrates it into the record.
///
/// A wake event is delivered either way, plus a scheduler notify for the
/// sleep/join parkers (they wait on the scheduler condvar, not the
/// ticket).
pub(crate) fn set_interrupt(rec: Option<&Arc<ThreadRecord>>, obj: &JRef) {
    match rec {
        Some(rec) => {
            rec.interrupted.store(true, Ordering::Release);
            rec.deliver_wakeup();
        }
        None => set_slot(obj, 5, Value::Int(1)),
    }
    notify_all(&lock_sched());
}
/// Clears the flag after a delivered InterruptedException (or a static
/// `interrupted()` poll): record flag and object slot 5 together.
pub(crate) fn clear_interrupt(rec: &Arc<ThreadRecord>) {
    rec.interrupted.store(false, Ordering::Release);
    let j = rec.java_object.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(obj) = *j {
        set_slot(&obj, 5, Value::Int(0));
    }
}
/// `Thread.interrupt()`. JDK: no exception, no-op observable only through
/// the flag; on a dead thread it still sets the flag (dead-probe).
pub fn n_thread_interrupt(recv: Option<&JRef>) -> Result<Value, Throw> {
    let r = *recv.expect("j2r internal: Thread.interrupt without receiver");
    let rec = target_record(&r);
    set_interrupt(rec.as_ref(), &r);
    Ok(Value::Void)
}
/// `Thread.isInterrupted()` — reads the flag without clearing (probe:
/// is1==is2==true). Threads without a record (never started) report the
/// object flag.
pub fn n_thread_is_interrupted(recv: Option<&JRef>) -> Result<Value, Throw> {
    let r = *recv.expect("j2r internal: Thread.isInterrupted without receiver");
    let rec = target_record(&r);
    let set = rec
        .map(|t| t.interrupted.load(Ordering::Acquire))
        .unwrap_or(false)
        || slot_int(&r, 5) != 0;
    Ok(Value::Int(set as i32))
}
/// static `Thread.interrupted()` — reads AND clears the current thread's
/// flag (probe: st1=true, st2=false).
pub fn n_thread_interrupted() -> Result<Value, Throw> {
    let me = current_record();
    let old = me.interrupted.swap(false, Ordering::AcqRel);
    let j = me.java_object.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(obj) = *j {
        set_slot(&obj, 5, Value::Int(0));
    }
    Ok(Value::Int(old as i32))
}
/// The current thread's interrupt flag, for the blocking paths. The
/// object flag counts too: a pre-start interrupt that migrated (or a
/// flag set through a second Thread object alias) must deliver.
pub(crate) fn interrupted_now(rec: &Arc<ThreadRecord>) -> bool {
    rec.interrupted.load(Ordering::Acquire)
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

// --------------------------------------------------- chaos knob (T17)
