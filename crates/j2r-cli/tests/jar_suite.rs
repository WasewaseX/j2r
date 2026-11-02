//! M3 Wave 1 differential suite: fixtures compiled from real JAR files.
//!
//! Each fixture directory under `tests/suite_jar/<Name>/` contains Java
//! sources, a `main` file naming the entry class, and (optionally) a
//! `kind` file selecting the archive-building recipe:
//! * `jar` (default) — compile every `.java` under the fixture dir, pack
//!   all classes into `app.jar` with the JDK `jar` tool, compile with
//!   `j2r compile -p app.jar --main-class <main>` (no explicit inputs),
//!   and compare the generated binary against `java -cp app.jar <main>`
//!   on all three channels.
//! * `dupdir` — the same class exists in a classpath *directory* (first
//!   entry) and inside the jar (second entry); both JVMs must resolve the
//!   directory copy (first-match-wins, M3_PLAN §4.5).
//!
//! The generated crate must be byte-independent of how the classes were
//! delivered — the closure loader hands the same bytes to the same
//! parser (M3_PLAN §4.6 determinism guard).

use std::path::{Path, PathBuf};
use std::process::Command;

struct RunResult {
    stdout: String,
    stderr: String,
    code: i32,
}

fn locate_java() -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let home = std::env::var("JAVA_HOME").unwrap_or_default();
    let base = if home.is_empty() {
        PathBuf::from("/")
    } else {
        PathBuf::from(&home)
    };
    let javac = base.join("bin/javac");
    let java = base.join("bin/java");
    let jar = base.join("bin/jar");
    let ok = |p: &Path| p.is_file() || which_ok(p);
    if ok(&javac) && ok(&java) && ok(&jar) {
        return Ok((javac, java, jar));
    }
    // Fall back to PATH.
    let ok2 = |n: &str| {
        Command::new(n)
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if ok2("javac") && ok2("java") && ok2("jar") {
        return Ok((
            PathBuf::from("javac"),
            PathBuf::from("java"),
            PathBuf::from("jar"),
        ));
    }
    Err("javac/java/jar not found (set JAVA_HOME)".to_string())
}

fn which_ok(p: &Path) -> bool {
    p.exists()
}

fn run(cmd: &mut Command) -> Result<RunResult, String> {
    let out = cmd
        .output()
        .map_err(|e| format!("failed to run {cmd:?}: {e}"))?;
    Ok(RunResult {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        code: out.status.code().unwrap_or(-1),
    })
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

/// Compile all `.java` files under `src_root` into `classes_out`.
fn javac_all(javac: &Path, src_root: &Path, classes_out: &Path) -> Result<(), String> {
    let mut sources: Vec<PathBuf> = Vec::new();
    collect_java(src_root, &mut sources);
    sources.sort();
    assert!(
        !sources.is_empty(),
        "no java sources under {}",
        src_root.display()
    );
    let status = Command::new(javac)
        .arg("-d")
        .arg(classes_out)
        .args(&sources)
        .status()
        .map_err(|e| format!("javac failed to start: {e}"))?;
    if !status.success() {
        return Err("javac failed".to_string());
    }
    Ok(())
}

fn collect_java(dir: &Path, out: &mut Vec<PathBuf>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_java(&p, out);
        } else if p.extension().map(|x| x == "java").unwrap_or(false) {
            out.push(p);
        }
    }
}

fn pack_jar(jar: &Path, classes_dir: &Path, archive: &Path) -> Result<(), String> {
    let status = Command::new(jar)
        .arg("cf")
        .arg(archive)
        .arg("-C")
        .arg(classes_dir)
        .arg(".")
        .status()
        .map_err(|e| format!("jar failed to start: {e}"))?;
    if !status.success() {
        return Err("jar cf failed".to_string());
    }
    Ok(())
}

