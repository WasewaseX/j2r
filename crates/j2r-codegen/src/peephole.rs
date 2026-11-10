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

use crate::ir::{ElemKind, Instr, Ldc, PrimType};

/// Minimum run size worth collapsing. Below this the old per-element
/// form is harmless and the generated diff stays minimal.
const MIN_FILL: i32 = 8;

/// The int-group prims a literal fill may target: everything whose
/// runtime payload is `PrimData::I` and whose `iastore`-family opcode
/// is `AStore(ElemKind::IntGroup)` — i.e. exactly `array_store_i_raw`'s
/// descriptor table.
fn int_group_prim(p: PrimType) -> bool {
    matches!(
        p,
        PrimType::Boolean | PrimType::Char | PrimType::Byte | PrimType::Short | PrimType::Int
    )
}

fn const_int(i: &Instr) -> Option<i32> {
    match i {
        Instr::IConst(v) => Some(*v),
        Instr::Ldc(Ldc::Int(v)) => Some(*v),
        _ => None,
    }
}

/// Every pc a branch or exception handler can transfer control to.
fn targets_of(instrs: &[(u32, Instr)], out: &mut Vec<u32>) {
    for (_, i) in instrs {
        match i {
            Instr::IfZero(_, t) | Instr::IfICmp(_, t) => out.push(*t),
            Instr::IfRefCmp { target: t, .. } => out.push(*t),
            Instr::IfNull(t) | Instr::IfNonNull(t) | Instr::Goto(t) => out.push(*t),
            Instr::TableSwitch {
                default, targets, ..
            } => {
                out.push(*default);
                out.extend_from_slice(targets);
            }
            Instr::LookupSwitch { default, pairs } => {
                out.push(*default);
                for (_, t) in pairs {
                    out.push(*t);
                }
            }
            _ => {}
        }
    }
}

