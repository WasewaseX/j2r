#!/usr/bin/env python3
"""Stub/elision integrity gate (master-ci-inspired gate-upgrade package, 2026-11-06).

Blocking-grade deterministic content defects only (the master-ci model:
"deterministic content defects" may block; stylistic heuristics must
never do so). Scans crates/**/*.rs for:

  * `todo!(` / `unimplemented!(`   — Rust stub macros (master-ci rust-stub)
  * `// ... existing code ...`     — LLM elision markers: an edit that
                                     silently deleted half a file
  * `FIXME` tokens                 — j2r policy: defects are registered on
                                     the TODO board with evidence, never
                                     parked silently in comments

A zero-hit baseline is the birth state of this gate (verified 2026-11-06:
crates/ carries none of these). If a rule starts producing false
positives, the fix is to sharpen the rule here — never to weaken the gate.

Self-test (synthetic tree, runs inside the gate job before the scan):

  python3 scripts/check_stubs.py --selftest
"""

from __future__ import annotations

import pathlib
import re
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent

RULES: list[tuple[str, re.Pattern[str]]] = [
    ("todo!-stub", re.compile(r"\btodo!\s*\(")),
    ("unimplemented!-stub", re.compile(r"\bunimplemented!\s*\(")),
    ("elided-code-marker", re.compile(r"existing\s+code", re.IGNORECASE)),
    ("FIXME-token", re.compile(r"\bFIXME\b")),
]


def check_tree(root: pathlib.Path) -> list[str]:
    problems: list[str] = []
    for path in sorted(root.glob("crates/**/*.rs")):
        for i, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
            for rule, pat in RULES:
                if pat.search(line):
                    problems.append(f"{path.relative_to(root)}:{i}: {rule}: {line.strip()[:90]}")
    return problems


# --- selftest -----------------------------------------------------------


def selftest() -> int:
    with tempfile.TemporaryDirectory() as td:
        root = pathlib.Path(td)
        (root / "crates/app/src").mkdir(parents=True)
        (root / "crates/app/src/lib.rs").write_text(
            "fn f() { todo!(\"later\") }\n"
            "fn g() { unimplemented!() }\n"
            "// ... existing code ...\n"
            "// FIXME: rework\n"
            "fn ok() -> u32 { 42 }\n"
            "fn also_ok() { /* todo later in prose without macro */ }\n"
            "// docs mention todo! without a call paren — allowed\n"
        )
        (root / "crates/app/src/clean.rs").write_text("fn clean() {}\n")

        global ROOT
        real_root, ROOT = ROOT, root
        try:
            problems = check_tree(root)
        finally:
            ROOT = real_root

        got = [p.split(":")[0] for p in problems]                      # file path
        rules_hit = sorted(p.split(": ", 1)[1].split(":")[0].strip() for p in problems)
        want_rules = ["FIXME-token", "elided-code-marker", "todo!-stub", "unimplemented!-stub"]
        ok = len(got) == 4 and rules_hit == want_rules and all("lib.rs" in g for g in got)
        for p in problems:
            print(f"  found: {p}")
        print(f"stubs selftest: {'GREEN' if ok else 'FAIL'} "
              f"({len(got)} findings, clean.rs untouched: {all('clean' not in g for g in got)})")
        return 0 if ok else 1


def main() -> int:
    if "--selftest" in sys.argv:
        return selftest()
    problems = check_tree(ROOT)
    if problems:
        print(f"STUB MARKERS FOUND ({len(problems)}) — register the work on the "
              "TODO board or finish it; silent stubs never merge:")
        for p in problems:
            print("  - " + p)
        return 1
    print("code integrity: no stubs, elision markers or FIXME tokens in crates/")
    return 0


if __name__ == "__main__":
    sys.exit(main())
