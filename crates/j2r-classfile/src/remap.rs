//! T15.1 — the raw class reader + byte-exact writer: the remap stage's
//! transport (TODO.md T15, M6 wave-3). The resolver (`ClassFile::parse`)
//! interprets attributes and DROPS what it does not need (LineNumberTable,
//! StackMapTable bodies, unknown attributes, precise attribute order), so a
//! parse→serialize round trip through the resolved model cannot be
//! byte-exact. The remap stage needs the opposite contract: read a class
//! file without interpreting anything, mutate exactly the constant-pool
//! slots that carry remapped names (plus the few non-Code attributes that
//! hold class/name references), and write every other byte back untouched —
//! javac's StackMapTable and friends stay verbatim, so the rewritten class
//! re-verifies on any JVM and feeds `j2r build` unchanged.
//!
//! Laws (the `#[cfg(test)]` suite below pins them on committed javac
//! vectors, `tests/vectors/remap/`):
//! 1. parse→serialize is byte-identical for every vector.
//! 2. the raw reader agrees with the resolver on names, descriptors and
//!    member census (they read the same bytes two ways and must never
//!    disagree).
//! 3. `retarget_this_class` rewrites ONLY the one Class entry and appends
//!    exactly one Utf8 — every method/field attribute payload (the Code
//!    bodies, the StackMapTable) stays byte-identical, which is the whole
//!    point of the raw transport.
//! 4. modified UTF-8 survives the transport bit-for-bit (NUL as `C0 80`,
//!    lone surrogates as CESU-8) — the resolver normalizes these, the raw
//!    transport must not.

use crate::Error;

const MAGIC: u32 = 0xCAFEBABE;

/// One constant-pool entry: the tag byte plus the payload AFTER the tag,
/// kept as raw bytes. Long/Double occupy two slots; the unused second slot
/// is `None` in [`RawPool::slots`] and contributes no bytes on write.
#[derive(Clone, Debug, PartialEq)]
pub struct RawCpEntry {
    pub tag: u8,
    pub data: Vec<u8>,
}

impl RawCpEntry {
    /// The raw modified-UTF-8 bytes of a Utf8 entry (tag 1).
    pub fn utf8_bytes(&self) -> Option<&[u8]> {
        if self.tag == 1 {
            Some(&self.data)
        } else {
            None
        }
    }

    /// The Utf8 content, decoded with the same rules the resolver uses.
    pub fn utf8_str(&self) -> Option<String> {
        if self.tag == 1 {
            Some(decode_modified_utf8(&self.data))
        } else {
            None
        }
    }

    /// The `name_index` of a Class entry (tag 7).
    pub fn class_name_index(&self) -> Option<u16> {
        if self.tag == 7 && self.data.len() == 2 {
            Some(u16::from_be_bytes([self.data[0], self.data[1]]))
        } else {
            None
        }
    }

    pub fn set_class_name_index(&mut self, index: u16) {
        self.data[0] = (index >> 8) as u8;
        self.data[1] = index as u8;
    }

    /// The `(name_index, descriptor_index)` of a NameAndType entry (tag 12).
    pub fn name_and_type(&self) -> Option<(u16, u16)> {
        if self.tag == 12 && self.data.len() == 4 {
            Some((
                u16::from_be_bytes([self.data[0], self.data[1]]),
                u16::from_be_bytes([self.data[2], self.data[3]]),
            ))
        } else {
            None
        }
    }

    pub fn set_name_and_type(&mut self, name: u16, descriptor: u16) {
        self.data[0] = (name >> 8) as u8;
        self.data[1] = name as u8;
        self.data[2] = (descriptor >> 8) as u8;
        self.data[3] = descriptor as u8;
    }
}

/// The constant pool: 1-based slots exactly as the class file lays them
/// out; slot 0 is always `None` (and so is a Long/Double's second slot).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RawPool {
    pub slots: Vec<Option<RawCpEntry>>,
}

