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
//! * The heap is `Rc<RefCell<..>>`: single-threaded, reference-counted.
//!   Cycle-collecting GC and true Java threads are planned milestones; a
//!   cycle leak is *memory*, never *behaviour*, so program output stays
//!   correct.
//! * Every fallible generated operation returns `Result<_, Throw>`; nothing
//!   is silently ignored.

#![forbid(unsafe_code)]

pub mod array;
pub mod builtin;
pub mod concat;
pub mod desc;
pub mod frame;
pub mod heap;
pub mod javafmt;
pub mod natives;
pub mod object;
pub mod stack;
pub mod subtype;

use std::rc::Rc;

pub use desc::{parse_field_desc, parse_method_desc, unmarshal_args, TypeDesc};
pub use heap::JRef;
pub use object::{intern_string, JObj};
pub use stack::{pop_double, pop_float, pop_int, pop_long, pop_n, pop_ref};
pub use subtype::is_subtype;

/// The uniform value representation of the compiled world.
///
/// Java primitives map as follows: `boolean/byte/short/char/int` -> `Int`,
/// `long` -> `Long`, `float` -> `Float`, `double` -> `Double`. References use
/// `Ref(None)` for Java `null`.
///
/// `Str` never appears on an operand stack or in locals; it is only stored in
/// the field slot of `java/lang/String` and `java/lang/StringBuilder` objects.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Value {
    #[default]
    Void,
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    Str(Rc<String>),
    Ref(Option<JRef>),
}

impl Value {
    /// True when the value occupies two JVM operand-stack slots
    /// (`long`/`double` category-2 values).
    pub fn cat2(&self) -> bool {
        matches!(self, Value::Long(_) | Value::Double(_))
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Ref(None))
    }

    pub fn expect_int(&self) -> i32 {
        match self {
            Value::Int(v) => *v,
            other => panic!("j2r internal: expected int value, got {other:?}"),
        }
    }

    pub fn expect_long(&self) -> i64 {
        match self {
            Value::Long(v) => *v,
            other => panic!("j2r internal: expected long value, got {other:?}"),
        }
    }

    pub fn expect_float(&self) -> f32 {
        match self {
            Value::Float(v) => *v,
            other => panic!("j2r internal: expected float value, got {other:?}"),
        }
    }

    pub fn expect_double(&self) -> f64 {
        match self {
            Value::Double(v) => *v,
            other => panic!("j2r internal: expected double value, got {other:?}"),
        }
    }

    pub fn expect_ref(&self) -> Option<JRef> {
        match self {
            Value::Ref(r) => *r,
            other => panic!("j2r internal: expected reference value, got {other:?}"),
        }
    }
}

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

/// ABI of every generated method: `recv` is `Some` for instance methods,
/// `None` for statics. Arguments and results use [`Value`].
pub type MethodFn = fn(Option<&JRef>, &[Value]) -> Result<Value, Throw>;

/// Provided by the generated crate: look up a compiled method by
/// `(class, name, descriptor)` (class names in internal form `a/b/C`).
pub type UserLookup = fn(&str, &str, &str) -> Option<MethodFn>;

/// Provided by the generated crate: parents (superclass first, then
/// interfaces) of a compiled class, or `None` when the class is not compiled.
pub type UserParents = fn(&str) -> Option<&'static [&'static str]>;

/// The generated crate's symbol tables, passed as a plain value so native
/// implementations can dispatch back into compiled code (e.g. `toString`
/// during string concatenation).
#[derive(Copy, Clone, Default)]
pub struct Dispatch {
    pub lookup: Option<UserLookup>,
    pub parents: Option<UserParents>,
}

impl Dispatch {
    pub fn new(lookup: Option<UserLookup>, parents: Option<UserParents>) -> Self {
        Dispatch { lookup, parents }
    }

    pub fn find_method(&self, class: &str, name: &str, desc: &str) -> Option<MethodFn> {
        self.lookup.and_then(|u| u(class, name, desc))
    }

    pub fn parent_of(&self, class: &str) -> Option<&'static str> {
        subtype::parent_of(class, self.parents)
    }

    pub fn is_subtype(&self, child: &str, parent: &str) -> bool {
        is_subtype(child, parent, self.parents)
    }
}

/// Signature of a native (JDK) method implementation.
pub type NativeFn = fn(Option<&JRef>, &[Value], Dispatch) -> Result<Value, Throw>;

/// Throwing helper used by natives: builds a heap exception object.
/// The message slot (`Throwable.detailMessage`) is slot 0.
pub fn throw_new(class: &str, msg: Option<&str>) -> Throw {
    let msg_val = match msg {
        Some(m) => Value::Ref(Some(object::new_string(m))),
        None => Value::Ref(None),
    };
    let obj = crate::heap::alloc(JObj {
        jclass: Rc::from(class),
        slots: vec![msg_val],
    });
    Throw { obj }
}

/// Internal-only marker error for invariant violations (a codegen or native
/// bug, never a Java program error).
pub fn internal_error(msg: &str) -> Throw {
    throw_new("java/lang/Error", Some(&format!("j2r internal: {msg}")))
}

// ------------------------------------------------------------- generated-call helpers

/// Java integer division: throws `ArithmeticException("/ by zero")` on zero
/// and wraps `MIN_VALUE / -1` exactly like the JVM.
pub fn int_div(a: i32, b: i32) -> Result<i32, Throw> {
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    Ok(a.wrapping_div(b))
}

/// Java integer remainder (sign of the dividend, as in the JVM).
pub fn int_rem(a: i32, b: i32) -> Result<i32, Throw> {
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    Ok(a.wrapping_rem(b))
}

pub fn long_div(a: i64, b: i64) -> Result<i64, Throw> {
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    Ok(a.wrapping_div(b))
}

