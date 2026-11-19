//! `j2r-runtime` — the Java-semantics runtime for programs produced by the
//! J2R compiler.
//!
//! Generated crates link against this one. It provides:
//! * the uniform [`Value`] representation used on operand stacks, in locals,
//!   in object field slots and across the generated-call ABI,
//! * the heap model ([`JObj`] / [`JRef`]) with flattened field slots,
//! * array primitives, string/interning helpers and Java-exact number
//!   formatting ([`javafmt`]),
//! * the natives table (JDK methods implemented directly in Rust),
//! * builtin JDK class hierarchy knowledge ([`builtin`]) used for
//!   `instanceof`, `checkcast` and exception-handler matching,
//! * the [`j2r_try!`] macro used by every generated method body.
//!
//! Design notes (milestone 1):
//! * The heap is `Arc<RefCell<..>>`: single-threaded, reference-counted.
//!   Cycle-collecting GC and true Java threads are planned milestones; a
//!   cycle leak is *memory*, never *behaviour*, so program output stays
//!   correct.
//! * Every fallible generated operation returns `Result<_, Throw>`; nothing
//!   is silently ignored.

#![forbid(unsafe_code)]
pub mod array;
pub mod builtin;
pub mod clinit;
pub mod collections;
pub mod concat;
pub mod desc;
pub mod fdlibm;
pub mod formatter;
pub mod frame;
pub mod heap;
pub mod hexfloat;
pub mod io;
pub mod javafmt;
pub mod jdkutil;
pub mod jni;
pub mod lambda;
pub mod langext;
pub mod monitor;
pub mod natives;
pub mod object;
pub mod reflect;
pub mod regex;
pub mod stack;
pub mod subtype;
pub mod thread;

use std::sync::Arc;

pub use desc::{parse_field_desc, parse_method_desc, type_slots, unmarshal_args, TypeDesc};
pub use heap::JRef;
pub use object::{intern_string, object_class, JObj};
pub use stack::{pop_double, pop_float, pop_int, pop_long, pop_n, pop_ref, pop_ref_msg};
pub use subtype::is_subtype;
/// Test-only serialization for the shared runtime (M4 Wave 1). Tests used
/// to be isolated by thread_local state; the shared substrate makes the
/// runtime genuinely process-global, so runtime-touching unit tests take
/// this lock. NOT compiled outside tests; generated programs never see it.
#[cfg(test)]
pub(crate) mod test_support {
    use std::ops::{Deref, DerefMut};
    use std::sync::{Mutex, MutexGuard, OnceLock};
    static RUNTIME_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    /// Guard wrapping the test-serialization lock. On drop it marks the
    /// thread's registry record `Parked` — the thread has stopped
    /// mutating Java state. This is what keeps libtest's per-test threads
    /// from leaving zombie `Running` records that would stall every later
    /// STW collection (M4 Wave 2, found by the first thread-test run).
    pub struct RuntimeGuard {
        inner: MutexGuard<'static, ()>,
    }
    impl Deref for RuntimeGuard {
        type Target = ();
        fn deref(&self) -> &() {
            &self.inner
        }
    }
    impl DerefMut for RuntimeGuard {
        fn deref_mut(&mut self) -> &mut () {
            &mut self.inner
        }
    }
    impl Drop for RuntimeGuard {
        fn drop(&mut self) {
            crate::thread::mark_test_lock_released();
        }
    }

    pub fn runtime() -> RuntimeGuard {
        // M4 Wave 2: a thread waiting for this lock cannot mutate Java
        // state — mark its record Parked so the STW collector (running
        // inside the lock holder) never waits on the lock chain
        // (collector → holder → waiter → collector).
        crate::thread::mark_test_lock_wait();
        let inner = RUNTIME_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::thread::mark_test_lock_acquired();
        RuntimeGuard { inner }
    }
}

/// Test-only re-export: the shared-runtime lock helper reaches every test
/// module as `crate::runtime()` without the fully-qualified path.
#[cfg(test)]
pub(crate) use test_support::runtime;

// --- scanaislop split: the items below live verbatim in the child modules ---
#[path = "lib/checks.rs"]
mod checks;
#[cfg(test)]
#[path = "lib/default_dispatch.rs"]
mod default_dispatch;
#[path = "lib/dispatch.rs"]
mod dispatch;
#[path = "lib/invoke.rs"]
mod invoke;
#[path = "lib/plan.rs"]
mod plan;
#[cfg(test)]
#[path = "lib/tests_a.rs"]
mod tests_a;
#[path = "lib/throw.rs"]
mod throw;
#[path = "lib/throwable.rs"]
mod throwable;
#[path = "lib/value.rs"]
mod value;
pub(crate) use checks::*;
pub use checks::{checkcast, int_div, int_rem, j2r_instance_of, long_div, long_rem, ref_eq};
pub use dispatch::Dispatch;
pub use dispatch::NativeFn;
pub use invoke::invoke_virtual;
pub use plan::MethodFn;
pub use plan::UserClassInfo;
pub use plan::UserCtor;
pub use plan::UserField;
pub use plan::UserLookup;
pub use plan::UserMeta;
pub use plan::UserMethod;
pub use plan::UserParents;
pub use throw::throw_new;
pub use throw::Throw;
pub(crate) use throw::*;
pub use throwable::{throwable_display, throwable_enclosed_trace};
pub use value::Value;
