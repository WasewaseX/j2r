use super::*;
use std::sync::Barrier;
use std::thread;

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

/// Panic-proof ring cleanup: a failed assertion must never leak a ring
/// record into the registry (a leaked ring member can later receive
/// the turn and wedge the scheduler for every test after it).
struct RingLeaver {
    rec: Arc<ThreadRecord>,
}
impl Drop for RingLeaver {
    fn drop(&mut self) {
        self.rec.in_ring.store(false, Ordering::Release);
    }
}

/// T06: currentThread identity — same object across calls in one
/// thread; different objects across two threads (M4_PLAN §1).
#[test]
fn current_thread_identity_holds() {
    let _rt = crate::runtime();
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
    let _rt = crate::runtime();
    let barrier = Arc::new(Barrier::new(2));
    let b2 = barrier.clone();
    let child = thread::spawn(move || {
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
    let _rt = crate::runtime();
    let obj = crate::object::new_obj("test/CrossRoot", 1).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let b2 = barrier.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let s2 = stop.clone();
    let child = thread::spawn(move || {
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
    let _rt = crate::runtime();
    set_coop_for_test(true);
    let parent = current_record();
    parent.in_ring.store(true, Ordering::Release);
    let _ring = RingLeaver {
        rec: parent.clone(),
    };
    *parent.name.lock().unwrap_or_else(|e| e.into_inner()) = "m".to_string();
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let l2 = log.clone();
    let done = Arc::new(AtomicBool::new(false));
    let d2 = done.clone();
    let barrier = Arc::new(Barrier::new(2));
    let b2 = barrier.clone();
    let child = thread::spawn(move || {
        let _d = TestDeath(current_record());
        let rec = current_record();
        rec.in_ring.store(true, Ordering::Release);
        *rec.name.lock().unwrap_or_else(|e| e.into_inner()) = "c".to_string();
        b2.wait();
        for i in 0..3 {
            yield_point();
            l2.lock().unwrap().push(format!("c{i}"));
        }
        d2.store(true, Ordering::Release);
    });
    // Pin the initial turn to the parent BEFORE the barrier (M4 Wave 3
    // determinism fix: the child's first yield must observe a
    // live-holder pin, never race it) — the job apply_coop_seed does
    // for a fresh ring in a generated program (seed % ring size).
    lock_sched().turn = parent.reg_index;
    barrier.wait();
    for i in 0..3 {
        yield_point();
        log.lock().unwrap().push(format!("m{i}"));
    }
    // Drain: keep yielding until the child finished its ring work.
    // Without this, the parent can win the final turn exchange and,
    // its loop being over, never yield again — starving the child's
    // last iteration (a Wave-2 latent flake, hit flakily once the
    // monitor wave perturbed the timing).
    while !done.load(Ordering::Acquire) {
        yield_point();
    }
    child.join().unwrap();
    set_coop_for_test(false);
    // Rotation contract: with two ring members the turn strictly
    // alternates holders, so the log must never name the same thread
    // twice in a row, must keep each thread's program order, and must
    // have exactly 3 entries per member. The first exchange's exact
    // global interleaving is an OS race; the property under test is
    // the alternation itself.
    let got = log.lock().unwrap().clone();
    assert_eq!(got.len(), 6, "3 entries per ring member: {got:?}");
    for w in got.windows(2) {
        assert!(
            w[0].as_bytes()[0] != w[1].as_bytes()[0],
            "turn holders must alternate: {got:?}"
        );
    }
    let c_seq: Vec<&String> = got.iter().filter(|s| s.starts_with('c')).collect();
    let m_seq: Vec<&String> = got.iter().filter(|s| s.starts_with('m')).collect();
    assert_eq!(c_seq, ["c0", "c1", "c2"], "child program order");
    assert_eq!(m_seq, ["m0", "m1", "m2"], "parent program order");
    // Which member moves first after the barrier is an OS race (both
    // reach their first yields together); the alternation above is
    // the scheduler property under test.
}

/// T09: ring participation is the scheduler's visibility rule — a
/// registered thread that is NOT in the ring never receives the turn,
/// so the rotation order of the real participants is unchanged.
#[test]
fn coop_ignores_non_ring_threads() {
    let _rt = crate::runtime();
    set_coop_for_test(true);
    let parent = current_record();
    parent.in_ring.store(true, Ordering::Release);
    let _ring = RingLeaver {
        rec: parent.clone(),
    };
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let l2 = log.clone();
    let done = Arc::new(AtomicBool::new(false));
    let d2 = done.clone();
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
        d2.store(true, Ordering::Release);
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
    // Pin the initial turn to the parent BEFORE the barrier (same
    // determinism fix as coop_round_robin). The outsider never
    // participates either way.
    lock_sched().turn = parent.reg_index;
    barrier.wait();
    for i in 0..2 {
        yield_point();
        log.lock().unwrap().push(format!("m{i}"));
    }
    // Drain (see coop_round_robin): never let the parent's finished
    // loop starve the ring child's last turn.
    while !done.load(Ordering::Acquire) {
        yield_point();
    }
    in_ring_child.join().unwrap();
    outsider.join().unwrap();
    set_coop_for_test(false);
    let binding = log.lock().unwrap();
    let m_and_a: Vec<&String> = binding
        .iter()
        .filter(|s| s.starts_with('m') || s.starts_with('a'))
        .collect();
    // Rotation contract (see coop_round_robin): holders alternate,
    // per-thread program order holds, counts exact, child first.
    assert_eq!(m_and_a.len(), 4, "2 entries per ring member");
    for w in m_and_a.windows(2) {
        assert!(
            w[0].as_bytes()[0] != w[1].as_bytes()[0],
            "turn holders must alternate: {m_and_a:?}"
        );
    }
    assert_eq!(m_and_a.iter().filter(|s| s.starts_with('a')).count(), 2);
    drop(binding);
}

/// T06: join happens-before — writes by the child are visible after
/// join returns (the JDK start/join memory-model edge).
#[test]
fn join_happens_before_edge() {
    let _rt = crate::runtime();
    static SEEN: AtomicI64 = AtomicI64::new(0);
    let flag = Arc::new(AtomicBool::new(false));
    let f2 = flag.clone();
    let child = thread::spawn(move || {
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
    let _rt = crate::runtime();
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
    let _rt = crate::runtime();
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
            Value::Int(0), // pendingInterrupt (T14)
        ],
        prim: None,
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
    let _rt = crate::runtime();
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

// ------------------------------------------- T14 interrupt (Wave 4)
// Semantics pinned on Temurin 21.0.13 (InterruptProbe / EdgeProbe,
// /home/z/probe): messages, ordering, clearing, monitor-enter
// immunity, never-started and dead-thread observability.

/// A Thread-shaped object linked to a registry id (or tid -1 for
/// never-started). Six slots since T14 — slot 5 is pendingInterrupt.
fn thread_obj_for(tid: i64) -> JRef {
    crate::heap::alloc(crate::object::JObj {
        jclass: Arc::from("java/lang/Thread"),
        slots: vec![
            Value::Ref(None),
            Value::Str(Arc::new("t".to_string())),
            Value::Int(0),
            Value::Int(1),
            Value::Int(tid as i32),
            Value::Int(0),
        ],
        prim: None,
    })
}

fn throw_msg(t: &Throw) -> Option<String> {
    // Extract the message ref UNDER the heap lock, convert OUTSIDE it —
    // the heap Mutex is not reentrant (same shape as monitor.rs's
    // message_of helper).
    let m = crate::heap::with_obj(t.obj, |o| match &o.slots[0] {
        Value::Ref(Some(m)) => Some(*m),
        Value::Ref(None) => None,
        other => panic!("unexpected message slot {other:?}"),
    });
    m.map(|r| crate::object::string_value(&r).to_string())
}

/// isInterrupted reads without clearing; the static interrupted()
/// reads AND clears (probe: true/true/true/false).
#[test]
fn interrupt_flag_semantics_are_jdk_exact() {
    let _rt = crate::runtime();
    let me = current_record();
    let obj = thread_obj_for(me.reg_index as i64);
    n_thread_interrupt(Some(&obj)).unwrap();
    assert_eq!(n_thread_is_interrupted(Some(&obj)).unwrap(), Value::Int(1));
    assert_eq!(n_thread_is_interrupted(Some(&obj)).unwrap(), Value::Int(1));
    assert_eq!(n_thread_interrupted().unwrap(), Value::Int(1));
    assert_eq!(n_thread_interrupted().unwrap(), Value::Int(0));
    assert_eq!(n_thread_is_interrupted(Some(&obj)).unwrap(), Value::Int(0));
}

/// A pre-set flag makes sleep throw IMMEDIATELY with the pinned
/// "sleep interrupted" message, and the delivery clears the flag
/// (EdgeProbe before-sleep + C).
#[test]
fn pre_interrupted_sleep_throws_and_clears() {
    let _rt = crate::runtime();
    let me = current_record();
    let obj = thread_obj_for(me.reg_index as i64);
    n_thread_interrupt(Some(&obj)).unwrap();
    let t0 = Instant::now();
    let err = n_thread_sleep(&[Value::Long(60_000)]).unwrap_err();
    assert!(t0.elapsed() < std::time::Duration::from_secs(1));
    assert_eq!(err.java_class(), "java/lang/InterruptedException");
    assert_eq!(throw_msg(&err).as_deref(), Some("sleep interrupted"));
    assert_eq!(n_thread_is_interrupted(Some(&obj)).unwrap(), Value::Int(0));
}

/// interrupt() wakes a thread parked in sleep well before its budget
/// (the scheduler-condvar wake channel; no 60-second hang).
#[test]
fn interrupt_wakes_a_sleeping_thread() {
    let _rt = crate::runtime();
    let (tx, rx) = std::sync::mpsc::channel::<JRef>();
    let child = thread::spawn(move || {
        let rec = current_record();
        let _d = TestDeath(rec);
        tx.send(current_thread_object()).unwrap();
        let start = Instant::now();
        let res = n_thread_sleep(&[Value::Long(60_000)]);
        (start.elapsed(), res)
    });
    let obj = rx.recv().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    n_thread_interrupt(Some(&obj)).unwrap();
    let (elapsed, res) = child.join().unwrap();
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "interrupt must wake the sleeper early, took {elapsed:?}"
    );
    let err = res.unwrap_err();
    assert_eq!(err.java_class(), "java/lang/InterruptedException");
    assert_eq!(throw_msg(&err).as_deref(), Some("sleep interrupted"));
}

/// Self-join with a pre-set flag: the JDK join blocks forever until
/// interrupted — the interrupt delivers the message-less IE
/// (JoinInterruptProbe: msg=null).
#[test]
fn pre_interrupted_self_join_throws_message_less() {
    let _rt = crate::runtime();
    let me = current_record();
    let obj = thread_obj_for(me.reg_index as i64);
    n_thread_interrupt(Some(&obj)).unwrap();
    let err = n_thread_join(Some(&obj), &[]).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/InterruptedException");
    assert_eq!(throw_msg(&err), None, "join IE carries no message");
}

/// Pre-interrupted wait: throws immediately WITHOUT releasing the
/// monitor — the catch handler sees it still held (EdgeProbe A),
/// proven here by a clean exit afterwards and an IMSE on the second.
#[test]
fn pre_interrupted_wait_keeps_the_monitor() {
    let _rt = crate::runtime();
    let obj = crate::object::new_obj("test/WaitPreInt", 0).unwrap();
    let me = current_record();
    let tobj = thread_obj_for(me.reg_index as i64);
    crate::monitor::monitor_enter(&obj).unwrap();
    n_thread_interrupt(Some(&tobj)).unwrap();
    let err = crate::monitor::object_wait(&obj, 0, 0).unwrap_err();
    assert_eq!(err.java_class(), "java/lang/InterruptedException");
    assert_eq!(throw_msg(&err), None);
    // Monitor never left this thread's hands: one clean exit, then IMSE.
    crate::monitor::monitor_exit(&obj).unwrap();
    assert!(crate::monitor::monitor_exit(&obj).is_err());
}

/// interrupt() wakes a monitor waiter; delivery happens AFTER the
/// monitor is re-acquired with the full count (JDK epilog), so the
/// catch handler owns the monitor.
#[test]
fn interrupt_wakes_a_monitor_waiter() {
    let _rt = crate::runtime();
    let obj = crate::object::new_obj("test/WaitInt", 0).unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<usize>();
    let o2 = obj;
    let child = thread::spawn(move || {
        let rec = current_record();
        let _d = TestDeath(rec);
        crate::monitor::monitor_enter(&o2).unwrap();
        tx.send(current_record().reg_index).unwrap();
        let res = crate::monitor::object_wait(&o2, 0, 0);
        // The re-acquired monitor is in our hands: one clean exit.
        crate::monitor::monitor_exit(&o2).unwrap();
        (res, crate::monitor::monitor_exit(&o2).is_err())
    });
    let child_tid = rx.recv().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    let tobj = thread_obj_for(child_tid as i64);
    n_thread_interrupt(Some(&tobj)).unwrap();
    let (res, second_exit_was_imse) = child.join().unwrap();
    let err = res.unwrap_err();
    assert_eq!(err.java_class(), "java/lang/InterruptedException");
    assert_eq!(throw_msg(&err), None);
    assert!(
        second_exit_was_imse,
        "monitor must be re-acquired before the IE is delivered"
    );
}

/// Monitor-enter is NOT interruptible (probe: no throw, flag stays
/// set through the enter).
#[test]
fn monitor_enter_ignores_the_interrupt_flag() {
    let _rt = crate::runtime();
    let obj = crate::object::new_obj("test/EnterInt", 0).unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<JRef>();
    let holder = std::thread::spawn(move || {
        let _d = TestDeath(current_record());
        crate::monitor::monitor_enter(&obj).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(400));
        crate::monitor::monitor_exit(&obj).unwrap();
    });
    let o2 = obj;
    let contender = std::thread::spawn(move || {
        let rec = current_record();
        let _d = TestDeath(rec);
        let own = current_thread_object();
        tx.send(own).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        crate::monitor::monitor_enter(&o2).unwrap(); // blocks ~350ms
        crate::thread::n_thread_is_interrupted(Some(&own)).unwrap()
    });
    let contender_obj = rx.recv().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(150));
    n_thread_interrupt(Some(&contender_obj)).unwrap();
    holder.join().unwrap();
    let flag_after_enter = contender.join().unwrap();
    assert_eq!(
        flag_after_enter,
        Value::Int(1),
        "the flag survives a monitor-enter that raced it"
    );
}

/// interrupt() on a dead thread is a silent no-op that still sets the
/// flag (dead-probe: isInterrupted reads true afterwards).
#[test]
fn interrupt_after_death_sets_the_flag() {
    let _rt = crate::runtime();
    let tid = std::thread::spawn(|| current_record().reg_index)
        .join()
        .unwrap();
    let obj = thread_obj_for(tid as i64);
    n_thread_interrupt(Some(&obj)).unwrap();
    assert_eq!(n_thread_is_interrupted(Some(&obj)).unwrap(), Value::Int(1));
}

/// interrupt() on a never-started thread: no record exists, the flag
/// lives on the object (slot 5) and isInterrupted reports it
/// (D-probe).
#[test]
fn interrupt_before_start_is_observable() {
    let _rt = crate::runtime();
    let obj = thread_obj_for(-1);
    n_thread_interrupt(Some(&obj)).unwrap();
    assert_eq!(n_thread_is_interrupted(Some(&obj)).unwrap(), Value::Int(1));
}
// --------------------------------------------- T15 daemon shutdown (Wave 4)

/// The JVM shutdown rule: main exits when ONLY daemons remain —
/// await_non_daemons must not wait on a live daemon record.
#[test]
fn main_exits_with_live_daemons() {
    let _rt = crate::runtime();
    let daemon = register_named_record("daemon", false, false);
    daemon.daemon.store(true, Ordering::Release);
    daemon.state.store(STATE_RUNNING, Ordering::Release);
    let t0 = Instant::now();
    await_non_daemons();
    assert!(
        t0.elapsed() < std::time::Duration::from_secs(2),
        "a live daemon must not block shutdown"
    );
    daemon.state.store(STATE_EXITED, Ordering::Release);
}

/// ...and main WAITS for a live non-daemon: await_non_daemons blocks
/// until the record exits (the collector's notify reaches it). The
/// child joins the coop ring, exactly as `start` grants — a raw
/// bystander thread must NOT hold shutdown open (that rule is pinned
/// by main_exits_with_live_daemons' counterpart).
#[test]
fn main_waits_for_non_daemon() {
    let _rt = crate::runtime();
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let child = thread::spawn(move || {
        let rec = current_record();
        rec.in_ring.store(true, Ordering::Release);
        let _d = TestDeath(rec);
        tx.send(()).unwrap(); // the record exists before the parent's await snapshot
        std::thread::sleep(std::time::Duration::from_millis(150));
    });
    rx.recv().unwrap();
    let t0 = Instant::now();
    await_non_daemons(); // joins the child (non-daemon) before returning
    assert!(
        t0.elapsed() >= std::time::Duration::from_millis(100),
        "shutdown must wait for the live non-daemon"
    );
    assert!(t0.elapsed() < std::time::Duration::from_secs(5));
    child.join().unwrap();
}

/// T15's registry edge case: a thread DYING DURING the STW quiescence
/// handshake must release the collector — the death sequence (EXITED +
/// notify under the SCHED lock) is the unregister-before-park
/// discipline. Bounded by a deadline so a regression fails loudly
/// instead of hanging CI.
#[test]
fn daemon_death_releases_the_stw_handshake() {
    let _rt = crate::runtime();
    let dying = register_named_record("dying-daemon", false, true);
    dying.daemon.store(true, Ordering::Release);
    dying.state.store(STATE_RUNNING, Ordering::Release);
    let d2 = dying.clone();
    let killer = std::thread::spawn(move || {
        // Die while the collector is mid-handshake, using run_body's
        // exact death sequence (EXITED under the SCHED lock + notify).
        std::thread::sleep(std::time::Duration::from_millis(50));
        d2.alive.store(false, Ordering::Release);
        d2.state.store(STATE_EXITED, Ordering::Release);
        let g = lock_sched();
        notify_all(&g);
    });
    let t0 = Instant::now();
    let guard = StwGuard::begin().expect("collector must win the token");
    crate::heap::collect_under_held_stw_for_test();
    drop(guard);
    assert!(
        t0.elapsed() < std::time::Duration::from_secs(10),
        "a death mid-handshake must release the collector"
    );
    killer.join().unwrap();
}
// ------------------------------------------ T17 stress (Wave 4)

/// Both stress suites mutate process-global scheduler state (coop
/// mode, chaos budget), so they must never overlap — libtest runs
/// tests in parallel threads and would otherwise corrupt each
/// other's rounds. One slot, taken for the whole round set.
static STRESS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// One producer/consumer pair on a real monitor: the producer fills,
/// the consumer drains, both use wait/notifyAll for the handoff. The
/// invariant is the exact tally — a lost wakeup or a missed notify
/// hangs the pair, which the test deadline converts into a loud
/// failure (never a CI hang).
fn stress_round(pairs: usize, iters: usize, deadline: std::time::Instant) -> std::time::Duration {
    let t0 = Instant::now();
    let mut handles = Vec::new();
    let done = Arc::new(AtomicU32::new(0));
    for pair in 0..pairs {
        // PER-PAIR cell: one producer, one consumer, exact handoff
        // counts — with a shared cell a consumer's last drain can
        // arrive after its producers finished and wait forever (the
        // first stress draft's workload bug; found by its own trace,
        // not a runtime race).
        let obj = crate::object::new_obj("test/StressCell", 0).unwrap();
        let count = Arc::new(AtomicI64::new(0));
        if pair == 0 && std::env::var("J2R_STRESS_DUMP").is_ok() {
            dump_while_stalled(obj, 20);
        }
        for is_producer in [true, false] {
            let count = count.clone();
            let done = done.clone();
            handles.push(std::thread::spawn(move || {
                let rec = current_record();
                rec.in_ring.store(true, Ordering::Release);
                let tid = rec.reg_index;
                let _d = TestDeath(rec);
                for it in 0..iters {
                    if std::env::var("J2R_STRESS_TRACE").is_ok() {
                        eprintln!(
                            "T{} pair{} {} iter{}",
                            tid,
                            pair,
                            if is_producer { "P" } else { "C" },
                            it
                        );
                    }
                    crate::monitor::monitor_enter(&obj).unwrap();
                    loop {
                        let c = count.load(Ordering::Acquire);
                        if is_producer && c < 1 {
                            count.store(c + 1, Ordering::Release);
                            crate::monitor::object_notify(&obj, true).unwrap();
                            break;
                        }
                        if !is_producer && c > 0 {
                            count.store(c - 1, Ordering::Release);
                            crate::monitor::object_notify(&obj, true).unwrap();
                            break;
                        }
                        crate::monitor::object_wait(&obj, 0, 0).unwrap();
                    }
                    crate::monitor::monitor_exit(&obj).unwrap();
                }
                done.fetch_add(1, Ordering::AcqRel);
            }));
        }
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(done.load(Ordering::Acquire), (pairs * 2) as u32);
    let elapsed = Instant::now() - t0;
    let limit = deadline - t0;
    assert!(
        elapsed < limit,
        "stress round took {elapsed:?} (limit {limit:?}) — treat as a hang"
    );
    elapsed
}

/// T17 coop stress: producer/consumer pairs under the deterministic
/// scheduler, several starting turns ("seeds") × fixed iterations.
/// Every round must finish with the exact tally inside the deadline.
#[test]
fn stress_coop_producer_consumer_rounds() {
    let _serial = STRESS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _rt = crate::runtime();
    set_coop_for_test(true);
    let seeds: [usize; 5] = [0, 1, 3, 7, 11];
    for seed in seeds {
        lock_sched().turn = seed;
        let elapsed = stress_round(3, 40, Instant::now() + std::time::Duration::from_secs(60));
        assert!(
            elapsed < std::time::Duration::from_secs(30),
            "coop round seed {seed} too slow: {elapsed:?}"
        );
    }
    set_coop_for_test(false);
}

/// Stall-state dump (T17 debugging): prints the ring, per-record
/// states, and a live monitor snapshot every 300ms from a bystander
/// thread. Test-only; removed once the race class is fixed.
fn dump_while_stalled(obj: crate::heap::JRef, secs: u64) {
    std::thread::spawn(move || {
        for _ in 0..(secs * 10) {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let m = crate::heap::entry_monitor(obj);
            let st = m.lock();
            let line = format!(
                "DUMP owner={:?} count={} waiters={:?}",
                st.owner, st.count, st.waiters
            );
            drop(st);
            let g = lock_sched();
            let states: Vec<String> = g
                .records
                .iter()
                .map(|r| {
                    format!(
                        "{}:{}{}",
                        r.reg_index,
                        r.state.load(Ordering::Acquire),
                        if r.in_ring.load(Ordering::Acquire) {
                            "r"
                        } else {
                            "-"
                        }
                    )
                })
                .collect();
            eprintln!("{} turn={} [{}]", line, g.turn, states.join(" "));
            drop(g);
        }
    });
}

/// T17 free-running stress: the same workload with the chaos knob on
/// (randomized micro-delays at the protocol seams — the local
/// stand-in for CI's slow scheduler). Invariant assertions only; no
/// output comparison. This is the harness the CI-timing monitor race
/// hunt runs through; a reproduction is a hang converted into a loud
/// deadline failure.
#[test]
fn stress_free_running_producer_consumer_chaos() {
    let _serial = STRESS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _rt = crate::runtime();
    for us in [0u32, 300, 800, 1500] {
        chaos_set(us);
        stress_round(4, 30, Instant::now() + std::time::Duration::from_secs(60));
    }
    chaos_set(0);
}
