//! Bytecode verifier — the compile-time safety net between the IR decoder
//! and the code generator.
//!
//! The decoder ([`crate::ir`]) is deliberately mechanical: it translates
//! bytes to instructions without judging them. This module judges them.
//! Before a single line of Rust is emitted, every method body is simulated
//! over an abstract operand stack and local-variable table, so that
//! *malformed* class files (hand-built, corrupted, or produced by broken
//! tools) are rejected with a **structured `CodegenError::Verify`
//! diagnostic** instead of reaching the panic paths of the generated code
//! or the runtime (`pop on empty stack`, `expected int value`, out-of-range
//! local indices, `pc out of range`, ...).
//!
//! What is checked (per method):
//!
//! * **Operand-stack shape** — underflow, and the exact category-1 /
//!   category-2 forms of `pop`/`pop2`/`dup*`/`swap` per JVMS §6.5
//!   (stricter than the runtime, which is deliberately lenient; malformed
//!   forms are compile errors here and unreachable at run time).
//! * **Stack consistency at control-flow joins** — every path reaching an
//!   instruction must agree on stack height and on the primitive/reference
//!   kind of each slot (reference identity is *not* refined further; the
//!   runtime's uniform `Value::Ref` makes that safe).
//! * **Local-variable indices** — every load/store/iinc must be in range;
//!   the high half of a category-2 local and never-assigned locals cannot
//!   be loaded; overwriting the high half poisons the category-2 local.
//! * **Branch and switch targets** — must be real instruction boundaries
//!   (no jumping into the middle of an instruction, which would desync the
//!   generated `pc` dispatch); control flow may not fall off the end of
//!   the body.
//! * **Return/instruction descriptor agreement** — `ireturn` in a `()V`
//!   method, wrong argument counts or kinds for invokes/fields, `laload`
//!   producing a `long`, `lastore` consuming one, etc.
//! * **`max_stack` / `max_locals`** — the abstract stack never exceeds the
//!   declared maximum; the declared locals cover the descriptor.
//! * **Exception tables** — `start_pc < end_pc`, all pcs are instruction
//!   boundaries, handlers start with exactly one reference on the stack
//!   (what the generated handler code expects), and `catch_type` names a
//!   class that exists (compiled, builtin, or `Object`).
//!
//! What is intentionally *not* checked (documented leniencies, safe for
//! this runtime): reference subtype precision at joins (refs merge to
//! `Ref`), array element *class* types for `[L...` arrays, and
//! uninitialized-`this` tracking — none of these can turn malformed input
//! into a panic, because the runtime's reference operations (`checkcast`,
//! `instanceof`, dispatch, field slots) are total functions over
//! `Value::Ref`.

use crate::ir::{ElemKind, PrimType};
use crate::{ClassPlan, CodegenError, MethodPlan, Program, Result};
use j2r_runtime::{parse_field_desc, parse_method_desc, TypeDesc};

/// A verification failure: always carries the faulting `pc` and a precise
/// message, surfaced as a structured compile-time diagnostic.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifyError {
    pub pc: u32,
    pub message: String,
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pc {}: {}", self.pc, self.message)
    }
}

// ---------------------------------------------------------------- lattice

/// The abstract type of one operand-stack slot / local variable.
///
/// `boolean`, `byte`, `short`, `char` and `int` are deliberately unified
/// into [`VTy::Int`]: the runtime represents them all as `Value::Int`, so
/// no generated operation can tell them apart.
/// NOT `Copy` (the recursive `VTy::Array(ElemKind)` carries the
/// `Nest(Box<ElemKind>)` heap box, same wave).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VTy {
    /// Never assigned on any path so far. Loading is an error.
    Unset,
    /// Two paths disagree on this *local's* type (or its category-2 pair
    /// was broken). Loading is an error; a store re-initializes it.
    /// Only ever found in locals, never on the stack.
    Poison,
    /// An unknown category-1 value (defensive lattice member; current IR
    /// positions are all precisely typed). Merges with any category-1.
    Top,
    Int,
    Long,
    Float,
    Double,
    Ref,
    /// An array whose element kind is statically known (from `newarray`,
    /// `anewarray` or an array descriptor). Behaves as a reference for
    /// merges and reference-expecting pops, but lets `xXaload`/`xXastore`
    /// check the element kind — so e.g. `lastore` consuming an `int` is a
    /// compile error, not a runtime type panic.
    Array(ElemKind),
    /// Definitely-null reference. Merges with `Ref` into `Ref`.
    Null,
    /// The reserved second slot of a category-2 local. Loading is an error.
    HighHalf,
}

