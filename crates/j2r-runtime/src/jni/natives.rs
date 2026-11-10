//! T12 — the native-method binding registry (design of record:
//! `docs/M5_JNI_NATIVES.md`). `RegisterNatives` (slot 215) stores
//! implementations for a class's `native` methods; `UnregisterNatives`
//! (216) clears them. The call side consults the registry in two
//! places: the `Call*MethodA/V` dispatch walk (every class it visits,
//! before the loud miss pend) and the emitted call-through body for
//! `ACC_NATIVE` methods ([`call_native`]).
//!
//! Binding shapes (M5_JNI_NATIVES.md §2): a **Rust** binding is the
//! runtime's own `MethodFn` shape — the v1 round-trip ("bound from
//! Rust, called from Java") needs no new FFI primitive. A **foreign**
//! `fnPtr` (the T13 C-harness shape) is stored faithfully as a raw
//! address but CANNOT be called from a `forbid(unsafe_code)` runtime —
//! the C native ABI is the bare/variadic form (M5_JNI_CALLS.md §2);
//! calling one LOUDLY aborts naming the registered T13 primitive 11
//! (`call_foreign_native`), never a silent wrong answer.
//!
//! Laws (M5_JNI_NATIVES.md §3): binding requires the class to DECLARE
//! the method `native` with the exact name+descriptor (else
//! `JNI_ERR` + a pending `NoSuchMethodError`); rebinding overrides;
//! keys are class-scoped (binding on a subclass never shadows an
//! inherited native); `UnregisterNatives` clears the class's bindings;
//! an unbound native call pends `java/lang/UnsatisfiedLinkError` and
//! answers the zero value.

use crate::jni::classes::class_name_from_tag;
use crate::{Throw, Value};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// One binding: the fn payload in one of the two shapes plus the
/// staticness recorded at bind time (the call sites already know the
/// family, so this is bookkeeping for `getModifiers`-class consumers).
#[derive(Clone, Copy)]
pub enum Binding {
    /// The v1 round-trip shape: a runtime method fn (the same type the
    /// generated lookup and the `Call*MethodA/V` dispatch speak).
    Rust(crate::MethodFn),
    /// A foreign C fn pointer, stored as the raw address the caller
    /// handed `RegisterNatives`. Not callable until T13's audited call
    /// thunk (primitive 11) lands — a call LOUDLY aborts.
    Foreign(usize),
}

struct Entry {
    binding: Binding,
}

/// The registry key: (declaring class, method name, descriptor).
type BindKey = (String, String, String);

static BINDINGS: LazyLock<Mutex<HashMap<BindKey, Entry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn bindings() -> &'static Mutex<HashMap<(String, String, String), Entry>> {
    &BINDINGS
}

/// The JNI law check: does this class DECLARE a native method with the
/// exact name+descriptor? Class-SCOPED by design — no ancestor walk
/// (M5_JNI_NATIVES.md §3.3).
fn declares_native(ctx: crate::Dispatch, class: &str, name: &str, desc: &str) -> bool {
    ctx.class_info(class).is_some_and(|info| {
        info.methods
            .iter()
            .any(|m| m.name == name && m.desc == desc && m.is_native)
    })
}

/// `RegisterNatives` (slot 215) body — one triple. `Some(())` = bound;
/// `None` = the declared-native check failed (the wrapper pends
/// `NoSuchMethodError` and answers `JNI_ERR`). Rebinding the same
/// triple replaces the earlier binding (the spec's override law).
/// The DECLARED method's own `is_static` is the fact of record — the
/// registry keys by (class, name, desc) and the call sites already
/// know the family from the declared row.
pub fn bind(class: &str, name: &str, desc: &str, binding: Binding) -> Option<()> {
    let ctx = super::host_dispatch();
    ctx.class_info(class)?
        .methods
        .iter()
        .find(|m| m.name == name && m.desc == desc && m.is_native)?;
    let mut b = bindings().lock().unwrap_or_else(|e| e.into_inner());
    b.insert(
        (class.to_string(), name.to_string(), desc.to_string()),
        Entry { binding },
    );
    Some(())
}

