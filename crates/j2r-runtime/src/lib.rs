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
    Str(Arc<String>),
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

/// One declared instance/static field of a compiled class, exported for the
/// reflection subset (M3_PLAN §8): `Class.getDeclaredField` + `Field.get/set`.
#[derive(Debug)]
pub struct UserField {
    pub name: &'static str,
    /// Field descriptor (`I`, `J`, `Ljava/lang/String;`, …).
    pub desc: &'static str,
    pub is_static: bool,
    /// Absolute instance slot in the flattened layout (`None`-free form:
    /// meaningful only when `!is_static`).
    pub instance_slot: usize,
    /// Static-pool slot (meaningful only when `is_static`).
    pub static_slot: u32,
    /// The raw ACC_* access flags (R2 slice 2: `Field.getModifiers`).
    pub modifiers: u16,
}

/// Compile-time facts of a compiled class that the reflection subset needs
/// at runtime (M3_PLAN §8.1). Generated per class into the crate registry.
#[derive(Debug)]
pub struct UserClassInfo {
    /// The class's compiled id — the `clinit::ensure` state-table key
    /// (FORNAME-INIT: `Class.forName` must run the `<clinit>` through
    /// the same JLS 12.4.2 machine the bytecode sites use).
    pub id: u32,
    /// `ACC_INTERFACE` (drives `isInterface` / `getSuperclass == null`).
    pub is_interface: bool,
    /// `ACC_ABSTRACT` (NEW-INSTANCE: `Constructor.newInstance` throws the
    /// message-null `InstantiationException`, Temurin probe P6).
    pub is_abstract: bool,
    /// `InnerClasses` `inner_name` (`None` = top-level / absent attribute,
    /// `Some("")` = anonymous). `getSimpleName` uses it verbatim.
    pub simple_name: Option<&'static str>,
    /// `NestHost` attribute (JVMS §4.7.28): the declared host, `None`
    /// when absent (a top-level class IS its own nest host). The
    /// different-package private-ctor IllegalAccessException law
    /// (JEP 181 areNestMates — the 2026-11-16 probe S1/S2/S6) resolves
    /// both sides' hosts and compares.
    pub nest_host: Option<&'static str>,
    /// Total instance slots including inherited ones (NEW-INSTANCE: the
    /// `new_obj` arity).
    pub total_slots: usize,
    /// The JLS §4.12.5 default-slot spec over the tail of the layout
    /// (NEW-INSTANCE: the ctor sees x=0 before assignment — probe P1;
    /// `object::set_defaults` applies it from the tail, the builtin base
    /// region stays Void exactly like the emitted `new` sites).
    pub defaults_spec: &'static str,
    pub fields: &'static [UserField],
    /// Declared methods — the `getDeclaredMethod` search space. `is_static`
    /// rides along because `Method.invoke` must know whether to null-check
    /// the receiver (Temurin ignores it for statics).
    pub methods: &'static [UserMethod],
    /// Declared constructors in CLASSFILE order (R2 slice 3:
    /// `getDeclaredConstructors` — the runtime applies Temurin's
    /// reversal).
    pub ctors: &'static [UserCtor],
    /// The class's `<clinit>` (FORNAME-INIT), `None` when the class has
    /// none — `Class.forName(name)` equals `forName(name, true, loader)`
    /// and runs it through `clinit::ensure` exactly once.
    pub clinit: Option<MethodFn>,
}

/// One declared CONSTRUCTOR of a compiled class (R2 slice 3:
/// `getDeclaredConstructors`). The `<init>` rows are filtered out of
/// `methods` — ctors are not `Method.invoke`-able in the JDK either, but
/// `Class.getDeclaredConstructors` needs them, so they ride a dedicated
/// table in CLASSFILE order (the Temurin REVERSAL is applied at runtime —
/// ReflectProbe capture: Temurin lists the protected 3-arg ctor of a
/// four-ctor class first, javac's classfile lists it last).
#[derive(Debug)]
pub struct UserCtor {
    /// Full descriptor `(ILjava/lang/String;)V`.
    pub desc: &'static str,
    /// The raw ACC_* access flags.
    pub modifiers: u16,
}

