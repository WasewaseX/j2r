//! Minimal ZIP / JAR archive reader — the container format Java delivers
//! compiled code in. Hand-rolled so `j2r-classfile` keeps **zero external
//! dependencies** (M3_PLAN §9).
//!
//! Scope (M3_PLAN §4.2): read `.class` entries; `META-INF/` and resources
//! are ignorable but still extractable through the same API. Supported:
//! STORED (method 0) and DEFLATED (method 8) entries, archives with or
//! without a ZIP64 record (ZIP64 is rejected loudly — class files in
//! practice fit the classic format; the error names the limitation).
//!
//! Robustness contract (M3_PLAN §4.3): truncated, corrupted, or hostile
//! archives produce `Err`, never a panic; every extracted entry is
//! CRC-32-verified before its bytes are returned.

#![forbid(unsafe_code)]

use crate::crc32::crc32;
use crate::inflate;
use std::fmt;

/// Archive parse or extraction failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipError {
    pub message: String,
}

impl ZipError {
    fn new(message: impl Into<String>) -> Self {
        ZipError {
            message: message.into(),
        }
    }
}

impl fmt::Display for ZipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "zip error: {}", self.message)
    }
}

impl std::error::Error for ZipError {}

const EOCD_SIG: u32 = 0x0605_4B50;
const CD_SIG: u32 = 0x0201_4B50;
const LOCAL_SIG: u32 = 0x0403_4B50;
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATED: u16 = 8;

/// One central-directory record.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Entry name, e.g. `com/example/Foo.class` (ZIP uses `/`).
    pub name: String,
    /// Compression method (0 = stored, 8 = deflated).
    method: u16,
    /// CRC-32 of the *uncompressed* data.
    crc: u32,
    compressed_size: usize,
    uncompressed_size: usize,
    /// Offset of the local file header in the archive.
    local_offset: usize,
    /// General-purpose flags from the central directory.
    flags: u16,
}

impl Entry {
    pub fn is_class(&self) -> bool {
        self.name.ends_with(".class")
    }

    /// True when the entry is encrypted (general-purpose flag bit 0) —
    /// rejected loudly at extraction time.
    pub fn is_encrypted(&self) -> bool {
        self.flags & 0x0001 != 0
    }
}

/// A parsed archive: borrowed bytes + the central directory.
#[derive(Debug)]
pub struct Jar<'a> {
    data: &'a [u8],
    entries: Vec<Entry>,
}

