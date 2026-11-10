//! M5 T05 — crate-partitioned emission (docs/M5_SCALE.md §3).
//!
//! The scale census (TODO.md §M5 T01) measured rustc at ~4.2 MB peak
//! memory per emitted class in ONE process: a 4 GB machine dies near
//! ~1000 classes, and the registered 70k-class target would need
//! ~300 GB. This module emits the generated program as a cargo
//! **workspace** instead:
//!
//! ```text
//! out/
//!   Cargo.toml        [workspace] members = support, p0..pN, app
//!   runtime/          the vendored j2r-runtime (shared path dependency)
//!   support/          program-agnostic: statics pool, link table,
//!                     dispatch slot (the three seams cross-partition
//!                     code is allowed to touch)
//!   p0..pN/           ≤ k classes each; same-partition calls stay
//!                     direct paths; cross-partition calls go through
//!                     the link table
//!   app/              main.rs + the composite Dispatch + registration
//! ```
//!
//! Why a link table: Java classes are mutually recursive, Rust crates
//! cannot be. Partition p's call to class X in partition q emits
//! `support::link::call(GID, recv, args)` — one global method id
//! assigned deterministically over the name-sorted class list — and q's
//! `register_link` fills that slot at startup. Same-partition calls keep
//! the exact pre-M5 direct-call shape, so the dispatch cost is paid only
//! at partition boundaries (the same cost class as the vtable dispatch
//! the runtime already performs; T23's inline caches can cache hot
//! cross-partition targets later).
//!
//! Correctness contract: the link table carries the SAME fn pointers the
//! direct paths would have called, so semantics are identical by
//! construction; the farm differential (T04) and the differential
//! battery pin it end to end.

use crate::manifest::{write_if_changed, EmitStats, Entry, Manifest, EMITTER_VERSION};
use crate::sha256::sha256_hex;
use crate::{ClassPlan, CodegenError, Options, Program};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

const ALLOW: &str = "#![allow(dead_code, unused_variables, unused_mut, unused_assignments, unused_labels, unreachable_code, unused_parens, unused_imports, non_snake_case)]\n";

