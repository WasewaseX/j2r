//! T15.2a — the MappingSet-driven remap pass over the T15.1 raw transport
//! (TODO.md T15, M5 wave 3). The pass answers one question for every pool
//! slot that carries a name: what is its name in the target namespace? A
//! [`MappingSet`] (the parsed Tiny v2 file) supplies the answers; this
//! module applies them to a [`RawClassFile`] without touching any other
//! byte — Code bodies, StackMapTable payloads, exception tables and every
//! unmapped slot ride verbatim, which is the whole point of the raw
//! transport (see `remap.rs`).
//!
//! The pass is TWO-PHASE on purpose: every lookup (class names, member
//! names, descriptors) is resolved against the pool AS IT WAS READ, and
//! the mutations are applied only afterwards. Remapping `this_class`'s
//! Class entry mid-walk would otherwise change what the member lookups
//! see (the owner name would already be the new one) and make the result
//! order-dependent. One pass over a stable plan cannot drift.
//!
//! Scope (T15.2a, the pool pass):
//! * every CONSTANT_Class entry whose binary name maps in the set;
//! * the NameAndType behind every FieldRef/MethodRef/InterfaceMethodRef
//!   (name via `map_field`/`map_method`, descriptor via the descriptor
//!   remapper below);
//! * the NameAndType of InvokeDynamic/ConstantDynamic — descriptor only
//!   (an indy name is a call-site label, not a member name; no owner to
//!   look it up against, and the reference remappers keep it too);
//! * every field/method member name + descriptor of the class itself.
//!
//! NOT in this slice (T15.2b, the non-Code attribute pass): Exceptions,
//! InnerClasses, NestHost/NestMembers, EnclosingMethod, Signature text,
//! StackMapTable Object_variable_info, Record components.
//!
//! Interning: new Utf8 constants are appended through [`RawPool::
//! append_utf8`] and deduplicated WITHIN one pass (two slots remapped to
//! the same new name share one appended entry). Pre-existing equal
//! entries are not reused — appending is always byte-safe and the pool
//! growth is bounded by the number of distinct new names; matching
//! against the whole pool would cost a full-content scan per name for a
//! few saved slots.

use std::collections::HashMap;

use j2r_mappings::MappingSet;

use crate::remap::{RawClassFile, RawPool};
use crate::Error;

/// What one pass did, in slots and bytes (the T15.2a evidence numbers).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RemapStats {
    /// Class entries whose binary name was mapped (this_class included).
    pub classes_renamed: usize,
    /// Ref NameAndTypes whose NAME was mapped (field/method refs only).
    pub refs_renamed: usize,
    /// The class's own fields/methods whose name was mapped.
    pub members_renamed: usize,
    /// Descriptors rewritten (member descriptors + ref NameAndType
    /// descriptors, indy/condy included) — a rewrite is counted when the
    /// remapped text differs from the original.
    pub descriptors_rewritten: usize,
    /// New Utf8 constants appended (already deduplicated).
    pub utf8_appended: usize,
    /// Signature attribute texts remapped (the T15.2b attribute layer).
    pub signatures_rewritten: usize,
    /// Record components whose NAME was mapped (T15.2b).
    pub record_parts_renamed: usize,
    /// EnclosingMethod NameAndTypes remapped (T15.2b).
    pub enclosing_renamed: usize,
    /// LocalVariableTable/LVTT entries remapped inside Code (T15.2c).
    pub code_vars_rewritten: usize,
}

/// One planned NameAndType rewrite (planned against the ORIGINAL pool).
struct NatPlan {
    slot: u16,
    new_name: Option<String>,
    new_desc: Option<String>,
}

/// One planned member index rewrite (planned against the ORIGINAL pool).
struct MemberPlan {
    index: usize,
    new_name: Option<String>,
    new_desc: Option<String>,
}