/// One declared method of a compiled class.
#[derive(Debug)]
pub struct UserMethod {
    pub name: &'static str,
    pub desc: &'static str,
    pub is_static: bool,
    /// The raw ACC_* access flags (R2 slice 2: `Method.getModifiers`).
    pub modifiers: u16,
    /// `ACC_NATIVE` (T12, M5_JNI_NATIVES.md §5): the method has no
    /// bytecode body — its implementation arrives via `RegisterNatives`.
    /// The metadata rows carry the flag so the JNI declared-native check
    /// and `getModifiers` read the same facts; the generated lookup
    /// table omits native methods (no body exists) and the call reaches
    /// the binding registry through the emitted call-through body.
    pub is_native: bool,
}

/// Provided by the generated crate: compile-time class facts, or `None`
/// when the class is not compiled (builtin classes answer from the runtime
/// builtin tables instead).
pub type UserMeta = fn(&str) -> Option<&'static UserClassInfo>;

/// The generated crate's symbol tables, passed as a plain value so native
/// implementations can dispatch back into compiled code (e.g. `toString`
/// during string concatenation).
#[derive(Copy, Clone, Default)]
pub struct Dispatch {
    pub lookup: Option<UserLookup>,
    pub parents: Option<UserParents>,
    /// Reflection-subset class facts (M3 Wave 5).
    pub meta: Option<UserMeta>,
    /// Static-field pool access for `Field.get/set` on static fields:
    /// read/write pool slot by absolute index.
    pub static_get: Option<fn(u32) -> Value>,
    pub static_set: Option<fn(u32, &Value)>,
}

impl Dispatch {
    pub fn new(lookup: Option<UserLookup>, parents: Option<UserParents>) -> Self {
        Dispatch {
            lookup,
            parents,
            meta: None,
            static_get: None,
            static_set: None,
        }
    }

    pub fn find_method(&self, class: &str, name: &str, desc: &str) -> Option<MethodFn> {
        self.lookup.and_then(|u| u(class, name, desc))
    }

    pub fn parent_of(&self, class: &str) -> Option<&'static str> {
        subtype::parent_of(class, self.parents)
    }

    /// The FULL parents array of a compiled class (superclass first, then
    /// interfaces) — the interface-default resolution walks the interface
    /// edges, which `parent_of` (parents[0] only) never visits. `None`
    /// when the class is not compiled (builtin callers use
    /// `subtype::builtin_parent_of`).
    pub fn parents_of_full(&self, class: &str) -> Option<&'static [&'static str]> {
        let u = self.parents?;
        u(class)
    }

    pub fn is_subtype(&self, child: &str, parent: &str) -> bool {
        is_subtype(child, parent, self.parents)
    }

    /// Compiled-class facts, `None` for builtin/unknown classes.
    pub fn class_info(&self, class: &str) -> Option<&'static UserClassInfo> {
        self.meta.and_then(|m| m(class))
    }

    /// Is `class` a compiled interface? Falls back to the builtin table.
    pub fn is_interface(&self, class: &str) -> bool {
        if let Some(info) = self.class_info(class) {
            return info.is_interface;
        }
        crate::builtin::builtin_is_interface(class)
    }
}

/// Signature of a native (JDK) method implementation.
pub type NativeFn = fn(Option<&JRef>, &[Value], Dispatch) -> Result<Value, Throw>;

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