pub(crate) fn emit_workspace(prog: &Program, opts: &Options, k: usize) -> crate::Result<EmitStats> {
    let out = &opts.out_dir;
    std::fs::create_dir_all(out)?;

    let main_info = crate::main_entry(prog);
    let main_info = main_info.ok_or_else(|| CodegenError::Semantic {
        class: "-".to_string(),
        message: "partitioned emission requires a main class (the link table and dispatch \
                  are installed by the generated main)"
            .to_string(),
    })?;

    // Deterministic layout: classes sorted by name, contiguous chunks of k.
    let mut sorted: Vec<&ClassPlan> = prog.classes.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let parts: Vec<Vec<&ClassPlan>> = sorted.chunks(k.max(1)).map(<[_]>::to_vec).collect();
    let part_of_module: HashMap<&str, usize> = parts
        .iter()
        .enumerate()
        .flat_map(|(i, p)| p.iter().map(move |c| (c.module.as_str(), i)))
        .collect();
    let part_of_name: HashMap<&str, usize> = parts
        .iter()
        .enumerate()
        .flat_map(|(i, p)| p.iter().map(move |c| (c.name.as_str(), i)))
        .collect();

    // Global method ids over the sorted classes, declaration order within
    // a class. Every method (including <init>/<clinit>) gets a slot: the
    // link table serves both direct cross-partition calls and the clinit
    // fn pointers passed to statics::ensure.
    let mut method_ids: HashMap<(String, String), u32> = HashMap::new();
    let mut total_methods = 0u32;
    for c in &sorted {
        for m in &c.methods {
            method_ids.insert((c.module.clone(), m.fn_name.clone()), total_methods);
            total_methods += 1;
        }
    }

    let lib_crate = crate::emit::bin_name_pub(&opts.crate_name);

    // Build manifest + reuse gate (M5 T06/T07) ------------------------------
    // Same four soundness conditions as the single-crate emitter, plus the
    // partition layout hash: the class → part-crate mapping is a pure
    // function of the name-sorted classes and k, so any membership or
    // budget change reshuffles it and the gate refuses per-class reuse
    // (conservative full re-emit — M5_PLAN §2's "conservative first").
    let mut stats = EmitStats {
        manifest_path: Manifest::path(out),
        ..EmitStats::default()
    };
    let surface = crate::manifest::surface_hash(prog, opts.emit_cfg, opts.emit_typed);
    let layout = crate::manifest::partition_layout_hash(prog, k);
    let prior = Manifest::load(out);
    stats.had_manifest = prior.is_some();
    let gate_ok = prior
        .as_ref()
        .map(|m| {
            m.emitter == EMITTER_VERSION
                && m.emit_cfg == opts.emit_cfg
                && m.emit_typed == opts.emit_typed
                && m.surface == surface
                && m.layout == layout
        })
        .unwrap_or(false);
    let mut ws = Ws {
        out,
        glue: BTreeMap::new(),
        entries: BTreeMap::new(),
        stats: &mut stats,
    };
    // class name -> recorded emitted sha, for gate-passing classes
    let mut reuse: BTreeMap<String, String> = BTreeMap::new();
    if gate_ok {
        let m = prior.as_ref().expect("gate_ok implies prior");
        for c in &prog.classes {
            let pi = part_of_name[&c.name.as_str()];
            let emitted_path = out
                .join(format!("p{pi}"))
                .join(crate::emit::class_file_rel(&c.module));
            if crate::manifest::verify_reuse(c, &emitted_path, m) {
                let entry = m.entries.get(&c.name).expect("verified entry exists");
                reuse.insert(c.name.clone(), entry.emitted_sha.clone());
            }
        }
    }

    // workspace manifest ---------------------------------------------------
    let mut wsc = String::from("# Generated by j2r — partitioned workspace (M5 T05).\n");
    let _ = writeln!(wsc, "[workspace]");
    let _ = writeln!(wsc, "members = [\"support\", \"app\",");
    for i in 0..parts.len() {
        let _ = writeln!(wsc, "  \"p{i}\",");
    }
    let _ = writeln!(wsc, "]");
    let _ = writeln!(wsc, "resolver = \"2\"");
    ws.glue_write("Cargo.toml", wsc.as_bytes())?;

    // vendored runtime, once, shared by every member ------------------------
    crate::emit::copy_runtime(out)?;
    crate::emit::record_runtime_glue_pub(out, &mut ws.glue, prior.as_ref(), ws.stats);

    emit_support(out, &lib_crate, &mut ws)?;

    for (i, part) in parts.iter().enumerate() {
        emit_part(
            prog,
            out,
            &lib_crate,
            i,
            part,
            &method_ids,
            opts,
            &mut ws,
            &reuse,
        )?;
    }

    emit_app(
        prog,
        out,
        &lib_crate,
        opts,
        &parts,
        &part_of_module,
        total_methods,
        &main_info,
        &mut ws,
    )?;

    // orphan stale part dirs (a previous run with a bigger class set or a
    // smaller k created p{i} crates this layout does not have) ------------
    for i in parts.len().. {
        let stale = out.join(format!("p{i}"));
        if stale.exists() {
            std::fs::remove_dir_all(&stale)?;
            ws.stats.orphans_removed += 1;
        } else {
            break;
        }
    }

    // refresh the manifest --------------------------------------------------
    let manifest = Manifest {
        emitter: EMITTER_VERSION,
        emit_cfg: opts.emit_cfg,
        emit_typed: opts.emit_typed,
        surface,
        layout,
        crate_name: opts
            .crate_name
            .chars()
            .map(|c| if c == '\t' || c == '\n' { '_' } else { c })
            .collect(),
        entries: ws.entries,
        glue: ws.glue,
    };
    manifest.save(out)?;
    let mut stats = std::mem::take(ws.stats);
    stats.manifest_path = Manifest::path(out);
    Ok(stats)
}

/// Workspace write context: routes every crate-level file through
/// write-if-changed, records its manifest hash, and accumulates the
/// per-class entries emit_part fills in.
struct Ws<'a> {
    out: &'a Path,
    glue: BTreeMap<String, String>,
    entries: BTreeMap<String, Entry>,
    stats: &'a mut EmitStats,
}

impl Ws<'_> {
    fn glue_write(&mut self, rel: &str, bytes: &[u8]) -> crate::Result<()> {
        if write_if_changed(&self.out.join(rel), bytes)? {
            self.stats.files_written += 1;
        } else {
            self.stats.files_unchanged += 1;
        }
        self.glue.insert(rel.to_string(), sha256_hex(bytes));
        Ok(())
    }

    fn class_write(&mut self, class: &ClassPlan, rel_path: &Path, bytes: &[u8]) {
        if write_if_changed(rel_path, bytes).unwrap_or(false) {
            self.stats.files_written += 1;
        } else {
            self.stats.files_unchanged += 1;
        }
        self.entries.insert(
            class.name.clone(),
            Entry {
                input_sha: class.input_sha.clone(),
                emitted_sha: sha256_hex(bytes),
            },
        );
        self.stats.classes_reemitted += 1;
    }
}

fn part_dep(i: usize, lib_crate: &str) -> String {
    format!("{lib_crate}-p{i}")
}

