//! `j2r` — command line driver for the J2R Java-to-Rust compiler.
//!
//! * `j2r compile <inputs...> -o DIR` — translate `.class` files into a Rust
//!   crate (source only).
//! * `j2r build <inputs...> -o DIR` — translate *and* build a native binary.
//! * `j2r test <File.java>` — differential test: compile with `javac`, run
//!   with `java`, compile the class files with j2r, run the native binary,
//!   compare stdout and exit code.

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
        /// Input .class files.
        inputs: Vec<PathBuf>,
        /// Output crate directory.
        #[arg(short, long, default_value = "j2r-out")]
        out: PathBuf,
        /// Main class (internal `a/b/C` or dotted `a.b.C` form).
        #[arg(long)]
        main_class: Option<String>,
        /// Name of the generated crate.
        #[arg(long, default_value = "j2r-app")]
        crate_name: String,
    },
    /// Translate .class files and build a native binary with cargo.
    Build {
        /// Input .class files.
        inputs: Vec<PathBuf>,
        /// Output crate directory.
        #[arg(short, long, default_value = "j2r-out")]
        out: PathBuf,
        /// Main class (internal `a/b/C` or dotted `a.b.C` form).
        #[arg(long)]
        main_class: Option<String>,
        /// Name of the generated crate.
        #[arg(long, default_value = "j2r-app")]
        crate_name: String,
        /// Build in release mode.
        #[arg(long)]
        release: bool,
    },
    /// Differential test: javac + java vs j2r + cargo, comparing stdout and
    /// exit code.
    Test {
        /// Java source file to test.
        file: PathBuf,
        /// Build the generated Rust crate in release mode.
        #[arg(long)]
        release: bool,
        /// Program arguments passed to both runs (after `--`).
        #[arg(last = true)]
        args: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Cmd::Compile {
            inputs,
            out,
            main_class,
            crate_name,
        } => cmd_compile(&inputs, &out, main_class, &crate_name),
        Cmd::Build {
            inputs,
            out,
            main_class,
            crate_name,
            release,
        } => cmd_build(&inputs, &out, main_class, &crate_name, release),
        Cmd::Test {
            file,
            release,
            args,
        } => cmd_test(&file, release, &args),
    };
    std::process::exit(code);
}

fn compile_crate(
    inputs: &[PathBuf],
    out: &Path,
    main_class: Option<String>,
    crate_name: &str,
) -> Result<PathBuf, String> {
    let opts = Options {
        inputs: inputs.to_vec(),
        main_class,
        crate_name: crate_name.to_string(),
        out_dir: out.to_path_buf(),
    };
    j2r_codegen::compile_to_crate(&opts).map_err(|e| e.to_string())
}

fn cmd_compile(
    inputs: &[PathBuf],
    out: &Path,
    main_class: Option<String>,
    crate_name: &str,
) -> i32 {
    match compile_crate(inputs, out, main_class, crate_name) {
        Ok(dir) => {
            println!("generated Rust crate: {}", dir.display());
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

fn cmd_build(
    inputs: &[PathBuf],
    out: &Path,
    main_class: Option<String>,
    crate_name: &str,
    release: bool,
) -> i32 {
    let dir = match compile_crate(inputs, out, main_class, crate_name) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("j2r: error: {e}");
            return 1;
        }
    };
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

fn cmd_test(file: &Path, release: bool, args: &[String]) -> i32 {
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

    let run_java = || -> Result<(Vec<u8>, i32), String> {
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
        let main_class = find_main_class(&classes_dir)?;
        // 3. run java
        run_java_program(&java, &classes_dir, &main_class, args)
    };

    let run_rust = || -> Result<(Vec<u8>, i32), String> {
        // 1. collect class files
        let mut class_files = Vec::new();
        collect_class_files(&classes_dir, &mut class_files);
        // 2. j2r compile
        let dir = compile_crate(&class_files, &out_dir, None, "j2r-test-app")?;
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
        Ok((output.stdout, exit_code(&output.status)))
    };

    let java_res = run_java();
    let rust_res = run_rust();

    let (ok, report) = match (java_res, rust_res) {
        (Ok((j_out, j_code)), Ok((r_out, r_code))) => {
            let out_ok = j_out == r_out;
            let code_ok = j_code == r_code;
            let mut report = String::new();
            if !out_ok {
                report.push_str(&stdout_diff(&j_out, &r_out));
            }
            if !code_ok {
                report.push_str(&format!("exit codes differ: java={j_code} rust={r_code}\n"));
            }
            (out_ok && code_ok, report)
        }
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
) -> Result<(Vec<u8>, i32), String> {
    let dotted = main_class.replace('/', ".");
    let output = Command::new(java)
        .arg("-XX:+EnableDynamicAgentLoading")
        .arg("-cp")
        .arg(classes_dir)
        .arg(&dotted)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run java: {e}"))?;
    Ok((output.stdout, exit_code(&output.status)))
}

fn exit_code(status: &std::process::ExitStatus) -> i32 {
    status.code().unwrap_or(-1)
}

fn find_main_class(classes_dir: &Path) -> Result<String, String> {
    let mut class_files = Vec::new();
    collect_class_files(classes_dir, &mut class_files);
    let mut fallback: Option<String> = None;
    for cf_path in &class_files {
        let bytes = std::fs::read(cf_path).map_err(|e| e.to_string())?;
        let cf = j2r_classfile::ClassFile::parse(&bytes)
            .map_err(|e| format!("parsing {}: {e}", cf_path.display()))?;
        let has_main = cf
            .method("main", "([Ljava/lang/String;)V")
            .map(|m| m.is_static())
            .unwrap_or(false);
        if has_main {
            // prefer the class matching the source file name
            let simple = cf.this_class.rsplit('/').next().unwrap_or("");
            let _ = simple;
            if fallback.is_none() {
                fallback = Some(cf.this_class.clone());
            }
        }
    }
    fallback.ok_or_else(|| "no class with `public static void main(String[])` found".to_string())
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

fn stdout_diff(expected: &[u8], actual: &[u8]) -> String {
    use std::fmt::Write as _;
    let exp = String::from_utf8_lossy(expected);
    let act = String::from_utf8_lossy(actual);
    let exp_lines: Vec<&str> = exp.lines().collect();
    let act_lines: Vec<&str> = act.lines().collect();
    let mut out = String::from("stdout differs:\n");
    let n = exp_lines.len().max(act_lines.len());
    let mut shown = 0;
    for i in 0..n {
        let e = exp_lines.get(i).copied().unwrap_or("<missing>");
        let a = act_lines.get(i).copied().unwrap_or("<missing>");
        if e != a {
            let _ = writeln!(out, "  line {}: expected: {e:?}", i + 1);
            let _ = writeln!(out, "  line {}: actual:   {a:?}", i + 1);
            shown += 1;
            if shown >= 10 {
                let _ = writeln!(out, "  ...");
                break;
            }
        }
    }
    out
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
    for dir in path.split(':') {
        let candidate = Path::new(dir).join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    // common Linux JDK locations
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
