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

use crate::manifest::{EmitStats, Manifest, EMITTER_VERSION};
use crate::{CodegenError, Options, Program};
use std::collections::BTreeMap;
use std::fmt::Write as _;

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

    // Deterministic layout: classes sorted by name, contiguous chunks of k
    // (the helper below carries the section verbatim).
    let (parts, part_of_module, part_of_name, method_ids, total_methods) = layout_parts(prog, k);

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
    // class name -> recorded emitted sha, for gate-passing classes.
    let _sp_gate = crate::StageGuard::enter("emit_gate_reuse");
    let reuse = if gate_ok {
        scan_reuse(
            prog,
            out,
            &part_of_name,
            prior.as_ref().expect("gate_ok implies prior"),
        )
    } else {
        BTreeMap::new()
    };
    drop(_sp_gate);

    // workspace manifest + vendored runtime + support crate (helper below).
    workspace_glue(out, &lib_crate, &parts, prior.as_ref(), &mut ws)?;

    let _sp_render = crate::StageGuard::enter("emit_render_write");
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

    // refresh the manifest (helper below).
    drop(_sp_render);
    let _sp_save = crate::StageGuard::enter("emit_manifest_save");
    let mut stats = finish_manifest(opts, out, surface, layout, ws.entries, ws.glue, ws.stats)?;
    drop(_sp_save);
    stats.manifest_path = Manifest::path(out);
    Ok(stats)
}

// --- round-7 file-split pass: verbatim children below ---
mod workspace;
pub(crate) use workspace::*;
mod member;
pub(crate) use member::*;
mod support;
pub(crate) use support::*;
mod part;
pub(crate) use part::*;
mod app;
pub(crate) use app::*;
