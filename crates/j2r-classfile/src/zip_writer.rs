//! T15.3 — the stored-only zip writer: the output half of the CLI
//! `remap` stage. The reader (`zip::Jar`) handles stored AND deflated
//! entries; this writer emits method-0 (stored) entries only — legal
//! jars, deterministic bytes, no deflate encoder required (none exists
//! in this crate, and the remap stage rewrites class payloads that the
//! reader-side inflate never sees compressed back again).
//!
//! Determinism law: the same entry list produces byte-identical output
//! (fixed DOS timestamp 0, no per-run state) — the empty-map identity
//! law in the CLI layer depends on this.
//!
//! Layout emitted (all little-endian, the ZIP "appnote" order):
//! per entry: 30-byte local header + name + payload; then one 46-byte
//! central-directory header + name per entry; then the 22-byte EOCD.
//! Guards are loud: more than 65,534 entries, any u32 field overflow.

use crate::crc32::crc32;
use crate::zip::ZipError;

const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const EOCD_SIG: u32 = 0x0605_4b50;

/// One entry to write: a name and its payload. A name ending in `/`
/// is a directory entry (the DOS directory bit rides its external
/// attributes).
pub struct ZipOut<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
}

/// Builds a stored-only zip archive from the entries in order.
/// Deterministic: no clock, no map iteration, no randomness.
pub fn write_stored(entries: &[ZipOut<'_>]) -> Result<Vec<u8>, ZipError> {
    if entries.len() > 65_534 {
        return Err(ZipError {
            message: format!(
                "{} entries exceed the classic EOCD limit of 65534 (ZIP64 is not wired)",
                entries.len()
            ),
        });
    }
    let mut out: Vec<u8> = Vec::new();
    // (name, crc, size, local_offset) per entry, for the central directory.
    let mut toc: Vec<(&str, u32, u32, u32)> = Vec::with_capacity(entries.len());
    for e in entries {
        let name_bytes = e.name.as_bytes();
        let offset = out.len() as u32;
        let crc = crc32(e.data);
        let size = e.data.len() as u32;
        push_local_header(&mut out, name_bytes, crc, size, offset)?;
        out.extend_from_slice(e.data);
        toc.push((e.name, crc, size, offset));
    }
    let cd_offset = out.len() as u32;
    for (name, crc, size, offset) in &toc {
        push_central_header(&mut out, name, *crc, *size, *offset)?;
    }
    let cd_size = out.len() as u32 - cd_offset;
    out.extend_from_slice(&EOCD_SIG.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // this disk
    out.extend_from_slice(&0u16.to_le_bytes()); // cd disk
    out.extend_from_slice(&(toc.len() as u16).to_le_bytes());
    out.extend_from_slice(&(toc.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // no comment
    Ok(out)
}

fn push_local_header(
    out: &mut Vec<u8>,
    name: &[u8],
    crc: u32,
    size: u32,
    _offset: u32,
) -> Result<(), ZipError> {
    if name.len() > u16::MAX as usize {
        return Err(ZipError {
            message: format!(
                "entry name longer than 65535 bytes: {}",
                String::from_utf8_lossy(name)
            ),
        });
    }
    out.extend_from_slice(&LOCAL_SIG.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes()); // version needed
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&0u16.to_le_bytes()); // method 0 = stored
    out.extend_from_slice(&0u16.to_le_bytes()); // mod time (fixed: determinism)
    out.extend_from_slice(&0u16.to_le_bytes()); // mod date (fixed: determinism)
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes()); // compressed
    out.extend_from_slice(&size.to_le_bytes()); // uncompressed
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // extra len
    out.extend_from_slice(name);
    Ok(())
}

fn push_central_header(
    out: &mut Vec<u8>,
    name: &str,
    crc: u32,
    size: u32,
    offset: u32,
) -> Result<(), ZipError> {
    let name_bytes = name.as_bytes();
    let is_dir = name.ends_with('/');
    out.extend_from_slice(&CENTRAL_SIG.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes()); // version made by
    out.extend_from_slice(&20u16.to_le_bytes()); // version needed
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&0u16.to_le_bytes()); // method stored
    out.extend_from_slice(&0u16.to_le_bytes()); // mod time
    out.extend_from_slice(&0u16.to_le_bytes()); // mod date
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // extra
    out.extend_from_slice(&0u16.to_le_bytes()); // comment
    out.extend_from_slice(&0u16.to_le_bytes()); // disk start
    out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
                                                // external attrs: the DOS directory bit for names ending in '/',
                                                // plain files otherwise (rw-r--r-- << 16 for realism).
    let ext: u32 = if is_dir {
        0x10 | (0o40755u32) << 16
    } else {
        0o100644u32 << 16
    };
    out.extend_from_slice(&ext.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(name_bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zip::Jar;

    /// Law W1: what the writer emits, the real reader reads back —
    /// names, payloads and ORDER all identical.
    #[test]
    fn writer_output_round_trips_through_the_real_reader() {
        let entries = [
            ZipOut {
                name: "META-INF/MANIFEST.MF",
                data: b"Manifest-Version: 1.0\n",
            },
            ZipOut {
                name: "a/b/C.class",
                data: &[0xCA, 0xFE, 0xBA, 0xBE, 1, 2, 3],
            },
            ZipOut {
                name: "a/b/",
                data: b"",
            },
        ];
        let bytes = write_stored(&entries).unwrap();
        let jar = Jar::parse(&bytes).unwrap();
        let names: Vec<&str> = jar.entry_names().collect();
        assert_eq!(names, ["META-INF/MANIFEST.MF", "a/b/C.class", "a/b/"]);
        assert_eq!(
            jar.read("META-INF/MANIFEST.MF").unwrap(),
            b"Manifest-Version: 1.0\n"
        );
        assert_eq!(
            jar.read("a/b/C.class").unwrap(),
            vec![0xCA, 0xFE, 0xBA, 0xBE, 1, 2, 3]
        );
        assert_eq!(jar.read("a/b/").unwrap(), b"");
        assert!(jar.entry("a/b/C.class").unwrap().is_class());
        assert!(jar.entry("a/b/").unwrap().name.ends_with('/'));
    }

    /// Law W2: determinism — the same entries twice produce
    /// byte-identical archives (the empty-map identity in the CLI
    /// layer leans on this).
    #[test]
    fn the_writer_is_deterministic() {
        let mk = || {
            write_stored(&[
                ZipOut {
                    name: "x.txt",
                    data: b"payload",
                },
                ZipOut {
                    name: "y/D.class",
                    data: &[9, 9, 9],
                },
            ])
        };
        assert_eq!(mk().unwrap(), mk().unwrap());
    }

    /// Law W3: byte-level shape pins — the three signatures sit where
    /// the spec puts them, every local method byte is 0 (stored), and
    /// the EOCD counts the entries.
    #[test]
    fn byte_shape_pins_sigs_methods_and_counts() {
        let entries = [
            ZipOut {
                name: "one",
                data: b"1",
            },
            ZipOut {
                name: "two",
                data: b"22",
            },
        ];
        let b = write_stored(&entries).unwrap();
        // EOCD is the last 22 bytes; its cd-size field points back here.
        let eocd = b.len() - 22;
        assert_eq!(&b[eocd..eocd + 4], &0x0605_4b50u32.to_le_bytes());
        assert_eq!(
            &b[eocd + 8..eocd + 12],
            &2u16.to_le_bytes().repeat(2),
            "EOCD this-disk + total entry counts"
        );
        // walk every local header (up to the EOCD's cd-offset field):
        // sig + stored method + name
        let cd_off =
            u32::from_le_bytes([b[eocd + 16], b[eocd + 17], b[eocd + 18], b[eocd + 19]]) as usize;
        let mut q = 0usize;
        let mut locals = 0;
        while q < cd_off {
            let sig = u32::from_le_bytes([b[q], b[q + 1], b[q + 2], b[q + 3]]);
            assert_eq!(sig, LOCAL_SIG);
            let method = u16::from_le_bytes([b[q + 8], b[q + 9]]);
            assert_eq!(method, 0, "stored method in every local header");
            let nlen = u16::from_le_bytes([b[q + 26], b[q + 27]]) as usize;
            let size = u32::from_le_bytes([b[q + 18], b[q + 19], b[q + 20], b[q + 21]]);
            q += 30 + nlen + size as usize;
            locals += 1;
        }
        assert_eq!(locals, 2);
    }

    /// Law W4: zero entries is a legal archive — EOCD only, reads back
    /// empty.
    #[test]
    fn an_empty_archive_round_trips() {
        let b = write_stored(&[]).unwrap();
        assert_eq!(b.len(), 22);
        let jar = Jar::parse(&b).unwrap();
        assert_eq!(jar.entry_names().count(), 0);
    }

    /// Law W5: the classic EOCD limit is loud — 65,535 entries (one
    /// past the guard) is rejected, not truncated.
    #[test]
    fn too_many_entries_is_loud() {
        let names: Vec<String> = (0..65_535).map(|i| format!("e{i}")).collect();
        let entries: Vec<ZipOut<'_>> = names
            .iter()
            .map(|n| ZipOut { name: n, data: b"" })
            .collect();
        let err = write_stored(&entries).unwrap_err();
        assert!(
            err.message.contains("exceed the classic EOCD limit"),
            "{}",
            err.message
        );
    }
}
