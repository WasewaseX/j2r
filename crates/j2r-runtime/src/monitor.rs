//! Object monitors — M4 Wave 3 (T10–T12): contention, `wait`/`notify`/
//! `notifyAll`, timed wait. Replaces the M3 single-threaded re-entrancy
//! counter (a `Cell<u32>` per heap entry) with a real per-entry state
//! machine.
//!
//! # State machine (T10)
//!
//! Each live heap entry lazily owns one `Arc<Monitor>`:
//!
//! ```text
//! unowned                        -- owner: None
//! owned { owner: reg_index, count: u32 }   -- re-entrancy depth >= 1
//! ```
//!
//! Contended `monitorenter` parks on the monitor's own `Condvar`. The
//! wait set (`MonState::waiters`) is a FIFO of thread ids; `notify` moves
//! one waiter out of the set (it then competes for the monitor like any
//! enter-contender — Mesa semantics, the same shape HotSpot uses), and
//! the woken thread re-acquires with its **full saved count**.
//!
//! # Lock law (extends RUNTIME_SAFETY L1–L6 with L7)
//!
//! **L7 — heap → monitor-inner → sched, never backwards, and never park
//! while holding the heap lock or monitor inner.**
//!
//! * `entry_monitor` (heap.rs) takes the heap lock to fetch/clone the
//!   entry's `Arc<Monitor>` and drops it before any monitor op; monitor
//!   code never re-enters the heap while holding `inner` (exception
//!   objects are allocated after the guard is dropped, as in `monitor_exit`).
//! * Parks happen through `Condvar::wait` on `inner` (which *releases*
//!   `inner` while parked) — never through a sched/heap wait while
//!   holding `inner`.
//! * `monitor_park_prep` may take the sched lock while `inner` is held,
//!   but only to *rotate the turn away* (no wait); `monitor_wake_prep`
//!   runs with no lock held at all.
//!
//! # STW / coop integration
//!
//! A thread about to park on a monitor marks itself `Parked` first (the
//! collector never waits for it) and, in coop mode, hands the coop turn
//! on — a blocked thread must never hold the turn, or the monitor owner
//! could never run to release it. On wake it runs `monitor_wake_prep`:
//! `safepoint_poll` (absorbs an in-flight STW), then the coop turn
//! re-acquisition, then `Running`. All of this is invisible to Java
//! code; spurious condvar wakeups are absorbed by the wait loops.

use crate::heap::JRef;
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// Per-object monitor (T10). One `Arc` per synchronized heap entry, held
/// inside the `Entry` so its lifetime is exactly the object's lifetime
/// (a contender's receiver is frame-rooted, so the entry cannot be swept
/// while a thread parks on it; a swept entry with no outstanding clone
/// simply drops its monitor).
pub(crate) struct Monitor {
    inner: Mutex<MonState>,
    cv: Condvar,
}

pub(crate) struct MonState {
    /// Owning thread's registry id; `None` = unowned.
    pub(crate) owner: Option<usize>,
    /// Owner's re-entrancy depth (>= 1 while owned).
    pub(crate) count: u32,
    /// FIFO wait set (thread ids). `notify` removes a waiter; the removed
    /// thread then competes for ownership like any enter-contender.
    pub(crate) waiters: VecDeque<usize>,
}

impl Monitor {
    pub(crate) fn new() -> Monitor {
        Monitor {
            inner: Mutex::new(MonState {
                owner: None,
                count: 0,
                waiters: VecDeque::new(),
            }),
            cv: Condvar::new(),
        }
    }

    pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, MonState> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

// ------------------------------------------------------------- enter / exit

/// `monitorenter`: re-entrant acquisition; contended enter parks on the
/// monitor's `Condvar` (T10). The only Java-visible failure is a stale
/// handle (loud panic — a rooting bug) or counter overflow (internal).
pub fn monitor_enter(r: &JRef) -> Result<(), crate::Throw> {
    let m = crate::heap::entry_monitor(*r);
    let rec = crate::thread::current_record();
    rec.mon_ops
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let me = rec.reg_index;
    let mut parked = false;
    let mut st = m.lock();
    loop {
        match st.owner {
            None => {
                st.owner = Some(me);
                st.count = 1;
                drop(st);
                if parked {
                    crate::thread::monitor_wake_prep(&rec);
                }
                return Ok(());
            }
            Some(o) if o == me => {
                st.count = st
                    .count
                    .checked_add(1)
                    .expect("j2r internal: monitor count overflow");
                drop(st);
                if parked {
                    crate::thread::monitor_wake_prep(&rec);
                }
                return Ok(());
            }
            Some(_) => {
                // Contended (T10): mark Parked so the collector does not
                // wait for us, hand the coop turn on (the owner needs it to
                // release), then block. `cv.wait` releases `inner` for the
                // duration of the park, so the owner can enter/exit freely.
                crate::thread::monitor_park_prep(&rec);
                parked = true;
                st = m.cv.wait(st).unwrap_or_else(|e| e.into_inner());
                // Re-check under the reacquired guard; spurious wakeups
                // simply loop.
            }
        }
    }
}

/// `monitorexit`: releases one level of ownership. Exiting a monitor the
/// thread does not hold throws `IllegalMonitorStateException` with
/// HotSpot's message (pinned since M3). A full release (count 1 → 0)
/// wakes enter-contenders; wait-set members are NOT woken by an exit —
/// only `Object.notify`/`notifyAll` reach them (JDK behaviour).
pub fn monitor_exit(r: &JRef) -> Result<(), crate::Throw> {
    let m = crate::heap::entry_monitor(*r);
    let rec = crate::thread::current_record();
    rec.mon_ops
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let me = rec.reg_index;
    let mut unowned = false;
    {
        let mut st = m.lock();
        if st.owner == Some(me) {
            if st.count > 1 {
                st.count -= 1;
            } else {
                st.owner = None;
                st.count = 0;
                drop(st);
                m.cv.notify_all(); // wake enter-contenders
            }
        } else {
            unowned = true;
        }
    }
    if unowned {
        // monitorexit's IMSE carries NO message on Temurin 21 (pinned with
        // a hand-built classfile, T13) — unlike Object.wait/notify's
        // "current thread is not owner".
        return Err(crate::throw_new(
            "java/lang/IllegalMonitorStateException",
            None,
        ));
    }
    Ok(())
}

/// Unwind-release (FrameGuard `Drop` safety net, frame.rs): decrements
/// without the ownership check, because the frame recorded the
/// acquisition. Debug-asserted to never underflow.
pub(crate) fn monitor_release(r: &JRef) {
    let m = crate::heap::entry_monitor(*r);
    let me = crate::thread::current_record().reg_index;
    let mut st = m.lock();
    if st.owner == Some(me) && st.count > 0 {
        st.count -= 1;
        if st.count == 0 {
            st.owner = None;
            drop(st);
            m.cv.notify_all();
        }
    } else {
        debug_assert!(
            st.owner.is_none() && st.count == 0,
            "j2r internal: monitor unwind-release underflow"
        );
    }
}

/// Current monitor count for one object (test/observation helper).
#[cfg(test)]
pub(crate) fn monitor_count(r: &JRef) -> u32 {
    let m = crate::heap::entry_monitor(*r);
    let st = m.lock();
    st.count
}

/// Current owner thread id, if any (test/observation helper).
#[cfg(test)]
pub(crate) fn monitor_owner(r: &JRef) -> Option<usize> {
    let m = crate::heap::entry_monitor(*r);
    let st = m.lock();
    st.owner
}

// --------------------------------------------------------- wait / notify

/// `Object.wait(timeoutMillis, nanos)` (T11/T12) — all three JDK shapes
/// funnel here (`wait()` = `(0, 0)` = wait forever, `wait(ms)` = `(ms, 0)`).
///
/// Semantics (JDK-faithful, pinned empirically against Temurin 21.0.13
/// with an OrderProbe covering owner × arg-validity — see T13):
/// * the ARGUMENT ranges are checked first (the JDK checks them in the
///   Java-level `Object.wait` wrapper, before the native ever runs):
///   negative millis → `IllegalArgumentException("timeout value is
///   negative")`; nanos outside 0..=999_999 →
///   `IllegalArgumentException("nanosecond timeout value out of range")`;
///   so a non-owner with a negative timeout gets the IAE, not the IMSE;
/// * then ownership — `IllegalMonitorStateException("current thread is
///   not owner")` (JDK 21 wording; the old bare "current thread not
///   owner" is the pre-15 raw-monitor string);
/// * nanos >= 500_000 rounds the effective budget up one millisecond
///   (same rounding as `Thread.sleep`, pinned in T12);
/// * `(0, 0)` waits forever; any other `(0, nanos)` waits a sub-
///   millisecond budget and returns promptly;
/// * the FULL re-entrancy count is released atomically with parking and
///   restored on reacquisition — re-entrancy across `wait` is preserved;
/// * spurious condvar wakeups are absorbed internally (the loop re-checks
///   wait-set membership and the deadline), invisible to Java code;
/// * a timed-out wait returns normally (no exception), after
///   re-acquiring the monitor.
pub fn object_wait(
    r: &JRef,
    timeout_millis: i64,
    nanos: i32,
) -> Result<crate::Value, crate::Throw> {
    let m = crate::heap::entry_monitor(*r);
    let rec = crate::thread::current_record();
    let me = rec.reg_index;

    // Argument validation FIRST: the JDK range-checks in the Java-level
    // wrapper, before the native wait0 (pinned on Temurin 21 — a non-owner
    // with a negative timeout gets the IAE). Messages mirror Thread.sleep's
    // pinned strings (T12).
    if timeout_millis < 0 {
        return Err(crate::throw_new(
            "java/lang/IllegalArgumentException",
            Some("timeout value is negative"),
        ));
    }
    if !(0..=999_999).contains(&nanos) {
        return Err(crate::throw_new(
            "java/lang/IllegalArgumentException",
            Some("nanosecond timeout value out of range"),
        ));
    }

    // Ownership check AFTER the ranges (JDK order). The throw is built
    // without holding the monitor lock (throw_new allocates; L7).
    {
        let st = m.lock();
        if st.owner != Some(me) {
            drop(st);
            return Err(crate::throw_new(
                "java/lang/IllegalMonitorStateException",
                Some("current thread is not owner"),
            ));
        }
    }
    let forever = timeout_millis == 0 && nanos == 0;
    let budget_ms: u64 = if nanos >= 500_000 {
        (timeout_millis as u64).saturating_add(1)
    } else {
        timeout_millis as u64
    };
    // Deadline is fixed once, before the release; each park recomputes the
    // remaining slice from it.
    let deadline = if forever {
        None
    } else {
        Some(Instant::now() + Duration::from_millis(budget_ms))
    };

    // T14 entry interrupt check (EdgeProbe A): a pre-set flag makes wait
    // throw IMMEDIATELY, before the monitor is released — the catch
    // handler sees the monitor still held (JDK). Placed BETWEEN the two
    // ownership checks and OUTSIDE any runtime lock: clear_interrupt
    // touches the heap (slot 5), and the monitor lock must never overlap
    // the heap lock (L7 — the same reason throw_new allocates unlocked).
    // Ranges and ownership run first (the Java-wrapper order is pinned by
    // the T13 OrderProbe).
    if crate::thread::interrupted_now(&rec) {
        crate::thread::clear_interrupt(&rec);
        return Err(throw_interrupted_ie());
    }

    let mut st = m.lock();
    if st.owner != Some(me) {
        // Unreachable via the natives (the check above already returned),
        // but object_wait is also the re-entry point for callers that
        // validated earlier — keep the invariant enforced.
        drop(st);
        return Err(crate::throw_new(
            "java/lang/IllegalMonitorStateException",
            Some("current thread is not owner"),
        ));
    }
    let saved = st.count;
    // Atomic full-count release + wait-set entry (T11).
    st.count = 0;
    st.owner = None;
    st.waiters.push_back(me);
    drop(st);
    m.cv.notify_all(); // enter-contenders may take the freed monitor

    let mut parked = false;
    // Park loop (T14-refactored): the wake channel is now the thread's
    // OWN ticket condvar — `notify` delivers through it for each removed
    // waiter, `interrupt()` delivers through it always, and timed waits
    // wake at the deadline. The monitor lock is never held while parked
    // (the notifier must be able to lock it). Wake-order pinned from the
    // JDK epilog: interrupt beats notified beats deadline.
    let mut interrupted = false;
    loop {
        st = m.lock();
        if crate::thread::interrupted_now(&rec) {
            st.waiters.retain(|&t| t != me);
            drop(st);
            interrupted = true;
            break; // JDK: interrupt wins even over a concurrent notify
        }
        let in_set = st.waiters.iter().any(|&t| t == me);
        if !in_set {
            drop(st);
            break; // notified (T11): leave the wait set, recompete below
        }
        if let Some(dl) = deadline {
            if Instant::now() >= dl {
                st.waiters.retain(|&t| t != me);
                drop(st);
                break; // timed out — wait(timeout) returns normally (T12)
            }
        }
        crate::thread::monitor_park_prep(&rec);
        parked = true;
        drop(st);
        rec.ticket_wait(deadline);
    }

    // Leaving the park state: absorb an in-flight STW, rejoin the coop
    // ring, mark Running. No runtime lock is held here (L7).
    if parked {
        crate::thread::monitor_wake_prep(&rec);
    }

    // Recompete: reacquire with the FULL saved count. The notified (or
    // timed-out, or interrupted) thread contends like any enter-contender
    // — the monitor may have been taken in between (Mesa semantics); if
    // so we park on the same condvar until it is free again. JDK: the
    // interrupted waiter must RE-ACQUIRE before the exception is
    // delivered, so the catch handler sees the monitor held.
    let mut st = m.lock();
    let mut parked2 = false;
    loop {
        match st.owner {
            None => {
                st.owner = Some(me);
                st.count = saved;
                drop(st);
                if parked2 {
                    crate::thread::monitor_wake_prep(&rec);
                }
                // JDK epilog (pinned): an interrupt pending at wake beats
                // a normal (notified or timed-out) return — the exception
                // is thrown WITH the monitor re-acquired and the flag
                // cleared (EdgeProbe C: flag cleared after delivery).
                if interrupted || crate::thread::interrupted_now(&rec) {
                    crate::thread::clear_interrupt(&rec);
                    return Err(throw_interrupted_ie());
                }
                // A timed-out wait returns normally — no exception, the
                // monitor held with the full saved count (JDK behaviour).
                return Ok(crate::Value::Void);
            }
            Some(o) if o == me => unreachable!("j2r internal: wait released the full count"),
            Some(_) => {
                crate::thread::monitor_park_prep(&rec);
                parked2 = true;
                // Bounded re-check slice, NOT an indefinite wait: the
                // release notify this waiter is waiting for may already
                // have FIRED while it was between its ticket wake and
                // this park (found by the T17 coop stress round — and it
                // IS the CI-timing race the gc-probe localized:
                // MonitorContention/WaitNotify hang when the notified
                // waiter loses that window on slow runners, because with
                // one pair left no further m.cv event ever fires). The
                // JDK closes the window by relocating the notified
                // waiter onto the entry list at notify time; our slice
                // re-check converts any such loss into a bounded delay.
                // m.cv.wait_timeout(st) keeps the park atomic with the
                // ownership check (L7 shape of the Wave-3 park).
                st =
                    m.cv.wait_timeout(st, std::time::Duration::from_millis(5))
                        .unwrap_or_else(|e| e.into_inner())
                        .0;
            }
        }
    }
}

/// The message-less InterruptedException the T14 probes pinned for wait
/// (and join). Built without holding any runtime lock (L7).
fn throw_interrupted_ie() -> crate::Throw {
    crate::throw_new("java/lang/InterruptedException", None)
}

/// `Object.notify` (`all = false`) / `Object.notifyAll` (T11). Ownership
/// required — the JDK-21 IMSE wording pinned by the OrderProbe. `notify` moves the
/// FIFO head out of the wait set; `notifyAll` clears it. Removed waiters
/// then compete for the monitor like enter-contenders; none of them can
/// actually acquire until the notifier releases (the notifier still owns
/// the monitor when it calls notify — JDK behaviour).
pub fn object_notify(r: &JRef, all: bool) -> Result<(), crate::Throw> {
    let m = crate::heap::entry_monitor(*r);
    let me = crate::thread::current_record().reg_index;
    let mut unowned = false;
    let mut woken: Vec<usize> = Vec::new();
    {
        let mut st = m.lock();
        if st.owner == Some(me) {
            if all {
                woken = st.waiters.iter().copied().collect();
                st.waiters.clear();
            } else if let Some(tid) = st.waiters.pop_front() {
                woken.push(tid);
            }
        } else {
            unowned = true;
        }
    }
    if unowned {
        return Err(crate::throw_new(
            "java/lang/IllegalMonitorStateException",
            Some("current thread is not owner"),
        ));
    }
    // T14: each REMOVED waiter gets its own wake event on its ticket
    // channel (the waiter's park loop re-locks the monitor and observes
    // the empty wait set). The monitor condvar still fires for the
    // enter-contenders. Ticket locks are taken with the monitor lock
    // already dropped (leaf-lock discipline).
    for tid in woken {
        if let Some(t) = crate::thread::record_by_index(tid) {
            t.deliver_wakeup();
        }
    }
    m.cv.notify_all();
    Ok(())
}

// ------------------------------------------------ park/wake housekeeping
//
// The state flips themselves live in thread.rs (next to the STW and coop
// machinery): `thread::monitor_park_prep` (Parked + coop turn hand-on,
// called while holding `inner` because it never waits) and
// `thread::monitor_wake_prep` (safepoint poll → turn re-acquisition →
// Running, called with no lock held).

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    fn new_object() -> JRef {
        crate::object::new_obj("java/lang/Object", 0).unwrap()
    }