/// Test-only direct bind (bypasses the declared-native check; the slot
/// body and [`call_native`] laws are tested through the public path).
#[cfg(test)]
pub fn bind_for_test(class: &str, name: &str, desc: &str, binding: Binding) {
    bindings().lock().unwrap_or_else(|e| e.into_inner()).insert(
        (class.to_string(), name.to_string(), desc.to_string()),
        Entry { binding },
    );
}

/// `UnregisterNatives` (slot 216) body: clears ALL bindings of the
/// class. Unregistering a class with no bindings is still success (the
/// spec gives it no error).
pub fn unbind_class(class: &str) {
    bindings()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(c, _, _), _| c != class);
}

/// The consult: the binding for `(class, name, desc)`, if any.
pub fn lookup(class: &str, name: &str, desc: &str) -> Option<Binding> {
    bindings()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(class.to_string(), name.to_string(), desc.to_string()))
        .map(|e| e.binding)
}

/// The `MethodFn`-shaped view the dispatch tail invokes: a Rust binding
/// is returned as-is; a foreign fn pointer is a LOUD abort naming the
/// registered T13 primitive (M5_JNI_NATIVES.md §2 — never a silent
/// wrong answer).
pub fn as_method_fn(class: &str, name: &str, desc: &str, b: Binding) -> crate::MethodFn {
    match b {
        Binding::Rust(f) => f,
        Binding::Foreign(_) => panic!(
            "j2r JNI bridge: native binding for {class}::{name}{desc} is a \
             FOREIGN fn pointer — calling the C native ABI from a \
             forbid(unsafe_code) runtime needs the registered T13 \
             primitive 11 (call_foreign_native); refusing loudly"
        ),
    }
}

/// The emitted call-through body for `ACC_NATIVE` methods (M5_JNI_NATIVES.md
/// §4): consult the registry for the DECLARING class's binding and
/// invoke it, or pend `UnsatisfiedLinkError` (the unbound law — the
/// oracle's shape, never a silent zero). Same `MethodFn` contract as
/// every generated body: `Err` propagates through the caller's
/// zero-on-throw machinery.
pub fn call_native(
    owner: &str,
    name: &str,
    desc: &str,
    recv: Option<&crate::heap::JRef>,
    args: &[Value],
) -> Result<Value, Throw> {
    match lookup(owner, name, desc) {
        Some(b) => {
            let f = as_method_fn(owner, name, desc, b);
            f(recv, args)
        }
        None => Err(crate::throw_new(
            "java/lang/UnsatisfiedLinkError",
            Some(&format!("{owner}::{name}{desc}")),
        )),
    }
}

// ---------------------------------------------------------------------------
// Slot bodies (the wrappers are thin casts, the established pattern).
// ---------------------------------------------------------------------------

