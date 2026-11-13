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

use crate::ir::{ArithOp, CmpKind, Conv, ElemKind, Instr, Ldc2, NumKind, PrimType};
use crate::{ClassPlan, CodegenError, ConcatPlan, MethodPlan, Program, Result};
use j2r_runtime::{parse_field_desc, parse_method_desc, TypeDesc};
use std::collections::{BTreeMap, HashMap, VecDeque};

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
struct State {
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
pub fn verify_method_shapes(
    m: &MethodPlan,
    concats: &BTreeMap<u16, ConcatPlan>,
    lambdas: &BTreeMap<u16, crate::lambda::LambdaPlan>,
) -> std::result::Result<MethodShapes, VerifyError> {
    if m.is_native {
        // T12 12c (docs/M5_JNI_NATIVES.md §4): a native method has no
        // bytecode to verify — its body is the emitted call-through to
        // the binding registry, correct by construction. There is no
        // abstract/native distinction here: abstract methods never
        // reach a MethodPlan.
        return Ok(MethodShapes::default());
    }
    if m.instrs.is_empty() {
        return Err(err(0, "method body has no instructions"));
    }
    // instruction boundary table
    let mut at: HashMap<u32, usize> = HashMap::with_capacity(m.instrs.len());
    for (i, (pc, _)) in m.instrs.iter().enumerate() {
        at.insert(*pc, i);
    }
    let boundary = |pc: u32, what: &str| -> std::result::Result<usize, VerifyError> {
        at.get(&pc)
            .copied()
            .ok_or_else(|| err(pc, format!("{what} {pc} is not an instruction boundary")))
    };

    // worklist fixpoint
    let mut states: Vec<Option<State>> = vec![None; m.instrs.len()];
    let mut queue: VecDeque<usize> = VecDeque::new();
    states[0] = Some(entry_state(m)?);
    queue.push_back(0);

    while let Some(idx) = queue.pop_front() {
        let Some(state) = states[idx].clone() else {
            continue;
        };
        let (pc, instr) = &m.instrs[idx];
        let pc = *pc;
        let mut sim = Sim {
            m,
            pc,
            stack: state.stack,
            locals: state.locals,
            concats,
            lambdas,
            max_stack: m.max_stack,
        };
        let successors = sim.execute(instr)?;
        // exception edges: every covered instruction can jump to any handler
        for (start, end, handler, _) in &m.exception_table {
            if *start <= pc && pc < *end {
                let h = at.get(handler).copied().ok_or_else(|| {
                    err(
                        *handler,
                        format!("exception handler_pc {handler} is not an instruction boundary"),
                    )
                })?;
                let hstate = State {
                    stack: vec![VTy::Ref],
                    locals: sim.locals.clone(),
                };
                merge_state(&mut states, &mut queue, h, *handler, &hstate, pc)?;
            }
        }
        for (target, st) in successors {
            let t = boundary(target, "branch target")?;
            merge_state(&mut states, &mut queue, t, target, &st, pc)?;
        }
    }

    // Export the verified input state of every reached instruction.
    let mut per_pc = std::collections::BTreeMap::new();
    for (idx, st) in states.iter().enumerate() {
        if let Some(st) = st {
            per_pc.insert(
                m.instrs[idx].0,
                PcShape {
                    stack: st.stack.clone(),
                    locals: st.locals.clone(),
                },
            );
        }
    }
    Ok(MethodShapes {
        per_pc,
        max_locals: m.max_locals,
        max_stack: m.max_stack,
    })
}

/// Verifies one method body (shape simulation + fixpoint over joins).
pub fn verify_method(
    m: &MethodPlan,
    concats: &BTreeMap<u16, ConcatPlan>,
    lambdas: &BTreeMap<u16, crate::lambda::LambdaPlan>,
) -> std::result::Result<(), VerifyError> {
    verify_method_shapes(m, concats, lambdas).map(|_| ())
}

/// Merges `st` into the recorded state of instruction slot `t` (whose pc is
/// `t_pc`, for diagnostics); queues `t` when its state changed.
fn merge_state(
    states: &mut [Option<State>],
    queue: &mut VecDeque<usize>,
    t: usize,
    t_pc: u32,
    st: &State,
    from_pc: u32,
) -> std::result::Result<(), VerifyError> {
    match &mut states[t] {
        None => {
            states[t] = Some(st.clone());
            queue.push_back(t);
        }
        Some(existing) => {
            let changed = existing
                .merge_into(st)
                .map_err(|msg| err(from_pc, format!("{msg} (edge into pc {t_pc})")))?;
            if changed && !queue.contains(&t) {
                queue.push_back(t);
            }
        }
    }
    Ok(())
}

struct Sim<'a> {
    m: &'a MethodPlan,
    pc: u32,
    stack: Vec<VTy>,
    locals: Vec<VTy>,
    concats: &'a BTreeMap<u16, ConcatPlan>,
    lambdas: &'a BTreeMap<u16, crate::lambda::LambdaPlan>,
    max_stack: usize,
}

impl<'a> Sim<'a> {
    fn e(&self, msg: impl Into<String>) -> VerifyError {
        err(self.pc, msg)
    }

    fn pop(&mut self) -> std::result::Result<VTy, VerifyError> {
        self.stack
            .pop()
            .ok_or_else(|| self.e("operand stack underflow"))
    }

    /// Pops one value that must be reference-shaped (`Ref`/`Null`/`Top`,
    /// or a typed array, which is itself a reference).
    fn pop_ref(&mut self) -> std::result::Result<VTy, VerifyError> {
        let v = self.pop()?;
        match v {
            VTy::Ref | VTy::Null | VTy::Top | VTy::Array(_) => Ok(VTy::Ref),
            other => Err(self.e(format!(
                "expected a reference on the stack, found {other:?}"
            ))),
        }
    }

    /// Pops one value that must be exactly `want` (`Top` is accepted for
    /// any category-1 `want`; `Null` is accepted for every reference shape
    /// — `Ref` *and* typed arrays, because `null` is assignable to any
    /// array type, corpus F2 — and typed arrays for `Ref`; arrays-of-
    /// references are accepted for `Array(Ref)`, JLS 4.10.3 covariance:
    /// `[[D <: [Object` because `[D <: Object`, so a `double[][]` may be
    /// passed where an `Object[]` parameter is declared — JAMA wave 3,
    /// `Vector.copyInto(Object[])` on `double[][]`).
    fn pop_exact(&mut self, want: VTy) -> std::result::Result<VTy, VerifyError> {
        let v = self.pop()?;
        let ok = v == want
            || (v == VTy::Top && want.is_cat1())
            || (v == VTy::Null && matches!(want, VTy::Ref | VTy::Array(_)))
            || (matches!(v, VTy::Array(_)) && want == VTy::Ref)
            || (matches!(&v, VTy::Array(k) if k.elements_are_refs())
                && want == VTy::Array(ElemKind::Ref));
        if ok {
            Ok(want)
        } else {
            Err(self.e(format!("expected {want:?} on the stack, found {v:?}")))
        }
    }