impl RawPool {
    /// The Utf8 content at `idx` (resolver-equivalent error when the slot
    /// is not a Utf8 entry).
    pub fn utf8(&self, idx: u16) -> Result<String, Error> {
        let at = usize::from(idx);
        match self.slots.get(at).and_then(|s| s.as_ref()) {
            Some(e) => e.utf8_str().ok_or_else(|| Error {
                message: format!("cp slot {idx} is not a Utf8 constant (tag {})", e.tag),
                offset: 0,
            }),
            None => Err(Error {
                message: format!("cp slot {idx} is empty or out of range"),
                offset: 0,
            }),
        }
    }

    /// The binary class name at `idx`: Class entry → its Utf8 name.
    pub fn class_name(&self, idx: u16) -> Result<String, Error> {
        let at = usize::from(idx);
        let class_index = match self.slots.get(at).and_then(|s| s.as_ref()) {
            Some(e) => e.class_name_index().ok_or_else(|| Error {
                message: format!("cp slot {idx} is not a Class constant (tag {})", e.tag),
                offset: 0,
            })?,
            None => {
                return Err(Error {
                    message: format!("cp slot {idx} is empty or out of range"),
                    offset: 0,
                })
            }
        };
        self.utf8(class_index)
    }

    /// Append a Utf8 entry built from `text` (encoded as modified UTF-8)
    /// and return its index. This is how the remap stage grows the pool
    /// without disturbing any existing index.
    pub fn append_utf8(&mut self, text: &str) -> Result<u16, Error> {
        let idx = u16::try_from(self.slots.len()).map_err(|_| Error {
            message: "constant pool overflow (u16 slot count)".into(),
            offset: 0,
        })?;
        self.slots.push(Some(RawCpEntry {
            tag: 1,
            data: encode_modified_utf8(text),
        }));
        Ok(idx)
    }

    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&(self.slots.len() as u16).to_be_bytes());
        for slot in self.slots.iter().skip(1).flatten() {
            out.push(slot.tag);
            // Only Utf8 (tag 1) carries a u16 length prefix; every other
            // tag is a fixed-width payload (JVMS §4.4).
            if slot.tag == 1 {
                out.extend_from_slice(&(slot.data.len() as u16).to_be_bytes());
            }
            out.extend_from_slice(&slot.data);
        }
    }
}

/// One attribute: the `name_index` plus the RAW body bytes. The writer
/// emits exactly `u2 name, u4 len, body` — no attribute is interpreted.
#[derive(Clone, Debug, PartialEq)]
pub struct RawAttribute {
    pub name_index: u16,
    pub payload: Vec<u8>,
}

/// One field or method: the fixed header plus the raw attribute list.
#[derive(Clone, Debug, PartialEq)]
pub struct RawMember {
    pub access: u16,
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Vec<RawAttribute>,
}

/// A class file that kept everything the reader saw.
#[derive(Clone, Debug, PartialEq)]
pub struct RawClassFile {
    pub minor_version: u16,
    pub major_version: u16,
    pub pool: RawPool,
    pub access: u16,
    pub this_class: u16,
    pub super_class: u16,
    pub interfaces: Vec<u16>,
    pub fields: Vec<RawMember>,
    pub methods: Vec<RawMember>,
    pub attributes: Vec<RawAttribute>,
}

impl RawClassFile {
    /// Parse without interpreting: every attribute body stays raw bytes.
    pub fn parse(bytes: &[u8]) -> Result<RawClassFile, Error> {
        let mut r = Reader { bytes, pos: 0 };
        let magic = r.u4()?;
        if magic != MAGIC {
            return Err(Error {
                message: format!("bad magic 0x{magic:08X}, expected 0xCAFEBABE"),
                offset: 0,
            });
        }
        let minor_version = r.u2()?;
        let major_version = r.u2()?;
        let pool = parse_raw_pool(&mut r)?;
        let access = r.u2()?;
        let this_class = r.u2()?;
        let super_class = r.u2()?;
        let interface_count = r.u2()?;
        let mut interfaces = Vec::with_capacity(interface_count as usize);
        for _ in 0..interface_count {
            interfaces.push(r.u2()?);
        }
        let fields = parse_raw_members(&mut r)?;
        let methods = parse_raw_members(&mut r)?;
        let attributes = parse_raw_attributes(&mut r)?;
        if r.pos != bytes.len() {
            return Err(Error {
                message: format!(
                    "trailing bytes after the class file: {} of {} unused",
                    bytes.len() - r.pos,
                    bytes.len()
                ),
                offset: r.pos,
            });
        }
        Ok(RawClassFile {
            minor_version,
            major_version,
            pool,
            access,
            this_class,
            super_class,
            interfaces,
            fields,
            methods,
            attributes,
        })
    }

