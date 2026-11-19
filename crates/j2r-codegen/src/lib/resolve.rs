//! `lib`::resolve — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use j2r_runtime::{builtin, natives};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Resolves a static or special (non-virtual) method reference.
pub(crate) fn resolve_call(
    prog: &Program,
    start: &str,
    name: &str,
    desc: &str,
) -> Result<CallTarget> {
    let mut cur = start.to_string();
    for _ in 0..64 {
        if prog.is_user(&cur) {
            let c = prog.class(&cur).unwrap();
            if let Some(m) = c.method(name, desc) {
                return Ok(CallTarget::User {
                    module: c.module.clone(),
                    fn_name: m.fn_name.clone(),
                });
            }
            match &c.super_name {
                Some(s) => cur = s.clone(),
                None => break,
            }
        } else {
            break;
        }
    }
    // builtin chain, continuing from the first builtin ancestor
    for _ in 0..64 {
        if let Some(n) = natives::lookup_exact(&cur, name, desc) {
            let _ = n;
            return Ok(CallTarget::Native { class: cur });
        }
        match builtin::builtin_parent_of(&cur) {
            Some(p) => cur = p.to_string(),
            None => break,
        }
    }
    Err(CodegenError::Semantic {
        class: start.to_string(),
        message: format!(
            "cannot resolve method {name}{desc} starting at {start} (is its class in the inputs, or is it an unsupported JDK method?)"
        ),
    })
}

/// Validates a virtual/interface call target exists somewhere in the
/// receiver's hierarchy (compiled or native). Abstract declarations count.
///
/// The walk covers the superclass chain **and the interface hierarchy**:
/// JVMS 5.4.3.3 resolves `invokevirtual` through superinterfaces too —
/// javac emits the receiver's compile-time type as the methodref owner
/// even when the method is declared in an interface the class implements
/// (caught by the JarSuper differential fixture, M3 Wave 1).
pub(crate) fn validate_virtual(
    prog: &Program,
    declared: &str,
    name: &str,
    desc: &str,
) -> Result<()> {
    // Array receivers (corpus wave 2, JZlib `$values()`): every array type
    // is a subtype of Object with a public `clone()` (JVMS 2.15) plus the
    // Object methods — javac emits ordinary invokevirtual sites against the
    // array class.
    if declared.starts_with('[') {
        const ARRAY_OK: &[(&str, &str)] = &[
            ("clone", "()Ljava/lang/Object;"),
            ("getClass", "()Ljava/lang/Class;"),
            ("hashCode", "()I"),
            ("equals", "(Ljava/lang/Object;)Z"),
            ("toString", "()Ljava/lang/String;"),
        ];
        if ARRAY_OK.iter().any(|(n, d)| *n == name && *d == desc) {
            return Ok(());
        }
    }
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut builtin_frontier: Vec<String> = Vec::new();
    queue.push_back(declared.to_string());
    while let Some(cur) = queue.pop_front() {
        if !seen.insert(cur.clone()) {
            continue;
        }
        if prog.is_user(&cur) {
            let c = prog.class(&cur).unwrap();
            let declared = c.cf.method(name, desc).is_some();
            let compiled = c.method(name, desc).is_some();
            if declared || compiled {
                return Ok(()); // abstract declarations dispatch from the runtime class
            }
            if let Some(s) = &c.super_name {
                queue.push_back(s.clone());
            }
            for itf in &c.interfaces {
                queue.push_back(itf.clone());
            }
        } else {
            builtin_frontier.push(cur);
        }
    }
    // Continue the walk from every builtin ancestor reached (its own
    // superclass chain may still declare a native implementation).
    for start in builtin_frontier {
        let mut cur = start;
        for _ in 0..64 {
            if natives::lookup_exact(&cur, name, desc).is_some() {
                return Ok(());
            }
            match builtin::builtin_parent_of(&cur) {
                Some(p) => cur = p.to_string(),
                None => break,
            }
        }
    }
    Err(CodegenError::Semantic {
        class: declared.to_string(),
        message: format!(
            "virtual call target {name}{desc} not found on {declared} or its hierarchy"
        ),
    })
}

