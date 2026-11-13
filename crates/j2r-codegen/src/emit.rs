//! Rust source emission.
//!
//! Strategy (milestone 1): every Java method becomes a Rust function with an
//! explicit operand stack and a `pc`-dispatch loop — a per-method threaded
//! interpreter with typed locals and a typed operand stack, compiled
//! natively. This is correct for arbitrary (even irreducible) control flow;
//! lifting basic blocks into structured Rust control flow is a planned
//! optimization pass. Exceptions use the method's real exception table via
//! the [`j2r_runtime::j2r_try!`] macro.
//!
//! Layout of the generated crate:
//! ```text
//! out/
//!   Cargo.toml          (detached workspace, depends on ./runtime)
//!   runtime/            (vendored copy of j2r-runtime)
//!   src/lib.rs          (modules + Dispatch)
//!   src/main.rs         (only when a main class exists)
//!   src/registry.rs     (method + hierarchy tables for dispatch)
//!   src/statics.rs      (static field pool + class-init flags)
//!   src/classes/*.rs    (one module per class)
//! ```

use crate::ir::{ArithOp, CmpKind, Conv, Instr, Ldc, Ldc2, NumKind};
use crate::manifest::{write_if_changed, EmitStats, Entry, Manifest, EMITTER_VERSION};
use crate::sha256::sha256_hex;
use crate::{CallTarget, ClassPlan, FieldPlan, MainInfo, MethodPlan, Options, Program};
use j2r_classfile::ConstantValue;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

const RT: &str = "::j2r_runtime";

pub fn emit_crate(prog: &Program, opts: &Options) -> crate::Result<EmitStats> {
    // M5 T05 (docs/M5_SCALE.md §3): partitioned workspace emission when the
    // class count exceeds the per-crate budget. The single-crate path below
    // is byte-identical to the pre-M5 emitter.
    if let Some(k) = opts.partition {
        if prog.classes.len() > k {
            return crate::partition::emit_workspace(prog, opts, k);
        }
    }
    let out = &opts.out_dir;
    std::fs::create_dir_all(out)?;
    let src = out.join("src");
    std::fs::create_dir_all(src.join("classes"))?;

    let lib_crate = sanitize_crate_ident(&opts.crate_name);
    let has_main = prog.main_class.is_some();
    let main_info: Option<MainInfo> = crate::main_entry(prog);

    // Build manifest + reuse gate (M5 T06/T07) ------------------------------
    // A class is reused (its emitted file kept byte-for-byte, its rendering
    // skipped) only when the four soundness conditions in manifest.rs hold:
    // the global gate (emitter version, emit_cfg, emit_typed, signature
    // surface, partition layout) matches the recorded one, the class's own
    // input hash matches, and the file on disk hashes to exactly the
    // recorded emitted hash. Any doubt falls back to rendering. Every file
    // is written only when its bytes change — untouched mtimes are what
    // keep a following `cargo build` a true no-op.
    let mut stats = EmitStats {
        manifest_path: Manifest::path(out),
        ..EmitStats::default()
    };
    let mut glue: BTreeMap<String, String> = BTreeMap::new();
    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    let surface = crate::manifest::surface_hash(prog, opts.emit_cfg, opts.emit_typed);
    let prior = Manifest::load(out);
    stats.had_manifest = prior.is_some();
    let _sp_gate = crate::StageGuard::enter("emit_gate_reuse");
    let gate_ok = prior
        .as_ref()
        .map(|m| {
            m.emitter == EMITTER_VERSION
                && m.emit_cfg == opts.emit_cfg
                && m.emit_typed == opts.emit_typed
                && m.surface == surface
                && m.layout == "-"
        })
        .unwrap_or(false);
    // class -> recorded emitted sha, for gate-passing classes
    let mut reuse: BTreeMap<String, String> = BTreeMap::new();
    if gate_ok {
        let m = prior.as_ref().expect("gate_ok implies prior");
        for c in &prog.classes {
            let emitted_path = out.join(class_file_rel(&c.module));
            if crate::manifest::verify_reuse(c, &emitted_path, m) {
                let entry = m.entries.get(&c.name).expect("verified entry exists");
                reuse.insert(c.name.clone(), entry.emitted_sha.clone());
            }
        }
    }

    // vendored runtime ------------------------------------------------------
    drop(_sp_gate);
    let _sp_glue = crate::StageGuard::enter("emit_runtime_glue");
    copy_runtime(out)?;
    record_runtime_glue_pub(out, &mut glue, prior.as_ref(), &mut stats);
    drop(_sp_glue);

    // Cargo.toml -----------------------------------------------------------
    let mut cargo = String::new();
    let _ = writeln!(cargo, "[package]");
    let _ = writeln!(cargo, "name = \"{}\"", opts.crate_name);
    let _ = writeln!(cargo, "version = \"0.1.0\"");
    let _ = writeln!(cargo, "edition = \"2021\"");
    let _ = writeln!(cargo);
    let _ = writeln!(cargo, "[lib]");
    let _ = writeln!(cargo, "name = \"{lib_crate}\"");
    let _ = writeln!(cargo, "path = \"src/lib.rs\"");
    let _ = writeln!(cargo);
    if has_main {
        let bin = bin_name(&opts.crate_name);
        let _ = writeln!(cargo, "[[bin]]");
        let _ = writeln!(cargo, "name = \"{bin}\"");
        let _ = writeln!(cargo, "path = \"src/main.rs\"");
        let _ = writeln!(cargo);
    }
    let _ = writeln!(cargo, "[dependencies]");
    let _ = writeln!(cargo, "j2r-runtime = {{ path = \"runtime\" }}");
    let _ = writeln!(cargo);
    let _ = writeln!(cargo, "# detach from any enclosing cargo workspace");
    let _ = writeln!(cargo, "[workspace]");
    emit_glue(out, "Cargo.toml", cargo.as_bytes(), &mut glue, &mut stats)?;

    // lib.rs ----------------------------------------------------------------
    let lib_rs = r#"// Generated by j2r — do not edit.
#![allow(dead_code, unused_variables, unused_mut, unused_assignments, unused_labels, unreachable_code, unused_parens, unused_imports, non_snake_case)]

pub mod classes;
pub mod jmm_boundary;
pub mod registry;
pub mod statics;

/// The generated crate's symbol tables, handed to the runtime for dynamic
/// dispatch, subtype checks and exception matching.
pub fn j2r_dispatch() -> ::j2r_runtime::Dispatch {
    ::j2r_runtime::Dispatch {
        lookup: ::core::option::Option::Some(registry::lookup),
        parents: ::core::option::Option::Some(registry::parents_of),
        meta: ::core::option::Option::Some(registry::meta_of),
        static_get: ::core::option::Option::Some(statics::pool_get_value),
        static_set: ::core::option::Option::Some(statics::pool_set_value),
    }
}
"#;
    emit_glue(out, "src/lib.rs", lib_rs.as_bytes(), &mut glue, &mut stats)?;

    // registry.rs -------------------------------------------------------------
    let mut registry = String::new();
    let _ = writeln!(registry, "// Generated by j2r — do not edit.");
    let _ = writeln!(
        registry,
        "pub fn lookup(class: &str, name: &str, desc: &str) -> ::core::option::Option<::j2r_runtime::MethodFn> {{"
    );
    let _ = writeln!(registry, "    match (class, name, desc) {{");
    for c in &prog.classes {
        for m in &c.methods {
            if m.name == "<clinit>" {
                continue; // reached via statics::ensure, not dispatch
            }
            let _ = writeln!(
                registry,
                "        ({:?}, {:?}, {:?}) => ::core::option::Option::Some(crate::classes::{}::{} as ::j2r_runtime::MethodFn),",
                c.name, m.name, m.desc, c.module, m.fn_name
            );
        }
    }
    let _ = writeln!(registry, "        _ => ::core::option::Option::None,");
    let _ = writeln!(registry, "    }}");
    let _ = writeln!(registry, "}}");
    let _ = writeln!(registry);
    let _ = writeln!(
        registry,
        "pub fn parents_of(class: &str) -> ::core::option::Option<&'static [&'static str]> {{"
    );
    let _ = writeln!(registry, "    match class {{");
    for c in &prog.classes {
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
            registry,
            "        {:?} => ::core::option::Option::Some(&[{}]),",
            c.name, list
        );
    }
    let _ = writeln!(registry, "        _ => ::core::option::Option::None,");
    let _ = writeln!(registry, "    }}");
    let _ = writeln!(registry, "}}");
    let _ = writeln!(registry);
    // Reflection-subset class facts (M3_PLAN §8.1): per-class declared
    // fields/methods plus interface flag and InnerClasses simple name. The
    // runtime's `Class.getDeclaredField/getDeclaredMethod` reads these —
    // reflection only ever sees what the compiler proved exists. Static
    // names are SCREAMING_SNAKE (module names are CamelCase, and Rust lints
    // generated-crate non-upper-case globals).
    for c in &prog.classes {
        let tag = upper_snake(&c.module);
        let _ = writeln!(
            registry,
            "static FIELDS_{tag}: [::j2r_runtime::UserField; {}] = [",
            c.fields.len()
        );
        for f in &c.fields {
            // G4.2 arm (b): boundary fields are marked at every site the
            // generated crate exposes, including the reflection table.
            if let Some(marker) = jmm_boundary_marker(f, &c.name) {
                let _ = writeln!(registry, "    {marker}");
            }
            let _ = writeln!(
                registry,
                "    ::j2r_runtime::UserField {{ name: {:?}, desc: {:?}, is_static: {}, instance_slot: {}, static_slot: {}, modifiers: {} }},",
                f.name,
                f.desc,
                f.is_static,
                f.instance_slot.unwrap_or(0),
                f.static_slot.unwrap_or(0),
                f.modifiers
            );
        }
        let _ = writeln!(registry, "];");
        let declared_methods: Vec<_> = c
            .methods
            .iter()
            .filter(|m| m.name != "<init>" && m.name != "<clinit>")
            .collect();
        let _ = writeln!(
            registry,
            "static METHODS_{tag}: [::j2r_runtime::UserMethod; {}] = [",
            declared_methods.len()
        );
        for m in &declared_methods {
            let _ = writeln!(
                registry,
                "    ::j2r_runtime::UserMethod {{ name: {:?}, desc: {:?}, is_static: {}, is_native: {}, modifiers: {} }},",
                m.name, m.desc, m.is_static, m.is_native, m.modifiers
            );
        }
        let _ = writeln!(registry, "];");
        let simple = match &c.simple_name {
            Some(s) => format!("::core::option::Option::Some({s:?})"),
            None => "::core::option::Option::None".to_string(),
        };
        let _ = writeln!(
            registry,
            "static META_{tag}: ::j2r_runtime::UserClassInfo = ::j2r_runtime::UserClassInfo {{ is_interface: {}, simple_name: {}, fields: &FIELDS_{tag}, methods: &METHODS_{tag} }};",
            c.is_interface, simple
        );
    }
    let _ = writeln!(
        registry,
        "pub fn meta_of(class: &str) -> ::core::option::Option<&'static ::j2r_runtime::UserClassInfo> {{"
    );
    let _ = writeln!(registry, "    match class {{");
    for c in &prog.classes {
        let tag = upper_snake(&c.module);
        let _ = writeln!(
            registry,
            "        {:?} => ::core::option::Option::Some(&META_{tag}),",
            c.name
        );
    }
    let _ = writeln!(registry, "        _ => ::core::option::Option::None,");
    let _ = writeln!(registry, "    }}");
    let _ = writeln!(registry, "}}");
    emit_glue(
        out,
        "src/registry.rs",
        registry.as_bytes(),
        &mut glue,
        &mut stats,
    )?;
    // G4.2 arm (b): the machine-readable boundary sidecar. Deterministic
    // content; lists EVERY field whose input bytecode flags promise
    // semantics the runtime does not provide (volatile ordering, final
    // freeze). CI gate-23 (scripts/check_jmm_boundary.py) re-derives the
    // expected set from the INPUT .class files and fails loudly when this
    // list diverges from it or when an access site lacks its marker.
    let mut jmm = String::new();
    jmm.push_str(
        "// J2R-JMM-BOUNDARY sidecar (docs/RUNTIME_SAFETY.md §6): the fields\n// below are volatile / final (non-constant) in the INPUT bytecode and\n// are compiled as PLAIN non-atomic slot accesses — no Java volatile\n// ordering, no final-field freeze. Checked by CI gate-23; the marker\n// comments sit at every access site in the generated sources.\n",
    );
    jmm.push_str("pub const BOUNDARY_FIELDS: &[(&str, &str, &str)] = &[\n");
    let mut boundary_rows = 0usize;
    for c in &prog.classes {
        for f in &c.fields {
            if let Some(marker) = jmm_boundary_marker(f, &c.name) {
                let kind = if f.is_volatile { "volatile" } else { "final" };
                let _ = writeln!(
                    jmm,
                    "    {marker}\n    ({:?}, {:?}, {kind:?}),",
                    c.name, f.name
                );
                boundary_rows += 1;
            }
        }
    }
    jmm.push_str(
        "];
",
    );
    let _ = writeln!(
        jmm,
        "// {} boundary field(s) at this compile.",
        boundary_rows
    );
    emit_glue(
        out,
        "src/jmm_boundary.rs",
        jmm.as_bytes(),
        &mut glue,
        &mut stats,
    )?;

    // statics.rs ----------------------------------------------------------------
    emit_glue(
        out,
        "src/statics.rs",
        statics_source().as_bytes(),
        &mut glue,
        &mut stats,
    )?;

    // classes/mod.rs ------------------------------------------------------------
    let mut classes_mod = String::from("// Generated by j2r — do not edit.\n");
    for c in &prog.classes {
        let _ = writeln!(classes_mod, "pub mod {};", c.module);
    }
    emit_glue(
        out,
        "src/classes/mod.rs",
        classes_mod.as_bytes(),
        &mut glue,
        &mut stats,
    )?;

    // per-class modules -----------------------------------------------------------
    // Reuse path (M5 T06/T07): a verified-current class is kept
    // byte-for-byte and its rendering skipped; the manifest record is
    // refreshed with the hashes that were just verified.
    let _sp_render = crate::StageGuard::enter("emit_render_write");
    for c in &prog.classes {
        if let Some(emitted_sha) = reuse.get(&c.name) {
            let emitted_path = out.join(class_file_rel(&c.module));
            entries.insert(
                c.name.clone(),
                Entry {
                    input_sha: c.input_sha.clone(),
                    emitted_sha: emitted_sha.clone(),
                    emitted_stat: crate::manifest::stat_of(&emitted_path),
                },
            );
            stats.classes_reused += 1;
            continue;
        }
        let code = emit_class(prog, c, opts.emit_cfg, opts.emit_typed);
        let bytes = code.as_bytes();
        let dest = out.join(class_file_rel(&c.module));
        if write_if_changed(&dest, bytes)? {
            stats.files_written += 1;
        } else {
            stats.files_unchanged += 1;
        }
        entries.insert(
            c.name.clone(),
            Entry {
                input_sha: c.input_sha.clone(),
                emitted_sha: sha256_hex(bytes),
                emitted_stat: crate::manifest::stat_of(&dest),
            },
        );
        stats.classes_reemitted += 1;
    }
    drop(_sp_render);

    // main.rs -----------------------------------------------------------------
    if let (true, Some(m)) = (has_main, &main_info) {
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
    // T13 13d: the main thread is attached to the VM at birth (HotSpot
    // semantics, MEASURED by the 13d probe: GetEnv from main answers
    // JNI_OK; a fresh foreign pthread answers JNI_EDETACHED until IT
    // attaches — docs/M5_JNI_HARNESS.md §9).
    ::j2r_runtime::jni::attach::mark_attached();
    {lib_crate}::statics::init_pool({statics}usize, {classes}usize);
    {clinit_names}
    {lib_crate}::statics::apply_defaults({defaults});
    {lib_crate}::statics::set_constants({constants});
    // T10 slice 3 (docs/M5_JNI_CLASSES.md §1): the JNI env-table slots
    // have no context parameter, so the program's dispatch is installed
    // as the process-global host dispatch — before any Java code runs.
    ::j2r_runtime::jni::install_host_dispatch({lib_crate}::j2r_dispatch());
    // Static fields are roots for the tracing GC (M2_PLAN §4.4).
    ::j2r_runtime::heap::set_statics_scanner({lib_crate}::statics::gc_scan_roots);
    let args: ::std::vec::Vec<::std::string::String> = ::std::env::args().skip(1).collect();
    let args_array = ::j2r_runtime::object::new_string_array(&args);
    // JLS 12.4.2: the JVM launcher initializes the main class BEFORE
    // invoking main (EXTERNAL-B wave finding, 2026-11-13: a main() that
    // touches none of its own class's statics otherwise runs without the
    // `<clinit>`). No-op when the class has no `<clinit>`. CLINIT-THROW:
    // a throwing clinit surfaces as Err — the SAME uncaught path as a
    // main() that throws (Temurin: "Exception in thread \"main\" java.lang
    // .ExceptionInInitializerError").
    let uncaught = |t: ::j2r_runtime::Throw| -> i32 {{
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
    }};
    let code = match {main_clinit_ensure} {{
        ::core::result::Result::Err(t) => uncaught(t),
        ::core::result::Result::Ok(()) => match {lib_crate}::classes::{module}::{main_fn}(
            ::core::option::Option::None,
            &[::j2r_runtime::Value::Ref(::core::option::Option::Some(args_array))],
        ) {{
            ::core::result::Result::Ok(_) => 0,
            ::core::result::Result::Err(t) => uncaught(t),
        }},
    }};
    // JVM shutdown rule (M4 Wave 2): the process waits for non-daemon
    // threads before the exit code is produced (T15 owns edge cases).
    ::j2r_runtime::thread::await_non_daemons();
    code
}}
"#,
            statics = prog.total_statics,
            classes = prog.classes.len(),
            clinit_names = clinit_names_line(prog),
            module = m.module,
            main_fn = m.fn_name,
            defaults = static_defaults_literal(prog, &lib_crate),
            constants = static_constants_literal(prog),
            main_clinit_ensure = main_clinit_ensure_line(&lib_crate, m),
        );
        emit_glue(
            out,
            "src/main.rs",
            main_rs.as_bytes(),
            &mut glue,
            &mut stats,
        )?;
    } else if out.join("src/main.rs").exists() {
        // a previous compile had a main class, this one does not: a stale
        // src/main.rs would be auto-discovered by cargo as a binary target
        std::fs::remove_file(out.join("src/main.rs"))?;
        stats.orphans_removed += 1;
    }

    // orphaned per-class files (classes no longer in the program) ---------
    let mut orphan_dirs = vec![src.join("classes")];
    while let Some(dir) = orphan_dirs.pop() {
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let p = entry.path();
            if p.is_dir() {
                orphan_dirs.push(p);
                continue;
            }
            if p.extension().map(|e| e == "rs").unwrap_or(false)
                && p.file_name().map(|n| n != "mod.rs").unwrap_or(false)
            {
                let stem = p.file_stem().unwrap_or_default().to_string_lossy();
                if !prog.classes.iter().any(|c| c.module == stem) {
                    std::fs::remove_file(&p)?;
                    stats.orphans_removed += 1;
                }
            }
        }
    }

    // refresh the manifest --------------------------------------------------
    let manifest = Manifest {
        emitter: EMITTER_VERSION,
        emit_cfg: opts.emit_cfg,
        emit_typed: opts.emit_typed,
        surface,
        layout: "-".to_string(),
        crate_name: opts
            .crate_name
            .chars()
            .map(|c| if c == '\t' || c == '\n' { '_' } else { c })
            .collect(),
        entries,
        glue,
    };
    manifest.save(out)?;

    Ok(stats)
}

