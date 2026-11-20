//! T15.2c — the Code-internal variable tables: LocalVariableTable
//! descriptors and LocalVariableTypeTable signatures (TODO.md T15, M5
//! wave 3). The final text layer of the remap pass.
//!
//! PROBE FACT (scripts/probe_vec_attrs.py, pinned by law C5): NO
//! committed vector carries either table — javac emits them only under
//! `-g:vars`, and the vectors were compiled without it — so the laws
//! ride a hand-built Code attribute (the A5-style synthetic shape).
//!
//! The rewrites are pure TEXT rewrites with no owner lookups (a local
//! variable is not a member of anything): each table entry carries one
//! u2 index to a Utf8 descriptor (LVT) or signature (LVTT) text, and
//! the shared descriptor walker remaps that text exactly like a member
//! descriptor. That means the layer can plan AND apply AFTER the pool
//! pass — the texts it reads were never touched by the pool pass (the
//! pool pass only retargets pool entries, and these Utf8 slots are not
//! entries any ref points at). Every rewrite is a u2 index swap INSIDE
//! the Code payload: payload length stable, the Code envelope
//! unchanged, the exception table and StackMapTable ride byte-identical
//! (law C2), and the A4 law (class-referencing payloads byte-identical)
//! keeps holding — its vectors carry no variable tables.
//!
//! Layout walked here (JVMS §4.7.3 + §4.7.13/§4.7.14): Code payload =
//! u2 max_stack, u2 max_locals, u4 code_length, code bytes, u2
//! exception_table_length + 8×len bytes, u2 attributes_count, then
//! nested attributes (u2 name, u4 len, body). LocalVariableTable body =
//! u2 count + count×(u2 start_pc, u2 length, u2 name_index, u2
//! descriptor_index, u2 index); LocalVariableTypeTable is the same
//! shape with signature_index in the same slot. The variable-type index
//! sits at entry offset 6 in both.

use j2r_mappings::MappingSet;

use crate::remap::RawClassFile;
use crate::remap_pass::{remap_descriptor, Interner, RemapStats};
use crate::Error;

/// The T15.2c layer: remap every LocalVariableTable descriptor text and
/// LocalVariableTypeTable signature text inside every method's Code
/// attribute. Runs after the pool pass and the attribute layer (see the
/// module docs for why no separate plan phase is needed here).
pub fn remap_code_tables(
    raw: &mut RawClassFile,
    ms: &MappingSet,
    from_ns: usize,
    to_ns: usize,
    stats: &mut RemapStats,
) -> Result<(), Error> {
    let RawClassFile { pool, methods, .. } = raw;
    let mut interner = Interner::new(pool);
    for m in methods.iter_mut() {
        for a in m.attributes.iter_mut() {
            if interner.utf8(a.name_index)? != "Code" {
                continue;
            }
            patch_code_payload(&mut a.payload, ms, from_ns, to_ns, &mut interner, stats)?;
        }
    }
    stats.utf8_appended += interner.appended();
    Ok(())
}

/// Walks one Code payload and patches the variable-table texts in place.
fn patch_code_payload(
    payload: &mut [u8],
    ms: &MappingSet,
    from_ns: usize,
    to_ns: usize,
    interner: &mut Interner<'_>,
    stats: &mut RemapStats,
) -> Result<(), Error> {
    if payload.len() < 8 {
        return Err(Error {
            message: "Code payload shorter than its fixed header".into(),
            offset: 0,
        });
    }
    let code_len = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]) as usize;
    let mut q = 8 + code_len;
    if q + 2 > payload.len() {
        return Err(Error {
            message: "Code payload ends before the exception table".into(),
            offset: 0,
        });
    }
    let exc_count = u16::from_be_bytes([payload[q], payload[q + 1]]) as usize;
    q += 2 + exc_count * 8;
    if q + 2 > payload.len() {
        return Err(Error {
            message: "Code payload ends before the nested attribute count".into(),
            offset: 0,
        });
    }
    let attr_count = u16::from_be_bytes([payload[q], payload[q + 1]]) as usize;
    q += 2;
    for _ in 0..attr_count {
        if q + 6 > payload.len() {
            return Err(Error {
                message: "nested Code attribute header past the payload".into(),
                offset: 0,
            });
        }
        let name_index = u16::from_be_bytes([payload[q], payload[q + 1]]);
        let alen = u32::from_be_bytes([
            payload[q + 2],
            payload[q + 3],
            payload[q + 4],
            payload[q + 5],
        ]) as usize;
        let name = interner.utf8(name_index)?;
        if name == "LocalVariableTable" || name == "LocalVariableTypeTable" {
            patch_variable_table(
                &mut payload[q + 6..q + 6 + alen],
                ms,
                from_ns,
                to_ns,
                interner,
                stats,
            )?;
        }
        q += 6 + alen;
    }
    if q != payload.len() {
        return Err(Error {
            message: format!(
                "Code payload walk ended at {q}, payload holds {} bytes",
                payload.len()
            ),
            offset: 0,
        });
    }
    Ok(())
}