impl VTy {
    pub fn is_cat2(&self) -> bool {
        matches!(self, VTy::Long | VTy::Double)
    }

    pub fn is_cat1(&self) -> bool {
        !self.is_cat2()
    }

    /// Merge for **stack** slots: incompatible kinds are malformed
    /// bytecode (the JVM verifier also rejects them), so they error.
    fn merge_stack(a: VTy, b: VTy) -> std::result::Result<VTy, String> {
        use VTy::*;
        match (&a, &b) {
            (x, y) if x == y => Ok(x.clone()),
            (Null, Ref) | (Ref, Null) => Ok(Ref),
            (Array(k), Array(j)) => Ok(if k == j { Array(k.clone()) } else { Ref }),
            (Array(_), Ref) | (Ref, Array(_)) => Ok(Ref),
            (Null, Array(_)) | (Array(_), Null) => Ok(Ref),
            (Top, x) | (x, Top) if x.is_cat1() => Ok(Top),
            (Unset, _) | (_, Unset) => Err("unset value on the operand stack".to_string()),
            (Poison, _) | (_, Poison) | (HighHalf, _) | (_, HighHalf) => {
                Err("invalid value on the operand stack".to_string())
            }
            _ => Err(format!("incompatible stack types {a:?} vs {b:?}")),
        }
    }

    /// Merge for **local** slots: an incompatible merge makes the local
    /// unusable (`Poison`) rather than failing — javac legitimately reuses
    /// one local for different types in disjoint live ranges, and the JVM
    /// verifier treats this the same way. Any *later* load of the poisoned
    /// local is a verification error.
    fn merge_local(a: VTy, b: VTy) -> VTy {
        use VTy::*;
        match (&a, &b) {
            (x, y) if x == y => x.clone(),
            (Unset, x) | (x, Unset) => x.clone(),
            (Null, Ref) | (Ref, Null) => Ref,
            (Top, x) | (x, Top) if x.is_cat1() && !matches!(x, HighHalf | Poison) => Top,
            _ => Poison,
        }
    }
}

/// Abstract state at one instruction: operand stack (bottom → top) and the
/// local table.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct State {
    stack: Vec<VTy>,
    locals: Vec<VTy>,
}

/// The verified type shape at one instruction (public export for the M2
/// Wave-3 specialization pass — M2_PLAN §5.1: the verifier's per-pc stack
/// and locals shapes are exactly its input).
#[derive(Clone, Debug, PartialEq)]
pub struct PcShape {
    /// Operand stack, bottom → top, as it looks when control reaches `pc`.
    pub stack: Vec<VTy>,
    /// Local-variable table at the same point.
    pub locals: Vec<VTy>,
}

/// Per-method verified shapes, keyed by instruction pc.
#[derive(Clone, Debug, Default)]
pub struct MethodShapes {
    pub per_pc: std::collections::BTreeMap<u32, PcShape>,
    pub max_locals: usize,
    pub max_stack: usize,
}

impl State {
    fn merge_into(&mut self, other: &State) -> std::result::Result<bool, String> {
        let mut changed = false;
        if self.stack.len() != other.stack.len() {
            return Err(format!(
                "inconsistent stack heights at join: {} vs {}",
                self.stack.len(),
                other.stack.len()
            ));
        }
        for (dst, src) in self.stack.iter_mut().zip(other.stack.iter()) {
            let merged = VTy::merge_stack(dst.clone(), src.clone())
                .map_err(|e| format!("incompatible types on stack at join: {e}"))?;
            if merged != *dst {
                *dst = merged;
                changed = true;
            }
        }
        for (dst, src) in self.locals.iter_mut().zip(other.locals.iter()) {
            let merged = VTy::merge_local(dst.clone(), src.clone());
            if merged != *dst {
                *dst = merged;
                changed = true;
            }
        }
        Ok(changed)
    }
}

// ---------------------------------------------------------------- helpers

fn err(pc: u32, msg: impl Into<String>) -> VerifyError {
    VerifyError {
        pc,
        message: msg.into(),
    }
}