fn write_member_cargo(
    out: &Path,
    dir_name: &str,
    package: &str,
    lib_crate: &str,
    is_app: bool,
    has_bin: bool,
    ws: &mut Ws,
) -> crate::Result<()> {
    let dir = out.join(dir_name);
    std::fs::create_dir_all(dir.join("src"))?;
    let mut cargo = String::new();
    if is_app {
        let _ = writeln!(cargo, "[package]");
        let _ = writeln!(cargo, "name = \"{package}\"");
        let _ = writeln!(cargo, "version = \"0.1.0\"");
        let _ = writeln!(cargo, "edition = \"2021\"");
        let _ = writeln!(cargo);
        let _ = writeln!(cargo, "[lib]");
        let _ = writeln!(cargo, "name = \"{lib_crate}\"");
        let _ = writeln!(cargo, "path = \"src/lib.rs\"");
        let _ = writeln!(cargo);
        if has_bin {
            let bin = crate::emit::bin_name_pub(&package.replace('-', "_"));
            let _ = writeln!(cargo, "[[bin]]");
            let _ = writeln!(cargo, "name = \"{bin}\"");
            let _ = writeln!(cargo, "path = \"src/main.rs\"");
            let _ = writeln!(cargo);
        }
    } else {
        let _ = writeln!(cargo, "[package]");
        let _ = writeln!(cargo, "name = \"{package}\"");
        let _ = writeln!(cargo, "version = \"0.1.0\"");
        let _ = writeln!(cargo, "edition = \"2021\"");
        let _ = writeln!(cargo);
    }
    let _ = writeln!(cargo, "[dependencies]");
    let _ = writeln!(cargo, "j2r-runtime = {{ path = \"../runtime\" }}");
    let _ = writeln!(
        cargo,
        "support = {{ package = \"{lib_crate}-support\", path = \"../support\" }}"
    );
    ws.glue_write(&format!("{dir_name}/Cargo.toml"), cargo.as_bytes())?;
    Ok(())
}

/// The shared `support` crate: statics pool (verbatim from the
/// single-crate emitter — it is id-keyed and program-agnostic), the
/// global link table, and the dispatch slot the app installs the
/// composite Dispatch into.
fn emit_support(out: &Path, lib_crate: &str, ws: &mut Ws) -> crate::Result<()> {
    let dir = out.join("support");
    std::fs::create_dir_all(dir.join("src"))?;

    let mut cargo = String::new();
    let _ = writeln!(cargo, "[package]");
    let _ = writeln!(cargo, "name = \"{lib_crate}-support\"");
    let _ = writeln!(cargo, "version = \"0.1.0\"");
    let _ = writeln!(cargo, "edition = \"2021\"");
    let _ = writeln!(cargo);
    let _ = writeln!(cargo, "[dependencies]");
    let _ = writeln!(cargo, "j2r-runtime = {{ path = \"../runtime\" }}");
    ws.glue_write("support/Cargo.toml", cargo.as_bytes())?;

    let lib_rs = format!(
        "// Generated by j2r — partitioned support crate (M5 T05). Do not edit.\n{ALLOW}\npub mod dispatch_slot;\npub mod link;\npub mod statics;\n"
    );
    ws.glue_write("support/src/lib.rs", lib_rs.as_bytes())?;
    ws.glue_write(
        "support/src/statics.rs",
        crate::emit::statics_source().as_bytes(),
    )?;

    let link_rs = r#"// Generated by j2r — do not edit.
//! The cross-partition link table (M5_SCALE.md §3): global method ids
//! assigned deterministically over the name-sorted class list; every
//! partition registers its methods at startup (`register_link`), and
//! cross-partition call sites go through `call`. The table carries the
//! SAME fn pointers the direct paths would have called — semantics are
//! identical by construction.

static LINK: ::std::sync::OnceLock<Vec<::j2r_runtime::MethodFn>> =
    ::std::sync::OnceLock::new();

/// Installs the full table (one slot per compiled method). A `None` slot
/// is an emitter/partition bug and fails loud — never a silent miscompile.
/// `OnceLock`, not `Mutex`: cross-partition calls NEST (a callee can make
/// its own cross-partition call), and a call must never run under a lock
/// it would have to re-take. After install the table is immutable, so
/// reads are a lock-free atomic load and the fn pointer is copied out
/// before the call.
pub fn install(entries: Vec<::core::option::Option<::j2r_runtime::MethodFn>>) {
    let v: Vec<::j2r_runtime::MethodFn> = entries
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            e.unwrap_or_else(|| panic!("j2r internal: link slot {i} never registered"))
        })
        .collect();
    LINK.set(v)
        .expect("j2r internal: link table installed twice");
}

pub fn call(
    id: u32,
    recv: ::core::option::Option<&::j2r_runtime::JRef>,
    args: &[::j2r_runtime::Value],
) -> ::core::result::Result<::j2r_runtime::Value, ::j2r_runtime::Throw> {
    let v = LINK
        .get()
        .unwrap_or_else(|| panic!("j2r internal: link table not installed"));
    let f = *v
        .get(id as usize)
        .unwrap_or_else(|| panic!("j2r internal: link target {id} out of range"));
    f(recv, args)
}