/// The T15.2a pass: rename every pool name the mapping set knows about.
/// Lookups resolve against the pool as it was read; the mutations land
/// after the plan is complete. Descriptors remap alongside names — a
/// renamed class referenced inside a signature or descriptor must move
/// there too, or the rewritten class would mix two namespaces.
pub fn remap_with(
    raw: &mut RawClassFile,
    ms: &MappingSet,
    from_ns: usize,
    to_ns: usize,
) -> Result<RemapStats, Error> {
    let owner = raw.this_class_name()?;

    // ---- phase 1: plan (the pool is only read here) -----------------
    let mut class_plans: Vec<(u16, String)> = Vec::new();
    for idx in 1..raw.pool.slots.len() {
        if let Some(entry) = &raw.pool.slots[idx] {
            if entry.tag == 7 {
                let name = raw.pool.class_name(idx as u16)?;
                if let Some(new) = ms.map_class(from_ns, to_ns, &name) {
                    class_plans.push((idx as u16, new.to_string()));
                }
            }
        }
    }

    let mut nat_plans: Vec<NatPlan> = Vec::new();
    for idx in 1..raw.pool.slots.len() {
        let Some(entry) = &raw.pool.slots[idx] else {
            continue;
        };
        // 9 FieldRef, 10 MethodRef, 11 InterfaceMethodRef: the NameAndType
        // carries both a name and a descriptor to remap. 17/18 (indy and
        // condy): descriptor only — the name is a call-site label.
        let is_ref = matches!(entry.tag, 9..=11);
        let is_indy = matches!(entry.tag, 17 | 18);
        if !(is_ref || is_indy) {
            continue;
        }
        let class_index = u16::from_be_bytes([entry.data[0], entry.data[1]]);
        let nat_index = u16::from_be_bytes([entry.data[2], entry.data[3]]);
        let (n_idx, d_idx) = raw
            .pool
            .slots
            .get(usize::from(nat_index))
            .and_then(|s| s.as_ref())
            .and_then(|s| s.name_and_type())
            .ok_or_else(|| Error {
                message: format!("cp slot {nat_index} behind the ref is not a NameAndType"),
                offset: 0,
            })?;
        let name = raw.pool.utf8(n_idx)?;
        let desc = raw.pool.utf8(d_idx)?;
        let new_desc = {
            let mapped = remap_descriptor(&desc, ms, from_ns, to_ns);
            if mapped == desc {
                None
            } else {
                Some(mapped)
            }
        };
        let new_name = if is_ref {
            let owner_name = raw.pool.class_name(class_index)?;
            let mapped = if entry.tag == 9 {
                ms.map_field(from_ns, to_ns, &owner_name, &desc, &name)
            } else {
                ms.map_method(from_ns, to_ns, &owner_name, &desc, &name)
            };
            mapped.map(str::to_string)
        } else {
            None
        };
        if new_name.is_some() || new_desc.is_some() {
            nat_plans.push(NatPlan {
                slot: nat_index,
                new_name,
                new_desc,
            });
        }
    }

    let mut field_plans: Vec<MemberPlan> = Vec::new();
    let mut method_plans: Vec<MemberPlan> = Vec::new();
    for (i, m) in raw.fields.iter().enumerate() {
        let name = raw.pool.utf8(m.name_index)?;
        let desc = raw.pool.utf8(m.descriptor_index)?;
        let mapped_name = ms
            .map_field(from_ns, to_ns, &owner, &desc, &name)
            .map(str::to_string);
        let mapped_desc = remap_descriptor(&desc, ms, from_ns, to_ns);
        let new_desc = (mapped_desc != desc).then_some(mapped_desc);
        if mapped_name.is_some() || new_desc.is_some() {
            field_plans.push(MemberPlan {
                index: i,
                new_name: mapped_name,
                new_desc,
            });
        }
    }
    for (i, m) in raw.methods.iter().enumerate() {
        let name = raw.pool.utf8(m.name_index)?;
        let desc = raw.pool.utf8(m.descriptor_index)?;
        let mapped_name = ms
            .map_method(from_ns, to_ns, &owner, &desc, &name)
            .map(str::to_string);
        let mapped_desc = remap_descriptor(&desc, ms, from_ns, to_ns);
        let new_desc = (mapped_desc != desc).then_some(mapped_desc);
        if mapped_name.is_some() || new_desc.is_some() {
            method_plans.push(MemberPlan {
                index: i,
                new_name: mapped_name,
                new_desc,
            });
        }
    }

    // ---- phase 2: apply (appends first, then the index rewrites) ----
    let mut interner = Interner {
        cache: HashMap::new(),
        appended: 0,
        pool: &mut raw.pool,
    };
    for (slot, new_name) in &class_plans {
        let idx = interner.intern(new_name)?;
        let entry = interner.pool.slots.get_mut(usize::from(*slot));
        match entry.and_then(|s| s.as_mut()) {
            Some(e) if e.tag == 7 => e.set_class_name_index(idx),
            _ => {
                return Err(Error {
                    message: format!("cp slot {slot} stopped being a Class constant"),
                    offset: 0,
                })
            }
        }
    }
    for plan in &nat_plans {
        let (n_idx, d_idx) = interner
            .pool
            .slots
            .get(usize::from(plan.slot))
            .and_then(|s| s.as_ref())
            .and_then(|s| s.name_and_type())
            .ok_or_else(|| Error {
                message: format!("cp slot {} stopped being a NameAndType", plan.slot),
                offset: 0,
            })?;
        let new_n = match &plan.new_name {
            Some(name) => interner.intern(name)?,
            None => n_idx,
        };
        let new_d = match &plan.new_desc {
            Some(desc) => interner.intern(desc)?,
            None => d_idx,
        };
        let entry = interner.pool.slots.get_mut(usize::from(plan.slot));
        match entry.and_then(|s| s.as_mut()) {
            Some(e) if e.tag == 12 => e.set_name_and_type(new_n, new_d),
            _ => {
                return Err(Error {
                    message: format!("cp slot {} stopped being a NameAndType", plan.slot),
                    offset: 0,
                })
            }
        }
    }
    for (plans, members) in [
        (&field_plans, &mut raw.fields),
        (&method_plans, &mut raw.methods),
    ] {
        for plan in plans {
            let m = &mut members[plan.index];
            if let Some(name) = &plan.new_name {
                m.name_index = interner.intern(name)?;
            }
            if let Some(desc) = &plan.new_desc {
                m.descriptor_index = interner.intern(desc)?;
            }
        }
    }

    Ok(RemapStats {
        classes_renamed: class_plans.len(),
        refs_renamed: nat_plans.iter().filter(|p| p.new_name.is_some()).count(),
        members_renamed: field_plans.iter().filter(|p| p.new_name.is_some()).count()
            + method_plans.iter().filter(|p| p.new_name.is_some()).count(),
        descriptors_rewritten: nat_plans.iter().filter(|p| p.new_desc.is_some()).count()
            + field_plans.iter().filter(|p| p.new_desc.is_some()).count()
            + method_plans.iter().filter(|p| p.new_desc.is_some()).count(),
        utf8_appended: interner.appended,
        signatures_rewritten: 0,
        record_parts_renamed: 0,
        enclosing_renamed: 0,
        code_vars_rewritten: 0,
    })
}