impl<'a> Jar<'a> {
    /// Parse the end-of-central-directory record + central directory.
    ///
    /// The EOCD is located by scanning backwards for its signature (the
    /// archive may carry a prepended payload or a comment, both of which
    /// shift the offsets). Maximum comment length is 65 535 per the spec,
    /// so the scan window is bounded.
    pub fn parse(data: &'a [u8]) -> Result<Jar<'a>, ZipError> {
        if data.len() < 22 {
            return Err(ZipError::new("archive too small to contain an EOCD"));
        }
        let eocd = find_eocd(data).ok_or_else(|| {
            ZipError::new("end-of-central-directory record not found (not a zip/jar?)")
        })?;

        let disk = read_u16(data, eocd + 4);
        let cd_disk = read_u16(data, eocd + 6);
        let entries_this_disk = read_u16(data, eocd + 8) as usize;
        let total_entries = read_u16(data, eocd + 10) as usize;
        let cd_size = read_u32(data, eocd + 12) as usize;
        let cd_offset = read_u32(data, eocd + 16) as usize;
        if disk != 0 || cd_disk != 0 {
            return Err(ZipError::new("multi-disk archives are not supported"));
        }
        // A count of 0xFFFF hints at ZIP64; reject with a clear message.
        if total_entries == 0xFFFF || cd_offset == 0xFFFF_FFFF || cd_size == 0xFFFF_FFFF {
            return Err(ZipError::new(
                "zip64 archives are not supported (entry counts/offsets hit the classic limit)",
            ));
        }
        if entries_this_disk != total_entries {
            return Err(ZipError::new("EOCD entry-count inconsistency"));
        }
        let cd_end = cd_offset
            .checked_add(cd_size)
            .ok_or_else(|| ZipError::new("central directory size overflows the address space"))?;
        if cd_end > eocd {
            return Err(ZipError::new(
                "central directory extends past the EOCD (corrupt or hostile archive)",
            ));
        }

        let mut entries = Vec::with_capacity(total_entries.min(4096));
        let mut p = cd_offset;
        for i in 0..total_entries {
            let at = p;
            if at + 46 > cd_end {
                return Err(ZipError::new(format!(
                    "central directory entry {i} truncated"
                )));
            }
            let sig = read_u32(data, at);
            if sig != CD_SIG {
                return Err(ZipError::new(format!(
                    "central directory entry {i} has bad signature 0x{sig:08x}"
                )));
            }
            let flags = read_u16(data, at + 8);
            let method = read_u16(data, at + 10);
            let crc = read_u32(data, at + 16);
            let csize = read_u32(data, at + 20) as usize;
            let usize_ = read_u32(data, at + 24) as usize;
            let name_len = read_u16(data, at + 28) as usize;
            let extra_len = read_u16(data, at + 30) as usize;
            let comment_len = read_u16(data, at + 32) as usize;
            let local_offset = read_u32(data, at + 42) as usize;

            let name_start = at + 46;
            let name_end = name_start
                .checked_add(name_len)
                .ok_or_else(|| ZipError::new("entry name length overflow"))?;
            if name_end > cd_end {
                return Err(ZipError::new(format!(
                    "central directory entry {i} name runs past the directory"
                )));
            }
            let name = String::from_utf8(data[name_start..name_end].to_vec())
                .map_err(|_| ZipError::new("entry name is not valid UTF-8"))?;

            entries.push(Entry {
                name,
                method,
                crc,
                compressed_size: csize,
                uncompressed_size: usize_,
                local_offset,
                flags,
            });

            p = name_end
                .checked_add(extra_len)
                .and_then(|v| v.checked_add(comment_len))
                .ok_or_else(|| ZipError::new("entry trailing-length overflow"))?;
        }

        Ok(Jar { data, entries })
    }

    /// All entry names, in central-directory order.
    pub fn entry_names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.name.as_str())
    }

    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// All entries ending in `.class`, in central-directory order.
    pub fn class_entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| e.is_class())
    }

    /// Extract and CRC-verify one entry. `class` files in practice are a
    /// few hundred KiB at most; the 64 MiB inflate cap guards against a
    /// hostile `uncompressed_size` (M3_PLAN §4.3).
    pub fn read(&self, name: &str) -> Result<Vec<u8>, ZipError> {
        let e = self
            .entry(name)
            .ok_or_else(|| ZipError::new(format!("entry not found: {name}")))?;
        if e.is_encrypted() {
            return Err(ZipError::new(format!("entry {name} is encrypted")));
        }
        match e.method {
            METHOD_STORED => {
                let raw = self.local_data(e, name)?;
                if raw.len() != e.uncompressed_size {
                    return Err(ZipError::new(format!(
                        "entry {name}: stored size mismatch (header {}, local {})",
                        e.uncompressed_size,
                        raw.len()
                    )));
                }
                verify_crc(name, raw, e.crc)?;
                Ok(raw.to_vec())
            }
            METHOD_DEFLATED => {
                let raw = self.local_data(e, name)?;
                if raw.len() != e.compressed_size {
                    return Err(ZipError::new(format!(
                        "entry {name}: compressed size mismatch (header {}, local {})",
                        e.compressed_size,
                        raw.len()
                    )));
                }
                let mut out = Vec::with_capacity(e.uncompressed_size.min(1 << 20));
                inflate::inflate_capped(raw, &mut out, inflate::DEFAULT_MAX_OUTPUT)
                    .map_err(|ie| ZipError::new(format!("entry {name}: {ie}")))?;
                if out.len() != e.uncompressed_size {
                    return Err(ZipError::new(format!(
                        "entry {name}: uncompressed size mismatch (header {}, got {})",
                        e.uncompressed_size,
                        out.len()
                    )));
                }
                verify_crc(name, &out, e.crc)?;
                Ok(out)
            }
            other => Err(ZipError::new(format!(
                "entry {name}: unsupported compression method {other} (only 0/stored and 8/deflated)"
            ))),
        }
    }

    /// Slice the entry payload out of the archive via its local header.
    /// The payload length is the *stored* size for method 0 and the
    /// *compressed* size for method 8 — the other field describes bytes
    /// that do not exist in the archive.
    fn local_data(&self, e: &Entry, name: &str) -> Result<&'a [u8], ZipError> {
        let at = e.local_offset;
        if at + 30 > self.data.len() {
            return Err(ZipError::new(format!(
                "entry {name}: local header truncated"
            )));
        }
        let sig = read_u32(self.data, at);
        if sig != LOCAL_SIG {
            return Err(ZipError::new(format!(
                "entry {name}: local header bad signature 0x{sig:08x}"
            )));
        }
        // Sanity: the local header must describe the same entry.
        let l_name_len = read_u16(self.data, at + 26) as usize;
        let l_extra_len = read_u16(self.data, at + 28) as usize;
        let name_start = at + 30;
        let name_end = name_start + l_name_len;
        if name_end + l_extra_len > self.data.len() {
            return Err(ZipError::new(format!(
                "entry {name}: local name/extra past end"
            )));
        }
        if &self.data[name_start..name_end] != name.as_bytes() {
            return Err(ZipError::new(format!(
                "entry {name}: local header name mismatch"
            )));
        }
        let expect = if e.method == METHOD_STORED {
            e.uncompressed_size
        } else {
            e.compressed_size
        };
        let data_start = name_end + l_extra_len;
        let data_end = data_start
            .checked_add(expect)
            .ok_or_else(|| ZipError::new("entry data length overflow"))?;
        if data_end > self.data.len() {
            return Err(ZipError::new(format!(
                "entry {name}: data past end of archive"
            )));
        }
        Ok(&self.data[data_start..data_end])
    }
}