/// Render a glue file (crate-level, not per-class) and write it only when
/// its bytes change; record its hash for the manifest.
fn emit_glue(
    out: &Path,
    rel: &str,
    bytes: &[u8],
    glue: &mut BTreeMap<String, String>,
    stats: &mut EmitStats,
) -> crate::Result<()> {
    if write_if_changed(&out.join(rel), bytes)? {
        stats.files_written += 1;
    } else {
        stats.files_unchanged += 1;
    }
    glue.insert(rel.to_string(), sha256_hex(bytes));
    Ok(())
}

/// Record glue hashes for the vendored runtime (already copied
/// content-idempotently by [`copy_runtime`]); written/unchanged counting
/// comes from comparing against the prior manifest's recorded hashes.
pub(crate) fn record_runtime_glue_pub(
    out: &Path,
    glue: &mut BTreeMap<String, String>,
    prior: Option<&Manifest>,
    stats: &mut EmitStats,
) {
    let rt = out.join("runtime");
    let mut record = |rel: String, path: &Path| {
        if let Ok(bytes) = std::fs::read(path) {
            let sha = sha256_hex(&bytes);
            match prior.and_then(|m| m.glue.get(&rel)) {
                Some(old) if *old == sha => stats.files_unchanged += 1,
                _ => stats.files_written += 1,
            }
            glue.insert(rel, sha);
        }
    };
    record("runtime/Cargo.toml".to_string(), &rt.join("Cargo.toml"));
    // The runtime source tree is walked recursively (it contains directory
    // modules since T08 — `jni/`): manifest keys are the slash-separated
    // paths relative to the vendored crate root.
    let src_root = rt.join("src");
    let mut stack = vec![src_root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                stack.push(path);
            } else if e.file_type().map(|t| t.is_file()).unwrap_or(false) {
                let rel = path
                    .strip_prefix(&src_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                record(format!("runtime/src/{rel}"), &path);
            }
        }
    }
}

/// Relative path of a per-class module inside its crate (single-crate:
/// under the out dir; partitioned: under `p{i}`).
pub fn class_file_rel(module: &str) -> std::path::PathBuf {
    std::path::PathBuf::from("src")
        .join("classes")
        .join(format!("{module}.rs"))
}

/// The generated statics pool module (id-keyed, program-agnostic — the
/// same source serves the single-crate emission and the M5 support
/// crate verbatim).
pub(crate) fn statics_source() -> &'static str {
    r#"// Generated by j2r — do not edit.
