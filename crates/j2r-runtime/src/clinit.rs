//! JLS 12.4.2 class-initialization gate (M4 Wave 3, T13 finding).
//!
//! The generated statics pool used to set the per-class "initialized"
//! flag BEFORE running `<clinit>` and never blocked anyone else, so two
//! threads racing to first-touch a class could both proceed: the loser
//! skipped the clinit and read raw default values — a null static fed
//! straight into `monitorenter` (NPE) and one worker's entire contribution
//! vanished (caught by the MonitorContention differential fixture, 5/10).
//!
//! The JVM instead defines per-class states (JLS 12.4.2): the first thread
//! to demand initialization RUNS the clinit while every OTHER thread
//! blocks until the class reaches the Initialized state; re-entrant
//! demands from the SAME thread (a clinit touching its own class's
//! statics) proceed without blocking.
//!
//! Lock law: no heap access happens while the state lock is held — the
//! clinit itself runs with the lock DROPPED (it allocates; L2).

use crate::thread::current_record;
use crate::MethodFn;
use std::sync::{Condvar, Mutex};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Uninit,
    Init { owner: usize },
    Done,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ClassState {
    status: Status,
}

static STATES: Mutex<Vec<ClassState>> = Mutex::new(Vec::new());
static CV: Condvar = Condvar::new();
/// Debug (T17.1): registry index of the thread currently holding the
/// STATES mutex, or -1. Written by the lock helper below; the STW
/// quiescence dump prints it so a frozen wait names the holder.
pub(crate) static STATES_OWNER: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(-1);

fn states_lock(trace: bool) -> std::sync::MutexGuard<'static, Vec<ClassState>> {
    let g = STATES.lock().unwrap_or_else(|e| e.into_inner());
    if trace {
        let me = current_record().reg_index as i64;
        STATES_OWNER.store(me, std::sync::atomic::Ordering::Relaxed);
    }
    g
}

/// Pre-size the state vector to the program's class count (called from the
/// generated `init_pool`, which knows the number). Purely an optimization:
/// `ensure` grows the vector on demand otherwise.
pub fn reserve_classes(classes: usize) {
    let mut st = STATES.lock().unwrap_or_else(|e| e.into_inner());
    if st.len() < classes {
        st.resize(
            classes,
            ClassState {
                status: Status::Uninit,
            },
        );
    }
}

/// Runs `<clinit>` for `class_id` exactly once process-wide; concurrent
/// first-touchers block until it completes; same-thread re-entry (clinit
/// touching its own statics) proceeds without blocking (JLS 12.4.2).
pub fn ensure(class_id: u32, clinit: Option<MethodFn>) {
    let me = current_record().reg_index;
    let trace = std::env::var("J2R_STW_TRACE").is_ok();
    let mut st = states_lock(trace);
    if st.len() <= class_id as usize {
        st.resize(
            class_id as usize + 1,
            ClassState {
                status: Status::Uninit,
            },
        );
    }
    loop {
        match st[class_id as usize].status {
            Status::Done => return,
            // Recursion: the clinit of this class touching its own
            // statics must not self-deadlock (JLS: "in initialization by
            // the current thread" re-enters).
            Status::Init { owner } if owner == me => return,
            // Another thread is initializing: block until Done (the
            // JVM's thread blocks in "initializing" state too).
            //
            // STW participation (T17.1 — the MonitorContention N=1
            // freeze): a waiter blocked on this gate must NOT look
            // RUNNING to the collector. Under J2R_GC_EVERY_N=1 the
            // clinit-runner itself collects (its clinit allocates), and
            // quiescence waits for every RUNNING thread — a CV-blocked
            // waiter can never reach a safepoint, so the whole runtime
            // deadlocked before any monitor op ever ran (zero mon_ops
            // in the trace). Park across the wait (park_prep also hands
            // the coop turn on); wake_prep re-absorbs an in-flight STW
            // and restores Running. Lock law: park_prep never waits
            // (safe under the STATES guard); wake_prep is called with
            // the guard DROPPED (L7 — it takes the SCHED lock).
            Status::Init { .. } => {
                if trace {
                    eprintln!("clwait cls={class_id} tid={me}");
                }
                let rec = current_record();
                crate::thread::monitor_park_prep(&rec);
                st = CV.wait(st).unwrap_or_else(|e| e.into_inner());
                drop(st);
                if trace {
                    STATES_OWNER.store(-1, std::sync::atomic::Ordering::Relaxed);
                }
                crate::thread::monitor_wake_prep(&rec);
                st = states_lock(trace);
            }
            Status::Uninit => break,
        }
    }
    st[class_id as usize].status = Status::Init { owner: me };
    drop(st);
    if trace {
        STATES_OWNER.store(-1, std::sync::atomic::Ordering::Relaxed);
    }

    if std::env::var("J2R_STW_TRACE").is_ok() {
        eprintln!("clrun cls={class_id} tid={me}");
    }
    if let Some(f) = clinit {
        // The lock is NOT held here: clinit allocates (L2), throws, and
        // touches other classes' gates (cross-class init chains).
        let _ = f(None, &[]);
    }

    let mut st = states_lock(trace);
    st[class_id as usize].status = Status::Done;
    drop(st);
    if trace {
        STATES_OWNER.store(-1, std::sync::atomic::Ordering::Relaxed);
        eprintln!("cldone cls={class_id} tid={me}");
    }
    CV.notify_all();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    static RUNS: AtomicUsize = AtomicUsize::new(0);
    static GATE: AtomicBool = AtomicBool::new(false);
    static NEXT: AtomicUsize = AtomicUsize::new(1);

    fn next_class() -> u32 {
        NEXT.fetch_add(1, Ordering::SeqCst) as u32 + 500
    }

    fn fake_clinit(
        _r: Option<&crate::JRef>,
        _a: &[crate::Value],
    ) -> Result<crate::Value, crate::Throw> {
        std::thread::sleep(std::time::Duration::from_millis(30));
        RUNS.fetch_add(1, Ordering::SeqCst);
        GATE.store(true, Ordering::Release);
        Ok(crate::Value::Void)
    }

    /// First ensure runs the clinit; the same thread re-entering returns
    /// without running it again (recursion path).
    #[test]
    fn clinit_runs_once_and_reentry_is_free() {
        let _rt = crate::test_support::runtime();
        RUNS.store(0, Ordering::SeqCst);
        let c = next_class();
        ensure(c, Some(fake_clinit));
        ensure(c, Some(fake_clinit));
        assert_eq!(RUNS.load(Ordering::SeqCst), 1);
    }

    /// Concurrent first-touchers block: the loser waits for the winner's
    /// clinit instead of reading defaults (the MonitorContention bug).
    #[test]
    fn concurrent_first_touchers_block_until_done() {
        let _rt = crate::test_support::runtime();
        RUNS.store(0, Ordering::SeqCst);
        GATE.store(false, Ordering::SeqCst);
        let c = next_class();
        let child = std::thread::spawn(move || {
            ensure(c, Some(fake_clinit));
            GATE.load(Ordering::Acquire)
        });
        std::thread::sleep(std::time::Duration::from_millis(5));
        // Main demands the same class: must BLOCK until the child's clinit
        // finished (RUNS == 1), never run its own copy.
        ensure(c, None);
        assert_eq!(RUNS.load(Ordering::SeqCst), 1);
        assert!(child.join().unwrap());
        assert_eq!(RUNS.load(Ordering::SeqCst), 1); // still exactly one run
    }
}
