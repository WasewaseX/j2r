//! Java frame registry — Wave 1 + Wave 2 of M2 (`docs/M2_PLAN.md` §3, §4.4).
//!
//! Generated methods call [`FrameGuard::enter`] as their first statement.
//! Construction checks the Java-frame depth: past the limit it returns the
//! [`Throw`] for `java/lang/StackOverflowError` (a catchable
//! `VirtualMachineError`, exactly like the JVM) instead of registering, and
//! the emitted code turns that into `return Err(..)` before doing any work.
//! `Drop` pops the frame — correct on normal return, on `Err` propagation
//! and on panic unwind alike.
//!
//! ## The depth limit and its hard safety ceiling (M2_PLAN §3.1, §3.1.1)
//!
//! Java call depth == native thread-stack depth (the approved M1 call ABI),
//! so a depth limit is what stands between a recursive Java program and a
//! native stack overflow. The limit defaults to [`DEFAULT_FRAMES`] and is
//! overridable through `J2R_MAX_JAVA_FRAMES`, but only up to
//! [`MAX_FRAMES_ABS`]:
//!
//! ```text
//! MAX_FRAMES_ABS = WORKER_STACK_BYTES / (FRAME_FOOTPRINT × 8)
//!                = 536_870_912 / (512 × 8)
//!                = 131_072
//! ```
//!
//! `FRAME_FOOTPRINT` is the measured native-stack cost of one Java frame
//! (worst shape 304 B — pure recursion measured 208 B/frame, recursion with
//! a native dispatch per level 304 B/frame; see M2_PLAN §3.1.1 for the
//! calibration method). The 8× factor is the safety margin the plan
//! specifies; the CI calibration e2e runs at the ceiling with the release
//! profile and must not abort.
//!
//! `J2R_MAX_JAVA_FRAMES` values outside `1..=MAX_FRAMES_ABS` and
//! non-numeric values are operator errors, not Java-observable results: the
//! generated `main` calls [`validate_env_or_exit`] first thing, which prints
//! `j2r: invalid J2R_MAX_JAVA_FRAMES=<v>` and exits with status 2 before any
//! Java code runs.
//!
//! ## The root-snapshot registry (Wave 2, M2_PLAN §4.4 — pointer-free)
//!
//! Every registered frame owns one *snapshot slot* in the LIFO registry
//! below: an owned `Vec<u32>` of ref handle indices, capacity reused across
//! calls (the steady state is allocation-free). Generated code refreshes the
//! slot before **every call of any kind** (user, native, virtual, static,
//! concat, `<clinit>` dispatch) and at every collection point, so a
//! suspended frame's snapshot is exact for the whole suspension. The heap
//! scans all occupied slots when collecting; stale indices in a snapshot are
//! over-approximation residue and are skipped at mark time, while any *use*
//! of a stale handle panics loudly (heap accessors check generations).
//!
//! [`NativeRoots`] gives native code the same rooting for refs it holds
//! across a dispatch into user code (e.g. `toString` during concatenation).

use crate::Throw;
use std::cell::Cell;

/// Native stack reserved for the generated program's worker thread
/// (`emit.rs` main template spawns it with exactly this size).
pub const WORKER_STACK_BYTES: usize = 512 * 1024 * 1024;

/// Measured worst-case native-stack bytes per Java frame (M2_PLAN §3.1.1:
/// 208–304 B measured on M1.6-generated code; 512 gives 1.68× headroom).
/// The CI calibration asserts the live footprint stays ≤ 1.25× this
/// constant (≤ 640 B).
pub const FRAME_FOOTPRINT: usize = 512;

/// Hard ceiling: no `J2R_MAX_JAVA_FRAMES` override above this is accepted,
/// because `MAX_FRAMES_ABS` frames of the measured footprint must still fit
/// the worker stack with the plan's 8× margin.
pub const MAX_FRAMES_ABS: usize = WORKER_STACK_BYTES / (FRAME_FOOTPRINT * 8);

