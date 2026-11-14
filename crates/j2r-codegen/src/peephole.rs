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
const BOXED_VALUE_OF: &[(&str, &str, PrimType)] = &[
    (
        "java/lang/Double",
        "(D)Ljava/lang/Double;",
        PrimType::Double,
    ),
    ("java/lang/Float", "(F)Ljava/lang/Float;", PrimType::Float),
    ("java/lang/Integer", "(I)Ljava/lang/Integer;", PrimType::Int),
    ("java/lang/Long", "(J)Ljava/lang/Long;", PrimType::Long),
];

const MAP_RECEIVERS: &[&str] = &["java/util/Map", "java/util/HashMap"];

fn boxed_of(class: &str) -> Option<&'static (&'static str, &'static str, PrimType)> {
    BOXED_VALUE_OF.iter().find(|(c, _, _)| *c == class)
}

fn unbox_prim(name: &str) -> Option<PrimType> {
    match name {
        "doubleValue" => Some(PrimType::Double),
        "floatValue" => Some(PrimType::Float),
        "intValue" => Some(PrimType::Int),
        "longValue" => Some(PrimType::Long),
        _ => None,
    }
}

fn is_const_push(i: &Instr) -> bool {
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
fn is_control(i: &Instr) -> bool {
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

/// One recognized, receiver-resolved core: the instruction INDEX of the
/// core start, the pc of the core start (the synthetic's pc), the pc one
/// past the core's last instruction, the receiver, and the prim.
struct MapCore {
    start_idx: usize,
    start_pc: u32,
    end_pc: u32,
    recv: Recv,
    recv_pc: u32,
    prim: PrimType,
    put: bool,
}

/// The proven provenance of a core's map operand (G3.3 slice 2 + 3):
/// either a HashMap-fresh LOCAL (slice 2's closure law, proven per
/// method) or a static FIELD whose class-level law holds (slice 3:
/// every `PutStatic` is a HashMap creation run — the field's runtime
/// object is always the implemented container — with the field's own
/// generic signature pinning the map family). A field core's prim need
/// not agree with the pin or with its sibling cores: the fused get's
/// unbox throws the faithful CCE on any mismatch, exactly like the
/// faithful `checkcast` it replaces.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Recv {
    Local(u16),
    Field { class: String, field: String },
}

/// The class-level proven static-field set for the getstatic receiver
/// layer: `(class, field) -> pinned prim` for every static field of
/// `cls` whose generic signature pins an exact boxed-primitive map value
/// AND whose class-wide PutStatic pattern proves the provenance law:
/// every store is a HashMap creation run. Reader closure is NOT
/// required — non-core reads (the generic exits) stay faithful through
/// the runtime's lazy materialization and the fused get's unbox law.
pub fn proven_map_fields(
    cls: &crate::ClassPlan,
) -> std::collections::BTreeMap<(String, String), PrimType> {
    proven_map_fields_of(&cls.name, &cls.fields, &cls.methods)
}

/// The free-function form of [`proven_map_fields`] — the same scan over
/// explicitly passed parts, so the unit tests can build the inputs
/// without a full `ClassPlan` (whose classfile field is private).
pub(crate) fn proven_map_fields_of(
    class_name: &str,
    fields: &[crate::FieldPlan],
    methods: &[crate::MethodPlan],
) -> std::collections::BTreeMap<(String, String), PrimType> {
    let mut out = std::collections::BTreeMap::new();
    for f in fields {
        if !f.is_static {
            continue;
        }
        let Some(sig) = &f.signature else { continue };
        let Some(bp) = crate::signature::map_value_boxed(sig) else {
            continue;
        };
        let prim = prim_of_boxed(bp);
        if field_proven(class_name, f, methods) {
            out.insert((class_name.to_string(), f.name.clone()), prim);
        }
    }
    out
}

fn prim_of_boxed(bp: crate::signature::BoxedPrim) -> PrimType {
    match bp {
        crate::signature::BoxedPrim::Double => PrimType::Double,
        crate::signature::BoxedPrim::Float => PrimType::Float,
        crate::signature::BoxedPrim::Integer => PrimType::Int,
        crate::signature::BoxedPrim::Long => PrimType::Long,
    }
}

/// The class-level closure proof for one static field (the scan side of
/// slice 3): PROVENANCE + PIN. The field proves when
///   1. its generic signature pins an exact boxed-primitive map value
///      (the pin gate — raw types refuse), and
///   2. every `PutStatic(f)` in any method of the class stores a
///      `New HashMap; Dup; (consts); Invokespecial <init>` creation run
///      with no branch target inside, and at least one exists — the
///      field's runtime object is always the implemented container.
///
/// There is NO consumption-closure requirement on the field's readers:
/// every non-core read is made faithful at RUNTIME — the generic exits
/// materialize lazily through the node's class tag (get-twice-returns-
/// the-same-ref, the JLS identity caches) and the fused get's unbox
/// throws the faithful CCE on a foreign box or a mismatched prim tag.
/// Any non-creation store refuses the WHOLE field, the faithful path
/// stands everywhere it is used.
fn field_proven(class_name: &str, f: &crate::FieldPlan, methods: &[crate::MethodPlan]) -> bool {
    let mut creation_sites = 0usize;
    for m in methods {
        for i in 0..m.instrs.len() {
            if let Instr::PutStatic { class, field, .. } = &m.instrs[i].1 {
                if class == class_name && *field == f.name {
                    // The branch-target guard needs the method's targets.
                    let mut targets = Vec::new();
                    targets_of(&m.instrs, &mut targets);
                    if !is_creation_putstatic(&m.instrs, i, &targets) {
                        return false;
                    }
                    creation_sites += 1;
                }
            }
        }
    }
    creation_sites > 0
}

/// `New HashMap; Dup; (const pushes); Invokespecial <init>; PutStatic f`
/// with no branch target inside — the PutStatic-side mirror of Phase A's
/// creation run.
fn is_creation_putstatic(instrs: &[(u32, Instr)], i: usize, targets: &[u32]) -> bool {
    if i == 0 {
        return false;
    }
    if !matches!(
        instrs[i - 1].1,
        Instr::InvokeSpecial { ref class, ref name, .. }
            if class == "java/util/HashMap" && name == "<init>"
    ) {
        return false;
    }
    let mut j = i - 1;
    while j > 0 && matches!(instrs[j - 1].1, ref x if is_const_push(x)) {
        j -= 1;
    }
    if j == 0 {
        return false;
    }
    if !matches!(instrs[j - 1].1, Instr::Dup) {
        return false;
    }
    if j < 2 {
        return false;
    }
    if !matches!(
        instrs[j - 2].1,
        Instr::New { ref class } if class == "java/util/HashMap"
    ) {
        return false;
    }
    let start_pc = instrs[j - 2].0;
    let end_pc = instrs.get(i + 1).map(|p| p.0).unwrap_or(u32::MAX);
    !targets.iter().any(|t| *t > start_pc && *t < end_pc)
}

/// Rewrites the specialized-map cores. Returns the number of cores
/// replaced. `shapes` must be the method's verified per-pc shapes (the
/// caller re-verifies after the rewrite; the synthetic instructions have
/// verifier arms). `proven` is the class-level proven static-field set
/// (slice 3) — the getstatic receivers this method's cores may use.
pub fn map_prim_rewrites(
    instrs: &mut Vec<(u32, Instr)>,
    exception_table: &[(u32, u32, u32, Option<String>)],
    shapes: &crate::verify::MethodShapes,
    proven: &std::collections::BTreeMap<(String, String), PrimType>,
) -> usize {
    // Branch targets, for the span guards.
    let mut targets = Vec::new();
    targets_of(instrs, &mut targets);
    targets.sort_unstable();
    targets.dedup();

    // Phase A — HashMap-fresh locals. A creation run is
    // `New HashMap; Dup; (const pushes); Invokespecial HashMap.<init>;
    //  Store(n)` with no branch target inside. Every OTHER Store(n)
    // kills the local's freshness.
    let mut fresh: std::collections::BTreeMap<u16, ()> = Default::default();
    let mut creation_stores: Vec<u32> = Vec::new();
    for i in 0..instrs.len() {
        let Some(store_idx) = (|| {
            if !matches!(&instrs[i].1, Instr::New { class: c } if c == "java/util/HashMap") {
                return None;
            }
            if !matches!(instrs.get(i + 1).map(|p| &p.1), Some(Instr::Dup)) {
                return None;
            }
            let mut j = i + 2;
            while matches!(instrs.get(j).map(|p| &p.1), Some(x) if is_const_push(x)) {
                j += 1;
            }
            match instrs.get(j).map(|p| &p.1) {
                Some(Instr::InvokeSpecial { class, name, .. })
                    if class == "java/util/HashMap" && name == "<init>" => {}
                _ => return None,
            }
            match instrs.get(j + 1).map(|p| &p.1) {
                Some(Instr::Store(n)) => Some((j + 1, *n)),
                _ => None,
            }
        })() else {
            continue;
        };
        let (store_idx, n) = store_idx;
        let start_pc = instrs[i].0;
        let end_pc = instrs.get(store_idx + 1).map(|p| p.0).unwrap_or(u32::MAX);
        if targets.iter().any(|t| *t > start_pc && *t < end_pc) {
            continue;
        }
        creation_stores.push(instrs[store_idx].0);
        fresh.insert(n, ());
    }
    for (pc, i) in instrs.iter() {
        if let Instr::Store(n) = i {
            if !creation_stores.contains(pc) {
                fresh.remove(n);
            }
        }
    }
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

    // Phase D — the span guards (branch targets + handler overlap). A
    // refused core keeps the faithful path, which then OBSERVES the prim
    // slots its sibling rewrites wrote — so a refusal must remove every
    // core of the same receiver local. The closure check below does
    // exactly that: after the guards, the refused core's receiver Load
    // is no longer covered.
    cores.retain(|c| {
        let inside = |t: u32| t > c.recv_pc && t < c.end_pc;
        if targets.iter().any(|t| inside(*t)) {
            return false;
        }
        exception_table.iter().all(|(s, e, _, _)| {
            let overlaps = *s < c.end_pc && *e > c.recv_pc;
            let contains = *s <= c.recv_pc && *e >= c.end_pc;
            !overlaps || contains
        })
    });
    if cores.is_empty() {
        return 0;
    }

    // Phase E — consumption closure for the LOCAL receivers: every
    // Load(n) of a fresh local must be some surviving core's receiver.
    // Any other load (an argument pass, a field store, a return,
    // `keySet()`, a non-core put/get) can observe the map or a value
    // where a BOX is expected. Field receivers are exempt here: their
    // closure was proven class-wide by the proven-field scan.
    let recv_pcs: Vec<u32> = cores.iter().map(|c| c.recv_pc).collect();
    let mut closed: std::collections::BTreeSet<u16> = fresh.keys().copied().collect();
    for (pc, i) in instrs.iter() {
        if let Instr::Load(n) = i {
            if fresh.contains_key(n) && !recv_pcs.contains(pc) {
                closed.remove(n);
            }
        }
    }
    cores.retain(|c| match &c.recv {
        Recv::Local(n) => closed.contains(n),
        Recv::Field { .. } => true,
    });
    if cores.is_empty() {
        return 0;
    }

    // Phase F — splice, descending so the indices stay valid.
    let mut replaced = 0;
    for c in cores.iter().rev() {
        let synthetic = if c.put {
            Instr::MapPutPrim { prim: c.prim }
        } else {
            Instr::MapGetPrim { prim: c.prim }
        };
        instrs.splice(c.start_idx..c.start_idx + 3, vec![(c.start_pc, synthetic)]);
        replaced += 1;
    }
    replaced
}

fn unbox_desc(p: PrimType) -> &'static str {
    match p {
        PrimType::Float => "()F",
        PrimType::Long => "()J",
        PrimType::Double => "()D",
        _ => "()I",
    }
}