//! Static field pool; class initialization is gated by the runtime's
//! JLS 12.4.2 state machine (`j2r_runtime::clinit`).
//!
//! All pools are process-global `Mutex`es since M4 Wave 1 (T03) — shared
//! substrate for the threads milestone. Initialized by `init_pool` from
//! the generated `main`. `apply_defaults` gives every static slot its Java
//! default value (0, 0.0, false, null) — the JVM does this at class
//! initialization, and javac emits no `<clinit>` for fields without
//! initializers.
//!
//! Lock law (j2r-runtime heap docs L1–L3): a pool lock may be held while
//! allocating (L2: cache→heap nesting), but the collector's root scan
//! (`gc_scan_roots`) runs lock-free from the heap's pointer-free phase.

use ::std::sync::Mutex;

static POOL: Mutex<Vec<::j2r_runtime::Value>> = Mutex::new(Vec::new());
/// Synthetic per-class monitor objects for `ACC_SYNCHRONIZED` static
/// methods (M3_PLAN §7.1). Allocated lazily on first acquisition and
/// scanned as permanent roots by `gc_scan_roots`.
static MONITORS: Mutex<Vec<Option<::j2r_runtime::heap::JRef>>> =
    Mutex::new(Vec::new());

/// The Java default value kind of a static field's descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Df {
    /// boolean/byte/short/char/int → 0
    Int,
    /// long → 0L
    Long,
    /// float → 0.0f
    Float,
    /// double → 0.0d
    Double,
    /// references → null
    Ref,
}

/// Must be called once before any generated code runs.
pub fn init_pool(statics: usize, classes: usize) {
    *POOL.lock().unwrap_or_else(|e| e.into_inner()) = vec![::j2r_runtime::Value::Void; statics];
    ::j2r_runtime::clinit::reserve_classes(classes);
}

/// Fills slot ranges with their Java default values (run-length encoded by
/// the generator; a pool of 80,000 same-typed slots is one range).
pub fn apply_defaults(ranges: &[(u32, u32, Df)]) {
    let v_of = |d: Df| match d {
        Df::Int => ::j2r_runtime::Value::Int(0),
        Df::Long => ::j2r_runtime::Value::Long(0),
        Df::Float => ::j2r_runtime::Value::Float(0.0),
        Df::Double => ::j2r_runtime::Value::Double(0.0),
        Df::Ref => ::j2r_runtime::Value::Ref(::core::option::Option::None),
    };
    let mut pool = POOL.lock().unwrap_or_else(|e| e.into_inner());
    for (start, len, d) in ranges {
        let v = v_of(*d);
        for slot in *start as usize..(*start + *len) as usize {
            if let Some(cell) = pool.get_mut(slot) {
                *cell = v.clone();
            }
        }
    }
}

/// Sets `ConstantValue` static finals (JVMS preparation-phase semantics:
/// these hold before any `<clinit>` code runs). Emitted as a literal table.
pub fn set_constants(constants: &[(u32, ::j2r_runtime::Value)]) {
    let mut pool = POOL.lock().unwrap_or_else(|e| e.into_inner());
    for (slot, v) in constants {
        if let Some(cell) = pool.get_mut(*slot as usize) {
            *cell = v.clone();
        }
    }
}

pub fn get(idx: u32) -> ::j2r_runtime::Value {
    POOL.lock().unwrap_or_else(|e| e.into_inner())[idx as usize].clone()
}

pub fn set(idx: u32, v: ::j2r_runtime::Value) {
    POOL.lock().unwrap_or_else(|e| e.into_inner())[idx as usize] = v;
}

/// Static-pool access for the reflection subset (`Field.get/set` on static
/// fields, M3_PLAN §8): the same pool through the `Dispatch` fn-pointer
/// shape the runtime natives call.
pub fn pool_get_value(idx: u32) -> ::j2r_runtime::Value {
    get(idx)
}

pub fn pool_set_value(idx: u32, v: &::j2r_runtime::Value) {
    set(idx, v.clone());
}

/// Runs `<clinit>` for the class (by id) exactly once, before first use of
/// its statics / first instantiation. Delegates to the runtime's JLS
/// 12.4.2 gate (clinit.rs): concurrent first-touchers BLOCK until the
/// winner's clinit completes; same-thread re-entry is free. A throwing
/// clinit marks the class errant and surfaces as Err (the EIIE wrapper or
/// the raw Error, JLS 12.4.2 step 11) — CLINIT-THROW.
pub fn ensure(class_id: u32, clinit: ::core::option::Option<::j2r_runtime::MethodFn>) -> ::core::result::Result<(), ::j2r_runtime::Throw> {
    ::j2r_runtime::clinit::ensure(class_id, clinit)
}

/// Collector hook (M2_PLAN §4.4): every reference in the statics pool is a
/// root. Registered by `main_run` via `j2r_runtime::heap::set_statics_scanner`.
pub fn gc_scan_roots(out: &mut ::std::vec::Vec<u32>) {
    for v in POOL.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        if let ::j2r_runtime::Value::Ref(::core::option::Option::Some(r)) = v {
            out.push(r.index());
        }
    }
    for r in MONITORS.lock().unwrap_or_else(|e| e.into_inner()).iter().flatten() {
        out.push(r.index());
    }
}

/// The synthetic per-class monitor object for `ACC_SYNCHRONIZED` static
/// methods (M3_PLAN §7.1): one object per class needing it, allocated on
/// first use, rooted forever.
pub fn class_monitor(idx: u32) -> ::j2r_runtime::heap::JRef {
    let mut b = MONITORS.lock().unwrap_or_else(|e| e.into_inner());
    while b.len() <= idx as usize {
        b.push(::core::option::Option::None);
    }
    if let ::core::option::Option::Some(r) = b[idx as usize] {
        return r;
    }
    let r = ::j2r_runtime::object::new_obj("j2r/internal/ClassMonitor", 0)
        .expect("j2r internal: class monitor allocation");
    b[idx as usize] = ::core::option::Option::Some(r);
    r
}
"#
}

/// SCREAMING_SNAKE form of a sanitized module name, used for generated
/// registry statics (`ClassMeta` → `CLASSMETA`).
fn upper_snake(module: &str) -> String {
    module
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// The Java default value kind of a static field descriptor.
pub(crate) fn static_default_of(desc: &str) -> DfKind {
    match desc {
        "J" => DfKind::Long,
        "F" => DfKind::Float,
        "D" => DfKind::Double,
        d if d.starts_with('L') || d.starts_with('[') => DfKind::Ref,
        _ => DfKind::Int, // I Z B C S all default to 0
    }
}

pub(crate) enum DfKind {
    Int,
    Long,
    Float,
    Double,
    Ref,
}

/// Run-length-encoded default values for the static pool, in slot order.
/// A program whose statics are all ints (the common case) is a single range.
fn static_default_ranges(prog: &Program) -> Vec<(u32, u32, DfKind)> {
    static_default_ranges_in(prog.classes.iter())
}

/// Per-partition core (G2.1, console.md F3): the same run-length merge
/// over exactly the given classes. Coverage — not order — decides the
/// applied defaults (each slot is written its kind's default exactly
/// once), so partition-local ranges compose to the same pool state as
/// the old global literal; the partition differential pins this.
fn static_default_ranges_in<'a>(
    classes: impl Iterator<Item = &'a ClassPlan>,
) -> Vec<(u32, u32, DfKind)> {
    let mut slots: Vec<(u32, &str)> = Vec::new();
    for c in classes {
        for f in &c.fields {
            if f.is_static {
                slots.push((f.static_slot.expect("static slot"), f.desc.as_str()));
            }
        }
    }
    slots.sort_unstable_by_key(|(s, _)| *s);
    let mut ranges: Vec<(u32, u32, DfKind)> = Vec::new();
    for (slot, desc) in slots {
        let kind = static_default_of(desc);
        match ranges.last_mut() {
            // run-length: consecutive slots of the same kind merge
            Some((start, len, last)) if *start + *len == slot && same_kind(last, &kind) => {
                *len += 1;
            }
            _ => ranges.push((slot, 1, kind)),
        }
    }
    ranges
}

fn same_kind(a: &DfKind, b: &DfKind) -> bool {
    matches!(
        (a, b),
        (DfKind::Int, DfKind::Int)
            | (DfKind::Long, DfKind::Long)
            | (DfKind::Float, DfKind::Float)
            | (DfKind::Double, DfKind::Double)
            | (DfKind::Ref, DfKind::Ref)
    )
}

/// The `&[(u32, u32, Df)]` literal for the generated `main.rs` (the
/// template calls `apply_defaults({defaults})`; `Df` is fully qualified
/// because `main.rs` is a separate crate root from the `statics` module).
pub(crate) fn static_defaults_literal(prog: &Program, lib_crate: &str) -> String {
    let mut out = String::from("&[");
    for (start, len, kind) in static_default_ranges(prog) {
        let k = match kind {
            DfKind::Int => "Int",
            DfKind::Long => "Long",
            DfKind::Float => "Float",
            DfKind::Double => "Double",
            DfKind::Ref => "Ref",
        };
        out.push_str(&format!(
            "({start}u32, {len}u32, {lib_crate}::statics::Df::{k}), "
        ));
    }
    out.push(']');
    out
}

/// The per-partition `register_defaults` fn source (G2.1): the defaults
/// table lives in the partition crate, bounded by k, so no generated
/// compilation unit scales with TOTAL class count (console.md F3 — the
/// 20k app-bin peak was the total-class literal in main.rs). The app
/// composes the partitions in order at startup.
pub(crate) fn static_defaults_register_fn(part: &[&ClassPlan]) -> String {
    let mut out = String::from(
        "pub fn register_defaults(out: &mut ::std::vec::Vec<(u32, u32, support::statics::Df)>) {\n",
    );
    out.push_str("    out.extend_from_slice(&[");
    for (start, len, kind) in static_default_ranges_in(part.iter().copied()) {
        let k = match kind {
            DfKind::Int => "Int",
            DfKind::Long => "Long",
            DfKind::Float => "Float",
            DfKind::Double => "Double",
            DfKind::Ref => "Ref",
        };
        out.push_str(&format!(
            "({start}u32, {len}u32, support::statics::Df::{k}), "
        ));
    }
    out.push_str("]);\n}\n");
    out
}

/// The `&[(u32, Value)]` literal of `ConstantValue` static finals, in slot
/// order — the JVM's preparation-phase initialization.
pub(crate) fn static_constants_literal(prog: &Program) -> String {
    let mut out = String::from("&[");
    let mut slots: Vec<(u32, &ConstantValue)> = Vec::new();
    for c in &prog.classes {
        for f in &c.fields {
            if f.is_static {
                if let Some(cv) = &f.constant_value {
                    slots.push((f.static_slot.expect("static slot"), cv));
                }
            }
        }
    }
    slots.sort_unstable_by_key(|(s, _)| *s);
    for (slot, cv) in slots {
        let v = match cv {
            ConstantValue::Int(v) => format!("{RT}::Value::Int({v})"),
            ConstantValue::Long(v) => format!("{RT}::Value::Long({v}i64)"),
            ConstantValue::Float(v) => format!("{RT}::Value::Float({})", f32_lit(*v)),
            ConstantValue::Double(v) => format!("{RT}::Value::Double({})", f64_lit(*v)),
            ConstantValue::String(s) => {
                format!("{RT}::Value::Str(::std::sync::Arc::new({s:?}.to_string()))")
            }
        };
        out.push_str(&format!("({slot}u32, {v}), "));
    }
    out.push(']');
    out
}