/// Default Java-frame limit (same order of magnitude as HotSpot's default
/// thread-stack depth).
pub const DEFAULT_FRAMES: usize = 8192;

// Compile-time pin: a footprint regression that shrinks the ceiling below
// the default must fail the build, never silently erode the safety margin.
const _: () = assert!(
    DEFAULT_FRAMES <= MAX_FRAMES_ABS,
    "DEFAULT_FRAMES exceeds the measured hard ceiling"
);

// M4-SEAM (M4_PLAN §3 Wave 2 / T07): the depth and limit counters are
// owner-only scalars that never leave the thread — no collector or other
// thread ever reads them, so thread_local is their final shape. The frame
// SLOT storage itself moved to the per-thread `ThreadRecord` in
// `crate::thread` (T07), which the collector scans under the STW protocol
// (T08); that ended the last shared-state seam in the crate (gate-12
// census: these two thread_local! blocks plus thread.rs's CURRENT handle).
thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    static LIMIT: Cell<Option<usize>> = const { Cell::new(None) };
}

// ---------------------------------------------------------------- limit policy

/// Parses one `J2R_MAX_JAVA_FRAMES` value.
///
/// * `None` (variable unset) → the default.
/// * Numeric and inside `1..=MAX_FRAMES_ABS` → the value.
/// * Anything else (0, above the ceiling, non-numeric, empty) → `Err` with
///   the raw text for the diagnostic.
pub fn parse_limit(raw: Option<&str>) -> Result<usize, String> {
    match raw {
        None => Ok(DEFAULT_FRAMES),
        Some(s) => match s.parse::<usize>() {
            Ok(n) if (1..=MAX_FRAMES_ABS).contains(&n) => Ok(n),
            _ => Err(s.to_string()),
        },
    }
}

/// Validates `J2R_MAX_JAVA_FRAMES` and exits the process with status 2 on an
/// invalid value — an operator error, never a Java-observable result. The
/// generated `main` calls this as its very first statement, before the
/// worker thread (and therefore before any Java code) starts.
pub fn validate_env_or_exit() -> usize {
    match parse_limit(std::env::var("J2R_MAX_JAVA_FRAMES").ok().as_deref()) {
        Ok(n) => {
            LIMIT.with(|l| l.set(Some(n)));
            n
        }
        Err(bad) => {
            eprintln!("j2r: invalid J2R_MAX_JAVA_FRAMES={bad}");
            std::process::exit(2);
        }
    }
}

/// The effective limit for this thread (cached after the first read).
fn limit() -> usize {
    LIMIT.with(|l| {
        if let Some(n) = l.get() {
            return n;
        }
        let n = parse_limit(std::env::var("J2R_MAX_JAVA_FRAMES").ok().as_deref())
            .unwrap_or(MAX_FRAMES_ABS);
        // An invalid value never reaches here in a generated program (main
        // validated first); in direct runtime use, fail closed to the safe
        // ceiling rather than exit a caller that may not own the process.
        l.set(Some(n));
        n
    })
}

// ------------------------------------------------------------- slot registry
//
// The slot storage lives in the current thread's `ThreadRecord` (T07,
// `crate::thread`): one `Mutex<SlotState>` per Java thread. The owning
// thread locks its own record; the collector locks each record in turn
// under the STW protocol (everyone parked — no contention, and the slot
// mutex is a leaf lock: law L4 in thread.rs's module docs).

use crate::thread::frame_slots::SlotState;

fn with_slots<R>(f: impl FnOnce(&mut SlotState) -> R) -> R {
    let rec = crate::thread::current_record();
    let mut s = rec.slots.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut s)
}

fn take_slot() -> usize {
    with_slots(|s| {
        if let Some(i) = s.free.pop() {
            s.slots[i] = Some(Vec::new());
            return i;
        }
        s.slots.push(Some(Vec::new()));
        s.slots.len() - 1
    })
}