/// Phase B — the adjacency recognition (shared by the rewrite pass and
/// the class-level field scan): returns `(index, put, prim)` for every
/// recognized core shape in the method.
///
/// put core: `valueOf(p); Map|HashMap.put; Pop` at (i, i+1, i+2).
/// get core: `Map|HashMap.get; CheckCast Box; Box.xValue()p` at (i..i+2).
fn recognize_cores(instrs: &[(u32, Instr)]) -> Vec<(usize, bool, PrimType)> {
    let mut recognized: Vec<(usize, bool, PrimType)> = Vec::new();
    for i in 0..instrs.len() {
        match &instrs[i].1 {
            // put core: `valueOf(p); Map|HashMap.put; Pop`.
            Instr::InvokeStatic { class, name, desc } if name == "valueOf" => {
                let Some((_, d, p)) = boxed_of(class) else {
                    continue;
                };
                if desc != *d {
                    continue;
                }
                match (
                    instrs.get(i + 1).map(|x| &x.1),
                    instrs.get(i + 2).map(|x| &x.1),
                ) {
                    (
                        Some(Instr::InvokeVirtual { class, name, desc })
                        | Some(Instr::InvokeInterface { class, name, desc }),
                        Some(Instr::Pop),
                    ) if name == "put"
                        && MAP_RECEIVERS.contains(&class.as_str())
                        && desc == "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;" => {}
                    _ => continue,
                }
                recognized.push((i, true, *p));
            }
            // get core: `Map|HashMap.get; CheckCast Box; Box.xValue()p`.
            Instr::InvokeVirtual { class, name, desc }
            | Instr::InvokeInterface { class, name, desc }
                if name == "get"
                    && MAP_RECEIVERS.contains(&class.as_str())
                    && desc == "(Ljava/lang/Object;)Ljava/lang/Object;" =>
            {
                let Some(Instr::CheckCast { class: box_cls }) = instrs.get(i + 1).map(|x| &x.1)
                else {
                    continue;
                };
                let Some((bc, _, _)) = boxed_of(box_cls) else {
                    continue;
                };
                let Some(Instr::InvokeVirtual {
                    class: ucls,
                    name: uname,
                    desc: udesc,
                }) = instrs.get(i + 2).map(|x| &x.1)
                else {
                    continue;
                };
                if ucls != *bc {
                    continue;
                }
                let Some(p) = unbox_prim(uname) else { continue };
                if udesc != unbox_desc(p) {
                    continue;
                }
                recognized.push((i, false, p));
            }
            _ => {}
        }
    }
    recognized
}