/// `RegisterNatives` (slot 215): reads the `JNINativeMethod` array
/// through the audited primitives and binds each triple. Returns
/// `JNI_OK`/`JNI_ERR` per the spec; a failed declared-native check
/// PENDS `NoSuchMethodError` naming the entry (already-bound earlier
/// entries of THIS call stay bound — the spec promises no atomicity
/// and HotSpot binds as it validates).
pub fn slot_register_natives(class_tag: u64, methods_ptr: usize, n: i32) -> i32 {
    use j2r_jni_ffi::ffi::{copy_from_caller, read_caller_str};
    if class_tag == 0 {
        // Defensive (Temurin's is UB there): null clazz → NPE + JNI_ERR.
        super::bridge::pend_exception(crate::throw_new("java/lang/NullPointerException", None));
        return super::JNI_ERR;
    }
    if n < 0 {
        // The spec gives a negative count no meaning; refuse honestly.
        return super::JNI_ERR;
    }
    let class = class_name_from_tag(class_tag).expect("a live jclass tag resolves");
    let ctx = super::host_dispatch();
    if n == 0 {
        return if class_name_known(ctx, &class) {
            super::JNI_OK
        } else {
            // Registering on an unknown class has no declared natives to
            // check — the spec's NoSuchMethodError arm.
            super::bridge::pend_exception(crate::throw_new(
                "java/lang/NoSuchMethodError",
                Some("<no bindings>: class has no compiled facts"),
            ));
            super::JNI_ERR
        };
    }
    if methods_ptr == 0 {
        // A null array with n > 0: Temurin's is UB; refuse with NPE.
        super::bridge::pend_exception(crate::throw_new("java/lang/NullPointerException", None));
        return super::JNI_ERR;
    }
    // JNINativeMethod = {const char* name; const char* signature; void* fnPtr;}
    // — three pointer-width words per entry (the generated struct is
    // repr(C) with exactly this layout; 24 bytes on this ABI).
    let bytes = copy_from_caller(methods_ptr as *const u8, n as usize * 24);
    if bytes.len() != n as usize * 24 {
        super::bridge::pend_exception(crate::throw_new(
            "java/lang/NoSuchMethodError",
            Some("<no bindings>: the method array is unreadable"),
        ));
        return super::JNI_ERR;
    }
    for i in 0..n as usize {
        let word = |k: usize| -> usize {
            usize::from_le_bytes(
                bytes[i * 24 + k * 8..i * 24 + k * 8 + 8]
                    .try_into()
                    .expect("8"),
            )
        };
        let name_ptr = word(0) as *const std::os::raw::c_char;
        let sig_ptr = word(1) as *const std::os::raw::c_char;
        let fn_ptr = word(2);
        let Some(name_bytes) = read_caller_str(name_ptr) else {
            super::bridge::pend_exception(crate::throw_new(
                "java/lang/NoSuchMethodError",
                Some("<null name>"),
            ));
            return super::JNI_ERR;
        };
        let Some(sig_bytes) = read_caller_str(sig_ptr) else {
            super::bridge::pend_exception(crate::throw_new(
                "java/lang/NoSuchMethodError",
                Some("<null signature>"),
            ));
            return super::JNI_ERR;
        };
        let name = String::from_utf8_lossy(&name_bytes).to_string();
        let desc = String::from_utf8_lossy(&sig_bytes).to_string();
        if fn_ptr == 0 {
            super::bridge::pend_exception(crate::internal_error(&format!(
                "RegisterNatives entry {class}::{name}{desc} has a NULL fnPtr"
            )));
            return super::JNI_ERR;
        }
        if !declares_native(ctx, &class, &name, &desc) {
            super::bridge::pend_exception(crate::throw_new(
                "java/lang/NoSuchMethodError",
                Some(&format!("{name} {desc}")),
            ));
            return super::JNI_ERR;
        }
        // Shape law (M5_JNI_NATIVES.md §2, 12b precision of record): the
        // C-array slot path can only yield FOREIGN fn pointers — the C
        // ABI gives no way to distinguish a Rust MethodFn from a C fn,
        // and reconstructing a Rust fn from a word would take an unsafe
        // transmute this crate forbids. Rust-shape bindings ride the
        // Rust API ([`bind`], used by the emitted glue and the tests);
        // a Foreign binding call LOUDLY aborts until T13's audited call
        // thunk (primitive 11) lands.
        if bind(&class, &name, &desc, Binding::Foreign(fn_ptr)).is_none() {
            // bind() only fails on the same declared-native check we just
            // ran; reaching here is a bookkeeping bug — loud.
            super::bridge::pend_exception(crate::internal_error(&format!(
                "RegisterNatives: {class}::{name}{desc} passed validation but failed to bind"
            )));
            return super::JNI_ERR;
        }
    }
    super::JNI_OK
}

fn class_name_known(ctx: crate::Dispatch, class: &str) -> bool {
    ctx.class_info(class).is_some()
}