/// The per-partition `register_constants` fn source (G2.1): the same
/// split as the defaults table — a per-partition slot-sorted literal
/// (bounded by k) composed by the app at startup in partition order,
/// instead of one total-class-scaled literal in app/src/main.rs.
pub(crate) fn static_constants_register_fn(part: &[&ClassPlan]) -> String {
    let mut slots: Vec<(u32, &ConstantValue)> = Vec::new();
    for c in part {
        for f in &c.fields {
            if f.is_static {
                if let Some(cv) = &f.constant_value {
                    slots.push((f.static_slot.expect("static slot"), cv));
                }
            }
        }
    }
    slots.sort_unstable_by_key(|(s, _)| *s);
    let mut out = String::from(
        "pub fn register_constants(out: &mut ::std::vec::Vec<(u32, ::j2r_runtime::Value)>) {\n",
    );
    out.push_str("    out.extend_from_slice(&[");
    for (slot, cv) in slots {
        let v = match cv {
            ConstantValue::Int(v) => format!("{RT}::Value::Int({v})"),
            ConstantValue::Long(v) => format!("{RT}::Value::Long({v}i64)"),
            ConstantValue::Float(v) => format!("{RT}::Value::Float({})", f32_lit(*v)),
            ConstantValue::Double(v) => format!("{RT}::Value::Double({})", f64_lit(*v)),
            ConstantValue::String(s) => {
                format!("{RT}::Value::Str(::std::sync::Arc::new({s:?}.to_string()))")
            }
        };
        out.push_str(&format!("({slot}u32, {v}), "));
    }
    out.push_str("]);\n}\n");
    out
}

fn sanitize_crate_ident(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub(crate) fn bin_name(crate_name: &str) -> String {
    bin_name_pub(crate_name)
}

/// Public-inside-crate alias used by the partition emitter.
pub(crate) fn bin_name_pub(crate_name: &str) -> String {
    sanitize_crate_ident(crate_name)
}

pub(crate) fn copy_runtime(out: &Path) -> crate::Result<()> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let crates_root = Path::new(manifest).join("..");
    // T09: the runtime gained an internal path dependency on the audited
    // FFI crate (j2r-jni-ffi, docs/M5_JNI_FFI.md) — a generated project
    // vendors BOTH crates, and the runtime manifest's `../j2r-jni-ffi`
    // path resolves inside the generated layout because the vendored
    // directory names mirror the workspace-relative names.
    for (crate_dir, vendored) in [("j2r-runtime", "runtime"), ("j2r-jni-ffi", "j2r-jni-ffi")] {
        let source = crates_root.join(crate_dir);
        let dest = out.join(vendored);
        // Idempotent vendoring: a file is copied only when its CONTENT differs
        // from what is already at the destination, and stale files that no
        // longer exist upstream are removed. An unchanged runtime therefore
        // keeps its mtimes, so cargo's fingerprints stay valid across repeated
        // emit_crate calls into the same out dir — the cfg-equiv harness
        // regenerates per fixture, and without this the vendored runtime (the
        // expensive compile unit) rebuilt 3 × fixtures times. A stale or
        // partially-written file fails the content compare and is re-copied:
        // self-healing. The walk is recursive since T08 (the runtime has
        // directory modules — `jni/`).
        std::fs::create_dir_all(&dest)?;
        copy_file_idempotent(&source.join("Cargo.toml"), &dest.join("Cargo.toml"))?;
        std::fs::create_dir_all(dest.join("src"))?;
        let mut upstream: Vec<std::path::PathBuf> = Vec::new();
        copy_dir_recursive(
            &source.join("src"),
            &dest.join("src"),
            Path::new(""),
            &mut upstream,
        )?;
        remove_stale_files(&dest.join("src"), &dest.join("src"), &upstream)?;
    }
    Ok(())
}

/// Recursively copy `src_dir` into `dst_dir`, mirroring subdirectories and
/// collecting every copied file's path relative to the src root.
fn copy_dir_recursive(
    src_dir: &Path,
    dst_dir: &Path,
    rel: &Path,
    upstream: &mut Vec<std::path::PathBuf>,
) -> crate::Result<()> {
    std::fs::create_dir_all(dst_dir)?;
    for entry in std::fs::read_dir(src_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let child_rel = rel.join(&name);
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_dir.join(&name), &child_rel, upstream)?;
        } else {
            upstream.push(child_rel);
            copy_file_idempotent(&entry.path(), &dst_dir.join(&name))?;
        }
    }
    Ok(())
}

/// Remove vendored files that no longer exist upstream (recursively), then
/// any directories left empty by those removals.
fn remove_stale_files(
    dir: &Path,
    src_root: &Path,
    upstream: &[std::path::PathBuf],
) -> crate::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            remove_stale_files(&path, src_root, upstream)?;
            if std::fs::read_dir(&path)?.next().is_none() {
                std::fs::remove_dir(&path)?;
            }
        } else {
            let rel = path.strip_prefix(src_root).unwrap_or(&path).to_path_buf();
            if !upstream.iter().any(|u| u == &rel) {
                std::fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

/// Copy `src` to `dst` only when the destination's content differs (missing,
/// stale, or partially written). Preserves mtimes — and with them cargo's
/// path-dep fingerprints — when nothing changed.
fn copy_file_idempotent(src: &Path, dst: &Path) -> crate::Result<()> {
    let want = std::fs::read(src)?;
    if let Ok(have) = std::fs::read(dst) {
        if have == want {
            return Ok(());
        }
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dst, want)?;
    Ok(())
}

// ---------------------------------------------------------------- class module

pub(crate) fn emit_class(
    prog: &Program,
    c: &ClassPlan,
    emit_cfg: bool,
    emit_typed: bool,
) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "// Generated by j2r from {}.class — do not edit.",
        c.name
    );
    let _ = writeln!(
        out,
        "#![allow(dead_code, unused_variables, unused_mut, unused_assignments, unused_labels, unreachable_code, unused_parens, non_snake_case)]"
    );
    for m in &c.methods {
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", emit_method(prog, c, m, emit_cfg, emit_typed));
    }
    out
}

fn emit_method(
    prog: &Program,
    c: &ClassPlan,
    m: &MethodPlan,
    emit_cfg: bool,
    emit_typed: bool,
) -> String {
    // The native call-through body (T12 12c, docs/M5_JNI_NATIVES.md §4):
    // a `native` method has no bytecode — the implementation arrives via
    // the binding registry (RegisterNatives / the Rust bind API). The
    // body is ONE call into the runtime under the SAME MethodFn
    // contract as every generated body, so the ordinary
    // zero-on-throw/UnsatisfiedLinkError machinery applies unchanged.
    // The FrameGuard stays: a native call is a Java frame (the
    // catchable-SOE law counts it).
    if m.is_native {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "pub fn {}(recv: ::core::option::Option<&{RT}::JRef>, args: &[{RT}::Value]) -> ::core::result::Result<{RT}::Value, {RT}::Throw> {{",
            m.fn_name
        );
        let _ = writeln!(
            out,
            "    let __frame = match {RT}::frame::FrameGuard::enter() {{"
        );
        let _ = writeln!(out, "        ::core::result::Result::Ok(f) => f,");
        let _ = writeln!(
            out,
            "        ::core::result::Result::Err(t) => return ::core::result::Result::Err(t),"
        );
        let _ = writeln!(out, "    }};");
        let _ = writeln!(
            out,
            "    {RT}::jni::natives::call_native({:?}, {:?}, {:?}, recv, args)",
            c.name, m.name, m.desc
        );
        let _ = writeln!(out, "}}");
        out
    } else if emit_typed {
        // The typed-operand body (the registered lever, off by default) is a
        // complete function — its own preamble, 'run loop and dispatch. Any
        // unsupported shape falls back to the approved default body below,
        // with a loud machine-countable marker comment.
        match crate::typed::try_emit(prog, c, m) {
            Ok(mut body) => {
                body.push('\n');
                body
            }
            Err(reason) => {
                format!(
                    "// j2r: typed fallback ({})\n{}",
                    reason.name(),
                    emit_method(prog, c, m, emit_cfg, false)
                )
            }
        }
    } else {
        emit_default_body(prog, c, m, emit_cfg)
    }
}

