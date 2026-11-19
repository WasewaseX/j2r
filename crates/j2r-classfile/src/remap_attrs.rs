//! T15.2b — the non-Code attribute pass over the raw transport, layered
//! on the T15.2a pool pass (`remap_pass::remap_with`) through
//! [`remap_class`], the FULL remap entry.
//!
//! THE DESIGN FACT this module is built on (measured with
//! scripts/probe_vec_sigs.py + probe_vec_attrs.py, pinned by law A4):
//! every non-Code attribute that references a CLASS does so through a
//! pool INDEX — Exceptions (class entries), InnerClasses (inner/outer),
//! NestHost/NestMembers, EnclosingMethod's class_index,
//! StackMapTable's Object_variable_info, BootstrapMethods' argument
//! indexes. The pool pass retargets those Class entries IN PLACE (same
//! slot, new name Utf8), so all of those payloads stay byte-identical:
//! same index, new name. Nothing to rewrite there — the pass must NOT
//! touch them (a redundant rewrite would only risk drift).
//!
//! What the pool pass cannot reach — because it hangs off an attribute
//! and not a ref tag, or lives inside a Utf8 TEXT:
//! * Signature attribute texts (`Ljava/util/List<Ljava/lang/String;>;`…)
//!   — class-level, member-level and Record-component-level; the class
//!   references inside are TEXT, remapped with the shared descriptor
//!   walker (`remap_pass::remap_descriptor`);
//! * Record component names + descriptors (plain Utf8 slots);
//! * the EnclosingMethod NameAndType (attribute-hung, so the ref walk
//!   never saw it; remapped against the ORIGINAL enclosing-class name).
//!
//! All of these rewrite to the SAME payload length (u2 index swaps and
//! in-place component fields) — law A7 pins every attribute payload
//! length as stable, which keeps the whole attribute envelope intact.

use j2r_mappings::MappingSet;

use crate::remap::RawClassFile;
use crate::remap_pass::{remap_with, Interner, RemapStats};
use crate::Error;

/// Which attribute list an edit targets (indexes are stable across the
/// pool pass — it mutates pool slots and member index fields, never the
/// attribute lists themselves).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttrList {
    Class,
    Field(usize),
    Method(usize),
}

/// One planned rewrite (planned against the pool AS READ).
enum AttrEdit {
    /// Signature text: the attribute payload is one u2 Utf8 index to
    /// replace with the interned remapped text.
    Sig {
        list: AttrList,
        attr_idx: usize,
        new_text: String,
    },
    /// One Record component (addressed by class-attribute index +
    /// component index): name/descriptor index swaps plus any
    /// component-Signature texts, all in one edit so the apply walks
    /// the payload once.
    Rec {
        attr_idx: usize,
        comp: usize,
        new_name: Option<String>,
        new_desc: Option<String>,
        sigs: Vec<(usize, String)>,
    },
    /// EnclosingMethod's NameAndType slot: name (and descriptor) via
    /// map_method against the ORIGINAL enclosing-class name.
    Enc {
        nat_slot: u16,
        new_name: Option<String>,
        new_desc: Option<String>,
    },
}

/// The planning context: the raw file AS READ, the mapping set, the two
/// namespace indexes, and the class's own (original) binary name — one
/// struct keeps every planning helper's signature short.
struct PlanCtx<'a> {
    raw: &'a RawClassFile,
    ms: &'a MappingSet,
    from_ns: usize,
    to_ns: usize,
    owner: String,
}

/// The FULL T15.2 remap entry: the attribute plan resolves against the
/// pool as read, the pool pass (`remap_with`) renames every pool name
/// in place, then the attribute edits apply. Two planning phases, one
/// apply phase — the same order-independence law as the pool pass.
pub fn remap_class(
    raw: &mut RawClassFile,
    ms: &MappingSet,
    from_ns: usize,
    to_ns: usize,
) -> Result<RemapStats, Error> {
    let edits = plan_attrs(raw, ms, from_ns, to_ns)?;
    let mut stats = remap_with(raw, ms, from_ns, to_ns)?;
    apply_attrs(raw, &edits, &mut stats)?;
    // the T15.2c layer (Code-internal variable tables) plans nothing
    // against the original pool — its rewrites are pure TEXT swaps, so
    // it can run last against the current pool (see remap_code.rs)
    crate::remap_code::remap_code_tables(raw, ms, from_ns, to_ns, &mut stats)?;
    Ok(stats)
}

