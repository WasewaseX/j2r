//! `peephole`::core — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use crate::ir::{ElemKind, Instr, Ldc, PrimType};

/// Minimum run size worth collapsing. Below this the old per-element
/// form is harmless and the generated diff stays minimal.
pub(crate) const MIN_FILL: i32 = 8;

/// The int-group prims a literal fill may target: everything whose
/// runtime payload is `PrimData::I` and whose `iastore`-family opcode
/// is `AStore(ElemKind::IntGroup)` — i.e. exactly `array_store_i_raw`'s
/// descriptor table.
pub(crate) fn int_group_prim(p: PrimType) -> bool {
    matches!(
        p,
        PrimType::Boolean | PrimType::Char | PrimType::Byte | PrimType::Short | PrimType::Int
    )
}

pub(crate) fn const_int(i: &Instr) -> Option<i32> {
    match i {
        Instr::IConst(v) => Some(*v),
        Instr::Ldc(Ldc::Int(v)) => Some(*v),
        _ => None,
    }
}

/// Every pc a branch or exception handler can transfer control to.
pub(crate) fn targets_of(instrs: &[(u32, Instr)], out: &mut Vec<u32>) {
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

// ---------------------------------------------------------------------------
// G3.3 slice 2 — the specialized-map cores (the 2.78x container tax,
// evidence g3_3/opening_census). The registered design's step 2: rewrite
// javac's autobox map chains into the prim-slot natives where a dataflow
// window proves the box never escapes.
//
// THE RECOGNIZED CORES (javac 8-21, `Map<String,Double> m`):
//
//   put:  InvokeStatic  Box.valueOf(p)          <- the fresh box
//         Invoke{V,I}    Map|HashMap.put         (LL;LL;)LL;
//         Pop                                    <- the return discarded
//   =>    MapPutPrim { prim: p }                  (pops map,key,prim)
//
//   get:  Invoke{V,I}    Map|HashMap.get         (L;)L;
//         CheckCast      java/lang/Box
//         InvokeVirtual  Box.xValue()p           <- the only consumer
//   =>    MapGetPrim { prim: p }                  (pops map,key; pushes prim)
//
// THE CLOSURE LAW (why the raw prim slot is unobservable there). The
// rewrites fire only on a receiver LOCAL n that is:
//   1. HASHMAP-FRESH: every `Store(n)` in the method stores a
//      `New java/util/HashMap; Dup; (consts); Invokespecial <init>`
//      creation, so the runtime object is the implemented container
//      (the faithful dispatch would resolve there anyway);
//   2. CONSUMPTION-CLOSED: EVERY `Load(n)` in the method is the receiver
//      of a rewritten core. Any other load — an argument pass, a field
//      store, a return, `keySet()`, a non-core `put`/`get` — means the
//      map (or a value read) can be observed where a BOX is expected,
//      so n is refused entirely and its cores keep the faithful path.
//      (`m.put(m = other, v)` reassignment stores are non-creation
//      stores and kill freshness by rule 1 — conservative; the synthetic
//      reads the STACK operand, so the rule is not strictly needed, but
//      the simpler law is kept until a case needs the wider one.)
// Under both rules every slot in the map was written by a rewritten put
// (a raw prim) and every read is a rewritten get (prim expect), so the
// box never materializes. The heap-pollution CCE the faithful
// `checkcast` would throw is unreachable by the same construction.
//
// THE DATAFLOW WINDOW. The receiver is resolved by walking BACKWARD from
// the core through the verifier's per-pc entry shapes: the first
// instruction whose entry stack depth equals the map operand's slot is
// the push that produced it — and it must be `Load(n)`. (No width table:
// the verifier's shapes already encode category-2 widths.) The walk
// refuses on any branch/switch/return/athrow (the window must be
// straight-line) and on any entry depth below the map slot. Guards
// mirror the fill pass: no branch target may land in
// `(receiver_pc, end_pc)` (a target AT the receiver pc re-executes
// `Load(n)` — the same provenance; a target at or after the core start
// could supply a foreign map), and no exception entry may PARTIALLY
// overlap the span (the synthetic's faults all report the core's first
// pc, so a handler must cover the whole span or none of it).
//
// Every refused shape keeps the old per-instruction emission —
// correctness never depends on the rewrite (the fill pass's law).

/// The boxed classes whose `valueOf`/`xValue()` pair javac emits, with
/// the prim each direction carries.
pub(crate) const BOXED_VALUE_OF: &[(&str, &str, PrimType)] = &[
    (
        "java/lang/Double",
        "(D)Ljava/lang/Double;",
        PrimType::Double,
    ),
    ("java/lang/Float", "(F)Ljava/lang/Float;", PrimType::Float),
    ("java/lang/Integer", "(I)Ljava/lang/Integer;", PrimType::Int),
    ("java/lang/Long", "(J)Ljava/lang/Long;", PrimType::Long),
];

pub(crate) const MAP_RECEIVERS: &[&str] = &[
    "java/util/Map",
    "java/util/HashMap",
    // G3.3 slice 4: the rewrites ride both new storages. The
    // LinkedHashMap entry is Comparable-key-safe and the TreeMap put
    // self-compares (the addEntryToEmptyMap law) — the receiver-class
    // check only opens the ADJACENCY window; the receiver resolution
    // below still demands a proven creation run (fresh local or proven
    // field), so a TreeMap-typed receiver the scan never proved keeps
    // the faithful path.
    "java/util/TreeMap",
    "java/util/LinkedHashMap",
];

/// The storage kind a creation-run class proves (G3.3 slice 4). The
/// LHM 3-arg access-order ctor is NOT here — see the creation-run
/// matcher's refusal below.
pub(crate) fn kind_of_class(class: &str) -> Option<crate::ir::MapKind> {
    match class {
        "java/util/HashMap" => Some(crate::ir::MapKind::HashMap),
        "java/util/LinkedHashMap" => Some(crate::ir::MapKind::LinkedHashMap),
        "java/util/TreeMap" => Some(crate::ir::MapKind::TreeMap),
        _ => None,
    }
}

pub(crate) fn boxed_of(class: &str) -> Option<&'static (&'static str, &'static str, PrimType)> {
    BOXED_VALUE_OF.iter().find(|(c, _, _)| *c == class)
}

pub(crate) fn unbox_prim(name: &str) -> Option<PrimType> {
    match name {
        "doubleValue" => Some(PrimType::Double),
        "floatValue" => Some(PrimType::Float),
        "intValue" => Some(PrimType::Int),
        "longValue" => Some(PrimType::Long),
        _ => None,
    }
}

pub(crate) fn is_const_push(i: &Instr) -> bool {
    matches!(
        i,
        Instr::IConst(_)
            | Instr::Ldc(Ldc::Int(_))
            | Instr::FConst(_)
            | Instr::Ldc(Ldc::Float(_))
            | Instr::Ldc2W(_)
    )
}

/// Instructions the receiver walk must not cross (the window is
/// straight-line reachable code).
pub(crate) fn is_control(i: &Instr) -> bool {
    matches!(
        i,
        Instr::Goto(_)
            | Instr::IfZero(_, _)
            | Instr::IfICmp(_, _)
            | Instr::IfRefCmp { .. }
            | Instr::IfNull(_)
            | Instr::IfNonNull(_)
            | Instr::TableSwitch { .. }
            | Instr::LookupSwitch { .. }
            | Instr::Return(_)
            | Instr::AThrow
    )
}
