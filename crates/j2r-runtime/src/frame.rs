//! Java frame registry — Wave 1 of M2 (`docs/M2_PLAN.md` §3).
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
//! specifies; the Wave-1 CI calibration test re-measures the live footprint
//! and fails the build if it exceeds 1.25× the const.
//!
//! `J2R_MAX_JAVA_FRAMES` values outside `1..=MAX_FRAMES_ABS` and
//! non-numeric values are operator errors, not Java-observable results: the
//! generated `main` calls [`validate_env_or_exit`] first thing, which prints
//! `j2r: invalid J2R_MAX_JAVA_FRAMES=<v>` and exits with status 2 before any
//! Java code runs.

use crate::Throw;
use std::cell::Cell;

/// Native stack reserved for the generated program's worker thread
/// (`emit.rs` main template spawns it with exactly this size).
pub const WORKER_STACK_BYTES: usize = 512 * 1024 * 1024;

/// Measured worst-case native-stack bytes per Java frame (M2_PLAN §3.1.1:
/// 208–304 B measured on M1.6-generated code; 512 gives 1.68× headroom).
/// The CI calibration test asserts the live footprint stays ≤ 1.25× this
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
}

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
            Ok(n) if n >= 1 && n <= MAX_FRAMES_ABS => Ok(n),
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

/// Registration ticket for one live Java frame.
///
/// Wave 1 carries no pointers: the record is the process depth counter, so
/// there is nothing to dereference and nothing to leak. Wave 2 (GC) extends
/// this type with the frame's owned root-snapshot slot (M2_PLAN §4.4).
#[derive(Debug)]
pub struct FrameGuard {
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
            _not_send: std::marker::PhantomData,
        })
    }

    /// Current registered depth (test/observation helper).
    pub fn current_depth() -> usize {
        DEPTH.with(|d| d.get())
    }
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        // Runs on normal return, on `Err` propagation and on panic unwind:
        // the depth always returns to its pre-call value.
        DEPTH.with(|d| {
            let n = d.get();
            debug_assert!(n >= 1, "j2r internal: FrameGuard underflow");
            d.set(n.saturating_sub(1));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn const_math_pins_the_ceiling() {
        assert_eq!(MAX_FRAMES_ABS, 131_072);
        assert_eq!(DEFAULT_FRAMES, 8_192);
        // The 8× margin must hold: default depth × measured footprint stays
        // far below the worker stack.
        assert!(DEFAULT_FRAMES * FRAME_FOOTPRINT * 8 <= WORKER_STACK_BYTES);
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
        let msg = t.obj.borrow().slots[0].clone();
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
        assert_eq!(FrameGuard::current_depth(), limit(), "refused frame must not register");
        drop(guards);
        assert_eq!(FrameGuard::current_depth(), before);
    }
}
