//! `LambdaMetafactory` bootstrap resolution (M3 Wave 3, M3_PLAN §6).
//!
//! javac compiles every lambda and method reference to an `invokedynamic`
//! site bootstrapped by `java/lang/invoke/LambdaMetafactory.metafactory`.
//! At compile time J2R resolves the site into a [`LambdaPlan`]:
//!
//! * the functional interface (the indy descriptor's return type),
//! * the SAM name (the indy `NameAndType` name),
//! * the capture signature (the indy descriptor's parameter types —
//!   JLS 15.27.2: captured values are copied at construction),
//! * the implementation target (the `implMethod` handle: kind + member),
//! * and a coarse [`shape`](LambdaPlan::shape) the runtime uses to
//!   reassemble the call at SAM-invocation time.
//!
//! Emission turns the site into closure construction: a heap object whose
//! class *is* the functional interface (so subtype checks and interface
//! dispatch are exact, see the honesty note in M3_EVIDENCE Wave 3) and
//! whose slots carry the shape, target and captured values. The native
//! SAM rows in `j2r-runtime/src/natives.rs` re-prepend the captures and
//! dispatch into the compiled `lambda$…` method or the method-ref target
//! through the ordinary `Dispatch` walk.

use crate::{Program, TypeDesc};
use j2r_runtime::{builtin, natives, parse_method_desc};

/// The only supported bootstrap method.
pub const LMF_CLASS: &str = "java/lang/invoke/LambdaMetafactory";
pub const LMF_METHOD: &str = "metafactory";

/// The M3 functional-interface set (M3_PLAN §6.1). Interfaces outside the
/// set (e.g. `IntPredicate`) are loud compile errors.
pub const SUPPORTED_INTERFACES: &[&str] = &[
    "java/util/function/Function",
    "java/util/function/BiFunction",
    "java/util/function/Supplier",
    "java/util/function/Consumer",
    "java/util/function/BiConsumer",
    "java/util/function/Predicate",
    "java/util/function/BiPredicate",
    "java/util/function/UnaryOperator",
    "java/util/function/BinaryOperator",
    // java.util.Comparator — the everyday sort key (real programs use it
    // constantly; the BigRpg fixture exercises it through a merge sort).
    "java/util/Comparator",
    // M4 Wave 2: java.lang.Runnable — the Thread body (T06). The closure
    // machinery is interface-generic; this adds the everyday
    // `new Thread(() -> ...)` shape (ThreadHello/ThreadSafepoint).
    "java/lang/Runnable",
];

/// Shape of the implementation call, reassembled at SAM-invocation time:
///
/// * `0` `SHAPE_STATIC` — `caps ++ sam_args`, no receiver
///   (`REF_invokeStatic`, kind 6).
/// * `1` `SHAPE_BOUND` — receiver = capture 0, args = `caps[1..] ++ sam_args`
///   (`REF_invokeVirtual`/`REF_invokeInterface` with captures, and
///   `REF_invokeSpecial` for `this::m`/`super::m`).
/// * `2` `SHAPE_UNBOUND` — receiver = `sam_args[0]`, args = `sam_args[1..]`
///   (`REF_invokeVirtual`/`REF_invokeInterface` with no captures, e.g.
///   `String::toUpperCase`).
/// * `3` `SHAPE_CTOR` — construct the target, call `<init>` with
///   `caps ++ sam_args` (`REF_newInvokeSpecial`, kind 8).
pub const SHAPE_STATIC: u8 = 0;
pub const SHAPE_BOUND: u8 = 1;
pub const SHAPE_UNBOUND: u8 = 2;
pub const SHAPE_CTOR: u8 = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct LambdaPlan {
    /// Functional interface the closure must satisfy (internal name).
    pub interface: String,
    /// SAM method name (the indy site's method name, e.g. `apply`).
    pub sam_name: String,
    /// One descriptor char per captured value (same alphabet as
    /// `ConcatPlan::kinds`).
    pub capture_kinds: String,
    /// One of [`SHAPE_STATIC`]..[`SHAPE_CTOR`].
    pub shape: u8,
    /// `implMethod` target member.
    pub target_class: String,
    pub target_name: String,
    pub target_desc: String,
    /// Instance-slot count for `REF_newInvokeSpecial` targets (compile-time
    /// knowledge the runtime cannot recover; unused by other shapes).
    pub ctor_slots: u32,
}

