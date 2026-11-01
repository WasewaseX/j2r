//! Typed-storage specialization decision pass — Wave 3 of M2
//! (`docs/M2_PLAN.md` §5).
//!
//! Inputs: the verifier's per-pc [`MethodShapes`] (stack **and** locals
//! shapes, exported in Wave 3 step 3a). Output: a per-method, all-or-
//! nothing decision — which bottom-indexed operand-stack positions and
//! which local slots may live in typed primitive pools instead of
//! `Vec<Value>`, or the reason the whole method falls back to the uniform
//! emission.
//!
//! Scope (settled with the reviewer, M2_PLAN §5.1): specialization covers
//! operand-stack positions **and** local-variable slots for primitives;
//! **references never leave `Vec<Value>` storage** — ref stack positions,
//! ref locals and `thrown` stay exactly as they are, so the Wave-2 GC scan
//! surface is unchanged by construction. Correctness never depends on
//! specialization succeeding: a fallback method compiles and runs
//! identically.

use crate::verify::{MethodShapes, VTy};

/// The storage class decided for one stack position / local slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pool {
    /// `Value::Int` traffic only → `i32` pool.
    Int,
    /// `Value::Long` traffic only → `i64` pool.
    Long,
    /// `Value::Float` traffic only → `f32` pool.
    Float,
    /// `Value::Double` traffic only → `f64` pool.
    Double,
    /// References (or a kind the decision pass cannot pin) stay in
    /// `Vec<Value>` — the GC scan surface.
    Value,
}

impl Pool {
    fn of(v: VTy) -> Option<Pool> {
        match v {
            VTy::Int => Some(Pool::Int),
            VTy::Long => Some(Pool::Long),
            VTy::Float => Some(Pool::Float),
            VTy::Double => Some(Pool::Double),
            // References of every shape stay in Value storage.
            VTy::Ref | VTy::Null | VTy::Array(_) | VTy::Top => Some(Pool::Value),
            // Never legal to load; a position that is ever Poison/HighHalf
            // is not cleanly typed, so it must stay in Value storage (the
            // verifier already rejects loads of them — the pool would never
            // be read — but the all-or-nothing rule keeps the frame shape
            // simple).
            VTy::Unset => None,
            VTy::Poison | VTy::HighHalf => Some(Pool::Value),
        }
    }
}

/// Why a method fell back to uniform emission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fallback {
    /// Specialized (no fallback).
    None,
    /// Some stack position or local slot mixes kinds across pcs.
    KindVaries,
    /// No stack position and no local slot is primitive — nothing to gain.
    NoPrimitives,
}

/// The per-method specialization decision (all-or-nothing).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodSpec {
    /// One entry per bottom-indexed operand-stack position (0..max_stack).
    pub stack: Vec<Pool>,
    /// One entry per local slot (0..max_locals).
    pub locals: Vec<Pool>,
    /// True when at least one stack position or local slot is primitive.
    pub specialize: bool,
    pub fallback: Fallback,
    /// (total stack positions, of which primitive pools)
    pub stack_counts: (usize, usize),
    /// (total local slots, of which primitive pools)
    pub local_counts: (usize, usize),
}