    /// The binary class name of `this_class`.
    pub fn this_class_name(&self) -> Result<String, Error> {
        self.pool.class_name(self.this_class)
    }

    /// The T15 remap primitive, shape (a): rename the class itself. Appends
    /// one Utf8 with `new_name` and retargets `this_class`'s Class entry to
    /// it — no other slot changes, no member byte moves (law 3 pins this).
    pub fn retarget_this_class(&mut self, new_name: &str) -> Result<(), Error> {
        let name_index = self.pool.append_utf8(new_name)?;
        let at = usize::from(self.this_class);
        let entry = self.pool.slots.get_mut(at).and_then(|s| s.as_mut());
        match entry {
            Some(e) if e.class_name_index().is_some() => {
                e.set_class_name_index(name_index);
                Ok(())
            }
            Some(e) => Err(Error {
                message: format!(
                    "cp slot {} is not a Class constant (tag {})",
                    self.this_class, e.tag
                ),
                offset: 0,
            }),
            None => Err(Error {
                message: format!("cp slot {} is empty or out of range", self.this_class),
                offset: 0,
            }),
        }
    }

    /// Serialize back to class-file bytes (law 1: identical to the input
    /// when nothing was mutated).
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(1024);
        out.extend_from_slice(&MAGIC.to_be_bytes());
        out.extend_from_slice(&self.minor_version.to_be_bytes());
        out.extend_from_slice(&self.major_version.to_be_bytes());
        self.pool.write(&mut out);
        out.extend_from_slice(&self.access.to_be_bytes());
        out.extend_from_slice(&self.this_class.to_be_bytes());
        out.extend_from_slice(&self.super_class.to_be_bytes());
        out.extend_from_slice(&(self.interfaces.len() as u16).to_be_bytes());
        for iface in &self.interfaces {
            out.extend_from_slice(&iface.to_be_bytes());
        }
        write_members(&mut out, &self.fields);
        write_members(&mut out, &self.methods);
        write_attributes(&mut out, &self.attributes);
        out
    }
}

fn write_members(out: &mut Vec<u8>, members: &[RawMember]) {
    out.extend_from_slice(&(members.len() as u16).to_be_bytes());
    for m in members {
        out.extend_from_slice(&m.access.to_be_bytes());
        out.extend_from_slice(&m.name_index.to_be_bytes());
        out.extend_from_slice(&m.descriptor_index.to_be_bytes());
        write_attributes(out, &m.attributes);
    }
}

fn write_attributes(out: &mut Vec<u8>, attrs: &[RawAttribute]) {
    out.extend_from_slice(&(attrs.len() as u16).to_be_bytes());
    for a in attrs {
        out.extend_from_slice(&a.name_index.to_be_bytes());
        out.extend_from_slice(&(a.payload.len() as u32).to_be_bytes());
        out.extend_from_slice(&a.payload);
    }
}

fn parse_raw_pool(r: &mut Reader<'_>) -> Result<RawPool, Error> {
    let count = r.u2()? as usize;
    let mut slots: Vec<Option<RawCpEntry>> = vec![None; count.max(1)];
    let mut i = 1usize;
    while i < count {
        let at = r.pos;
        let tag = r.u1()?;
        let (two_slots, payload_len) = match tag {
            1 => (false, r.u2()? as usize),
            3 | 4 => (false, 4),
            5 | 6 => (true, 8),
            7 | 8 | 16 | 19 | 20 => (false, 2),
            9 | 10 | 11 | 12 | 17 | 18 => (false, 4),
            15 => (false, 3),
            other => {
                return Err(Error {
                    message: format!("unknown constant pool tag {other}"),
                    offset: at,
                })
            }
        };
        if two_slots && i + 1 >= count {
            return Err(Error {
                message: if tag == 5 {
                    "Long constant ends the constant pool".into()
                } else {
                    "Double constant ends the constant pool".into()
                },
                offset: at,
            });
        }
        let data = r.bytes(payload_len)?.to_vec();
        slots[i] = Some(RawCpEntry { tag, data });
        if two_slots {
            i += 2;
        } else {
            i += 1;
        }
    }
    Ok(RawPool { slots })
}

