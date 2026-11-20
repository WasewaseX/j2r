//! T15.3 — the CLI `remap` stage: load a Tiny v2 map + a jar, remap
//! every .class entry through `remap_attrs::remap_class`, write the
//! remapped jar through the stored-only zip writer. T16 then loads the
//! REAL Fabric intermediary map for the remapped-jar differential.
//!
//! The pipeline (all loud at the seam — a malformed map or a class
//! entry that fails to parse never reaches the output):
//!
//! 1. `j2r_mappings::parser::parse` the map text (namespaces\[0\] =
//!    from, namespaces\[1\] = to; fewer than two namespaces is an error).
//! 2. `zip::Jar::parse` the input jar (stored AND deflated entries
//!    both readable).
//! 3. Walk the entries IN ORDER: `.class` entries are read, parsed as
//!    raw class files, remapped and re-serialized; every other entry
//!    (manifests, directories, resources) rides byte-identical.
//! 4. `zip_writer::write_stored` emits the output — deterministic
//!    bytes, so an EMPTY map is a byte-exact identity through the
//!    whole pipeline when the input was also stored (law E1).
//!
//! The command is a thin wrapper: this module is the library seam the
//! laws test directly.

use j2r_classfile::remap::RawClassFile;
use j2r_classfile::remap_attrs::remap_class;
use j2r_classfile::zip::Jar;
use j2r_classfile::zip_writer::{write_stored, ZipOut};
use j2r_mappings::MappingSet;

/// What one `remap_jar` run did (honest measured numbers only).
#[derive(Debug, PartialEq, Eq)]
pub struct RemapJarStats {
    /// Total entries carried through (classes + non-classes).
    pub entries_total: usize,
    /// Entries that were `.class` files.
    pub class_entries: usize,
    /// Class entries whose remapped bytes differ from their input
    /// bytes (an empty map leaves this at zero).
    pub classes_remapped: usize,
}