/// Decides the storage plan for one method from its verified shapes.
///
/// Rule (M2_PLAN §5.1): every stack position and every local slot must be
/// *kind-stable* across all pcs where it is live — one [`Pool`] each. Any
/// position that mixes primitive kinds (e.g. `Int` at one pc, `Long` at
/// another) or mixes primitive with reference makes the **whole method**
/// fall back; a partially specialized frame is never emitted, so there is
/// exactly one frame shape per method and one GC scan surface.
pub fn plan_method(shapes: &MethodShapes) -> MethodSpec {
    let n_stack = shapes.max_stack.max(1);
    let n_locals = shapes.max_locals;

    let mut stack: Vec<Option<Pool>> = vec![None; n_stack];
    let mut locals: Vec<Option<Pool>> = vec![None; n_locals];
    let mut kind_varies = false;

    for shape in shapes.per_pc.values() {
        for (i, v) in shape.stack.iter().enumerate() {
            if i >= n_stack {
                continue;
            }
            match (stack[i], Pool::of(*v)) {
                (None, p) => stack[i] = p,
                (Some(cur), Some(p)) if p != cur => {
                    // A slot that is ever Unset next to a known kind is
                    // handled by the None arm ordering; two known kinds
                    // that disagree → not kind-stable. Unset beats
                    // nothing: positions live only where reached.
                    kind_varies = true;
                    stack[i] = Some(if p == Pool::Value || cur == Pool::Value {
                        Pool::Value
                    } else {
                        // keep tracking; the method falls back anyway
                        p
                    });
                }
                _ => {}
            }
        }
        for (j, v) in shape.locals.iter().enumerate() {
            if j >= n_locals {
                continue;
            }
            match (locals[j], Pool::of(*v)) {
                (None, p) => locals[j] = p,
                (Some(cur), Some(p)) if p != cur => {
                    kind_varies = true;
                    locals[j] = Some(if p == Pool::Value || cur == Pool::Value {
                        Pool::Value
                    } else {
                        p
                    });
                }
                _ => {}
            }
        }
    }

    // A slot only ever seen as Unset never holds a value — treat as Value
    // storage (harmless, never read).
    let stack: Vec<Pool> = stack
        .into_iter()
        .map(|p| p.unwrap_or(Pool::Value))
        .collect();
    let locals: Vec<Pool> = locals
        .into_iter()
        .map(|p| p.unwrap_or(Pool::Value))
        .collect();

    let stack_prims = stack.iter().filter(|p| **p != Pool::Value).count();
    let local_prims = locals.iter().filter(|p| **p != Pool::Value).count();
    let has_prims = stack_prims + local_prims > 0;

    let fallback = if kind_varies {
        Fallback::KindVaries
    } else if !has_prims {
        Fallback::NoPrimitives
    } else {
        Fallback::None
    };

    MethodSpec {
        specialize: fallback == Fallback::None,
        fallback,
        stack_counts: (n_stack, stack_prims),
        local_counts: (n_locals, local_prims),
        stack,
        locals,
    }
}

/// One line of the 3a shape report (M2_PLAN §5.2).
#[derive(Clone, Debug)]
pub struct MethodReport {
    pub class: String,
    pub method: String,
    pub spec: MethodSpec,
}

/// Builds the per-method decision table for a whole program (the caller
/// must have verified it already; methods that fail verification are
/// compile errors anyway).
pub fn plan_program(prog: &crate::Program) -> Vec<MethodReport> {
    let mut out = Vec::new();
    for c in &prog.classes {
        for m in &c.methods {
            let Ok(shapes) = crate::verify::verify_method_shapes(m, &c.concats) else {
                continue; // verification errors are reported by the pipeline
            };
            let spec = plan_method(&shapes);
            out.push(MethodReport {
                class: c.name.clone(),
                method: m.name.clone(),
                spec,
            });
        }
    }
    out
}

