//! The build manifest (M4 wave-1, tasks T06/T07): the per-class map that
//! makes incremental recompiles sound.
//!
//! # Why a manifest at all
//!
//! `j2r compile` is a pure function from (input `.class` bytes, the program's
//! *signature surface*, codegen options, emitter version) to emitted crate
//! files. A class's emitted Rust module depends on
//!
//! * its own bytecode (bodies, constant pool, exception tables), and
//! * the *resolution surface* of every other class in the program — method
//!   and field signatures, hierarchy edges, static-ness — because method
//!   bodies embed the resolved call/field targets of other classes, and
//! * the global slot allocation (static-field slots are assigned in input
//!   order), and
//! * the emitter version and `emit_cfg` option.
//!
//! It does **not** depend on other classes' method *bodies*. Editing the
//! body of `Util.g` must not force re-emitting `App` — that is what makes
//! per-class incrementality real for large codebases.
//!
//! # Soundness of a reuse decision
//!
//! A class is *reused* (its emitted file is kept and its rendering skipped)
//! only when all four hold:
//!
//! 1. the recorded emitter version, `emit_cfg` and global surface hash match
//!    the current run (nothing structural moved anywhere),
//! 2. the recorded input hash equals SHA-256 of the class's `.class` bytes
//!    *as read on this run* (the class's own bytecode is unchanged),
//! 3. the emitted file exists on disk,
//! 4. SHA-256 of that file equals the recorded emitted hash (the file on
//!    disk is exactly what j2r wrote — not stale, not hand-edited).
//!
//! Given 1–4, re-rendering would produce byte-identical output (the emitter
//! is a deterministic pure function), so skipping it cannot change the crate.
//! Any doubt — missing/corrupt manifest, length mismatch between inputs and
//! classes, hash mismatch — falls back to a full re-emit: the manifest is an
//! optimization cache, never authoritative. Deleting the manifest file is
//! always a clean rebuild.
//!
//! # File format
//!
//! `j2r-manifest.tsv`, tab-separated, one record per line — trivially
//! parseable without a JSON dependency (the workspace has none):
//!
//! ```text
//! v2  emitter=2       emit_cfg=false  emit_typed=false        surface=<64 hex>        layout=<64 hex|->       crate=<name>
//! e   <pkg/Class>     <input sha>     <emitted sha>   <emitted mtime_ns.size|->
//! g   <src/lib.rs>    <sha>
//! ```
//!
//! `e` lines are the per-class input-hash → emitted-hash map; `g` lines are
//! glue files (everything the crate needs beyond per-class modules), hashed
//! so any template change invalidates cleanly. `layout` is the partition
//! layout hash (`-` for single-crate emission): the class → crate mapping
//! is a deterministic function of the name-sorted classes and the
//! per-crate budget, so a budget or membership change reshuffles it and
//! must invalidate the per-class reuse gate.
//!
//! v2 (G2.2, F2) adds the emitted file's `(mtime_ns, size)` to the `e`
//! line: the reuse check fast-paths on a stat match and only falls back to
//! the SHA-256 content hash when the stat differs — the measured no-op
//! cost was rehash-bound (~854 MB of emitted sources ≈ 22 MB/s). A legacy
//! 4-field `e` line parses with `emitted_stat: None` (content-hash path);
//! a v1 header parses fine and every entry takes the slow path once, then
//! records stats. The manifest remains a CACHE: any parse problem, missing
//! entry or stat/hash mismatch falls back to rendering, never to trusting
//! a doubtful state. The stat fast path's trust model is the standard
//! incremental-build one (same as Cargo fingerprints): a file whose
//! `(mtime, size)` match the recorded pair is assumed unchanged —
//! adversarial same-stat rewrites are out of scope, and any other doubt
//! still hits the content hash.

use crate::sha256::{sha256_file, sha256_hex};
use crate::{ClassPlan, Program};
use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};

/// Bumped whenever any emitted byte could change for identical inputs
/// (emitter logic, templates, runtime vendoring). Recorded in the manifest;
/// a mismatch forces a full re-emit. v2: the G2.6 array-literal peephole
/// changes emitted bytes for clinit-bearing classes. v3: the G4.2 JMM
/// boundary marker — every emitted crate gains `src/jmm_boundary.rs`
/// and a `pub mod jmm_boundary;` line in lib.rs; boundary-field access
/// sites gain the marker comment.
pub const EMITTER_VERSION: u32 = 3;

const MANIFEST_NAME: &str = "j2r-manifest.tsv";
const FORMAT_VERSION: &str = "v2";

/// The parsed manifest file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub emitter: u32,
    pub emit_cfg: bool,
    pub emit_typed: bool,
    /// Global signature-surface hash (structural state every per-class
    /// emission reads).
    pub surface: String,
    /// Partition layout hash, or `-` for single-crate emission.
    pub layout: String,
    pub crate_name: String,
    pub entries: BTreeMap<String, Entry>,
    /// glue relative path (against the out dir) → sha256 of the content
    pub glue: BTreeMap<String, String>,
}