/// Resolves the bootstrap method's owner class for one indy site (used to
/// dispatch between StringConcatFactory and LambdaMetafactory planning).
pub fn bootstrap_class_of(c: &crate::ClassPlan, indy_index: u16) -> Result<String, String> {
    let cp = &c.cf.constant_pool;
    let (bsm_index, _, _) = cp.dynamic(indy_index).map_err(|e| e.message)?;
    let bsm =
        c.cf.bootstrap_methods
            .get(bsm_index as usize)
            .ok_or_else(|| format!("bootstrap method #{bsm_index} missing"))?;
    let handle = cp.entry(bsm.method_handle).map_err(|e| e.message)?;
    match handle {
        j2r_classfile::CpEntry::MethodHandle { index, .. } => {
            let (cls, _, _) = cp.member_ref(*index).map_err(|e| e.message)?;
            Ok(cls)
        }
        other => Err(format!("bootstrap handle is {}", other.tag_name())),
    }
}

/// Resolves one `invokedynamic` site to a [`LambdaPlan`], or fails with the
/// exact unsupported shape (loud, naming the bootstrap — M3_PLAN §6.4).
pub fn resolve_lambda(prog: &Program, ci: usize, indy_index: u16) -> Result<LambdaPlan, String> {
    let c = &prog.classes[ci];
    let cp = &c.cf.constant_pool;
    let (bsm_index, sam_name, indy_desc) = cp.dynamic(indy_index).map_err(|e| e.message)?;

    // The indy descriptor: (captures...)Interface.
    let (caps, ret) = parse_method_desc(&indy_desc)
        .ok_or_else(|| format!("bad indy descriptor {indy_desc:?}"))?;
    let interface = match ret {
        TypeDesc::Object(o) => o,
        other => {
            return Err(format!(
                "indy site must return an interface type, got {other:?}"
            ))
        }
    };
    if !SUPPORTED_INTERFACES.contains(&interface.as_str()) {
        return Err(format!(
            "functional interface {interface} is outside the M3 LambdaMetafactory set (supported: {})",
            SUPPORTED_INTERFACES.join(", ")
        ));
    }
    let capture_kinds: String = caps.iter().map(kind_char).collect();

    let bsm =
        c.cf.bootstrap_methods
            .get(bsm_index as usize)
            .ok_or_else(|| format!("bootstrap method #{bsm_index} missing"))?;
    let bhandle = cp.entry(bsm.method_handle).map_err(|e| e.message)?;
    let (bcls, bmeth) = match bhandle {
        j2r_classfile::CpEntry::MethodHandle { index, .. } => {
            let (cls, meth, _) = cp.member_ref(*index).map_err(|e| e.message)?;
            (cls, meth)
        }
        other => return Err(format!("bootstrap handle is {}", other.tag_name())),
    };
    if bcls != LMF_CLASS || bmeth != LMF_METHOD {
        return Err(format!(
            "unsupported invokedynamic bootstrap {bcls}.{bmeth} (J2R supports LambdaMetafactory.metafactory and StringConcatFactory.makeConcatWithConstants)"
        ));
    }

    // Static arguments: [samMethodType, implMethod, instantiatedMethodType].
    let sam_desc = match bsm.arguments.first() {
        Some(j2r_classfile::BootstrapArgument::MethodType(d)) => d.clone(),
        Some(other) => {
            return Err(format!(
                "LambdaMetafactory argument 0 (samMethodType) must be a MethodType, found {other:?}"
            ))
        }
        None => return Err("LambdaMetafactory without samMethodType".to_string()),
    };
    let (sam_params, sam_ret) =
        parse_method_desc(&sam_desc).ok_or_else(|| format!("bad samMethodType {sam_desc:?}"))?;
    let impl_handle = match bsm.arguments.get(1) {
        Some(j2r_classfile::BootstrapArgument::MethodHandle {
            kind,
            class,
            name,
            desc,
        }) => (*kind, class.clone(), name.clone(), desc.clone()),
        Some(other) => {
            return Err(format!(
                "LambdaMetafactory argument 1 (implMethod) must be a MethodHandle, found {other:?}"
            ))
        }
        None => return Err("LambdaMetafactory without implMethod".to_string()),
    };
    let inst_desc = match bsm.arguments.get(2) {
        Some(j2r_classfile::BootstrapArgument::MethodType(d)) => d.clone(),
        Some(other) => {
            return Err(format!(
                "LambdaMetafactory argument 2 (instantiatedMethodType) must be a MethodType, found {other:?}"
            ))
        }
        None => return Err("LambdaMetafactory without instantiatedMethodType".to_string()),
    };
    let (inst_params, _inst_ret) = parse_method_desc(&inst_desc)
        .ok_or_else(|| format!("bad instantiatedMethodType {inst_desc:?}"))?;

    // SAM arity pinned per interface (the M3 set is exactly the generic
    // java.util.function shapes; erasure agreement check M3_PLAN §6.1).
    let sam_arity = sam_arity_of(&interface)?;
    if sam_params.len() != sam_arity {
        return Err(format!(
            "samMethodType {sam_desc} has {} parameters, {interface}'s SAM has {sam_arity}",
            sam_params.len()
        ));
    }
    if inst_params.len() != sam_arity {
        return Err(format!(
            "instantiatedMethodType {inst_desc} disagrees with samMethodType {sam_desc}"
        ));
    }
    check_sam_return(&interface, &sam_ret)?;

    let (kind, tcls, tname, tdesc) = impl_handle;
    let (impl_params, impl_ret) =
        parse_method_desc(&tdesc).ok_or_else(|| format!("bad implMethod descriptor {tdesc:?}"))?;
    let n_caps = caps.len();
    // Void-ness must agree: consumer SAMs (accept(... )V) have void impls,
    // every other SAM in the M3 set has a non-void impl.
    let sam_void = matches!(sam_ret, TypeDesc::Void);

    let (shape, ctor_slots) = match kind {
        // REF_invokeStatic: (caps..., sam-args...)ret
        6 => {
            if impl_params.len() != n_caps + sam_arity {
                return Err(format!(
                    "implMethod {tcls}.{tname}{tdesc} arity disagrees with the capture signature ({n_caps} captures, SAM arity {sam_arity})"
                ));
            }
            (SHAPE_STATIC, 0)
        }
        // REF_invokeVirtual / REF_invokeInterface. Bound (captures>0): the
        // receiver was captured, the impl takes exactly the SAM arguments.
        // Unbound (captures==0): the receiver arrives as the first SAM
        // argument, so the impl descriptor holds one fewer parameter than
        // the SAM (e.g. `String::toUpperCase` — impl `()String`, SAM
        // `apply(Object)Object`).
        5 | 9 => {
            if n_caps > 0 {
                if impl_params.len() != sam_arity {
                    return Err(format!(
                        "bound implMethod {tcls}.{tname}{tdesc} arity disagrees with SAM arity {sam_arity}"
                    ));
                }
                (SHAPE_BOUND, 0)
            } else {
                if sam_arity < 1 || impl_params.len() != sam_arity - 1 {
                    return Err(format!(
                        "unbound implMethod {tcls}.{tname}{tdesc} must take the receiver plus {} SAM arguments",
                        sam_arity.saturating_sub(1)
                    ));
                }
                (SHAPE_UNBOUND, 0)
            }
        }
        // REF_invokeSpecial: javac captures the receiver (this::m, super::m).
        7 => {
            if n_caps == 0 {
                return Err(
                    "REF_invokeSpecial method reference without a captured receiver is not supported"
                        .to_string(),
                );
            }
            if impl_params.len() != sam_arity {
                return Err(format!(
                    "implMethod {tcls}.{tname}{tdesc} arity disagrees with SAM arity {sam_arity}"
                ));
            }
            (SHAPE_BOUND, 0)
        }
        // REF_newInvokeSpecial: constructor reference.
        8 => {
            if !matches!(impl_ret, TypeDesc::Void) {
                return Err(format!(
                    "constructor reference target {tcls}.{tname}{tdesc} must return void"
                ));
            }
            if impl_params.len() != n_caps + sam_arity {
                return Err(format!(
                    "constructor {tcls}.{tname}{tdesc} arity disagrees with the capture signature"
                ));
            }
            let slots = ctor_slot_count(prog, ci, &tcls)?;
            (SHAPE_CTOR, slots)
        }
        other => {
            return Err(format!(
                "unsupported implMethod handle kind {other} (supported kinds: 5, 6, 7, 8, 9)"
            ))
        }
    };

    // Void-ness agreement applies uniformly (constructor refs handled by
    // their own void check above).
    if shape != SHAPE_CTOR && matches!(impl_ret, TypeDesc::Void) != sam_void {
        return Err(format!(
            "implMethod {tcls}.{tname}{tdesc} return disagrees with the SAM (void: impl={}, sam={sam_void})",
            matches!(impl_ret, TypeDesc::Void)
        ));
    }

    // The target must resolve somewhere: a compiled method (the generated
    // registry) or a native row. Otherwise the closure would explode at
    // first SAM invocation instead of failing the build. The check uses the
    // raw classfile method list because resolution happens *during*
    // plan_methods — the compiled `MethodPlan`s do not exist yet.
    if let Some(&ci_target) = prog.user_index.get(&tcls) {
        let tc = &prog.classes[ci_target];
        if tc.cf.method(&tname, &tdesc).is_none() {
            return Err(format!(
                "implMethod target {tcls}.{tname}{tdesc} is not a compiled method"
            ));
        }
    } else if natives::lookup_exact(&tcls, &tname, &tdesc).is_none() {
        return Err(format!(
            "implMethod target {tcls}.{tname}{tdesc} is neither compiled nor a runtime native"
        ));
    }

    Ok(LambdaPlan {
        interface,
        sam_name,
        capture_kinds,
        shape,
        target_class: tcls,
        target_name: tname,
        target_desc: tdesc,
        ctor_slots,
    })
}