    fn class_of(err: &crate::Throw) -> String {
        crate::heap::with_obj(err.obj, |o| o.jclass.to_string())
    }

    fn message_of(err: &crate::Throw) -> String {
        let m = crate::heap::with_obj(err.obj, |o| match &o.slots[0] {
            crate::Value::Ref(Some(m)) => Some(*m),
            crate::Value::Ref(None) => None, // message-less throw (monitorexit)
            other => panic!("unexpected message slot {other:?}"),
        });
        match m {
            Some(r) => crate::object::string_value(&r).to_string(),
            None => String::new(),
        }
    }

    /// T10: re-entrant acquisition on the real state machine.
    #[test]
    fn monitor_enter_is_reentrant() {
        let _rt = crate::test_support::runtime();
        let r = new_object();
        assert_eq!(monitor_owner(&r), None);
        monitor_enter(&r).unwrap();
        monitor_enter(&r).unwrap();
        assert_eq!(monitor_count(&r), 2);
        monitor_exit(&r).unwrap();
        assert_eq!(monitor_count(&r), 1);
        monitor_exit(&r).unwrap();
        assert_eq!(monitor_owner(&r), None);
    }

    /// T10: exiting an unowned monitor is the pinned message-less IMSE
    /// (Temurin 21 monitorexit carries no message).
    #[test]
    fn monitor_exit_without_ownership_is_imse() {
        let _rt = crate::test_support::runtime();
        let r = new_object();
        let err = monitor_exit(&r).unwrap_err();
        assert_eq!(class_of(&err), "java/lang/IllegalMonitorStateException");
        assert!(message_of(&err).is_empty());
    }

