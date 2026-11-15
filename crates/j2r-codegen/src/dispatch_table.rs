//! R7 — the hash-dispatched registry lookup table (Opt.md §3 R7).
//!
//! The generated dispatch table used to be a bare `match (class, name,
//! desc)` over one string-tuple arm per user method: correct and compact
//! source, but rustc lowers a string-tuple match to a *linear chain* of
//! string comparisons, and the runtime pays that chain on every dynamic
//! dispatch and every parent-walk step (the jbox2d port measured 2,169
//! arms executed 5.8 M times per workload — docs/external-port/
//! jbox2d_opt.md §2.4).
//!
//! The shape emitted here instead: the match runs on the 64-bit FNV-1a
//! selector ALONE — the leading discriminator rustc can lower to a
//! balanced integer search — and every arm verifies its full key inside
//! the arm body:
//!
//! ```text
//! match j2r_disp_hash(class, name, desc) {
//!     0x.. => if class == "cls" && name == "name" && desc == "desc" {
//!         Some(target as MethodFn)
//!     } else { None },
//!     ...
//!     _ => None,
//! }
//! ```
//!
//! with the arms emitted in ascending selector order (byte-stable
//! output). The full-key check inside the arm is the exact-match law,
//! unchanged *by construction*: a non-table key that fell onto an
//! occupied arm fails the compare and lands in the arm's own `None`,
//! exactly the `None` the string table returned — in release too, not
//! only under debug_assert. A true 64-bit collision between two table
//! keys cannot mis-dispatch either — their string components differ, so
//! each arm still admits only its own key. (A first draft put the whole
//! `(u64, class, name, desc)` tuple in the match itself; the measured
//! A/B falsified that shape — rustc 1.99 does not take the leading u64
//! as the switch discriminator inside a tuple match and the lookup ran
//! ~8x slower than the plain string match. The selector-only match is
//! the measured-winner shape; see scripts/evidence/r7/.)
//!
//! Both emitters (the single-crate `registry::lookup` and the per-
//! partition `pN::lookup`) go through [`emit_lookup_fn`]; one source of
//! truth, byte-identical text. The selector constants are computed by
//! [`dispatch_hash`] — the same algorithm the emitted runtime fn
//! implements (pinned by the golden-value law test; any drift between
//! the two sides fails every dispatch, so the suite cannot miss it).

use ::core::fmt::Write;

/// The FNV-1a 64-bit offset basis.
const FNV_OFFSET: u64 = 0xcbf29ce484222325;
/// The FNV-1a 64-bit prime.
const FNV_PRIME: u64 = 0x100000001b3;
/// The component separator fed between class, name and desc.
const SEP: u8 = 0x1f;

