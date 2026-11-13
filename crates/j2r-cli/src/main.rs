//! `j2r` — command line driver for the J2R Java-to-Rust compiler.
//!
//! * `j2r compile <inputs...> -o DIR` — translate `.class` files into a Rust
//!   crate (source only).
//! * `j2r build <inputs...> -o DIR` — translate *and* build a native binary.
//! * `j2r test <File.java>` — differential test: compile with `javac`, run
//!   with `java`, compile the class files with j2r, run the native binary,
//!   and compare **stdout, stderr and exit code** (stderr with JVM
//!   stack-trace frames normalized away — see [`compare`]).

#![forbid(unsafe_code)]
mod compare;

use compare::{streams_match, RunResult};
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::{Parser, Subcommand};
use j2r_codegen::Options;

#[derive(Parser)]
#[command(
    name = "j2r",
    version,
    about = "J2R — a Java-to-Rust compiler (JVM bytecode -> native Rust)",
    after_help = "Use 'j2r <command> --help' for command-specific options."
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Translate .class files into a Rust crate.
    Compile {
        /// Input .class files (empty when compiling from a classpath).
        inputs: Vec<PathBuf>,
        /// Output crate directory.
        #[arg(short, long, default_value = "j2r-out")]
        out: PathBuf,
        /// Classpath entries (dirs, .jar, .zip), searched in order for
        /// classes not named on the command line (M3 Wave 1). Separator is
        /// ':' on Unix, ';' on Windows (drive-letter safe) — audit A7.
        #[cfg_attr(windows, arg(short = 'p', long = "classpath", value_delimiter = ';'))]
        #[cfg_attr(
            not(windows),
            arg(short = 'p', long = "classpath", value_delimiter = ':')
        )]
        classpath: Vec<PathBuf>,
        /// Main class (internal `a/b/C` or dotted `a.b.C` form).
        #[arg(long)]
        main_class: Option<String>,
        /// Name of the generated crate.
        #[arg(long, default_value = "j2r-app")]
        crate_name: String,
        /// Wave-4 investigation (M2_PLAN §6, default OFF): attempt
        /// structured CFG method bodies; unsupported shapes fall back to
        /// the approved pc-dispatch body per method.
        #[arg(long)]
        emit_cfg: bool,
        /// The registered typed-operand lever (docs/TYPED_REGISTER_READINESS.md
        /// §5, default OFF): per-epoch typed operand storage for planned
        /// methods; unsupported shapes fall back per method. Mutually
        /// exclusive with --emit-cfg.
        #[arg(long)]
        emit_typed: bool,
        /// M5 T05 (docs/M5_SCALE.md §3, default OFF): emit a cargo
        /// workspace with at most this many classes per crate (a shared
        /// support crate holds the statics pool, the global link table and
        /// the dispatch slot). Bounds one rustc process's memory; the farm
        /// census showed ~4.2 MB/class makes a single crate die near 1000
        /// classes on a 4 GB machine. Requires --main-class.
        #[arg(long)]
        partition_classes: Option<usize>,
    },
    /// Translate .class files and build a native binary with cargo.
    Build {
        /// Input .class files (empty when compiling from a classpath).
        inputs: Vec<PathBuf>,
        /// Output crate directory.
        #[arg(short, long, default_value = "j2r-out")]
        out: PathBuf,
        /// Classpath entries (dirs, .jar, .zip), searched in order for
        /// classes not named on the command line (M3 Wave 1). Separator is
        /// ':' on Unix, ';' on Windows (drive-letter safe) — audit A7.
        #[cfg_attr(windows, arg(short = 'p', long = "classpath", value_delimiter = ';'))]
        #[cfg_attr(
            not(windows),
            arg(short = 'p', long = "classpath", value_delimiter = ':')
        )]
        classpath: Vec<PathBuf>,
        /// Main class (internal `a/b/C` or dotted `a.b.C` form).
        #[arg(long)]
        main_class: Option<String>,
        /// Name of the generated crate.
        #[arg(long, default_value = "j2r-app")]
        crate_name: String,
        /// Build in release mode.
        #[arg(long)]
        release: bool,
        /// Wave-4 investigation (M2_PLAN §6, default OFF): attempt
        /// structured CFG method bodies; unsupported shapes fall back to
        /// the approved pc-dispatch body per method.
        #[arg(long)]
        emit_cfg: bool,
        /// The registered typed-operand lever, default OFF. Mutually
        /// exclusive with --emit-cfg.
        #[arg(long)]
        emit_typed: bool,
        /// M5 T05 (docs/M5_SCALE.md §3, default OFF): emit a cargo
        /// workspace with at most this many classes per crate. Requires
        /// --main-class.
        #[arg(long)]
        partition_classes: Option<usize>,
    },
    /// Differential test: javac + java vs j2r + cargo, comparing stdout,
    /// stderr and exit code.
    Test {
        /// Java source file to test.
        file: PathBuf,
        /// Build the generated Rust crate in release mode.
        #[arg(long)]
        release: bool,
        /// M5 T04: compile the program's classes into a partitioned
        /// workspace with at most this many classes per crate before the
        /// comparison (default OFF = single-crate emission).
        #[arg(long)]
        partition_classes: Option<usize>,
        /// Program arguments passed to both runs (after `--`).
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Wave-4 lift report (M2_PLAN §6): for each input class, print the
    /// per-method CFG-lift decision (structured body vs dispatch fallback
    /// with the exact unsupported-shape reason).
    CfgReport {
        /// Input .class files.
        inputs: Vec<PathBuf>,
        /// EXTERNAL-HARNESS H-1: instead of the CFG-lift table, print the
        /// sorted, deduplicated JDK member surface the inputs need, each
        /// site marked ok/MISSING by the compiler's own resolution law.
        #[arg(long)]
        surface: bool,
        /// EXTERNAL-HARNESS H-2 (with --surface): walk EVERY class,
        /// report every unresolved member, then exit 1 — the first
        /// unresolved reference is only the first.
        #[arg(long)]
        diagnostics: bool,
    },
    /// Wave-3 shape report (M2_PLAN §5.2): verify each input class and
    /// print the per-method typed-storage specialization decision table.
    ShapeReport {
        /// Input .class files.
        inputs: Vec<PathBuf>,
    },
    /// Typed-register readiness report (the M2_SHAPE_REPORT registered
    /// follow-up): per-method live-range epoch analysis + CFG-lift
    /// status — the measurement the typed-operand GO/NO-GO is decided
    /// on. Observes and decides; changes no emission.
    ReadinessReport {
        /// Input .class files.
        inputs: Vec<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Cmd::Compile {
            inputs,
            out,
            classpath,
            main_class,
            crate_name,
            emit_cfg,
            emit_typed,
            partition_classes,
        } => {
            if emit_cfg && emit_typed {
                eprintln!("j2r: error: --emit-cfg and --emit-typed are mutually exclusive (the typed wave composes with the dispatch body only)");
                2
            } else {
                cmd_compile(
                    &inputs,
                    &out,
                    &classpath,
                    main_class,
                    &crate_name,
                    emit_cfg,
                    emit_typed,
                    partition_classes,
                )
            }
        }
        Cmd::Build {
            inputs,
            out,
            classpath,
            main_class,
            crate_name,
            release,
            emit_cfg,
            emit_typed,
            partition_classes,
        } => {
            if emit_cfg && emit_typed {
                eprintln!("j2r: error: --emit-cfg and --emit-typed are mutually exclusive (the typed wave composes with the dispatch body only)");
                2
            } else {
                cmd_build(
                    &inputs,
                    &out,
                    &classpath,
                    main_class,
                    &crate_name,
                    release,
                    emit_cfg,
                    emit_typed,
                    partition_classes,
                )
            }
        }
        Cmd::Test {
            file,
            release,
            partition_classes,
            args,
        } => cmd_test(&file, release, partition_classes, &args),
        Cmd::CfgReport {
            inputs,
            surface,
            diagnostics,
        } => {
            if surface {
                cmd_surface_report(&inputs, diagnostics)
            } else if diagnostics {
                eprintln!("j2r: --diagnostics requires --surface");
                2
            } else {
                cmd_cfg_report(&inputs)
            }
        }
        Cmd::ShapeReport { inputs } => cmd_shape_report(&inputs),
        Cmd::ReadinessReport { inputs } => cmd_readiness_report(&inputs),
    };
    std::process::exit(code);
}

