//! Negative LambdaMetafactory tests: indy shapes outside the M3 supported
//! set must fail the build loudly, naming the shape (M3_PLAN §6.4).
//!
//! Skips silently when no JDK is available (CI installs one).

use std::process::Command;

fn javac_available() -> bool {
    if let Ok(p) = std::env::var("J2R_JAVAC") {
        return std::path::Path::new(&p).is_file();
    }
    if which("javac").is_some() {
        return true;
    }
    false
}

fn which(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var("PATH").ok()?;
    for dir in path.split(':') {
        let candidate = std::path::Path::new(dir).join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn javac_binary() -> String {
    // honor J2R_JAVAC like the availability check does (corpus wave 1 fix:
    // the two must never disagree or the suite panics on spawn)
    std::env::var("J2R_JAVAC").unwrap_or_else(|_| "javac".to_string())
}

fn compile_fails_with(src: &str, class_name: &str, needle: &str) {
    let exe = env!("CARGO_BIN_EXE_j2r");
    let dir = std::env::temp_dir().join(format!(
        "j2r-lambda-neg-{}-{class_name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let java = dir.join(format!("{class_name}.java"));
    std::fs::write(&java, src).expect("write source");

    let out = Command::new(javac_binary())
        .arg("-d")
        .arg(&dir)
        .arg(&java)
        .output()
        .expect("javac run");
    assert!(
        out.status.success(),
        "javac failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = Command::new(exe)
        .arg("compile")
        .arg(dir.join(format!("{class_name}.class")))
        .output()
        .expect("j2r run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let combined = format!("{stdout}{stderr}");
    assert!(
        !out.status.success(),
        "expected j2r to reject {class_name}, but it compiled (output: {combined})"
    );
    assert!(
        combined.contains(needle),
        "expected rejection of {class_name} to mention {needle:?}; got: {combined}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn functional_interface_outside_the_m3_set_is_loud() {
    if !javac_available() {
        eprintln!("skipping: javac not found");
        return;
    }
    // IntPredicate is not in the M3 LambdaMetafactory set (M3_PLAN §6.1).
    compile_fails_with(
        r#"
import java.util.function.IntPredicate;

public class LambdaNegIntPred {
    public static void main(String[] args) {
        IntPredicate odd = n -> n % 2 != 0;
        System.out.println(odd.test(3));
    }
}
"#,
        "LambdaNegIntPred",
        "outside the M3 LambdaMetafactory set",
    );
}

#[test]
fn to_int_function_specialization_is_loud() {
    if !javac_available() {
        eprintln!("skipping: javac not found");
        return;
    }
    // ToIntFunction<String> — again outside the supported set.
    compile_fails_with(
        r#"
import java.util.function.ToIntFunction;

public class LambdaNegToInt {
    public static void main(String[] args) {
        ToIntFunction<String> len = String::length;
        System.out.println(len.applyAsInt("abcd"));
    }
}
"#,
        "LambdaNegToInt",
        "outside the M3 LambdaMetafactory set",
    );
}