/// Const-generic trampoline: `tramp::<ID>` is a concrete fn item, so the
/// emitter can pass `Some(support::link::tramp::<ID> as MethodFn)` where
/// a direct cross-crate path is impossible (clinit fn pointers).
pub fn tramp<const ID: u32>(
    recv: ::core::option::Option<&::j2r_runtime::JRef>,
    args: &[::j2r_runtime::Value],
) -> ::core::result::Result<::j2r_runtime::Value, ::j2r_runtime::Throw> {
    call(ID, recv, args)
}
"#;
    ws.glue_write("support/src/link.rs", link_rs.as_bytes())?;

    let slot_rs = r#"// Generated by j2r — do not edit.
//! The composite Dispatch slot: the app crate builds one Dispatch over
//! all partitions and installs it before any Java code runs; generated
//! code reads it back through `dispatch()` (the same value the
//! single-crate emission passes as `crate::j2r_dispatch()`).

static LOOKUP: ::std::sync::Mutex<::core::option::Option<::j2r_runtime::UserLookup>> =
    ::std::sync::Mutex::new(::core::option::Option::None);
static PARENTS: ::std::sync::Mutex<::core::option::Option<::j2r_runtime::UserParents>> =
    ::std::sync::Mutex::new(::core::option::Option::None);
static META: ::std::sync::Mutex<::core::option::Option<::j2r_runtime::UserMeta>> =
    ::std::sync::Mutex::new(::core::option::Option::None);
static SGET: ::std::sync::Mutex<::core::option::Option<fn(u32) -> ::j2r_runtime::Value>> =
    ::std::sync::Mutex::new(::core::option::Option::None);
static SSET: ::std::sync::Mutex<
    ::core::option::Option<fn(u32, &::j2r_runtime::Value)>,
> = ::std::sync::Mutex::new(::core::option::Option::None);

pub fn install(d: ::j2r_runtime::Dispatch) {
    *LOOKUP.lock().unwrap_or_else(|e| e.into_inner()) = d.lookup;
    *PARENTS.lock().unwrap_or_else(|e| e.into_inner()) = d.parents;
    *META.lock().unwrap_or_else(|e| e.into_inner()) = d.meta;
    *SGET.lock().unwrap_or_else(|e| e.into_inner()) = d.static_get;
    *SSET.lock().unwrap_or_else(|e| e.into_inner()) = d.static_set;
}

pub fn dispatch() -> ::j2r_runtime::Dispatch {
    ::j2r_runtime::Dispatch {
        lookup: *LOOKUP.lock().unwrap_or_else(|e| e.into_inner()),
        parents: *PARENTS.lock().unwrap_or_else(|e| e.into_inner()),
        meta: *META.lock().unwrap_or_else(|e| e.into_inner()),
        static_get: *SGET.lock().unwrap_or_else(|e| e.into_inner()),
        static_set: *SSET.lock().unwrap_or_else(|e| e.into_inner()),
    }
}
"#;
    ws.glue_write("support/src/dispatch_slot.rs", slot_rs.as_bytes())?;
    Ok(())
}