fn emit_default_body(prog: &Program, c: &ClassPlan, m: &MethodPlan, emit_cfg: bool) -> String {
    let mut out = String::new();
    let instance = !m.is_static;
    let unmarshal_start = if instance { 1usize } else { 0usize };
    let locals_len = m.max_locals.max(if instance { 1 } else { 0 });

    let _ = writeln!(
        out,
        "pub fn {}(recv: ::core::option::Option<&{RT}::JRef>, args: &[{RT}::Value]) -> ::core::result::Result<{RT}::Value, {RT}::Throw> {{",
        m.fn_name
    );
    // FrameGuard first — before any work (M2_PLAN §3.1): past the frame
    // limit it yields the catchable StackOverflowError and the emitted
    // match turns it into `return Err(..)` immediately. The JVM throws SOE
    // at the call site, so propagating the Err through the caller's
    // j2r_try! gives the caller's exception table first crack — the
    // callee's own handlers must not see it, and they don't.
    let _ = writeln!(
        out,
        "    let __frame = match {RT}::frame::FrameGuard::enter() {{"
    );
    let _ = writeln!(out, "        ::core::result::Result::Ok(f) => f,");
    let _ = writeln!(
        out,
        "        ::core::result::Result::Err(t) => return ::core::result::Result::Err(t),"
    );
    let _ = writeln!(out, "    }};");
    let _ = writeln!(
        out,
        "    let mut locals: ::std::vec::Vec<{RT}::Value> = ::std::vec![{RT}::Value::Void; {locals_len}usize];"
    );
    let _ = writeln!(
        out,
        "    {RT}::unmarshal_args(&mut locals, args, {:?}, {unmarshal_start}usize).expect(\"j2r internal: argument unmarshal\");",
        m.desc
    );
    if instance {
        let _ = writeln!(
            out,
            "    if let ::core::option::Option::Some(this_ref) = recv {{"
        );
        let _ = writeln!(
            out,
            "        locals[0usize] = {RT}::Value::Ref(::core::option::Option::Some(this_ref.clone()));"
        );
        let _ = writeln!(out, "    }}");
    }
    // ACC_SYNCHRONIZED prologue (M3_PLAN §7.1): acquire on entry; the
    // FrameGuard's Drop releases on every exit path (normal, `Err`, panic).
    // Plain early-return on failure (no `j2r_try!` — the `'run` label is
    // not yet in scope this early in the body; monitor acquisition cannot
    // fail in the single-threaded runtime anyway).
    if m.is_sync {
        if instance {
            out.push_str("    if let ::core::option::Option::Some(r) = recv {\n");
            out.push_str(&format!(
                "        if let ::core::result::Result::Err(t) = {RT}::monitor::monitor_enter(r) {{ return ::core::result::Result::Err(t); }}\n"
            ));
            out.push_str("        __frame.record_monitor(*r);\n    }\n");
        } else {
            let idx = c.static_monitor.expect("validated static monitor index");
            out.push_str(&format!(
                "    let __cm = crate::statics::class_monitor({idx}u32);\n"
            ));
            out.push_str(&format!(
                "    if let ::core::result::Result::Err(t) = {RT}::monitor::monitor_enter(&__cm) {{ return ::core::result::Result::Err(t); }}\n"
            ));
            out.push_str("    __frame.record_monitor(__cm);\n");
        }
    }
    let _ = writeln!(
        out,
        "    let mut stack: ::std::vec::Vec<{RT}::Value> = ::std::vec::Vec::with_capacity({}usize);",
        m.max_stack.max(4)
    );
    let _ = writeln!(out, "    let mut pc: u32 = 0u32;");
    let _ = writeln!(
        out,
        "    let mut thrown: ::core::option::Option<{RT}::Throw> = ::core::option::Option::None;"
    );
    let _ = writeln!(out, "    let mut fault_pc: u32 = 0u32;");

    // exception handler lookup
    let has_table = !m.exception_table.is_empty();
    if has_table {
        let _ = writeln!(
            out,
            "    fn __j2r_find_handler(thrown_class: &str, fault: u32) -> ::core::option::Option<u32> {{"
        );
        for (start, end, handler, catch) in &m.exception_table {
            let cond = match catch {
                Some(cls) => format!(
                    " && {RT}::is_subtype(thrown_class, {cls:?}, ::core::option::Option::Some(crate::registry::parents_of))"
                ),
                None => String::new(),
            };
            let _ = writeln!(
                out,
                "        if ({start}u32..{end}u32).contains(&fault){cond} {{ return ::core::option::Option::Some({handler}u32); }}"
            );
        }
        let _ = writeln!(out, "        ::core::option::Option::None");
        let _ = writeln!(out, "    }}");
    }

    let _ = writeln!(out, "    'run: loop {{");
    if has_table {
        let _ = writeln!(
            out,
            "        if let ::core::option::Option::Some(t) = thrown.take() {{"
        );
        let _ = writeln!(
            out,
            "            match __j2r_find_handler(&t.java_class(), fault_pc) {{"
        );
        let _ = writeln!(out, "                ::core::option::Option::Some(h) => {{");
        let _ = writeln!(out, "                    stack.clear();");
        let _ = writeln!(
            out,
            "                    stack.push({RT}::Value::Ref(::core::option::Option::Some(t.obj)));"
        );
        let _ = writeln!(out, "                    pc = h;");
        let _ = writeln!(out, "                }}");
        let _ = writeln!(
            out,
            "                ::core::option::Option::None => return ::core::result::Result::Err(t),"
        );
        let _ = writeln!(out, "            }}");
        let _ = writeln!(out, "        }}");
    } else {
        let _ = writeln!(
            out,
            "        if let ::core::option::Option::Some(t) = thrown.take() {{"
        );
        let _ = writeln!(out, "            return ::core::result::Result::Err(t);");
        let _ = writeln!(out, "        }}");
    }

    // Body emission. Default (approved M1 architecture): pc dispatch over
    // basic blocks. With `emit_cfg` (Wave-4 investigation, M2_PLAN §6, off
    // by default): try a structured lift for the conservative subset; any
    // unsupported shape silently falls back to the identical dispatch body,
    // so `--emit-cfg` can never change semantics — only the shape of the
    // bodies it successfully lifts. The typed-operand body was already
    // decided above (emit_typed); it never reaches this point.
    let lifted = if emit_cfg {
        crate::cfg::lift(m, |idx, tmp| {
            let (pc, instr) = &m.instrs[idx];
            emit_instr(prog, c, m, *pc, instr, tmp);
        })
        .ok()
    } else {
        None
    };
    if let Some(body) = lifted {
        for line in body.lines() {
            let _ = writeln!(out, "        {line}");
        }
    } else {
        // pc dispatch over basic blocks
        let leaders = compute_leaders(m);
        let mut sorted: Vec<u32> = leaders.iter().copied().collect();
        sorted.sort_unstable();

        let _ = writeln!(out, "        match pc {{");
        for (i, &leader) in sorted.iter().enumerate() {
            let next_leader = sorted.get(i + 1).copied();
            let block = emit_block(prog, c, m, leader, next_leader);
            let _ = writeln!(out, "            {leader}u32 => {{");
            for line in block {
                let _ = writeln!(out, "                {line}");
            }
            let _ = writeln!(out, "            }}");
        }
        let _ = writeln!(
            out,
            "            _ => ::core::panic!(\"j2r internal: pc out of range\"),"
        );
        let _ = writeln!(out, "        }}");
    }
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "}}");
    out
}

/// Safepoint poll for backward branches (T17.1, heap::gc_pending): an
/// allocation-free loop can never reach an allocation-site collection
/// point, so a concurrent stop-the-world collection would wait for it
/// forever — found by the MonitorContention N=1 freeze (the true root
/// cause of the CI-timing gate-9 hangs; the quiescence handshake waits
/// for every running thread to park and a tight `synchronized` loop has
/// no allocating opcode). A backward branch emits a guarded poll: one
/// Acquire load in the common case; the frame refresh + gc_point park
/// run only while a collection is actually in flight.
fn backward_poll(out: &mut Vec<String>, pc: u32, targets: &[u32]) {
    if !targets.iter().any(|t| *t <= pc) {
        return;
    }
    out.push(format!(
        "if {RT}::heap::gc_pending() {{ __frame.refresh(&locals, &stack, &thrown); {RT}::heap::gc_point(); }}"
    ));
}

pub(crate) fn is_terminator(instr: &Instr) -> bool {
    matches!(
        instr,
        Instr::Goto(_)
            | Instr::IfZero(..)
            | Instr::IfICmp(..)
            | Instr::IfRefCmp { .. }
            | Instr::IfNull(_)
            | Instr::IfNonNull(_)
            | Instr::TableSwitch { .. }
            | Instr::LookupSwitch { .. }
            | Instr::Return(_)
            | Instr::AThrow
    )
}

pub(crate) fn compute_leaders(m: &MethodPlan) -> std::collections::BTreeSet<u32> {
    let mut leaders = std::collections::BTreeSet::new();
    if m.instrs.is_empty() {
        return leaders;
    }
    leaders.insert(m.instrs[0].0);
    for (i, (_, instr)) in m.instrs.iter().enumerate() {
        let next_pc = m.instrs.get(i + 1).map(|(p, _)| *p);
        let mut branch_targets: Vec<u32> = Vec::new();
        let mut jumps = false;
        match instr {
            Instr::Goto(t) => {
                branch_targets.push(*t);
                jumps = true;
            }
            Instr::IfZero(_, t)
            | Instr::IfICmp(_, t)
            | Instr::IfNull(t)
            | Instr::IfNonNull(t)
            | Instr::IfRefCmp { target: t, .. } => {
                branch_targets.push(*t);
                // conditional: the fall-through path continues at the next pc
                if let Some(n) = next_pc {
                    leaders.insert(n);
                }
                jumps = true;
            }
            Instr::TableSwitch {
                default, targets, ..
            } => {
                branch_targets.push(*default);
                branch_targets.extend(targets.iter().copied());
                jumps = true;
            }
            Instr::LookupSwitch { default, pairs, .. } => {
                branch_targets.push(*default);
                branch_targets.extend(pairs.iter().map(|(_, t)| *t));
                jumps = true;
            }
            Instr::Return(_) | Instr::AThrow => jumps = true,
            _ => {}
        }
        let _ = jumps;
        for t in branch_targets {
            leaders.insert(t);
        }
    }
    // exception handler pcs are leaders
    for (_, _, handler, _) in &m.exception_table {
        leaders.insert(*handler);
    }
    leaders
}

fn emit_block(
    prog: &Program,
    c: &ClassPlan,
    m: &MethodPlan,
    leader: u32,
    next_leader: Option<u32>,
) -> Vec<String> {
    let mut lines = Vec::new();
    let start = match m.instrs.iter().position(|(pc, _)| *pc == leader) {
        Some(s) => s,
        None => {
            // leader is not an instruction pc (e.g. one past the end)
            match next_leader {
                Some(n) => lines.push(format!("pc = {n}u32;")),
                None => lines.push("::core::panic!(\"j2r internal: pc out of range\")".to_string()),
            }
            return lines;
        }
    };
    let mut terminated = false;
    for (pc, instr) in &m.instrs[start..] {
        if next_leader == Some(*pc) && *pc != leader {
            break;
        }
        emit_instr(prog, c, m, *pc, instr, &mut lines);
        if is_terminator(instr) {
            terminated = true;
            break;
        }
    }
    if !terminated {
        match next_leader {
            Some(n) => lines.push(format!("pc = {n}u32;")),
            None => lines.push(
                "::core::panic!(\"j2r internal: method fell off the end without a return\")"
                    .to_string(),
            ),
        }
    }
    lines
}

// ---------------------------------------------------------------- instructions