/// Wave-3 step 3a: the per-method specialization decision table over whole
/// compiled classes (written evidence, never a silent claim).
fn cmd_cfg_report(inputs: &[PathBuf]) -> i32 {
    let opts = j2r_codegen::Options {
        inputs: inputs.to_vec(),
        classpath: Vec::new(),
        main_class: None,
        crate_name: "j2r-cfg-report".to_string(),
        out_dir: std::env::temp_dir().join("j2r-cfg-report"),
        emit_cfg: false,
        emit_typed: false,
        partition: None,
    };
    match j2r_codegen::compile(&opts) {
        Ok(prog) => {
            let rows = j2r_codegen::cfg::plan_program(&prog);
            print!("{}", j2r_codegen::cfg::render_report(&rows));
            for r in &rows {
                println!(
                    "| {} | {} | {} | {} |",
                    r.class,
                    r.method,
                    if r.lifted { "LIFT" } else { "DISPATCH" },
                    r.reason
                        .map(|x| format!("{x:?}"))
                        .unwrap_or_else(|| "-".to_string()),
                );
            }
            0
        }
        Err(e) => {
            eprintln!("j2r: {e}");
            1
        }
    }
}

fn cmd_shape_report(inputs: &[PathBuf]) -> i32 {
    let opts = j2r_codegen::Options {
        inputs: inputs.to_vec(),
        classpath: Vec::new(),
        main_class: None,
        crate_name: "j2r-shape-report".to_string(),
        out_dir: std::env::temp_dir().join("j2r-shape-report"),
        emit_cfg: false,
        emit_typed: false,
        partition: None,
    };
    match j2r_codegen::compile(&opts) {
        Ok(prog) => {
            let rows = j2r_codegen::specialize::plan_program(&prog);
            print!("{}", j2r_codegen::specialize::render_report(&rows));
            for r in &rows {
                println!(
                    "| {} | {} | {} | {:?} | {}/{} | {}/{} |",
                    r.class,
                    r.method,
                    if r.spec.specialize {
                        "SPEC"
                    } else {
                        "FALLBACK"
                    },
                    r.spec.fallback,
                    r.spec.stack_counts.1,
                    r.spec.stack_counts.0,
                    r.spec.local_counts.1,
                    r.spec.local_counts.0,
                );
            }
            0
        }
        Err(e) => {
            eprintln!("j2r: {e}");
            1
        }
    }
}