/// Remaps every class entry of `jar_bytes` through `map_text`.
/// Errors are one flat message string — the CLI seam prints them and
/// exits nonzero; the laws pin the loudness.
pub fn remap_jar(map_text: &str, jar_bytes: &[u8]) -> Result<(Vec<u8>, RemapJarStats), String> {
    let ms: MappingSet =
        j2r_mappings::parser::parse(map_text).map_err(|e| format!("map parse failed: {e}"))?;
    if ms.namespaces.len() < 2 {
        return Err(format!(
            "the map carries {} namespace(s); the remap stage needs a from- and a to-namespace",
            ms.namespaces.len()
        ));
    }
    let (from_ns, to_ns) = (0usize, 1usize);
    let jar = Jar::parse(jar_bytes).map_err(|e| format!("input jar parse failed: {e}"))?;

    let mut out_entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut stats = RemapJarStats {
        entries_total: 0,
        class_entries: 0,
        classes_remapped: 0,
    };
    for name in jar.entry_names() {
        let data = jar.read(name).map_err(|e| format!("entry {name}: {e}"))?;
        stats.entries_total += 1;
        if name.ends_with(".class") {
            stats.class_entries += 1;
            let mut raw = RawClassFile::parse(&data)
                .map_err(|e| format!("entry {name}: not a parseable class file: {}", e.message))?;
            remap_class(&mut raw, &ms, from_ns, to_ns)
                .map_err(|e| format!("entry {name}: remap failed: {}", e.message))?;
            let out = raw.serialize();
            if out != data {
                stats.classes_remapped += 1;
            }
            out_entries.push((name.to_string(), out));
        } else {
            out_entries.push((name.to_string(), data));
        }
    }

    let outs: Vec<ZipOut<'_>> = out_entries
        .iter()
        .map(|(n, d)| ZipOut { name: n, data: d })
        .collect();
    let bytes = write_stored(&outs).map_err(|e| format!("output jar build failed: {e}"))?;
    Ok((bytes, stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use j2r_classfile::ClassFile;

    const VEC2: &[u8] = include_bytes!("../../j2r-classfile/tests/vectors/remap/RemapVec2.class");

    /// A stored jar carrying one committed vector + a manifest, built
    /// by the writer itself (so law E1's identity is exact).
    fn vector_jar() -> Vec<u8> {
        write_stored(&[
            ZipOut {
                name: "META-INF/MANIFEST.MF",
                data: b"Manifest-Version: 1.0\n",
            },
            ZipOut {
                name: "RemapVec2.class",
                data: VEC2,
            },
        ])
        .unwrap()
    }

    /// Law E1: an EMPTY map is a byte-exact identity through the whole
    /// pipeline — parse, remap (no-op), serialize, re-write — because
    /// the writer is deterministic and the class bytes never move.
    #[test]
    fn an_empty_map_is_a_byte_exact_identity() {
        let jar = vector_jar();
        let tiny = "tiny\t2\t0\tsrc\tdst\n";
        let (out, stats) = remap_jar(tiny, &jar).unwrap();
        assert_eq!(out, jar, "empty map: the whole pipeline is identity");
        assert_eq!(
            stats,
            RemapJarStats {
                entries_total: 2,
                class_entries: 1,
                classes_remapped: 0
            }
        );
    }

    /// Law E2: under a real rename, non-class entries ride
    /// byte-identical and the entry ORDER is preserved.
    #[test]
    fn non_class_entries_ride_and_order_is_preserved() {
        let jar = vector_jar();
        let tiny = "tiny\t2\t0\tsrc\tdst\n\
                    c\tRemapVec2\tmapped/RemapVec2\n";
        let (out, stats) = remap_jar(tiny, &jar).unwrap();
        assert_eq!(stats.classes_remapped, 1);
        let out_jar = Jar::parse(&out).unwrap();
        let names: Vec<&str> = out_jar.entry_names().collect();
        assert_eq!(
            names,
            ["META-INF/MANIFEST.MF", "RemapVec2.class"],
            "order preserved"
        );
        assert_eq!(
            out_jar.read("META-INF/MANIFEST.MF").unwrap(),
            b"Manifest-Version: 1.0\n",
            "manifest byte-identical"
        );
    }

    /// Law E3: the remapped class entry reads back with the MAPPED
    /// this_class through the resolver — the jar-level pipeline really
    /// drives the class-level remap, not just copies bytes.
    #[test]
    fn the_class_entry_remaps_end_to_end() {
        let jar = vector_jar();
        let tiny = "tiny\t2\t0\tsrc\tdst\n\
                    c\tRemapVec2\tmapped/RemapVec2\n\
                    \tf\tI\tsquare\tsq\n\
                    \tm\t()I\thalf\thf\n";
        let (out, stats) = remap_jar(tiny, &jar).unwrap();
        assert_eq!(stats.classes_remapped, 1);
        let out_jar = Jar::parse(&out).unwrap();
        let remapped = out_jar.read("RemapVec2.class").unwrap();
        let cf = ClassFile::parse(&remapped).unwrap();
        assert_eq!(cf.this_class, "mapped/RemapVec2");
    }

    /// Law E4: a malformed Tiny v2 map is loud at the seam — the
    /// parser's error text rides the command's error, nothing is
    /// written.
    #[test]
    fn a_malformed_map_is_loud() {
        let jar = vector_jar();
        let bad = "tiny\t2\t0\tsrc\tdst\n\
                   x\tRemapVec2\tmapped/RemapVec2\n";
        let err = remap_jar(bad, &jar).unwrap_err();
        assert!(err.contains("map parse failed"), "{err}");
    }

    /// Law E5: a one-namespace map is loud — the parser itself refuses
    /// it ("a usable mapping file names at least two namespaces") and
    /// the error rides the command's seam; the stage's own guard is
    /// defense-in-depth behind it.
    #[test]
    fn a_one_namespace_map_is_loud() {
        let jar = vector_jar();
        let one = "tiny\t2\t0\tonly\n";
        let err = remap_jar(one, &jar).unwrap_err();
        assert!(
            err.contains("at least two namespaces")
                || err.contains("needs a from- and a to-namespace"),
            "{err}"
        );
    }

    /// Law E6: a .class entry that is NOT a parseable class file is
    /// loud, WITH the entry name in the message — a corrupted class
    /// never silently rides through.
    #[test]
    fn a_corrupt_class_entry_is_loud_with_its_name() {
        let jar = write_stored(&[ZipOut {
            name: "broken.class",
            data: b"not a class file",
        }])
        .unwrap();
        let tiny = "tiny\t2\t0\tsrc\tdst\n";
        let err = remap_jar(tiny, &jar).unwrap_err();
        assert!(err.contains("broken.class"), "{err}");
        assert!(err.contains("not a parseable class file"), "{err}");
    }
}