/// The ClassCastException the faithful `checkcast` throws: the exact
/// message builder, shared with the specialized-map get's fused unbox
/// (G3.3 slice 3) so a foreign box or a mismatched prim tag in a node
/// slot throws the same bytes the faithful `checkcast` would.
pub(crate) fn cce_throw(actual: &str, expect: &str) -> Throw {
    throw_new(
        "java/lang/ClassCastException",
        Some(&format!(
            "class {} cannot be cast to class {}",
            actual.replace('/', "."),
            expect.replace('/', ".")
        )),
    )
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
                Err(cce_throw(&cls, class))
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
    if let Some(res) = dispatch_chain(recv, name, desc, args, ctx) {
        return res;
    }
    // The interface-default resolution (the registered Probe5 residual,
    // standing run 3, 2026-11-16): the chain above walks parents[0] only,
    // so a user interface's DEFAULT method is off it — Temurin runs the
    // default body, j2r died the loud internal Error. JVMS 5.4.4.4/5.4.6:
    // when no concrete class implementation exists, the MAXIMALLY-SPECIFIC
    // default among the receiver's transitive interface set runs. Law
    // details pinned by the probe (scripts/evidence/default_dispatch/):
    // - the closure is BFS over the FULL parents arrays (class parents =
    //   [super, interfaces...]; interface parents = [Object, super-ifaces],
    //   superclass interfaces included by expansion);
    // - a candidate is a lookup hit on an INTERFACE-typed node: abstract
    //   interface methods have no lookup row (codegen skips them at plan
    //   time), so a user-lookup hit IS a concrete default body; builtin
    //   interfaces answer from the native rows;
    // - dominance: candidate I loses to another candidate J when J is a
    //   strict subtype of I (is_ref_subtype walks the same parents edges);
    // - the diamond conflict (two unrelated survivors) is UNREACHABLE from
    //   javac input — javac refuses it at compile time ("inherits
    //   unrelated defaults"), so it can only arrive in a hand-crafted
    //   classfile; the loud IncompatibleClassChangeError is the JVM's
    //   resolution law, never a silent pick;
    // - the same closed-world argument bounds the static-via-instance and
    //   re-abstract shapes: javac refuses both, concrete classes always
    //   implement re-abstracted members, so no extra filter is taken here
    //   and the loud Error remains the honest fallthrough.
    let survivors = default_survivors(recv, name, desc, ctx);
    if survivors.len() == 1 {
        let (iface, is_user) = &survivors[0];
        if *is_user {
            if let Some(f) = ctx.find_method(iface, name, desc) {
                return f(Some(recv), args);
            }
        } else if let Some(n) = natives::lookup_exact(iface, name, desc) {
            return n(Some(recv), args, ctx);
        }
    }
    if survivors.len() > 1 {
        let names: Vec<String> = survivors.iter().map(|(i, _)| i.to_string()).collect();
        return Err(throw_new(
            "java/lang/IncompatibleClassChangeError",
            Some(&format!(
                "unrelated default methods found for {name}{desc} in {}",
                names.join(", ")
            )),
        ));
    }
    let cls = crate::heap::with_obj(*recv, |o| o.jclass.clone());
    Err(throw_new(
        "java/lang/Error",
        Some(&format!(
            "j2r internal: no implementation found for {name}{desc} when dispatched on {cls}"
        )),
    ))
}

/// The concrete-class chain walk (verbatim from `invoke_virtual`,
/// round-6 reshaping): the array-receiver specials, then up to 64
/// parent hops over compiled fns and native rows. `Some(result)` when
/// a target dispatched, `None` to fall through to the default walk.
fn dispatch_chain(
    recv: &JRef,
    name: &str,
    desc: &str,
    args: &[Value],
    ctx: Dispatch,
) -> Option<Result<Value, Throw>> {
    let mut cur: Arc<str> = crate::heap::with_obj(*recv, |o| o.jclass.clone());
    // Array receivers (corpus wave 2, JZlib): every array type is a
    // subtype of Object with a public `clone()` (JVMS 2.15/6.5 invokevirtual)
    // plus the Object methods. `clone` shallow-copies element-wise here;
    // the Object methods dispatch through the Object natives rows.
    if cur.starts_with('[') {
        if name == "clone" && desc == "()Ljava/lang/Object;" {
            return Some(match crate::array::array_shallow_copy(recv, ctx) {
                Ok(v) => Ok(Value::Ref(Some(v))),
                Err(e) => return Some(Err(e)),
            });
        }
        if matches!(
            (name, desc),
            ("getClass", "()Ljava/lang/Class;")
                | ("hashCode", "()I")
                | ("equals", "(Ljava/lang/Object;)Z")
                | ("toString", "()Ljava/lang/String;")
        ) {
            return Some(natives::invoke_exact(
                "java/lang/Object",
                name,
                desc,
                Some(recv),
                args,
                ctx,
            ));
        }
    }
    for _ in 0..64 {
        if let Some(f) = ctx.find_method(&cur, name, desc) {
            return Some(f(Some(recv), args));
        }
        if let Some(n) = natives::lookup_exact(&cur, name, desc) {
            return Some(n(Some(recv), args, ctx));
        }
        match ctx.parent_of(&cur) {
            Some(p) => cur = Arc::from(p),
            None => break,
        }
    }
    None
}