    /// Pops the array operand of `xXaload`/`xXastore`: any reference works,
    /// but a *typed* array must carry the matching element kind.
    fn pop_array(&mut self, k: ElemKind) -> std::result::Result<(), VerifyError> {
        let v = self.pop()?;
        match v {
            // A primitive load/store demands its exact element kind (a long
            // load off an int[] is malformed). A reference load (aaload) is
            // fine on ANY ref-element array — including nested primitive
            // arrays (`[[I` elements are `int[]` references).
            VTy::Array(j) if j != k && !matches!(k, ElemKind::Ref) => Err(self.e(format!(
                "array element kind mismatch: operation expects {k:?}, array is {j:?}"
            ))),
            VTy::Ref | VTy::Null | VTy::Top | VTy::Array(_) => Ok(()),
            other => Err(self.e(format!(
                "expected an array reference on the stack, found {other:?}"
            ))),
        }
    }

    fn push(&mut self, v: VTy) -> std::result::Result<(), VerifyError> {
        self.stack.push(v);
        self.check_max()
    }

    fn check_max(&self) -> std::result::Result<(), VerifyError> {
        if self.stack.len() > self.max_stack {
            return Err(self.e(format!(
                "operand stack overflow: {} entries exceed max_stack {}",
                self.stack.len(),
                self.max_stack
            )));
        }
        Ok(())
    }

    fn check_local(&self, idx: u16) -> std::result::Result<usize, VerifyError> {
        let idx = idx as usize;
        if idx >= self.locals.len() {
            return Err(self.e(format!(
                "local variable index {idx} out of range (max_locals {})",
                self.locals.len()
            )));
        }
        Ok(idx)
    }

    fn load(&mut self, idx: u16) -> std::result::Result<(), VerifyError> {
        let i = self.check_local(idx)?;
        match self.locals[i].clone() {
            VTy::Unset => Err(self.e(format!("load of never-assigned local {i}"))),
            VTy::Poison => Err(self.e(format!("load of local {i} with conflicting types"))),
            VTy::HighHalf => Err(self.e(format!(
                "load of the reserved high half of category-2 local {}",
                i.saturating_sub(1)
            ))),
            v => self.push(v),
        }
    }

    fn store(&mut self, idx: u16) -> std::result::Result<(), VerifyError> {
        let i = self.check_local(idx)?;
        let v = self.pop()?;
        if v.is_cat2() {
            if i + 1 >= self.locals.len() {
                return Err(self.e(format!(
                    "category-2 store at local {i} has no high-half slot (max_locals {})",
                    self.locals.len()
                )));
            }
            self.locals[i] = v;
            self.locals[i + 1] = VTy::HighHalf;
        } else {
            // a cat-1 store into the high half breaks the cat-2 local below
            if i > 0 && self.locals[i - 1].is_cat2() {
                self.locals[i - 1] = VTy::Poison;
            }
            self.locals[i] = v;
        }
        Ok(())
    }

    // -------------------------------------------------- the dup family,
    // mirroring JVMS §6.5 exactly (stricter than the runtime, which is
    // lenient on unreachable forms)

    fn pop_cat1(&mut self, what: &str) -> std::result::Result<VTy, VerifyError> {
        let v = self.pop()?;
        if v.is_cat2() {
            return Err(self.e(format!("{what} requires category-1 values")));
        }
        Ok(v)
    }

    fn dup_family(&mut self, instr: &Instr) -> std::result::Result<(), VerifyError> {
        match instr {
            Instr::Pop => {
                self.pop_cat1("pop")?;
            }
            Instr::Pop2 => {
                let v = self.pop()?;
                if !v.is_cat2() {
                    self.pop_cat1("pop2")?;
                }
            }
            Instr::Dup => {
                let v = self.pop_cat1("dup")?;
                self.push(v.clone())?;
                self.push(v.clone())?;
            }
            Instr::DupX1 => {
                let v1 = self.pop_cat1("dup_x1")?;
                let v2 = self.pop_cat1("dup_x1")?;
                self.push(v1.clone())?;
                self.push(v2.clone())?;
                self.push(v1.clone())?;
            }
            Instr::DupX2 => {
                let v1 = self.pop_cat1("dup_x2")?;
                let v2 = self.pop()?;
                if v2.is_cat2() {
                    // Form 2: v2(cat2), v1 -> v1, v2, v1
                    self.push(v1.clone())?;
                    self.push(v2.clone())?;
                    self.push(v1.clone())?;
                } else {
                    // Form 1: v3, v2, v1 (all cat1) -> v1, v3, v2, v1
                    let v3 = self.pop_cat1("dup_x2")?;
                    self.push(v1.clone())?;
                    self.push(v3.clone())?;
                    self.push(v2.clone())?;
                    self.push(v1.clone())?;
                }
            }
            Instr::Dup2 => {
                let v1 = self.pop()?;
                if v1.is_cat2() {
                    // Form 2: v1 -> v1, v1
                    self.push(v1.clone())?;
                    self.push(v1.clone())?;
                } else {
                    // Form 1: v2, v1 -> v2, v1, v2, v1
                    let v2 = self.pop_cat1("dup2")?;
                    self.push(v2.clone())?;
                    self.push(v1.clone())?;
                    self.push(v2.clone())?;
                    self.push(v1.clone())?;
                }
            }
            Instr::Dup2X1 => {
                let v1 = self.pop()?;
                if v1.is_cat2() {
                    // Form 2: v2(cat1), v1(cat2) -> v1, v2, v1
                    let v2 = self.pop_cat1("dup2_x1")?;
                    self.push(v1.clone())?;
                    self.push(v2.clone())?;
                    self.push(v1.clone())?;
                } else {
                    // Form 1: v3, v2, v1 (all cat1) -> v2, v1, v3, v2, v1
                    let v2 = self.pop_cat1("dup2_x1")?;
                    let v3 = self.pop_cat1("dup2_x1")?;
                    self.push(v2.clone())?;
                    self.push(v1.clone())?;
                    self.push(v3.clone())?;
                    self.push(v2.clone())?;
                    self.push(v1.clone())?;
                }
            }
            Instr::Dup2X2 => {
                // Forms follow the runtime's stack.rs table (and the JVMS):
                // each of the top two two-slot units (U = the duplicated
                // unit on top, X = the unit the copy is inserted below) is
                // either one category-2 value or two category-1 values.
                //   F1 U=c1,c1  X=c1,c1: v4,v3,v2,v1 -> v2,v1,v4,v3,v2,v1
                //   F2 U=cat2   X=c1,c1: v3,v2,v1 -> v1,v3,v2,v1
                //     (JAMA wave 3: `LUrowi[j] = LUcolj[i] -= s;` — javac's
                //     compound double assignment ends cat2-on-top)
                //   F3 U=c1,c1  X=cat2:  v3,v2,v1 -> v2,v1,v3,v2,v1
                //   F4 U=cat2   X=cat2:  v2,v1 -> v1,v2,v1
                let v1 = self.pop()?;
                let v2 = self.pop()?;
                match (v1.is_cat2(), v2.is_cat2()) {
                    (true, true) => {
                        // Form 4
                        self.push(v1.clone())?;
                        self.push(v2.clone())?;
                        self.push(v1.clone())?;
                    }
                    (true, false) => {
                        // Form 2
                        let v3 = self.pop_cat1("dup2_x2")?;
                        self.push(v1.clone())?;
                        self.push(v3.clone())?;
                        self.push(v2.clone())?;
                        self.push(v1.clone())?;
                    }
                    (false, true) => {
                        return Err(self.e(
                            "dup2_x2: no JVMS form has a category-1 value on top of a category-2 value",
                        ));
                    }
                    (false, false) => {
                        let v3 = self.pop()?;
                        if v3.is_cat2() {
                            // Form 3
                            self.push(v2.clone())?;
                            self.push(v1.clone())?;
                            self.push(v3.clone())?;
                            self.push(v2.clone())?;
                            self.push(v1.clone())?;
                        } else {
                            // Form 1
                            let v4 = self.pop_cat1("dup2_x2")?;
                            self.push(v2.clone())?;
                            self.push(v1.clone())?;
                            self.push(v4.clone())?;
                            self.push(v3.clone())?;
                            self.push(v2.clone())?;
                            self.push(v1.clone())?;
                        }
                    }
                }
            }
            Instr::Swap => {
                let v1 = self.pop_cat1("swap")?;
                let v2 = self.pop_cat1("swap")?;
                self.push(v1.clone())?;
                self.push(v2.clone())?;
            }
            _ => unreachable!("not a dup-family instruction"),
        }
        Ok(())
    }

