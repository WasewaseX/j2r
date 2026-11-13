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
//! CLINIT-THROW (2026-11-13, found by the EXTERNAL-B wave): a clinit that
//! completes abruptly no longer silently marks the class Done. JLS 12.4.2
//! steps 11-12, Temurin 21.0.13-measured (ClinitProbe/ClinitProbe2):
//!   * first thrower: an Error (or EIIE — itself an Error subclass) is
//!     rethrown AS-IS; anything else is wrapped in a fresh
//!     ExceptionInInitializerError whose message is null and whose cause
//!     slot holds the original object (the JDK's `EIIE(Throwable)` ctor
//!     shape: `getException()` AND `getCause()` answer the original);
//!   * the class is marked ERRANT with the two pre-formatted strings the
//!     errant path needs — Temurin builds, on every later first-use, a
//!     `NoClassDefFoundError: Could not initialize class <binary name>`
//!     whose cause is a FRESH `ExceptionInInitializerError` carrying the
//!     message `Exception <E.toString()> [in thread "<name>"]` (name of
//!     the thread that ran the failing clinit, captured at failure time —
//!     a later access from another thread still prints the failing
//!     thread's name), with `getException()` and `getCause()` both null.
//!
//! Lock law: no heap access happens while the state lock is held — the
//! clinit itself runs with the lock DROPPED (it allocates; L2), and the
//! error-path message formatting (a `toString` dispatch that can re-enter
//! Java) also runs with the lock dropped.

use crate::thread::current_record;
use crate::{MethodFn, Throw, Value};
use std::sync::{Condvar, Mutex};

#[derive(Clone, PartialEq, Eq)]
enum Status {
    Uninit,
    Init {
        owner: usize,
    },
    Done,
    /// JLS 12.4.2 step 12: a clinit that completed abruptly marks the
    /// class erroneous. Only pre-formatted strings live here — never a
    /// JRef: this table is process-global and invisible to the tracing
    /// GC, so the original throwable CANNOT be stored (it is rooted by
    /// the caller instead, or dies; the errant path rebuilds fresh
    /// throwables from the strings on every access, exactly like the
    /// JVM's fresh NCDFE per access).
    Errant {
        ncdfd_msg: String,
        cause_msg: String,
    },
}

#[derive(Clone, PartialEq, Eq)]
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

/// Internal class names by class id (`a/b/C`), registered by the
/// generated `main_run` before any code runs. The errant path needs the
/// name for the NCDFE message; ids without a row (runtime unit tests use
/// synthetic ids) fall back to a synthetic label so the message is never
/// empty.
static CLASS_NAMES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Generated-crate hook: register internal class names indexed by class
/// id. Called once from `main_run` (both emission modes) before any Java
/// code runs; safe to call again (later calls overwrite row-wise).
pub fn register_class_names(names: &[&str]) {
    let mut cn = CLASS_NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if cn.len() < names.len() {
        cn.resize(names.len(), String::new());
    }
    for (i, n) in names.iter().enumerate() {
        cn[i] = n.to_string();
    }
}

/// Binary name (`a.b.C$D`) for the NCDFE message (Temurin's shape).
fn binary_name(class_id: u32) -> String {
    let cn = CLASS_NAMES.lock().unwrap_or_else(|e| e.into_inner());
    match cn.get(class_id as usize) {
        Some(n) if !n.is_empty() => n.replace('/', "."),
        _ => format!("class#{class_id}"),
    }
}

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