    /// T11/T13: wait/notify without ownership is the JDK-21 IMSE — and
    /// the ORDER is ranges-first (pinned by the Temurin OrderProbe): a
    /// non-owner with a negative timeout gets the IAE, not the IMSE.
    #[test]
    fn wait_notify_without_ownership_is_imse() {
        let _rt = crate::test_support::runtime();
        let r = new_object();
        // Ranges run before the ownership check (Java-level wrapper).
        let err = object_wait(&r, -1, 0).unwrap_err();
        assert_eq!(class_of(&err), "java/lang/IllegalArgumentException");
        assert_eq!(message_of(&err), "timeout value is negative");
        let err = object_wait(&r, 1, -1).unwrap_err();
        assert_eq!(class_of(&err), "java/lang/IllegalArgumentException");
        assert_eq!(message_of(&err), "nanosecond timeout value out of range");
        // Valid args reach the ownership check.
        let err = object_wait(&r, 0, 0).unwrap_err();
        assert_eq!(class_of(&err), "java/lang/IllegalMonitorStateException");
        assert_eq!(message_of(&err), "current thread is not owner");
        let err = object_notify(&r, false).unwrap_err();
        assert_eq!(class_of(&err), "java/lang/IllegalMonitorStateException");
        assert_eq!(message_of(&err), "current thread is not owner");
        let err = object_notify(&r, true).unwrap_err();
        assert_eq!(class_of(&err), "java/lang/IllegalMonitorStateException");
        assert_eq!(message_of(&err), "current thread is not owner");
    }

