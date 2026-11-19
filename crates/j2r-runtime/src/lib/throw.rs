use super::*;

/// A thrown Java exception/`Throwable`. The thrown object itself lives on the
/// heap so user code can catch it, inspect it and rethrow it.
#[derive(Clone, Debug)]
pub struct Throw {
    pub obj: JRef,
}
impl Throw {
    /// Runtime class name of the thrown object, in internal form (`a/b/C`).
    pub fn java_class(&self) -> String {
        crate::heap::with_obj(self.obj, |o| o.jclass.to_string())
    }
}
/// Throwing helper used by natives: builds a heap exception object.
/// Full `Throwable` layout (audit A2): slot 0 `detailMessage`, slot 1
/// `cause` (null), slot 2 the suppressed list (null until first
/// `addSuppressed`).
pub fn throw_new(class: &str, msg: Option<&str>) -> Throw {
    let msg_val = match msg {
        Some(m) => Value::Ref(Some(object::new_string(m))),
        None => Value::Ref(None),
    };
    let obj = crate::heap::alloc(JObj {
        jclass: Arc::from(class),
        slots: vec![msg_val, Value::Ref(None), Value::Ref(None)],
        prim: None,
    });
    Throw { obj }
}
/// The §5 no-message NPE law: `java/lang/NullPointerException` with no
/// detail message — the exact shape the Helpful-NPE work pins. One
/// constructor so a message change can never drift across the native
/// dereference sites.
pub(crate) fn npe() -> Throw {
    throw_new("java/lang/NullPointerException", None)
}
/// The CME law: `java/util/ConcurrentModificationException` with no
/// detail message, thrown at every comod-detection site.
pub(crate) fn cme() -> Throw {
    throw_new("java/util/ConcurrentModificationException", None)
}
/// The AIOOBE class path: the bounds-check law's exception class, thrown
/// by the array primitives and asserted by the law tests.
pub(crate) const AIOOBE_CLASS: &str = "java/lang/ArrayIndexOutOfBoundsException";

/// The §5 first-argument law: the first argument's JRef, the no-message
/// NPE on a null reference, or the caller's arity fallback. `$fallback`
/// is the caller's early-return value for a missing argument
/// (`Err(missing(..))`, `Ok(Value::Void)`).
#[macro_export]
macro_rules! first_arg_ref {
    ($args:expr, $fallback:expr) => {
        match $args.first() {
            Some(Value::Ref(Some(s))) => *s,
            Some(Value::Ref(None)) => return Err($crate::npe()),
            _ => return $fallback,
        }
    };
}
/// Internal-only marker error for invariant violations (a codegen or native
/// bug, never a Java program error).
pub fn internal_error(msg: &str) -> Throw {
    throw_new("java/lang/Error", Some(&format!("j2r internal: {msg}")))
}

// ------------------------------------------------------------- generated-call helpers