fn emit_instr(
    prog: &Program,
    c: &ClassPlan,
    m: &MethodPlan,
    pc: u32,
    instr: &Instr,
    out: &mut Vec<String>,
) {
    let fault = format!("{pc}u32");
    match instr {
        Instr::Nop => {}
        Instr::AConstNull => out.push(format!(
            "stack.push({RT}::Value::Ref(::core::option::Option::None));"
        )),
        Instr::IConst(v) => out.push(format!("stack.push({RT}::Value::Int({v}));")),
        Instr::LConst(v) => out.push(format!("stack.push({RT}::Value::Long({v}i64));")),
        Instr::FConst(v) => out.push(format!("stack.push({RT}::Value::Float({}));", f32_lit(*v))),
        Instr::DConst(v) => out.push(format!("stack.push({RT}::Value::Double({}));", f64_lit(*v))),
        Instr::Ldc(Ldc::Int(v)) => out.push(format!("stack.push({RT}::Value::Int({v}));")),
        Instr::Ldc(Ldc::Float(v)) => {
            out.push(format!("stack.push({RT}::Value::Float({}));", f32_lit(*v)))
        }
        Instr::Ldc(Ldc::String(s)) => out.push(format!(
            "stack.push({RT}::Value::Ref(::core::option::Option::Some({RT}::intern_string({s:?}))));"
        )),
        Instr::Ldc(Ldc::Class(name)) => {
            // M3 Wave 5 (M3_PLAN §8.1): `X.class` / `int.class` / array
            // literals resolve to the interned per-class `Class` object.
            out.push(format!(
                "stack.push({RT}::reflect::class_value({name:?}));"
            ))
        }
        Instr::Ldc2W(Ldc2::Long(v)) => {
            out.push(format!("stack.push({RT}::Value::Long({v}i64));"))
        }
        Instr::Ldc2W(Ldc2::Double(v)) => {
            out.push(format!("stack.push({RT}::Value::Double({}));", f64_lit(*v)))
        }
        Instr::Load(idx) => out.push(format!("stack.push(locals[{idx}usize].clone());")),
        Instr::Store(idx) => out.push(format!(
            "locals[{idx}usize] = {RT}::stack::stack_pop(&mut stack);"
        )),
        Instr::ALoad(_) => {
            out.push("let __idx = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push(format!(
                "let __arr = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::pop_ref(&mut stack));"
            ));
            out.push(format!(
                "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::array::array_load(&__arr, __idx));"
            ));
            out.push("stack.push(__v);".into());
        }
        Instr::AStore(_) => {
            out.push("let __v = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            out.push("let __idx = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push(format!(
                "let __arr = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::pop_ref(&mut stack));"
            ));
            out.push(format!(
                "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::array::array_store(&__arr, __idx, __v, crate::j2r_dispatch()));"
            ));
        }
        Instr::ArrayLength => {
            out.push(format!(
                "let __arr = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::pop_ref(&mut stack));"
            ));
            out.push(format!(
                "stack.push({RT}::Value::Int({RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::array::array_length(&__arr))));"
            ));
        }
        Instr::Pop => out.push("let _ = stack.pop();".into()),
        Instr::Pop2 => out.push("::j2r_runtime::stack::stack_pop2(&mut stack);".into()),
        Instr::Dup => out.push("::j2r_runtime::stack::stack_dup(&mut stack);".into()),
        Instr::DupX1 => out.push("::j2r_runtime::stack::stack_dup_x1(&mut stack);".into()),
        Instr::DupX2 => out.push("::j2r_runtime::stack::stack_dup_x2(&mut stack);".into()),
        Instr::Dup2 => out.push("::j2r_runtime::stack::stack_dup2(&mut stack);".into()),
        Instr::Dup2X1 => out.push("::j2r_runtime::stack::stack_dup2_x1(&mut stack);".into()),
        Instr::Dup2X2 => out.push("::j2r_runtime::stack::stack_dup2_x2(&mut stack);".into()),
        Instr::Swap => out.push("::j2r_runtime::stack::stack_swap(&mut stack);".into()),
        Instr::Arith { op, kind } => emit_arith(*op, *kind, &fault, out),
        Instr::Convert(conv) => emit_convert(*conv, out),
        Instr::IInc { idx, c } => out.push(format!(
            "locals[{idx}usize] = {RT}::Value::Int(locals[{idx}usize].expect_int().wrapping_add({c}));"
        )),
        Instr::Cmp(kind) => emit_cmp(*kind, out),
        Instr::IfZero(op, t) => {
            backward_poll(out, pc, &[*t]);
            out.push("let __a = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push(format!(
                "if __a {} 0 {{ pc = {t}u32; }} else {{ pc = {}u32; }}",
                op.rust_op(),
                next_pc_of(m, pc)
            ));
        }
        Instr::IfICmp(op, t) => {
            backward_poll(out, pc, &[*t]);
            out.push("let __b = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push("let __a = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push(format!(
                "if __a {} __b {{ pc = {t}u32; }} else {{ pc = {}u32; }}",
                op.rust_op(),
                next_pc_of(m, pc)
            ));
        }
        Instr::IfRefCmp { eq, target } => {
            backward_poll(out, pc, &[*target]);
            out.push("let __b = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            out.push("let __a = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            let cond = if *eq {
                format!("{RT}::ref_eq(&__a, &__b)")
            } else {
                format!("!{RT}::ref_eq(&__a, &__b)")
            };
            out.push(format!(
                "if {cond} {{ pc = {target}u32; }} else {{ pc = {}u32; }}",
                next_pc_of(m, pc)
            ));
        }
        Instr::IfNull(t) => {
            backward_poll(out, pc, &[*t]);
            out.push("let __a = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            out.push(format!(
                "if __a.is_null() {{ pc = {t}u32; }} else {{ pc = {}u32; }}",
                next_pc_of(m, pc)
            ));
        }
        Instr::IfNonNull(t) => {
            backward_poll(out, pc, &[*t]);
            out.push("let __a = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            out.push(format!(
                "if !__a.is_null() {{ pc = {t}u32; }} else {{ pc = {}u32; }}",
                next_pc_of(m, pc)
            ));
        }
        Instr::Goto(t) => {
            backward_poll(out, pc, &[*t]);
            out.push(format!("pc = {t}u32;"));
        }
        Instr::TableSwitch { default, low, targets } => {
            let mut ts = targets.clone();
            ts.push(*default);
            backward_poll(out, pc, &ts);
            out.push("let __k = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push("match __k {".into());
            for (i, t) in targets.iter().enumerate() {
                let v = *low as i64 + i as i64;
                out.push(format!("    {v}i32 => {{ pc = {t}u32; }}"));
            }
            out.push(format!("    _ => {{ pc = {default}u32; }}"));
            out.push("}".into());
        }
        Instr::LookupSwitch { default, pairs } => {
            let mut ts: Vec<u32> = pairs.iter().map(|(_, t)| *t).collect();
            ts.push(*default);
            backward_poll(out, pc, &ts);
            out.push("let __k = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push("match __k {".into());
            for (v, t) in pairs {
                out.push(format!("    {v}i32 => {{ pc = {t}u32; }}"));
            }
            out.push(format!("    _ => {{ pc = {default}u32; }}"));
            out.push("}".into());
        }
        Instr::Return(t) => {
            if *t == j2r_runtime::TypeDesc::Void {
                out.push("return ::core::result::Result::Ok(::j2r_runtime::Value::Void);".into());
            } else {
                out.push(
                    "return ::core::result::Result::Ok(::j2r_runtime::stack::stack_pop(&mut stack));"
                        .into(),
                );
            }
        }
        Instr::GetStatic { class, field, .. } => {
            if class == "java/lang/System" && field == "out" {
                out.push(format!("stack.push({RT}::natives::system_out());"));
            } else if class == "java/lang/System" && field == "err" {
                out.push(format!("stack.push({RT}::natives::system_err());"));
            } else if class == "java/util/Locale" && field == "US" {
                // Corpus wave 3 (JAMA): the Locale.US constant object.
                out.push(format!("stack.push({RT}::natives::builtin_locale_us());"));
            } else if field == "TYPE" {
                if let Some(desc) = crate::primitive_type_field(class) {
                    out.push(format!(
                        "stack.push({RT}::reflect::class_value({desc:?}));"
                    ));
                } else {
                    let f = crate::resolve_field_pub(prog, class, field, true)
                        .expect("validated static field");
                    let slot = f.static_slot.expect("static field");
                    if let Some(marker) = jmm_boundary_marker(f, class) {
                        out.push(marker);
                    }
                    out.push(format!("stack.push(crate::statics::get({slot}u32));"));
                }
            } else {
                let f = crate::resolve_field_pub(prog, class, field, true)
                    .expect("validated static field");
                let slot = f.static_slot.expect("static field");
                if let Some(marker) = jmm_boundary_marker(f, class) {
                    out.push(marker);
                }
                let ensures = ensure_calls(prog, class);
                // `ensure` may dispatch into user code (<clinit>): refresh
                // the frame snapshot first (M2_PLAN §4.4).
                if !ensures.is_empty() {
                    out.push("__frame.refresh(&locals, &stack, &thrown);".into());
                }
                for (id, expr) in ensures {
                    // CLINIT-THROW: a throwing clinit surfaces HERE as an
                    // Err — the EIIE wrapper (or the raw Error) must ride
                    // the method's exception table like any other throw.
                    out.push(format!(
                        "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, crate::statics::ensure({id}u32, {expr}));"
                    ));
                }
                out.push(format!("stack.push(crate::statics::get({slot}u32));"));
            }
        }
        Instr::PutStatic { class, field, .. } => {
            let f = crate::resolve_field_pub(prog, class, field, true)
                .expect("validated static field");
            let slot = f.static_slot.expect("static field");
            if let Some(marker) = jmm_boundary_marker(f, class) {
                out.push(marker);
            }
            // ensure → possible <clinit> dispatch; refresh while the value
            // is still on the stack so the snapshot holds it.
            out.push("__frame.refresh(&locals, &stack, &thrown);".into());
            out.push("let __v = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            for (id, expr) in ensure_calls(prog, class) {
                // CLINIT-THROW: the putstatic's clinit throw propagates.
                out.push(format!(
                    "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, crate::statics::ensure({id}u32, {expr}));"
                ));
            }
            out.push(format!("crate::statics::set({slot}u32, __v);"));
        }
        Instr::GetField { class, field, .. } => {
            let slot =
                crate::resolve_field_slot_pub(prog, class, field, false).expect("validated field");
            // Marker only for USER fields — a builtin-ancestor access
            // (corpus wave 2: FilterInputStream.in) resolves to a slot,
            // not a FieldPlan, and the checker's scope is the input
            // bytecode's own fields.
            if let Ok(f) = crate::resolve_field_pub(prog, class, field, false) {
                if let Some(marker) = jmm_boundary_marker(f, class) {
                    out.push(marker);
                }
            }
            out.push(format!(
                "let __r = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::pop_ref(&mut stack));"
            ));
            out.push(format!(
                "stack.push({RT}::object::obj_get(&__r, {slot}usize));"
            ));
        }
        Instr::PutField { class, field, .. } => {
            let slot =
                crate::resolve_field_slot_pub(prog, class, field, false).expect("validated field");
            if let Ok(f) = crate::resolve_field_pub(prog, class, field, false) {
                if let Some(marker) = jmm_boundary_marker(f, class) {
                    out.push(marker);
                }
            }
            out.push("let __v = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            out.push(format!(
                "let __r = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::pop_ref(&mut stack));"
            ));
            out.push(format!("{RT}::object::obj_set(&__r, {slot}usize, __v);"));
        }
        Instr::InvokeVirtual { class, name, desc }
        | Instr::InvokeInterface { class, name, desc } => {
            // Call boundary: refresh the root snapshot before popping args
            // so the receiver and arguments stay rooted during the call
            // (M2_PLAN §4.4).
            out.push("__frame.refresh(&locals, &stack, &thrown);".into());
            let n = arg_count(desc);
            out.push(format!("let __a = {RT}::stack::pop_n(&mut stack, {n}usize);"));
            out.push(format!(
                "let __r = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::pop_ref(&mut stack));"
            ));
            out.push(format!(
                "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::invoke_virtual(&__r, {class:?}, {name:?}, {desc:?}, &__a, crate::j2r_dispatch()));"
            ));
            if !returns_void(desc) {
                out.push("stack.push(__v);".into());
            }
        }
        Instr::InvokeStatic { class, name, desc } => {
            out.push("__frame.refresh(&locals, &stack, &thrown);".into());
            let n = arg_count(desc);
            out.push(format!("let __a = {RT}::stack::pop_n(&mut stack, {n}usize);"));
            match crate::resolve_call_pub(prog, class, name, desc).expect("validated call") {
                CallTarget::User { module, fn_name } => {
                    for (id, expr) in ensure_calls(prog, class) {
                        // CLINIT-THROW: the invokestatic's clinit throw
                        // propagates before the call.
                        out.push(format!(
                            "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, crate::statics::ensure({id}u32, {expr}));"
                        ));
                    }
                    out.push(format!(
                        "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, crate::classes::{module}::{fn_name}(::core::option::Option::None, &__a));"
                    ));
                }
                CallTarget::Native { class: ncls } => {
                    out.push(format!(
                        "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::natives::invoke_exact({ncls:?}, {name:?}, {desc:?}, ::core::option::Option::None, &__a, crate::j2r_dispatch()));"
                    ));
                }
            }
            if !returns_void(desc) {
                out.push("stack.push(__v);".into());
            }
        }
        Instr::InvokeSpecial { class, name, desc } => {
            out.push("__frame.refresh(&locals, &stack, &thrown);".into());
            let n = arg_count(desc);
            out.push(format!("let __a = {RT}::stack::pop_n(&mut stack, {n}usize);"));
            out.push(format!(
                "let __r = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::pop_ref(&mut stack));"
            ));
            match crate::resolve_call_pub(prog, class, name, desc).expect("validated call") {
                CallTarget::User { module, fn_name } => {
                    out.push(format!(
                        "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, crate::classes::{module}::{fn_name}(::core::option::Option::Some(&__r), &__a));"
                    ));
                }
                CallTarget::Native { class: ncls } => {
                    out.push(format!(
                        "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::natives::invoke_exact({ncls:?}, {name:?}, {desc:?}, ::core::option::Option::Some(&__r), &__a, crate::j2r_dispatch()));"
                    ));
                }
            }
            if !returns_void(desc) {
                out.push("stack.push(__v);".into());
            }
        }
        Instr::InvokeDynamic { index } => {
            out.push("__frame.refresh(&locals, &stack, &thrown);".into());
            if let Some(plan) = c.lambdas.get(index) {
                // Closure construction (M3_PLAN §6.1): the captures are the
                // indy site's dynamic arguments; the refresh above rooted
                // them through the frame snapshot before the pop.
                let n = plan.capture_kinds.len();
                out.push(format!("let __a = {RT}::stack::pop_n(&mut stack, {n}usize);"));
                out.push(format!(
                    "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::lambda::make_closure({}u8, {:?}, {:?}, {:?}, {:?}, {}u32, &__a));",
                    plan.shape, plan.interface, plan.target_class, plan.target_name, plan.target_desc, plan.ctor_slots
                ));
            } else {
                let plan = c.concats.get(index).expect("validated concat site");
                let n = plan.kinds.len();
                out.push(format!("let __a = {RT}::stack::pop_n(&mut stack, {n}usize);"));
                let consts = plan
                    .constants
                    .iter()
                    .map(constant_literal)
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push(format!(
                    "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::concat::string_concat({:?}, &[{}], &__a, {:?}, crate::j2r_dispatch()));",
                    plan.recipe, consts, plan.kinds
                ));
            }
            out.push("stack.push(__v);".into());
        }
        Instr::MonitorEnter => {
            out.push(format!(
                "let __r = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::pop_ref(&mut stack));"
            ));
            out.push(format!(
                "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::monitor::monitor_enter(&__r));"
            ));
            out.push("__frame.record_monitor(__r);".into());
        }
        Instr::MonitorExit => {
            out.push(format!(
                "let __r = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::pop_ref(&mut stack));"
            ));
            out.push(format!(
                "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::monitor::monitor_exit(&__r));"
            ));
            out.push("__frame.unrecord_monitor(&__r);".into());
        }
        Instr::New { class } => {
            // The one collection point shape (M2_PLAN §4.3): refresh the
            // frame snapshot, then gc_point — before the allocation and
            // before the <clinit> ensure dispatch.
            out.push("__frame.refresh(&locals, &stack, &thrown);".into());
            out.push(format!("{RT}::heap::gc_point();"));
            new_obj_lines(prog, class, &fault, out);
            out.push(format!(
                "stack.push({RT}::Value::Ref(::core::option::Option::Some(__nobj)));"
            ));
        }
        Instr::NewArray(prim) => {
            out.push("__frame.refresh(&locals, &stack, &thrown);".into());
            out.push(format!("{RT}::heap::gc_point();"));
            let desc = prim.array_desc();
            out.push("let __len = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push(format!(
                "stack.push({RT}::Value::Ref(::core::option::Option::Some({RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::array::array_new_prim({desc:?}, __len)))));"
            ));
        }
        Instr::NewIntArrayFilled { prim, vals } => {
            // G2.6 arm a (peephole.rs): the collapsed array-literal fill.
            // The element source is a NESTED static — a separate
            // const-eval item, so the enclosing fn's MIR sees ONE call
            // instead of N aggregate operands (the measured
            // borrow-check/MIR wall). Fresh JRef per evaluation: same
            // newarray semantics as the uncollapsed form. The runtime
            // constructor applies the same per-prim narrowing as
            // array_store_i_raw, so the payload is byte-identical.
            out.push("__frame.refresh(&locals, &stack, &thrown);".into());
            out.push(format!("{RT}::heap::gc_point();"));
            let sname = format!("__J2R_AFILL_{pc}");
            let lit = vals
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let desc = prim.array_desc();
            out.push(format!(
                "static {sname}: [i32; {}] = [{}];",
                vals.len(),
                lit
            ));
            out.push(format!(
                "stack.push({RT}::Value::Ref(::core::option::Option::Some({RT}::array::array_int_group_filled({desc:?}, &{sname}))));"
            ));
        }
        Instr::ANewArray { elem } => {
            out.push("__frame.refresh(&locals, &stack, &thrown);".into());
            out.push(format!("{RT}::heap::gc_point();"));
            out.push("let __len = ::j2r_runtime::pop_int(&mut stack);".into());
            out.push(format!(
                "stack.push({RT}::Value::Ref(::core::option::Option::Some({RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::array::array_new_ref({elem:?}, __len)))));"
            ));
        }
        Instr::MultiNewArray { desc, dims } => {
            out.push("__frame.refresh(&locals, &stack, &thrown);".into());
            out.push(format!("{RT}::heap::gc_point();"));
            // The dimension sizes were pushed outermost-first, so pop them
            // back-to-front into __dim0..__dim(n-1).
            for i in (0..*dims).rev() {
                out.push(format!("let __dim{i} = ::j2r_runtime::pop_int(&mut stack);"));
            }
            let list: Vec<String> = (0..*dims).map(|i| format!("__dim{i}")).collect();
            out.push(format!(
                "stack.push({RT}::Value::Ref(::core::option::Option::Some({RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::array::array_new_multi({desc:?}, &[{}])))));",
                list.join(", ")
            ));
        }
        Instr::AThrow => {
            // pop the exception object; on success record the pending throw
            // (the Ok path of j2r_try! would otherwise fall through!)
            out.push(format!(
                "let __thr = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::object::take_thrown(&mut stack));"
            ));
            out.push(format!(
                "thrown = ::core::option::Option::Some({RT}::Throw {{ obj: __thr }});"
            ));
            // the faulting pc for this throw is the athrow itself — without
            // this, a rethrow inside its own catch handler loops forever
            out.push(format!("fault_pc = {fault};"));
        }
        Instr::CheckCast { class } => {
            out.push(format!(
                "let __v = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::checkcast(::j2r_runtime::stack::stack_pop(&mut stack), {class:?}, crate::j2r_dispatch()));"
            ));
            out.push("stack.push(__v);".into());
        }
        Instr::InstanceOf { class } => {
            out.push("let __v = ::j2r_runtime::stack::stack_pop(&mut stack);".into());
            out.push(format!(
                "stack.push({RT}::Value::Int({RT}::j2r_instance_of(&__v, {class:?}, crate::j2r_dispatch()) as i32));"
            ));
        }
    }
}

/// Rust float literal that is valid for every value (NaN/inf included).
pub(crate) fn f64_lit(v: f64) -> String {
    if v.is_nan() {
        "f64::NAN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 {
            "f64::INFINITY".to_string()
        } else {
            "f64::NEG_INFINITY".to_string()
        }
    } else {
        format!("{v:?}")
    }
}