/// Appends Utf8 constants with within-pass deduplication (see the module
/// docs for why pre-existing entries are not reused). Shared with the
/// T15.2b attribute layer (`remap_attrs`).
pub(crate) struct Interner<'a> {
    cache: HashMap<String, u16>,
    appended: usize,
    pool: &'a mut RawPool,
}

impl Interner<'_> {
    pub(crate) fn new(pool: &mut RawPool) -> Interner<'_> {
        Interner {
            cache: HashMap::new(),
            appended: 0,
            pool,
        }
    }

    pub(crate) fn intern(&mut self, text: &str) -> Result<u16, Error> {
        if let Some(&idx) = self.cache.get(text) {
            return Ok(idx);
        }
        let idx = self.pool.append_utf8(text)?;
        self.cache.insert(text.to_string(), idx);
        self.appended += 1;
        Ok(idx)
    }

    pub(crate) fn appended(&self) -> usize {
        self.appended
    }

    /// Reads a Utf8 through the borrowed pool (so callers never need a
    /// second borrow of it).
    pub(crate) fn utf8(&self, idx: u16) -> Result<String, Error> {
        self.pool.utf8(idx)
    }

    /// Reads the (name, descriptor) indexes of a NameAndType slot.
    pub(crate) fn name_and_type(&self, slot: u16) -> Result<(u16, u16), Error> {
        self.pool
            .slots
            .get(usize::from(slot))
            .and_then(|s| s.as_ref())
            .and_then(|s| s.name_and_type())
            .ok_or_else(|| Error {
                message: format!("cp slot {slot} is not a NameAndType"),
                offset: 0,
            })
    }

    /// Retargets a NameAndType slot's (name, descriptor) indexes.
    pub(crate) fn set_name_and_type_slot(&mut self, slot: u16, name: u16, desc: u16) {
        if let Some(e) = self
            .pool
            .slots
            .get_mut(usize::from(slot))
            .and_then(|s| s.as_mut())
        {
            if e.tag == 12 {
                e.set_name_and_type(name, desc);
            }
        }
    }
}

/// Remaps every class reference inside a descriptor or signature text.
/// The grammar this walks (JVMS §4.3 descriptors + §4.7.6.1 signature
/// syntax): every `L` at a type position opens a class reference; its
/// binary name runs to the next `<` or `;`; generic type arguments
/// recurse (they nest their own `L` references); `T`-prefixed type
/// variables copy verbatim; primitives, arrays (`[`), wildcards and
/// everything outside `L`-references ride through untouched.
///
/// The binary name inside `L` is mapped name-for-name through the
/// mapping set; an unmapped name stays exactly as it was. Inner-class
/// suffixes after `.` (the signature spelling `LOuter<TT;>.Inner;`)
/// are carried verbatim — the mapped prefix is the OUTER binary name.
pub fn remap_descriptor(desc: &str, ms: &MappingSet, from_ns: usize, to_ns: usize) -> String {
    let chars: Vec<char> = desc.chars().collect();
    let mut out = String::with_capacity(desc.len());
    remap_region(&chars, ms, from_ns, to_ns, &mut out);
    out
}