impl Manifest {
    pub fn path(out_dir: &Path) -> PathBuf {
        out_dir.join(MANIFEST_NAME)
    }

    pub fn load(out_dir: &Path) -> Option<Manifest> {
        let file = std::fs::File::open(Self::path(out_dir)).ok()?;
        parse(&file)
    }

    pub fn save(&self, out_dir: &Path) -> std::io::Result<()> {
        std::fs::write(Self::path(out_dir), self.render())
    }

    pub fn render(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!(
            "{FORMAT_VERSION}\temitter={}\temit_cfg={}\temit_typed={}\tsurface={}\tlayout={}\tcrate={}\n",
            self.emitter, self.emit_cfg, self.emit_typed, self.surface, self.layout, self.crate_name
        ));
        for (class, e) in &self.entries {
            s.push_str(&format!(
                "e\t{class}\t{}\t{}\t{}\n",
                e.input_sha,
                e.emitted_sha,
                enc_stat(&e.emitted_stat)
            ));
        }
        for (path, sha) in &self.glue {
            s.push_str(&format!("g\t{path}\t{sha}\n"));
        }
        s
    }
}

/// Parse manifest bytes; `None` on any malformed input (the manifest is a
/// cache — a parse failure means "no manifest", i.e. a full re-emit).
pub fn parse(r: impl std::io::Read) -> Option<Manifest> {
    let mut m = Manifest {
        emitter: 0,
        emit_cfg: false,
        emit_typed: false,
        surface: String::new(),
        layout: "-".to_string(),
        crate_name: String::new(),
        entries: BTreeMap::new(),
        glue: BTreeMap::new(),
    };
    let mut header_seen = false;
    let reader = std::io::BufReader::new(r);
    for line in reader.lines() {
        let line = line.ok()?;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        match fields[0] {
            // v2 = the G2.2 stat field on `e` lines; v1 manifests parse
            // fine (their 4-field e lines take the content-hash path).
            "v1" | "v2" => {
                if fields.len() != 7 {
                    return None;
                }
                for f in &fields[1..] {
                    let (k, v) = f.split_once('=')?;
                    match k {
                        "emitter" => m.emitter = v.parse().ok()?,
                        "emit_cfg" => m.emit_cfg = v.parse().ok()?,
                        "emit_typed" => m.emit_typed = v.parse().ok()?,
                        "surface" => {
                            if !is_sha(v) {
                                return None;
                            }
                            m.surface = v.to_string();
                        }
                        "layout" => {
                            if v != "-" && !is_sha(v) {
                                return None;
                            }
                            m.layout = v.to_string();
                        }
                        "crate" => m.crate_name = v.to_string(),
                        _ => return None,
                    }
                }
                header_seen = true;
            }
            "e" => {
                // v2: 5 fields (with the emitted stat); legacy: 4 fields.
                let emitted_stat = match fields.len() {
                    5 => dec_stat(fields[4]),
                    4 => None,
                    _ => return None,
                };
                if !header_seen || !is_sha(fields[2]) || !is_sha(fields[3]) {
                    return None;
                }
                if fields.len() == 5 && emitted_stat.is_none() {
                    // a present-but-malformed stat field is corruption
                    return None;
                }
                m.entries.insert(
                    fields[1].to_string(),
                    Entry {
                        input_sha: fields[2].to_string(),
                        emitted_sha: fields[3].to_string(),
                        emitted_stat,
                    },
                );
            }
            "g" => {
                if !header_seen || fields.len() != 3 || !is_sha(fields[2]) {
                    return None;
                }
                m.glue.insert(fields[1].to_string(), fields[2].to_string());
            }
            _ => return None,
        }
    }
    if header_seen {
        Some(m)
    } else {
        None
    }
}

/// One per-class record: input `.class` hash → emitted `.rs` hash, with
/// the emitted file's `(mtime_ns, size)` when known (G2.2 fast path;
/// `None` for legacy manifest lines).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub input_sha: String,
    pub emitted_sha: String,
    pub emitted_stat: Option<(u128, u64)>,
}

/// The current `(mtime_ns, size)` of a file, or `None` when it cannot be
/// stated (missing/unreadable).
pub fn stat_of(path: &Path) -> Option<(u128, u64)> {
    let md = std::fs::metadata(path).ok()?;
    let mt = md.modified().ok()?;
    let nanos = mt.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    Some((nanos, md.len()))
}

/// `"<mtime_ns>.<size>"` for the manifest `e` line; `-` when unknown.
fn enc_stat(s: &Option<(u128, u64)>) -> String {
    match s {
        Some((mt, sz)) => format!("{mt}.{sz}"),
        None => "-".to_string(),
    }
}

/// Inverse of [`enc_stat`]; `None` for `-` or malformed input.
fn dec_stat(s: &str) -> Option<(u128, u64)> {
    if s == "-" {
        return None;
    }
    let (mt, sz) = s.split_once('.')?;
    Some((mt.parse().ok()?, sz.parse().ok()?))
}