/// Resolves an instance or static field reference to its owning user class
/// and its slot plan. `want_static` selects the field kind. The walk follows
/// JVMS 5.4.3.2 field resolution: the named class, then its direct
/// superinterfaces (recursively, BFS — a class inherits statics through a
/// constant interface), then the superclass chain. javac emits the Fieldref
/// with the ACCESSING class as owner for inherited statics, so the interface
/// steps are load-bearing for the constant-interface pattern.
pub(crate) fn resolve_field<'a>(
    prog: &'a Program,
    start: &str,
    field: &str,
    want_static: bool,
) -> Result<&'a FieldPlan> {
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(start.to_string());
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..256 {
        let Some(cur) = queue.pop_front() else { break };
        if !seen.insert(cur.clone()) {
            continue;
        }
        if prog.is_user(&cur) {
            let c = prog.class(&cur).unwrap();
            if let Some(f) = c.fields.iter().find(|f| f.name == field) {
                if f.is_static == want_static {
                    return Ok(f);
                }
                return Err(CodegenError::Semantic {
                    class: start.to_string(),
                    message: format!(
                        "field {field} of {cur} is {} but accessed as {}",
                        if f.is_static { "static" } else { "instance" },
                        if want_static { "static" } else { "instance" }
                    ),
                });
            }
            for i in &c.interfaces {
                queue.push_back(i.clone());
            }
            if let Some(s) = &c.super_name {
                queue.push_back(s.clone());
            }
        } else {
            continue;
        }
    }
    Err(CodegenError::Semantic {
        class: start.to_string(),
        message: format!(
            "cannot resolve {} field {field} starting at {start} (unsupported JDK fields must be modelled as natives)",
            if want_static { "static" } else { "instance" }
        ),
    })
}

/// Resolves an INSTANCE field access to its absolute object-slot index,
/// walking user classes and then builtin ancestors. The builtin case
/// (corpus wave 2, JZlib): user stream subclasses touch the protected
/// `FilterOutputStream.out` / `FilterInputStream.in` fields by name —
/// `builtin_field_slot` names them, and the absolute slot is the owner's
/// inherited base (its parent chain's total, exactly how the layout pass
/// computes `base_slots`) plus the field's own index.
pub fn resolve_field_slot(
    prog: &Program,
    start: &str,
    field: &str,
    want_static: bool,
) -> Result<usize> {
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(start.to_string());
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..256 {
        let Some(cur) = queue.pop_front() else { break };
        if !seen.insert(cur.clone()) {
            continue;
        }
        if prog.is_user(&cur) {
            let c = prog.class(&cur).unwrap();
            if let Some(f) = c.fields.iter().find(|f| f.name == field) {
                if f.is_static != want_static {
                    return Err(CodegenError::Semantic {
                        class: start.to_string(),
                        message: format!(
                            "field {field} of {cur} is {} but accessed as {}",
                            if f.is_static { "static" } else { "instance" },
                            if want_static { "static" } else { "instance" }
                        ),
                    });
                }
                return Ok(if want_static {
                    f.static_slot.expect("static slot assigned") as usize
                } else {
                    f.instance_slot.expect("instance slot assigned")
                });
            }
            for i in &c.interfaces {
                queue.push_back(i.clone());
            }
            if let Some(s) = &c.super_name {
                queue.push_back(s.clone());
            }
        } else {
            if !want_static {
                if let Some(own) = builtin::builtin_field_slot(&cur, field) {
                    let base = match builtin::builtin_parent_of(&cur) {
                        Some(p) => builtin::builtin_total_slots(p).ok_or_else(|| {
                            CodegenError::Semantic {
                                class: start.to_string(),
                                message: format!("unknown builtin ancestor {p} of {cur}"),
                            }
                        })?,
                        None => 0,
                    };
                    return Ok(base + own);
                }
            }
            continue;
        }
    }
    Err(CodegenError::Semantic {
        class: start.to_string(),
        message: format!(
            "cannot resolve {} field {field} starting at {start} (unsupported JDK fields must be modelled as natives)",
            if want_static { "static" } else { "instance" }
        ),
    })
}

pub fn resolve_field_slot_pub(
    prog: &Program,
    start: &str,
    field: &str,
    want_static: bool,
) -> Result<usize> {
    resolve_field_slot(prog, start, field, want_static)
}

/// Validates every instruction of every method of `c` against `prog`.
pub(crate) fn validate_class(prog: &Program, c: &ClassPlan) -> Result<()> {
    for m in &c.methods {
        let ctx = format!("{}{}", m.name, m.desc);
        for (pc, instr) in &m.instrs {
            validate_instr(prog, c, &ctx, *pc, instr)?;
        }
    }
    Ok(())
}

pub(crate) fn unsupported(c: &ClassPlan, method: &str, pc: u32, what: String) -> CodegenError {
    CodegenError::Unsupported {
        class: c.name.clone(),
        method: method.to_string(),
        pc,
        what,
    }
}