/// Maps a `TypeDesc` (from a descriptor) to the verifier lattice.
fn td_vty(t: &TypeDesc) -> VTy {
    match t {
        TypeDesc::Int | TypeDesc::Boolean | TypeDesc::Byte | TypeDesc::Short | TypeDesc::Char => {
            VTy::Int
        }
        TypeDesc::Long => VTy::Long,
        TypeDesc::Float => VTy::Float,
        TypeDesc::Double => VTy::Double,
        TypeDesc::Object(_) => VTy::Ref,
        TypeDesc::Array(desc) => VTy::Array(array_elem_kind(desc)),
        TypeDesc::Void => VTy::Top, // returns are handled separately
    }
}

/// Element kind of an array descriptor — the RECURSIVE DESCRIPTOR
/// (2026-11-12 closure wave): `[I` → IntGroup, `[[I` → the elements are
/// `int[]` values (`Nest(IntGroup)`), `[[[I` → `Nest(Nest(IntGroup))`,
/// and EVERY depth folds one more level with zero new variants. The old
/// per-depth enumeration (RefPrimArray{,2,3} × 4 prims) and the depth >= 5
/// loud collapse are RETIRED (corpus/DeepArraysProbe.java pinned the 3D
/// shape that forced the first levels; the runtime's shape dispatch was
/// always depth-agnostic — the lattice was the only limit). `[L...;` is
/// Ref at every depth.
fn array_elem_kind(desc: &str) -> ElemKind {
    let depth = desc.len() - desc.trim_start_matches('[').len();
    if depth == 0 {
        return ElemKind::Ref;
    }
    let bottom = &desc[depth..];
    match bottom.chars().next() {
        _ if depth == 1 => match bottom.chars().next() {
            Some('J') => ElemKind::Long,
            Some('F') => ElemKind::Float,
            Some('D') => ElemKind::Double,
            Some('Z') | Some('B') | Some('C') | Some('S') | Some('I') => ElemKind::IntGroup,
            _ => ElemKind::Ref,
        },
        // depth >= 2: the elements are (depth-1)-shaped array values —
        // fold one `[` per level, any depth.
        Some('J') | Some('F') | Some('D') | Some('Z') | Some('B') | Some('C') | Some('S')
        | Some('I') => ElemKind::Nest(Box::new(array_elem_kind(&desc[1..]))),
        _ => ElemKind::Ref,
    }
}

/// The `kind` char of a resolved `invokedynamic` concat site → lattice type.
fn kind_vty(k: char) -> VTy {
    match k {
        'J' => VTy::Long,
        'F' => VTy::Float,
        'D' => VTy::Double,
        'L' => VTy::Ref,
        // I B S C Z all render as Int in the runtime
        _ => VTy::Int,
    }
}

fn elem_vty(k: ElemKind) -> VTy {
    match k {
        ElemKind::IntGroup => VTy::Int,
        ElemKind::Long => VTy::Long,
        ElemKind::Float => VTy::Float,
        ElemKind::Double => VTy::Double,
        ElemKind::Ref => VTy::Ref,
        // The recursive descriptor: a nest's elements are arrays of the
        // folded kind — ONE arm, any depth (deep-arrays + depth-4 waves'
        // per-level arms replaced).
        ElemKind::Nest(inner) => VTy::Array(*inner),
    }
}

/// The element kind of a `newarray` primitive-array type.
fn prim_elem_kind(p: PrimType) -> ElemKind {
    match p {
        PrimType::Long => ElemKind::Long,
        PrimType::Float => ElemKind::Float,
        PrimType::Double => ElemKind::Double,
        _ => ElemKind::IntGroup,
    }
}

/// The verified type of a G3.3 map-rewrite prim operand. Only the four
/// boxable prims reach the synthetic instructions (the peephole matches
/// the four `valueOf`/`xValue()` pairs); the int-group prims are named
/// for exhaustiveness and never verified here.
fn boxed_prim_vty(p: PrimType) -> VTy {
    match p {
        PrimType::Long => VTy::Long,
        PrimType::Float => VTy::Float,
        PrimType::Double => VTy::Double,
        _ => VTy::Int,
    }
}