fn is_sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The global signature-surface hash: everything outside per-class method
/// bodies that per-class emission depends on. Any structural change
/// anywhere (signature added/removed/retyped, hierarchy edge, slot
/// renumbering, class order) changes this hash and forces a full re-emit;
/// method-body-only edits leave it untouched and only the edited class
/// re-emits.
pub fn surface_hash(prog: &Program, emit_cfg: bool, emit_typed: bool) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "j2r-surface-v1|emit_cfg={emit_cfg}|emit_typed={emit_typed}|classes={}|statics={}|main={}\n",
        prog.classes.len(),
        prog.total_statics,
        prog.main_class.as_deref().unwrap_or("-"),
    ));
    for c in &prog.classes {
        s.push_str(&format!(
            "C|{}|{}|{}|{}|{}|{}|{}|{}\n",
            c.name,
            c.super_name.as_deref().unwrap_or("java/lang/Object"),
            c.is_interface,
            c.is_abstract,
            c.module,
            c.id,
            c.base_slots,
            c.total_slots,
        ));
        for f in &c.fields {
            s.push_str(&format!(
                "F|{}|{}|{}|{:?}|{:?}\n",
                f.name, f.desc, f.is_static, f.static_slot, f.instance_slot
            ));
        }
        for m in &c.methods {
            s.push_str(&format!(
                "M|{}|{}|{}|{}\n",
                m.name, m.desc, m.is_static, m.fn_name
            ));
        }
        s.push_str(&format!("K|{:?}\n", c.clinit_fn));
    }
    sha256_hex(s.as_bytes())
}

/// The partition layout hash: the class → part-crate mapping is a
/// deterministic function of the name-sorted classes and the per-crate
/// budget k (partition.rs chunks the sorted list), so hashing exactly
/// those two inputs captures every reshuffle. Single-crate emission has
/// no layout and uses `-` in the manifest header.
pub fn partition_layout_hash(prog: &Program, k: usize) -> String {
    let mut names: Vec<&str> = prog.classes.iter().map(|c| c.name.as_str()).collect();
    names.sort_unstable();
    let mut s = format!("j2r-layout-v1|k={k}|n={}\n", names.len());
    for n in names {
        s.push_str(n);
        s.push('\n');
    }
    sha256_hex(s.as_bytes())
}

/// Statistics from one crate emission, reported by the CLI.
#[derive(Debug, Default, Clone)]
pub struct EmitStats {
    /// classes whose emitted file was verified current and kept
    pub classes_reused: usize,
    /// classes rendered this run (changed, new, or unverified)
    pub classes_reemitted: usize,
    /// files whose bytes changed and were written
    pub files_written: usize,
    /// files rendered identical to disk (not rewritten — mtimes preserved,
    /// which is what keeps a following `cargo build` a true no-op)
    pub files_unchanged: usize,
    /// per-class files removed for classes no longer in the program
    pub orphans_removed: usize,
    /// false on the first compile into a directory (no prior manifest)
    pub had_manifest: bool,
    pub manifest_path: PathBuf,
}

impl EmitStats {
    pub fn summary(&self) -> String {
        let cold = if self.had_manifest {
            ""
        } else {
            " (no prior manifest)"
        };
        format!(
            "incremental: {} reused, {} re-emitted, {} files unchanged, {} written, {} orphans removed{cold}",
            self.classes_reused,
            self.classes_reemitted,
            self.files_unchanged,
            self.files_written,
            self.orphans_removed,
        )
    }
}

/// Write `bytes` to `path` only if the on-disk content differs. Returns
/// `Ok(true)` when written, `Ok(false)` when identical (mtime untouched).
pub fn write_if_changed(path: &Path, bytes: &[u8]) -> std::io::Result<bool> {
    if let Ok(existing) = std::fs::read(path) {
        if existing == bytes {
            return Ok(false);
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(true)
}

/// Verify one class for reuse; `true` = all soundness conditions hold
/// (see the module docs). `emitted_path` is the class's per-crate
/// module file (which crate the class lives in is decided by the caller,
/// from the same deterministic layout the emitter uses). The class's
/// input hash comes from [`ClassPlan::input_sha`] — computed at parse
/// time over the exact `.class` bytes this run loaded.
///
/// G2.2 (F2): when the recorded `(mtime_ns, size)` of the emitted file
/// matches the file's current stat, the content hash is SKIPPED (the
/// measured no-op cost was emitted-side rehashing, ~854 MB ≈ 22 MB/s at
/// 20k). On any stat mismatch the content hash decides — a changed file
/// with stale stats re-verifies or forces re-render, never a silent
/// stale reuse.
pub fn verify_reuse(class: &ClassPlan, emitted_path: &Path, manifest: &Manifest) -> bool {
    let entry = match manifest.entries.get(&class.name) {
        Some(e) => e,
        None => return false,
    };
    if class.input_sha != entry.input_sha {
        return false;
    }
    if let Some(recorded) = entry.emitted_stat {
        if stat_of(emitted_path) == Some(recorded) {
            return true;
        }
    }
    match sha256_file(emitted_path) {
        Ok(disk_sha) => disk_sha == entry.emitted_sha,
        Err(_) => false,
    }
}