fn release_slot(slot: usize) {
    with_slots(|s| {
        if let Some(v) = s.slots[slot].take() {
            s.pool.push(v);
        }
        s.free.push(slot);
    })
}

/// Hands every occupied snapshot slot of EVERY registered thread to the
/// collector (heap-side root scan; M2_PLAN §4.4, T08). Released slots are
/// `None` and skipped. Runs under the STW protocol: no thread mutates its
/// slots while this walks (owners are parked), and each record's mutex is
/// taken alone (L4/L5).
pub(crate) fn for_each_live_slot(mut f: impl FnMut(&[u32])) {
    for rec in crate::thread::registry_records() {
        let s = rec.slots.lock().unwrap_or_else(|e| e.into_inner());
        for slot in s.slots.iter().flatten() {
            f(slot);
        }
    }
}

// ---------------------------------------------------------------- FrameGuard

/// Registration ticket for one live Java frame.
///
/// Wave 1 carried no pointers (a depth counter only); Wave 2 adds the
/// frame's owned root-snapshot slot — still no raw pointers, no `unsafe`
/// (M2_PLAN §4.4).
#[derive(Debug)]
pub struct FrameGuard {
    slot: usize,
    /// Monitors acquired in this frame and not yet exited (M3 Wave 4):
    /// `monitorenter` pushes after acquiring, `monitorexit` pops, and the
    /// `Drop` releases the remainder — so an exceptional exit that outruns
    /// javac's structured `monitorexit` paths still cannot leak a lock
    /// (M3_PLAN §7.1, the monitor analogue of the root-snapshot slot).
    /// Capacity 8 inline covers the 99% case without reallocation.
    monitors: std::cell::RefCell<Vec<crate::heap::JRef>>,
    _not_send: std::marker::PhantomData<*const ()>, // registry is thread_local
}

impl FrameGuard {
    /// Registers the current Java frame or fails with a catchable
    /// `StackOverflowError` when the depth limit is reached. Must be the
    /// first statement of every generated method body.
    pub fn enter() -> Result<FrameGuard, Throw> {
        let next = DEPTH.with(|d| d.get()) + 1;
        if next > limit() {
            return Err(crate::throw_new("java/lang/StackOverflowError", None));
        }
        DEPTH.with(|d| d.set(next));
        Ok(FrameGuard {
            slot: take_slot(),
            monitors: std::cell::RefCell::new(Vec::with_capacity(8)),
            _not_send: std::marker::PhantomData,
        })
    }

    /// Refreshes this frame's root snapshot from its current `locals`,
    /// operand `stack` and pending `thrown` object. Generated code emits
    /// this before every call of any kind and at every collection point, so
    /// the snapshot is exact for as long as the frame stays suspended.
    pub fn refresh(&self, locals: &[crate::Value], stack: &[crate::Value], thrown: &Option<Throw>) {
        let mut handles: Vec<u32> = with_slots(|s| s.pool.pop()).unwrap_or_default();
        handles.clear();
        for v in locals {
            if let crate::Value::Ref(Some(h)) = v {
                handles.push(h.index());
            }
        }
        for v in stack {
            if let crate::Value::Ref(Some(h)) = v {
                handles.push(h.index());
            }
        }
        if let Some(t) = thrown {
            handles.push(t.obj.index());
        }
        with_slots(|s| {
            // The slot exists for the guard's whole lifetime.
            s.slots[self.slot] = Some(handles);
        });
    }

    /// Records a monitor acquisition made by this frame (after a successful
    /// `heap::monitor_enter`).
    pub fn record_monitor(&self, r: crate::heap::JRef) {
        self.monitors.borrow_mut().push(r);
    }

    /// Pops the record for a successful `monitorexit` of `r` (the frame may
    /// hold several monitors; only the most recent matching record goes).
    pub fn unrecord_monitor(&self, r: &crate::heap::JRef) {
        let mut ms = self.monitors.borrow_mut();
        if let Some(pos) = ms.iter().rposition(|m| m == r) {
            ms.remove(pos);
        }
    }