pub(crate) fn validate_instr(
    prog: &Program,
    c: &ClassPlan,
    method_ctx: &str,
    pc: u32,
    instr: &ir::Instr,
) -> Result<()> {
    match instr {
        ir::Instr::Ldc(ir::Ldc::Class(_)) => {
            // M3 Wave 5: emitted as an interned `Class` object (M3_PLAN §8.1).
            Ok(())
        }
        ir::Instr::GetStatic { class, field, .. } => {
            if class == "java/lang/System" && (field == "out" || field == "err") {
                return Ok(());
            }
            // Corpus wave 3 (JAMA): the Locale constant objects — builtin
            // materialized at emit time (only the referenced subset).
            if class == "java/util/Locale" && field == "US" {
                return Ok(());
            }
            // The nine `X.class` pseudo-fields (M3_PLAN §8.1): javac
            // compiles primitive class literals to `getstatic X.TYPE`, so
            // they are modelled as statics returning the interned Class
            // object of the primitive descriptor.
            if field == "TYPE" && primitive_type_field(class).is_some() {
                return Ok(());
            }
            resolve_field(prog, class, field, true).map(|_| ())
        }
        ir::Instr::PutStatic { class, field, .. } => {
            if class == "java/lang/System" {
                return Err(unsupported(
                    c,
                    method_ctx,
                    pc,
                    "assigning to System.out/System.err is not supported".to_string(),
                ));
            }
            resolve_field(prog, class, field, true).map(|_| ())
        }
        ir::Instr::GetField { class, field, .. } => {
            resolve_field_slot(prog, class, field, false).map(|_| ())
        }
        ir::Instr::PutField { class, field, .. } => {
            resolve_field_slot(prog, class, field, false).map(|_| ())
        }
        ir::Instr::InvokeStatic { class, name, desc } => {
            resolve_call(prog, class, name, desc).map(|_| ())
        }
        ir::Instr::InvokeSpecial { class, name, desc } => {
            resolve_call(prog, class, name, desc).map(|_| ())
        }
        ir::Instr::InvokeVirtual { class, name, desc }
        | ir::Instr::InvokeInterface { class, name, desc } => {
            validate_virtual(prog, class, name, desc)
        }
        ir::Instr::InvokeDynamic { index } => {
            if !c.concats.contains_key(index) && !c.lambdas.contains_key(index) {
                return Err(unsupported(
                    c,
                    method_ctx,
                    pc,
                    format!("invokedynamic #{index} was not resolved"),
                ));
            }
            Ok(())
        }
        ir::Instr::New { class } => {
            if prog.is_user(class) {
                let target = prog.class(class).unwrap();
                if target.is_abstract {
                    return Err(unsupported(
                        c,
                        method_ctx,
                        pc,
                        format!("cannot instantiate abstract class {class}"),
                    ));
                }
                return Ok(());
            }
            if builtin_is_instantiable(class) {
                Ok(())
            } else {
                Err(unsupported(
                    c,
                    method_ctx,
                    pc,
                    format!("instantiation of JDK class {class} is not supported"),
                ))
            }
        }
        ir::Instr::ANewArray { elem } => {
            // Interface-typed arrays are ordinary ref arrays: the F4/F5
            // wave registered Comparable by hand in builtin_parent_of, but
            // the general law is "the element type must be a user class,
            // Object, an array descriptor, or a known builtin CLASS OR
            // INTERFACE" (aastore's ASE check already walks the interface
            // subtype graph, so a stored value is checked against the
            // component type at the full strength of the type law).
            if prog.is_user(elem)
                || elem == "java/lang/Object"
                || builtin::builtin_parent_of(elem).is_some()
                || builtin::builtin_is_interface(elem)
                || elem.starts_with('[')
            {
                Ok(())
            } else {
                Err(unsupported(
                    c,
                    method_ctx,
                    pc,
                    format!("unknown array element type {elem}"),
                ))
            }
        }
        _ => Ok(()),
    }
}

/// Full pipeline: compile + write the crate to `opts.out_dir`.
pub fn compile_to_crate(opts: &Options) -> Result<PathBuf> {
    compile_to_crate_stats(opts).map(|(dir, _stats)| dir)
}