/// The current Java thread's visible name (the errant EIIE message rides
/// it, Temurin-observed).
fn current_thread_name() -> String {
    current_record()
        .name
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// `E.toString()` through the real virtual dispatch — a user Throwable
/// subclass may override toString and Temurin's errant message rides the
/// same surface. Called with the state lock DROPPED (it re-enters Java).
fn throwable_to_string(e: &Throw) -> String {
    crate::throwable_display(e, crate::jni::host_dispatch())
}

/// JLS 12.4.2 step 11, the first-thrower law: an Error (ExceptionInInitial
/// izerError included — it is a LinkageError) is rethrown as-is; anything
/// else is wrapped in a fresh EIIE with a null message and the original in
/// the cause slot (the JDK's `EIIE(Throwable)` ctor shape, Temurin-measured).
fn first_thrower_law(e: Throw) -> Throw {
    let cls = e.java_class();
    let d = crate::jni::host_dispatch();
    if crate::is_subtype(&cls, "java/lang/Error", d.parents) {
        return e;
    }
    let wrap = crate::heap::alloc(crate::JObj {
        jclass: std::sync::Arc::from("java/lang/ExceptionInInitializerError"),
        slots: vec![Value::Ref(None), Value::Ref(Some(e.obj)), Value::Ref(None)],
        prim: None,
    });
    Throw { obj: wrap }
}

/// The errant-access throw (JLS 12.4.2 step 5's NCDFE, Temurin's exact
/// shapes): `NoClassDefFoundError: Could not initialize class <binary>`
/// with a fresh `ExceptionInInitializerError` in the cause slot — built
/// with the STRING ctor shape (message set, `exception` field null and
/// `cause` null, Temurin-measured `getException()==null`).
fn errant_throw(ncdfd_msg: &str, cause_msg: &str) -> Throw {
    let cause = crate::throw_new("java/lang/ExceptionInInitializerError", Some(cause_msg));
    let ncdf = crate::throw_new("java/lang/NoClassDefFoundError", Some(ncdfd_msg));
    crate::heap::with_obj_mut(ncdf.obj, |o| {
        o.slots[1] = Value::Ref(Some(cause.obj));
    });
    ncdf
}

/// Runs `<clinit>` for `class_id` exactly once process-wide; concurrent
/// first-touchers block until it completes; same-thread re-entry (clinit
/// touching its own statics) proceeds without blocking (JLS 12.4.2).
/// A clinit that completes abruptly marks the class ERRANT: the first
/// thrower's error follows the step-11 law, every later first-use throws
/// the step-5 NCDFE with the errant cause chain.
pub fn ensure(class_id: u32, clinit: Option<MethodFn>) -> Result<(), Throw> {
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
        match st[class_id as usize].status.clone() {
            Status::Done => return Ok(()),
            // Recursion: the clinit of this class touching its own
            // statics must not self-deadlock (JLS: "in initialization by
            // the current thread" re-enters).
            Status::Init { owner } if owner == me => return Ok(()),
            // Errant: JLS 12.4.2 step 5 — initialization is not possible;
            // every later first-use throws the NCDFE with the errant
            // cause chain (fresh objects each access, like the JVM).
            // Building them ALLOCATES — drop the state lock first (L2:
            // no heap access under the state lock; alloc can trigger the
            // collector).
            Status::Errant {
                ncdfd_msg,
                cause_msg,
            } => {
                drop(st);
                return Err(errant_throw(&ncdfd_msg, &cause_msg));
            }
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
        if let Err(e) = f(None, &[]) {
            // JLS 12.4.2 steps 11-12. The message formatting dispatches
            // into Java (a toString override) and must run with the
            // state lock DROPPED (L2); the class stays in Init{owner:
            // me} until the Errant store below, so competing threads
            // keep parking — exactly the JVM's window.
            let ncdfd_msg = format!("Could not initialize class {}", binary_name(class_id));
            let cause_msg = format!(
                "Exception {} [in thread \"{}\"]",
                throwable_to_string(&e),
                current_thread_name(),
            );
            let mut st = states_lock(trace);
            st[class_id as usize].status = Status::Errant {
                ncdfd_msg,
                cause_msg,
            };
            drop(st);
            if trace {
                STATES_OWNER.store(-1, std::sync::atomic::Ordering::Relaxed);
                eprintln!("clerr cls={class_id} tid={me}");
            }
            CV.notify_all();
            return Err(first_thrower_law(e));
        }
    }

    let mut st = states_lock(trace);
    st[class_id as usize].status = Status::Done;
    drop(st);
    if trace {
        STATES_OWNER.store(-1, std::sync::atomic::Ordering::Relaxed);
        eprintln!("cldone cls={class_id} tid={me}");
    }
    CV.notify_all();
    Ok(())
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
        ensure(c, Some(fake_clinit)).expect("ok");
        ensure(c, Some(fake_clinit)).expect("ok");
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
        let child = std::thread::spawn(move || ensure(c, Some(fake_clinit)).is_ok());
        std::thread::sleep(std::time::Duration::from_millis(5));
        // Main demands the same class: must BLOCK until the child's clinit
        // finished (RUNS == 1), never run its own copy.
        ensure(c, None).expect("ok");
        assert_eq!(RUNS.load(Ordering::SeqCst), 1);
        assert!(child.join().unwrap());
        assert_eq!(RUNS.load(Ordering::SeqCst), 1); // still exactly one run
    }

    // ---- CLINIT-THROW (JLS 12.4.2 steps 11-12) ----

    /// Helper: read a Throwable's detail-message slot (slot 0) as a String.
    /// The JRef is copied OUT of the with_obj closure and dereferenced
    /// AFTER it — string_value itself takes the heap lock, and nesting it
    /// inside with_obj would self-deadlock the non-reentrant lock (L3:
    /// no heap access inside a with_obj closure).
    fn msg_of(t: &Throw) -> Option<String> {
        let s = crate::heap::with_obj(t.obj, |o| match &o.slots[0] {
            Value::Ref(Some(s)) => Some(*s),
            _ => None,
        });
        s.map(|s| crate::object::string_value(&s).to_string())
    }

    /// Helper: read a Throwable's cause slot (slot 1).
    fn cause_of(t: &Throw) -> Option<Throw> {
        crate::heap::with_obj(t.obj, |o| match &o.slots[1] {
            Value::Ref(Some(c)) => Some(Throw { obj: *c }),
            _ => None,
        })
    }

    /// A clinit throwing a non-Error: the first thrower gets a fresh
    /// ExceptionInInitializerError (message null, cause = the original
    /// object — the JDK's EIIE(Throwable) ctor shape).
    #[test]
    fn non_error_is_wrapped_in_eiie() {
        let _rt = crate::test_support::runtime();
        let c = next_class();
        fn thrower(
            _r: Option<&crate::JRef>,
            _a: &[crate::Value],
        ) -> Result<crate::Value, crate::Throw> {
            Err(crate::throw_new(
                "java/lang/IllegalStateException",
                Some("boom-rt"),
            ))
        }
        let t = ensure(c, Some(thrower)).expect_err("must throw");
        assert_eq!(t.java_class(), "java/lang/ExceptionInInitializerError");
        assert_eq!(msg_of(&t), None); // Temurin: the wrapper's message is null
        let cause = cause_of(&t).expect("cause slot holds the original");
        assert_eq!(cause.java_class(), "java/lang/IllegalStateException");
        assert_eq!(msg_of(&cause).as_deref(), Some("boom-rt"));
    }

    /// A clinit throwing an Error subclass: rethrown AS-IS (same class,
    /// same message — Temurin/ClinitProbe err-1).
    #[test]
    fn error_is_rethrown_as_is() {
        let _rt = crate::test_support::runtime();
        let c = next_class();
        fn thrower(
            _r: Option<&crate::JRef>,
            _a: &[crate::Value],
        ) -> Result<crate::Value, crate::Throw> {
            Err(crate::throw_new(
                "java/lang/AssertionError",
                Some("boom-err"),
            ))
        }
        let t = ensure(c, Some(thrower)).expect_err("must throw");
        assert_eq!(t.java_class(), "java/lang/AssertionError");
        assert_eq!(msg_of(&t).as_deref(), Some("boom-err"));
    }

    /// After an errant clinit, every later first-use throws NCDFE
    /// "Could not initialize class <binary name>" whose cause is a fresh
    /// EIIE with the "Exception <E> [in thread \"<name>\"]" message and
    /// NO further cause (Temurin/ClinitProbe rt-2/rt-3).
    #[test]
    fn errant_access_throws_ncdfe_with_eiie_cause() {
        let _rt = crate::test_support::runtime();
        let c = next_class();
        register_class_names_named(c, "probe/Errant");
        fn thrower(
            _r: Option<&crate::JRef>,
            _a: &[crate::Value],
        ) -> Result<crate::Value, crate::Throw> {
            Err(crate::throw_new(
                "java/lang/IllegalStateException",
                Some("boom-rt"),
            ))
        }
        let _ = ensure(c, Some(thrower)).expect_err("first access throws");
        // Second and third access (the law must be repeatable).
        for _k in 0..2 {
            let t = ensure(c, None).expect_err("errant access throws");
            assert_eq!(t.java_class(), "java/lang/NoClassDefFoundError");
            assert_eq!(
                msg_of(&t).as_deref(),
                Some("Could not initialize class probe.Errant")
            );
            let cause = cause_of(&t).expect("NCDFE cause = the errant EIIE");
            assert_eq!(cause.java_class(), "java/lang/ExceptionInInitializerError");
            let m = msg_of(&cause).expect("EIIE carries the formatted message");
            assert!(
                m.starts_with("Exception java.lang.IllegalStateException: boom-rt [in thread \""),
                "unexpected errant message: {m}"
            );
            assert!(m.ends_with("\"]"), "unexpected errant message: {m}");
            assert!(cause_of(&cause).is_none(), "errant EIIE cause is null");
        }
        // The clinit ran exactly once: the errant state never re-runs it.
    }

    /// register_class_names + binary_name (the message uses the binary
    /// form: dots for packages, `$` kept).
    #[test]
    fn binary_name_form() {
        let _rt = crate::test_support::runtime();
        let c = next_class();
        register_class_names_named(c, "a/b/C$Inner");
        assert_eq!(binary_name(c), "a.b.C$Inner");
        assert_eq!(binary_name(c + 1), format!("class#{}", c + 1));
    }

    /// Test hook: register a single name for a synthetic id (the global
    /// table is shared by tests, so a targeted row-write is used).
    fn register_class_names_named(class_id: u32, name: &str) {
        let mut cn = CLASS_NAMES.lock().unwrap_or_else(|e| e.into_inner());
        if cn.len() <= class_id as usize {
            cn.resize(class_id as usize + 1, String::new());
        }
        cn[class_id as usize] = name.to_string();
    }
}
