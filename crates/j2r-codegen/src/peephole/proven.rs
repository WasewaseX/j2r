//! `peephole`::proven — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use crate::ir::{Instr, PrimType};

/// One recognized, receiver-resolved core: the instruction INDEX of the
/// core start, the pc of the core start (the synthetic's pc), the pc one
/// past the core's last instruction, the receiver, and the prim.
pub(crate) struct MapCore {
    pub(crate) start_idx: usize,
    pub(crate) start_pc: u32,
    pub(crate) end_pc: u32,
    pub(crate) recv: Recv,
    pub(crate) recv_pc: u32,
    pub(crate) prim: PrimType,
    pub(crate) put: bool,
    pub(crate) kind: crate::ir::MapKind,
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
pub(crate) enum Recv {
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
/// The free-function form of [`proven_map_fields`] — the same scan over
/// explicitly passed parts, so the unit tests can build the inputs
/// without a full `ClassPlan` (whose classfile field is private).
pub(crate) fn proven_map_fields_of(
    class_name: &str,
    fields: &[crate::FieldPlan],
    methods: &[crate::MethodPlan],
) -> std::collections::BTreeMap<(String, String), (PrimType, crate::ir::MapKind)> {
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
        if let Some(kind) = field_proven(class_name, f, methods) {
            out.insert((class_name.to_string(), f.name.clone()), (prim, kind));
        }
    }
    out
}

pub(crate) fn prim_of_boxed(bp: crate::signature::BoxedPrim) -> PrimType {
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
pub(crate) fn field_proven(
    class_name: &str,
    f: &crate::FieldPlan,
    methods: &[crate::MethodPlan],
) -> Option<crate::ir::MapKind> {
    let mut creation_sites = 0usize;
    let mut kind: Option<crate::ir::MapKind> = None;
    for m in methods {
        for i in 0..m.instrs.len() {
            if let Instr::PutStatic { class, field, .. } = &m.instrs[i].1 {
                if class == class_name && *field == f.name {
                    // The branch-target guard needs the method's targets.
                    let mut targets = Vec::new();
                    targets_of(&m.instrs, &mut targets);
                    let site_kind = is_creation_putstatic(&m.instrs, i, &targets)?;
                    // Every creation site must prove the SAME storage —
                    // a mixed-provenance field refuses the whole pin.
                    match kind {
                        Some(k) if k != site_kind => return None,
                        _ => kind = Some(site_kind),
                    }
                    creation_sites += 1;
                }
            }
        }
    }
    if creation_sites > 0 {
        kind
    } else {
        None
    }
}

/// `New <map>; Dup; (const pushes); Invokespecial <map>.<init>; PutStatic f`
/// with no branch target inside — the PutStatic-side mirror of Phase A's
/// creation run. Returns the storage kind the run's class proves (the
/// map family only, G3.3 slice 4).
pub(crate) fn is_creation_putstatic(
    instrs: &[(u32, Instr)],
    i: usize,
    targets: &[u32],
) -> Option<crate::ir::MapKind> {
    if i == 0 {
        return None;
    }
    let init_class = match &instrs[i - 1].1 {
        Instr::InvokeSpecial { class, name, .. } if name == "<init>" => class,
        _ => return None,
    };
    let kind = kind_of_class(init_class)?;
    let mut j = i - 1;
    while j > 0 && matches!(instrs[j - 1].1, ref x if is_const_push(x)) {
        j -= 1;
    }
    if j == 0 {
        return None;
    }
    if !matches!(instrs[j - 1].1, Instr::Dup) {
        return None;
    }
    if j < 2 {
        return None;
    }
    match &instrs[j - 2].1 {
        Instr::New { class } if class == init_class => {}
        _ => return None,
    }
    let start_pc = instrs[j - 2].0;
    let end_pc = instrs.get(i + 1).map(|p| p.0).unwrap_or(u32::MAX);
    if targets.iter().any(|t| *t > start_pc && *t < end_pc) {
        return None;
    }
    Some(kind)
}

/// Rewrites the specialized-map cores. Returns the number of cores
/// Phase A (verbatim from `map_prim_rewrites`, round-5 reshaping): the
/// map-fresh local scan. A creation run is `New <map>; Dup; (const
/// pushes); Invokespecial <map>.<init>; Store(n)` with no branch target
/// inside, where <map> is one of the three storages (G3.3 slice 4). Every
/// OTHER Store(n) kills the local's freshness. The LinkedHashMap
/// 3-const-arg run (the access-order ctor `(int,float,boolean)`) is
/// REFUSED — the rewrite must not fire where the faithful get's law could
/// differ (j2r's LHM is insertion-ordered; the conservative default keeps
/// the faithful path everywhere it is used).
pub(crate) fn fresh_map_locals(
    instrs: &[(u32, Instr)],
    targets: &[u32],
) -> std::collections::BTreeMap<u16, crate::ir::MapKind> {
    let mut fresh: std::collections::BTreeMap<u16, crate::ir::MapKind> = Default::default();
    let mut creation_stores: Vec<u32> = Vec::new();
    for i in 0..instrs.len() {
        let Some(store_idx) = (|| {
            let kind = match &instrs[i].1 {
                Instr::New { class: c } => kind_of_class(c)?,
                _ => return None,
            };
            let new_class = match &instrs[i].1 {
                Instr::New { class: c } => c.clone(),
                _ => unreachable!(),
            };
            if !matches!(instrs.get(i + 1).map(|p| &p.1), Some(Instr::Dup)) {
                return None;
            }
            let mut j = i + 2;
            while matches!(instrs.get(j).map(|p| &p.1), Some(x) if is_const_push(x)) {
                j += 1;
            }
            // The access-order refusal: exactly 3 const args between the
            // Dup and the <init> on a LinkedHashMap run.
            if kind == crate::ir::MapKind::LinkedHashMap && j == i + 5 {
                return None;
            }
            match instrs.get(j).map(|p| &p.1) {
                Some(Instr::InvokeSpecial { class, name, .. })
                    if *class == new_class && name == "<init>" => {}
                _ => return None,
            }
            match instrs.get(j + 1).map(|p| &p.1) {
                Some(Instr::Store(n)) => Some((j + 1, *n, kind)),
                _ => None,
            }
        })() else {
            continue;
        };
        let (store_idx, n, kind) = store_idx;
        let start_pc = instrs[i].0;
        let end_pc = instrs.get(store_idx + 1).map(|p| p.0).unwrap_or(u32::MAX);
        if targets.iter().any(|t| *t > start_pc && *t < end_pc) {
            continue;
        }
        creation_stores.push(instrs[store_idx].0);
        fresh.insert(n, kind);
    }
    for (pc, i) in instrs.iter() {
        if let Instr::Store(n) = i {
            if !creation_stores.contains(pc) {
                fresh.remove(n);
            }
        }
    }
    fresh
}

/// Phase D (verbatim from `map_prim_rewrites`, round-5 reshaping): the
/// span guards (branch targets + handler overlap). A refused core keeps
/// the faithful path, which then OBSERVES the prim slots its sibling
/// rewrites wrote — so a refusal must remove every core of the same
/// receiver local. The closure check below does exactly that: after the
/// guards, the refused core's receiver Load is no longer covered.
pub(crate) fn span_guard_pass(
    cores: &mut Vec<MapCore>,
    targets: &[u32],
    exception_table: &[(u32, u32, u32, Option<String>)],
) {
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
}

/// Phase E (verbatim from `map_prim_rewrites`, round-5 reshaping): the
/// consumption closure for the LOCAL receivers — every Load(n) of a fresh
/// local must be some surviving core's receiver. Any other load (an
/// argument pass, a field store, a return, `keySet()`, a non-core
/// put/get) can observe the map or a value where a BOX is expected. Field
/// receivers are exempt here: their closure was proven class-wide by the
/// proven-field scan.
pub(crate) fn consumption_closure(
    instrs: &[(u32, Instr)],
    cores: &mut Vec<MapCore>,
    fresh: &std::collections::BTreeMap<u16, crate::ir::MapKind>,
) {
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
}

/// Phase F (verbatim from `map_prim_rewrites`, round-5 reshaping): splice
/// the synthetic instructions in, descending so the indices stay valid.
pub(crate) fn splice_synthetic(instrs: &mut Vec<(u32, Instr)>, cores: &[MapCore]) -> usize {
    let mut replaced = 0;
    for c in cores.iter().rev() {
        let synthetic = if c.put {
            Instr::MapPutPrim {
                kind: c.kind,
                prim: c.prim,
            }
        } else {
            Instr::MapGetPrim {
                kind: c.kind,
                prim: c.prim,
            }
        };
        instrs.splice(c.start_idx..c.start_idx + 3, vec![(c.start_pc, synthetic)]);
        replaced += 1;
    }
    replaced
}