/// SAM arity of each interface in the M3 set.
fn sam_arity_of(interface: &str) -> Result<usize, String> {
    Ok(match interface {
        "java/util/function/Function"
        | "java/util/function/Consumer"
        | "java/util/function/Predicate"
        | "java/util/function/UnaryOperator" => 1,
        // M4 Wave 2: Runnable.run()V — a 0-arity SAM (the Thread body).
        "java/lang/Runnable" => 0,
        "java/util/function/BiFunction"
        | "java/util/function/BiConsumer"
        | "java/util/function/BiPredicate"
        | "java/util/function/BinaryOperator"
        | "java/util/Comparator" => 2,
        "java/util/function/Supplier" => 0,
        other => unreachable!("interface {other} passed the supported-set check"),
    })
}

/// Erased SAM return kind per interface: `Object` for the generic ones,
/// `void` for consumers, `boolean` for predicates, primitive `int` for
/// Comparator (its SAM is `compare(Ljava/lang/Object;Ljava/lang/Object;)I`).
fn check_sam_return(interface: &str, ret: &TypeDesc) -> Result<(), String> {
    let ok = match interface {
        "java/util/function/Consumer" | "java/util/function/BiConsumer" => {
            matches!(ret, TypeDesc::Void)
        }
        // M4 Wave 2: Runnable.run()V — void like the consumers.
        "java/lang/Runnable" => matches!(ret, TypeDesc::Void),
        "java/util/function/Predicate" | "java/util/function/BiPredicate" => {
            matches!(ret, TypeDesc::Boolean)
        }
        "java/util/Comparator" => matches!(ret, TypeDesc::Int),
        _ => matches!(ret, TypeDesc::Object(o) if o == "java/lang/Object"),
    };
    if ok {
        Ok(())
    } else {
        Err(format!(
            "samMethodType return {ret:?} does not match {interface}'s erased SAM"
        ))
    }
}