#[test]
fn jar_differential_suite() {
    let (javac, java, jar) = match locate_java() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("skipping jar suite: {e}");
            return;
        }
    };
    let suite_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/suite_jar");
    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&suite_dir)
        .expect("suite_jar directory")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.is_dir())
        .collect();
    fixtures.sort();
    assert!(
        fixtures.len() >= 4,
        "jar suite shrank: {} fixtures (expected >= 4)",
        fixtures.len()
    );

    let j2r = env!("CARGO_BIN_EXE_j2r");
    let work_root = std::env::temp_dir().join(format!("j2r-jar-{}", std::process::id()));
    std::fs::create_dir_all(&work_root).expect("work root");
    let mut failures = Vec::new();

    for fixture in &fixtures {
        let name = fixture.file_name().unwrap().to_string_lossy().to_string();
        let work = work_root.join(&name);
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).expect("work dir");
        let main_class = std::fs::read_to_string(fixture.join("main"))
            .expect("main file")
            .trim()
            .to_string();
        let kind = std::fs::read_to_string(fixture.join("kind"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "jar".to_string());

        let build = || -> Result<(String, String), String> {
            let app_jar = work.join("app.jar");
            let classpath: String = match kind.as_str() {
                "jar" => {
                    let cls = work.join("classes");
                    std::fs::create_dir_all(&cls).unwrap();
                    javac_all(&javac, fixture, &cls)?;
                    pack_jar(&jar, &cls, &app_jar)?;
                    app_jar.to_string_lossy().to_string()
                }
                "dupdir" => {
                    // The directory copy of Dup goes first on the
                    // classpath; the jar (built from dup_jar) second. Both
                    // JVMs must load Dup from the directory copy.
                    let dupcls = work.join("dupcls");
                    let jarcls = work.join("jarcls");
                    std::fs::create_dir_all(&dupcls).unwrap();
                    std::fs::create_dir_all(&jarcls).unwrap();
                    javac_all(&javac, &fixture.join("dup_dir"), &dupcls)?;
                    javac_all(&javac, &fixture.join("dup_jar"), &jarcls)?;
                    // Compile App.java (fixture root) together with the
                    // jar copy of Dup so javac resolves the reference.
                    let status = Command::new(&javac)
                        .arg("-d")
                        .arg(&jarcls)
                        .arg(fixture.join("dup_jar").join("Dup.java"))
                        .arg(fixture.join("App.java"))
                        .status()
                        .map_err(|e| format!("javac failed to start: {e}"))?;
                    if !status.success() {
                        return Err("javac App failed".to_string());
                    }
                    pack_jar(&jar, &jarcls, &app_jar)?;
                    format!("{}:{}", dupcls.display(), app_jar.display())
                }
                other => return Err(format!("unknown fixture kind {other}")),
            };
            Ok((classpath, app_jar.to_string_lossy().to_string()))
        };
        let (classpath, app_jar) = match build() {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!("{name}: fixture build failed: {e}"));
                continue;
            }
        };

        // Reference JVM: java -cp <classpath> <main>
        let reference = match run(Command::new(&java)
            .arg("-cp")
            .arg(&classpath)
            .arg(&main_class))
        {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("{name}: reference jvm failed: {e}"));
                continue;
            }
        };

        // j2r compile from the classpath (no explicit inputs).
        let out_dir = work.join("crate");
        let compile = run(Command::new(j2r)
            .arg("compile")
            .arg("-p")
            .arg(&classpath)
            .arg("--main-class")
            .arg(&main_class)
            .arg("--crate-name")
            .arg("j2rjarfix")
            .arg("-o")
            .arg(&out_dir));
        if let Ok(c) = &compile {
            if c.code != 0 {
                failures.push(format!(
                    "{name}: j2r compile failed (exit {})\n--- stderr:\n{}",
                    c.code, c.stderr
                ));
                continue;
            }
        } else {
            failures.push(format!("{name}: j2r compile did not run"));
            continue;
        }

        // Build the generated crate (debug is enough for correctness).
        let build = run(Command::new(cargo())
            .arg("build")
            .arg("--manifest-path")
            .arg(out_dir.join("Cargo.toml")));
        if let Ok(b) = &build {
            if b.code != 0 {
                failures.push(format!(
                    "{name}: generated crate build failed\n--- stderr:\n{}",
                    b.stderr
                ));
                continue;
            }
        } else {
            failures.push(format!("{name}: cargo did not run"));
            continue;
        }
        let bin = out_dir.join("target/debug/j2rjarfix");
        let generated = match run(&mut Command::new(&bin)) {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("{name}: generated binary failed to run: {e}"));
                continue;
            }
        };

        // Three-channel comparison. These fixtures are deterministic and
        // exception-free, so no stack-frame normalization is needed.
        if generated.stdout != reference.stdout {
            failures.push(format!(
                "{name}: stdout mismatch\n--- jvm:\n{}\n--- j2r:\n{}",
                reference.stdout, generated.stdout
            ));
        }
        if generated.stderr != reference.stderr {
            failures.push(format!(
                "{name}: stderr mismatch\n--- jvm:\n{}\n--- j2r:\n{}",
                reference.stderr, generated.stderr
            ));
        }
        if generated.code != reference.code {
            failures.push(format!(
                "{name}: exit mismatch (jvm {}, j2r {})",
                reference.code, generated.code
            ));
        }
        let _ = app_jar;
    }

    let _ = std::fs::remove_dir_all(&work_root);
    assert!(
        failures.is_empty(),
        "{} jar fixture(s) failed:\n{}",
        failures.len(),
        failures.join("\n=====\n")
    );
}

