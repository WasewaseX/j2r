//! `peephole`::map — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use crate::ir::{Instr, PrimType};

pub(crate) fn unbox_desc(p: PrimType) -> &'static str {
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
pub(crate) fn recognize_cores(instrs: &[(u32, Instr)]) -> Vec<(usize, bool, PrimType)> {
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
pub(crate) fn resolve_core(
    instrs: &[(u32, Instr)],
    shapes: &crate::verify::MethodShapes,
    i: usize,
    put: bool,
    prim: PrimType,
    fresh: &std::collections::BTreeMap<u16, crate::ir::MapKind>,
    proven: &std::collections::BTreeMap<(String, String), (PrimType, crate::ir::MapKind)>,
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
    let recv = recv?;
    // G3.3 slice 4: the storage kind follows the receiver's proof — the
    // creation run's class for a fresh local, the proven field's class
    // for a field.
    let kind = match &recv {
        Recv::Local(n) => fresh[n],
        Recv::Field { class, field } => proven.get(&(class.clone(), field.clone())).map(|p| p.1)?,
    };
    Some(MapCore {
        start_idx: i,
        start_pc: instrs[i].0,
        end_pc: instrs.get(i + 3).map(|p| p.0).unwrap_or(u32::MAX),
        recv,
        recv_pc: recv_pc?,
        prim,
        put,
        kind,
    })
}