/// Typed-register readiness report: per-method epoch analysis (the M2
/// shape report's registered live-range refinement) + the real CFG-lift
/// decision, so the typed-operand GO/NO-GO is measured, not assumed.
fn cmd_readiness_report(inputs: &[PathBuf]) -> i32 {
    let opts = j2r_codegen::Options {
        inputs: inputs.to_vec(),
        classpath: Vec::new(),
        main_class: None,
        crate_name: "j2r-readiness-report".to_string(),
        out_dir: std::env::temp_dir().join("j2r-readiness-report"),
        emit_cfg: false,
        emit_typed: false,
        partition: None,
    };
    match j2r_codegen::compile(&opts) {
        Ok(prog) => {
            let rows = j2r_codegen::readiness::plan_program(&prog);
            print!("{}", j2r_codegen::readiness::render_report(&rows));
            for r in &rows {
                println!(
                    "| {} | {} | {:?} | {} | {:.1}% | {}/{} | {}/{} | {}/{} |",
                    r.class,
                    r.method,
                    r.verdict,
                    r.lift_reason
                        .as_ref()
                        .map(|x| format!("{x:?}"))
                        .unwrap_or_else(|| "-".to_string()),
                    if r.prim_traffic + r.ref_traffic == 0 {
                        0.0
                    } else {
                        r.prim_traffic as f64 * 100.0 / (r.prim_traffic + r.ref_traffic) as f64
                    },
                    r.epochs_prim,
                    r.epochs_total,
                    r.positions_epoch_clean,
                    r.positions_total,
                    r.positions_pool_stable,
                    r.positions_total,
                );
            }
            0
        }
        Err(e) => {
            eprintln!("j2r: {e}");
            1
        }
    }
}