/// `UnregisterNatives` (slot 216): clears the class's bindings, `JNI_OK`.
/// A null/unknown class pends NPE and answers `JNI_ERR` (defensive —
/// Temurin's is UB there).
pub fn slot_unregister_natives(class_tag: u64) -> i32 {
    if class_tag == 0 {
        super::bridge::pend_exception(crate::throw_new("java/lang/NullPointerException", None));
        return super::JNI_ERR;
    }
    let class = class_name_from_tag(class_tag).expect("a live jclass tag resolves");
    unbind_class(&class);
    super::JNI_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    // The fixture (Base/Derived with a native method declared on Base)
    // mirrors the calls.rs mock-dispatch fixture's shape; every test
    // takes `runtime()` first — the registry is process-global.
    static BASE_METHODS: [crate::UserMethod; 2] = [
        crate::UserMethod {
            name: "nativeAdd",
            desc: "(II)I",
            is_static: false,
            is_native: true,
        },
        crate::UserMethod {
            name: "nativeScale",
            desc: "(I)I",
            is_static: true,
            is_native: true,
        },
    ];
    static NATIVES_DERIVED_PARENTS: [&str; 1] = ["j2r/test/NativesBase"];

    fn meta_of(c: &str) -> Option<&'static crate::UserClassInfo> {
        static BASE_INFO: crate::UserClassInfo = crate::UserClassInfo {
            is_interface: false,
            simple_name: Some("NativesBase"),
            fields: &[],
            methods: &BASE_METHODS,
        };
        static DERIVED_INFO: crate::UserClassInfo = crate::UserClassInfo {
            is_interface: false,
            simple_name: Some("NativesDerived"),
            fields: &[],
            methods: &[],
        };
        match c {
            "j2r/test/NativesBase" => Some(&BASE_INFO),
            "j2r/test/NativesDerived" => Some(&DERIVED_INFO),
            _ => None,
        }
    }

    fn parents_of(c: &str) -> Option<&'static [&'static str]> {
        if c == "j2r/test/NativesDerived" {
            Some(&NATIVES_DERIVED_PARENTS)
        } else {
            None
        }
    }

    fn rust_add(recv: Option<&crate::heap::JRef>, args: &[Value]) -> Result<Value, Throw> {
        let _ = recv;
        match (args.first(), args.get(1)) {
            (Some(Value::Int(a)), Some(Value::Int(b))) => Ok(Value::Int(a + b)),
            other => panic!("fixture nativeAdd expects two Ints, got {other:?}"),
        }
    }

    fn rust_scale(recv: Option<&crate::heap::JRef>, args: &[Value]) -> Result<Value, Throw> {
        let _ = recv;
        match args.first() {
            Some(Value::Int(a)) => Ok(Value::Int(a * 10)),
            other => panic!("fixture nativeScale expects one Int, got {other:?}"),
        }
    }

    fn install() {
        super::super::install_host_dispatch(crate::Dispatch {
            lookup: None,
            parents: Some(parents_of),
            meta: Some(meta_of),
            static_get: None,
            static_set: None,
        });
    }

    fn class_tag(internal: &str) -> u64 {
        super::super::bridge::new_local(crate::reflect::class_ref(internal))
    }

    #[test]
    fn bind_requires_a_declared_native_method() {
        let _rt = crate::test_support::runtime();
        install();
        // Declared native → binds.
        assert!(bind(
            "j2r/test/NativesBase",
            "nativeAdd",
            "(II)I",
            Binding::Rust(rust_add)
        )
        .is_some());
        // Not declared at all → None.
        assert!(bind(
            "j2r/test/NativesBase",
            "nope",
            "(II)I",
            Binding::Rust(rust_add)
        )
        .is_none());
        // Declared but NOT native (no such row here) → None; a wrong
        // descriptor → None.
        assert!(bind(
            "j2r/test/NativesBase",
            "nativeAdd",
            "(J)I",
            Binding::Rust(rust_add)
        )
        .is_none());
        // The consult sees exactly the bound triple.
        assert!(lookup("j2r/test/NativesBase", "nativeAdd", "(II)I").is_some());
        assert!(lookup("j2r/test/NativesBase", "nativeAdd", "(J)I").is_none());
        clear();
    }

    #[test]
    fn rebind_overrides_and_unregister_clears() {
        let _rt = crate::test_support::runtime();
        install();
        assert!(bind(
            "j2r/test/NativesBase",
            "nativeScale",
            "(I)I",
            Binding::Rust(rust_scale)
        )
        .is_some());
        let recv = None;
        let f = as_method_fn(
            "j2r/test/NativesBase",
            "nativeScale",
            "(I)I",
            lookup("j2r/test/NativesBase", "nativeScale", "(I)I").expect("bound"),
        );
        assert!(matches!(f(recv, &[Value::Int(7)]), Ok(Value::Int(70))));
        // Rebinding overrides: a Rust binding returning -1 wins forever.
        fn rust_neg(_recv: Option<&crate::heap::JRef>, _args: &[Value]) -> Result<Value, Throw> {
            Ok(Value::Int(-1))
        }
        assert!(bind(
            "j2r/test/NativesBase",
            "nativeScale",
            "(I)I",
            Binding::Rust(rust_neg)
        )
        .is_some());
        let f = as_method_fn(
            "j2r/test/NativesBase",
            "nativeScale",
            "(I)I",
            lookup("j2r/test/NativesBase", "nativeScale", "(I)I").expect("bound"),
        );
        assert!(matches!(f(recv, &[Value::Int(7)]), Ok(Value::Int(-1))));
        // Unregister clears; the consult misses afterwards.
        unbind_class("j2r/test/NativesBase");
        assert!(lookup("j2r/test/NativesBase", "nativeScale", "(I)I").is_none());
        clear();
    }

    #[test]
    fn call_native_unbound_pends_unsatisfied_link_error() {
        let _rt = crate::test_support::runtime();
        install();
        // Unbound: the law is Err(UnsatisfiedLinkError) — the caller's
        // zero-on-throw machinery turns that into the faithful shape.
        let err = call_native(
            "j2r/test/NativesBase",
            "nativeAdd",
            "(II)I",
            None,
            &[Value::Int(1), Value::Int(2)],
        )
        .expect_err("unbound");
        assert_eq!(
            crate::heap::with_obj(err.obj, |o| o.jclass.to_string()),
            "java/lang/UnsatisfiedLinkError"
        );
        // Bound: the round-trip works through the SAME entry point the
        // emitted call-through body uses.
        assert!(bind(
            "j2r/test/NativesBase",
            "nativeAdd",
            "(II)I",
            Binding::Rust(rust_add)
        )
        .is_some());
        assert!(matches!(
            call_native(
                "j2r/test/NativesBase",
                "nativeAdd",
                "(II)I",
                None,
                &[Value::Int(20), Value::Int(22)],
            ),
            Ok(Value::Int(42))
        ));
        clear();
    }

    #[test]
    fn foreign_fn_pointer_is_stored_but_loud_on_call() {
        let _rt = crate::test_support::runtime();
        install();
        // A foreign fnPtr binds (the C harness shape, T13); calling it
        // LOUDLY aborts naming primitive 11 — pinned here without a
        // child process (the panic never crosses an extern boundary in
        // this test).
        bind_for_test(
            "j2r/test/NativesBase",
            "nativeScale",
            "(I)I",
            Binding::Foreign(0xDEAD_C0DE),
        );
        let b = lookup("j2r/test/NativesBase", "nativeScale", "(I)I").expect("stored");
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = as_method_fn("j2r/test/NativesBase", "nativeScale", "(I)I", b);
        }))
        .is_err();
        assert!(panicked, "a foreign fnPtr call must be loud until T13");
        clear();
    }

    #[test]
    fn the_dispatch_walk_consults_the_registry_through_inheritance() {
        let _rt = crate::test_support::runtime();
        install();
        // Bind the native on BASE; the receiver's runtime class is
        // Derived (which declares nothing). The Call*MethodA walk must
        // reach the binding at Base — the M5_JNI_NATIVES.md §4 law.
        assert!(bind(
            "j2r/test/NativesBase",
            "nativeAdd",
            "(II)I",
            Binding::Rust(rust_add)
        )
        .is_some());
        let recv = super::super::bridge::new_local(crate::heap::alloc(crate::object::JObj {
            jclass: std::sync::Arc::from("j2r/test/NativesDerived"),
            slots: vec![],
            prim: None,
        }));
        // The method ID mints against Base (the walk finds it there).
        let base_cls = class_tag("j2r/test/NativesBase");
        let mid = super::super::classes::slot_get_method_id(base_cls, "nativeAdd", "(II)I", false)
            .expect("Base declares nativeAdd native");
        let mut w0 = [0u8; 8];
        let mut w1 = [0u8; 8];
        w0[0..4].copy_from_slice(&20_i32.to_le_bytes());
        w1[0..4].copy_from_slice(&22_i32.to_le_bytes());
        let words = [w0, w1].concat();
        assert_eq!(
            super::super::calls::slot_call_int_method_a(mid, recv, words.as_ptr() as usize),
            42,
            "the walk consult reaches the binding through inheritance"
        );
        clear();
        super::super::bridge::delete_local(recv);
        super::super::bridge::delete_local(base_cls);
    }

    fn clear() {
        unbind_class("j2r/test/NativesBase");
    }
}