/// Walks every attribute the raw reader kept, plans the TEXT-level
/// rewrites (Signature texts, Record components, the EnclosingMethod
/// NameAndType). Everything else is index-stable by design and is left
/// for the pool pass (see the module docs).
fn plan_attrs(
    raw: &RawClassFile,
    ms: &MappingSet,
    from_ns: usize,
    to_ns: usize,
) -> Result<Vec<AttrEdit>, Error> {
    let ctx = PlanCtx {
        raw,
        ms,
        from_ns,
        to_ns,
        owner: raw.this_class_name()?,
    };
    let mut edits = Vec::new();

    for (ai, a) in raw.attributes.iter().enumerate() {
        match raw.pool.utf8(a.name_index)?.as_str() {
            "Signature" => plan_sig(&ctx, a, AttrList::Class, ai, &mut edits)?,
            "Record" => plan_record(&ctx, ai, a, &mut edits)?,
            "EnclosingMethod" => plan_enclosing(&ctx, a, &mut edits)?,
            _ => {}
        }
    }
    for (i, m) in raw.fields.iter().enumerate() {
        for (ai, a) in m.attributes.iter().enumerate() {
            if raw.pool.utf8(a.name_index)? == "Signature" {
                plan_sig(&ctx, a, AttrList::Field(i), ai, &mut edits)?;
            }
        }
    }
    for (i, m) in raw.methods.iter().enumerate() {
        for (ai, a) in m.attributes.iter().enumerate() {
            if raw.pool.utf8(a.name_index)? == "Signature" {
                plan_sig(&ctx, a, AttrList::Method(i), ai, &mut edits)?;
            }
        }
    }
    Ok(edits)
}

/// Plans one Signature attribute: payload = u2 Utf8 index; the text
/// remaps through the shared descriptor walker; unmapped text stays.
fn plan_sig(
    ctx: &PlanCtx<'_>,
    a: &crate::remap::RawAttribute,
    list: AttrList,
    attr_idx: usize,
    edits: &mut Vec<AttrEdit>,
) -> Result<(), Error> {
    let sig_index = signature_index(a)?;
    let text = ctx.raw.pool.utf8(sig_index)?;
    let mapped = crate::remap_pass::remap_descriptor(&text, ctx.ms, ctx.from_ns, ctx.to_ns);
    if mapped != text {
        edits.push(AttrEdit::Sig {
            list,
            attr_idx,
            new_text: mapped,
        });
    }
    Ok(())
}

/// Validates the u2 Utf8 index that a Signature attribute payload holds.
fn signature_index(a: &crate::remap::RawAttribute) -> Result<u16, Error> {
    if a.payload.len() != 2 {
        return Err(Error {
            message: format!(
                "Signature attribute payload is {} bytes, expected 2",
                a.payload.len()
            ),
            offset: 0,
        });
    }
    Ok(u16::from_be_bytes([a.payload[0], a.payload[1]]))
}

/// Plans a Record attribute: u2 count, then per component name_index +
/// descriptor_index + attribute list (each component attribute can
/// carry its own Signature — collected into the same edit).
fn plan_record(
    ctx: &PlanCtx<'_>,
    attr_idx: usize,
    a: &crate::remap::RawAttribute,
    edits: &mut Vec<AttrEdit>,
) -> Result<(), Error> {
    let raw = ctx.raw;
    let payload = &a.payload;
    if payload.len() < 2 {
        return Err(Error {
            message: "Record attribute payload shorter than the u2 count".into(),
            offset: 0,
        });
    }
    let count = u16::from_be_bytes([payload[0], payload[1]]);
    let mut q = 2usize;
    for comp in 0..usize::from(count) {
        if q + 6 > payload.len() {
            return Err(Error {
                message: format!("Record component {comp} header past the payload"),
                offset: 0,
            });
        }
        let name_index = u16::from_be_bytes([payload[q], payload[q + 1]]);
        let desc_index = u16::from_be_bytes([payload[q + 2], payload[q + 3]]);
        let attr_count = u16::from_be_bytes([payload[q + 4], payload[q + 5]]);
        q += 6;
        let name = raw.pool.utf8(name_index)?;
        let desc = raw.pool.utf8(desc_index)?;
        let mapped_name = ctx
            .ms
            .map_field(ctx.from_ns, ctx.to_ns, &ctx.owner, &desc, &name)
            .map(str::to_string);
        let mapped_desc =
            crate::remap_pass::remap_descriptor(&desc, ctx.ms, ctx.from_ns, ctx.to_ns);
        let new_desc = (mapped_desc != desc).then_some(mapped_desc);
        let mut sigs: Vec<(usize, String)> = Vec::new();
        for k in 0..usize::from(attr_count) {
            if q + 6 > payload.len() {
                return Err(Error {
                    message: format!("Record component {comp} attribute {k} past the payload"),
                    offset: 0,
                });
            }
            let an = u16::from_be_bytes([payload[q], payload[q + 1]]);
            let alen = u32::from_be_bytes([
                payload[q + 2],
                payload[q + 3],
                payload[q + 4],
                payload[q + 5],
            ]) as usize;
            if raw.pool.utf8(an)? == "Signature" {
                if alen != 2 {
                    return Err(Error {
                        message: format!(
                            "Record component {comp} Signature payload is {alen} bytes"
                        ),
                        offset: 0,
                    });
                }
                let sig_index = u16::from_be_bytes([payload[q + 6], payload[q + 7]]);
                let text = raw.pool.utf8(sig_index)?;
                let mapped =
                    crate::remap_pass::remap_descriptor(&text, ctx.ms, ctx.from_ns, ctx.to_ns);
                if mapped != text {
                    sigs.push((k, mapped));
                }
            }
            q += 6 + alen;
        }
        if mapped_name.is_some() || new_desc.is_some() || !sigs.is_empty() {
            edits.push(AttrEdit::Rec {
                attr_idx,
                comp,
                new_name: mapped_name,
                new_desc,
                sigs,
            });
        }
    }
    if q != payload.len() {
        return Err(Error {
            message: format!(
                "Record attribute payload walk ended at {q}, payload holds {} bytes",
                payload.len()
            ),
            offset: 0,
        });
    }
    Ok(())
}