/// Full pipeline with build-manifest statistics (M5 T06/T07): compiles,
/// emits with per-class reuse + write-if-changed (single-crate or
/// partitioned workspace — both paths maintain the manifest), and returns
/// both the crate directory and what the emission actually did.
pub fn compile_to_crate_stats(opts: &Options) -> Result<(PathBuf, crate::manifest::EmitStats)> {
    let prog = compile(opts)?;
    let stats = emit::emit_crate(&prog, opts)?;
    Ok((opts.out_dir.clone(), stats))
}

/// Public wrappers used by the emitter (it needs the same resolution logic
/// the validator used).
pub fn resolve_call_pub(prog: &Program, start: &str, name: &str, desc: &str) -> Result<CallTarget> {
    resolve_call(prog, start, name, desc)
}

// ------------------------------------------------------ R5 slice 1: virtual resolution

/// Codegen-time resolution of a virtual/interface call site (R5 slice 1).
///
/// The generated crate is a closed world: a receiver statically typed as
/// the user class `declared` can carry, at run time, exactly the classes
/// `declared` plus its transitive user subclasses through `super_name`
/// links (builtins, arrays and foreign classes cannot appear — they are
/// not subtypes of a user class). For every such possible receiver the
/// walk mirrors the runtime [`j2r_runtime::invoke_virtual`] law level for
/// level: the compiled method at the level (the same `ClassPlan::method`
/// source the registry match arms come from), then a natives row at the
/// level, then the parent (the user `super_name` with the
/// `java/lang/Object` default, then `builtin::builtin_parent_of`).
///
/// `Some(target)` — the SAME single target for every possible receiver —
/// lets the emitter place the direct call. `None` keeps the dynamic path:
/// any divergence between walks, any walk that would die the internal
/// Error, any interface/array/builtin declared type. The dynamic path is
/// always faithful; the fold is only ever a shortcut over it.
pub fn resolve_virtual_target(
    prog: &Program,
    declared: &str,
    name: &str,
    desc: &str,
) -> Option<CallTarget> {
    // Closed-world gate: builtins, arrays and unknown names have an open
    // receiver set (a user class may extend a builtin; an array carries
    // the Object rows; the JVM may load classes we never compiled).
    if !prog.is_user(declared) {
        return None;
    }
    let receivers = virtual_receiver_set(prog, declared)?;
    // Unify every possible receiver's walk.
    let mut unified: Option<CallTarget> = None;
    for recv in &receivers {
        let target = walk_virtual_chain(prog, recv, name, desc);
        match (&unified, target) {
            (None, t) => unified = t,
            (Some(_), None) => return None, // one receiver would die the Error
            (Some(prev), Some(t)) => {
                let same = match (prev, &t) {
                    (
                        CallTarget::User { module, fn_name },
                        CallTarget::User {
                            module: m2,
                            fn_name: f2,
                        },
                    ) => module == m2 && fn_name == f2,
                    (CallTarget::Native { class }, CallTarget::Native { class: c2 }) => class == c2,
                    _ => false,
                };
                if !same {
                    return None; // the polymorphic residue — stays dynamic
                }
            }
        }
    }
    unified
}

/// The closed-world receiver set of a virtual/interface call site whose
/// declared type is the user class or interface `declared` (R5 slices 1–2).
///
/// * a user CLASS: the class plus every transitive user subclass through
///   `super_name` links (an `implements` edge does not make a runtime
///   subclass for dispatch);
/// * a user INTERFACE: the implementor closure — every user class whose
///   subtype closure reaches the interface through the user tables
///   (`interface_receiver_closure`).
///
/// `None` = the declared type is not a user class/interface, or the
/// closure is empty (no compiled class can carry the receiver — the site
/// can only ever NPE or die the internal Error, and the dynamic path
/// already does that faithfully).
pub fn virtual_receiver_set(prog: &Program, declared: &str) -> Option<Vec<String>> {
    if prog.class(declared).map(|c| c.is_interface).unwrap_or(true) {
        interface_receiver_closure(prog, declared)
    } else {
        // The declared class plus every transitive user subclass
        // (through `super_name` links only).
        let mut receivers: Vec<String> = vec![declared.to_string()];
        let mut i = 0;
        while i < receivers.len() {
            let cur = receivers[i].clone();
            i += 1;
            for c in &prog.classes {
                if c.super_name.as_deref() == Some(cur.as_str())
                    && !receivers.iter().any(|r| r == &c.name)
                {
                    receivers.push(c.name.clone());
                }
            }
        }
        Some(receivers)
    }
}