/// Renders the 3a shape report as markdown (per-fixture totals +
/// per-method rows with fallback reasons — the delivered scope, stacks
/// *and* locals, is always visible and never overstated).
pub fn render_report(rows: &[MethodReport]) -> String {
    use std::fmt::Write as _;
    let methods = rows.len();
    let specialized = rows.iter().filter(|r| r.spec.specialize).count();
    let stack_total: usize = rows.iter().map(|r| r.spec.stack_counts.0).sum();
    let stack_prims: usize = rows.iter().map(|r| r.spec.stack_counts.1).sum();
    let local_total: usize = rows.iter().map(|r| r.spec.local_counts.0).sum();
    let local_prims: usize = rows.iter().map(|r| r.spec.local_counts.1).sum();
    let mut s = String::new();
    let _ = writeln!(s, "| methods | specialized | fallback | stack positions (prim/total) | local slots (prim/total) |");
    let _ = writeln!(s, "| --- | --- | --- | --- | --- |");
    let _ = writeln!(
        s,
        "| {methods} | {specialized} | {} | {stack_prims}/{stack_total} | {local_prims}/{local_total} |",
        methods - specialized
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::PcShape;
    use std::collections::BTreeMap;

    fn shapes(entries: &[(u32, Vec<VTy>, Vec<VTy>)]) -> MethodShapes {
        let mut per_pc = BTreeMap::new();
        for (pc, stack, locals) in entries {
            per_pc.insert(
                *pc,
                PcShape {
                    stack: stack.clone(),
                    locals: locals.clone(),
                },
            );
        }
        MethodShapes {
            per_pc,
            max_locals: entries.iter().map(|e| e.2.len()).max().unwrap_or(0),
            max_stack: entries.iter().map(|e| e.1.len()).max().unwrap_or(0),
        }
    }

    #[test]
    fn pure_arithmetic_method_specializes() {
        // iconst/iload/iadd loops: stack is all Int, locals all Int.
        let s = shapes(&[
            (0, vec![VTy::Int, VTy::Int], vec![VTy::Int, VTy::Unset]),
            (8, vec![VTy::Int], vec![VTy::Int, VTy::Int]),
        ]);
        let spec = plan_method(&s);
        assert!(spec.specialize);
        assert_eq!(spec.fallback, Fallback::None);
        assert_eq!(spec.stack[0], Pool::Int);
        assert_eq!(spec.stack[1], Pool::Int);
        assert_eq!(spec.locals[0], Pool::Int);
        assert_eq!(spec.locals[1], Pool::Int);
        assert_eq!(spec.stack_counts, (2, 2));
        assert_eq!(spec.local_counts, (2, 2));
    }

    #[test]
    fn kind_varying_slot_falls_back_whole_method() {
        // Stack position 0 is Int at one pc and Ref at another.
        let s = shapes(&[(0, vec![VTy::Int], vec![]), (8, vec![VTy::Ref], vec![])]);
        let spec = plan_method(&s);
        assert!(!spec.specialize);
        assert_eq!(spec.fallback, Fallback::KindVaries);
    }

    #[test]
    fn primitive_kind_mix_falls_back() {
        // Position 0 is Int at one pc, Long at another — a pool of the
        // wrong width would be unsound; all-or-nothing fallback.
        let s = shapes(&[(0, vec![VTy::Int], vec![]), (8, vec![VTy::Long], vec![])]);
        let spec = plan_method(&s);
        assert!(!spec.specialize);
        assert_eq!(spec.fallback, Fallback::KindVaries);
    }

    #[test]
    fn reference_only_method_falls_back_not_worthwhile() {
        let s = shapes(&[
            (0, vec![VTy::Ref], vec![VTy::Ref]),
            (8, vec![VTy::Null], vec![VTy::Ref]),
        ]);
        let spec = plan_method(&s);
        assert!(!spec.specialize);
        assert_eq!(spec.fallback, Fallback::NoPrimitives);
        // References stay in Value storage either way.
        assert_eq!(spec.stack[0], Pool::Value);
        assert_eq!(spec.locals[0], Pool::Value);
    }

    #[test]
    fn reference_positions_stay_in_value_even_in_specialized_methods() {
        // Mixed method: an int local plus an object local. The object
        // stays in Vec<Value> (GC scan surface unchanged); the int can
        // still specialize.
        let s = shapes(&[
            (0, vec![VTy::Int, VTy::Ref], vec![VTy::Int, VTy::Ref]),
            (8, vec![VTy::Int, VTy::Ref], vec![VTy::Int, VTy::Ref]),
        ]);
        let spec = plan_method(&s);
        assert!(spec.specialize);
        assert_eq!(spec.stack[0], Pool::Int);
        assert_eq!(
            spec.stack[1],
            Pool::Value,
            "references never leave Vec<Value>"
        );
        assert_eq!(
            spec.locals[1],
            Pool::Value,
            "references never leave Vec<Value>"
        );
        assert_eq!(spec.stack_counts, (2, 1));
    }

    #[test]
    fn cat2_values_occupy_one_pool_entry() {
        // Our Value model: a long is ONE stack slot (VTy::Long), so a
        // long-typed position maps to one Long pool entry — never two.
        let s = shapes(&[(0, vec![VTy::Long, VTy::Int], vec![VTy::Long])]);
        let spec = plan_method(&s);
        assert!(spec.specialize);
        assert_eq!(spec.stack[0], Pool::Long);
        assert_eq!(spec.stack[1], Pool::Int);
        assert_eq!(
            spec.stack.len(),
            2,
            "cat2 must not alias a second pool entry"
        );
        assert_eq!(spec.locals[0], Pool::Long);
    }

    #[test]
    fn array_and_top_are_reference_storage() {
        let s = shapes(&[(
            0,
            vec![VTy::Array(crate::ir::ElemKind::IntGroup), VTy::Top],
            vec![],
        )]);
        let spec = plan_method(&s);
        assert!(!spec.specialize, "no primitives at all");
        assert_eq!(spec.stack[0], Pool::Value);
        assert_eq!(spec.stack[1], Pool::Value);
    }

    #[test]
    fn poisoned_local_stays_in_value_storage() {
        // A local reused for Int and Ref across disjoint live ranges is
        // Poison after the join — it must not enter a primitive pool.
        let s = shapes(&[
            (0, vec![], vec![VTy::Int]),
            (8, vec![], vec![VTy::Ref]),
            (16, vec![], vec![VTy::Poison]),
        ]);
        let spec = plan_method(&s);
        assert!(!spec.specialize);
        assert_eq!(spec.fallback, Fallback::KindVaries);
    }
}