/// Instance-slot count for a constructor-reference target.
fn ctor_slot_count(prog: &Program, _ci: usize, cls: &str) -> Result<u32, String> {
    if let Some(&i) = prog.user_index.get(cls) {
        return Ok(prog.classes[i].total_slots as u32);
    }
    if let Some(n) = builtin::builtin_total_slots(cls) {
        return Ok(n as u32);
    }
    Err(format!(
        "constructor reference to class {cls}, which J2R cannot instantiate (not compiled, not a runtime builtin)"
    ))
}

/// Same alphabet as `ConcatPlan::kinds` (one char per operand-stack value).
fn kind_char(t: &TypeDesc) -> char {
    match t {
        TypeDesc::Int => 'I',
        TypeDesc::Long => 'J',
        TypeDesc::Float => 'F',
        TypeDesc::Double => 'D',
        TypeDesc::Char => 'C',
        TypeDesc::Boolean => 'Z',
        TypeDesc::Byte => 'B',
        TypeDesc::Short => 'S',
        TypeDesc::Object(_) | TypeDesc::Array(_) => 'L',
        TypeDesc::Void => 'V',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sam_arities_match_the_m3_set() {
        assert_eq!(sam_arity_of("java/util/function/Function").unwrap(), 1);
        assert_eq!(sam_arity_of("java/util/function/BiFunction").unwrap(), 2);
        assert_eq!(sam_arity_of("java/util/function/Supplier").unwrap(), 0);
        assert_eq!(sam_arity_of("java/util/function/Consumer").unwrap(), 1);
        assert_eq!(sam_arity_of("java/util/function/BiConsumer").unwrap(), 2);
        assert_eq!(sam_arity_of("java/util/function/Predicate").unwrap(), 1);
        assert_eq!(sam_arity_of("java/util/function/BiPredicate").unwrap(), 2);
        assert_eq!(sam_arity_of("java/util/function/UnaryOperator").unwrap(), 1);
        assert_eq!(
            sam_arity_of("java/util/function/BinaryOperator").unwrap(),
            2
        );
    }

    #[test]
    fn sam_return_kinds_are_pinned() {
        let obj = TypeDesc::Object("java/lang/Object".into());
        let z = TypeDesc::Boolean;
        let v = TypeDesc::Void;
        assert!(check_sam_return("java/util/function/Function", &obj).is_ok());
        assert!(check_sam_return("java/util/function/Supplier", &obj).is_ok());
        assert!(check_sam_return("java/util/function/Consumer", &v).is_ok());
        assert!(check_sam_return("java/util/function/BiConsumer", &v).is_ok());
        assert!(check_sam_return("java/util/function/Predicate", &z).is_ok());
        assert!(check_sam_return("java/util/function/BiPredicate", &z).is_ok());
        // wrong kinds are loud
        assert!(check_sam_return("java/util/function/Consumer", &obj).is_err());
        assert!(check_sam_return("java/util/function/Predicate", &v).is_err());
        assert!(check_sam_return("java/util/function/Function", &z).is_err());
    }

    #[test]
    fn capture_kinds_alphabet_matches_concat() {
        assert_eq!(kind_char(&TypeDesc::Int), 'I');
        assert_eq!(kind_char(&TypeDesc::Long), 'J');
        assert_eq!(kind_char(&TypeDesc::Object("x".into())), 'L');
        assert_eq!(kind_char(&TypeDesc::Array("[I".into())), 'L');
    }
}