    /// Current registered depth (test/observation helper).
    pub fn current_depth() -> usize {
        DEPTH.with(|d| d.get())
    }
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        // Release any monitors the frame still holds (normal return after
        // ACC_SYNCHRONIZED prologues, or exceptional exits that outrun
        // javac's structured release paths). LIFO order.
        for r in self.monitors.borrow().iter().rev() {
            crate::heap::monitor_release(r);
        }
        self.monitors.borrow_mut().clear();
        // Runs on normal return, on `Err` propagation and on panic unwind:
        // the depth always returns to its pre-call value. (release moved
        // into a plain method so both Drop paths share it; this Drop runs
        // exactly once per guard.)
        let slot = self.slot;
        with_slots(|s| {
            if let Some(v) = s.slots[slot].take() {
                s.pool.push(v);
            }
            s.free.push(slot);
        });
        DEPTH.with(|d| {
            let n = d.get();
            debug_assert!(n >= 1, "j2r internal: FrameGuard underflow");
            d.set(n.saturating_sub(1));
        });
    }
}

// ---------------------------------------------------------------- NativeRoots

/// Roots a set of handles for the duration of the scope: native code
/// dispatching into user code (concat → `toString`, future comparators)
/// wraps every ref it holds live across the dispatch (M2_PLAN §4.4).
#[derive(Debug)]
pub struct NativeRoots {
    slot: usize,
    _not_send: std::marker::PhantomData<*const ()>,
}

impl NativeRoots {
    pub fn push(handles: &[crate::heap::JRef]) -> NativeRoots {
        let (v, slot) = with_slots(|s| {
            let v = s.pool.pop().unwrap_or_default();
            let slot = s.free.pop().unwrap_or_else(|| {
                s.slots.push(None);
                s.slots.len() - 1
            });
            (v, slot)
        });
        let mut v = v;
        v.clear();
        v.extend(handles.iter().map(|h| h.index()));
        with_slots(|s| s.slots[slot] = Some(v));
        NativeRoots {
            slot,
            _not_send: std::marker::PhantomData,
        }
    }
}

impl Drop for NativeRoots {
    fn drop(&mut self) {
        release_slot(self.slot);
    }
}

// ------------------------------------------------------------- test plumbing

/// Runs `f` with one extra registered slot (simulates a suspended frame in
/// unit tests).
#[cfg(test)]
pub(crate) fn with_test_frame(f: impl FnOnce()) {
    let slot = take_slot();
    f();
    release_slot(slot);
}