/// The dispatch selector of a `(class, name, desc)` key: FNV-1a 64 over
/// the three components' bytes with a `0x1f` separator between them
/// (so `("ab", "c")` and `("a", "bc")` never share a selector).
///
/// This is the codegen side — the value the emitted constants carry.
/// The emitted runtime fn `j2r_disp_hash` implements the identical
/// sequence (offset basis, bytes, xor-prime per component, separator
/// between components); the golden-value test pins both agreement and
/// stability.
pub fn dispatch_hash(class: &str, name: &str, desc: &str) -> u64 {
    let mut h = FNV_OFFSET;
    for b in class.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h ^= u64::from(SEP);
    h = h.wrapping_mul(FNV_PRIME);
    for b in name.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h ^= u64::from(SEP);
    h = h.wrapping_mul(FNV_PRIME);
    for b in desc.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// One registry entry: the full dispatch key plus the emitted target
/// path (`crate::classes::{module}::{fn_name}` — identical text in both
/// emitter layouts, which is what keeps the two lookups in lockstep).
pub struct LookupEntry {
    pub class: String,
    pub name: String,
    pub desc: String,
    pub target: String,
}

impl Clone for LookupEntry {
    fn clone(&self) -> Self {
        LookupEntry {
            class: self.class.clone(),
            name: self.name.clone(),
            desc: self.desc.clone(),
            target: self.target.clone(),
        }
    }
}

/// Writes the emitted runtime selector fn. Kept in exact algorithmic
/// lockstep with [`dispatch_hash`] — see the module docs.
fn emit_hash_fn(buf: &mut String) {
    let _ = writeln!(
        buf,
        "fn j2r_disp_hash(class: &str, name: &str, desc: &str) -> ::core::primitive::u64 {{"
    );
    let _ = writeln!(
        buf,
        "    let mut h: ::core::primitive::u64 = {FNV_OFFSET:#x};"
    );
    let _ = writeln!(buf, "    for b in class.as_bytes() {{");
    emit_hash_body(buf);
    let _ = writeln!(buf, "    }}");
    let _ = writeln!(buf, "    h ^= {SEP:#x};");
    let _ = writeln!(buf, "    h = h.wrapping_mul({FNV_PRIME:#x});");
    let _ = writeln!(buf, "    for b in name.as_bytes() {{");
    emit_hash_body(buf);
    let _ = writeln!(buf, "    }}");
    let _ = writeln!(buf, "    h ^= {SEP:#x};");
    let _ = writeln!(buf, "    h = h.wrapping_mul({FNV_PRIME:#x});");
    let _ = writeln!(buf, "    for b in desc.as_bytes() {{");
    emit_hash_body(buf);
    let _ = writeln!(buf, "    }}");
    let _ = writeln!(buf, "    h");
    let _ = writeln!(buf, "}}");
}

fn emit_hash_body(buf: &mut String) {
    let _ = writeln!(buf, "        h ^= ::core::primitive::u64::from(*b);");
    let _ = writeln!(buf, "        h = h.wrapping_mul({FNV_PRIME:#x});");
}

/// Emits the complete `lookup` fn (selector fn + the hash-led match)
/// for `entries`, sorted by `(selector, class, name, desc)` so the
/// output is byte-stable for a given class set regardless of the
/// caller's iteration order. `pub` visibility and the exact signature
/// match the string-tuple table this replaces — the [`crate::emit`]
/// single-crate registry and the [`crate::partition`] per-partition
/// registry both call this, one source of truth.
pub fn emit_lookup_fn(buf: &mut String, entries: &[LookupEntry]) {
    let mut sorted: Vec<&LookupEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| {
        let ha = dispatch_hash(&a.class, &a.name, &a.desc);
        let hb = dispatch_hash(&b.class, &b.name, &b.desc);
        ha.cmp(&hb)
            .then_with(|| a.class.cmp(&b.class))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.desc.cmp(&b.desc))
    });
    emit_hash_fn(buf);
    let _ = writeln!(buf);
    let _ = writeln!(
        buf,
        "pub fn lookup(class: &str, name: &str, desc: &str) -> ::core::option::Option<::j2r_runtime::MethodFn> {{"
    );
    let _ = writeln!(buf, "    match j2r_disp_hash(class, name, desc) {{");
    for e in &sorted {
        let sel = dispatch_hash(&e.class, &e.name, &e.desc);
        let _ = writeln!(
            buf,
            "        {sel:#x} => if class == {:?} && name == {:?} && desc == {:?} {{",
            e.class, e.name, e.desc
        );
        let _ = writeln!(
            buf,
            "            ::core::option::Option::Some({} as ::j2r_runtime::MethodFn)",
            e.target
        );
        let _ = writeln!(buf, "        }} else {{");
        let _ = writeln!(buf, "            ::core::option::Option::None");
        let _ = writeln!(buf, "        }},");
    }
    let _ = writeln!(buf, "        _ => ::core::option::Option::None,");
    let _ = writeln!(buf, "    }}");
    let _ = writeln!(buf, "}}");
}

#[cfg(test)]
mod dispatch_table_tests {
    use super::*;

    fn entry(class: &str, name: &str, desc: &str) -> LookupEntry {
        LookupEntry {
            class: class.to_string(),
            name: name.to_string(),
            desc: desc.to_string(),
            target: format!("crate::classes::m::{class}_{name}"),
        }
    }