pub(crate) fn f32_lit(v: f32) -> String {
    if v.is_nan() {
        "f32::NAN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 {
            "f32::INFINITY".to_string()
        } else {
            "f32::NEG_INFINITY".to_string()
        }
    } else {
        format!("{v:?}")
    }
}

pub(crate) fn next_pc_of(m: &MethodPlan, pc: u32) -> u32 {
    for (p, _) in &m.instrs {
        if *p > pc {
            return *p;
        }
    }
    pc + 1
}

pub(crate) fn returns_void(desc: &str) -> bool {
    matches!(
        j2r_runtime::parse_method_desc(desc),
        Some((_, j2r_runtime::TypeDesc::Void))
    )
}

pub(crate) fn arg_count(desc: &str) -> usize {
    let (params, _) = j2r_runtime::parse_method_desc(desc).expect("valid descriptor");
    params.len()
}

fn constant_literal(v: &j2r_runtime::Value) -> String {
    match v {
        j2r_runtime::Value::Int(i) => format!("{RT}::Value::Int({i})"),
        j2r_runtime::Value::Long(l) => format!("{RT}::Value::Long({l}i64)"),
        j2r_runtime::Value::Float(f) => format!("{RT}::Value::Float({})", f32_lit(*f)),
        j2r_runtime::Value::Double(d) => format!("{RT}::Value::Double({})", f64_lit(*d)),
        j2r_runtime::Value::Str(s) => {
            format!("{RT}::Value::Str(::std::sync::Arc::new({s:?}.to_string()))")
        }
        other => unreachable!("concat constant {other:?}"),
    }
}

/// G4.2 arm (b) — the JMM boundary marker. Returns the comment text for
/// every generated access to a field whose input bytecode flags promise
/// semantics the runtime does not provide: `ACC_VOLATILE` (no Java
/// volatile ordering) and `ACC_FINAL` without a `ConstantValue` (no
/// final-field freeze; folded constants are JMM-safe). The marker makes
/// the incompatibility visible AT THE USE SITE (docs/RUNTIME_SAFETY.md
/// §6), and CI gate-23 (scripts/check_jmm_boundary.py) fails loudly when
/// a boundary field lacks its markers or the `src/jmm_boundary.rs`
/// sidecar diverges from the input facts.
pub(crate) fn jmm_boundary_marker(f: &FieldPlan, class: &str) -> Option<String> {
    let kind = if f.is_volatile {
        "volatile"
    } else if f.is_final {
        "final"
    } else {
        return None;
    };
    Some(format!(
        "// J2R-JMM-BOUNDARY: {class}.{} is {kind} in the input bytecode; this access is a PLAIN non-atomic slot access — no Java volatile ordering / no final-field freeze (docs/RUNTIME_SAFETY.md §6).",
        f.name
    ))
}

/// `ensure` calls for the user-class chain from the topmost user ancestor
/// down to `start` — mirrors Java's rule that initializing a class first
/// initializes its superclasses.
pub(crate) fn ensure_calls(prog: &Program, start: &str) -> Vec<(u32, String)> {
    let mut chain: Vec<String> = Vec::new();
    let mut cur = start.to_string();
    for _ in 0..64 {
        if prog.is_user(&cur) {
            chain.push(cur.clone());
            let c = prog.class(&cur).unwrap();
            match &c.super_name {
                Some(s) if prog.is_user(s) => cur = s.clone(),
                _ => break,
            }
        } else {
            break;
        }
    }
    chain.reverse();
    chain
        .into_iter()
        .map(|name| {
            let c = prog.class(&name).expect("user class");
            let expr = match &c.clinit_fn {
                Some(fname) => format!(
                    "::core::option::Option::Some(crate::classes::{}::{} as ::j2r_runtime::MethodFn)",
                    c.module, fname
                ),
                None => "::core::option::Option::None".to_string(),
            };
            (c.id, expr)
        })
        .collect()
}