/// Patches one LocalVariableTable/LocalVariableTypeTable body: u2 count,
/// then count×(start_pc, length, name_index, type_index, index). The
/// type index at entry offset 6 is the Utf8 whose TEXT remaps.
fn patch_variable_table(
    body: &mut [u8],
    ms: &MappingSet,
    from_ns: usize,
    to_ns: usize,
    interner: &mut Interner<'_>,
    stats: &mut RemapStats,
) -> Result<(), Error> {
    if body.len() < 2 {
        return Err(Error {
            message: "variable table body shorter than the u2 count".into(),
            offset: 0,
        });
    }
    let count = u16::from_be_bytes([body[0], body[1]]) as usize;
    if body.len() != 2 + count * 10 {
        return Err(Error {
            message: format!(
                "variable table body holds {} bytes, expected {} for {count} entries",
                body.len(),
                2 + count * 10
            ),
            offset: 0,
        });
    }
    for i in 0..count {
        let at = 2 + i * 10 + 6;
        let idx = u16::from_be_bytes([body[at], body[at + 1]]);
        let text = interner.utf8(idx)?;
        let mapped = remap_descriptor(&text, ms, from_ns, to_ns);
        if mapped != text {
            let new_idx = interner.intern(&mapped)?;
            body[at] = (new_idx >> 8) as u8;
            body[at + 1] = new_idx as u8;
            stats.code_vars_rewritten += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use j2r_mappings::{ClassEntry, MemberEntry};
    use std::sync::Mutex;

    const ALL: &[&[u8]] = &[
        include_bytes!("../tests/vectors/remap/RemapVec1.class"),
        include_bytes!("../tests/vectors/remap/RemapVec2.class"),
        include_bytes!("../tests/vectors/remap/RemapVec2$Inner.class"),
        include_bytes!("../tests/vectors/remap/RemapVec2$Box.class"),
        include_bytes!("../tests/vectors/remap/RemapVec2$Shape.class"),
        include_bytes!("../tests/vectors/remap/RemapVec3.class"),
        include_bytes!("../tests/vectors/remap/RemapVec3$Pair.class"),
        include_bytes!("../tests/vectors/remap/RemapVec3$Tone.class"),
    ];

    /// Serializes the hand-built bytes builder (the laws below share it).
    static BUILD_LOCK: Mutex<()> = Mutex::new(());

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

    /// Builds a minimal class file whose ONE method carries a Code
    /// attribute with a LocalVariableTable (one entry, desc
    /// `Ljava/lang/String;`) and a LocalVariableTypeTable (one entry,
    /// sig `Ljava/util/List<Ljava/lang/String;>;`), plus an exception
    /// table (one entry) so law C2 can pin the neighbors.
    fn synthetic_with_variable_tables() -> Vec<u8> {
        let _guard = BUILD_LOCK.lock().unwrap();
        // pool slots: 1 Utf8 "Code", 2 Utf8 "LocalVariableTable",
        // 3 Utf8 "LocalVariableTypeTable", 4 Utf8 "demo/Syn",
        // 5 Class->4, 6 Utf8 "<init>", 7 Utf8 "()V", 8 Utf8 "x",
        // 9 Utf8 "Ljava/lang/String;", 10 Utf8 "xs",
        // 11 Utf8 "Ljava/util/List<Ljava/lang/String;>;",
        // 12 Utf8 "java/lang/Throwable", 13 Class->12,
        // 14 Utf8 "demo/Syn$Inner" (unused-but-valid filler name)
        let mut p: Vec<u8> = Vec::new();
        let mut utf8 = |slot_text: &str| {
            p.push(1);
            let e = crate::remap::encode_modified_utf8(slot_text);
            p.extend_from_slice(&(e.len() as u16).to_be_bytes());
            p.extend_from_slice(&e);
        };
        utf8("Code");
        utf8("LocalVariableTable");
        utf8("LocalVariableTypeTable");
        utf8("demo/Syn");
        utf8("<init>");
        utf8("()V");
        utf8("x");
        utf8("Ljava/lang/String;");
        utf8("xs");
        utf8("Ljava/util/List<Ljava/lang/String;>;");
        utf8("java/lang/Throwable");
        // slots 1..11 are Utf8s; 12 = Class->"demo/Syn" (slot 4),
        // 13 = Class->"java/lang/Throwable" (slot 11)
        p.push(7);
        p.extend_from_slice(&4u16.to_be_bytes());
        p.push(7);
        p.extend_from_slice(&11u16.to_be_bytes());
        let pool_slots: Vec<u8> = p;

        // the Code attribute body
        let mut code: Vec<u8> = Vec::new();
        code.extend_from_slice(&0u16.to_be_bytes()); // max_stack
        code.extend_from_slice(&2u16.to_be_bytes()); // max_locals
        code.extend_from_slice(&1u32.to_be_bytes()); // code_length
        code.push(0xB1); // return
        code.extend_from_slice(&1u16.to_be_bytes()); // exception_table_length
                                                     // exception entry: start 0, end 1, handler 1, catch_type -> Throwable (slot 13)
        code.extend_from_slice(&0u16.to_be_bytes());
        code.extend_from_slice(&1u16.to_be_bytes());
        code.extend_from_slice(&1u16.to_be_bytes());
        code.extend_from_slice(&13u16.to_be_bytes());
        code.extend_from_slice(&2u16.to_be_bytes()); // nested attributes
                                                     // LocalVariableTable: name 2, len 12
        code.extend_from_slice(&2u16.to_be_bytes());
        code.extend_from_slice(&12u32.to_be_bytes());
        code.extend_from_slice(&1u16.to_be_bytes()); // one entry
        code.extend_from_slice(&0u16.to_be_bytes()); // start_pc
        code.extend_from_slice(&1u16.to_be_bytes()); // length
        code.extend_from_slice(&7u16.to_be_bytes()); // name -> x
        code.extend_from_slice(&8u16.to_be_bytes()); // desc -> Ljava/lang/String;
        code.extend_from_slice(&0u16.to_be_bytes()); // slot index
                                                     // LocalVariableTypeTable: name 3, len 12
        code.extend_from_slice(&3u16.to_be_bytes());
        code.extend_from_slice(&12u32.to_be_bytes());
        code.extend_from_slice(&1u16.to_be_bytes());
        code.extend_from_slice(&0u16.to_be_bytes());
        code.extend_from_slice(&1u16.to_be_bytes());
        code.extend_from_slice(&9u16.to_be_bytes()); // name -> xs
        code.extend_from_slice(&10u16.to_be_bytes()); // sig -> Ljava/util/List<...>;
        code.extend_from_slice(&1u16.to_be_bytes()); // slot index

        // assemble the class file
        let mut b = Vec::new();
        b.extend_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE]);
        b.extend_from_slice(&[0, 0, 0, 55]); // minor 0, major 55
        let pool_count = 14u16; // slot 0 + 13 entries
        b.extend_from_slice(&pool_count.to_be_bytes());
        b.extend_from_slice(&pool_slots);
        b.extend_from_slice(&0x0021u16.to_be_bytes()); // ACC_PUBLIC | ACC_SUPER
        b.extend_from_slice(&12u16.to_be_bytes()); // this_class -> demo/Syn
        b.extend_from_slice(&13u16.to_be_bytes()); // super_class -> Throwable
        b.extend_from_slice(&0u16.to_be_bytes()); // no interfaces
        b.extend_from_slice(&0u16.to_be_bytes()); // no fields
        b.extend_from_slice(&1u16.to_be_bytes()); // one method
        b.extend_from_slice(&0x0001u16.to_be_bytes()); // ACC_PUBLIC
        b.extend_from_slice(&5u16.to_be_bytes()); // name -> <init>
        b.extend_from_slice(&6u16.to_be_bytes()); // desc -> ()V
        b.extend_from_slice(&1u16.to_be_bytes()); // one attribute
        b.extend_from_slice(&1u16.to_be_bytes()); // name -> Code
        b.extend_from_slice(&(code.len() as u32).to_be_bytes());
        b.extend_from_slice(&code);
        b.extend_from_slice(&0u16.to_be_bytes()); // no class attributes
        b
    }

    /// The LVT/LVTT texts inside the synthetic's method attribute.
    fn variable_texts(raw: &RawClassFile) -> (Vec<String>, Vec<String>) {
        let mut lvt = Vec::new();
        let mut lvtt = Vec::new();
        let m = &raw.methods[0];
        for a in &m.attributes {
            if raw.pool.utf8(a.name_index).unwrap() != "Code" {
                continue;
            }
            let body = &a.payload;
            let code_len = u32::from_be_bytes([body[4], body[5], body[6], body[7]]) as usize;
            let mut q = 8 + code_len;
            let exc = u16::from_be_bytes([body[q], body[q + 1]]) as usize;
            q += 2 + exc * 8;
            let ac = u16::from_be_bytes([body[q], body[q + 1]]) as usize;
            q += 2;
            for _ in 0..ac {
                let ni = u16::from_be_bytes([body[q], body[q + 1]]);
                let al = u32::from_be_bytes([body[q + 2], body[q + 3], body[q + 4], body[q + 5]])
                    as usize;
                let name = raw.pool.utf8(ni).unwrap();
                let seg = &body[q + 6..q + 6 + al];
                if name == "LocalVariableTable" || name == "LocalVariableTypeTable" {
                    let n = u16::from_be_bytes([seg[0], seg[1]]) as usize;
                    for i in 0..n {
                        let at = 2 + i * 10 + 6;
                        let ti = u16::from_be_bytes([seg[at], seg[at + 1]]);
                        let text = raw.pool.utf8(ti).unwrap();
                        if name == "LocalVariableTable" {
                            lvt.push(text);
                        } else {
                            lvtt.push(text);
                        }
                    }
                }
                q += 6 + al;
            }
        }
        (lvt, lvtt)
    }

    /// Law C1: the LVT descriptors and LVTT signatures remap as TEXT
    /// inside the Code payload — index swaps only, payload length
    /// stable, the output re-verifies.
    #[test]
    fn code_variable_tables_remap_as_text() {
        let bytes = synthetic_with_variable_tables();
        let ms = tiny_class_map(&[("java/lang/String", "s/Str"), ("java/util/List", "s/Lst")]);
        let mut raw = RawClassFile::parse(&bytes).unwrap();
        let (lvt, lvtt) = variable_texts(&raw);
        assert_eq!(lvt, ["Ljava/lang/String;"]);
        assert_eq!(lvtt, ["Ljava/util/List<Ljava/lang/String;>;"]);
        let code_len_before = raw.methods[0].attributes[0].payload.len();

        let mut stats = RemapStats::default();
        remap_code_tables(&mut raw, &ms, 0, 1, &mut stats).unwrap();
        assert_eq!(stats.code_vars_rewritten, 2, "one LVT + one LVTT entry");
        let (lvt, lvtt) = variable_texts(&raw);
        assert_eq!(lvt, ["Ls/Str;"]);
        assert_eq!(lvtt, ["Ls/Lst<Ls/Str;>;"], "both mapped names land inside");
        assert_eq!(
            raw.methods[0].attributes[0].payload.len(),
            code_len_before,
            "the Code payload length must not move"
        );
        let out = raw.serialize();
        assert_eq!(RawClassFile::parse(&out).unwrap().serialize(), out);
    }

    /// Law C2: the variable tables' NEIGHBORS ride byte-identical — the
    /// exception table and the Code header stay exactly as they were
    /// while only the variable-text indexes moved.
    #[test]
    fn exception_table_and_header_ride_identical() {
        let bytes = synthetic_with_variable_tables();
        let ms = tiny_class_map(&[("java/lang/String", "s/Str")]);
        let before = RawClassFile::parse(&bytes).unwrap();
        let mut raw = RawClassFile::parse(&bytes).unwrap();
        let mut stats = RemapStats::default();
        remap_code_tables(&mut raw, &ms, 0, 1, &mut stats).unwrap();

        let grab = |r: &RawClassFile| -> Vec<u8> {
            let body = &r.methods[0].attributes[0].payload;
            let code_len = u32::from_be_bytes([body[4], body[5], body[6], body[7]]) as usize;
            let q = 8 + code_len;
            let exc = u16::from_be_bytes([body[q], body[q + 1]]) as usize;
            let mut out = body[..8].to_vec();
            out.extend_from_slice(&body[q..q + 2 + exc * 8]);
            out
        };
        assert_eq!(
            grab(&before),
            grab(&raw),
            "header + exception table identical"
        );
    }

    /// Law C3: an empty map is a byte-exact identity through the layer.
    #[test]
    fn empty_map_is_a_byte_exact_identity() {
        let bytes = synthetic_with_variable_tables();
        let ms = tiny_class_map(&[]);
        let mut raw = RawClassFile::parse(&bytes).unwrap();
        let mut stats = RemapStats::default();
        remap_code_tables(&mut raw, &ms, 0, 1, &mut stats).unwrap();
        assert_eq!(raw.serialize(), bytes);
        assert_eq!(stats.code_vars_rewritten, 0);
        assert_eq!(stats.utf8_appended, 0);
    }

    /// Law C4: the variable tables need NO owner lookups — a map that
    /// names NO member still remaps the DESCRIPTOR texts (they are pure
    /// type text; the reference remappers rewrite local-variable
    /// descriptors on class renames regardless of member rows).
    #[test]
    fn descriptor_texts_remap_without_member_rows() {
        let bytes = synthetic_with_variable_tables();
        let ms = tiny_class_map(&[("java/lang/String", "s/Str")]);
        let mut raw = RawClassFile::parse(&bytes).unwrap();
        let mut stats = RemapStats::default();
        remap_code_tables(&mut raw, &ms, 0, 1, &mut stats).unwrap();
        let (lvt, lvtt) = variable_texts(&raw);
        assert_eq!(lvt, ["Ls/Str;"]);
        assert_eq!(lvtt, ["Ljava/util/List<Ls/Str;>;"]);
    }

    /// Law C5 (the probe fact pinned): NO committed vector carries a
    /// LocalVariableTable or a LocalVariableTypeTable — if javac ever
    /// regenerates the vectors with -g:vars, this law alerts and the
    /// vector coverage can extend to the real tables.
    #[test]
    fn no_committed_vector_carries_variable_tables() {
        for (i, bytes) in ALL.iter().enumerate() {
            let raw = RawClassFile::parse(bytes)
                .unwrap_or_else(|e| panic!("vector {i}: parse failed: {}", e.message));
            for m in &raw.methods {
                for a in &m.attributes {
                    if raw.pool.utf8(a.name_index).unwrap() != "Code" {
                        continue;
                    }
                    let body = &a.payload;
                    let code_len =
                        u32::from_be_bytes([body[4], body[5], body[6], body[7]]) as usize;
                    let mut q = 8 + code_len;
                    let exc = u16::from_be_bytes([body[q], body[q + 1]]) as usize;
                    q += 2 + exc * 8;
                    let ac = u16::from_be_bytes([body[q], body[q + 1]]) as usize;
                    q += 2;
                    for _ in 0..ac {
                        let ni = u16::from_be_bytes([body[q], body[q + 1]]);
                        let name = raw.pool.utf8(ni).unwrap();
                        assert!(
                            name != "LocalVariableTable" && name != "LocalVariableTypeTable",
                            "vector {i} unexpectedly carries {name} — extend the vector laws"
                        );
                        let al = u32::from_be_bytes([
                            body[q + 2],
                            body[q + 3],
                            body[q + 4],
                            body[q + 5],
                        ]) as usize;
                        q += 6 + al;
                    }
                    assert_eq!(q, body.len(), "vector {i}: Code payload walk must close");
                }
            }
        }
    }

    /// Law C6: the full entry remap_class includes this layer — a
    /// synthetic remapped THROUGH remap_class shows the variable texts
    /// moved (the layering is wired, not just available).
    #[test]
    fn remap_class_includes_the_code_layer() {
        let bytes = synthetic_with_variable_tables();
        // a member row (k->kk) AND the String class row: the variable
        // texts remap on CLASS renames even though they are not members
        let mut set = MappingSet::default();
        set.namespaces = vec!["src".to_string(), "dst".to_string()];
        set.classes = vec![
            ClassEntry {
                names: vec![Some("demo/Syn".to_string()), Some("mapped/Syn".to_string())],
                ..Default::default()
            },
            ClassEntry {
                names: vec![
                    Some("java/lang/String".to_string()),
                    Some("s/Str".to_string()),
                ],
                ..Default::default()
            },
        ];
        set.classes[0].fields.push(MemberEntry {
            desc: "I".to_string(),
            names: vec![Some("k".to_string()), Some("kk".to_string())],
            params: Vec::new(),
        });
        let ms = set.index();

        let mut raw = RawClassFile::parse(&bytes).unwrap();
        let stats = crate::remap_attrs::remap_class(&mut raw, &ms, 0, 1).unwrap();
        assert_eq!(stats.code_vars_rewritten, 2, "both variable entries moved");
        let (lvt, lvtt) = variable_texts(&raw);
        assert_eq!(lvt, ["Ls/Str;"]);
        assert_eq!(lvtt, ["Ljava/util/List<Ls/Str;>;"]);
        let out = raw.serialize();
        assert_eq!(
            crate::ClassFile::parse(&out).unwrap().this_class,
            "mapped/Syn"
        );
    }
}