/// Plans an EnclosingMethod attribute: payload = u2 class_index +
/// u2 method_index (0 when the enclosing member does not exist). The
/// method NameAndType hangs off the attribute, so the pool pass's ref
/// walk never reached it; the lookup uses the ORIGINAL enclosing-class
/// name (phase-0 planning, before `remap_with` renames anything).
fn plan_enclosing(
    ctx: &PlanCtx<'_>,
    a: &crate::remap::RawAttribute,
    edits: &mut Vec<AttrEdit>,
) -> Result<(), Error> {
    let raw = ctx.raw;
    if a.payload.len() != 4 {
        return Err(Error {
            message: format!(
                "EnclosingMethod payload is {} bytes, expected 4",
                a.payload.len()
            ),
            offset: 0,
        });
    }
    let class_index = u16::from_be_bytes([a.payload[0], a.payload[1]]);
    let nat_index = u16::from_be_bytes([a.payload[2], a.payload[3]]);
    if nat_index == 0 {
        return Ok(());
    }
    let owner = raw.pool.class_name(class_index)?;
    let (n_idx, d_idx) = raw
        .pool
        .slots
        .get(usize::from(nat_index))
        .and_then(|s| s.as_ref())
        .and_then(|s| s.name_and_type())
        .ok_or_else(|| Error {
            message: format!("cp slot {nat_index} behind EnclosingMethod is not a NameAndType"),
            offset: 0,
        })?;
    let name = raw.pool.utf8(n_idx)?;
    let desc = raw.pool.utf8(d_idx)?;
    let mapped_name = ctx
        .ms
        .map_method(ctx.from_ns, ctx.to_ns, &owner, &desc, &name)
        .map(str::to_string);
    let mapped_desc = crate::remap_pass::remap_descriptor(&desc, ctx.ms, ctx.from_ns, ctx.to_ns);
    let new_desc = (mapped_desc != desc).then_some(mapped_desc);
    if mapped_name.is_some() || new_desc.is_some() {
        edits.push(AttrEdit::Enc {
            nat_slot: nat_index,
            new_name: mapped_name,
            new_desc,
        });
    }
    Ok(())
}

/// Applies the planned edits after the pool pass. The class file is
/// DESTRUCTURED into disjoint field borrows — the interner owns the
/// pool, the attribute lists are reachable independently, so no aliasing
/// ever arises. Attribute LISTS are untouched by the pool pass, so
/// (list, attr_idx) addressing is exact.
fn apply_attrs(
    raw: &mut RawClassFile,
    edits: &[AttrEdit],
    stats: &mut RemapStats,
) -> Result<(), Error> {
    let RawClassFile {
        pool,
        attributes,
        fields,
        methods,
        ..
    } = raw;
    let mut interner = Interner::new(pool);
    for e in edits {
        match e {
            AttrEdit::Sig {
                list,
                attr_idx,
                new_text,
            } => {
                let attr = attr_at_mut(attributes, fields, methods, *list, *attr_idx)?;
                let idx = interner.intern(new_text)?;
                attr.payload[0] = (idx >> 8) as u8;
                attr.payload[1] = idx as u8;
                stats.signatures_rewritten += 1;
            }
            AttrEdit::Rec {
                attr_idx,
                comp,
                new_name,
                new_desc,
                sigs,
            } => {
                // locate the Record attribute by index (immutable scan on
                // the attribute list; the pool reads flow through the
                // interner's borrow — disjoint fields)
                let rec_pos = attributes
                    .iter()
                    .position(|a| {
                        interner
                            .utf8(a.name_index)
                            .map(|n| n == "Record")
                            .unwrap_or(false)
                    })
                    .ok_or_else(|| Error {
                        message: "the Record attribute vanished before apply".into(),
                        offset: 0,
                    })?;
                debug_assert_eq!(*attr_idx, rec_pos, "the plan addressed the Record attr");
                let payload = &mut attributes[rec_pos].payload;
                patch_record_component(payload, *comp, new_name, new_desc, sigs, &mut interner)?;
                if new_name.is_some() {
                    stats.record_parts_renamed += 1;
                }
            }
            AttrEdit::Enc {
                nat_slot,
                new_name,
                new_desc,
            } => {
                let (cur_n, cur_d) = interner.name_and_type(*nat_slot).map_err(|e| Error {
                    message: format!(
                        "cp slot {nat_slot} stopped being a NameAndType: {}",
                        e.message
                    ),
                    offset: 0,
                })?;
                let nn = match new_name {
                    Some(t) => interner.intern(t)?,
                    None => cur_n,
                };
                let nd = match new_desc {
                    Some(t) => interner.intern(t)?,
                    None => cur_d,
                };
                interner.set_name_and_type_slot(*nat_slot, nn, nd);
                stats.enclosing_renamed += 1;
            }
        }
    }
    stats.utf8_appended += interner.appended();
    Ok(())
}

