//! DEFLATE decompression (RFC 1951) — the compression JAR entries use
//! ("method 8"). Hand-rolled so `j2r-classfile` keeps **zero external
//! dependencies** (M3_PLAN §9); correctness is pinned by test vectors
//! produced by the real JDK `Deflater` and by the differential jar
//! fixtures.
//!
//! Design constraints (M3_PLAN §4.3):
//! * no recursion anywhere — a hostile archive cannot overflow the
//!   native stack;
//! * every out-of-range access is an `Err`, never an index panic;
//! * decompression-bomb guard: the caller caps the uncompressed size
//!   (default 64 MiB per entry) and exceeding it is a loud error.

#![forbid(unsafe_code)]

use std::fmt;

/// Decompression failure with the byte offset in the *compressed* stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InflateError {
    /// Byte offset in the input where decoding failed.
    pub at: usize,
    pub message: String,
}

impl fmt::Display for InflateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "inflate error at byte {}: {}", self.at, self.message)
    }
}

impl std::error::Error for InflateError {}

/// Default bomb guard: 64 MiB uncompressed per entry (M3_PLAN §4.3).
pub const DEFAULT_MAX_OUTPUT: usize = 64 * 1024 * 1024;

// ------------------------------------------------------------------ bit reader

/// LSB-first bit reader over a byte slice (RFC 1951 §3.1.1: "Data elements
/// other than Huffman codes are packed starting with the least-significant
/// bit").
struct BitReader<'a> {
    data: &'a [u8],
    /// Next byte index.
    pos: usize,
    /// Up to 56 buffered bits (LSB = next bit out).
    bits: u64,
    nbits: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader {
            data,
            pos: 0,
            bits: 0,
            nbits: 0,
        }
    }

    /// Byte offset of the next unread bit (for error reporting).
    fn byte_pos(&self) -> usize {
        self.pos
    }

    #[inline]
    fn refill(&mut self) {
        while self.nbits <= 56 && self.pos < self.data.len() {
            self.bits |= (self.data[self.pos] as u64) << self.nbits;
            self.pos += 1;
            self.nbits += 8;
        }
    }

    /// Read one bit.
    #[inline]
    fn bit(&mut self) -> Result<u32, InflateError> {
        if self.nbits == 0 {
            self.refill();
            if self.nbits == 0 {
                return Err(InflateError {
                    at: self.pos,
                    message: "unexpected end of input".to_string(),
                });
            }
        }
        let b = (self.bits & 1) as u32;
        self.bits >>= 1;
        self.nbits -= 1;
        Ok(b)
    }

    /// Read `n` bits (n <= 32), LSB-first.
    #[inline]
    fn bits(&mut self, n: u32) -> Result<u32, InflateError> {
        debug_assert!(n <= 32);
        if n == 0 {
            return Ok(0);
        }
        if self.nbits < n {
            self.refill();
            if self.nbits < n {
                return Err(InflateError {
                    at: self.pos,
                    message: format!("unexpected end of input (wanted {n} bits)"),
                });
            }
        }
        let mask = if n == 64 { u64::MAX } else { (1u64 << n) - 1 };
        let v = (self.bits & mask) as u32;
        self.bits >>= n;
        self.nbits -= n;
        Ok(v)
    }

    /// Align to the next byte boundary. Subsequent whole-byte reads
    /// (`bits(8)`) then consume exactly the stream bytes that follow —
    /// buffered-but-unconsumed bytes stay in the buffer, which keeps the
    /// position domain single and absolute for the whole stream.
    fn align(&mut self) {
        let dropped = self.nbits % 8;
        self.bits >>= dropped;
        self.nbits -= dropped;
    }
}

// ------------------------------------------------------------------ Huffman