    /// Branch successors helper: fallthrough (optional) plus the target.
    fn branch_out(
        &mut self,
        target: u32,
        fallthrough: bool,
    ) -> std::result::Result<Vec<(u32, State)>, VerifyError> {
        let mut out = Vec::with_capacity(2);
        if fallthrough {
            let np = next_pc(self.m, self.pc);
            if np == u32::MAX {
                return Err(self.e("control flow falls off the end of the method body"));
            }
            out.push((np, self.snapshot()));
        }
        out.push((target, self.snapshot()));
        Ok(out)
    }

    /// Executes one instruction against the abstract state. Returns the
    /// control successors as `(target_pc, entry_state)` pairs (empty for
    /// `return`/`athrow`).
    fn execute(&mut self, instr: &Instr) -> std::result::Result<Vec<(u32, State)>, VerifyError> {
        use Instr::*;
        use NumKind as NK;
        match instr {
            Nop => {}
            AConstNull => self.push(VTy::Null)?,
            IConst(_) => self.push(VTy::Int)?,
            LConst(_) => self.push(VTy::Long)?,
            FConst(_) => self.push(VTy::Float)?,
            DConst(_) => self.push(VTy::Double)?,
            Ldc(c) => match c {
                crate::ir::Ldc::Int(_) => self.push(VTy::Int)?,
                crate::ir::Ldc::Float(_) => self.push(VTy::Float)?,
                crate::ir::Ldc::String(_) | crate::ir::Ldc::Class(_) => self.push(VTy::Ref)?,
            },
            Ldc2W(c) => match c {
                Ldc2::Long(_) => self.push(VTy::Long)?,
                Ldc2::Double(_) => self.push(VTy::Double)?,
            },
            Load(idx) => self.load(*idx)?,
            Store(idx) => self.store(*idx)?,
            IInc { idx, .. } => {
                let i = self.check_local(*idx)?;
                match self.locals[i].clone() {
                    VTy::Int | VTy::Top => {}
                    VTy::Unset => return Err(self.e(format!("iinc of never-assigned local {i}"))),
                    other => {
                        return Err(self.e(format!(
                            "iinc requires an int local, local {i} is {other:?}"
                        )))
                    }
                }
            }
            ALoad(k) => {
                let k = k.clone();
                self.pop_exact(VTy::Int)?;
                let v = self.pop()?;
                match &v {
                    VTy::Array(j) => {
                        // A primitive opcode demands its exact kind (a long
                        // load off an int[] is malformed). `aaload` takes
                        // the RECEIVER's static element shape when the
                        // array is typed — `[[I` yields `[I`, exactly like
                        // the JVM verifier (corpus wave 2, JZlib
                        // InfBlocks.proc feeding InfCodes.init's `[I`).
                        if !matches!(k, ElemKind::Ref) && *j != k {
                            return Err(self.e(format!(
                                "array element kind mismatch: operation expects {k:?}, array is {j:?}"
                            )));
                        }
                        self.push(elem_vty(j.clone()))?;
                    }
                    VTy::Ref | VTy::Null | VTy::Top => self.push(elem_vty(k))?,
                    other => {
                        return Err(self.e(format!(
                            "expected an array reference on the stack, found {other:?}"
                        )))
                    }
                }
            }
            AStore(k) => {
                self.pop_exact(elem_vty(k.clone()))?;
                self.pop_exact(VTy::Int)?;
                self.pop_array(k.clone())?;
            }
            ArrayLength => {
                self.pop_ref()?;
                self.push(VTy::Int)?;
            }
            Pop | Pop2 | Dup | DupX1 | DupX2 | Dup2 | Dup2X1 | Dup2X2 | Swap => {
                self.dup_family(instr)?;
            }
            Arith { op, kind } => {
                use ArithOp::*;
                match kind {
                    NK::Int => match op {
                        Neg => {
                            self.pop_exact(VTy::Int)?;
                            self.push(VTy::Int)?;
                        }
                        Shl | Shr | UShr => {
                            self.pop_exact(VTy::Int)?; // shift amount
                            self.pop_exact(VTy::Int)?;
                            self.push(VTy::Int)?;
                        }
                        _ => {
                            self.pop_exact(VTy::Int)?;
                            self.pop_exact(VTy::Int)?;
                            self.push(VTy::Int)?;
                        }
                    },
                    NK::Long => match op {
                        Neg => {
                            self.pop_exact(VTy::Long)?;
                            self.push(VTy::Long)?;
                        }
                        Shl | Shr | UShr => {
                            self.pop_exact(VTy::Int)?; // shift amount is int
                            self.pop_exact(VTy::Long)?;
                            self.push(VTy::Long)?;
                        }
                        _ => {
                            self.pop_exact(VTy::Long)?;
                            self.pop_exact(VTy::Long)?;
                            self.push(VTy::Long)?;
                        }
                    },
                    NK::Float => match op {
                        Neg => {
                            self.pop_exact(VTy::Float)?;
                            self.push(VTy::Float)?;
                        }
                        _ => {
                            self.pop_exact(VTy::Float)?;
                            self.pop_exact(VTy::Float)?;
                            self.push(VTy::Float)?;
                        }
                    },
                    NK::Double => match op {
                        Neg => {
                            self.pop_exact(VTy::Double)?;
                            self.push(VTy::Double)?;
                        }
                        _ => {
                            self.pop_exact(VTy::Double)?;
                            self.pop_exact(VTy::Double)?;
                            self.push(VTy::Double)?;
                        }
                    },
                    // the decoder never produces these for Arith (i2b/i2c/i2s
                    // are Convert); defensive rejection
                    NK::Byte | NK::Char | NK::Short => {
                        return Err(self.e("arithmetic on byte/char/short kind"))
                    }
                }
            }
            Convert(Conv { from, to }) => {
                let f = match from {
                    NK::Int | NK::Byte | NK::Char | NK::Short => VTy::Int,
                    NK::Long => VTy::Long,
                    NK::Float => VTy::Float,
                    NK::Double => VTy::Double,
                };
                self.pop_exact(f)?;
                let t = match to {
                    NK::Int | NK::Byte | NK::Char | NK::Short => VTy::Int,
                    NK::Long => VTy::Long,
                    NK::Float => VTy::Float,
                    NK::Double => VTy::Double,
                };
                self.push(t)?;
            }
            Cmp(k) => match k {
                CmpKind::LCmp => {
                    self.pop_exact(VTy::Long)?;
                    self.pop_exact(VTy::Long)?;
                    self.push(VTy::Int)?;
                }
                CmpKind::FCmpL | CmpKind::FCmpG => {
                    self.pop_exact(VTy::Float)?;
                    self.pop_exact(VTy::Float)?;
                    self.push(VTy::Int)?;
                }
                CmpKind::DCmpL | CmpKind::DCmpG => {
                    self.pop_exact(VTy::Double)?;
                    self.pop_exact(VTy::Double)?;
                    self.push(VTy::Int)?;
                }
            },
            IfZero(_, t) => {
                self.pop_exact(VTy::Int)?;
                return self.branch_out(*t, true);
            }
            IfICmp(_, t) => {
                self.pop_exact(VTy::Int)?;
                self.pop_exact(VTy::Int)?;
                return self.branch_out(*t, true);
            }
            IfRefCmp { target: t, .. } => {
                self.pop_ref()?;
                self.pop_ref()?;
                return self.branch_out(*t, true);
            }
            IfNull(t) | IfNonNull(t) => {
                self.pop_ref()?;
                return self.branch_out(*t, true);
            }
            Goto(t) => return self.branch_out(*t, false),
            TableSwitch {
                default, targets, ..
            } => {
                self.pop_exact(VTy::Int)?;
                let snap = self.snapshot();
                let mut out = Vec::with_capacity(targets.len() + 1);
                out.push((*default, snap.clone()));
                for t in targets {
                    out.push((*t, snap.clone()));
                }
                return Ok(out);
            }
            LookupSwitch { default, pairs } => {
                self.pop_exact(VTy::Int)?;
                let snap = self.snapshot();
                let mut out = Vec::with_capacity(pairs.len() + 1);
                out.push((*default, snap.clone()));
                for (_, t) in pairs {
                    out.push((*t, snap.clone()));
                }
                return Ok(out);
            }
            Return(t) => {
                let (_params, ret) = parse_method_desc(&self.m.desc)
                    .ok_or_else(|| self.e(format!("bad descriptor {:?}", self.m.desc)))?;
                match (t, &ret) {
                    (TypeDesc::Void, TypeDesc::Void) => {}
                    (TypeDesc::Void, _) | (_, TypeDesc::Void) => {
                        return Err(self.e(format!(
                            "{} in a method declared -> {}",
                            if *t == TypeDesc::Void {
                                "void return"
                            } else {
                                "value return"
                            },
                            self.m.desc
                        )))
                    }
                    (TypeDesc::Long, TypeDesc::Long)
                    | (TypeDesc::Float, TypeDesc::Float)
                    | (TypeDesc::Double, TypeDesc::Double) => {
                        self.pop_exact(td_vty(t))?;
                    }
                    (TypeDesc::Object(_), TypeDesc::Object(_))
                    | (TypeDesc::Object(_), TypeDesc::Array(_))
                    | (TypeDesc::Array(_), _) => {
                        self.pop_ref()?;
                    }
                    (TypeDesc::Int, _)
                    | (TypeDesc::Boolean, _)
                    | (TypeDesc::Byte, _)
                    | (TypeDesc::Short, _)
                    | (TypeDesc::Char, _) => {
                        self.pop_exact(VTy::Int)?;
                    }
                    _ => {
                        return Err(self.e(format!(
                            "return kind {t:?} does not match descriptor {}",
                            self.m.desc
                        )))
                    }
                }
                return Ok(Vec::new());
            }
            GetStatic { desc, .. } => {
                let t = field_vty(desc).map_err(|m| self.e(m))?;
                self.push(t)?;
            }
            PutStatic { desc, .. } => {
                let t = field_vty(desc).map_err(|m| self.e(m))?;
                self.pop_exact(t)?;
            }
            GetField { desc, .. } => {
                let t = field_vty(desc).map_err(|m| self.e(m))?;
                self.pop_ref()?;
                self.push(t)?;
            }
            PutField { desc, .. } => {
                let t = field_vty(desc).map_err(|m| self.e(m))?;
                self.pop_exact(t)?;
                self.pop_ref()?;
            }
            InvokeVirtual { desc, .. }
            | InvokeSpecial { desc, .. }
            | InvokeInterface { desc, .. } => {
                let (params, ret) = parse_method_desc(desc)
                    .ok_or_else(|| self.e(format!("bad method descriptor {desc:?}")))?;
                for p in params.iter().rev() {
                    self.pop_exact(td_vty(p))?;
                }
                self.pop_ref()?;
                push_ret(self, &ret)?;
            }
            InvokeStatic { desc, .. } => {
                let (params, ret) = parse_method_desc(desc)
                    .ok_or_else(|| self.e(format!("bad method descriptor {desc:?}")))?;
                for p in params.iter().rev() {
                    self.pop_exact(td_vty(p))?;
                }
                push_ret(self, &ret)?;
            }
            InvokeDynamic { index } => {
                if let Some(plan) = self.concats.get(index) {
                    for k in plan.kinds.chars().rev() {
                        self.pop_exact(kind_vty(k))?;
                    }
                    self.push(VTy::Ref)?;
                } else if let Some(lp) = self.lambdas.get(index) {
                    for k in lp.capture_kinds.chars().rev() {
                        self.pop_exact(kind_vty(k))?;
                    }
                    self.push(VTy::Ref)?;
                } else {
                    return Err(self.e(format!("invokedynamic #{index} has no resolved plan")));
                }
            }
            New { .. } => self.push(VTy::Ref)?,
            NewArray(p) => {
                self.pop_exact(VTy::Int)?;
                // primitive arrays keep their precise kind so e.g. a long
                // loaded from a long[] stays category-2-correct
                self.push(VTy::Array(prim_elem_kind(*p)))?;
            }
            // G2.6 arm a: the collapsed fill pops nothing and pushes one
            // int-group array ref — the same verified shape as NewArray.
            NewIntArrayFilled { prim, .. } => self.push(VTy::Array(prim_elem_kind(*prim)))?,
            ANewArray { elem } => {
                self.pop_exact(VTy::Int)?;
                // The operand is the ELEMENT class, which may itself be an
                // array type: javac emits `anewarray "[I"` for
                // `new int[n][]`, producing a `[[I` whose aaload yields
                // `[I` — so the result's element kind follows the operand
                // (`[<elem>` descriptor), not a generic Ref (corpus wave 2,
                // JZlib InfBlocks).
                if elem.starts_with('[') {
                    self.push(VTy::Array(array_elem_kind(&format!("[{elem}"))))?;
                } else {
                    self.push(VTy::Array(ElemKind::Ref))?;
                }
            }
            MultiNewArray { desc, dims } => {
                if *dims == 0 {
                    return Err(self.e("multianewarray with zero dimensions"));
                }
                if !desc.starts_with('[') {
                    return Err(self.e(format!(
                        "multianewarray descriptor {desc} is not an array class"
                    )));
                }
                let depth = desc.chars().take_while(|&c| c == '[').count();
                if *dims as usize > depth {
                    return Err(self.e(format!(
                        "multianewarray descriptor {desc} has {depth} array levels but {dims} dimension sizes are consumed"
                    )));
                }
                for _ in 0..*dims {
                    self.pop_exact(VTy::Int)?;
                }
                if *dims as usize == depth {
                    // Fully allocated: the element kind comes from the
                    // descriptor — a primitive keeps its precise kind, a
                    // nested primitive array keeps its typed shape, class
                    // and sub-array components are refs.
                    self.push(VTy::Array(array_elem_kind(desc)))?;
                } else {
                    // Partially allocated: elements are null or sub-arrays,
                    // same element kind by the descriptor.
                    self.push(VTy::Array(array_elem_kind(desc)))?;
                }
            }
            AThrow => {
                self.pop_ref()?;
                return Ok(Vec::new());
            }
            MonitorEnter | MonitorExit => {
                self.pop_ref()?;
            }
            CheckCast { class } => {
                self.pop_ref()?;
                // Refine the stack type when the target is an array
                // descriptor: javac emits `checkcast [Ljava/lang/String;`
                // after the generic Arrays.copyOf bridge (and for
                // `(String[]) list.toArray()` shapes), and the refined
                // value feeds array-shaped natives right after.
                match class.strip_prefix('[') {
                    Some(_) => self.push(VTy::Array(array_elem_kind(class)))?,
                    None => self.push(VTy::Ref)?,
                }
            }
            InstanceOf { .. } => {
                self.pop_ref()?;
                self.push(VTy::Int)?;
            }
        }
        // fallthrough successor
        let np = next_pc(self.m, self.pc);
        if np == u32::MAX {
            return Err(self.e("control flow falls off the end of the method body"));
        }
        Ok(vec![(np, self.snapshot())])
    }