#[allow(clippy::too_many_arguments)] // 8 args: the compile surface, same precedent as cmd_build
fn compile_crate(
    inputs: &[PathBuf],
    out: &Path,
    classpath: &[PathBuf],
    main_class: Option<String>,
    crate_name: &str,
    emit_cfg: bool,
    emit_typed: bool,
    partition: Option<usize>,
) -> Result<(PathBuf, j2r_codegen::manifest::EmitStats), String> {
    let opts = Options {
        inputs: inputs.to_vec(),
        classpath: classpath.to_vec(),
        main_class,
        crate_name: crate_name.to_string(),
        out_dir: out.to_path_buf(),
        emit_cfg,
        emit_typed,
        partition,
    };
    j2r_codegen::compile_to_crate_stats(&opts).map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)] // 8 args: the compile surface, same precedent as cmd_build
fn cmd_compile(
    inputs: &[PathBuf],
    out: &Path,
    classpath: &[PathBuf],
    main_class: Option<String>,
    crate_name: &str,
    emit_cfg: bool,
    emit_typed: bool,
    partition_classes: Option<usize>,
) -> i32 {
    match compile_crate(
        inputs,
        out,
        classpath,
        main_class,
        crate_name,
        emit_cfg,
        emit_typed,
        partition_classes,
    ) {
        Ok((dir, stats)) => {
            println!("generated Rust crate: {}", dir.display());
            println!("{}", stats.summary());
            if let Some(report) = j2r_codegen::stages_report() {
                println!("{report}");
            }
            println!(
                "build it with: cargo build --manifest-path {}/Cargo.toml",
                dir.display()
            );
            0
        }
        Err(e) => {
            eprintln!("j2r: error: {e}");
            1
        }
    }
}

fn cargo_binary() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

// 8 args = the build command's natural surface (paths + naming + release
// + the two emission-mode flags); bundling them would move the CLI's
// grammar for lint-count reasons alone.
#[allow(clippy::too_many_arguments)]
fn cmd_build(
    inputs: &[PathBuf],
    out: &Path,
    classpath: &[PathBuf],
    main_class: Option<String>,
    crate_name: &str,
    release: bool,
    emit_cfg: bool,
    emit_typed: bool,
    partition_classes: Option<usize>,
) -> i32 {
    let (dir, stats) = match compile_crate(
        inputs,
        out,
        classpath,
        main_class,
        crate_name,
        emit_cfg,
        emit_typed,
        partition_classes,
    ) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("j2r: error: {e}");
            return 1;
        }
    };
    eprintln!("{}", stats.summary());
    let mut cmd = Command::new(cargo_binary());
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(dir.join("Cargo.toml"))
        .arg("--quiet");
    if release {
        cmd.arg("--release");
    }
    let status = match cmd.status() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("j2r: failed to run cargo ({e}); is Rust installed?");
            return 1;
        }
    };
    if !status.success() {
        eprintln!("j2r: cargo build failed");
        return 1;
    }
    let profile = if release { "release" } else { "debug" };
    let bin = dir.join("target").join(profile).join(bin_name(crate_name));
    println!("native binary: {}", bin.display());
    0
}