pub fn long_rem(a: i64, b: i64) -> Result<i64, Throw> {
    if b == 0 {
        return Err(throw_new(
            "java/lang/ArithmeticException",
            Some("/ by zero"),
        ));
    }
    Ok(a.wrapping_rem(b))
}

/// Reference identity (`if_acmpeq`/`if_acmpne`).
pub fn ref_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Ref(x), Value::Ref(y)) => match (x, y) {
            (None, None) => true,
            (Some(x), Some(y)) => JRef::ptr_eq(x, y),
            _ => false,
        },
        _ => panic!("j2r internal: if_acmp on non-reference values"),
    }
}

/// `checkcast`: passes `null` through, otherwise verifies the runtime type.
pub fn checkcast(v: Value, class: &str, ctx: Dispatch) -> Result<Value, Throw> {
    match &v {
        Value::Ref(None) => Ok(v),
        Value::Ref(Some(o)) => {
            let cls = crate::heap::with_obj(*o, |obj| obj.jclass.to_string());
            if ctx.is_subtype(&cls, class) {
                Ok(v)
            } else {
                Err(throw_new(
                    "java/lang/ClassCastException",
                    Some(&format!(
                        "class {} cannot be cast to class {}",
                        cls.replace('/', "."),
                        class.replace('/', ".")
                    )),
                ))
            }
        }
        _ => Err(internal_error("checkcast on a primitive value")),
    }
}

/// `instanceof`: `null` is an instance of nothing.
pub fn j2r_instance_of(v: &Value, class: &str, ctx: Dispatch) -> bool {
    match v {
        Value::Ref(None) => false,
        Value::Ref(Some(o)) => {
            let cls = crate::heap::with_obj(*o, |obj| obj.jclass.to_string());
            ctx.is_subtype(&cls, class)
        }
        _ => false,
    }
}

/// Dynamic dispatch of an `invokevirtual`/`invokeinterface`: walks the
/// receiver's class chain from the concrete class upwards and returns the
/// first compiled or native implementation found.
pub fn invoke_virtual(
    recv: &JRef,
    _declared: &str,
    name: &str,
    desc: &str,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    // The receiver's class chain is walked by name; the receiver itself is
    // rooted by the caller's refreshed frame snapshot (generated code
    // refreshes before every call), so no NativeRoots scope is needed here.
    let mut cur: Rc<str> = crate::heap::with_obj(*recv, |o| o.jclass.clone());
    for _ in 0..64 {
        if let Some(f) = ctx.find_method(&cur, name, desc) {
            return f(Some(recv), args);
        }
        if let Some(n) = natives::lookup_exact(&cur, name, desc) {
            return n(Some(recv), args, ctx);
        }
        match ctx.parent_of(&cur) {
            Some(p) => cur = Rc::from(p),
            None => break,
        }
    }
    let cls = crate::heap::with_obj(*recv, |o| o.jclass.clone());
    Err(throw_new(
        "java/lang/Error",
        Some(&format!(
            "j2r internal: no implementation found for {name}{desc} when dispatched on {cls}"
        )),
    ))
}

/// Renders an uncaught throwable the way the JVM does:
/// `Exception in thread "main" <toString>`.
pub fn throwable_display(t: &Throw, ctx: Dispatch) -> String {
    // main_run is not a generated method, so nothing else roots the thrown
    // object while `toString` dispatches into (possibly user) code — scope
    // it explicitly (M2_PLAN §4.4, native→Java re-entry).
    let _nr = crate::frame::NativeRoots::push(&[t.obj]);
    match invoke_virtual(
        &t.obj,
        "java/lang/Object",
        "toString",
        "()Ljava/lang/String;",
        &[],
        ctx,
    ) {
        Ok(Value::Ref(Some(s))) => object::string_value(&s).to_string(),
        _ => t.java_class(),
    }
}

/// The one control-flow macro generated code needs.
///
/// Evaluates `$e` (a `Result<T, Throw>`). On `Ok(v)` yields `v`; on `Err(t)`
/// records the pending throw with the faulting instruction's pc and continues
/// the enclosing loop so the exception table can be consulted.
///
/// The caller passes its own `thrown` / `fault_pc` bindings explicitly —
/// `macro_rules!` hygiene would otherwise give the macro's local-variable
/// references a fresh, unresolvable scope. The `'run` label is passed
/// explicitly for the same hygiene reason: a label defined at the call
/// site is *not* referenceable inside the macro body.
#[macro_export]
macro_rules! j2r_try {
    ($run:lifetime, $thrown:ident, $fault_pc:ident, $fault:expr, $e:expr) => {
        match $e {
            ::core::result::Result::Ok(v) => v,
            ::core::result::Result::Err(t) => {
                $thrown = ::core::option::Option::Some(t);
                $fault_pc = $fault;
                // Labeled: the lifted body (Wave-4 `--emit-cfg`) wraps arms
                // in labeled *blocks*, and an unlabeled `continue` that
                // diverges through a labeled block is E0695.
                continue $run;
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cat2_flags() {
        assert!(Value::Long(0).cat2());
        assert!(Value::Double(0.0).cat2());
        assert!(!Value::Int(0).cat2());
        assert!(!Value::Ref(None).cat2());
    }

    #[test]
    fn throw_new_sets_message_slot() {
        let t = throw_new("java/lang/RuntimeException", Some("boom"));
        assert_eq!(t.java_class(), "java/lang/RuntimeException");
        let msg = crate::heap::with_obj(t.obj, |o| o.slots[0].clone());
        match msg {
            Value::Ref(Some(s)) => assert_eq!(&*object::string_value(&s), "boom"),
            other => panic!("unexpected {other:?}"),
        }
    }
}