    fn snapshot(&self) -> State {
        State {
            stack: self.stack.clone(),
            locals: self.locals.clone(),
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn plan(
        desc: &str,
        is_static: bool,
        max_stack: usize,
        max_locals: usize,
        instrs: Vec<(u32, Instr)>,
    ) -> MethodPlan {
        MethodPlan {
            name: "t".to_string(),
            desc: desc.to_string(),
            is_static,
            is_native: false,
            is_sync: false,
            modifiers: 0,
            fn_name: "m_t".to_string(),
            instrs,
            exception_table: Vec::new(),
            max_locals,
            max_stack,
        }
    }

    fn verify_ok(m: &MethodPlan) {
        let concats = BTreeMap::new();
        let lambdas = BTreeMap::new();
        verify_method(m, &concats, &lambdas).unwrap_or_else(|e| panic!("expected OK, got: {e}"));
    }

    fn verify_err(m: &MethodPlan) -> VerifyError {
        let concats = BTreeMap::new();
        let lambdas = BTreeMap::new();
        verify_method(m, &concats, &lambdas).expect_err("expected a verification error")
    }

    #[test]
    fn straight_line_add_is_valid() {
        // iconst_1 iconst_2 iadd ireturn — ()I
        let m = plan(
            "()I",
            true,
            2,
            0,
            vec![
                (0, Instr::IConst(1)),
                (1, Instr::IConst(2)),
                (
                    2,
                    Instr::Arith {
                        op: ArithOp::Add,
                        kind: NumKind::Int,
                    },
                ),
                (3, Instr::Return(TypeDesc::Int)),
            ],
        );
        verify_ok(&m);
    }

    #[test]
    fn pop_on_empty_stack_is_rejected() {
        let m = plan(
            "()V",
            true,
            1,
            0,
            vec![(0, Instr::Pop), (1, Instr::Return(TypeDesc::Void))],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("underflow"), "{e}");
    }

    #[test]
    fn pop_of_long_is_rejected() {
        let m = plan(
            "()V",
            true,
            2,
            0,
            vec![
                (0, Instr::LConst(1)),
                (1, Instr::Pop),
                (2, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("category-1"), "{e}");
    }

    #[test]
    fn iadd_with_one_operand_is_rejected() {
        // only one int on the stack — the second pop underflows
        let m = plan(
            "()I",
            true,
            2,
            0,
            vec![
                (0, Instr::IConst(1)),
                (
                    1,
                    Instr::Arith {
                        op: ArithOp::Add,
                        kind: NumKind::Int,
                    },
                ),
                (2, Instr::Return(TypeDesc::Int)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("underflow"), "{e}");
    }

    #[test]
    fn iadd_on_int_and_ref_is_rejected() {
        // one operand is an int, the other a reference — kind mismatch
        let m = plan(
            "()I",
            true,
            2,
            0,
            vec![
                (0, Instr::AConstNull),
                (1, Instr::IConst(1)),
                (
                    2,
                    Instr::Arith {
                        op: ArithOp::Add,
                        kind: NumKind::Int,
                    },
                ),
                (3, Instr::Return(TypeDesc::Int)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("expected Int"), "{e}");
    }

    #[test]
    fn swap_of_long_is_rejected() {
        // lconst_0 iconst_1 swap — swap requires two category-1 values
        let m = plan(
            "()V",
            true,
            3,
            0,
            vec![
                (0, Instr::LConst(0)),
                (1, Instr::IConst(1)),
                (2, Instr::Swap),
                (3, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("category-1"), "{e}");
    }

    fn dup2x2_case(instrs: Vec<(u32, Instr)>, max_stack: usize) -> MethodPlan {
        plan("()V", true, max_stack, 0, instrs)
    }

    #[test]
    fn dup2_x2_form1_four_cat1() {
        let m = dup2x2_case(
            vec![
                (0, Instr::IConst(1)),
                (1, Instr::IConst(2)),
                (2, Instr::IConst(3)),
                (3, Instr::IConst(4)),
                (4, Instr::Dup2X2),
                (5, Instr::Pop),
                (6, Instr::Pop),
                (7, Instr::Pop),
                (8, Instr::Pop),
                (9, Instr::Pop),
                (10, Instr::Pop),
                (11, Instr::Return(TypeDesc::Void)),
            ],
            8,
        );
        verify_ok(&m);
    }

    #[test]
    fn dup2_x2_form2_cat2_top_jama_compound_assignment() {
        // JAMA wave 3 shape: javac's compound double assignment
        // `LUrowi[j] = LUcolj[i] -= s;` ends with a cat-2 value on top of
        // two cat-1 values — JVMS form 2. Result [v1(cat2), v3, v2, v1]:
        // the two dastores that follow consume v1,i,arr then v1,j,arr2.
        let m = dup2x2_case(
            vec![
                (0, Instr::IConst(1)),
                (1, Instr::IConst(2)),
                (2, Instr::LConst(3)),
                (3, Instr::Dup2X2),
                (4, Instr::Pop2),
                (5, Instr::Pop),
                (6, Instr::Pop),
                (7, Instr::Pop2),
                (8, Instr::Return(TypeDesc::Void)),
            ],
            6,
        );
        verify_ok(&m);
    }

    #[test]
    fn dup2_x2_form3_cat2_at_bottom() {
        // v3(cat2), v2, v1 -> v2, v1, v3, v2, v1
        let m = dup2x2_case(
            vec![
                (0, Instr::LConst(1)),
                (1, Instr::IConst(2)),
                (2, Instr::IConst(3)),
                (3, Instr::Dup2X2),
                (4, Instr::Pop),
                (5, Instr::Pop),
                (6, Instr::Pop2),
                (7, Instr::Pop),
                (8, Instr::Pop),
                (9, Instr::Return(TypeDesc::Void)),
            ],
            6,
        );
        verify_ok(&m);
    }

    #[test]
    fn dup2_x2_form4_two_cat2() {
        let m = dup2x2_case(
            vec![
                (0, Instr::LConst(1)),
                (1, Instr::DConst(2.0)),
                (2, Instr::Dup2X2),
                (3, Instr::Pop2),
                (4, Instr::Pop2),
                (5, Instr::Pop2),
                (6, Instr::Return(TypeDesc::Void)),
            ],
            5,
        );
        verify_ok(&m);
    }

    #[test]
    fn dup2_x2_cat1_on_top_of_cat2_is_invalid() {
        // no JVMS form has a category-1 value on top of a category-2 one
        // (the old verifier ACCEPTED this shape with a scrambled push
        // order — the JAMA corpus wave corrected the classification)
        let m = dup2x2_case(
            vec![
                (0, Instr::LConst(2)),
                (1, Instr::IConst(1)),
                (2, Instr::Dup2X2),
                (3, Instr::Return(TypeDesc::Void)),
            ],
            5,
        );
        let e = verify_err(&m);
        assert!(e.message.contains("no JVMS form"), "{e}");
    }

    #[test]
    fn branch_into_middle_of_instruction_is_rejected() {
        // goto 2, but pc 2 is not an instruction start (instructions at 0,1,3)
        let m = plan(
            "()V",
            true,
            1,
            0,
            vec![
                (0, Instr::Goto(2)),
                (1, Instr::Nop),
                (3, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("not an instruction boundary"), "{e}");
    }

    #[test]
    fn out_of_range_local_is_rejected() {
        let m = plan(
            "(I)V",
            true,
            1,
            1,
            vec![
                (0, Instr::Load(5)),
                (1, Instr::Pop),
                (2, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("out of range"), "{e}");
    }

    #[test]
    fn loading_high_half_of_long_local_is_rejected() {
        // lstore_0; lload_1 — local 1 is the high half
        let m = plan(
            "()V",
            true,
            2,
            3,
            vec![
                (0, Instr::LConst(1)),
                (1, Instr::Store(0)),
                (2, Instr::Load(1)),
                (3, Instr::Pop),
                (4, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("high half"), "{e}");
    }

    #[test]
    fn loading_never_assigned_local_is_rejected() {
        let m = plan(
            "()V",
            true,
            1,
            2,
            vec![
                (0, Instr::Load(1)),
                (1, Instr::Pop),
                (2, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("never-assigned"), "{e}");
    }

    #[test]
    fn join_with_different_stack_heights_is_rejected() {
        // 0: iconst_1; 1: ifeq 4  (arm A enters 4 with an empty stack)
        // 2: iconst_1; 3: goto 4  (arm B enters 4 with one int)
        let m = plan(
            "()V",
            true,
            2,
            0,
            vec![
                (0, Instr::IConst(1)),
                (1, Instr::IfZero(CmpOp::Eq, 4)),
                (2, Instr::IConst(1)),
                (3, Instr::Goto(4)),
                (4, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("stack heights"), "{e}");
    }

    #[test]
    fn join_with_conflicting_types_is_rejected() {
        // arm A (ifeq taken) pushes an int, arm B (fallthrough) a ref;
        // both join at the pop
        let m = plan(
            "()V",
            true,
            2,
            0,
            vec![
                (0, Instr::IConst(0)),
                (1, Instr::IfZero(CmpOp::Eq, 4)),
                (2, Instr::AConstNull),
                (3, Instr::Goto(5)),
                (4, Instr::IConst(1)),
                (5, Instr::Pop),
                (6, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("incompatible"), "{e}");
    }

    #[test]
    fn locals_of_different_types_may_join_but_not_load() {
        // arm A stores an int in local 0, arm B a ref; both join; loading
        // the local afterwards is a verification error (poisoned)
        let m = plan(
            "(I)V",
            true,
            2,
            2,
            vec![
                (0, Instr::IConst(1)),
                (1, Instr::IfZero(CmpOp::Eq, 5)),
                (2, Instr::IConst(7)),
                (3, Instr::Store(1)),
                (4, Instr::Goto(7)),
                (5, Instr::AConstNull),
                (6, Instr::Store(1)),
                (7, Instr::Load(1)),
                (8, Instr::Pop),
                (9, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("conflicting types"), "{e}");
    }

    #[test]
    fn return_type_must_match_descriptor() {
        // ireturn in a ()V method
        let m = plan(
            "()V",
            true,
            1,
            0,
            vec![(0, Instr::IConst(0)), (1, Instr::Return(TypeDesc::Int))],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("declared"), "{e}");
    }

    #[test]
    fn areturn_of_int_is_rejected() {
        let m = plan(
            "()Ljava/lang/Object;",
            true,
            1,
            0,
            vec![
                (0, Instr::IConst(0)),
                (1, Instr::Return(TypeDesc::Object(String::new()))),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("reference"), "{e}");
    }

    #[test]
    fn max_stack_overflow_is_rejected() {
        let m = plan(
            "()V",
            true,
            1,
            0,
            vec![
                (0, Instr::IConst(1)),
                (1, Instr::IConst(2)),
                (2, Instr::Pop),
                (3, Instr::Pop),
                (4, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("max_stack"), "{e}");
    }

    #[test]
    fn handler_entry_state_is_one_ref() {
        // athrow; the handler (pc 3) starts with exactly the caught ref
        let mut m = plan(
            "()V",
            true,
            2,
            0,
            vec![
                (0, Instr::AConstNull),
                (1, Instr::AThrow),
                (2, Instr::Return(TypeDesc::Void)),
                (3, Instr::Pop),
                (4, Instr::Return(TypeDesc::Void)),
            ],
        );
        m.exception_table = vec![(0, 3, 3, Some("java/lang/Throwable".to_string()))];
        verify_ok(&m);
    }

    #[test]
    fn handler_entry_with_wrong_shape_is_rejected() {
        // the handler starts with pop2, which needs two cat-1 values, but a
        // handler entry always has exactly ONE ref — malformed
        let mut m = plan(
            "()V",
            true,
            3,
            0,
            vec![
                (0, Instr::AConstNull),
                (1, Instr::AThrow),
                (2, Instr::Return(TypeDesc::Void)),
                (3, Instr::Pop2),
                (4, Instr::Return(TypeDesc::Void)),
            ],
        );
        m.exception_table = vec![(0, 3, 3, Some("java/lang/Throwable".to_string()))];
        let e = verify_err(&m);
        assert!(e.message.contains("underflow"), "{e}");
    }

    #[test]
    fn unknown_catch_type_is_rejected() {
        use crate::{ClassPlan, Program};
        use j2r_classfile::{ClassFile, ConstantPool};
        let mut m = plan(
            "()V",
            true,
            2,
            0,
            vec![
                (0, Instr::AConstNull),
                (1, Instr::AThrow),
                (2, Instr::Return(TypeDesc::Void)),
                (3, Instr::Pop),
                (4, Instr::Return(TypeDesc::Void)),
            ],
        );
        m.exception_table = vec![(0, 3, 3, Some("com/example/Bogus".to_string()))];
        let cf = ClassFile {
            minor_version: 0,
            major_version: 52,
            constant_pool: ConstantPool::default(),
            access: j2r_classfile::flags::ACC_PUBLIC,
            this_class: "T".to_string(),
            super_class: Some("java/lang/Object".to_string()),
            interfaces: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            bootstrap_methods: Vec::new(),
            inner_name: None,
        };
        let c = ClassPlan {
            name: "T".to_string(),
            super_name: Some("java/lang/Object".to_string()),
            interfaces: Vec::new(),
            is_interface: false,
            is_abstract: false,
            simple_name: None,
            module: "T".to_string(),
            id: 0,
            base_slots: 0,
            total_slots: 0,
            fields: Vec::new(),
            methods: vec![m],
            clinit_fn: None,
            concats: BTreeMap::new(),
            lambdas: BTreeMap::new(),
            static_monitor: None,
            refs: std::collections::BTreeSet::new(),
            input_sha: String::new(),
            cf,
        };
        let prog = Program {
            classes: Vec::new(), // T deliberately absent → catch type unknown
            user_index: BTreeMap::new(),
            total_statics: 0,
            main_class: None,
        };
        let e = crate::verify::verify_class(&prog, &c).expect_err("bogus catch type");
        match e {
            crate::CodegenError::Verify { message, .. } => {
                assert!(message.contains("known class"), "{message}")
            }
            other => panic!("expected Verify error, got {other:?}"),
        }
    }

    #[test]
    fn long_shift_pops_int_amount() {
        // lshl: long, int -> long
        let m = plan(
            "()J",
            true,
            4,
            0,
            vec![
                (0, Instr::LConst(1)),
                (1, Instr::IConst(2)),
                (
                    2,
                    Instr::Arith {
                        op: ArithOp::Shl,
                        kind: NumKind::Long,
                    },
                ),
                (3, Instr::Return(TypeDesc::Long)),
            ],
        );
        verify_ok(&m);
        // wrong: two ints then lshl must fail
        let m = plan(
            "()J",
            true,
            4,
            0,
            vec![
                (0, Instr::IConst(1)),
                (1, Instr::IConst(2)),
                (
                    2,
                    Instr::Arith {
                        op: ArithOp::Shl,
                        kind: NumKind::Long,
                    },
                ),
                (3, Instr::Return(TypeDesc::Long)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("expected Long"), "{e}");
    }

    #[test]
    fn lastore_rejects_int_value() {
        // iconst_1 (instead of lconst) then lastore — category error
        let m = plan(
            "()V",
            true,
            4,
            1,
            vec![
                (0, Instr::IConst(2)), // array length
                (1, Instr::NewArray(PrimType::Long)),
                (2, Instr::IConst(0)), // index
                (3, Instr::IConst(5)), // WRONG: int value for a long[]
                (4, Instr::AStore(ElemKind::Long)),
                (5, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("expected Long"), "{e}");
    }

    #[test]
    fn lastore_accepts_long_value() {
        let m = plan(
            "()V",
            true,
            5,
            1,
            vec![
                (0, Instr::IConst(2)),
                (1, Instr::NewArray(PrimType::Long)),
                (2, Instr::IConst(0)),
                (3, Instr::LConst(5)),
                (4, Instr::AStore(ElemKind::Long)),
                (5, Instr::Return(TypeDesc::Void)),
            ],
        );
        verify_ok(&m);
    }

    #[test]
    fn aaload_pushes_a_ref_not_an_int() {
        let m = plan(
            "()I",
            true,
            3,
            0,
            vec![
                (0, Instr::IConst(2)),
                (
                    1,
                    Instr::ANewArray {
                        elem: "java/lang/Object".to_string(),
                    },
                ),
                (2, Instr::IConst(0)),
                (3, Instr::ALoad(ElemKind::Ref)),
                (
                    4,
                    Instr::Arith {
                        op: ArithOp::Add,
                        kind: NumKind::Int,
                    },
                ),
                (5, Instr::Return(TypeDesc::Int)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("expected Int"), "{e}");
    }

    #[test]
    fn putstatic_accepts_null_for_array_typed_static() {
        // Corpus F2: javac compiles `static char[] NONE = null;` in
        // <clinit> as aconst_null; putstatic F:[C. null is assignable to
        // every reference type — arrays included — so the verifier must
        // accept it for a typed-array slot.
        let m = plan(
            "()V",
            true,
            1,
            0,
            vec![
                (0, Instr::AConstNull),
                (
                    1,
                    Instr::PutStatic {
                        class: "C".to_string(),
                        field: "f".to_string(),
                        desc: "[C".to_string(),
                    },
                ),
                (2, Instr::Return(TypeDesc::Void)),
            ],
        );
        verify_ok(&m);
    }

    #[test]
    fn null_argument_to_array_typed_parameter_verifies() {
        // `foo((int[]) null)` — the same null-for-reference-shape
        // polymorphism at an invokestatic call site.
        let m = plan(
            "()V",
            true,
            1,
            0,
            vec![
                (0, Instr::AConstNull),
                (
                    1,
                    Instr::InvokeStatic {
                        class: "C".to_string(),
                        name: "m".to_string(),
                        desc: "([I)V".to_string(),
                    },
                ),
                (2, Instr::Return(TypeDesc::Void)),
            ],
        );
        verify_ok(&m);
    }

    #[test]
    fn null_still_rejected_for_primitive_static() {
        // The polymorphism is one-directional: null is a reference. An
        // int-typed static keeps rejecting it.
        let m = plan(
            "()V",
            true,
            1,
            0,
            vec![
                (0, Instr::AConstNull),
                (
                    1,
                    Instr::PutStatic {
                        class: "C".to_string(),
                        field: "f".to_string(),
                        desc: "I".to_string(),
                    },
                ),
                (2, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("expected Int"), "{e}");
    }

    #[test]
    fn wrong_primitive_still_rejected_for_array_typed_static() {
        // Exactness is kept beyond null: an int must not slip into a
        // char[]-typed static either.
        let m = plan(
            "()V",
            true,
            1,
            0,
            vec![
                (0, Instr::IConst(0)),
                (
                    1,
                    Instr::PutStatic {
                        class: "C".to_string(),
                        field: "f".to_string(),
                        desc: "[C".to_string(),
                    },
                ),
                (2, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("expected Array"), "{e}");
    }

    #[test]
    fn depth4_aaload_chain_yields_typed_shapes() {
        // Depth-4 wave: `[[[[I` elements are `[[[I`-shaped values
        // (RefPrimArray3Int); three aaloads descend the lattice
        // RefPrimArray3Int → RefPrimArray2Int → RefPrimArrayInt → IntGroup,
        // and the extracted `int[]` accepts iastore.
        let m = plan(
            "()V",
            true,
            4,
            0,
            vec![
                (0, Instr::IConst(0)),
                (1, Instr::IConst(0)),
                (2, Instr::IConst(0)),
                (3, Instr::IConst(0)),
                (
                    4,
                    Instr::MultiNewArray {
                        desc: "[[[[I".to_string(),
                        dims: 4,
                    },
                ),
                (5, Instr::IConst(0)),
                (6, Instr::ALoad(ElemKind::Ref)), // -> [[[I
                (7, Instr::IConst(0)),
                (8, Instr::ALoad(ElemKind::Ref)), // -> [[I
                (9, Instr::IConst(0)),
                (10, Instr::ALoad(ElemKind::Ref)), // -> [I
                (11, Instr::IConst(0)),
                (12, Instr::IConst(42)),
                (13, Instr::AStore(ElemKind::IntGroup)), // iastore into [I
                (14, Instr::Return(TypeDesc::Void)),
            ],
        );
        verify_ok(&m);
    }

    #[test]
    fn depth4_wrong_kind_load_names_precise_kind() {
        // The precision the wave buys: iaload directly on a `[[[[I` array
        // fails with the EXACT depth-4 shape named (the pre-wave Ref
        // fallback made the message generic).
        let m = plan(
            "()V",
            true,
            4,
            0,
            vec![
                (0, Instr::IConst(0)),
                (1, Instr::IConst(0)),
                (2, Instr::IConst(0)),
                (3, Instr::IConst(0)),
                (
                    4,
                    Instr::MultiNewArray {
                        desc: "[[[[I".to_string(),
                        dims: 4,
                    },
                ),
                (5, Instr::IConst(0)),
                (6, Instr::ALoad(ElemKind::IntGroup)), // iaload on the outer
                (7, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(
            e.message.contains("array is Nest(Nest(Nest(IntGroup)))"),
            "{e}"
        );
    }

    #[test]
    fn depth4_aaload_result_feeds_typed_call_site() {
        // The JZlib-wave pattern one level deeper: the extracted `[[[I`
        // rows satisfy a `([[[I)V`-typed call site — the whole reason the
        // lattice is typed instead of Ref-collapsed.
        let m = plan(
            "()V",
            true,
            4,
            0,
            vec![
                (0, Instr::IConst(0)),
                (1, Instr::IConst(0)),
                (2, Instr::IConst(0)),
                (3, Instr::IConst(0)),
                (
                    4,
                    Instr::MultiNewArray {
                        desc: "[[[[I".to_string(),
                        dims: 4,
                    },
                ),
                (5, Instr::IConst(0)),
                (6, Instr::ALoad(ElemKind::Ref)), // -> [[[I
                (
                    7,
                    Instr::InvokeStatic {
                        class: "C".to_string(),
                        name: "m".to_string(),
                        desc: "([[[I)V".to_string(),
                    },
                ),
                (8, Instr::Return(TypeDesc::Void)),
            ],
        );
        verify_ok(&m);
    }

    #[test]
    fn depth5_recursive_descriptor_any_depth() {
        // The closure law (2026-11-12 wave, owner-decided design): the
        // recursive descriptor folds ANY depth — depth 5 is
        // Nest(Nest(Nest(Nest(IntGroup)))), zero new variants. The typed
        // chain still verifies (aaload yields the exact (depth-1)-shaped
        // row, satisfying a `([[[[I)V` call site), and a wrong-kind
        // primitive load is still rejected — naming the exact folded
        // shape.
        let m = plan(
            "()V",
            true,
            5,
            0,
            vec![
                (0, Instr::IConst(0)),
                (1, Instr::IConst(0)),
                (2, Instr::IConst(0)),
                (3, Instr::IConst(0)),
                (4, Instr::IConst(0)),
                (
                    5,
                    Instr::MultiNewArray {
                        desc: "[[[[[I".to_string(),
                        dims: 5,
                    },
                ),
                (6, Instr::IConst(0)),
                (7, Instr::ALoad(ElemKind::Ref)), // -> the exact 4-deep row shape
                (
                    8,
                    Instr::InvokeStatic {
                        class: "C".to_string(),
                        name: "m".to_string(),
                        desc: "([[[[I)V".to_string(),
                    },
                ),
                (9, Instr::Return(TypeDesc::Void)),
            ],
        );
        verify_ok(&m);

        let bad = plan(
            "()V",
            true,
            5,
            0,
            vec![
                (0, Instr::IConst(0)),
                (1, Instr::IConst(0)),
                (2, Instr::IConst(0)),
                (3, Instr::IConst(0)),
                (4, Instr::IConst(0)),
                (
                    5,
                    Instr::MultiNewArray {
                        desc: "[[[[[I".to_string(),
                        dims: 5,
                    },
                ),
                (6, Instr::IConst(0)),
                (7, Instr::ALoad(ElemKind::IntGroup)), // iaload on the outer
                (8, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&bad);
        assert!(
            e.message
                .contains("array is Nest(Nest(Nest(Nest(IntGroup))))"),
            "{e}"
        );
    }

    #[test]
    fn putstatic_descriptor_shape_is_checked() {
        let m = plan(
            "()V",
            true,
            2,
            0,
            vec![
                (0, Instr::Ldc(Ldc::String("x".to_string()))),
                (
                    1,
                    Instr::PutStatic {
                        class: "C".to_string(),
                        field: "f".to_string(),
                        desc: "I".to_string(),
                    },
                ),
                (2, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("expected Int"), "{e}");
    }

    #[test]
    fn unreachable_code_is_not_verified() {
        // dead code after return: junk shapes are fine
        let m = plan(
            "()V",
            true,
            1,
            0,
            vec![
                (0, Instr::Return(TypeDesc::Void)),
                (1, Instr::Pop),  // unreachable underflow — ignored
                (2, Instr::Swap), // unreachable — ignored
                (3, Instr::Return(TypeDesc::Void)),
            ],
        );
        verify_ok(&m);
    }

    #[test]
    fn falling_off_the_end_is_rejected() {
        let m = plan(
            "()V",
            true,
            1,
            0,
            vec![(0, Instr::IConst(1)), (1, Instr::Pop)],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("falls off"), "{e}");
    }

    #[test]
    fn store_into_high_half_poisons_the_long() {
        // lstore_0; iconst_1; istore_1; lload_0 — local 0 was poisoned
        let m = plan(
            "()V",
            true,
            3,
            3,
            vec![
                (0, Instr::LConst(1)),
                (1, Instr::Store(0)),
                (2, Instr::IConst(1)),
                (3, Instr::Store(1)),
                (4, Instr::Load(0)),
                (5, Instr::Pop2),
                (6, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("conflicting types"), "{e}");
    }

    #[test]
    fn iinc_requires_int_local() {
        let m = plan(
            "()V",
            true,
            1,
            2,
            vec![
                (0, Instr::Ldc(Ldc::String("s".to_string()))),
                (1, Instr::Store(0)),
                (2, Instr::IInc { idx: 0, c: 1 }),
                (3, Instr::Return(TypeDesc::Void)),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("int local"), "{e}");
    }

    #[test]
    fn entry_locals_come_from_the_descriptor() {
        // instance method: local 0 is `this` (a ref)
        let m = plan(
            "()Ljava/lang/Object;",
            false,
            1,
            1,
            vec![
                (0, Instr::Load(0)),
                (1, Instr::Return(TypeDesc::Object(String::new()))),
            ],
        );
        verify_ok(&m);
        // but a static method with the same body must fail: local 0 unset
        let m = plan(
            "()Ljava/lang/Object;",
            true,
            1,
            1,
            vec![
                (0, Instr::Load(0)),
                (1, Instr::Return(TypeDesc::Object(String::new()))),
            ],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("never-assigned"), "{e}");
    }

    #[test]
    fn max_locals_too_small_for_descriptor_is_rejected() {
        // (JD)V needs 4 locals; declare 2
        let m = plan(
            "(JD)V",
            true,
            4,
            2,
            vec![(0, Instr::Return(TypeDesc::Void))],
        );
        let e = verify_err(&m);
        assert!(e.message.contains("max_locals"), "{e}");
    }

    #[test]
    fn join_of_null_and_ref_is_accepted() {
        // one arm produces null, the other a real ref; both merge to Ref
        let m = plan(
            "(I)Ljava/lang/Object;",
            true,
            2,
            1,
            vec![
                (0, Instr::Load(0)),
                (1, Instr::IfZero(CmpOp::Eq, 4)),
                (2, Instr::AConstNull),
                (3, Instr::Goto(5)),
                (4, Instr::Ldc(Ldc::String("x".to_string()))),
                (5, Instr::Return(TypeDesc::Object(String::new()))),
            ],
        );
        verify_ok(&m);
    }
}