fn cmd_test(file: &Path, release: bool, partition: Option<usize>, args: &[String]) -> i32 {
    if !file.exists() {
        eprintln!("j2r: error: {} does not exist", file.display());
        return 2;
    }
    let (javac, java) = match locate_java() {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("j2r: error: {e}");
            return 2;
        }
    };

    let work = std::env::temp_dir().join(format!("j2r-test-{}-{}", std::process::id(), nanos()));
    let classes_dir = work.join("classes");
    let out_dir = work.join("out");
    std::fs::create_dir_all(&classes_dir).expect("create work dir");

    // Audit A3: the main class is selected ONCE (source-stem preference,
    // loud ambiguity error) and used for BOTH the JVM run and the native
    // compile — the two sides cannot pick different mains anymore.
    let source_stem = file.file_stem().map(|s| s.to_string_lossy().into_owned());
    let selected_main = std::cell::RefCell::new(None::<String>);

    let run_java = || -> Result<RunResult, String> {
        // 1. javac
        let status = Command::new(&javac)
            .arg("-d")
            .arg(&classes_dir)
            .arg(file)
            .status()
            .map_err(|e| format!("failed to run javac: {e}"))?;
        if !status.success() {
            return Err("javac failed".to_string());
        }
        // 2. find the main class among the produced class files
        let main_class = select_main_class(&classes_dir, source_stem.as_deref())?;
        *selected_main.borrow_mut() = Some(main_class.clone());
        // 3. run java
        run_java_program(&java, &classes_dir, &main_class, args)
    };

    let run_rust = || -> Result<RunResult, String> {
        // 1. collect class files
        let mut class_files = Vec::new();
        collect_class_files(&classes_dir, &mut class_files);
        // 2. j2r compile — with the SAME main class the JVM side ran
        let main_class = selected_main
            .borrow()
            .clone()
            .ok_or_else(|| "internal: main class not selected yet".to_string())?;
        let (dir, _stats) = compile_crate(
            &class_files,
            &out_dir,
            &[],
            Some(main_class),
            "j2r-test-app",
            false,
            false,
            partition,
        )?;
        // 3. cargo build
        let mut cmd = Command::new(cargo_binary());
        cmd.arg("build")
            .arg("--manifest-path")
            .arg(dir.join("Cargo.toml"))
            .arg("--quiet");
        if release {
            cmd.arg("--release");
        }
        let status = cmd
            .status()
            .map_err(|e| format!("failed to run cargo: {e}"))?;
        if !status.success() {
            return Err("cargo build of generated crate failed".to_string());
        }
        // 4. run the binary
        let profile = if release { "release" } else { "debug" };
        let bin = dir
            .join("target")
            .join(profile)
            .join(bin_name("j2r-test-app"));
        let output = Command::new(&bin)
            .args(args)
            .output()
            .map_err(|e| format!("failed to run {}: {e}", bin.display()))?;
        Ok(RunResult {
            stdout: output.stdout,
            stderr: output.stderr,
            code: exit_code(&output.status),
        })
    };

    let java_res = run_java();
    let rust_res = run_rust();

    let (ok, report) = match (java_res, rust_res) {
        (Ok(j), Ok(r)) => match streams_match(&j, &r) {
            Ok(()) => (true, String::new()),
            Err(report) => (false, report),
        },
        (Err(e), _) | (_, Err(e)) => (false, format!("{e}\n")),
    };

    if ok {
        let _ = std::fs::remove_dir_all(&work);
        println!("PASS {}", file.display());
        0
    } else {
        eprintln!("FAIL {}", file.display());
        eprintln!("{report}");
        eprintln!("work directory kept at {}", work.display());
        1
    }
}

fn run_java_program(
    java: &Path,
    classes_dir: &Path,
    main_class: &str,
    args: &[String],
) -> Result<RunResult, String> {
    let dotted = main_class.replace('/', ".");
    let output = Command::new(java)
        .arg("-XX:+EnableDynamicAgentLoading")
        // EXTERNAL-HARNESS O6 (owner-verified 2026-11-12): the reference
        // channel must pin its own charset — j2r's differential harness
        // compares raw bytes, so an ambient C/POSIX locale made the JDK
        // write ASCII `?` for non-ASCII and produced five phantom
        // failures (BigJson, CharClassify, StringExt, StringIoSemantics,
        // Utf16). JEP 400 made UTF-8 the JDK 18+ default, but the pin
        // must not depend on the JDK version either.
        .arg("-Dfile.encoding=UTF-8")
        .arg("-cp")
        .arg(classes_dir)
        .arg(&dotted)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run java: {e}"))?;
    Ok(RunResult {
        stdout: output.stdout,
        stderr: output.stderr,
        code: exit_code(&output.status),
    })
}

fn exit_code(status: &std::process::ExitStatus) -> i32 {
    status.code().unwrap_or(-1)
}

