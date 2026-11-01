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
use std::cell::{Cell, RefCell};

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

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    static LIMIT: Cell<Option<usize>> = const { Cell::new(None) };

    /// The root-snapshot registry. `None` marks a released slot; released
    /// `Vec`s move to [`SLOT_POOL`] so capacity is reused across calls.
    static FRAME_SLOTS: RefCell<Vec<Option<Vec<u32>>>> = const { RefCell::new(Vec::new()) };
    static FREE_SLOTS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    static SLOT_POOL: RefCell<Vec<Vec<u32>>> = const { RefCell::new(Vec::new()) };
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

fn take_slot() -> usize {
    FREE_SLOTS.with(|f| {
        if let Some(i) = f.borrow_mut().pop() {
            FRAME_SLOTS.with(|s| s.borrow_mut()[i] = Some(Vec::new()));
            return i;
        }
        FRAME_SLOTS.with(|s| {
            let mut s = s.borrow_mut();
            s.push(Some(Vec::new()));
            s.len() - 1
        })
    })
}

fn release_slot(slot: usize) {
    let v = FRAME_SLOTS.with(|s| s.borrow_mut()[slot].take());
    if let Some(v) = v {
        SLOT_POOL.with(|p| p.borrow_mut().push(v));
    }
    FREE_SLOTS.with(|f| f.borrow_mut().push(slot));
}

/// Hands every occupied snapshot slot to the collector (heap-side root
/// scan; M2_PLAN §4.4). Released slots are `None` and skipped.
pub(crate) fn for_each_live_slot(mut f: impl FnMut(&[u32])) {
    FRAME_SLOTS.with(|s| {
        for slot in s.borrow().iter().flatten() {
            f(slot);
        }
    });
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
            _not_send: std::marker::PhantomData,
        })
    }

    /// Refreshes this frame's root snapshot from its current `locals`,
    /// operand `stack` and pending `thrown` object. Generated code emits
    /// this before every call of any kind and at every collection point, so
    /// the snapshot is exact for as long as the frame stays suspended.
    pub fn refresh(&self, locals: &[crate::Value], stack: &[crate::Value], thrown: &Option<Throw>) {
        let mut handles: Vec<u32> = SLOT_POOL.with(|p| p.borrow_mut().pop()).unwrap_or_default();
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
        FRAME_SLOTS.with(|s| {
            // The slot exists for the guard's whole lifetime.
            s.borrow_mut()[self.slot] = Some(handles);
        });
    }

    /// Current registered depth (test/observation helper).
    pub fn current_depth() -> usize {
        DEPTH.with(|d| d.get())
    }
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        // Runs on normal return, on `Err` propagation and on panic unwind:
        // the depth always returns to its pre-call value. (release moved
        // into a plain method so both Drop paths share it; this Drop runs
        // exactly once per guard.)
        let slot = self.slot;
        let v = FRAME_SLOTS.with(|s| s.borrow_mut()[slot].take());
        if let Some(v) = v {
            SLOT_POOL.with(|p| p.borrow_mut().push(v));
        }
        FREE_SLOTS.with(|f| f.borrow_mut().push(slot));
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
        let mut v: Vec<u32> = SLOT_POOL.with(|p| p.borrow_mut().pop()).unwrap_or_default();
        v.clear();
        v.extend(handles.iter().map(|h| h.index()));
        let slot = FREE_SLOTS.with(|f| {
            f.borrow_mut().pop().unwrap_or_else(|| {
                FRAME_SLOTS.with(|s| {
                    let mut s = s.borrow_mut();
                    s.push(None);
                    s.len() - 1
                })
            })
        });
        FRAME_SLOTS.with(|s| s.borrow_mut()[slot] = Some(v));
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
    let idx = FRAME_SLOTS.with(|s| {
        s.borrow()
            .iter()
            .rposition(|o| o.is_some())
            .expect("no live slot")
    });
    let v: Vec<u32> = handles.iter().map(|h| h.index()).collect();
    FRAME_SLOTS.with(|s| s.borrow_mut()[idx] = Some(v));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::JRef;

    #[test]
    fn const_math_pins_the_ceiling() {
        assert_eq!(MAX_FRAMES_ABS, 131_072);
        assert_eq!(DEFAULT_FRAMES, 8_192);
        const { assert!(DEFAULT_FRAMES * FRAME_FOOTPRINT * 8 <= WORKER_STACK_BYTES) };
    }

    #[test]
    fn parse_limit_accepts_default_min_max() {
        assert_eq!(parse_limit(None).unwrap(), DEFAULT_FRAMES);
        assert_eq!(parse_limit(Some("1")).unwrap(), 1);
        assert_eq!(parse_limit(Some("8192")).unwrap(), 8192);
        assert_eq!(parse_limit(Some("131072")).unwrap(), MAX_FRAMES_ABS);
    }

    #[test]
    fn parse_limit_rejects_zero_over_and_non_numeric() {
        assert!(parse_limit(Some("0")).is_err());
        assert!(parse_limit(Some("131073")).is_err());
        assert!(parse_limit(Some("abc")).is_err());
        assert!(parse_limit(Some("")).is_err());
        assert!(parse_limit(Some("-1")).is_err());
        assert!(parse_limit(Some("99999999999999999999")).is_err());
    }

    #[test]
    fn depth_accounting_across_nested_guards() {
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
        let before = FrameGuard::current_depth();
        let h = crate::heap::alloc(crate::object::JObj {
            jclass: std::rc::Rc::from("test/S"),
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