/// The closed-world receiver set for a USER-interface-typed call site
/// (R5 slice 2): every user class whose subtype closure reaches `iface`
/// through the user tables (`super_name` + `interfaces` edges,
/// transitively — the same traversal the runtime's subtype check makes
/// over the generated parents table, minus the builtin edges, which
/// provably cannot contribute: the builtin tables never name user
/// classes, so once a chain passes into builtin land it can never reach
/// a user interface).
///
/// Interfaces themselves are excluded: an interface is never a concrete
/// receiver — it cannot be instantiated, and lambdas targeting USER
/// functional interfaces are a loud compile error (the closure object's
/// class is the interface, but only the M3 builtin interface set is
/// supported — `lambda.rs`'s `SUPPORTED_INTERFACES` gate). A builtin
/// interface-typed site keeps its fully dynamic path at the
/// `!prog.is_user` gate above.
///
/// Deterministic: the classes iterate in declaration order, so the
/// emitted arm order (and the bytes) are stable across runs.
pub(crate) fn interface_receiver_closure(prog: &Program, iface: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for c in &prog.classes {
        if c.is_interface {
            continue;
        }
        // Upward BFS from c through user edges only (see the doc above).
        let mut frontier: Vec<&str> = vec![&c.name];
        let mut visited: Vec<&str> = vec![&c.name];
        let mut reaches = false;
        while let Some(cur) = frontier.pop() {
            if cur == iface {
                reaches = true;
                break;
            }
            if let Some(plan) = prog.class(cur) {
                let mut edges: Vec<&str> = Vec::new();
                if !plan.is_interface {
                    if let Some(sup) = &plan.super_name {
                        edges.push(sup);
                    }
                }
                for itf in &plan.interfaces {
                    edges.push(itf);
                }
                for e in edges {
                    if !visited.contains(&e) {
                        visited.push(e);
                        frontier.push(e);
                    }
                }
            }
        }
        if reaches {
            out.push(c.name.clone());
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// One receiver's walk, mirroring [`j2r_runtime::invoke_virtual`] exactly:
/// compiled method per level, native row per level, then the parent chain
/// (user supers with the Object default, builtins via the builtin table).
/// `None` = the walk would fall off the root — the runtime's internal
/// Error — which the fold never emulates.
pub(crate) fn walk_virtual_chain(
    prog: &Program,
    start: &str,
    name: &str,
    desc: &str,
) -> Option<CallTarget> {
    let mut cur = start.to_string();
    for _ in 0..64 {
        if prog.is_user(&cur) {
            let c = prog.class(&cur).unwrap();
            if let Some(m) = c.method(name, desc) {
                return Some(CallTarget::User {
                    module: c.module.clone(),
                    fn_name: m.fn_name.clone(),
                });
            }
        }
        // The runtime checks the natives table at EVERY level, user
        // levels included (rows are keyed by builtin names today, but the
        // order — user method first, then native — is the law, and it
        // holds even for a user class shadowing a builtin name).
        if natives::lookup_exact(&cur, name, desc).is_some() {
            return Some(CallTarget::Native { class: cur });
        }
        let parent = if prog.is_user(&cur) {
            prog.class(&cur)
                .unwrap()
                .super_name
                .clone()
                .unwrap_or_else(|| "java/lang/Object".to_string())
        } else {
            builtin::builtin_parent_of(&cur)?.to_string()
        };
        cur = parent;
    }
    None
}

/// Emitter wrapper (same rationale as [`resolve_call_pub`]).
pub fn resolve_virtual_target_pub(
    prog: &Program,
    declared: &str,
    name: &str,
    desc: &str,
) -> Option<CallTarget> {
    resolve_virtual_target(prog, declared, name, desc)
}

/// One vtable arm (R5 slice 2): every receiver class listed in `classes`
/// walks to the SAME `target`, so the emitter groups them under one match
/// pattern. `classes` keeps first-seen declaration order; `target` is
/// exactly what [`j2r_runtime::invoke_virtual`]'s walk returns for each
/// listed receiver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VtableArm {
    pub classes: Vec<String>,
    pub target: CallTarget,
}

/// The codegen-time vtable of a polymorphic virtual/interface site (R5
/// slice 2): per-receiver direct targets for the closed-world receiver
/// set, emitted as a match on the receiver's concrete class with the full
/// dynamic walk as the fallback arm. The direct fold
/// ([`resolve_virtual_target`]) stays the first choice — a vtable is
/// planned only when it would NOT fire (multiple distinct targets, or
/// walks that would die the internal Error) and at least two DISTINCT
/// groups exist (otherwise the match adds nothing over the dynamic path).
///
/// Faithfulness: every arm's target is what the runtime walk returns for
/// that receiver class; a receiver whose walk would die the Error gets NO
/// arm (the fallback re-enters the dynamic walk and dies it identically);
/// any class outside the receiver set — impossible in the closed world —
/// also falls back, byte-identically. The refresh, the `pop_ref` NPE law
/// and the `j2r_try!` wrap ride the same code for every arm.
pub fn resolve_vtable(
    prog: &Program,
    declared: &str,
    name: &str,
    desc: &str,
) -> Option<Vec<VtableArm>> {
    // The single-target fold already covers this site.
    if resolve_virtual_target(prog, declared, name, desc).is_some() {
        return None;
    }
    if !prog.is_user(declared) {
        return None;
    }
    let receivers = virtual_receiver_set(prog, declared)?;
    // Group the receivers by their walk target; first-seen order.
    let mut arms: Vec<VtableArm> = Vec::new();
    for recv in &receivers {
        let target = match walk_virtual_chain(prog, recv, name, desc) {
            Some(t) => t,
            // A receiver whose walk would die the internal Error gets no
            // arm — the emitter's fallback re-enters the dynamic walk and
            // throws the identical Error.
            None => continue,
        };
        if let Some(arm) = arms.iter_mut().find(|a| a.target == target) {
            arm.classes.push(recv.clone());
        } else {
            arms.push(VtableArm {
                classes: vec![recv.clone()],
                target,
            });
        }
    }
    // A single group adds nothing over the dynamic path (and the
    // all-receivers-unified case already returned at the fold gate
    // above): stay dynamic.
    if arms.len() < 2 {
        return None;
    }
    Some(arms)
}

/// Emitter wrapper (same rationale as [`resolve_call_pub`]).
pub fn resolve_vtable_pub(
    prog: &Program,
    declared: &str,
    name: &str,
    desc: &str,
) -> Option<Vec<VtableArm>> {
    resolve_vtable(prog, declared, name, desc)
}

/// R5 slice 2 measurement: how many virtual/interface sites carry a
/// vtable plan and how many grouped arms those vtables hold in total.
/// Observability only — the emission itself decides per site.
pub fn count_vtable_sites(prog: &Program) -> (usize, usize) {
    let mut sites = 0;
    let mut arms = 0;
    for c in &prog.classes {
        for m in &c.methods {
            for (_pc, instr) in &m.instrs {
                if let ir::Instr::InvokeVirtual { class, name, desc }
                | ir::Instr::InvokeInterface { class, name, desc } = instr
                {
                    if let Some(plan) = resolve_vtable(prog, class, name, desc) {
                        sites += 1;
                        arms += plan.len();
                    }
                }
            }
        }
    }
    (sites, arms)
}

/// R5 slice 1 measurement: over every user method's instructions, how
/// many `invokevirtual`/`invokeinterface` sites exist and how many fold
/// to a single codegen-time target. Observability only — the emission
/// itself decides per site.
pub fn count_virtual_sites(prog: &Program) -> (usize, usize) {
    let mut total = 0;
    let mut resolved = 0;
    for c in &prog.classes {
        for m in &c.methods {
            for (_pc, instr) in &m.instrs {
                if let ir::Instr::InvokeVirtual { class, name, desc }
                | ir::Instr::InvokeInterface { class, name, desc } = instr
                {
                    total += 1;
                    if resolve_virtual_target(prog, class, name, desc).is_some() {
                        resolved += 1;
                    }
                }
            }
        }
    }
    (total, resolved)
}

/// The boxed-class `TYPE` pseudo-fields: `getstatic java/lang/Integer.TYPE`
/// is how javac compiles `int.class` at literal positions (oracle javap,
/// M3_EVIDENCE.md wave-5). Returns the primitive descriptor the interned
/// `Class` object carries.
pub fn primitive_type_field(class: &str) -> Option<&'static str> {
    match class {
        "java/lang/Integer" => Some("I"),
        "java/lang/Long" => Some("J"),
        "java/lang/Float" => Some("F"),
        "java/lang/Double" => Some("D"),
        "java/lang/Byte" => Some("B"),
        "java/lang/Short" => Some("S"),
        "java/lang/Character" => Some("C"),
        "java/lang/Boolean" => Some("Z"),
        "java/lang/Void" => Some("V"),
        _ => None,
    }
}

pub fn resolve_field_pub<'a>(
    prog: &'a Program,
    start: &str,
    field: &str,
    want_static: bool,
) -> Result<&'a FieldPlan> {
    resolve_field(prog, start, field, want_static)
}