/// The allocation sequence for `new {class}`, shared by the default and
/// typed emitters (one source of truth for the JLS §4.12.5 default-value
/// law): the `<clinit>` ensure calls, the `new_obj`/`new_builtin` call
/// bound to `__nobj`, and the field-default spec. The refresh + gc_point
/// stay at the call site (identical for both emitters).
pub(crate) fn new_obj_lines(prog: &Program, class: &str, fault: &str, out: &mut Vec<String>) {
    if prog.is_user(class) {
        let target = prog.class(class).unwrap();
        let total = target.total_slots;
        // JLS §4.12.5: instance fields carry default values (0, false,
        // null) between allocation and the constructor body. The spec
        // must span the WHOLE user-ancestor chain — a subclass allocating
        // without running `new Parent()` anywhere still gets its
        // inherited fields defaulted. The builtin-parent region below the
        // root user class stays Void: those are the builtin's internals,
        // written by its constructors before any user code can read them.
        let mut root_name = class.to_string();
        loop {
            let sup = prog.class(&root_name).and_then(|c| c.super_name.clone());
            match sup {
                Some(s) if prog.is_user(&s) => root_name = s,
                _ => break,
            }
        }
        let spec_base = prog.class(&root_name).unwrap().base_slots;
        let mut spec = vec![b'i'; total - spec_base];
        // Fill from the root user class downward so the subclass
        // overrides any shadowed region (layouts are dense and
        // non-overlapping, so the walk order only matters for sanity).
        let mut chain = Vec::new();
        let mut cur = class.to_string();
        loop {
            chain.push(cur.clone());
            match prog.class(&cur).and_then(|c| c.super_name.clone()) {
                Some(s) if prog.is_user(&s) => cur = s,
                _ => break,
            }
        }
        for cls in chain.iter().rev() {
            let plan = prog.class(cls).unwrap();
            for f in &plan.fields {
                if let Some(abs) = f.instance_slot {
                    spec[abs - spec_base] = match f.desc.as_str() {
                        "B" | "C" | "S" | "I" | "Z" => b'i',
                        "J" => b'j',
                        "F" => b'f',
                        "D" => b'd',
                        _ => b'r',
                    };
                }
            }
        }
        let spec_str: String = spec.iter().map(|&c| c as char).collect();
        for (id, expr) in ensure_calls(prog, class) {
            // CLINIT-THROW: the new's clinit throw propagates before the
            // allocation.
            out.push(format!(
                "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, crate::statics::ensure({id}u32, {expr}));"
            ));
        }
        out.push(format!(
            "let __nobj = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::object::new_obj({class:?}, {total}usize));"
        ));
        out.push(format!(
            "{RT}::object::set_defaults(&__nobj, {spec_str:?});"
        ));
    } else {
        out.push(format!(
            "let __nobj = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::object::new_builtin({class:?}));"
        ));
    }
}

/// The JLS 12.4.2 launcher line for the app main template: ensure the
/// main class's `<clinit>` before main() runs. Empty when the class has
/// none. `crate_path` is `classes::` prefixed from the app crate root in
/// both emission modes (single-crate: `{lib_crate}::`; partitioned: the
/// owning partition crate `pN::`).
pub(crate) fn main_clinit_ensure_line(lib_crate: &str, m: &crate::MainInfo) -> String {
    match &m.clinit_fn {
        Some(fname) => format!(
            "{lib_crate}::statics::ensure({}u32, ::core::option::Option::Some({lib_crate}::classes::{}::{} as ::j2r_runtime::MethodFn))",
            m.class_id, m.module, fname
        ),
        None => "::core::result::Result::Ok(())".to_string(),
    }
}

/// The CLINIT-THROW name registration line: internal class names indexed
/// by class id, registered before any code runs so an errant clinit's
/// NCDFE message can carry the binary class name (Temurin's shape). Both
/// emission modes call it right after `init_pool`.
pub(crate) fn clinit_names_line(prog: &Program) -> String {
    let mut names: Vec<(u32, &str)> = prog
        .classes
        .iter()
        .map(|c| (c.id, c.name.as_str()))
        .collect();
    names.sort_by_key(|(id, _)| *id);
    let list: Vec<String> = names.iter().map(|(_, n)| format!("{n:?}")).collect();
    format!(
        "::j2r_runtime::clinit::register_class_names(&[{}]);",
        list.join(", ")
    )
}

fn emit_arith(op: ArithOp, kind: NumKind, fault: &str, out: &mut Vec<String>) {
    use NumKind::*;
    // pops: (reader fn, Value ctor); long shifts pop an int shift amount
    match kind {
        Int | Long => {
            emit_int_or_long_arith(op, kind == Long, fault, out);
        }
        Float | Double => {
            let pop = if kind == Float {
                "::j2r_runtime::pop_float"
            } else {
                "::j2r_runtime::pop_double"
            };
            let ctor = if kind == Float { "Float" } else { "Double" };
            match op {
                ArithOp::Neg => out.push(format!(
                    "let __a = {pop}(&mut stack); stack.push({RT}::Value::{ctor}(-__a));"
                )),
                ArithOp::Add => out.push(format!(
                    "let __b = {pop}(&mut stack); let __a = {pop}(&mut stack); stack.push({RT}::Value::{ctor}(__a + __b));"
                )),
                ArithOp::Sub => out.push(format!(
                    "let __b = {pop}(&mut stack); let __a = {pop}(&mut stack); stack.push({RT}::Value::{ctor}(__a - __b));"
                )),
                ArithOp::Mul => out.push(format!(
                    "let __b = {pop}(&mut stack); let __a = {pop}(&mut stack); stack.push({RT}::Value::{ctor}(__a * __b));"
                )),
                ArithOp::Div => out.push(format!(
                    "let __b = {pop}(&mut stack); let __a = {pop}(&mut stack); stack.push({RT}::Value::{ctor}(__a / __b));"
                )),
                ArithOp::Rem => out.push(format!(
                    "let __b = {pop}(&mut stack); let __a = {pop}(&mut stack); stack.push({RT}::Value::{ctor}(__a % __b));"
                )),
                _ => unreachable!("bit/shift arithmetic on floats"),
            }
        }
        Byte | Char | Short => unreachable!("arith on narrow kind"),
    }
}

fn emit_int_or_long_arith(op: ArithOp, is_long: bool, fault: &str, out: &mut Vec<String>) {
    let pop_self = if is_long {
        "::j2r_runtime::pop_long"
    } else {
        "::j2r_runtime::pop_int"
    };
    let ctor = if is_long { "Long" } else { "Int" };
    match op {
        ArithOp::Neg => out.push(format!(
            "let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}(__a.wrapping_neg()));"
        )),
        ArithOp::Add => out.push(format!(
            "let __b = {pop_self}(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}(__a.wrapping_add(__b)));"
        )),
        ArithOp::Sub => out.push(format!(
            "let __b = {pop_self}(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}(__a.wrapping_sub(__b)));"
        )),
        ArithOp::Mul => out.push(format!(
            "let __b = {pop_self}(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}(__a.wrapping_mul(__b)));"
        )),
        ArithOp::Div => out.push(format!(
            "let __b = {pop_self}(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}({RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::{}(__a, __b))));",
            if is_long { "long_div" } else { "int_div" }
        )),
        ArithOp::Rem => out.push(format!(
            "let __b = {pop_self}(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}({RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::{}(__a, __b))));",
            if is_long { "long_rem" } else { "int_rem" }
        )),
        ArithOp::And | ArithOp::Or | ArithOp::Xor => {
            let rust_op = match op {
                ArithOp::And => "&",
                ArithOp::Or => "|",
                _ => "^",
            };
            out.push(format!(
                "let __b = {pop_self}(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}(__a {rust_op} __b));"
            ));
        }
        ArithOp::Shl | ArithOp::Shr | ArithOp::UShr => {
            // shift amount is always an int; mask per JVMS
            let mask = if is_long { 63 } else { 31 };
            let shift_fn = match op {
                ArithOp::Shl => "wrapping_shl",
                ArithOp::Shr => "wrapping_shr",
                _ => "wrapping_shr",
            };
            if op == ArithOp::UShr {
                // logical shift on the unsigned reinterpretation
                let uty = if is_long { "u64" } else { "u32" };
                out.push(format!(
                    "let __b = ::j2r_runtime::pop_int(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}(((__a as {uty}).{shift_fn}((__b & {mask}) as u32)) as {}));",
                    if is_long { "i64" } else { "i32" }
                ));
            } else {
                out.push(format!(
                    "let __b = ::j2r_runtime::pop_int(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}(__a.{shift_fn}((__b & {mask}) as u32)));"
                ));
            }
        }
    }
}

fn emit_cmp(kind: CmpKind, out: &mut Vec<String>) {
    match kind {
        CmpKind::LCmp => {
            out.push(
                "let __b = ::j2r_runtime::pop_long(&mut stack); let __a = ::j2r_runtime::pop_long(&mut stack);"
                    .into(),
            );
            out.push(
                "stack.push(::j2r_runtime::Value::Int(match __a.cmp(&__b) { ::core::cmp::Ordering::Less => -1i32, ::core::cmp::Ordering::Greater => 1i32, ::core::cmp::Ordering::Equal => 0i32 }));"
                    .into(),
            );
        }
        CmpKind::FCmpL | CmpKind::FCmpG | CmpKind::DCmpL | CmpKind::DCmpG => {
            let pop = if matches!(kind, CmpKind::FCmpL | CmpKind::FCmpG) {
                "::j2r_runtime::pop_float"
            } else {
                "::j2r_runtime::pop_double"
            };
            let nan_val: i32 = if matches!(kind, CmpKind::FCmpG | CmpKind::DCmpG) {
                1
            } else {
                -1
            };
            out.push(format!(
                "let __b = {pop}(&mut stack); let __a = {pop}(&mut stack);"
            ));
            out.push(format!(
                "stack.push(::j2r_runtime::Value::Int(if __a.is_nan() || __b.is_nan() {{ {nan_val}i32 }} else {{ match __a.partial_cmp(&__b) {{ ::core::option::Option::Some(::core::cmp::Ordering::Less) => -1i32, ::core::option::Option::Some(::core::cmp::Ordering::Greater) => 1i32, _ => 0i32 }} }}));"
            ));
        }
    }
}

fn emit_convert(conv: Conv, out: &mut Vec<String>) {
    use NumKind::*;
    let from_pop = match conv.from {
        Int => "::j2r_runtime::pop_int",
        Long => "::j2r_runtime::pop_long",
        Float => "::j2r_runtime::pop_float",
        Double => "::j2r_runtime::pop_double",
        _ => unreachable!("conversion source"),
    };
    // Rust's `as` casts match Java exactly here: float/double -> int/long
    // saturate with NaN -> 0; int -> narrow types wrap; int -> char
    // zero-extends.
    let (ctor, cast) = match (conv.from, conv.to) {
        (Int, Long) => ("Long", " as i64"),
        (Int, Float) => ("Float", " as f32"),
        (Int, Double) => ("Double", " as f64"),
        (Int, Byte) => ("Int", " as i8 as i32"),
        (Int, Char) => ("Int", " as u16 as i32"),
        (Int, Short) => ("Int", " as i16 as i32"),
        (Long, Int) => ("Int", " as i32"),
        (Long, Float) => ("Float", " as f32"),
        (Long, Double) => ("Double", " as f64"),
        (Float, Int) => ("Int", " as i32"),
        (Float, Long) => ("Long", " as i64"),
        (Float, Double) => ("Double", " as f64"),
        (Double, Int) => ("Int", " as i32"),
        (Double, Long) => ("Long", " as i64"),
        (Double, Float) => ("Float", " as f32"),
        _ => unreachable!("conversion pair"),
    };
    out.push(format!(
        "let __a = {from_pop}(&mut stack); stack.push({RT}::Value::{ctor}(__a{cast}));"
    ));
}