    /// T12: JDK-exact argument validation messages (owner required for
    /// these to reach the range checks).
    #[test]
    fn wait_argument_validation_is_jdk_exact() {
        let _rt = crate::test_support::runtime();
        let r = new_object();
        monitor_enter(&r).unwrap();
        let err = object_wait(&r, -1, 0).unwrap_err();
        assert_eq!(class_of(&err), "java/lang/IllegalArgumentException");
        assert_eq!(message_of(&err), "timeout value is negative");
        let err = object_wait(&r, 1, 1_000_000).unwrap_err();
        assert_eq!(class_of(&err), "java/lang/IllegalArgumentException");
        assert_eq!(message_of(&err), "nanosecond timeout value out of range");
        let err = object_wait(&r, 1, -1).unwrap_err();
        assert_eq!(message_of(&err), "nanosecond timeout value out of range");
    }

    /// T11: the full re-entrancy count is released by `wait` and restored
    /// on return (re-entrancy across wait), and `notify` requires
    /// ownership while the waiter reacquires with the saved count.
    #[test]
    fn wait_releases_and_restores_full_count() {
        let _rt = crate::test_support::runtime();
        let r = new_object();
        monitor_enter(&r).unwrap();
        monitor_enter(&r).unwrap();
        assert_eq!(monitor_count(&r), 2);
        let r2 = r;
        let child = std::thread::spawn(move || {
            // Acquire (the waiter released everything), notify, exit.
            monitor_enter(&r2).unwrap();
            object_notify(&r2, false).unwrap();
            monitor_exit(&r2).unwrap();
        });
        object_wait(&r, 0, 0).unwrap(); // wait forever
        child.join().unwrap();
        // Reacquired with the FULL saved count.
        assert_eq!(monitor_count(&r), 2);
        assert!(monitor_owner(&r).is_some());
        monitor_exit(&r).unwrap();
        monitor_exit(&r).unwrap();
        assert_eq!(monitor_owner(&r), None);
    }