/// M3_PLAN §4.6 determinism guard: compiling the same program from a jar
/// and from the equivalent exploded directory must produce byte-identical
/// generated crates (the closure loader feeds the same bytes to the same
/// parser either way).
#[test]
fn jar_vs_exploded_determinism() {
    let (javac, _java, jar) = match locate_java() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("skipping determinism test: {e}");
            return;
        }
    };
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/suite_jar/JarMain");
    let work = std::env::temp_dir().join(format!("j2r-jardet-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("work dir");

    let cls = work.join("classes");
    std::fs::create_dir_all(&cls).unwrap();
    javac_all(&javac, &fixture, &cls).expect("javac");
    let app_jar = work.join("app.jar");
    pack_jar(&jar, &cls, &app_jar).expect("jar");

    let j2r = env!("CARGO_BIN_EXE_j2r");
    let compile = |cp: &str, out: &Path| {
        let r = run(Command::new(j2r)
            .arg("compile")
            .arg("-p")
            .arg(cp)
            .arg("--main-class")
            .arg("a.b.App")
            .arg("-o")
            .arg(out))
        .expect("j2r compile runs");
        assert_eq!(r.code, 0, "compile from {} failed:\n{}", cp, r.stderr);
    };
    let from_jar = work.join("from-jar");
    let from_dir = work.join("from-dir");
    compile(&app_jar.to_string_lossy(), &from_jar);
    compile(&cls.to_string_lossy(), &from_dir);

    // Compare every generated source file byte-for-byte.
    let mut mismatches = Vec::new();
    for entry in walk_rs(&from_jar.join("src")) {
        let rel = entry.strip_prefix(&from_jar).expect("prefix");
        let other = from_dir.join(rel);
        let a = std::fs::read(&entry).expect("read a");
        let b = std::fs::read(&other).unwrap_or_else(|_| panic!("missing {}", other.display()));
        if a != b {
            mismatches.push(rel.display().to_string());
        }
    }
    let _ = std::fs::remove_dir_all(&work);
    assert!(
        mismatches.is_empty(),
        "generated crates differ between jar and exploded-dir classpath: {:?}",
        mismatches
    );
}

fn walk_rs(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return out,
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk_rs(&p));
        } else if p.extension().map(|x| x == "rs").unwrap_or(false) {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// Negative e2e: a referenced class missing from the classpath is a loud
/// compile error naming the referer — never a silent partial program.
#[test]
fn missing_class_fails_loudly() {
    let (javac, _java, _jar) = match locate_java() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("skipping negative test: {e}");
            return;
        }
    };
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/suite_jar/JarSuper");
    let work = std::env::temp_dir().join(format!("j2r-jarneg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("work dir");

    // Compile only the shape/ classes into a classes dir; leave runner/
    // out of the classpath entirely.
    let cls = work.join("classes");
    std::fs::create_dir_all(&cls).unwrap();
    javac_all(&javac, &fixture.join("shape"), &cls).expect("javac shape");

    let j2r = env!("CARGO_BIN_EXE_j2r");
    let r = run(Command::new(j2r)
        .arg("compile")
        .arg("-p")
        .arg(&cls)
        .arg("--main-class")
        .arg("runner.Main")
        .arg("-o")
        .arg(work.join("out")))
    .expect("j2r runs");
    let _ = std::fs::remove_dir_all(&work);
    assert_ne!(
        r.code, 0,
        "compile must fail when a referenced class is missing"
    );
    assert!(
        r.stderr.contains("runner/Main"),
        "error must name the referer, got: {}",
        r.stderr
    );
}