fn parse_raw_members(r: &mut Reader<'_>) -> Result<Vec<RawMember>, Error> {
    let count = r.u2()?;
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let access = r.u2()?;
        let name_index = r.u2()?;
        let descriptor_index = r.u2()?;
        let attributes = parse_raw_attributes(r)?;
        out.push(RawMember {
            access,
            name_index,
            descriptor_index,
            attributes,
        });
    }
    Ok(out)
}

fn parse_raw_attributes(r: &mut Reader<'_>) -> Result<Vec<RawAttribute>, Error> {
    let count = r.u2()?;
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let name_index = r.u2()?;
        let len = r.u4()? as usize;
        let payload = r.bytes(len)?.to_vec();
        out.push(RawAttribute {
            name_index,
            payload,
        });
    }
    Ok(out)
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&'_ [u8], Error> {
        if self.pos + n > self.bytes.len() {
            return Err(Error {
                message: format!("unexpected end of file (needed {n} bytes)"),
                offset: self.pos,
            });
        }
        let out = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn u1(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn u2(&mut self) -> Result<u16, Error> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn u4(&mut self) -> Result<u32, Error> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn bytes(&mut self, n: usize) -> Result<&'_ [u8], Error> {
        self.take(n)
    }
}

/// Decodes modified UTF-8 with the resolver's rules (a private re-derive is
/// avoided by calling the resolver's own decoder).
fn decode_modified_utf8(bytes: &[u8]) -> String {
    crate::decode_modified_utf8(bytes)
        .unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned())
}