    /// T11: notify with an empty wait set is a no-op.
    #[test]
    fn notify_with_empty_wait_set_is_noop() {
        let _rt = crate::test_support::runtime();
        let r = new_object();
        monitor_enter(&r).unwrap();
        object_notify(&r, false).unwrap();
        object_notify(&r, true).unwrap();
        monitor_exit(&r).unwrap();
        assert_eq!(monitor_owner(&r), None);
    }

    /// T10: contended enter parks and the monitor changes hands; the
    /// previous owner can reacquire afterwards. Cross-thread, real park.
    #[test]
    fn contended_enter_parks_and_changes_hands() {
        let _rt = crate::test_support::runtime();
        let r = new_object();
        monitor_enter(&r).unwrap();
        let r2 = r;
        let child = std::thread::spawn(move || {
            // Blocks here until the parent exits below (T10 park path).
            monitor_enter(&r2).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
            monitor_exit(&r2).unwrap();
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
        monitor_exit(&r).unwrap(); // hand the monitor to the child
        child.join().unwrap();
        monitor_enter(&r).unwrap(); // reacquire after the child left
        monitor_exit(&r).unwrap();
        assert_eq!(monitor_owner(&r), None);
    }

    /// T12: a timed wait with no notifier returns normally after the
    /// deadline, with the monitor reacquired (nanos >= 500000 rounds up).
    #[test]
    fn timed_wait_returns_after_deadline() {
        let _rt = crate::test_support::runtime();
        let r = new_object();
        monitor_enter(&r).unwrap();
        let start = std::time::Instant::now();
        object_wait(&r, 50, 500_000).unwrap(); // effective 51ms
        let elapsed = start.elapsed();
        assert!(
            elapsed >= std::time::Duration::from_millis(50),
            "timed wait returned early: {elapsed:?}"
        );
        assert_eq!(monitor_count(&r), 1);
        monitor_exit(&r).unwrap();
        assert_eq!(monitor_owner(&r), None);
    }

    /// T11: notify wakes a waiting thread, which then recompetes and
    /// reacquires (Mesa semantics — the notifier still owns the monitor
    /// when notify returns).
    #[test]
    fn notify_wakes_the_waiter() {
        let _rt = crate::test_support::runtime();
        let r = new_object();
        let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let s2 = started.clone();
        let r2 = r;
        let child = std::thread::spawn(move || {
            monitor_enter(&r2).unwrap();
            s2.store(true, Ordering::Release);
            object_wait(&r2, 0, 0).unwrap();
            monitor_exit(&r2).unwrap();
        });
        while !started.load(Ordering::Acquire) {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        // The waiter released the monitor: we can acquire and notify.
        monitor_enter(&r).unwrap();
        object_notify(&r, false).unwrap();
        monitor_exit(&r).unwrap();
        child.join().unwrap();
        assert_eq!(monitor_owner(&r), None);
    }
}