/// The maximally-specific default-method scan (verbatim continuation):
/// BFS over the FULL parents arrays, interface-typed lookup hits as
/// candidates, dominance by strict subtype.
fn default_survivors(recv: &JRef, name: &str, desc: &str, ctx: Dispatch) -> Vec<(Arc<str>, bool)> {
    let start = crate::heap::with_obj(*recv, |o| o.jclass.clone());
    let mut queue: std::collections::VecDeque<Arc<str>> = std::collections::VecDeque::new();
    let mut seen: std::collections::HashSet<Arc<str>> = std::collections::HashSet::new();
    if let Some(ps) = ctx.parents_of_full(&start) {
        for p in ps {
            queue.push_back(Arc::from(*p));
        }
    }
    // (iface class, user-lookup hit) — natives ride as (class, false).
    let mut candidates: Vec<(Arc<str>, bool)> = Vec::new();
    while let Some(node) = queue.pop_front() {
        if seen.len() >= 1024 || !seen.insert(node.clone()) {
            continue;
        }
        if ctx.is_interface(&node) {
            let user_hit = ctx.find_method(&node, name, desc);
            let native_hit = natives::lookup_exact(&node, name, desc);
            if user_hit.is_some() || native_hit.is_some() {
                candidates.push((node.clone(), user_hit.is_some()));
            }
        }
        if let Some(ps) = ctx.parents_of_full(&node) {
            for p in ps {
                queue.push_back(Arc::from(*p));
            }
        } else if let Some(bp) = crate::builtin::builtin_parent_of(&node) {
            queue.push_back(Arc::from(bp));
        }
    }
    // Maximally specific: drop every candidate dominated by a strict
    // subtype candidate (the concrete-chain override already won above —
    // this loop only runs when it found nothing).
    let mut survivors: Vec<(Arc<str>, bool)> = Vec::new();
    for (iface, is_user) in &candidates {
        let dominated = candidates
            .iter()
            .any(|(other, _)| other != iface && ctx.is_subtype(other, iface));
        if !dominated {
            survivors.push((iface.clone(), *is_user));
        }
    }
    survivors
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

/// Renders the enclosed trace sections of a thrown throwable — the
/// `Suppressed: ...` sections and the `Caused by: ...` chain — in exactly
/// the JVM's `printStackTrace` order (audit A2): every suppressed section
/// of an exception prints before that exception's cause chain; suppressed
/// sections indent one tab deeper per nesting level while cause captions
/// stay at the enclosing prefix; section titles go through real virtual
/// `toString` dispatch so user overrides win, like the JVM.
///
/// Frame lines are not rendered (no `LineNumberTable` support yet); the
/// differential harness normalizes JVM frames away at any tab depth, so
/// the compared bytes are the caption lines only.
pub fn throwable_enclosed_trace(t: &Throw, ctx: Dispatch) -> String {
    let mut out = String::new();
    enclosed_sections(&t.obj, "", ctx, &mut out);
    out
}

/// `toString` for one trace section through real virtual dispatch; falls
/// back to the dotted class name when dispatch fails (same fallback rule
/// as [`throwable_display`]).
fn enclosed_title(obj: &JRef, ctx: Dispatch) -> String {
    let _nr = crate::frame::NativeRoots::push(std::slice::from_ref(obj));
    match invoke_virtual(
        obj,
        "java/lang/Object",
        "toString",
        "()Ljava/lang/String;",
        &[],
        ctx,
    ) {
        Ok(Value::Ref(Some(s))) => object::string_value(&s).to_string(),
        _ => crate::heap::with_obj(*obj, |o| o.jclass.replace('/', ".")),
    }
}

fn throwable_slot(obj: &JRef, slot: usize) -> Option<JRef> {
    crate::heap::with_obj(*obj, |o| match &o.slots[slot] {
        Value::Ref(r) => *r,
        _ => None,
    })
}

fn enclosed_sections(obj: &JRef, prefix: &str, ctx: Dispatch, out: &mut String) {
    // Root everything held across the `toString` dispatches below: the
    // dispatch re-enters generated code, which can reach a safepoint and
    // collect (M2_PLAN §4.4 native→Java rule).
    let sup = throwable_slot(obj, 2);
    let cause = throwable_slot(obj, 1);
    let mut roots: Vec<JRef> = vec![*obj];
    if let Some(a) = &sup {
        roots.push(*a);
    }
    if let Some(c) = &cause {
        roots.push(*c);
    }
    let _nr = crate::frame::NativeRoots::push(&roots);
    if let Some(sup) = sup {
        if let Ok(len) = crate::array::array_length(&sup) {
            for i in 0..len {
                let Ok(v) = crate::array::array_load(&sup, i) else {
                    continue;
                };
                if let Value::Ref(Some(se)) = v {
                    let _gr = crate::frame::NativeRoots::push(&[se]);
                    let title = enclosed_title(&se, ctx);
                    out.push_str(prefix);
                    out.push_str("\tSuppressed: ");
                    out.push_str(&title);
                    out.push('\n');
                    enclosed_sections(&se, &format!("{prefix}\t"), ctx, out);
                }
            }
        }
    }
    if let Some(cause) = cause {
        let title = enclosed_title(&cause, ctx);
        out.push_str(prefix);
        out.push_str("Caused by: ");
        out.push_str(&title);
        out.push('\n');
        enclosed_sections(&cause, prefix, ctx, out);
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
        let _rt = crate::runtime();
        assert!(Value::Long(0).cat2());
        assert!(Value::Double(0.0).cat2());
        assert!(!Value::Int(0).cat2());
        assert!(!Value::Ref(None).cat2());
    }

    #[test]
    fn throw_new_sets_message_slot() {
        let _rt = crate::runtime();
        let t = throw_new("java/lang/RuntimeException", Some("boom"));
        assert_eq!(t.java_class(), "java/lang/RuntimeException");
        let msg = crate::heap::with_obj(t.obj, |o| o.slots[0].clone());
        match msg {
            Value::Ref(Some(s)) => assert_eq!(&*object::string_value(&s), "boom"),
            other => panic!("unexpected {other:?}"),
        }
    }
}

/// M4 Wave 1 (T02–T05) — proof that the shared-substrate migration is
/// complete and stays complete.
#[cfg(test)]
mod m4_wave1 {
    /// R3 mitigation (M4_PLAN §2): every `thread_local!` remaining in
    /// j2r-runtime must carry an `M4-SEAM` justification. Wave 1 migrated
    /// the heap and all identity caches; frame.rs is the one documented
    /// per-thread-by-design seam. A future unannotated seam fails CI here.
    #[test]
    fn thread_local_seams_are_annotated() {
        let _rt = crate::runtime();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut seams = Vec::new();
        for entry in std::fs::read_dir(&src).expect("runtime src dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read src file");
            // Macro invocations only (line starts with `thread_local!`),
            // never prose mentions of the pattern.
            let invoked = text.lines().any(|l| l.starts_with("thread_local!"));
            if invoked {
                assert!(
                    text.contains("M4-SEAM"),
                    "unannotated thread_local! in {} — every remaining seam \
                     must carry an M4-SEAM justification (M4_PLAN §2 R3)",
                    path.display()
                );
                seams.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
        seams.sort();
        assert_eq!(
            seams,
            vec!["frame.rs".to_string(), "thread.rs".to_string()],
            "Wave 2 seams: frame.rs depth/limit scalars + thread.rs CURRENT \
             handle (both owner-only; all shared state lives in the registry)"
        );
    }

    /// T05: the shared containers are compile-proven Send+Sync — the
    /// property Wave 2's real threads depend on. `Value`/`JObj` going
    /// Send+Sync is what the Rc→Arc groundwork (W1a) bought.
    #[test]
    fn shared_containers_are_send_sync() {
        let _rt = crate::runtime();
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<crate::Value>();
        assert_send_sync::<crate::object::JObj>();
        assert_send_sync::<crate::heap::JRef>();
        // The Mutex<Heap> container assertion lives in heap.rs's own test
        // module, where the private Heap type is in scope.
    }
}

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

/// The interface-default dispatch laws (the registered Probe5 residual,
/// standing run 3, 2026-11-16): the maximally-specific default among the
/// receiver's transitive interface set runs when the concrete chain has
/// no implementation. Synthetic tables mirror the compiled world: the
/// parents arrays are [super, interfaces...], the user lookup carries
/// ONLY concrete bodies (abstract rows are skipped at plan time), and
/// the meta table carries the ACC_INTERFACE flag (the BFS's interface
/// detection reads it).
#[cfg(test)]
mod default_dispatch {
    use super::*;

    fn body_a(_recv: Option<&JRef>, _args: &[Value]) -> Result<Value, Throw> {
        Ok(Value::Int(1))
    }
    fn body_b(_recv: Option<&JRef>, _args: &[Value]) -> Result<Value, Throw> {
        Ok(Value::Int(2))
    }
    fn body_top(_recv: Option<&JRef>, _args: &[Value]) -> Result<Value, Throw> {
        Ok(Value::Int(10))
    }
    fn body_mid(_recv: Option<&JRef>, _args: &[Value]) -> Result<Value, Throw> {
        Ok(Value::Int(20))
    }
    fn body_class(_recv: Option<&JRef>, _args: &[Value]) -> Result<Value, Throw> {
        Ok(Value::Int(7))
    }

    // The Probe5 shape: Impl implements WithDefault; the default twice()
    // body carries sentinel 42.
    fn p5_parents(class: &str) -> Option<&'static [&'static str]> {
        match class {
            "P5$Impl" => Some(&["java/lang/Object", "P5$WithDefault"]),
            "P5$WithDefault" => Some(&["java/lang/Object"]),
            _ => None,
        }
    }
    fn p5_lookup(class: &str, name: &str, desc: &str) -> Option<MethodFn> {
        match (class, name, desc) {
            ("P5$Impl", "m", "()I") => Some(body_class),
            ("P5$WithDefault", "twice", "()I") => Some(body_a),
            _ => None,
        }
    }
    const P5_IFACE: UserClassInfo = UserClassInfo {
        id: 1,
        is_interface: true,
        is_abstract: false,
        simple_name: Some("WithDefault"),
        nest_host: None,
        total_slots: 0,
        defaults_spec: "",
        fields: &[],
        methods: &[
            UserMethod {
                name: "m",
                desc: "()I",
                is_static: false,
                modifiers: 0x0401,
                is_native: false,
            },
            UserMethod {
                name: "twice",
                desc: "()I",
                is_static: false,
                modifiers: 0x0001,
                is_native: false,
            },
        ],
        ctors: &[],
        clinit: None,
    };
    const P5_IMPL: UserClassInfo = UserClassInfo {
        id: 2,
        is_interface: false,
        is_abstract: false,
        simple_name: Some("Impl"),
        nest_host: None,
        total_slots: 0,
        defaults_spec: "",
        fields: &[],
        methods: &[UserMethod {
            name: "m",
            desc: "()I",
            is_static: false,
            modifiers: 0x0001,
            is_native: false,
        }],
        ctors: &[],
        clinit: None,
    };
    fn p5_meta(class: &str) -> Option<&'static UserClassInfo> {
        match class {
            "P5$WithDefault" => Some(&P5_IFACE),
            "P5$Impl" => Some(&P5_IMPL),
            _ => None,
        }
    }
    fn p5_dispatch() -> Dispatch {
        let mut d = Dispatch::new(Some(p5_lookup), Some(p5_parents));
        d.meta = Some(p5_meta);
        d
    }
    fn recv_of(class: &str) -> JRef {
        crate::heap::alloc(JObj {
            jclass: Arc::from(class),
            slots: Vec::new(),
            prim: None,
        })
    }

    #[test]
    fn single_default_dispatches() {
        let _rt = crate::runtime();
        let recv = recv_of("P5$Impl");
        let ctx = p5_dispatch();
        let v = invoke_virtual(&recv, "P5$WithDefault", "twice", "()I", &[], ctx)
            .expect("the interface default must run");
        assert_eq!(
            v.expect_int(),
            1,
            "the WithDefault default body (sentinel 1) runs"
        );
        // The concrete sibling still wins through the ordinary chain law.
        let v = invoke_virtual(&recv, "P5$WithDefault", "m", "()I", &[], ctx)
            .expect("the class method must run");
        assert_eq!(v.expect_int(), 7);
    }

    #[test]
    fn most_specific_default_wins() {
        let _rt = crate::runtime();
        // Mid extends Top; BOTH declare the default; Impl implements Mid;
        // the site is declared on Top (the L2 probe shape). Mid is the
        // strict subtype candidate and dominates Top.
        fn parents(class: &str) -> Option<&'static [&'static str]> {
            match class {
                "MS$Impl" => Some(&["java/lang/Object", "MS$Mid"]),
                "MS$Mid" => Some(&["java/lang/Object", "MS$Top"]),
                "MS$Top" => Some(&["java/lang/Object"]),
                _ => None,
            }
        }
        fn lookup(class: &str, name: &str, desc: &str) -> Option<MethodFn> {
            match (class, name, desc) {
                ("MS$Top", "t", "()I") => Some(body_top),
                ("MS$Mid", "t", "()I") => Some(body_mid),
                _ => None,
            }
        }
        const TOP: UserClassInfo = UserClassInfo {
            id: 3,
            is_interface: true,
            is_abstract: false,
            simple_name: Some("Top"),
            nest_host: None,
            total_slots: 0,
            defaults_spec: "",
            fields: &[],
            methods: &[UserMethod {
                name: "t",
                desc: "()I",
                is_static: false,
                modifiers: 0x0001,
                is_native: false,
            }],
            ctors: &[],
            clinit: None,
        };
        const MID: UserClassInfo = UserClassInfo {
            id: 4,
            is_interface: true,
            is_abstract: false,
            simple_name: Some("Mid"),
            nest_host: None,
            total_slots: 0,
            defaults_spec: "",
            fields: &[],
            methods: &[UserMethod {
                name: "t",
                desc: "()I",
                is_static: false,
                modifiers: 0x0001,
                is_native: false,
            }],
            ctors: &[],
            clinit: None,
        };
        const IMPL: UserClassInfo = UserClassInfo {
            id: 5,
            is_interface: false,
            is_abstract: false,
            simple_name: Some("Impl"),
            nest_host: None,
            total_slots: 0,
            defaults_spec: "",
            fields: &[],
            methods: &[],
            ctors: &[],
            clinit: None,
        };
        fn meta(class: &str) -> Option<&'static UserClassInfo> {
            match class {
                "MS$Top" => Some(&TOP),
                "MS$Mid" => Some(&MID),
                "MS$Impl" => Some(&IMPL),
                _ => None,
            }
        }
        let mut ctx = Dispatch::new(Some(lookup), Some(parents));
        ctx.meta = Some(meta);
        let recv = recv_of("MS$Impl");
        let v = invoke_virtual(&recv, "MS$Top", "t", "()I", &[], ctx)
            .expect("the most-specific default must run");
        assert_eq!(
            v.expect_int(),
            20,
            "Mid's body (sentinel 20) dominates Top's"
        );
    }

    #[test]
    fn superinterface_default_reached_through_non_redeclaring_sub() {
        let _rt = crate::runtime();
        // Mid2 extends Top2 but does NOT redeclare; Impl implements Mid2;
        // the site is declared on Mid2 (the L4 probe shape).
        fn parents(class: &str) -> Option<&'static [&'static str]> {
            match class {
                "SI$Impl" => Some(&["java/lang/Object", "SI$Mid2"]),
                "SI$Mid2" => Some(&["java/lang/Object", "SI$Top2"]),
                "SI$Top2" => Some(&["java/lang/Object"]),
                _ => None,
            }
        }
        fn lookup(class: &str, name: &str, desc: &str) -> Option<MethodFn> {
            match (class, name, desc) {
                ("SI$Top2", "t2", "()I") => Some(body_top),
                _ => None,
            }
        }
        const TOP2: UserClassInfo = UserClassInfo {
            id: 6,
            is_interface: true,
            is_abstract: false,
            simple_name: Some("Top2"),
            nest_host: None,
            total_slots: 0,
            defaults_spec: "",
            fields: &[],
            methods: &[UserMethod {
                name: "t2",
                desc: "()I",
                is_static: false,
                modifiers: 0x0001,
                is_native: false,
            }],
            ctors: &[],
            clinit: None,
        };
        const MID2: UserClassInfo = UserClassInfo {
            id: 7,
            is_interface: true,
            is_abstract: true,
            simple_name: Some("Mid2"),
            nest_host: None,
            total_slots: 0,
            defaults_spec: "",
            fields: &[],
            methods: &[UserMethod {
                name: "t2",
                desc: "()I",
                is_static: false,
                modifiers: 0x0401,
                is_native: false,
            }],
            ctors: &[],
            clinit: None,
        };
        const IMPL2: UserClassInfo = UserClassInfo {
            id: 8,
            is_interface: false,
            is_abstract: false,
            simple_name: Some("Impl"),
            nest_host: None,
            total_slots: 0,
            defaults_spec: "",
            fields: &[],
            methods: &[],
            ctors: &[],
            clinit: None,
        };
        fn meta(class: &str) -> Option<&'static UserClassInfo> {
            match class {
                "SI$Top2" => Some(&TOP2),
                "SI$Mid2" => Some(&MID2),
                "SI$Impl" => Some(&IMPL2),
                _ => None,
            }
        }
        let mut ctx = Dispatch::new(Some(lookup), Some(parents));
        ctx.meta = Some(meta);
        let recv = recv_of("SI$Impl");
        let v = invoke_virtual(&recv, "SI$Mid2", "t2", "()I", &[], ctx)
            .expect("the super-interface default must run through the closure");
        assert_eq!(v.expect_int(), 10, "Top2's body (sentinel 10) runs");
    }

    #[test]
    fn diamond_conflict_is_loud_icerror() {
        let _rt = crate::runtime();
        // Two UNRELATED defaults — javac refuses this at compile time, so
        // the shape only arrives in a hand-crafted classfile; the law is
        // the loud IncompatibleClassChangeError, never a silent pick.
        fn parents(class: &str) -> Option<&'static [&'static str]> {
            match class {
                "DD$Impl" => Some(&["java/lang/Object", "DD$DA", "DD$DB"]),
                "DD$DA" => Some(&["java/lang/Object"]),
                "DD$DB" => Some(&["java/lang/Object"]),
                _ => None,
            }
        }
        fn lookup(class: &str, name: &str, desc: &str) -> Option<MethodFn> {
            match (class, name, desc) {
                ("DD$DA", "d", "()I") => Some(body_a),
                ("DD$DB", "d", "()I") => Some(body_b),
                _ => None,
            }
        }
        const DA: UserClassInfo = UserClassInfo {
            id: 9,
            is_interface: true,
            is_abstract: false,
            simple_name: Some("DA"),
            nest_host: None,
            total_slots: 0,
            defaults_spec: "",
            fields: &[],
            methods: &[UserMethod {
                name: "d",
                desc: "()I",
                is_static: false,
                modifiers: 0x0001,
                is_native: false,
            }],
            ctors: &[],
            clinit: None,
        };
        const DB: UserClassInfo = UserClassInfo {
            id: 10,
            is_interface: true,
            is_abstract: false,
            simple_name: Some("DB"),
            nest_host: None,
            total_slots: 0,
            defaults_spec: "",
            fields: &[],
            methods: &[UserMethod {
                name: "d",
                desc: "()I",
                is_static: false,
                modifiers: 0x0001,
                is_native: false,
            }],
            ctors: &[],
            clinit: None,
        };
        const IMPL3: UserClassInfo = UserClassInfo {
            id: 11,
            is_interface: false,
            is_abstract: false,
            simple_name: Some("Impl"),
            nest_host: None,
            total_slots: 0,
            defaults_spec: "",
            fields: &[],
            methods: &[],
            ctors: &[],
            clinit: None,
        };
        fn meta(class: &str) -> Option<&'static UserClassInfo> {
            match class {
                "DD$DA" => Some(&DA),
                "DD$DB" => Some(&DB),
                "DD$Impl" => Some(&IMPL3),
                _ => None,
            }
        }
        let mut ctx = Dispatch::new(Some(lookup), Some(parents));
        ctx.meta = Some(meta);
        let recv = recv_of("DD$Impl");
        let err = invoke_virtual(&recv, "DD$DA", "d", "()I", &[], ctx)
            .expect_err("the diamond conflict must stay loud");
        assert_eq!(err.java_class(), "java/lang/IncompatibleClassChangeError");
        let msg = crate::heap::with_obj(err.obj, |o| o.slots[0].clone());
        match msg {
            Value::Ref(Some(s)) => {
                let m = object::string_value(&s).to_string();
                assert!(
                    m.contains("DD$DA") && m.contains("DD$DB"),
                    "the message names both conflicting interfaces: {m}"
                );
            }
            other => panic!("unexpected message {other:?}"),
        }
    }

    #[test]
    fn no_law_change_when_nothing_matches() {
        let _rt = crate::runtime();
        // No interface declares the method: the loud internal Error stays
        // the fallthrough (never a silent null).
        let ctx = p5_dispatch();
        let recv = recv_of("P5$Impl");
        let err = invoke_virtual(&recv, "P5$WithDefault", "absent", "()I", &[], ctx)
            .expect_err("an absent method must stay loud");
        assert_eq!(err.java_class(), "java/lang/Error");
    }
}