/// A canonical Huffman decoding table: code lengths → sorted symbols.
///
/// Decoding is the classic bit-at-a-time walk (as in `puff.c`): simple,
/// allocation-free per symbol, and immune to malformed-code panic paths.
#[derive(Debug)]
struct Huffman {
    /// Number of symbols of each code length 1..=15.
    counts: [u16; 16],
    /// Symbols ordered by (length, symbol value).
    symbols: Vec<u16>,
    /// `true` when every symbol has length 0 (an empty code — legal for
    /// the distance tree of a literal-only dynamic block; decode errors
    /// if a distance is ever demanded).
    empty: bool,
}

#[derive(Debug)]
enum TableError {
    OverSubscribed,
    Incomplete,
}

/// Which zlib table role the lengths describe (audit A5). The incomplete-
/// set rule differs by role, per zlib's `inftrees.c` (upstream madler/zlib,
/// the code `java.util.zip.Inflater` wraps): an incomplete Huffman code is
/// legal **only** when it consists of exactly one 1-bit code, and never
/// inside the code-length alphabet. An empty set is always accepted and
/// errors only when a symbol is actually demanded (zlib builds an
/// invalid-code marker table there).
#[derive(Clone, Copy, Debug, PartialEq)]
enum TableKind {
    /// The 19-symbol code-length alphabet (zlib `CODES`).
    CodeLengths,
    /// Literal/length symbols (zlib `LENS`).
    Literals,
    /// Distance symbols (zlib `DISTS`).
    Distances,
}

impl Huffman {
    /// Build from a code-length list (RFC 1951 §3.2.2 canonical rules),
    /// with zlib's exact acceptance rules for the given table role
    /// (audit A5 — the old `allow_incomplete: bool` was lenient in a way
    /// zlib is not, and the strict literal path was stricter than zlib in
    /// the single-1-bit-code case).
    fn from_lengths(lengths: &[u8], kind: TableKind) -> Result<Huffman, TableError> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0; // length-0 means "unused symbol"

        // Kraft check: reject over-subscribed codes outright.
        let mut left: i32 = 1;
        for &count in &counts[1..] {
            left <<= 1;
            left -= count as i32;
            if left < 0 {
                return Err(TableError::OverSubscribed);
            }
        }
        let used: u32 = counts[1..].iter().map(|&c| c as u32).sum();
        let empty = used == 0;
        // zlib inftrees.c: `if (left > 0 && (type == CODES || max != 1))
        // return -1;` — incomplete rejected unless exactly one 1-bit code,
        // never for the code-length alphabet. Empty sets bypass this
        // (zlib's `max == 0` builds an error marker and returns success).
        if left > 0 && !empty {
            let max = (1..=15).rev().find(|&l| counts[l] != 0).unwrap_or(0);
            if kind == TableKind::CodeLengths || max != 1 {
                return Err(TableError::Incomplete);
            }
        }

        // Offsets: first code of each length.
        let mut offs = [0u16; 16];
        for len in 1..15 {
            offs[len + 1] = offs[len] + counts[len];
        }

