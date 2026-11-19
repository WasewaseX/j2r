//! IR peephole passes (G2.6, console.md F6 — REVIEW_CHECKLIST §G2.6 arm a).
//!
//! The measured wall (docs/BENCHMARKS.md §2026-11-06): the release build
//! of the JZlib generated crate SIGKILLed rustc at every opt level
//! (3.03–3.56 GB, borrow-check/MIR phase) because javac's array-literal
//! initializers expand to thousands of straight-line interpreter
//! statements — e.g. the InfTree `<clinit>` fill of one 1536-element
//! `int[]` emitted 12,354 lines that MIR/borrow-check as ONE function.
//! Partitioning bounds CRATE size; it cannot split ONE enormous method
//! body, so the fix is at the IR level: recognize javac's exact
//! array-literal fill shape and collapse it to ONE literal-array
//! construction instruction.
//!
//! Byte-identical by construction:
//! * the replaced run has stack effect `[.., ref] -> [.., ref]` (the
//!   constructor pushes exactly one filled ref, identical object shape
//!   and payload as `array_new_prim` + N × `array_store`);
//! * a collapsed fill cannot throw (constant length ≥ threshold > 0,
//!   constant in-range indices, constant values, primitive int stores —
//!   no NPE/ArrayStoreException/NegativeArraySizeException is reachable),
//!   so dropping the run's fault points and any handler coverage over it
//!   changes no observable behavior;
//! * per-instruction GC safepoints inside the run become one safepoint
//!   per collapsed instruction — GC only runs AT safepoints, and fewer
//!   safepoints in straight-line code delays, never moves, collection;
//! * any shape that does not match exactly (interleaved instructions,
//!   non-constant values, out-of-order indices, length ≠ group count,
//!   branch/handler targets inside the span) is left untouched and keeps
//!   the old per-element emission.

use crate::ir::{Instr, PrimType};

pub fn proven_map_fields(
    cls: &crate::ClassPlan,
) -> std::collections::BTreeMap<(String, String), (PrimType, crate::ir::MapKind)> {
    proven_map_fields_of(&cls.name, &cls.fields, &cls.methods)
}

/// replaced. `shapes` must be the method's verified per-pc shapes (the
/// caller re-verifies after the rewrite; the synthetic instructions have
/// verifier arms). `proven` is the class-level proven static-field set
/// (slice 3) — the getstatic receivers this method's cores may use.
pub fn map_prim_rewrites(
    instrs: &mut Vec<(u32, Instr)>,
    exception_table: &[(u32, u32, u32, Option<String>)],
    shapes: &crate::verify::MethodShapes,
    proven: &std::collections::BTreeMap<(String, String), (PrimType, crate::ir::MapKind)>,
) -> usize {
    // Branch targets, for the span guards.
    let mut targets = Vec::new();
    targets_of(instrs, &mut targets);
    targets.sort_unstable();
    targets.dedup();

    // Phase A — map-fresh locals (the helper above).
    let fresh = fresh_map_locals(instrs, &targets);
    // (No early-out on empty `fresh`: slice 3's field-fed cores rewrite
    // in methods that never touch a map local.)

    // Phase B — recognize the cores (the adjacency window itself).
    let recognized = recognize_cores(instrs);

    // Phase C — resolve each core's receiver by the backward shape walk:
    // a HashMap-fresh local (slice 2) or a proven static field (slice 3).
    let mut cores: Vec<MapCore> = recognized
        .into_iter()
        .filter_map(|(i, put, prim)| resolve_core(instrs, shapes, i, put, prim, &fresh, proven))
        .collect();

    // Phase D — the span guards (branch targets + handler overlap).
    span_guard_pass(&mut cores, &targets, exception_table);
    if cores.is_empty() {
        return 0;
    }

    // Phase E — consumption closure for the LOCAL receivers.
    consumption_closure(instrs, &mut cores, &fresh);
    if cores.is_empty() {
        return 0;
    }

    // Phase F — splice, descending so the indices stay valid.
    splice_synthetic(instrs, &cores)
}

// --- round-7 file-split pass: verbatim children below ---
mod core;
pub(crate) use core::*;
mod proven;
pub(crate) use proven::*;
mod map;
pub(crate) use map::*;
#[cfg(test)]
mod tests;
