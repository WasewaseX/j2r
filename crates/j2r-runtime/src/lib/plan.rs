use super::*;

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