        let mut symbols = vec![0u16; used as usize];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Huffman {
            counts,
            symbols,
            empty,
        })
    }

    /// Decode one symbol, bit by bit.
    fn decode(&self, br: &mut BitReader<'_>) -> Result<u16, InflateError> {
        if self.empty {
            return Err(InflateError {
                at: br.byte_pos(),
                message: "huffman decode on empty code".to_string(),
            });
        }
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..=15usize {
            code |= br.bit()? as i32;
            let count = self.counts[len] as i32;
            if code - first < count {
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(InflateError {
            at: br.byte_pos(),
            message: "ran out of huffman codes (over-long code)".to_string(),
        })
    }
}

// Length codes 257..=285 (RFC 1951 §3.2.5).
const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
// Distance codes 0..=29 (RFC 1951 §3.2.5).
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
// Code-length alphabet permutation (RFC 1951 §3.2.7).
const CLC_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

const MAX_OUTPUT: usize = DEFAULT_MAX_OUTPUT;

fn err(at: usize, msg: &str) -> InflateError {
    InflateError {
        at,
        message: msg.to_string(),
    }
}

/// Build the distance table with zlib-exact acceptance (audit A5): an
/// incomplete set is legal only as a single 1-bit code; empty is accepted
/// and errors on demand; anything else incomplete is rejected exactly like
/// zlib's `Inflater` ("invalid distances set").
fn build_distance_table(lengths: &[u8], at: usize) -> Result<Huffman, InflateError> {
    Huffman::from_lengths(lengths, TableKind::Distances).map_err(|e| match e {
        TableError::OverSubscribed => err(at, "over-subscribed distance code"),
        TableError::Incomplete => err(at, "incomplete distance code"),
    })
}

/// Inflate a raw DEFLATE stream (no zlib/gzip wrapper — JAR entries store
/// raw deflate). Appends to `out`, which may be pre-sized by the caller.
pub fn inflate(data: &[u8], out: &mut Vec<u8>) -> Result<(), InflateError> {
    inflate_capped(data, out, MAX_OUTPUT)
}

/// Inflate with an explicit uncompressed-size cap (bomb guard).
pub fn inflate_capped(
    data: &[u8],
    out: &mut Vec<u8>,
    max_output: usize,
) -> Result<(), InflateError> {
    let mut br = BitReader::new(data);
    loop {
        let bfinal = br.bit()?;
        let btype = br.bits(2)?;
        match btype {
            0b00 => {
                // Stored block (RFC 1951 §3.1.4): align to a byte
                // boundary, then LEN, NLEN, and LEN raw bytes — all read
                // through the same reader so the position domain stays
                // single and absolute across any number of blocks.
                br.align();
                let len = br.bits(8)? as usize | (br.bits(8)? as usize) << 8;
                let nlen = br.bits(8)? as usize | (br.bits(8)? as usize) << 8;
                if len != (!nlen & 0xFFFF) {
                    return Err(err(br.byte_pos(), "stored block LEN/NLEN mismatch"));
                }
                if out.len() + len > max_output {
                    return Err(err(
                        br.byte_pos(),
                        "output exceeds the size cap (bomb guard)",
                    ));
                }
                for _ in 0..len {
                    out.push(br.bits(8)? as u8);
                }
            }
            0b01 => {
                // Fixed Huffman (RFC 1951 §3.2.6). The fixed distance table
                // is built in its complete 32-entry five-bit form (30 real
                // codes plus the two impossible codes 30/31, which the
                // `dsym >= 30` use-time check rejects) — exactly how zlib
                // builds it, so the table is never incomplete (audit A5).
                let mut lit_lengths = [0u8; 288];
                for (i, l) in lit_lengths.iter_mut().enumerate() {
                    *l = match i {
                        0..=143 => 8,
                        144..=255 => 9,
                        256..=279 => 7,
                        _ => 8,
                    };
                }
                let lit = Huffman::from_lengths(&lit_lengths, TableKind::Literals)
                    .map_err(|_| err(br.byte_pos(), "fixed literal table build failed"))?;
                let dist_lengths = [5u8; 32];
                let dist = Huffman::from_lengths(&dist_lengths, TableKind::Distances)
                    .map_err(|_| err(br.byte_pos(), "fixed distance table build failed"))?;
                inflate_block(&mut br, &lit, &dist, out, max_output)?;
            }
            0b10 => {
                // Dynamic Huffman (RFC 1951 §3.2.7).
                let hlit = br.bits(5)? as usize + 257;
                let hdist = br.bits(5)? as usize + 1;
                let hclen = br.bits(4)? as usize + 4;
                if hlit > 286 {
                    return Err(err(br.byte_pos(), "HLIT > 286"));
                }
                if hdist > 30 {
                    return Err(err(br.byte_pos(), "HDIST > 30"));
                }
                let mut clc_lengths = [0u8; 19];
                for i in 0..hclen {
                    clc_lengths[CLC_ORDER[i]] = br.bits(3)? as u8;
                }
                let clc = Huffman::from_lengths(&clc_lengths, TableKind::CodeLengths).map_err(
                    |e| match e {
                        TableError::OverSubscribed => {
                            err(br.byte_pos(), "over-subscribed code-length code")
                        }
                        TableError::Incomplete => err(br.byte_pos(), "incomplete code-length code"),
                    },
                )?;

                let mut lengths = vec![0u8; hlit + hdist];
                let mut i = 0;
                while i < hlit + hdist {
                    let sym = clc.decode(&mut br)?;
                    match sym {
                        0..=15 => {
                            lengths[i] = sym as u8;
                            i += 1;
                        }
                        16 => {
                            if i == 0 {
                                return Err(err(br.byte_pos(), "repeat with no previous length"));
                            }
                            let prev = lengths[i - 1];
                            let rep = 3 + br.bits(2)? as usize;
                            for _ in 0..rep {
                                if i >= lengths.len() {
                                    return Err(err(br.byte_pos(), "length repeat overflows"));
                                }
                                lengths[i] = prev;
                                i += 1;
                            }
                        }
                        17 => {
                            let rep = 3 + br.bits(3)? as usize;
                            i += rep;
                            if i > lengths.len() {
                                return Err(err(br.byte_pos(), "zero repeat overflows"));
                            }
                        }
                        18 => {
                            let rep = 11 + br.bits(7)? as usize;
                            i += rep;
                            if i > lengths.len() {
                                return Err(err(br.byte_pos(), "long zero repeat overflows"));
                            }
                        }
                        _ => return Err(err(br.byte_pos(), "bad code-length symbol")),
                    }
                }
                if lengths[256] == 0 {
                    return Err(err(br.byte_pos(), "no end-of-block code"));
                }
                let lit =
                    Huffman::from_lengths(&lengths[..hlit], TableKind::Literals).map_err(|e| {
                        match e {
                            TableError::OverSubscribed => {
                                err(br.byte_pos(), "over-subscribed literal code")
                            }
                            TableError::Incomplete => err(br.byte_pos(), "incomplete literal code"),
                        }
                    })?;
                let dist = build_distance_table(&lengths[hlit..], br.byte_pos())?;
                inflate_block(&mut br, &lit, &dist, out, max_output)?;
            }
            _ => return Err(err(br.byte_pos(), "reserved block type 0b11")),
        }
        if bfinal == 1 {
            return Ok(());
        }
    }
}

/// Decode one compressed block body with pre-built tables.
fn inflate_block(
    br: &mut BitReader<'_>,
    lit: &Huffman,
    dist: &Huffman,
    out: &mut Vec<u8>,
    max_output: usize,
) -> Result<(), InflateError> {
    loop {
        let sym = lit.decode(br)?;
        match sym {
            0..=255 => {
                out.push(sym as u8);
                if out.len() > max_output {
                    return Err(err(
                        br.byte_pos(),
                        "output exceeds the size cap (bomb guard)",
                    ));
                }
            }
            256 => return Ok(()),
            257..=285 => {
                let li = sym as usize - 257;
                let len = LENGTH_BASE[li] as usize + br.bits(LENGTH_EXTRA[li] as u32)? as usize;
                let dsym = dist.decode(br)? as usize;
                if dsym >= 30 {
                    return Err(err(br.byte_pos(), "invalid distance symbol"));
                }
                let d = DIST_BASE[dsym] as usize + br.bits(DIST_EXTRA[dsym] as u32)? as usize;
                if d > out.len() {
                    return Err(err(
                        br.byte_pos(),
                        "distance reaches before the start of output",
                    ));
                }
                if out.len() + len > max_output {
                    return Err(err(
                        br.byte_pos(),
                        "output exceeds the size cap (bomb guard)",
                    ));
                }
                // Byte-at-a-time copy: correct when the match overlaps the
                // destination (RLE-style), which byte-at-a-time handles by
                // construction.
                let start = out.len() - d;
                for k in 0..len {
                    let b = out[start + k];
                    out.push(b);
                }
            }
            _ => return Err(err(br.byte_pos(), "invalid literal/length symbol")),
        }
    }
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------ audit A5 table rules

    #[test]
    fn single_one_bit_code_is_accepted_for_symbol_tables() {
        // zlib inftrees.c: an incomplete set is legal only when it is
        // exactly one 1-bit code — for both LENS and DISTS.
        for kind in [TableKind::Literals, TableKind::Distances] {
            let t = Huffman::from_lengths(&[1, 0, 0, 0], kind)
                .expect("single 1-bit code must be accepted");
            assert!(!t.empty);
        }
    }

    #[test]
    fn single_one_bit_code_is_rejected_for_the_code_length_alphabet() {
        // ... but never inside the code-length alphabet (zlib CODES).
        let e = Huffman::from_lengths(&[1, 0, 0, 0], TableKind::CodeLengths).unwrap_err();
        assert!(matches!(e, TableError::Incomplete));
    }

    #[test]
    fn multi_code_incomplete_sets_are_rejected() {
        // Three 2-bit codes use 3/4 of the space: incomplete with max != 1
        // — rejected for every symbol role, exactly like zlib.
        for kind in [
            TableKind::CodeLengths,
            TableKind::Literals,
            TableKind::Distances,
        ] {
            let e = Huffman::from_lengths(&[2, 2, 2, 0], kind).unwrap_err();
            assert!(matches!(e, TableError::Incomplete));
        }
    }

    #[test]
    fn empty_tables_are_accepted_and_error_on_demand() {
        // zlib's max == 0 case: build succeeds with an error marker; the
        // error surfaces only when a symbol is demanded.
        for kind in [
            TableKind::CodeLengths,
            TableKind::Literals,
            TableKind::Distances,
        ] {
            let t = Huffman::from_lengths(&[0, 0, 0], kind).expect("empty set accepted");
            assert!(t.empty);
        }
    }

    #[test]
    fn over_subscribed_sets_are_always_rejected() {
        for kind in [
            TableKind::CodeLengths,
            TableKind::Literals,
            TableKind::Distances,
        ] {
            let e = Huffman::from_lengths(&[1, 1, 1], kind).unwrap_err();
            assert!(matches!(e, TableError::OverSubscribed));
        }
    }

    #[test]
    fn fixed_distance_table_is_now_complete() {
        // The 32-entry five-bit fixed form has Kraft sum exactly 1 — it
        // must build under the strict rule (it used to rely on
        // allow_incomplete with 30 of 32 codes).
        let t = Huffman::from_lengths(&[5u8; 32], TableKind::Distances)
            .expect("32x5-bit fixed distance table is complete");
        assert!(!t.empty);
    }

    /// Bit-writer for hand-built streams (stored blocks).
    struct BitWriter {
        out: Vec<u8>,
        cur: u8,
        n: u32,
    }
    impl BitWriter {
        fn new() -> Self {
            BitWriter {
                out: Vec::new(),
                cur: 0,
                n: 0,
            }
        }
        fn bit(&mut self, b: u32) {
            self.cur |= ((b & 1) as u8) << self.n;
            self.n += 1;
            if self.n == 8 {
                self.out.push(self.cur);
                self.cur = 0;
                self.n = 0;
            }
        }
        fn bits(&mut self, v: u32, n: u32) {
            for i in 0..n {
                self.bit((v >> i) & 1);
            }
        }
        /// Flush a stored block: BFINAL, BTYPE=00, pad to byte, LEN, NLEN, data.
        fn stored_block(&mut self, bfinal: bool, data: &[u8]) {
            self.bit(bfinal as u32);
            self.bits(0b00, 2);
            if self.n > 0 {
                self.out.push(self.cur);
                self.cur = 0;
                self.n = 0;
            }
            self.out
                .extend_from_slice(&(data.len() as u16).to_le_bytes());
            self.out
                .extend_from_slice(&(!(data.len() as u16)).to_le_bytes());
            self.out.extend_from_slice(data);
        }
        fn finish(mut self) -> Vec<u8> {
            if self.n > 0 {
                self.out.push(self.cur);
            }
            self.out
        }
    }

    #[test]
    fn stored_single_block() {
        let payload = b"hello stored block, hello!".to_vec();
        let mut w = BitWriter::new();
        w.stored_block(true, &payload);
        let compressed = w.finish();
        let mut out = Vec::new();
        inflate(&compressed, &mut out).unwrap();
        assert_eq!(out, payload);
    }

    #[test]
    fn stored_multi_block() {
        let payload: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
        let mut w = BitWriter::new();
        w.stored_block(false, &payload[..400]);
        w.stored_block(false, &payload[400..800]);
        w.stored_block(true, &payload[800..]);
        let compressed = w.finish();
        let mut out = Vec::new();
        inflate(&compressed, &mut out).unwrap();
        assert_eq!(out, payload);
    }

    #[test]
    fn truncated_stored_errors() {
        let mut w = BitWriter::new();
        w.stored_block(true, b"0123456789");
        let mut c = w.finish();
        c.truncate(c.len() - 3); // cut into the payload
        let mut out = Vec::new();
        assert!(inflate(&c, &mut out).is_err());
    }

    #[test]
    fn bad_nlen_errors() {
        let mut w = BitWriter::new();
        w.bit(1);
        w.bits(0b00, 2);
        w.out.push(4);
        w.out.push(0);
        w.out.push(0xFF); // NLEN must be ~LEN
        w.out.push(0xFF);
        w.out.extend_from_slice(b"abcd");
        let mut out = Vec::new();
        assert!(inflate(&w.finish(), &mut out).is_err());
    }

    #[test]
    fn reserved_block_type_errors() {
        let mut w = BitWriter::new();
        w.bit(1);
        w.bits(0b11, 2);
        let mut out = Vec::new();
        assert!(inflate(&w.finish(), &mut out).is_err());
    }

    #[test]
    fn bomb_guard_fires() {
        // LEN is a u16, so one stored block holds at most 65 535 bytes;
        // 40 000 in one block with a 20 000 cap trips the guard cleanly.
        let payload = vec![0u8; 40_000];
        let mut w = BitWriter::new();
        w.stored_block(true, &payload);
        let mut out = Vec::new();
        let e = inflate_capped(&w.finish(), &mut out, 20_000).unwrap_err();
        assert!(e.message.contains("bomb guard"), "{e}");
    }

    #[test]
    fn empty_input_errors() {
        let mut out = Vec::new();
        assert!(inflate(&[], &mut out).is_err());
    }

    // ---- JDK-produced vectors ----------------------------------------
    // `tests/vectors/deflate_l{0,1,9}.bin` hold raw DEFLATE streams of the
    // same 4 KiB payload, produced by `java.util.zip.Deflater` at levels
    // 0, 1 and 9 (see tests/vectors/README). The expected output is the
    // shared payload file `tests/vectors/deflate_payload.bin`.

    fn payload() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/vectors/deflate_payload.bin"
        ))
        .expect("vector payload")
    }

    fn check_vector(name: &str) {
        let compressed = std::fs::read(format!(
            "{}/tests/vectors/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("vector file");
        let mut out = Vec::new();
        inflate(&compressed, &mut out).expect("inflate succeeds");
        assert_eq!(out, payload(), "level {name} output mismatch");
    }

    #[test]
    fn jdk_deflate_level0() {
        check_vector("deflate_l0.bin");
    }
    #[test]
    fn jdk_deflate_level1() {
        check_vector("deflate_l1.bin");
    }
    #[test]
    fn jdk_deflate_level9() {
        check_vector("deflate_l9.bin");
    }

    #[test]
    fn jdk_deflate_corrupt_trailing_garbage_tail_ok() {
        // Trailing garbage after BFINAL is ignored by construction
        // (we stop at the final block); assert no panic and correct data.
        let mut c = std::fs::read(format!(
            "{}/tests/vectors/deflate_l9.bin",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        c.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        let mut out = Vec::new();
        inflate(&c, &mut out).unwrap();
        assert_eq!(out, payload());
    }
}
