#!/usr/bin/env python3
"""Doc-count sync gate (audit A8 / M3.5).

Hand-edited prose counts in README.md drifted (the external audit found
three stale numbers plus a duplicated bullet). Standing policy since:
counts are machine-checked. This script measures the repository and
fails on drift from the numbers written in README.md and the suite
guard in diff_suite.rs. Fix the docs, never the checker.

Measured facts:

  F = number of differential fixtures   crates/j2r-cli/tests/suite/*.java
  T = number of tests cargo would run   cargo test --workspace -- --list

Checked claims (README.md):

  **{F} fixture programs**
  $ cargo test   # {T} tests: ...
  The differential suite (crates/j2r-cli/tests/suite/*.java, {F} programs)
  cfg prototype: {F}/{F} fixtures output-identical

Checked claims (diff_suite.rs):

  fixtures.len() >= {F - 1}   (guard exactly one below the count)
"""

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def main() -> int:
    problems: list[str] = []

    fixtures = sorted((ROOT / "crates/j2r-cli/tests/suite").glob("*.java"))
    fixture_count = len(fixtures)

    out = subprocess.run(
        ["cargo", "test", "--workspace", "--", "--list", "--format", "terse"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        print("cargo test --list failed:\n" + out.stderr[-2000:])
        return 2
    test_lines = [ln for ln in out.stdout.splitlines() if ln.endswith(": test")]
    test_count = len(test_lines)

    readme = (ROOT / "README.md").read_text()
    dsuite = (ROOT / "crates/j2r-cli/tests/diff_suite.rs").read_text()

    def claim(pattern: str, actual: int, label: str) -> None:
        m = re.search(pattern, readme)
        if not m:
            problems.append(
                f"README: missing count claim: {label} (pattern {pattern!r})"
            )
            return
        got = int(m.group(1))
        if got != actual:
            problems.append(f"README: {label} says {got}, measured {actual}")

    claim(r"\*\*(\d+) fixture programs\*\*", fixture_count, "fixture-programs count")
    claim(r"cargo test\s+# (\d+) tests:", test_count, "cargo-test total")
    claim(r"tests/suite/\*\.java`, (\d+) programs\)", fixture_count, "differential-suite size")

    m = re.search(r"(\d+)/(\d+) fixtures output-identical", readme)
    if not m:
        problems.append("README: missing cfg-equivalence count claim")
    elif not (m.group(1) == m.group(2) == str(fixture_count)):
        problems.append(
            f"README: cfg-equivalence says {m.group(1)}/{m.group(2)}, "
            f"measured {fixture_count}/{fixture_count}"
        )

    mg = re.search(r"fixtures\.len\(\) >= (\d+)", dsuite)
    if not mg:
        problems.append("diff_suite.rs: missing fixtures.len() >= N guard")
    elif int(mg.group(1)) != fixture_count - 1:
        problems.append(
            f"diff_suite.rs: guard is {mg.group(1)}, expected "
            f"{fixture_count - 1} (one below the {fixture_count} fixtures)"
        )

    if problems:
        print("DOC-COUNT DRIFT (fix the docs, not the checker):")
        for p in problems:
            print("  - " + p)
        print(f"\nmeasured: fixtures={fixture_count} tests={test_count}")
        return 1
    print(f"doc counts in sync: fixtures={fixture_count} tests={test_count}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