/// Entry state: locals initialized from the descriptor (instance methods
/// reserve slot 0 for `this`), stack empty.
fn entry_state(m: &MethodPlan) -> std::result::Result<State, VerifyError> {
    let instance = !m.is_static;
    let locals_len = m.max_locals.max(if instance { 1 } else { 0 });
    let mut locals = vec![VTy::Unset; locals_len];
    let mut next = 0usize;
    if instance {
        locals[0] = VTy::Ref;
        next = 1;
    }
    let (params, _ret) = parse_method_desc(&m.desc)
        .ok_or_else(|| err(0, format!("unparseable method descriptor {:?}", m.desc)))?;
    for p in &params {
        let v = td_vty(p);
        if v.is_cat2() {
            if next + 2 > locals_len {
                return Err(err(
                    0,
                    format!(
                        "max_locals ({}) is too small for category-2 parameter of {}",
                        m.max_locals, m.desc
                    ),
                ));
            }
            locals[next] = v;
            locals[next + 1] = VTy::HighHalf;
            next += 2;
        } else {
            if next + 1 > locals_len {
                return Err(err(
                    0,
                    format!(
                        "max_locals ({}) is too small for descriptor {} (needs {})",
                        m.max_locals,
                        m.desc,
                        next + 1
                    ),
                ));
            }
            locals[next] = v;
            next += 1;
        }
    }
    Ok(State {
        stack: Vec::new(),
        locals,
    })
}

/// Verifies one method body (shape simulation + fixpoint over joins) and
/// returns the verified per-pc shapes for the specialization pass.
fn field_vty(desc: &str) -> std::result::Result<VTy, String> {
    let (t, _width) =
        parse_field_desc(desc).ok_or_else(|| format!("unparseable field descriptor {desc:?}"))?;
    Ok(td_vty(&t))
}

fn push_ret(sim: &mut Sim<'_>, ret: &TypeDesc) -> std::result::Result<(), VerifyError> {
    match ret {
        TypeDesc::Void => {}
        t => sim.push(td_vty(t))?,
    }
    Ok(())
}

/// The pc of the instruction that follows `pc` in code order (fallthrough);
/// `u32::MAX` when `pc` is the last instruction.
fn next_pc(m: &MethodPlan, pc: u32) -> u32 {
    for (i, (p, _)) in m.instrs.iter().enumerate() {
        if *p == pc {
            return m.instrs.get(i + 1).map(|(np, _)| *np).unwrap_or(u32::MAX);
        }
    }
    u32::MAX
}

// ------------------------------------------------------------ class-level

/// Verifies every method of `c`, including exception-table semantics that
/// need the whole-program view (`catch_type` must name a real class).
pub fn verify_class(prog: &Program, c: &ClassPlan) -> Result<()> {
    for m in &c.methods {
        verify_method(m, &c.concats, &c.lambdas).map_err(|e| CodegenError::Verify {
            class: c.name.clone(),
            method: format!("{}{}", m.name, m.desc),
            pc: e.pc,
            message: e.message,
        })?;
        // catch types must resolve somewhere (compiled, builtin, or
        // Object/Throwable); a bogus type would silently never match
        for (start, _end, _handler, catch) in &m.exception_table {
            if let Some(cls) = catch {
                if !known_class(prog, cls) {
                    return Err(CodegenError::Verify {
                        class: c.name.clone(),
                        method: format!("{}{}", m.name, m.desc),
                        pc: *start,
                        message: format!("exception catch type {cls} does not name a known class"),
                    });
                }
            }
        }
    }
    Ok(())
}

/// True when `name` is a class the runtime knows: compiled from the inputs,
/// a builtin JDK class, or one of the roots `java/lang/Object` /
/// `java/lang/Throwable`.
fn known_class(prog: &Program, name: &str) -> bool {
    if name == "java/lang/Object" || name == "java/lang/Throwable" {
        return true;
    }
    if prog.is_user(name) {
        return true;
    }
    // walk the builtin chain: any class that eventually reaches a root
    let mut cur = name.to_string();
    for _ in 0..16 {
        match j2r_runtime::builtin::builtin_parent_of(&cur) {
            Some(p) => {
                if p == "java/lang/Object" || p == "java/lang/Throwable" {
                    return true;
                }
                cur = p.to_string();
            }
            None => return false,
        }
    }
    false
}

// ---------------------------------------------------------------- tests

// --- round-7 file-split pass: verbatim children below ---
mod sim;
pub use sim::verify_method;
pub use sim::verify_method_shapes;
pub(crate) use sim::*;
#[cfg(test)]
mod tests_a;
#[cfg(test)]
mod tests_b;