/// Writes `handles` into the most recently taken slot (test-only stand-in
/// for `FrameGuard::refresh` in tests that have no guard).
#[cfg(test)]
pub(crate) fn test_refresh_slot(handles: &[crate::heap::JRef]) {
    let idx = with_slots(|s| {
        s.slots
            .iter()
            .rposition(|o| o.is_some())
            .expect("no live slot")
    });
    let v: Vec<u32> = handles.iter().map(|h| h.index()).collect();
    with_slots(|s| s.slots[idx] = Some(v));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::JRef;

    #[test]
    fn const_math_pins_the_ceiling() {
        let _rt = crate::test_support::runtime();
        assert_eq!(MAX_FRAMES_ABS, 131_072);
        assert_eq!(DEFAULT_FRAMES, 8_192);
        const { assert!(DEFAULT_FRAMES * FRAME_FOOTPRINT * 8 <= WORKER_STACK_BYTES) };
    }

    #[test]
    fn parse_limit_accepts_default_min_max() {
        let _rt = crate::test_support::runtime();
        assert_eq!(parse_limit(None).unwrap(), DEFAULT_FRAMES);
        assert_eq!(parse_limit(Some("1")).unwrap(), 1);
        assert_eq!(parse_limit(Some("8192")).unwrap(), 8192);
        assert_eq!(parse_limit(Some("131072")).unwrap(), MAX_FRAMES_ABS);
    }

    #[test]
    fn parse_limit_rejects_zero_over_and_non_numeric() {
        let _rt = crate::test_support::runtime();
        assert!(parse_limit(Some("0")).is_err());
        assert!(parse_limit(Some("131073")).is_err());
        assert!(parse_limit(Some("abc")).is_err());
        assert!(parse_limit(Some("")).is_err());
        assert!(parse_limit(Some("-1")).is_err());
        assert!(parse_limit(Some("99999999999999999999")).is_err());
    }

    #[test]
    fn depth_accounting_across_nested_guards() {
        let _rt = crate::test_support::runtime();
        let before = FrameGuard::current_depth();
        {
            let _a = FrameGuard::enter().unwrap();
            assert_eq!(FrameGuard::current_depth(), before + 1);
            {
                let _b = FrameGuard::enter().unwrap();
                assert_eq!(FrameGuard::current_depth(), before + 2);
            }
            assert_eq!(FrameGuard::current_depth(), before + 1);
        }
        assert_eq!(FrameGuard::current_depth(), before);
    }

    #[test]
    fn depth_restored_on_panic_unwind() {
        let _rt = crate::test_support::runtime();
        let before = FrameGuard::current_depth();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = FrameGuard::enter().unwrap();
            panic!("simulated internal bug");
        }));
        assert!(result.is_err());
        assert_eq!(FrameGuard::current_depth(), before);
    }

    #[test]
    fn soe_object_shape() {
        let _rt = crate::test_support::runtime();
        let t = crate::throw_new("java/lang/StackOverflowError", None);
        assert_eq!(t.java_class(), "java/lang/StackOverflowError");
        // No message (like the JVM's no-arg StackOverflowError constructor).
        let msg = crate::heap::with_obj(t.obj, |o| o.slots[0].clone());
        assert!(matches!(msg, crate::Value::Ref(None)));
        // Subtype of VirtualMachineError per the builtin hierarchy.
        assert!(crate::is_subtype(
            "java/lang/StackOverflowError",
            "java/lang/VirtualMachineError",
            None
        ));
        assert!(crate::is_subtype(
            "java/lang/StackOverflowError",
            "java/lang/Throwable",
            None
        ));
    }

    #[test]
    fn enter_returns_err_at_the_limit() {
        let _rt = crate::test_support::runtime();
        // Drive the counter straight to the limit with a fake deep chain,
        // then prove `enter` refuses and the refused frame did not register.
        let before = FrameGuard::current_depth();
        let deficit = limit() - before;
        let mut guards = Vec::new();
        for _ in 0..deficit {
            guards.push(FrameGuard::enter().unwrap());
        }
        assert_eq!(FrameGuard::current_depth(), limit());
        let t = FrameGuard::enter().unwrap_err();
        assert_eq!(t.java_class(), "java/lang/StackOverflowError");
        assert_eq!(
            FrameGuard::current_depth(),
            limit(),
            "refused frame must not register"
        );
        drop(guards);
        assert_eq!(FrameGuard::current_depth(), before);
    }

    #[test]
    fn slots_are_lifo_and_reused() {
        let _rt = crate::test_support::runtime();
        let before = FrameGuard::current_depth();
        let h = crate::heap::alloc(crate::object::JObj {
            jclass: std::sync::Arc::from("test/S"),
            slots: vec![],
        });
        {
            let g1 = FrameGuard::enter().unwrap();
            g1.refresh(&[], &[crate::Value::Ref(Some(h))], &None);
            {
                let g2 = FrameGuard::enter().unwrap();
                // inner guard's slot is separate and empty
                let mut seen = 0;
                for_each_live_slot(|s| seen += s.len());
                assert!(seen >= 1);
                drop(g2);
            }
            // outer slot still holds its handle
            let mut found = false;
            for_each_live_slot(|s| found |= s.contains(&h.index()));
            assert!(found, "outer frame slot must keep its refreshed handles");
        }
        assert_eq!(FrameGuard::current_depth(), before);
        let _ = JRef::ptr_eq(&h, &h);
    }
}