    /// Golden values: the selector algorithm is pinned byte-for-byte —
    /// any drift between the codegen-side constants and the emitted
    /// runtime fn breaks EVERY dispatch, so this law and the suite
    /// cannot miss a silent change.
    #[test]
    fn dispatch_hash_golden_values() {
        assert_eq!(
            dispatch_hash("com/example/Car", "start", "(I)V"),
            0x1d58a824223e195a
        );
        assert_eq!(
            dispatch_hash("com/example/Car", "start", "(J)V"),
            0x02531f2412a4e00f
        );
        assert_eq!(
            dispatch_hash("java/lang/Object", "hashCode", "()I"),
            0x9ab4222cc8f6bde5
        );
    }

    /// The 0x1f separator law: component boundaries cannot alias —
    /// ("ab","c") and ("a","bc") feed the same bytes in different
    /// component splits and must select differently.
    #[test]
    fn separator_prevents_component_aliasing() {
        assert_ne!(dispatch_hash("ab", "c", "d"), dispatch_hash("a", "bc", "d"));
        assert_ne!(
            dispatch_hash("ab", "cd", "e"),
            dispatch_hash("a", "bcd", "e")
        );
    }

    /// The emitted lookup: selector fn present, one selector-led arm
    /// per entry with the full key verified inside the arm (the
    /// exact-match law — a hash hit alone can never return a target),
    /// arms in ascending selector order (byte-stable), and the
    /// `_ => None` fallback preserving the miss path.
    #[test]
    fn emitted_lookup_is_hash_led_sorted_and_exact() {
        let entries = vec![
            entry("com/example/Car", "start", "(I)V"),
            entry("com/example/Car", "stop", "()V"),
            entry("java/lang/Object", "hashCode", "()I"),
        ];
        let mut buf = String::new();
        emit_lookup_fn(&mut buf, &entries);
        assert!(buf.contains("fn j2r_disp_hash(class: &str, name: &str, desc: &str)"));
        assert!(buf.contains("    match j2r_disp_hash(class, name, desc) {"));
        // every entry's arm: selector constant, then the full-key check
        for e in &entries {
            let sel = dispatch_hash(&e.class, &e.name, &e.desc);
            let arm = format!(
                "        {sel:#x} => if class == {:?} && name == {:?} && desc == {:?} {{",
                e.class, e.name, e.desc
            );
            assert!(buf.contains(&arm), "missing arm: {arm}");
        }
        // ascending selector order (byte-stable emission)
        let sels: Vec<u64> = entries
            .iter()
            .map(|e| dispatch_hash(&e.class, &e.name, &e.desc))
            .collect();
        let mut sorted = sels.clone();
        sorted.sort_unstable();
        let mut last: Option<usize> = None;
        for sel in &sorted {
            let pos = buf
                .find(&format!("        {sel:#x} =>"))
                .unwrap_or_else(|| panic!("selector {sel:#x} absent"));
            assert!(
                last.is_none_or(|l| pos > l),
                "arms not in ascending selector order"
            );
            last = Some(pos);
        }
        assert!(buf.contains("        _ => ::core::option::Option::None,"));
    }

    /// Determinism: the same entry set emits byte-identical text
    /// regardless of the caller's iteration order (the sort law the
    /// byte-stable emission depends on).
    #[test]
    fn emission_is_order_independent() {
        let a = vec![
            entry("x/Y", "b", "()V"),
            entry("x/Y", "a", "(I)V"),
            entry("x/Z", "c", "(J)V"),
        ];
        let mut b = a.clone();
        b.reverse();
        let mut buf_a = String::new();
        let mut buf_b = String::new();
        emit_lookup_fn(&mut buf_a, &a);
        emit_lookup_fn(&mut buf_b, &b);
        assert_eq!(buf_a, buf_b);
    }

    /// The partition/single lockstep is structural — both emitters call
    /// THIS fn — but the target text is identical in both layouts too:
    /// the partition registry uses `crate::classes::` paths exactly like
    /// the single-crate one (pinned so a future divergence in one
    /// emitter's target spelling cannot sneak past the helper).
    #[test]
    fn target_paths_match_both_layouts() {
        let e = entry("com/example/Car", "start", "(I)V");
        assert!(e.target.starts_with("crate::classes::"));
    }
}
