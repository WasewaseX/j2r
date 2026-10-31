//! Differential-run comparison: what `j2r test` checks between the JVM run
//! and the native run.
//!
//! All three observable channels are compared:
//!
//! * **stdout** — byte-for-byte.
//! * **stderr** — byte-for-byte *after* removing JVM stack-trace frame
//!   lines (`\tat ...`, `Suppressed: ...`, `\t... N more`). Frames carry
//!   source locations the milestone-1 runtime cannot produce (it has no
//!   `LineNumberTable` support yet); they contain no exception type or
//!   message information, which is why normalizing them away is sound.
//!   Everything else — the `Exception in thread "main" <type>: <message>`
//!   line, `Caused by:` chains, plain program output on stderr — must
//!   match exactly.
//! * **exit status** — exact.
//!
//! Unit tests below pin the semantics: different exception types, different
//! messages, different stderr output or different exit codes must all fail;
//! only frame lines may differ.

/// The observable result of one program run (JVM or native).
#[derive(Debug, Clone, PartialEq)]
pub struct RunResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub code: i32,
}

/// Removes JVM stack-trace frame lines from a stderr capture.
///
/// Normalized away (location details only, never types or messages):
/// * lines starting with a tab + `at ` — `\tat Uncaught.main(Uncaught.java:8)`
/// * lines starting with a tab + `... ` — the `\t... 3 more` tail
/// * `Suppressed: ...` lines (themselves frame-carrying)
/// * bare `...` continuation lines
///
/// Kept (must match exactly): the `Exception in thread "main" ...` header,
/// `Caused by: ...` sections, and every line a program prints itself.
pub fn normalize_stderr(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut kept: Vec<&str> = Vec::new();
    for line in text.lines() {
        let t = line.trim_end();
        if t.starts_with("\tat ") || t.starts_with("\t... ") {
            continue;
        }
        if t == "..." || t.starts_with("Suppressed: ") {
            continue;
        }
        kept.push(t);
    }
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Compares a JVM run against a native run across all three channels.
/// Returns a multi-line diagnostic on the first divergence found.
pub fn streams_match(java: &RunResult, native: &RunResult) -> Result<(), String> {
    let mut problems = String::new();
    if java.stdout != native.stdout {
        problems.push_str(&stream_diff("stdout", &java.stdout, &native.stdout));
    }
    let j_err = normalize_stderr(&java.stderr);
    let n_err = normalize_stderr(&native.stderr);
    if j_err != n_err {
        problems.push_str(&stream_diff(
            "stderr (JVM frames normalized)",
            j_err.as_bytes(),
            n_err.as_bytes(),
        ));
    }
    if java.code != native.code {
        problems.push_str(&format!(
            "exit codes differ: java={} native={}\n",
            java.code, native.code
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// A line-based diff of two captures (used for failure diagnostics).
pub fn stream_diff(label: &str, expected: &[u8], actual: &[u8]) -> String {
    use std::fmt::Write as _;
    let exp = String::from_utf8_lossy(expected);
    let act = String::from_utf8_lossy(actual);
    let exp_lines: Vec<&str> = exp.lines().collect();
    let act_lines: Vec<&str> = act.lines().collect();
    let mut out = String::new();
    let _ = writeln!(out, "{label} differs:");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run(stdout: &str, stderr: &str, code: i32) -> RunResult {
        RunResult {
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            code,
        }
    }

    #[test]
    fn frames_are_normalized_but_the_header_is_not() {
        let jvm = "Exception in thread \"main\" java.lang.IllegalStateException: boom\n\
                   \tat Uncaught.boom(Uncaught.java:3)\n\
                   \tat Uncaught.main(Uncaught.java:8)\n";
        let native = "Exception in thread \"main\" java.lang.IllegalStateException: boom\n";
        assert_eq!(
            normalize_stderr(jvm.as_bytes()),
            normalize_stderr(native.as_bytes())
        );
    }

    #[test]
    fn suppressed_and_more_lines_are_normalized() {
        let jvm = "Exception in thread \"main\" java.lang.Exception: x\n\
                   \tat A.main(A.java:1)\n\
                   Suppressed: java.lang.Exception: y\n\
                   \tat B.main(B.java:2)\n\
                   \t... 3 more\n";
        let native = "Exception in thread \"main\" java.lang.Exception: x\n";
        assert_eq!(
            normalize_stderr(jvm.as_bytes()),
            normalize_stderr(native.as_bytes())
        );
    }

    #[test]
    fn different_exception_types_fail() {
        let jvm = run("", "Exception in thread \"main\" java.lang.RuntimeException: boom\n\tat A.main(A.java:1)\n", 1);
        let native = run(
            "",
            "Exception in thread \"main\" java.lang.IllegalStateException: boom\n",
            1,
        );
        assert!(streams_match(&jvm, &native).is_err());
    }

    #[test]
    fn different_messages_fail() {
        let jvm = run("", "Exception in thread \"main\" java.lang.RuntimeException: boom\n\tat A.main(A.java:1)\n", 1);
        let native = run(
            "",
            "Exception in thread \"main\" java.lang.RuntimeException: bam\n",
            1,
        );
        assert!(streams_match(&jvm, &native).is_err());
    }

    #[test]
    fn different_exit_codes_fail() {
        let a = run("", "", 0);
        let b = run("", "", 1);
        assert!(streams_match(&a, &b).is_err());
    }

    #[test]
    fn stderr_content_differs_fail() {
        let a = run("ok\n", "err-one\n", 0);
        let b = run("ok\n", "err-two\n", 0);
        assert!(streams_match(&a, &b).is_err());
    }

    #[test]
    fn stdout_differs_fail() {
        let a = run("one\n", "", 0);
        let b = run("two\n", "", 0);
        assert!(streams_match(&a, &b).is_err());
    }

    #[test]
    fn empty_stderr_still_must_be_empty() {
        let a = run("ok\n", "", 0);
        let b = run("ok\n", "some noise\n", 0);
        assert!(streams_match(&a, &b).is_err());
    }

    #[test]
    fn identical_runs_pass() {
        let a = run("out\n", "err\n", 7);
        let b = run("out\n", "err\n", 7);
        assert_eq!(streams_match(&a, &b), Ok(()));
    }

    #[test]
    fn identical_runs_with_frames_pass() {
        let a = run(
            "out\n",
            "Exception in thread \"main\" java.lang.RuntimeException: r\n\tat A.main(A.java:1)\n",
            1,
        );
        let b = run(
            "out\n",
            "Exception in thread \"main\" java.lang.RuntimeException: r\n",
            1,
        );
        assert_eq!(streams_match(&a, &b), Ok(()));
    }
}