/// One `p{i}` crate: its class modules (rewritten for cross-partition
/// seams) plus its slice of the registry (lookup/parents/meta) and the
/// link-table registration.
#[allow(clippy::too_many_arguments)] // the workspace seam surface; bundling would hide the data flow
fn emit_part(
    prog: &Program,
    out: &Path,
    lib_crate: &str,
    i: usize,
    part: &[&ClassPlan],
    method_ids: &HashMap<(String, String), u32>,
    opts: &Options,
    ws: &mut Ws,
    reuse: &BTreeMap<String, String>,
) -> crate::Result<()> {
    let member = format!("{lib_crate}-p{i}");
    let dir = out.join(format!("p{i}"));
    let classes_dir = dir.join("src").join("classes");
    std::fs::create_dir_all(&classes_dir)?;
    write_member_cargo(out, &format!("p{i}"), &member, lib_crate, false, false, ws)?;

    let own: HashSet<&str> = part.iter().map(|c| c.module.as_str()).collect();

    // class modules, rewritten ------------------------------------------------
    // Reuse path (M5 T06/T07): a verified-current class is kept
    // byte-for-byte and its rendering skipped. The rendered text depends
    // on the class's own bytecode plus this partition's seam inputs (own
    // set + method ids — both pinned by the global gate), so gate-passing
    // makes the skip sound.
    for c in part {
        if let Some(emitted_sha) = reuse.get(&c.name) {
            ws.entries.insert(
                c.name.clone(),
                Entry {
                    input_sha: c.input_sha.clone(),
                    emitted_sha: emitted_sha.clone(),
                },
            );
            ws.stats.classes_reused += 1;
            continue;
        }
        let code = crate::emit::emit_class(prog, c, opts.emit_cfg, opts.emit_typed);
        let rewritten = rewrite_class_source(&code, &own, method_ids, &c.name)?;
        let dest = classes_dir.join(format!("{}.rs", c.module));
        ws.class_write(c, &dest, rewritten.as_bytes());
    }

    // classes/mod.rs -----------------------------------------------------------
    let mut classes_mod = String::from("// Generated by j2r — do not edit.\n");
    for c in part {
        let _ = writeln!(classes_mod, "pub mod {};", c.module);
    }
    ws.glue_write(&format!("p{i}/src/classes/mod.rs"), classes_mod.as_bytes())?;

    // lib.rs: registry slice + link registration -------------------------------
    let mut lib = format!("// Generated by j2r — do not edit.\n{ALLOW}\npub mod classes;\n\n");
    let _ = writeln!(
        lib,
        "pub fn lookup(class: &str, name: &str, desc: &str) -> ::core::option::Option<::j2r_runtime::MethodFn> {{"
    );
    let _ = writeln!(lib, "    match (class, name, desc) {{");
    for c in part {
        for m in &c.methods {
            if m.name == "<clinit>" {
                continue; // reached via statics::ensure, not dispatch
            }
            let _ = writeln!(
                lib,
                "        ({:?}, {:?}, {:?}) => ::core::option::Option::Some(crate::classes::{}::{} as ::j2r_runtime::MethodFn),",
                c.name, m.name, m.desc, c.module, m.fn_name
            );
        }
    }
    let _ = writeln!(lib, "        _ => ::core::option::Option::None,");
    let _ = writeln!(lib, "    }}");
    let _ = writeln!(lib, "}}");
    let _ = writeln!(lib);

    let _ = writeln!(
        lib,
        "pub fn parents_of(class: &str) -> ::core::option::Option<&'static [&'static str]> {{"
    );
    let _ = writeln!(lib, "    match class {{");
    for c in part {
        let mut parents: Vec<&str> = Vec::new();
        parents.push(c.super_name.as_deref().unwrap_or("java/lang/Object"));
        for itf in &c.interfaces {
            parents.push(itf.as_str());
        }
        let list = parents
            .iter()
            .map(|p| format!("{p:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            lib,
            "        {:?} => ::core::option::Option::Some(&[{}]),",
            c.name, list
        );
    }
    let _ = writeln!(lib, "        _ => ::core::option::Option::None,");
    let _ = writeln!(lib, "    }}");
    let _ = writeln!(lib, "}}");
    let _ = writeln!(lib);

    // reflection-subset meta tables (same shape as the single-crate registry)
    for c in part {
        let tag = upper_snake(&c.module);
        let _ = writeln!(
            lib,
            "static FIELDS_{tag}: [::j2r_runtime::UserField; {}] = [",
            c.fields.len()
        );
        for f in &c.fields {
            let _ = writeln!(
                lib,
                "    ::j2r_runtime::UserField {{ name: {:?}, desc: {:?}, is_static: {}, instance_slot: {}, static_slot: {} }},",
                f.name,
                f.desc,
                f.is_static,
                f.instance_slot.unwrap_or(0),
                f.static_slot.unwrap_or(0)
            );
        }
        let _ = writeln!(lib, "];");
        let declared_methods: Vec<_> = c
            .methods
            .iter()
            .filter(|m| m.name != "<init>" && m.name != "<clinit>")
            .collect();
        let _ = writeln!(
            lib,
            "static METHODS_{tag}: [::j2r_runtime::UserMethod; {}] = [",
            declared_methods.len()
        );
        for m in &declared_methods {
            let _ = writeln!(
                lib,
                "    ::j2r_runtime::UserMethod {{ name: {:?}, desc: {:?}, is_static: {}, is_native: {} }},",
                m.name, m.desc, m.is_static, m.is_native
            );
        }
        let _ = writeln!(lib, "];");
        let simple = match &c.simple_name {
            Some(s) => format!("::core::option::Option::Some({s:?})"),
            None => "::core::option::Option::None".to_string(),
        };
        let _ = writeln!(
            lib,
            "static META_{tag}: ::j2r_runtime::UserClassInfo = ::j2r_runtime::UserClassInfo {{ is_interface: {}, simple_name: {}, fields: &FIELDS_{tag}, methods: &METHODS_{tag} }};",
            c.is_interface, simple
        );
    }
    let _ = writeln!(
        lib,
        "pub fn meta_of(class: &str) -> ::core::option::Option<&'static ::j2r_runtime::UserClassInfo> {{"
    );
    let _ = writeln!(lib, "    match class {{");
    for c in part {
        let tag = upper_snake(&c.module);
        let _ = writeln!(
            lib,
            "        {:?} => ::core::option::Option::Some(&META_{tag}),",
            c.name
        );
    }
    let _ = writeln!(lib, "        _ => ::core::option::Option::None,");
    let _ = writeln!(lib, "    }}");
    let _ = writeln!(lib, "}}");
    let _ = writeln!(lib);

    let _ = writeln!(
        lib,
        "pub fn register_link(out: &mut Vec<::core::option::Option<::j2r_runtime::MethodFn>>) {{"
    );
    for c in part {
        for m in &c.methods {
            let gid = method_ids[&(c.module.clone(), m.fn_name.clone())];
            let _ = writeln!(
                lib,
                "    out[{}usize] = ::core::option::Option::Some(crate::classes::{}::{} as ::j2r_runtime::MethodFn);",
                gid, c.module, m.fn_name
            );
        }
    }
    let _ = writeln!(lib, "}}");
    // G2.1 (console.md F3 / REVIEW_CHECKLIST G2.1): the partition's
    // defaults/constants tables live HERE — bounded by k — and the app
    // composes them at startup; app/src/main.rs carries one line per
    // partition, never a total-class-scaled literal.
    let _ = writeln!(lib);
    lib.push_str(&crate::emit::static_defaults_register_fn(part));
    lib.push_str(&crate::emit::static_constants_register_fn(part));
    ws.glue_write(&format!("p{i}/src/lib.rs"), lib.as_bytes())?;
    Ok(())
}

/// The `app` crate: main.rs (init + install sequence), the composite
/// Dispatch, and the link-table assembly over all partitions.
#[allow(clippy::too_many_arguments)]
fn emit_app(
    prog: &Program,
    out: &Path,
    lib_crate: &str,
    opts: &Options,
    parts: &[Vec<&ClassPlan>],
    part_of_module: &HashMap<&str, usize>,
    total_methods: u32,
    main_info: &crate::MainInfo,
    ws: &mut Ws,
) -> crate::Result<()> {
    let member = opts.crate_name.clone();
    // dirs only — the full app Cargo.toml (with the p{i} dependency block)
    // is written below; going through write_member_cargo here would write a
    // short placeholder first and rewrite it, breaking mtime stability
    std::fs::create_dir_all(out.join("app").join("src"))?;

    // Cargo.toml: the app package keeps the user's crate name so the bin
    // name matches the single-crate layout; parts are aliased p0..pN.
    let mut cargo = String::new();
    let _ = writeln!(cargo, "[package]");
    let _ = writeln!(cargo, "name = \"{member}\"");
    let _ = writeln!(cargo, "version = \"0.1.0\"");
    let _ = writeln!(cargo, "edition = \"2021\"");
    let _ = writeln!(cargo);
    let _ = writeln!(cargo, "[lib]");
    let _ = writeln!(cargo, "name = \"{lib_crate}\"");
    let _ = writeln!(cargo, "path = \"src/lib.rs\"");
    let _ = writeln!(cargo);
    let bin = crate::emit::bin_name_pub(&member.replace('-', "_"));
    let _ = writeln!(cargo, "[[bin]]");
    let _ = writeln!(cargo, "name = \"{bin}\"");
    let _ = writeln!(cargo, "path = \"src/main.rs\"");
    let _ = writeln!(cargo);
    let _ = writeln!(cargo, "[dependencies]");
    let _ = writeln!(cargo, "j2r-runtime = {{ path = \"../runtime\" }}");
    let _ = writeln!(
        cargo,
        "support = {{ package = \"{lib_crate}-support\", path = \"../support\" }}"
    );
    for i in 0..parts.len() {
        let _ = writeln!(
            cargo,
            "p{i} = {{ package = \"{}\", path = \"../p{i}\" }}",
            part_dep(i, lib_crate)
        );
    }
    ws.glue_write("app/Cargo.toml", cargo.as_bytes())?;

    // lib.rs: composite dispatch ------------------------------------------------
    let starts: Vec<&str> = parts.iter().map(|p| p[0].name.as_str()).collect();
    let mut lib = format!("// Generated by j2r — do not edit.\n{ALLOW}\n");
    let _ = writeln!(lib, "static PART_START: [&str; {}] = [", parts.len());
    for s in &starts {
        let _ = writeln!(lib, "    {s:?},");
    }
    let _ = writeln!(lib, "];");
    let _ = writeln!(lib);

    let _ = writeln!(
        lib,
        "fn part_of(class: &str) -> ::core::option::Option<usize> {{"
    );
    let _ = writeln!(
        lib,
        "    if class < PART_START[0] {{ return ::core::option::Option::None; }}"
    );
    let _ = writeln!(
        lib,
        "    ::core::option::Option::Some(PART_START.partition_point(|&s| s <= class) - 1)"
    );
    let _ = writeln!(lib, "}}");
    let _ = writeln!(lib);

    // G2.1 (console.md F3): per-partition route tables + the part_of
    // binary search — the app crate carries one route token per
    // partition in STATIC tables: no per-partition match arms, and no
    // total-class-scaled literal anywhere in the app bin.
    for (fname, params, args, ret) in [
        (
            "lookup",
            "class: &str, name: &str, desc: &str",
            "class, name, desc",
            "::core::option::Option<::j2r_runtime::MethodFn>",
        ),
        (
            "parents_of",
            "class: &str",
            "class",
            "::core::option::Option<&'static [&'static str]>",
        ),
        (
            "meta_of",
            "class: &str",
            "class",
            "::core::option::Option<&'static ::j2r_runtime::UserClassInfo>",
        ),
    ] {
        let table = format!("ROUTE_{}", fname.to_ascii_uppercase());
        let _ = writeln!(
            lib,
            "static {table}: [fn({params}) -> {ret}; {}] = [",
            parts.len()
        );
        for i in 0..parts.len() {
            let _ = writeln!(lib, "    p{i}::{fname},");
        }
        let _ = writeln!(lib, "];");
        let _ = writeln!(lib);
        let _ = writeln!(lib, "pub fn {fname}({params}) -> {ret} {{");
        let _ = writeln!(lib, "    match part_of(class) {{");
        let _ = writeln!(
            lib,
            "        ::core::option::Option::Some(i) => {table}[i]({args}),"
        );
        let _ = writeln!(
            lib,
            "        ::core::option::Option::None => ::core::option::Option::None,"
        );
        let _ = writeln!(lib, "    }}");
        let _ = writeln!(lib, "}}");
        let _ = writeln!(lib);
    }

    let _ = writeln!(lib, "pub fn j2r_dispatch() -> ::j2r_runtime::Dispatch {{");
    let _ = writeln!(lib, "    ::j2r_runtime::Dispatch {{");
    let _ = writeln!(lib, "        lookup: ::core::option::Option::Some(lookup),");
    let _ = writeln!(
        lib,
        "        parents: ::core::option::Option::Some(parents_of),"
    );
    let _ = writeln!(lib, "        meta: ::core::option::Option::Some(meta_of),");
    let _ = writeln!(
        lib,
        "        static_get: ::core::option::Option::Some(support::statics::pool_get_value),"
    );
    let _ = writeln!(
        lib,
        "        static_set: ::core::option::Option::Some(support::statics::pool_set_value),"
    );
    let _ = writeln!(lib, "    }}");
    let _ = writeln!(lib, "}}");
    let _ = writeln!(lib);

    let _ = writeln!(
        lib,
        "pub fn build_link() -> Vec<::core::option::Option<::j2r_runtime::MethodFn>> {{"
    );
    let _ = writeln!(
        lib,
        "    let mut v: Vec<::core::option::Option<::j2r_runtime::MethodFn>> = ::std::vec![::core::option::Option::None; {}usize];",
        total_methods
    );
    for i in 0..parts.len() {
        let _ = writeln!(lib, "    p{i}::register_link(&mut v);");
    }
    let _ = writeln!(lib, "    v");
    let _ = writeln!(lib, "}}");
    ws.glue_write("app/src/lib.rs", lib.as_bytes())?;

    // main.rs: the single-crate template with support/part paths + installs
    let m = main_info;
    let main_part = part_of_module
        .get(m.module.as_str())
        .copied()
        .expect("main class in some partition");
    // G2.1 (console.md F3): one composition line per partition — the
    // per-partition register fns carry the tables (bounded by k); the old
    // total-class literals here made main_run the 20k peak-rustc driver.
    let defaults_regs: String = (0..parts.len())
        .map(|i| format!("p{i}::register_defaults(&mut defaults);"))
        .collect::<Vec<_>>()
        .join("\n    ");
    let constants_regs: String = (0..parts.len())
        .map(|i| format!("p{i}::register_constants(&mut constants);"))
        .collect::<Vec<_>>()
        .join("\n    ");
    let main_rs = format!(
        r#"// Generated by j2r — do not edit.
#![allow(dead_code, unused_variables, unused_mut, unused_assignments, unused_labels, unreachable_code, non_snake_case)]

fn main() {{
    // J2R_MAX_JAVA_FRAMES is an operator knob, not a Java-observable one:
    // an invalid value is rejected (exit 2) before any Java code runs
    // (M2_PLAN §3.1).
    ::j2r_runtime::frame::validate_env_or_exit();
    // A large stack: Java programs recurse deeply (the JVM grows its thread
    // stack automatically; we pre-allocate instead).
    let child = ::std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(main_run)
        .expect("j2r: failed to spawn main thread");
    match child.join() {{
        ::core::result::Result::Ok(code) => ::std::process::exit(code),
        ::core::result::Result::Err(_) => ::std::process::exit(101),
    }}
}}

fn main_run() -> i32 {{
    // M4 Wave 2: register this worker as the main Java thread BEFORE any
    // Java allocation — the thread registry is the rooting story (T07).
    ::j2r_runtime::thread::begin_main_java_thread();
    // M5 T05: install the link table + composite dispatch BEFORE any Java
    // code runs — the cross-partition seams are dead without them.
    support::link::install({lib_crate}::build_link());
    support::dispatch_slot::install({lib_crate}::j2r_dispatch());
    // T10 slice 3 (docs/M5_JNI_CLASSES.md §1): the JNI surface reads the
    // COMPOSED dispatch through the runtime's process-global host slot —
    // installed here, after the composite exists, before any Java code.
    ::j2r_runtime::jni::install_host_dispatch(support::dispatch_slot::dispatch());
    support::statics::init_pool({statics}usize, {classes}usize);
    let mut defaults: ::std::vec::Vec<(u32, u32, support::statics::Df)> =
        ::std::vec::Vec::new();
    {defaults_regs}
    support::statics::apply_defaults(&defaults);
    let mut constants: ::std::vec::Vec<(u32, ::j2r_runtime::Value)> =
        ::std::vec::Vec::new();
    {constants_regs}
    support::statics::set_constants(&constants);
    // Static fields are roots for the tracing GC (M2_PLAN §4.4).
    ::j2r_runtime::heap::set_statics_scanner(support::statics::gc_scan_roots);
    let args: ::std::vec::Vec<::std::string::String> = ::std::env::args().skip(1).collect();
    let args_array = ::j2r_runtime::object::new_string_array(&args);
    let code = match p{main_part}::classes::{module}::{main_fn}(
        ::core::option::Option::None,
        &[::j2r_runtime::Value::Ref(::core::option::Option::Some(args_array))],
    ) {{
        ::core::result::Result::Ok(_) => 0,
        ::core::result::Result::Err(t) => {{
            eprintln!(
                "Exception in thread \"main\" {{}}",
                ::j2r_runtime::throwable_display(&t, {lib_crate}::j2r_dispatch())
            );
            // Audit A2: the enclosed trace sections (Suppressed:/Caused by:)
            // ride behind the header, matching the JVM's printStackTrace
            // order; frame lines are not rendered (no LineNumberTable yet)
            // and the harness normalizes them away on the JVM side.
            let trace =
                ::j2r_runtime::throwable_enclosed_trace(&t, {lib_crate}::j2r_dispatch());
            if !trace.is_empty() {{
                eprint!("{{trace}}");
            }}
            1
        }}
    }};
    // JVM shutdown rule (M4 Wave 2): the process waits for non-daemon
    // threads before the exit code is produced (T15 owns edge cases).
    ::j2r_runtime::thread::await_non_daemons();
    code
}}
"#,
        statics = prog.total_statics,
        classes = prog.classes.len(),
        defaults_regs = defaults_regs,
        constants_regs = constants_regs,
        module = m.module,
        main_fn = m.fn_name,
    );
    ws.glue_write("app/src/main.rs", main_rs.as_bytes())?;
    Ok(())
}

/// SCREAMING_SNAKE form of a sanitized module name (same rule as the
/// single-crate registry's statics).
fn upper_snake(module: &str) -> String {
    module
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else if c.is_ascii_alphanumeric() {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Rewrites one emitted class module for its partition:
/// - same-partition `crate::classes::X::f` paths stay verbatim;
/// - cross-partition calls become `support::link::call(GID, …)` and
///   cross-partition fn-pointer casts become `support::link::tramp::<GID>`;
/// - `crate::statics::` → `support::statics::` (one shared pool);
/// - `crate::j2r_dispatch()` → `support::dispatch()` (the installed
///   composite).
fn rewrite_class_source(
    code: &str,
    own: &HashSet<&str>,
    method_ids: &HashMap<(String, String), u32>,
    class_name: &str,
) -> crate::Result<String> {
    let err = |msg: String| CodegenError::Semantic {
        class: class_name.to_string(),
        message: msg,
    };
    let mut out = String::with_capacity(code.len() + 64);
    let mut rest = code;
    while let Some(pos) = rest.find("crate::") {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        if let Some(tail) = rest.strip_prefix("crate::statics::") {
            out.push_str("support::statics::");
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("crate::j2r_dispatch()") {
            out.push_str("support::dispatch_slot::dispatch()");
            rest = tail;
        } else if let Some(after) = rest.strip_prefix("crate::classes::") {
            let mod_end = after
                .find("::")
                .ok_or_else(|| err("unparsable class path in generated module".into()))?;
            let module = &after[..mod_end];
            let after_fn = &after[mod_end + 2..];
            let fn_end = after_fn
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .ok_or_else(|| err("unparsable fn name in generated module".into()))?;
            let fn_name = &after_fn[..fn_end];
            let consumed = "crate::classes::".len() + mod_end + 2 + fn_end;
            if own.contains(module) {
                out.push_str(&rest[..consumed]);
                rest = &rest[consumed..];
                continue;
            }
            let gid = method_ids
                .get(&(module.to_string(), fn_name.to_string()))
                .copied()
                .ok_or_else(|| {
                    err(format!(
                        "no global link id for cross-partition target {module}::{fn_name}"
                    ))
                })?;
            let next = &rest[consumed..];
            let next_trim = next.trim_start();
            if next_trim.starts_with('(') {
                // call form: consume the '(' — link_call's own arg list takes over
                out.push_str(&format!("support::link::call({gid}u32, "));
                rest = &next[1..];
            } else if next_trim.starts_with("as") {
                out.push_str(&format!("support::link::tramp::<{gid}u32>"));
                rest = next;
            } else {
                return Err(err(format!(
                    "unrecognized cross-partition reference to {module}::{fn_name} — extend the partition rewriter"
                )));
            }
        } else {
            out.push_str("crate::");
            rest = &rest["crate::".len()..];
        }
    }
    out.push_str(rest);
    Ok(out)
}
