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
    let mut st = STATES.lock().unwrap_or_else(|e| e.into_inner());
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
            Status::Init { .. } => {
                st = CV.wait(st).unwrap_or_else(|e| e.into_inner());
            }
            Status::Uninit => break,
        }
    }
    st[class_id as usize].status = Status::Init { owner: me };
    drop(st);

    if let Some(f) = clinit {
        // The lock is NOT held here: clinit allocates (L2), throws, and
        // touches other classes' gates (cross-class init chains).
        let _ = f(None, &[]);
    }

    let mut st = STATES.lock().unwrap_or_else(|e| e.into_inner());
    st[class_id as usize].status = Status::Done;
    drop(st);
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