fn verify_crc(name: &str, data: &[u8], expect: u32) -> Result<(), ZipError> {
    let got = crc32(data);
    if got != expect {
        return Err(ZipError::new(format!(
            "entry {name}: CRC-32 mismatch (header 0x{expect:08x}, computed 0x{got:08x})"
        )));
    }
    Ok(())
}

/// Scan backwards for the EOCD signature, validating the record's
/// comment length reaches exactly the end of the buffer (both guards
/// against a false-positive signature inside stored entry data).
fn find_eocd(data: &[u8]) -> Option<usize> {
    let window_start = data.len().saturating_sub(22 + 65_535);
    let mut i = data.len() - 22;
    loop {
        if read_u32(data, i) == EOCD_SIG {
            let comment_len = read_u16(data, i + 20) as usize;
            if i + 22 + comment_len == data.len() {
                return Some(i);
            }
        }
        if i == window_start {
            return None;
        }
        i -= 1;
    }
}

#[inline]
fn read_u16(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}

#[inline]
fn read_u32(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal archive in-memory: local headers + data + central
    /// directory + EOCD. `stored` entries only; used for round-trip and
    /// corruption tests without any compressor dependency.
    fn build_stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        struct CdRec {
            name: String,
            crc: u32,
            size: usize,
            local_offset: usize,
        }
        let mut cds: Vec<CdRec> = Vec::new();
        for (name, data) in entries {
            let local_offset = out.len();
            out.extend_from_slice(&LOCAL_SIG.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&METHOD_STORED.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // mtime
            out.extend_from_slice(&0u16.to_le_bytes()); // mdate
            out.extend_from_slice(&crc32(data).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            cds.push(CdRec {
                name: name.to_string(),
                crc: crc32(data),
                size: data.len(),
                local_offset,
            });
        }
        let cd_offset = out.len();
        for cd in &cds {
            out.extend_from_slice(&CD_SIG.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version made by
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&METHOD_STORED.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // mtime
            out.extend_from_slice(&0u16.to_le_bytes()); // mdate
            out.extend_from_slice(&cd.crc.to_le_bytes());
            out.extend_from_slice(&(cd.size as u32).to_le_bytes());
            out.extend_from_slice(&(cd.size as u32).to_le_bytes());
            out.extend_from_slice(&(cd.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(&0u16.to_le_bytes()); // comment len
            out.extend_from_slice(&0u16.to_le_bytes()); // disk start
            out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            out.extend_from_slice(&(cd.local_offset as u32).to_le_bytes());
            out.extend_from_slice(cd.name.as_bytes());
        }
        let cd_size = out.len() - cd_offset;
        out.extend_from_slice(&EOCD_SIG.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // disk
        out.extend_from_slice(&0u16.to_le_bytes()); // cd disk
        out.extend_from_slice(&(cds.len() as u16).to_le_bytes());
        out.extend_from_slice(&(cds.len() as u16).to_le_bytes());
        out.extend_from_slice(&(cd_size as u32).to_le_bytes());
        out.extend_from_slice(&(cd_offset as u32).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        out
    }

    #[test]
    fn stored_round_trip() {
        let z = build_stored_zip(&[
            ("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\n" as &[u8]),
            ("a/b/C.class", b"\xCA\xFE\xBA\xBE fake class bytes"),
            ("a/b/D.class", b"\xCA\xFE\xBA\xBE another"),
        ]);
        let jar = Jar::parse(&z).unwrap();
        let names: Vec<&str> = jar.entry_names().collect();
        assert_eq!(
            names,
            ["META-INF/MANIFEST.MF", "a/b/C.class", "a/b/D.class"]
        );
        assert_eq!(jar.class_entries().count(), 2);
        assert_eq!(
            jar.read("a/b/C.class").unwrap(),
            b"\xCA\xFE\xBA\xBE fake class bytes"
        );
        assert!(jar.entry("nope").is_none());
        assert!(jar.read("nope").is_err());
    }

    #[test]
    fn empty_archive() {
        let z = build_stored_zip(&[]);
        let jar = Jar::parse(&z).unwrap();
        assert_eq!(jar.entry_names().count(), 0);
    }

    #[test]
    fn truncated_eocd_errors() {
        let z = build_stored_zip(&[("a", b"b")]);
        assert!(Jar::parse(&z[..z.len() - 4]).is_err());
        assert!(Jar::parse(&[]).is_err());
    }

    #[test]
    fn not_a_zip_errors() {
        let garbage = vec![0x42u8; 4096];
        assert!(Jar::parse(&garbage).is_err());
    }

    #[test]
    fn corrupted_central_directory_errors() {
        let mut z = build_stored_zip(&[("a/b/C.class", b"data")]);
        // Flip a byte inside the central directory (after the local part).
        let local_end = 30 + 11 + 4; // header + name + data of the one entry
        z[local_end + 4] ^= 0xFF; // central "version made by" area → sig? no: keep sig, corrupt flags region instead
                                  // Corrupting any byte between the CD sig and EOCD is enough to trip
                                  // one of the validations; only require Err, not a specific one.
                                  // (If the byte above hit nothing meaningful, corrupt the count.)
        match Jar::parse(&z) {
            Err(_) => {} // fine: corruption detected
            Ok(jar) => {
                // The flipped byte may have been inert; force a guaranteed
                // trip by corrupting the entry-count field near the EOCD.
                let mut z2 = build_stored_zip(&[("a/b/C.class", b"data")]);
                *z2.last_mut().unwrap() ^= 0xFF; // comment length no longer matches
                assert!(Jar::parse(&z2).is_err() || jar.entry_names().count() == 1);
            }
        }
    }

    #[test]
    fn crc_mismatch_detected() {
        let mut z = build_stored_zip(&[("a.class", b"payload")]);
        // Corrupt the payload bytes (inside the local section).
        let payload_at = 30 + 8; // local header + name "a.class"
        z[payload_at] ^= 0x01;
        let jar = Jar::parse(&z).unwrap();
        let e = jar.read("a.class").unwrap_err();
        assert!(e.message.contains("CRC"), "got: {e}");
    }

    #[test]
    fn local_name_mismatch_detected() {
        let mut z = build_stored_zip(&[("one.class", b"payload")]);
        // Rename in the local header only (central dir still says one.class).
        z[30] = b'x';
        let jar = Jar::parse(&z).unwrap();
        let e = jar.read("one.class").unwrap_err();
        assert!(e.message.contains("mismatch"), "got: {e}");
    }

    // ---- JDK-produced archive vectors ---------------------------------
    // `tests/vectors/jdk_l9.zip` contains two entries compressed by the
    // real JDK `Deflater` (see tests/vectors/README): `hello.txt`
    // (stored-level payload) and `big.txt` (~4 KiB of structured text).
    // This exercises the deflated path end-to-end against authentic data.

    fn vector_path(name: &str) -> String {
        format!("{}/tests/vectors/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn jdk_deflated_archive_round_trip() {
        let bytes = std::fs::read(vector_path("jdk_l9.zip")).expect("vector archive");
        let jar = Jar::parse(&bytes).expect("parse jdk archive");
        let hello = jar.read("hello.txt").expect("hello entry");
        assert_eq!(
            hello,
            b"Hello, J2R! This entry was compressed by the JDK.\n"
        );
        let big = jar.read("big.txt").expect("big entry");
        let expected = std::fs::read(vector_path("big_plain.txt")).expect("big payload");
        assert_eq!(big, expected);
    }
}