/// Picks the main class for a differential run (audit A3: ONE selection,
/// used for both the JVM run and the native compile — no more independent
/// fallbacks that can drift apart).
///
/// Rules: with zero main-bearing classes → error; with exactly one → it.
/// With several, the class whose **simple name matches the source-file
/// stem** wins (the JVM's `java <stem>` behavior); if no candidate or
/// several candidates match the stem, the ambiguity is a loud error that
/// lists every candidate — never a silent `read_dir`-order pick.
///
/// Candidates are sorted, so neither the choice nor the error text depends
/// on directory order.
fn select_main_class(classes_dir: &Path, source_stem: Option<&str>) -> Result<String, String> {
    let mut class_files = Vec::new();
    collect_class_files(classes_dir, &mut class_files);
    let mut candidates: Vec<String> = Vec::new();
    for cf_path in &class_files {
        let bytes = std::fs::read(cf_path).map_err(|e| e.to_string())?;
        let cf = j2r_classfile::ClassFile::parse(&bytes)
            .map_err(|e| format!("parsing {}: {e}", cf_path.display()))?;
        let has_main = cf
            .method("main", "([Ljava/lang/String;)V")
            .map(|m| m.is_static())
            .unwrap_or(false);
        if has_main {
            candidates.push(cf.this_class.clone());
        }
    }
    candidates.sort();
    match candidates.len() {
        0 => Err("no class with `public static void main(String[])` found".to_string()),
        1 => Ok(candidates.remove(0)),
        _ => {
            let stem_matches: Vec<&String> = candidates
                .iter()
                .filter(|c| {
                    source_stem
                        .map(|stem| c.rsplit('/').next() == Some(stem))
                        .unwrap_or(false)
                })
                .collect();
            match stem_matches.len() {
                1 => Ok(stem_matches[0].clone()),
                _ => Err(format!(
                    "multiple classes declare `public static void main(String[])`: {}; \
                     pass --main-class or rename so exactly one matches the source file",
                    candidates.join(", ")
                )),
            }
        }
    }
}

fn collect_class_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_class_files(&path, out);
            } else if path.extension().map(|e| e == "class").unwrap_or(false) {
                out.push(path);
            }
        }
    }
}

fn locate_java() -> Result<(PathBuf, PathBuf), String> {
    let javac_env = std::env::var("J2R_JAVAC").ok().map(PathBuf::from);
    let java_env = std::env::var("J2R_JAVA").ok().map(PathBuf::from);
    let javac = match javac_env {
        Some(p) => p,
        None => which("javac").ok_or_else(|| "javac not found on PATH".to_string())?,
    };
    let java = match java_env {
        Some(p) => p,
        None => which("java").unwrap_or_else(|| {
            // fall back to a sibling of javac
            javac
                .parent()
                .map(|d| d.join("java"))
                .unwrap_or_else(|| PathBuf::from("java"))
        }),
    };
    Ok((javac, java))
}

fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").ok()?;
    // std::env::split_paths uses the platform separator (':' Unix, ';'
    // Windows) — audit A7.
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    // common Linux JDK locations (Unix-only scan — audit A7)
    #[cfg(unix)]
    if let Ok(entries) = std::fs::read_dir("/usr/lib/jvm") {
        for entry in entries.flatten() {
            let candidate = entry.path().join("bin").join(program);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn bin_name(crate_name: &str) -> String {
    // must match the emitter's `sanitize_crate_ident` (cargo normalizes '-' too)
    crate_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// EXTERNAL-HARNESS H-1/H-2 (jbox2d process findings): the JDK member
/// surface the inputs need, diffed against the NATIVES table by the
/// compiler's own resolution law. Full mode = the one-page surface
/// report (H-1); `--diagnostics` = only the MISSING rows, exit 1 (H-2,
/// collect-all-then-fail — the first unresolved reference is only the
/// first).
fn cmd_surface_report(inputs: &[PathBuf], diagnostics: bool) -> i32 {
    let opts = j2r_codegen::Options {
        inputs: inputs.to_vec(),
        classpath: Vec::new(),
        main_class: None,
        crate_name: "j2r-surface".to_string(),
        out_dir: std::env::temp_dir().join("j2r-surface"),
        emit_cfg: false,
        emit_typed: false,
        partition: None,
    };
    match j2r_codegen::compile_unchecked(&opts) {
        Ok(prog) => {
            // compile_UNCHECKED on purpose (H-2's whole point): the
            // verify+validate gate hard-fails on the FIRST unsupported
            // shape; the surface walk must see past it and name every
            // gap at once. The classification predicates are the same
            // functions the gate calls, so the report can never
            // disagree with a later real compile.
            let (report, missing) = j2r_codegen::surface::render(&prog, diagnostics);
            print!("{report}");
            if missing > 0 {
                1
            } else {
                0
            }
        }
        Err(e) => {
            // parse/plan-level failures (malformed classfile, IO) — the
            // walk never got a program to inspect.
            eprintln!("j2r: {e}");
            1
        }
    }
}