/// Encodes a Rust string as JVM modified UTF-8 (JVMS §4.4.7): NUL becomes
/// `C0 80`; supplementary characters become two CESU-8 3-byte sequences
/// (the string's UTF-16 units each encode as 1, 2 or 3 bytes).
pub fn encode_modified_utf8(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for unit in text.encode_utf16() {
        if unit != 0 && unit < 0x80 {
            out.push(unit as u8);
        } else if unit < 0x800 {
            out.push(0xC0 | (unit >> 6) as u8);
            out.push(0x80 | (unit & 0x3F) as u8);
        } else {
            out.push(0xE0 | (unit >> 12) as u8);
            out.push(0x80 | ((unit >> 6) & 0x3F) as u8);
            out.push(0x80 | (unit & 0x3F) as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const VEC1: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec1.class");
    const VEC2: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec2.class");
    const VEC2_INNER: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec2$Inner.class");
    const VEC2_BOX: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec2$Box.class");
    const VEC2_SHAPE: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec2$Shape.class");
    const VEC3: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec3.class");
    const VEC3_PAIR: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec3$Pair.class");
    const VEC3_TONE: &[u8] = include_bytes!("../tests/vectors/remap/RemapVec3$Tone.class");

    /// Law 1: parse→serialize is byte-identical on every committed vector
    /// (javac 21.0.13 output: primitives+concat bootstrap, generics+inner
    /// classes+exception table+StackMapTable, records+lambda indy+enum
    /// switch+modified-UTF-8 constants).
    #[test]
    fn round_trip_is_byte_identical() {
        for (name, bytes) in [
            ("vec1", VEC1),
            ("vec2", VEC2),
            ("vec2$Inner", VEC2_INNER),
            ("vec2$Box", VEC2_BOX),
            ("vec2$Shape", VEC2_SHAPE),
            ("vec3", VEC3),
            ("vec3$Pair", VEC3_PAIR),
            ("vec3$Tone", VEC3_TONE),
        ] {
            let raw = RawClassFile::parse(bytes)
                .unwrap_or_else(|e| panic!("{name}: parse failed: {}", e.message));
            let out = raw.serialize();
            assert_eq!(
                out.as_slice(),
                bytes,
                "{name}: serialize diverged from the javac bytes"
            );
        }
    }

    /// Law 2: the raw reader and the resolver read the same bytes and must
    /// agree on the names, descriptors and member census.
    #[test]
    fn raw_reader_agrees_with_the_resolver() {
        let resolved = crate::ClassFile::parse(VEC2).unwrap();
        let raw = RawClassFile::parse(VEC2).unwrap();
        assert_eq!(resolved.this_class, raw.this_class_name().unwrap());
        let raw_super = if raw.super_class == 0 {
            None
        } else {
            Some(raw.pool.class_name(raw.super_class).unwrap())
        };
        assert_eq!(resolved.super_class.as_deref(), raw_super.as_deref());
        let raw_ifaces: Vec<String> = raw
            .interfaces
            .iter()
            .map(|i| raw.pool.class_name(*i).unwrap())
            .collect();
        assert_eq!(resolved.interfaces, raw_ifaces);
        assert_eq!(resolved.methods.len(), raw.methods.len());
        assert_eq!(resolved.fields.len(), raw.fields.len());
        for (m, r) in resolved.methods.iter().zip(raw.methods.iter()) {
            assert_eq!(m.name, raw.pool.utf8(r.name_index).unwrap());
            assert_eq!(m.descriptor, raw.pool.utf8(r.descriptor_index).unwrap());
            // The Code attribute bytes the resolver lifted must be a slice
            // of the raw payload (max_stack u2 + max_locals u2 + code_len
            // u4 prefix, then the code).
            if let Some(code) = &m.code {
                let payload = r
                    .attributes
                    .iter()
                    .find(|a| raw.pool.utf8(a.name_index).unwrap() == "Code")
                    .expect("the resolver saw Code, the raw reader must carry it");
                let code_len = u32::from_be_bytes([
                    payload.payload[4],
                    payload.payload[5],
                    payload.payload[6],
                    payload.payload[7],
                ]) as usize;
                assert_eq!(
                    &payload.payload[8..8 + code_len],
                    code.code.as_slice(),
                    "code bytes for {}{}",
                    m.name,
                    m.descriptor
                );
            }
        }
    }

    /// Law 3: `retarget_this_class` appends exactly one Utf8, rewrites
    /// exactly one Class entry, and leaves EVERY member attribute payload
    /// (Code bodies, StackMapTable, exception tables) and every other pool
    /// slot byte-identical — the serialized class still verifies: the
    /// resolver accepts it and reads the new name.
    #[test]
    fn retarget_this_class_rewrites_only_the_pool() {
        let mut raw = RawClassFile::parse(VEC2).unwrap();
        let before_members = (raw.fields.clone(), raw.methods.clone());
        let before_slots = raw.pool.slots.clone();
        raw.retarget_this_class("remapped/RemapVec2").unwrap();
        // exactly one slot appended, and it is the new Utf8
        assert_eq!(raw.pool.slots.len(), before_slots.len() + 1);
        assert_eq!(
            raw.pool.slots.last().unwrap().as_ref().unwrap().utf8_str(),
            Some("remapped/RemapVec2".to_string())
        );
        // every OTHER slot is byte-identical; the Class entry changed only
        // in its name_index
        for (i, (was, now)) in before_slots.iter().zip(raw.pool.slots.iter()).enumerate() {
            match (was, now) {
                (Some(w), Some(n)) if i == usize::from(raw.this_class) => {
                    let old_name = w.class_name_index().unwrap();
                    let new_name = n.class_name_index().unwrap();
                    assert_ne!(old_name, new_name, "the Class entry was retargeted");
                    assert_eq!(w.tag, n.tag);
                }
                _ => assert_eq!(was, now, "slot {i} must not move"),
            }
        }
        // members untouched, byte for byte
        assert_eq!(before_members.0, raw.fields);
        assert_eq!(before_members.1, raw.methods);
        // the rewritten bytes re-parse on BOTH readers; the resolver sees
        // the new name; the StackMapTable payloads ride untouched inside
        // the member attributes (they were compared byte-identical above)
        let out = raw.serialize();
        assert_eq!(
            crate::ClassFile::parse(&out).unwrap().this_class,
            "remapped/RemapVec2"
        );
        let reparsed = RawClassFile::parse(&out).unwrap();
        assert_eq!(reparsed.serialize(), out);
    }

    /// Law 4: modified UTF-8 survives the transport bit-for-bit. A
    /// hand-built pool with a NUL (`C0 80`) and a lone surrogate (CESU-8
    /// `ED A0 B4`, JVMS 4.4.7's legal encoding) round-trips exactly; the
    /// resolver still loads the class (lossy name) — the raw transport is
    /// the layer that must NOT normalize.
    #[test]
    fn modified_utf8_survives_the_transport() {
        // pool: 1 = Utf8 "\u{D834}" (lone surrogate, CESU-8), 2 = Utf8
        // "a\u{0}b" (C0 80), 3 = Class → 1
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 55]); // minor 0, major 55
        bytes.extend_from_slice(&[0, 4]); // pool count 4 (slot 0 + 3)
        bytes.extend_from_slice(&[1, 0, 3, 0xED, 0xA0, 0xB4]); // Utf8 lone surrogate
        bytes.extend_from_slice(&[1, 0, 4, b'a', 0xC0, 0x80, b'b']); // Utf8 NUL as C0 80
        bytes.extend_from_slice(&[7, 0, 1]); // Class → slot 1
        bytes.extend_from_slice(&[0x00, 0x21]); // ACC_PUBLIC | ACC_SUPER
        bytes.extend_from_slice(&[0, 3]); // this_class
        bytes.extend_from_slice(&[0, 0]); // super_class = none (Object-less)
                                          // no interfaces / no fields / no methods / no class attributes
        bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
        let raw = RawClassFile::parse(&bytes).unwrap();
        assert_eq!(raw.serialize(), bytes, "law 1 on the synthetic pool");
        // the raw layer carries the exotic bytes verbatim
        assert_eq!(
            raw.pool.slots[1].as_ref().unwrap().utf8_bytes().unwrap(),
            &[0xED_u8, 0xA0, 0xB4][..]
        );
        assert_eq!(
            raw.pool.slots[2].as_ref().unwrap().utf8_bytes().unwrap(),
            &[b'a', 0xC0, 0x80, b'b'][..]
        );
        // the resolver loads the same class (lossy on the lone surrogate)
        // and the transport's re-encoded spelling matches the original.
        // The retargeted name carries a REAL supplementary scalar (U+1D54E:
        // a &str cannot hold a lone surrogate, so the encoder's job is the
        // paired-surrogate CESU-8 spelling, ED A0 B4 ED B5 8E) plus a NUL.
        let resolved = crate::ClassFile::parse(&bytes).unwrap();
        assert_eq!(resolved.this_class, "\u{FFFD}");
        let mut renamed = raw.clone();
        renamed.retarget_this_class("\u{1D54E}x\u{0}y").unwrap();
        let out = renamed.serialize();
        let reparsed = RawClassFile::parse(&out).unwrap();
        assert_eq!(
            reparsed.this_class_name().unwrap(),
            "\u{1D54E}x\u{0}y".to_string()
        );
        // the re-encoded Utf8 for the new name carries the same C0 80 NUL
        // spelling and the paired-surrogate CESU-8 spelling
        let new_idx = reparsed.pool.slots[usize::from(reparsed.this_class)]
            .as_ref()
            .unwrap()
            .class_name_index()
            .unwrap();
        let expected = encode_modified_utf8("\u{1D54E}x\u{0}y");
        assert_eq!(
            reparsed.pool.slots[usize::from(new_idx)]
                .as_ref()
                .unwrap()
                .utf8_bytes()
                .unwrap(),
            expected.as_slice()
        );
    }
}