fn remap_region(chars: &[char], ms: &MappingSet, from_ns: usize, to_ns: usize, out: &mut String) {
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            'L' => {
                // the binary name runs to the next '<' (generics open)
                // or ';' (the reference closes)
                let start = i + 1;
                let mut j = start;
                while j < chars.len() && chars[j] != '<' && chars[j] != ';' {
                    j += 1;
                }
                let base: String = chars[start..j].iter().collect();
                out.push('L');
                out.push_str(ms.map_class(from_ns, to_ns, &base).unwrap_or(&base));
                if j < chars.len() && chars[j] == '<' {
                    // generic arguments: find the matching '>' (they nest)
                    // and remap the inside as its own region
                    let mut depth = 0usize;
                    let mut k = j;
                    while k < chars.len() {
                        if chars[k] == '<' {
                            depth += 1;
                        } else if chars[k] == '>' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        k += 1;
                    }
                    out.push('<');
                    remap_region(&chars[j + 1..k], ms, from_ns, to_ns, out);
                    out.push('>');
                    i = k + 1;
                } else {
                    i = j;
                }
                if i < chars.len() && chars[i] == ';' {
                    out.push(';');
                    i += 1;
                }
            }
            'T' => {
                // a type-variable REFERENCE rides verbatim through its
                // closing ';'. In signature text the same 'T' can also
                // open a type-parameter DECLARATION (`<T:Bound...>`): the
                // identifier then runs to ':' and the BOUND after it is a
                // real reference that must remap (the pre-2b walker
                // swallowed the whole bound verbatim — the gap the
                // signature-bound law pins).
                while i < chars.len() && chars[i] != ';' && chars[i] != ':' {
                    out.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() && chars[i] == ':' {
                    out.push(':');
                    i += 1;
                    // the bound is ONE ReferenceTypeSignature (L..., [L...,
                    // or a T-variable); anything else opens the next
                    // declaration — an empty bound rides through
                    if matches!(chars.get(i), Some('L') | Some('[') | Some('T')) {
                        let end = refsig_end(chars, i);
                        remap_region(&chars[i..end], ms, from_ns, to_ns, out);
                        i = end;
                    }
                } else if i < chars.len() {
                    out.push(';');
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
}

/// The end (exclusive) of ONE ReferenceTypeSignature starting at `start`:
/// `L...;` (the ';'s inside its own `<...>` arguments belong to the
/// nested types, so the close is the first ';' at bracket depth zero),
/// `T...;`, or `[` followed by one of those. Malformed text (no closing
/// `;`) ends at the text end — the walker never panics on junk.
fn refsig_end(chars: &[char], start: usize) -> usize {
    let mut i = start;
    while i < chars.len() && chars[i] == '[' {
        i += 1;
    }
    match chars.get(i) {
        Some('L') => {
            let mut depth = 0usize;
            while i < chars.len() {
                match chars[i] {
                    '<' => depth += 1,
                    '>' => depth -= 1,
                    ';' if depth == 0 => return i + 1,
                    _ => {}
                }
                i += 1;
            }
            chars.len()
        }
        _ => {
            while i < chars.len() && chars[i] != ';' {
                i += 1;
            }
            (i + 1).min(chars.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remap::RawCpEntry;
    use j2r_mappings::parser;
    use j2r_mappings::{ClassEntry, MemberEntry};

    const VEC1: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec1.class");
    const VEC2: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec2.class");
    const VEC2_INNER: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec2$Inner.class");
    const VEC2_BOX: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec2$Box.class");
    const VEC2_SHAPE: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec2$Shape.class");
    const VEC3: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec3.class");
    const VEC3_PAIR: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec3$Pair.class");
    const VEC3_TONE: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec3$Tone.class");

    const ALL_VECTORS: &[(&str, &[u8])] = &[
        ("vec1", VEC1),
        ("vec2", VEC2),
        ("vec2$Inner", VEC2_INNER),
        ("vec2$Box", VEC2_BOX),
        ("vec2$Shape", VEC2_SHAPE),
        ("vec3", VEC3),
        ("vec3$Pair", VEC3_PAIR),
        ("vec3$Tone", VEC3_TONE),
    ];

    /// A two-namespace set with just class rows (`from -> to`).
    fn tiny_class_map(rows: &[(&str, &str)]) -> MappingSet {
        let mut set = MappingSet::default();
        set.namespaces = vec!["src".to_string(), "dst".to_string()];
        set.classes = rows
            .iter()
            .map(|(from, to)| ClassEntry {
                names: vec![Some(from.to_string()), Some(to.to_string())],
                ..Default::default()
            })
            .collect();
        set.index()
    }

    /// A two-namespace set with one owner and a method row (`owner, desc,
    /// from -> to`) — the shape map_method resolves through.
    fn tiny_method_map(
        classes: &[(&str, &str)],
        owner: &str,
        desc: &str,
        from: &str,
        to: &str,
    ) -> MappingSet {
        let class_rows = classes
            .iter()
            .map(|(f, t)| ClassEntry {
                names: vec![Some(f.to_string()), Some(t.to_string())],
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let mut set = MappingSet::default();
        set.namespaces = vec!["src".to_string(), "dst".to_string()];
        set.classes = class_rows;
        let entry = set
            .classes
            .iter_mut()
            .find(|c| c.names[0].as_deref() == Some(owner))
            .expect("the owner must carry a class row");
        entry.methods.push(MemberEntry {
            desc: desc.to_string(),
            names: vec![Some(from.to_string()), Some(to.to_string())],
            params: Vec::new(),
        });
        set.index()
    }

    /// Law R1: an empty mapping set is a byte-exact identity — no slot
    /// changes, no Utf8 appends, no member index moves, on every vector.
    #[test]
    fn empty_map_is_a_byte_exact_identity() {
        let ms = tiny_class_map(&[]);
        for (name, bytes) in ALL_VECTORS.iter().copied() {
            let mut raw = RawClassFile::parse(bytes)
                .unwrap_or_else(|e| panic!("{name}: parse failed: {}", e.message));
            let before = raw.clone();
            let stats = remap_with(&mut raw, &ms, 0, 1).expect("identity pass");
            assert_eq!(raw.serialize(), bytes, "{name}: identity pass moved bytes");
            assert_eq!(raw, before, "{name}: identity pass mutated the model");
            assert_eq!(
                stats,
                RemapStats::default(),
                "{name}: identity pass counted work"
            );
        }
    }

    /// Law R2: class renames flow through every Class entry that carries
    /// a mapped name — the appended Utf8s are exactly the DISTINCT mapped
    /// names, every mapped Class slot points at its new name, every
    /// unmapped slot is byte-identical, and the output re-verifies (the
    /// resolver reads the new this_class; the transport re-serializes it
    /// byte-stably).
    #[test]
    fn class_renames_flow_through_the_pool() {
        let ms = tiny_class_map(&[
            ("RemapVec2", "mapped/RemapVec2"),
            ("java/util/ArrayList", "mapped/ArrayList"),
            ("RemapVec2$Shape", "mapped/Shape"),
        ]);
        let mut raw = RawClassFile::parse(VEC2).unwrap();
        let before_slots = raw.pool.slots.clone();
        let stats = remap_with(&mut raw, &ms, 0, 1).unwrap();

        // the appends are exactly the three distinct mapped names
        assert_eq!(stats.classes_renamed, 3, "three Class entries mapped");
        assert_eq!(stats.utf8_appended, 3, "one Utf8 per distinct name");
        assert_eq!(stats.refs_renamed, 0);
        assert_eq!(stats.members_renamed, 0);
        assert_eq!(stats.descriptors_rewritten, 0);

        // every mapped Class entry now points at a Utf8 with the new
        // name; every other PRE-EXISTING slot is byte-identical
        let mut renamed: Vec<(String, String)> = Vec::new();
        for (i, (was, now)) in before_slots.iter().zip(raw.pool.slots.iter()).enumerate() {
            let Some(w) = was else { continue };
            if w.tag == 7 {
                let old_name = raw_pool_name(&before_slots, w.class_name_index().unwrap());
                if let Some(new) = ms.map_class(0, 1, &old_name) {
                    let n = now.as_ref().expect("mapped slot must stay");
                    let new_idx = n.class_name_index().unwrap();
                    assert_eq!(
                        raw.pool.utf8(new_idx).unwrap(),
                        new,
                        "slot {i}: retargeted to the mapped name"
                    );
                    renamed.push((old_name, new.to_string()));
                } else {
                    assert_eq!(was, now, "slot {i}: unmapped slot must not move");
                }
            } else {
                assert_eq!(was, now, "slot {i}: non-Class slot must not move");
            }
        }
        renamed.sort();
        assert_eq!(
            renamed,
            vec![
                ("RemapVec2".to_string(), "mapped/RemapVec2".to_string()),
                ("RemapVec2$Shape".to_string(), "mapped/Shape".to_string()),
                (
                    "java/util/ArrayList".to_string(),
                    "mapped/ArrayList".to_string()
                ),
            ]
        );

        // the output re-verifies on both readers
        let out = raw.serialize();
        assert_eq!(
            crate::ClassFile::parse(&out).unwrap().this_class,
            "mapped/RemapVec2"
        );
        assert_eq!(RawClassFile::parse(&out).unwrap().serialize(), out);
    }

    fn raw_pool_name(slots: &[Option<RawCpEntry>], idx: u16) -> String {
        slots[usize::from(idx)]
            .as_ref()
            .unwrap()
            .utf8_str()
            .unwrap()
    }

    /// Law R3: a mapped method on a mapped owner renames the ref's
    /// NameAndType name and the owner's Class entry, while an UNMAPPED
    /// descriptor keeps its original slot — name-for-name, nothing else.
    /// Facts probed off the committed bytes (scripts/probe_vec_pool.py):
    /// VEC2 carries `MethodRef owner=java/lang/String nat=length/()I`
    /// (plus exactly three descs containing `Ljava/lang/String;`: the
    /// IAE-ctor and println NameAndTypes and the score member).
    #[test]
    fn member_refs_get_name_for_name() {
        // locate the ref BEFORE the pass (the plan must see the original
        // names — this pins the two-phase design)
        let before = RawClassFile::parse(VEC2).unwrap();
        let mut found: Option<(u16, u16, u16)> = None; // (ref slot, owner slot, nat slot)
        for idx in 1..before.pool.slots.len() {
            let Some(e) = &before.pool.slots[idx] else {
                continue;
            };
            if !matches!(e.tag, 10 | 11) {
                continue;
            }
            let class_index = u16::from_be_bytes([e.data[0], e.data[1]]);
            let nat_index = u16::from_be_bytes([e.data[2], e.data[3]]);
            let owner = before.pool.class_name(class_index).unwrap();
            let (n_idx, d_idx) = before.pool.slots[usize::from(nat_index)]
                .as_ref()
                .unwrap()
                .name_and_type()
                .unwrap();
            if owner == "java/lang/String"
                && before.pool.utf8(n_idx).unwrap() == "length"
                && before.pool.utf8(d_idx).unwrap() == "()I"
            {
                found = Some((idx as u16, class_index, nat_index));
            }
        }
        let (ref_slot, owner_slot, nat_slot) =
            found.expect("VEC2 calls String.length through a MethodRef");

        let ms = tiny_method_map(
            &[("java/lang/String", "s/Str")],
            "java/lang/String",
            "()I",
            "length",
            "len",
        );
        let mut raw = RawClassFile::parse(VEC2).unwrap();
        let before_slots = raw.pool.slots.clone();
        let stats = remap_with(&mut raw, &ms, 0, 1).unwrap();
        assert_eq!(stats.refs_renamed, 1);
        assert_eq!(stats.classes_renamed, 1);
        assert_eq!(stats.members_renamed, 0);
        // every desc containing Ljava/lang/String; rewrote: the two
        // NameAndType descs ((Ls/Str;)V — IAE ctor + println share the
        // same rewritten text) and the score member ((Ls/Str;)I)
        assert_eq!(stats.descriptors_rewritten, 3);
        // distinct new Utf8s: the mapped class name, the mapped method
        // name, and the two distinct rewritten descriptor texts
        assert_eq!(stats.utf8_appended, 4);

        // the ref's owner Class entry points at s/Str through the SAME
        // slot (indexes never move; only name_index fields retarget)
        let ref_entry = raw.pool.slots[usize::from(ref_slot)].as_ref().unwrap();
        let new_owner = u16::from_be_bytes([ref_entry.data[0], ref_entry.data[1]]);
        assert_eq!(new_owner, owner_slot, "the ref points at the same slot");
        let owner_entry = raw.pool.slots[usize::from(owner_slot)].as_ref().unwrap();
        assert_eq!(
            raw.pool
                .utf8(owner_entry.class_name_index().unwrap())
                .unwrap(),
            "s/Str"
        );
        // the NameAndType: name moved to the appended "len", descriptor
        // KEPT its original slot (no class reference inside "()I")
        let nat_entry = raw.pool.slots[usize::from(nat_slot)].as_ref().unwrap();
        let (new_n, new_d) = nat_entry.name_and_type().unwrap();
        assert_eq!(raw.pool.utf8(new_n).unwrap(), "len");
        let (old_n, old_d) = before_slots[usize::from(nat_slot)]
            .as_ref()
            .unwrap()
            .name_and_type()
            .unwrap();
        assert_eq!(new_d, old_d, "unmapped descriptor keeps its slot");
        assert_ne!(new_n, old_n, "the name moved to the appended Utf8");
        // the score member moved its descriptor, kept its name (the
        // method map's owner is java/lang/String, not RemapVec2)
        let score = raw
            .methods
            .iter()
            .find(|m| raw.pool.utf8(m.name_index).unwrap() == "score")
            .expect("score survives");
        assert_eq!(raw.pool.utf8(score.descriptor_index).unwrap(), "(Ls/Str;)I");
        // the output re-verifies
        let out = raw.serialize();
        assert_eq!(RawClassFile::parse(&out).unwrap().serialize(), out);
    }

    /// Law R4: the descriptor remapper moves class references inside
    /// method/field descriptors and signature text — primitives, arrays,
    /// unmapped names, type variables and inner-class suffixes ride
    /// verbatim; generic arguments recurse.
    #[test]
    fn descriptor_remap_moves_class_references_only() {
        let ms = tiny_class_map(&[
            ("java/lang/String", "s/Str"),
            ("java/util/List", "s/Lst"),
            ("java/util/Map", "s/Map"),
            ("demo/Outer", "s/Outer"),
        ]);
        let r = |d: &str| remap_descriptor(d, &ms, 0, 1);
        // method descriptor: primitives + array + generic + unmapped
        // (the L prefix stays; only the name inside moves)
        assert_eq!(
            r("(ILjava/lang/String;[[JLjava/util/List<Ljava/lang/String;>;)Ljava/util/Map<Ljava/lang/String;Ljava/lang/Integer;>;"),
            "(ILs/Str;[[JLs/Lst<Ls/Str;>;)Ls/Map<Ls/Str;Ljava/lang/Integer;>;"
        );
        // a signature with a type variable and an inner-class suffix
        assert_eq!(
            r("<TT;>Ljava/lang/Object;Ldemo/Outer<TT;>.Inner;"),
            "<TT;>Ljava/lang/Object;Ls/Outer<TT;>.Inner;"
        );
        // a descriptor with no class references at all
        assert_eq!(r("([I[[D)Z"), "([I[[D)Z");
        assert_eq!(r("()V"), "()V");
    }

    /// Law R4b (T15.2b): type-parameter BOUNDS inside signature text are
    /// real references and must remap. The pre-2b walker's `T` arm ran
    /// verbatim to the closing `;` and swallowed the whole
    /// `<T:Ljava/lang/From;>` bound — a mapped bound class stayed
    /// unmapped in the output. Declarations whose identifier does NOT
    /// start with `T` already rode the default arms correctly; the law
    /// pins both spellings plus the reference spelling next to them.
    #[test]
    fn signature_type_param_bounds_move_too() {
        let ms = tiny_class_map(&[("remap/From", "remap/To")]);
        let r = |d: &str| remap_descriptor(d, &ms, 0, 1);
        // a T-named declaration: the identifier runs to ':' and the
        // bound remaps; the type-variable REFERENCES stay verbatim
        assert_eq!(
            r("<T:Lremap/From;>(TT;)Lremap/From<TT;>;^Lremap/From;"),
            "<T:Lremap/To;>(TT;)Lremap/To<TT;>;^Lremap/To;"
        );
        // a declaration whose identifier does not start with T (the
        // default arms already handled this spelling) and a nested
        // generic bound with its own type variables
        assert_eq!(
            r("<U:Lremap/From<Lremap/From;TT;>;>Lremap/From;"),
            "<U:Lremap/To<Lremap/To;TT;>;>Lremap/To;"
        );
        // array + type-var bounds remap/ride correctly
        assert_eq!(
            r("<T:[Lremap/From;>Lremap/From;"),
            "<T:[Lremap/To;>Lremap/To;"
        );
        assert_eq!(r("<T:TT;>Lremap/From;"), "<T:TT;>Lremap/To;");
        // an empty bound (the next declaration opens right after ':')
        assert_eq!(
            r("<T:U:Lremap/From;>Lremap/From;"),
            "<T:U:Lremap/To;>Lremap/To;"
        );
    }

    /// Law R5: the full pass over every vector (each class mapped to its
    /// own new name) re-verifies on the resolver and re-serializes
    /// byte-stably — no vector the pass rewrites can stop parsing.
    #[test]
    fn remapped_vectors_reverify_and_round_trip() {
        for (name, bytes) in ALL_VECTORS.iter().copied() {
            let raw = RawClassFile::parse(bytes)
                .unwrap_or_else(|e| panic!("{name}: parse failed: {}", e.message));
            let own = raw.this_class_name().unwrap();
            let ms = tiny_class_map(&[(&own, &format!("mapped/{own}"))]);
            let mut raw = RawClassFile::parse(bytes).unwrap();
            remap_with(&mut raw, &ms, 0, 1).unwrap();
            let out = raw.serialize();
            let resolved = crate::ClassFile::parse(&out).unwrap_or_else(|e| {
                panic!("{name}: the remapped class must re-verify: {}", e.message)
            });
            assert_eq!(resolved.this_class, format!("mapped/{own}"));
            assert_eq!(
                RawClassFile::parse(&out).unwrap().serialize(),
                out,
                "{name}: the remapped bytes must serialize stably"
            );
        }
    }

    /// Law R6: interning collapses duplicates — two mapped sources with
    /// the SAME target share one appended Utf8 (the real 93k-line
    /// intermediary maps rename hundreds of slots to shared names; the
    /// pool must not grow per-slot).
    #[test]
    fn interning_collapses_duplicate_targets() {
        // VEC2's pool carries Class entries for java/lang/Object,
        // java/lang/String and java/util/ArrayList; map all three to one
        // target and count the appends.
        let ms = tiny_class_map(&[
            ("java/lang/Object", "x/One"),
            ("java/lang/String", "x/One"),
            ("java/util/ArrayList", "x/One"),
        ]);
        let mut raw = RawClassFile::parse(VEC2).unwrap();
        let stats = remap_with(&mut raw, &ms, 0, 1).unwrap();
        assert_eq!(stats.classes_renamed, 3, "all three Class entries moved");
        // exactly ONE Utf8 slot in the whole pool spells x/One — the
        // three sources collapsed onto one appended entry (descriptors
        // that reference the mapped classes legitimately append their
        // own distinct rewritten texts; the collapse law is about the
        // class name appearing once)
        let one_slots: Vec<u16> = raw
            .pool
            .slots
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.as_ref()
                    .and_then(|e| e.utf8_str())
                    .map(|t| t == "x/One")
                    .unwrap_or(false)
            })
            .map(|(i, _)| i as u16)
            .collect();
        assert_eq!(
            one_slots.len(),
            1,
            "the mapped name is interned exactly once"
        );
        // all three Class slots now point at the same appended Utf8
        let mut targets: Vec<u16> = Vec::new();
        for slot in raw.pool.slots.iter().flatten() {
            if slot.tag == 7 {
                let idx = slot.class_name_index().unwrap();
                if raw.pool.utf8(idx).unwrap() == "x/One" {
                    targets.push(idx);
                }
            }
        }
        assert!(targets.len() >= 3, "the three renames must be visible");
        assert!(
            targets.iter().all(|t| *t == targets[0]),
            "every mapped Class entry shares one Utf8 slot"
        );
        let out = raw.serialize();
        assert_eq!(RawClassFile::parse(&out).unwrap().serialize(), out);
    }

    /// Law R7: the pass accepts a REAL Tiny v2 file (the parser is the
    /// only way production data reaches the pass) — a three-namespace
    /// mapping parses, indexes and drives the pass end to end.
    #[test]
    fn real_tiny_v2_file_drives_the_pass() {
        let tiny = "tiny\t2\t0\tsrc\tdst\n\
                    c\tRemapVec2\tmapped/RemapVec2\n\
                    \tf\tI\tsquare\tsq\n\
                    \tm\t()I\thalf\thf\n";
        let ms = parser::parse(tiny).expect("the real parser");
        assert_eq!(ms.namespaces, vec!["src", "dst"]);
        let mut raw = RawClassFile::parse(VEC2).unwrap();
        let stats = remap_with(&mut raw, &ms, 0, 1).unwrap();
        assert_eq!(stats.classes_renamed, 1);
        let out = raw.serialize();
        assert_eq!(
            crate::ClassFile::parse(&out).unwrap().this_class,
            "mapped/RemapVec2"
        );
    }

    /// A parse guard so a malformed Tiny v2 row can never silently
    /// reach the pass (the mappings crate's own law, restated at the
    /// seam: the remap stage consumes PARSED sets only).
    #[test]
    fn malformed_tiny_v2_is_loud_at_the_seam() {
        let bad = "tiny\t2\t0\tsrc\tdst\n\
                   x\tRemapVec2\tmapped/RemapVec2\n";
        assert!(parser::parse(bad).is_err());
    }
}