/// Collapse javac's array-literal fill runs. Returns the number of runs
/// replaced. The recognized shape (javac 8–21, `static int[] a = {...}`):
///
/// ```text
/// const(len); NewArray(Int); ( Dup; const(idx); const(val); AStore(IntGroup) )*
/// ```
///
/// with `idx` ascending from 0 and exactly `len` groups. The run is
/// replaced with one [`Instr::NewIntArrayFilled`] at the length const's
/// pc; everything after (typically `putstatic`) sees the identical stack.
pub fn collapse_array_fill_runs(instrs: &mut Vec<(u32, Instr)>) -> usize {
    let mut targets = Vec::new();
    targets_of(instrs, &mut targets);
    targets.sort_unstable();
    targets.dedup();

    let mut replaced = 0;
    let mut i = 0usize;
    while i < instrs.len() {
        let len = match const_int(&instrs[i].1) {
            Some(v) => v,
            None => {
                i += 1;
                continue;
            }
        };
        let prim = match instrs.get(i + 1).map(|p| &p.1) {
            Some(Instr::NewArray(p)) if int_group_prim(*p) => *p,
            _ => {
                i += 1;
                continue;
            }
        };
        let mut j = i + 2;
        let mut vals: Vec<i32> = Vec::new();
        loop {
            if !matches!(instrs.get(j).map(|p| &p.1), Some(Instr::Dup)) {
                break;
            }
            let idx = instrs.get(j + 1).and_then(|p| const_int(&p.1)).or(None);
            let Some(idx) = idx else { break };
            if idx != vals.len() as i32 {
                break;
            }
            let Some(val) = instrs.get(j + 2).and_then(|p| const_int(&p.1)) else {
                break;
            };
            if !matches!(
                instrs.get(j + 3).map(|p| &p.1),
                Some(Instr::AStore(ElemKind::IntGroup))
            ) {
                break;
            }
            vals.push(val);
            j += 4;
        }
        if (vals.len() as i32) != len || len < MIN_FILL {
            i += 1;
            continue;
        }
        let start_pc = instrs[i].0;
        let end_pc = instrs.get(j).map(|p| p.0).unwrap_or(u32::MAX);
        // Guard: no branch/handler target may land INSIDE the run (a jump
        // into the middle would change behavior). Targets at start_pc
        // (lands on the replacement) and end_pc (the instruction after the
        // run, untouched) are fine.
        if targets.iter().any(|t| *t > start_pc && *t < end_pc) {
            i += 1;
            continue;
        }
        instrs.splice(
            i..j,
            vec![(start_pc, Instr::NewIntArrayFilled { prim, vals })],
        );
        replaced += 1;
        i += 1;
    }
    replaced
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Instr as I;

    fn fill_run(len: i32, vals: &[i32], putstatic: bool) -> Vec<(u32, I)> {
        let mut v: Vec<(u32, I)> = Vec::new();
        let mut pc = 0u32;
        v.push((pc, I::IConst(len)));
        pc += 1;
        v.push((pc, I::NewArray(PrimType::Int)));
        pc += 1;
        for (idx, val) in vals.iter().enumerate() {
            v.push((pc, I::Dup));
            pc += 1;
            v.push((pc, I::IConst(idx as i32)));
            pc += 1;
            v.push((pc, I::IConst(*val)));
            pc += 1;
            v.push((pc, I::AStore(ElemKind::IntGroup)));
            pc += 1;
        }
        if putstatic {
            v.push((
                pc,
                I::PutStatic {
                    class: "T".into(),
                    field: "a".into(),
                    desc: "[I".into(),
                },
            ));
        }
        v
    }

    #[test]
    fn collapses_a_literal_fill_run() {
        let vals: Vec<i32> = (0..32).map(|k| k * 7 - 3).collect();
        let mut instrs = fill_run(32, &vals, true);
        let n = collapse_array_fill_runs(&mut instrs);
        assert_eq!(n, 1);
        assert_eq!(instrs.len(), 2); // fill + putstatic
        assert_eq!(instrs[0].0, 0);
        match &instrs[0].1 {
            I::NewIntArrayFilled { vals: got, .. } => assert_eq!(got, &vals),
            other => panic!("expected NewIntArrayFilled, got {other:?}"),
        }
        assert!(matches!(instrs[1].1, I::PutStatic { .. }));
    }

    #[test]
    fn refuses_when_group_count_differs_from_len() {
        // len says 10 but only 8 stores follow (partial fill elsewhere)
        let mut instrs = fill_run(10, &(0..8).collect::<Vec<_>>(), true);
        assert_eq!(collapse_array_fill_runs(&mut instrs), 0);
        assert_eq!(instrs.len(), 2 + 8 * 4 + 1);
    }

    #[test]
    fn refuses_below_threshold() {
        let vals = vec![1i32; 4];
        let mut instrs = fill_run(4, &vals, true);
        assert_eq!(collapse_array_fill_runs(&mut instrs), 0);
    }

    #[test]
    fn refuses_interior_branch_target() {
        // a goto into the middle of the run: index 1's dup — must not collapse
        let mut instrs = fill_run(16, &(0..16).collect::<Vec<_>>(), true);
        // find the pc of the second group's dup (pc 2 + 4 + 1)
        let mid_pc = 2 + 4 + 1;
        instrs.push((mid_pc + 40, I::Goto(mid_pc))); // jump INTO the run
        assert_eq!(collapse_array_fill_runs(&mut instrs), 0);
    }

    #[test]
    fn allows_target_at_run_start_and_after() {
        let vals: Vec<i32> = vec![5i32; 12];
        let mut instrs = fill_run(12, &vals, true);
        // branch to the run start (pc 0) and to the putstatic (pc 50): legal
        instrs.push((50 + 10, I::Goto(0)));
        instrs.push((50 + 20, I::Goto(50)));
        assert_eq!(collapse_array_fill_runs(&mut instrs), 1);
    }

    #[test]
    fn collapses_short_and_byte_fills_too() {
        // [S fill: same group shape, NewArray(Short) — StaticTree's shape
        let mut v: Vec<(u32, I)> = Vec::new();
        v.push((0, I::IConst(9)));
        v.push((1, I::NewArray(PrimType::Short)));
        for k in 0..9u32 {
            v.push((2 + k * 4, I::Dup));
            v.push((3 + k * 4, I::IConst(k as i32)));
            v.push((4 + k * 4, I::IConst(300 + k as i32)));
            v.push((5 + k * 4, I::AStore(ElemKind::IntGroup)));
        }
        v.push((
            38,
            I::PutStatic {
                class: "T".into(),
                field: "s".into(),
                desc: "[S".into(),
            },
        ));
        assert_eq!(collapse_array_fill_runs(&mut v), 1);
        assert!(matches!(
            &v[0].1,
            I::NewIntArrayFilled {
                prim: PrimType::Short,
                ..
            }
        ));
    }

    #[test]
    fn leaves_non_int_shape_untouched() {
        // long store into [J is AStore(ElemKind::Long) — v1 collapses [I only
        let vals = vec![1i32; 8];
        let mut instrs = fill_run(8, &vals, true);
        // corrupt one store to Ref — shape broken
        instrs[2 + 4].1 = I::AStore(ElemKind::Ref);
        assert_eq!(collapse_array_fill_runs(&mut instrs), 0);
    }

    #[test]
    fn refuses_long_array_fill_v1() {
        // [J uses AStore(ElemKind::Long) — not an int-group store
        let vals = vec![1i32; 8];
        let mut instrs = fill_run(8, &vals, true);
        instrs[1].1 = I::NewArray(PrimType::Long);
        for k in 0..8i32 {
            instrs[2 + k as usize * 4 + 3].1 = I::AStore(ElemKind::Long);
        }
        assert_eq!(collapse_array_fill_runs(&mut instrs), 0);
    }

    #[test]
    fn collapses_two_runs_in_one_method() {
        let mut instrs = fill_run(8, &[1i32; 8], false);
        let tail_start = instrs.last().unwrap().0 + 1;
        let mut second = fill_run(9, &[2i32; 9], true);
        for (pc, _) in second.iter_mut() {
            *pc += tail_start;
        }
        let putstatic_pc = second.last().unwrap().0;
        instrs.extend(second);
        // branch target AFTER everything: must not block either run
        instrs.push((putstatic_pc + 10, I::Return(j2r_runtime::TypeDesc::Void)));
        assert_eq!(collapse_array_fill_runs(&mut instrs), 2);
    }
}
