//! CRC-32 (IEEE 802.3, reflected, polynomial `0xEDB88320`) — the checksum
//! ZIP/JAR entries carry (RFC 1952 §"CRC-32").
//!
//! Table-driven, dependency-free, `forbid(unsafe_code)`-clean. Used by
//! `crate::zip` to verify every extracted entry; a mismatch is a loud
//! error, never a silently corrupted class file (M3_PLAN §2, risk M3-R1).

use std::sync::LazyLock;

/// Precomputed reflected CRC-32 table (256 entries), built once on first
/// use (audit A6: `update()` used to rebuild it on every call).
static TABLE: LazyLock<[u32; 256]> = LazyLock::new(build_table);

/// Precomputed reflected CRC-32 table (256 entries).
fn build_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    table
}

/// Streaming CRC-32 state.
#[derive(Clone)]
pub struct Crc32 {
    state: u32,
}

impl Crc32 {
    pub fn new() -> Self {
        Crc32 { state: 0xFFFF_FFFF }
    }

    /// Feed bytes into the running checksum.
    pub fn update(&mut self, data: &[u8]) {
        let mut c = self.state;
        for &b in data {
            c = TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
        self.state = c;
    }

    /// Finalize (XOR-out) and return the checksum.
    pub fn finish(&self) -> u32 {
        self.state ^ 0xFFFF_FFFF
    }
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

/// One-shot convenience: CRC-32 of a byte slice.
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = Crc32::new();
    c.update(data);
    c.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        // Standard check values (RFC 1952 / POSIX cksum corpus).
        assert_eq!(crc32(b""), 0x0000_0000);
        assert_eq!(crc32(b"a"), 0xE8B7_BE43);
        assert_eq!(crc32(b"abc"), 0x3524_41C2);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414F_A339
        );
    }

    #[test]
    fn streaming_matches_one_shot() {
        let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let mut st = Crc32::new();
        st.update(&data[..37]);
        st.update(&data[37..5000]);
        st.update(&data[5000..]);
        assert_eq!(st.finish(), crc32(&data));
    }
}