/// Resolves one core's receiver by the backward shape walk (Phase C) and
/// returns the core when the receiver is a HashMap-fresh `Load(n)`
/// (slice 2) or a proven static field's `GetStatic` (slice 3, prim
/// matching the proven set's pin).
fn resolve_core(
    instrs: &[(u32, Instr)],
    shapes: &crate::verify::MethodShapes,
    i: usize,
    put: bool,
    prim: PrimType,
    fresh: &std::collections::BTreeMap<u16, ()>,
    proven: &std::collections::BTreeMap<(String, String), PrimType>,
) -> Option<MapCore> {
    // The consuming instruction: the put (i+1) for a put core, the get
    // (i) for a get core. The map operand's slot comes from its ENTRY
    // shape.
    let consume_idx = if put { i + 1 } else { i };
    let consume_pc = instrs[consume_idx].0;
    let shape = shapes.per_pc.get(&consume_pc)?;
    let map_slot = shape.stack.len().checked_sub(if put { 3 } else { 2 })?;
    let mut recv = None;
    let mut recv_pc = None;
    for j in (0..i).rev() {
        if is_control(&instrs[j].1) {
            return None;
        }
        let s = shapes.per_pc.get(&instrs[j].0)?;
        let depth = s.stack.len();
        if depth == map_slot {
            match &instrs[j].1 {
                Instr::Load(n) if fresh.contains_key(n) => {
                    recv = Some(Recv::Local(*n));
                    recv_pc = Some(instrs[j].0);
                }
                Instr::GetStatic { class, field, .. }
                    if proven.contains_key(&(class.clone(), field.clone())) =>
                {
                    recv = Some(Recv::Field {
                        class: class.clone(),
                        field: field.clone(),
                    });
                    recv_pc = Some(instrs[j].0);
                }
                _ => return None,
            }
            break;
        }
        if depth < map_slot {
            return None;
        }
    }
    Some(MapCore {
        start_idx: i,
        start_pc: instrs[i].0,
        end_pc: instrs.get(i + 3).map(|p| p.0).unwrap_or(u32::MAX),
        recv: recv?,
        recv_pc: recv_pc?,
        prim,
        put,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Instr as I;

    /// The canonical `Map<String,Double> m = new HashMap<>(); m.put("a",
    /// 0.5); double x = m.get("a");` main body (javac 21 shape, realistic
    /// pcs): creation store to local 1, put core at 13, get core at 26.
    fn canonical() -> Vec<(u32, I)> {
        vec![
            (
                0,
                I::New {
                    class: "java/util/HashMap".into(),
                },
            ),
            (3, I::Dup),
            (
                4,
                I::InvokeSpecial {
                    class: "java/util/HashMap".into(),
                    name: "<init>".into(),
                    desc: "()V".into(),
                },
            ),
            (7, I::Store(1)),
            (8, I::Load(1)),
            (9, I::Ldc(Ldc::String("a".into()))),
            (10, I::Ldc2W(crate::ir::Ldc2::Double(0.5))),
            (
                13,
                I::InvokeStatic {
                    class: "java/lang/Double".into(),
                    name: "valueOf".into(),
                    desc: "(D)Ljava/lang/Double;".into(),
                },
            ),
            (
                16,
                I::InvokeInterface {
                    class: "java/util/Map".into(),
                    name: "put".into(),
                    desc: "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;".into(),
                },
            ),
            (21, I::Pop),
            (22, I::Load(1)),
            (23, I::Ldc(Ldc::String("a".into()))),
            (
                26,
                I::InvokeInterface {
                    class: "java/util/Map".into(),
                    name: "get".into(),
                    desc: "(Ljava/lang/Object;)Ljava/lang/Object;".into(),
                },
            ),
            (
                31,
                I::CheckCast {
                    class: "java/lang/Double".into(),
                },
            ),
            (
                34,
                I::InvokeVirtual {
                    class: "java/lang/Double".into(),
                    name: "doubleValue".into(),
                    desc: "()D".into(),
                },
            ),
            (37, I::Store(2)),
            (38, I::Return(j2r_runtime::TypeDesc::Void)),
        ]
    }

    fn shapes_of(instrs: &[(u32, I)]) -> crate::verify::MethodShapes {
        let m = crate::MethodPlan {
            name: "main".into(),
            desc: "([Ljava/lang/String;)V".into(),
            is_static: true,
            is_native: false,
            is_sync: false,
            modifiers: 0,
            signature: None,
            lvtt: Vec::new(),
            fn_name: "m_main".into(),
            instrs: instrs.to_vec(),
            exception_table: Vec::new(),
            max_locals: 4,
            max_stack: 6,
        };
        let concats = std::collections::BTreeMap::new();
        let lambdas = std::collections::BTreeMap::new();
        crate::verify::verify_method_shapes(&m, &concats, &lambdas).expect("test body verifies")
    }

    #[test]
    fn put_and_get_cores_rewrite() {
        let mut instrs = canonical();
        let shapes = shapes_of(&instrs);
        let n = map_prim_rewrites(&mut instrs, &[], &shapes, &Default::default());
        assert_eq!(n, 2, "both cores rewrite");
        assert_eq!(
            instrs[7].1,
            I::MapPutPrim {
                prim: PrimType::Double
            },
            "the put core collapses at its valueOf pc"
        );
        assert_eq!(instrs[7].0, 13);
        assert_eq!(
            instrs[10].1,
            I::MapGetPrim {
                prim: PrimType::Double
            },
            "the get core collapses at its get pc"
        );
        assert_eq!(instrs[10].0, 26);
    }

    #[test]
    fn non_creation_store_refuses() {
        let mut instrs = canonical();
        // A second store to the map local that is not a fresh creation:
        // the local's provenance is no longer provable. Inserted directly
        // after the creation store (index 3); the rest re-spaces by +2 so
        // every pc stays distinct.
        instrs.insert(4, (8, I::AConstNull));
        instrs.insert(5, (9, I::Store(1)));
        for pc_entry in instrs.iter_mut().skip(6) {
            pc_entry.0 += 2;
        }
        let shapes = shapes_of(&instrs);
        let n = map_prim_rewrites(&mut instrs, &[], &shapes, &Default::default());
        assert_eq!(n, 0, "a non-creation store kills freshness");
    }

    #[test]
    fn escape_load_refuses_closure() {
        let mut instrs = canonical();
        // A load of the map local consumed by something that is not a
        // rewritten core's receiver (here: pushed and discarded — the
        // argument-pass / field-store shape class).
        instrs.push((39, I::Load(1)));
        instrs.push((40, I::Pop));
        instrs.push((41, I::Return(j2r_runtime::TypeDesc::Void)));
        let shapes = shapes_of(&instrs);
        let n = map_prim_rewrites(&mut instrs, &[], &shapes, &Default::default());
        assert_eq!(n, 0, "an uncovered load kills the whole local");
    }

    #[test]
    fn non_core_get_refuses_closure() {
        let mut instrs = canonical();
        // `Object o = m.get("a")` discarded: the get's result is NOT
        // consumed by checkcast+unbox, so no get core forms and the
        // receiver load is uncovered. Keep the creation + the put core,
        // then a bare get whose result is popped.
        instrs.truncate(10);
        instrs.push((22, I::Load(1)));
        instrs.push((23, I::Ldc(Ldc::String("a".into()))));
        instrs.push((
            26,
            I::InvokeInterface {
                class: "java/util/Map".into(),
                name: "get".into(),
                desc: "(Ljava/lang/Object;)Ljava/lang/Object;".into(),
            },
        ));
        instrs.push((31, I::Pop));
        instrs.push((32, I::Return(j2r_runtime::TypeDesc::Void)));
        let shapes = shapes_of(&instrs);
        let n = map_prim_rewrites(&mut instrs, &[], &shapes, &Default::default());
        assert_eq!(n, 0, "a non-core get refuses the closure");
    }

    #[test]
    fn partial_handler_overlap_refuses() {
        let instrs = canonical();
        let shapes = shapes_of(&instrs);
        let mut instrs2 = instrs.clone();
        // A handler that ENDS between the valueOf and the put: the
        // synthetic faults at the core's first pc, so a boundary inside
        // the span can move a throw across it.
        let table = vec![(8u32, 17u32, 38u32, None)];
        let n = map_prim_rewrites(&mut instrs2, &table, &shapes, &Default::default());
        assert_eq!(n, 0, "a handler boundary inside the span refuses");
        // A handler covering the WHOLE span is fine.
        let mut instrs3 = instrs;
        let table2 = vec![(8u32, 22u32, 38u32, None)];
        let n2 = map_prim_rewrites(&mut instrs3, &table2, &shapes, &Default::default());
        assert_eq!(n2, 2, "a full-span handler keeps both cores");
    }

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

    // ---- G3.3 slice 3: the getstatic receivers (field/materialization) ----

    fn mp(name: &str, instrs: Vec<(u32, I)>) -> crate::MethodPlan {
        crate::MethodPlan {
            name: name.into(),
            desc: "()V".into(),
            is_static: true,
            is_native: false,
            is_sync: false,
            modifiers: 0,
            signature: None,
            lvtt: Vec::new(),
            fn_name: format!("m_{name}"),
            instrs,
            exception_table: Vec::new(),
            max_locals: 4,
            max_stack: 6,
        }
    }

    fn map_field(sig: &str) -> crate::FieldPlan {
        crate::FieldPlan {
            name: "m".into(),
            desc: "Ljava/util/Map;".into(),
            is_static: true,
            static_slot: Some(0),
            instance_slot: None,
            constant_value: None,
            is_volatile: false,
            is_final: false,
            modifiers: 0,
            signature: Some(sig.into()),
        }
    }

    const SIG_DOUBLE: &str = "Ljava/util/Map<Ljava/lang/String;Ljava/lang/Double;>;";

    /// The scan fixture (the Probe3 javap shapes): creation in <clinit>,
    /// the put + get cores in main, all through the static field `m`.
    fn field_cls_methods() -> Vec<crate::MethodPlan> {
        vec![
            mp(
                "<clinit>",
                vec![
                    (
                        0,
                        I::New {
                            class: "java/util/HashMap".into(),
                        },
                    ),
                    (3, I::Dup),
                    (
                        4,
                        I::InvokeSpecial {
                            class: "java/util/HashMap".into(),
                            name: "<init>".into(),
                            desc: "()V".into(),
                        },
                    ),
                    (
                        7,
                        I::PutStatic {
                            class: "T".into(),
                            field: "m".into(),
                            desc: "Ljava/util/Map;".into(),
                        },
                    ),
                    (10, I::Return(j2r_runtime::TypeDesc::Void)),
                ],
            ),
            mp(
                "main",
                vec![
                    (
                        0,
                        I::GetStatic {
                            class: "T".into(),
                            field: "m".into(),
                            desc: "Ljava/util/Map;".into(),
                        },
                    ),
                    (3, I::Ldc(Ldc::String("a".into()))),
                    (6, I::Ldc2W(crate::ir::Ldc2::Double(0.5))),
                    (
                        9,
                        I::InvokeStatic {
                            class: "java/lang/Double".into(),
                            name: "valueOf".into(),
                            desc: "(D)Ljava/lang/Double;".into(),
                        },
                    ),
                    (
                        12,
                        I::InvokeInterface {
                            class: "java/util/Map".into(),
                            name: "put".into(),
                            desc: "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;".into(),
                        },
                    ),
                    (17, I::Pop),
                    (
                        18,
                        I::GetStatic {
                            class: "T".into(),
                            field: "m".into(),
                            desc: "Ljava/util/Map;".into(),
                        },
                    ),
                    (21, I::Ldc(Ldc::String("a".into()))),
                    (
                        24,
                        I::InvokeInterface {
                            class: "java/util/Map".into(),
                            name: "get".into(),
                            desc: "(Ljava/lang/Object;)Ljava/lang/Object;".into(),
                        },
                    ),
                    (
                        29,
                        I::CheckCast {
                            class: "java/lang/Double".into(),
                        },
                    ),
                    (
                        32,
                        I::InvokeVirtual {
                            class: "java/lang/Double".into(),
                            name: "doubleValue".into(),
                            desc: "()D".into(),
                        },
                    ),
                    (35, I::Store(2)),
                    (36, I::Return(j2r_runtime::TypeDesc::Void)),
                ],
            ),
        ]
    }

    #[test]
    fn scan_proves_field_and_rewrite_fires() {
        let methods = field_cls_methods();
        let fields = vec![map_field(SIG_DOUBLE)];
        let proven = proven_map_fields_of("T", &fields, &methods);
        assert_eq!(proven.len(), 1, "the field proves");
        assert_eq!(
            proven[&("T".to_string(), "m".to_string())],
            PrimType::Double
        );
        // The rewrite side: both field-fed cores collapse (the synthetic
        // lands at the valueOf pc / the get pc).
        let mut instrs = methods[1].instrs.clone();
        let shapes = shapes_of(&instrs);
        let n = map_prim_rewrites(&mut instrs, &[], &shapes, &proven);
        assert_eq!(n, 2, "both field-fed cores rewrite");
        assert_eq!(
            instrs[3].1,
            I::MapPutPrim {
                prim: PrimType::Double
            }
        );
        assert_eq!(instrs[3].0, 9);
        assert_eq!(
            instrs[6].1,
            I::MapGetPrim {
                prim: PrimType::Double
            }
        );
        assert_eq!(instrs[6].0, 24);
    }

    #[test]
    fn scan_allows_generic_exit_and_mixed_prims() {
        // The probe's P4 shape plus a prim-mismatched core: with the
        // runtime's lazy materialization and the fused get's unbox, the
        // generic exit stays faithful and the mismatched core throws the
        // faithful CCE — so the scan REQUIRES nothing of the readers.
        let mut methods = field_cls_methods();
        methods.push(mp(
            "exit",
            vec![
                (
                    0,
                    I::GetStatic {
                        class: "T".into(),
                        field: "m".into(),
                        desc: "Ljava/util/Map;".into(),
                    },
                ),
                (3, I::Ldc(Ldc::String("a".into()))),
                (
                    6,
                    I::InvokeInterface {
                        class: "java/util/Map".into(),
                        name: "get".into(),
                        desc: "(Ljava/lang/Object;)Ljava/lang/Object;".into(),
                    },
                ),
                (
                    11,
                    I::Return(j2r_runtime::TypeDesc::Object("java/lang/Object".into())),
                ),
            ],
        ));
        // A prim-mismatched get core (checkcast Integer) on the same
        // field: the fused unbox carries the faithful CCE.
        let mut mixed = methods[1].instrs.clone();
        mixed[9] = (
            29,
            I::CheckCast {
                class: "java/lang/Integer".into(),
            },
        );
        mixed[10] = (
            32,
            I::InvokeVirtual {
                class: "java/lang/Integer".into(),
                name: "intValue".into(),
                desc: "()I".into(),
            },
        );
        mixed[11] = (35, I::Store(2));
        methods[1].instrs = mixed;
        let fields = vec![map_field(SIG_DOUBLE)];
        let proven = proven_map_fields_of("T", &fields, &methods);
        assert_eq!(
            proven.len(),
            1,
            "the generic exit and the mismatched core do not refuse the field"
        );
        // Both cores of the mixed method rewrite (the mismatch's CCE is
        // a runtime law, carried by the fused unbox).
        let mut instrs = methods[1].instrs.clone();
        let shapes = shapes_of(&instrs);
        assert_eq!(map_prim_rewrites(&mut instrs, &[], &shapes, &proven), 2);
        assert!(matches!(
            instrs[6].1,
            I::MapGetPrim {
                prim: PrimType::Int
            }
        ));
    }

    #[test]
    fn scan_refuses_non_creation_putstatic() {
        let mut methods = field_cls_methods();
        // `m = null;` — a non-creation store kills the provenance.
        methods.push(mp(
            "reassign",
            vec![
                (0, I::AConstNull),
                (
                    1,
                    I::PutStatic {
                        class: "T".into(),
                        field: "m".into(),
                        desc: "Ljava/util/Map;".into(),
                    },
                ),
                (4, I::Return(j2r_runtime::TypeDesc::Void)),
            ],
        ));
        let fields = vec![map_field(SIG_DOUBLE)];
        assert!(proven_map_fields_of("T", &fields, &methods).is_empty());
    }

    #[test]
    fn scan_refuses_without_creation() {
        // The use-site method alone: cores but NO PutStatic anywhere, the
        // provenance is unprovable.
        let methods = vec![field_cls_methods().remove(1)];
        let fields = vec![map_field(SIG_DOUBLE)];
        assert!(proven_map_fields_of("T", &fields, &methods).is_empty());
    }

    #[test]
    fn getstatic_receiver_rewrites_only_when_proven() {
        let instrs_for = || {
            vec![
                (
                    0,
                    I::GetStatic {
                        class: "T".into(),
                        field: "m".into(),
                        desc: "Ljava/util/Map;".into(),
                    },
                ),
                (3, I::Ldc(Ldc::String("a".into()))),
                (6, I::Ldc2W(crate::ir::Ldc2::Double(0.5))),
                (
                    9,
                    I::InvokeStatic {
                        class: "java/lang/Double".into(),
                        name: "valueOf".into(),
                        desc: "(D)Ljava/lang/Double;".into(),
                    },
                ),
                (
                    12,
                    I::InvokeInterface {
                        class: "java/util/Map".into(),
                        name: "put".into(),
                        desc: "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;".into(),
                    },
                ),
                (17, I::Pop),
                (18, I::Return(j2r_runtime::TypeDesc::Void)),
            ]
        };
        let mut proven = std::collections::BTreeMap::new();
        proven.insert(("T".to_string(), "m".to_string()), PrimType::Double);
        let mut instrs = instrs_for();
        let shapes = shapes_of(&instrs);
        assert_eq!(
            map_prim_rewrites(&mut instrs, &[], &shapes, &proven),
            1,
            "the proven field receiver rewrites"
        );
        assert_eq!(
            instrs[3].1,
            I::MapPutPrim {
                prim: PrimType::Double
            }
        );
        // Unproven: the faithful path stands.
        let mut instrs2 = instrs_for();
        let shapes2 = shapes_of(&instrs2);
        assert_eq!(
            map_prim_rewrites(&mut instrs2, &[], &shapes2, &Default::default()),
            0,
            "an unproven field receiver refuses"
        );
    }
}