/// Resolves an (AttrList, attr_idx) to its mutable attribute over the
/// three disjoint attribute-list borrows. Record components never come
/// through here — their signatures patch inside the payload walk
/// (`patch_record_component`).
fn attr_at_mut<'a>(
    attributes: &'a mut [crate::remap::RawAttribute],
    fields: &'a mut [crate::remap::RawMember],
    methods: &'a mut [crate::remap::RawMember],
    list: AttrList,
    attr_idx: usize,
) -> Result<&'a mut crate::remap::RawAttribute, Error> {
    let slot = match list {
        AttrList::Class => attributes.get_mut(attr_idx),
        AttrList::Field(i) => fields
            .get_mut(i)
            .and_then(|m| m.attributes.get_mut(attr_idx)),
        AttrList::Method(i) => methods
            .get_mut(i)
            .and_then(|m| m.attributes.get_mut(attr_idx)),
    };
    slot.ok_or_else(|| Error {
        message: format!("attribute {attr_idx} vanished before apply"),
        offset: 0,
    })
}

/// Patches one Record component's name/descriptor indexes and its
/// Signature texts inside the payload (the walk mirrors `plan_record`;
/// the payload length is untouched — u2 swaps only).
fn patch_record_component(
    payload: &mut [u8],
    comp: usize,
    new_name: &Option<String>,
    new_desc: &Option<String>,
    sigs: &[(usize, String)],
    interner: &mut Interner<'_>,
) -> Result<(), Error> {
    if payload.len() < 2 {
        return Err(Error {
            message: "Record payload shorter than the u2 count".into(),
            offset: 0,
        });
    }
    let count = u16::from_be_bytes([payload[0], payload[1]]) as usize;
    if comp >= count {
        return Err(Error {
            message: format!("Record component {comp} out of range ({count} components)"),
            offset: 0,
        });
    }
    let mut q = 2usize;
    for k in 0..count {
        let name_at = q;
        let desc_at = q + 2;
        let attr_count = u16::from_be_bytes([payload[q + 4], payload[q + 5]]) as usize;
        q += 6;
        for a in 0..attr_count {
            let alen = u32::from_be_bytes([
                payload[q + 2],
                payload[q + 3],
                payload[q + 4],
                payload[q + 5],
            ]) as usize;
            if k == comp {
                if let Some((attr_k, new_text)) = sigs.iter().find(|(i, _)| *i == a) {
                    debug_assert_eq!(*attr_k, a);
                    let idx = interner.intern(new_text)?;
                    payload[q + 6] = (idx >> 8) as u8;
                    payload[q + 7] = idx as u8;
                }
            }
            q += 6 + alen;
        }
        if k == comp {
            if let Some(name) = new_name {
                let idx = interner.intern(name)?;
                payload[name_at] = (idx >> 8) as u8;
                payload[name_at + 1] = idx as u8;
            }
            if let Some(desc) = new_desc {
                let idx = interner.intern(desc)?;
                payload[desc_at] = (idx >> 8) as u8;
                payload[desc_at + 1] = idx as u8;
            }
        }
    }
    if q != payload.len() {
        return Err(Error {
            message: format!(
                "Record payload walk ended at {q}, payload holds {} bytes",
                payload.len()
            ),
            offset: 0,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remap_pass::remap_with;
    use j2r_mappings::{ClassEntry, MemberEntry};

    const VEC2: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec2.class");
    const VEC2_INNER: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec2$Inner.class");
    const VEC2_BOX: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec2$Box.class");
    const VEC2_SHAPE: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec2$Shape.class");
    const VEC3: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec3.class");
    const VEC3_PAIR: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec3$Pair.class");
    const VEC3_TONE: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec3$Tone.class");
    const VEC1: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec1.class");

    const ALL: &[(&str, &[u8])] = &[
        ("vec1", VEC1),
        ("vec2", VEC2),
        ("vec2$Inner", VEC2_INNER),
        ("vec2$Box", VEC2_BOX),
        ("vec2$Shape", VEC2_SHAPE),
        ("vec3", VEC3),
        ("vec3$Pair", VEC3_PAIR),
        ("vec3$Tone", VEC3_TONE),
    ];

    /// A two-namespace class map (`from -> to` rows).
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

    /// A two-namespace map with one FIELD row on `owner`.
    fn tiny_field_map(
        classes: &[(&str, &str)],
        owner: &str,
        desc: &str,
        from: &str,
        to: &str,
    ) -> MappingSet {
        let mut set = MappingSet::default();
        set.namespaces = vec!["src".to_string(), "dst".to_string()];
        set.classes = classes
            .iter()
            .map(|(f, t)| ClassEntry {
                names: vec![Some(f.to_string()), Some(t.to_string())],
                ..Default::default()
            })
            .collect();
        let entry = set
            .classes
            .iter_mut()
            .find(|c| c.names[0].as_deref() == Some(owner))
            .expect("the owner must carry a class row");
        entry.fields.push(MemberEntry {
            desc: desc.to_string(),
            names: vec![Some(from.to_string()), Some(to.to_string())],
            params: Vec::new(),
        });
        set.index()
    }

    /// EVERY Signature attribute's (location, text) — class-level,
    /// field-level and method-level (the probes located the interesting
    /// texts on FIELDS: VEC2's ledger is a List<Map<String,Integer>>,
    /// VEC3$Tone's $VALUES is an Enum<RemapVec3$Tone>).
    fn all_signatures(raw: &RawClassFile) -> Vec<(AttrList, usize, String)> {
        let mut out = Vec::new();
        for (ai, a) in raw.attributes.iter().enumerate() {
            if raw
                .pool
                .utf8(a.name_index)
                .map(|n| n == "Signature")
                .unwrap_or(false)
            {
                let si = u16::from_be_bytes([a.payload[0], a.payload[1]]);
                out.push((AttrList::Class, ai, raw.pool.utf8(si).unwrap()));
            }
        }
        for (i, m) in raw.fields.iter().enumerate() {
            for (ai, a) in m.attributes.iter().enumerate() {
                if raw
                    .pool
                    .utf8(a.name_index)
                    .map(|n| n == "Signature")
                    .unwrap_or(false)
                {
                    let si = u16::from_be_bytes([a.payload[0], a.payload[1]]);
                    out.push((AttrList::Field(i), ai, raw.pool.utf8(si).unwrap()));
                }
            }
        }
        for (i, m) in raw.methods.iter().enumerate() {
            for (ai, a) in m.attributes.iter().enumerate() {
                if raw
                    .pool
                    .utf8(a.name_index)
                    .map(|n| n == "Signature")
                    .unwrap_or(false)
                {
                    let si = u16::from_be_bytes([a.payload[0], a.payload[1]]);
                    out.push((AttrList::Method(i), ai, raw.pool.utf8(si).unwrap()));
                }
            }
        }
        out
    }

    /// Law A1: the class-level Signature TEXT remaps through the shared
    /// descriptor walker (VEC2's probed fact:
    /// `Ljava/util/List<Ljava/util/Map<Ljava/lang/String;Ljava/lang/Integer;>;>;`),
    /// as an APPENDED Utf8 with the attribute repointed — the payload
    /// length stays 2 and the original text slot survives.
    #[test]
    fn signature_text_remaps_as_text() {
        let ms = tiny_class_map(&[
            ("java/util/List", "s/Lst"),
            ("java/util/Map", "s/Map"),
            ("java/lang/String", "s/Str"),
        ]);
        let before = RawClassFile::parse(VEC2).unwrap();
        let sigs = all_signatures(&before);
        let (list, ai, old_sig) = sigs
            .iter()
            .find(|(_, _, s)| {
                s == "Ljava/util/List<Ljava/util/Map<Ljava/lang/String;Ljava/lang/Integer;>;>;"
            })
            .expect("the probed ledger Signature must exist")
            .clone();
        assert!(
            matches!(list, AttrList::Field(0)),
            "the probed text lives on the ledger FIELD: {sigs:?}"
        );
        let old_payload = match list {
            AttrList::Field(i) => before.fields[i].attributes[ai].payload.clone(),
            _ => unreachable!(),
        };

        let mut raw = RawClassFile::parse(VEC2).unwrap();
        let stats = remap_class(&mut raw, &ms, 0, 1).unwrap();
        assert_eq!(stats.signatures_rewritten, 1, "exactly one Signature maps");

        let after = all_signatures(&raw);
        let new_sig = after
            .iter()
            .find(|(_, _, s)| s == "Ls/Lst<Ls/Map<Ls/Str;Ljava/lang/Integer;>;>;")
            .unwrap_or_else(|| panic!("the remapped signature text is missing; got {after:?}"));
        assert_eq!(new_sig.0, list, "the attribute stayed on the ledger field");
        // the attribute repointed at an appended Utf8; the payload is
        // still 2 bytes; the OLD text slot survives untouched
        let payload = &raw.fields[0].attributes[new_sig.1].payload;
        assert_eq!(payload.len(), 2, "payload length stable");
        assert_ne!(*payload, old_payload, "the index moved");
        let new_idx = u16::from_be_bytes([payload[0], payload[1]]);
        assert_eq!(
            raw.pool.utf8(new_idx).unwrap(),
            "Ls/Lst<Ls/Map<Ls/Str;Ljava/lang/Integer;>;>;"
        );
        assert!(
            raw.pool
                .slots
                .iter()
                .flatten()
                .any(|e| e.utf8_str().as_deref() == Some(old_sig.as_str())),
            "the original Signature Utf8 slot survives (append, never replace)"
        );
    }

    /// Law A2: a MEMBER-level Signature remaps the same way (VEC3$Tone's
    /// probed fact: `Ljava/lang/Enum<LRemapVec3$Tone;>;` on a member).
    #[test]
    fn member_signature_text_remaps_as_text() {
        let ms = tiny_class_map(&[("RemapVec3$Tone", "mapped/Tone")]);
        let before = RawClassFile::parse(VEC3_TONE).unwrap();
        let before_sigs: Vec<String> = all_signatures(&before)
            .into_iter()
            .map(|(_, _, s)| s)
            .collect();
        assert!(
            before_sigs
                .iter()
                .any(|s| s == "Ljava/lang/Enum<LRemapVec3$Tone;>;"),
            "the probed member Signature must exist: {before_sigs:?}"
        );

        let mut raw = RawClassFile::parse(VEC3_TONE).unwrap();
        let stats = remap_class(&mut raw, &ms, 0, 1).unwrap();
        assert!(stats.signatures_rewritten >= 1);
        let after: Vec<String> = all_signatures(&raw)
            .into_iter()
            .map(|(_, _, s)| s)
            .collect();
        assert!(
            after.iter().any(|s| s == "Ljava/lang/Enum<Lmapped/Tone;>;"),
            "the member Signature moved with the class: {after:?}"
        );
        assert!(
            after.iter().any(|s| s == "()V"),
            "a signature with no class references stays verbatim"
        );
    }

    /// Law A3: Record components remap name-for-name (a component IS a
    /// field of the record class) and their descriptors remap as text —
    /// the Record payload length NEVER moves (u2 swaps only). Facts:
    /// VEC3$Pair carries `k: I` and `v: Ljava/lang/String;`.
    #[test]
    fn record_components_get_name_for_name() {
        let ms = tiny_field_map(
            &[
                ("RemapVec3$Pair", "mapped/Pair"),
                ("java/lang/String", "s/Str"),
            ],
            "RemapVec3$Pair",
            "I",
            "k",
            "kk",
        );
        let before = RawClassFile::parse(VEC3_PAIR).unwrap();
        let rec = before
            .attributes
            .iter()
            .find(|a| before.pool.utf8(a.name_index).unwrap() == "Record")
            .expect("the probed Record attribute");
        let len_before = rec.payload.len();
        assert_eq!(len_before, 14, "the probed Record payload length");

        let mut raw = RawClassFile::parse(VEC3_PAIR).unwrap();
        let stats = remap_class(&mut raw, &ms, 0, 1).unwrap();
        assert_eq!(stats.record_parts_renamed, 1, "only k mapped");

        let rec = raw
            .attributes
            .iter()
            .find(|a| raw.pool.utf8(a.name_index).unwrap() == "Record")
            .expect("the Record attribute survives");
        assert_eq!(rec.payload.len(), len_before, "payload length stable");
        let rc = u16::from_be_bytes([rec.payload[0], rec.payload[1]]);
        assert_eq!(usize::from(rc), 2);
        // component 0 = k -> kk (name moved, desc I kept)
        let (n0, d0) = (
            u16::from_be_bytes([rec.payload[2], rec.payload[3]]),
            u16::from_be_bytes([rec.payload[4], rec.payload[5]]),
        );
        assert_eq!(raw.pool.utf8(n0).unwrap(), "kk");
        assert_eq!(raw.pool.utf8(d0).unwrap(), "I");
        // component 1 = v: name kept, desc remapped as TEXT
        let (n1, d1) = (
            u16::from_be_bytes([rec.payload[8], rec.payload[9]]),
            u16::from_be_bytes([rec.payload[10], rec.payload[11]]),
        );
        assert_eq!(raw.pool.utf8(n1).unwrap(), "v");
        assert_eq!(raw.pool.utf8(d1).unwrap(), "Ls/Str;");
        // the output re-verifies
        let out = raw.serialize();
        assert_eq!(RawClassFile::parse(&out).unwrap().serialize(), out);
    }

    /// Law A4 (THE design fact): every CLASS-referencing attribute
    /// payload is BYTE-IDENTICAL across the full remap — Exceptions,
    /// InnerClasses, NestMembers, Code (StackMapTable inside) ride
    /// untouched because the pool pass retargets the Class entries in
    /// place and these payloads hold indexes, not names.
    #[test]
    fn class_referencing_attribute_payloads_stay_byte_identical() {
        let ms = tiny_class_map(&[
            ("java/lang/String", "s/Str"),
            ("java/util/List", "s/Lst"),
            ("java/util/Map", "s/Map"),
            ("RemapVec2", "mapped/RemapVec2"),
            ("RemapVec2$Box", "mapped/Box"),
            ("RemapVec2$Shape", "mapped/Shape"),
            ("RemapVec2$Inner", "mapped/Inner"),
        ]);
        let before = RawClassFile::parse(VEC2).unwrap();
        let mut raw = RawClassFile::parse(VEC2).unwrap();
        remap_class(&mut raw, &ms, 0, 1).unwrap();
        for name in ["Exceptions", "InnerClasses", "NestMembers", "Code"] {
            let grab = |r: &RawClassFile| -> Vec<Vec<u8>> {
                let mut out = Vec::new();
                for a in &r.attributes {
                    if r.pool.utf8(a.name_index).unwrap() == name {
                        out.push(a.payload.clone());
                    }
                }
                for m in r.fields.iter().chain(r.methods.iter()) {
                    for a in &m.attributes {
                        if r.pool.utf8(a.name_index).unwrap() == name {
                            out.push(a.payload.clone());
                        }
                    }
                }
                out
            };
            let was = grab(&before);
            let now = grab(&raw);
            assert!(
                !was.is_empty(),
                "{name} must be present on VEC2 (the census proved it)"
            );
            assert_eq!(
                was, now,
                "{name} payloads must be byte-identical (index-stable design)"
            );
        }
    }

    /// Law A5: the EnclosingMethod NameAndType (attribute-hung, invisible
    /// to the pool pass's ref walk) remaps against the ORIGINAL
    /// enclosing-class name — pinned on a hand-built class file (no
    /// vector carries EnclosingMethod; javac emits it for local and
    /// anonymous classes only).
    #[test]
    fn enclosing_method_nam_and_type_remaps() {
        // pool: 1 Utf8 "EnclosingMethod", 2 Utf8 "demo/Outer", 3 Utf8
        // "lambda$go$0", 4 Utf8 "()V", 5 Utf8 "demo/Inner", 6 Class->2,
        // 7 NameAndType->3,4, 8 Class->5
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE]);
        bytes.extend_from_slice(&[0, 0, 0, 55]);
        bytes.extend_from_slice(&[0, 9]);
        let utf8s: Vec<&[u8]> = vec![
            b"EnclosingMethod",
            b"demo/Outer",
            b"lambda$go$0",
            b"()V",
            b"demo/Inner",
        ];
        // slot 1..5 = the Utf8s (with length prefixes)
        for u in &utf8s {
            bytes.push(1);
            bytes.extend_from_slice(&(u.len() as u16).to_be_bytes());
            bytes.extend_from_slice(u);
        }
        bytes.extend_from_slice(&[7, 0, 2]); // slot 6: Class -> demo/Outer
        bytes.extend_from_slice(&[12, 0, 3, 0, 4]); // slot 7: NameAndType
        bytes.extend_from_slice(&[7, 0, 5]); // slot 8: Class -> demo/Inner
        bytes.extend_from_slice(&[0x00, 0x21]); // ACC_PUBLIC | ACC_SUPER
        bytes.extend_from_slice(&[0, 8]); // this_class -> demo/Inner
        bytes.extend_from_slice(&[0, 6]); // super_class -> demo/Outer
        bytes.extend_from_slice(&[0, 0]); // no interfaces
        bytes.extend_from_slice(&[0, 0]); // no fields
        bytes.extend_from_slice(&[0, 0]); // no methods
        bytes.extend_from_slice(&[0, 1]); // one class attribute
        bytes.extend_from_slice(&[0, 1]); // name -> EnclosingMethod
        bytes.extend_from_slice(&[0, 0, 0, 4]); // length 4
        bytes.extend_from_slice(&[0, 6, 0, 7]); // class 6, method NameAndType 7

        let ms = {
            let mut set = MappingSet::default();
            set.namespaces = vec!["src".to_string(), "dst".to_string()];
            set.classes = vec![ClassEntry {
                names: vec![
                    Some("demo/Outer".to_string()),
                    Some("mapped/Outer".to_string()),
                ],
                ..Default::default()
            }];
            let e = &mut set.classes[0];
            e.methods.push(MemberEntry {
                desc: "()V".to_string(),
                names: vec![
                    Some("lambda$go$0".to_string()),
                    Some("lambda$renamed$0".to_string()),
                ],
                params: Vec::new(),
            });
            set.index()
        };
        let mut raw = RawClassFile::parse(&bytes).unwrap();
        let stats = remap_class(&mut raw, &ms, 0, 1).unwrap();
        assert_eq!(stats.classes_renamed, 1, "only demo/Outer is mapped");
        assert_eq!(stats.enclosing_renamed, 1);
        // the EnclosingMethod NameAndType moved to the mapped name
        let nat = raw.pool.slots[7].as_ref().unwrap();
        let (n_idx, d_idx) = nat.name_and_type().unwrap();
        assert_eq!(raw.pool.utf8(n_idx).unwrap(), "lambda$renamed$0");
        assert_eq!(
            raw.pool.utf8(d_idx).unwrap(),
            "()V",
            "unmapped desc keeps its slot"
        );
        // the EnclosingMethod attribute payload itself is index-stable
        let attr = raw
            .attributes
            .iter()
            .find(|a| raw.pool.utf8(a.name_index).unwrap() == "EnclosingMethod")
            .unwrap();
        assert_eq!(attr.payload, [0, 6, 0, 7], "payload indexes never move");
        // the output re-verifies: the resolver reads the mapped owner
        let out = raw.serialize();
        assert_eq!(RawClassFile::parse(&out).unwrap().serialize(), out);
    }

    /// Law A6: the identity law through the FULL entry — an empty map is
    /// byte-exact on every vector and every stat stays zero.
    #[test]
    fn empty_map_is_a_byte_exact_identity_through_remap_class() {
        let ms = tiny_class_map(&[]);
        for (name, bytes) in ALL.iter().copied() {
            let mut raw = RawClassFile::parse(bytes)
                .unwrap_or_else(|e| panic!("{name}: parse failed: {}", e.message));
            let stats = remap_class(&mut raw, &ms, 0, 1).expect("identity pass");
            assert_eq!(raw.serialize(), bytes, "{name}: identity moved bytes");
            assert_eq!(
                stats,
                RemapStats::default(),
                "{name}: identity counted work"
            );
        }
    }

    /// Law A7: every attribute payload LENGTH is stable across a heavy
    /// map (u2 swaps and in-place fields only — the whole attribute
    /// envelope must not churn).
    #[test]
    fn every_attribute_payload_length_is_stable() {
        let ms = tiny_class_map(&[
            ("java/lang/String", "s/Str"),
            ("java/util/List", "s/Lst"),
            ("java/util/Map", "s/Map"),
            ("RemapVec3$Pair", "mapped/Pair"),
            ("RemapVec3$Tone", "mapped/Tone"),
            ("RemapVec3", "mapped/RemapVec3"),
        ]);
        for (name, bytes) in [
            ("vec2", VEC2),
            ("vec3$Pair", VEC3_PAIR),
            ("vec3$Tone", VEC3_TONE),
        ] {
            let before = RawClassFile::parse(bytes).unwrap();
            let mut raw = RawClassFile::parse(bytes).unwrap();
            remap_class(&mut raw, &ms, 0, 1).unwrap();
            let lens = |r: &RawClassFile| -> Vec<usize> {
                let mut out = Vec::new();
                for a in &r.attributes {
                    out.push(a.payload.len());
                }
                for m in r.fields.iter().chain(r.methods.iter()) {
                    for a in &m.attributes {
                        out.push(a.payload.len());
                    }
                }
                out
            };
            assert_eq!(
                lens(&before),
                lens(&raw),
                "{name}: attribute payload lengths must not move"
            );
        }
    }

    /// Law A8: remap_with (the pool-only pass) is INDEPENDENT of the
    /// attribute layer — calling it alone leaves every Signature text
    /// verbatim (the attribute layer's job), which pins the layering.
    #[test]
    fn pool_only_pass_leaves_signature_texts_alone() {
        let ms = tiny_class_map(&[("java/util/List", "s/Lst")]);
        let mut raw = RawClassFile::parse(VEC2).unwrap();
        remap_with(&mut raw, &ms, 0, 1).unwrap();
        let sigs: Vec<String> = all_signatures(&raw)
            .into_iter()
            .map(|(_, _, s)| s)
            .collect();
        assert!(
            sigs.iter().any(|s| s == "Ljava/util/List<Ljava/util/Map<Ljava/lang/String;Ljava/lang/Integer;>;>;"),
            "the TEXT is untouched by the pool pass (references inside are not pool entries): {sigs:?}"
        );
    }

    /// Law A9: the full entry re-verifies on the resolver and
    /// re-serializes byte-stably on every vector under a heavy map.
    #[test]
    fn remapped_vectors_reverify_through_remap_class() {
        for (name, bytes) in ALL.iter().copied() {
            let raw = RawClassFile::parse(bytes)
                .unwrap_or_else(|e| panic!("{name}: parse failed: {}", e.message));
            let own = raw.this_class_name().unwrap();
            let ms = tiny_class_map(&[
                (&own, &format!("mapped/{own}")),
                ("java/lang/String", "s/Str"),
                ("java/util/List", "s/Lst"),
            ]);
            let mut raw = RawClassFile::parse(bytes).unwrap();
            remap_class(&mut raw, &ms, 0, 1).unwrap();
            let out = raw.serialize();
            let resolved = crate::ClassFile::parse(&out).unwrap_or_else(|e| {
                panic!("{name}: the remapped class must re-verify: {}", e.message)
            });
            assert_eq!(resolved.this_class, format!("mapped/{own}"));
            assert_eq!(
                RawClassFile::parse(&out).unwrap().serialize(),
                out,
                "{name}: byte-stable re-serialization"
            );
        }
    }
}
