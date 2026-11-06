#!/usr/bin/env python3
"""Docs-only path filter (master-ci-inspired gate-upgrade package, 2026-11-06).

A pull request that touches ONLY prose (markdown under the repo root or
docs/, plus docs images) cannot change compiler/runtime behaviour, so the
five heavy compile-and-run legs (gate-3 test, gate-4 build-release,
gate-5 standalone, gate-9 gc-stress, gate-13 concurrency) may skip it.
Policy rules, chosen to fail OPEN (a false "docs-only" costs a red main,
a false "not docs-only" only costs minutes):

  * push events NEVER filter — main keeps every run (evidence chain)
  * anything under .github/ is never docs-only (workflow changes run all)
  * any uncertainty (empty diff, unknown event) is "not docs-only"
  * the review's edge cases are pinned by --selftest below: a shared
    library change, a package manifest, a lockfile, a root config file,
    a generated file and a workflow file must all keep full CI

Usage in the `changes` job:

  - id: filt
    run: python3 scripts/path_filter.py >> "$GITHUB_OUTPUT"

then downstream jobs:

    if: github.event_name == 'push' || needs.changes.outputs.docs_only != 'true'

Self-test (runs inside the same job, before the verdict):

  python3 scripts/path_filter.py --selftest
"""

from __future__ import annotations

import os
import subprocess
import sys

# Only prose and docs-only images. Nothing under .github/ matches on
# purpose (fail open on workflow/template changes).
DOCS_PATTERNS = (
    lambda name: (
        (name.endswith(".md") and "/" not in name)
        or name.startswith("docs/")
        and name.rsplit(".", 1)[-1].lower()
        in {"md", "png", "svg", "jpg", "jpeg", "gif", "webp"}
    ),
)


def is_docs_only(changed: list[str]) -> bool:
    """True iff `changed` is non-empty and every file is docs/prose."""
    if not changed:
        return False  # empty diff: fail open
    for name in changed:
        if name.startswith(".github/"):
            return False
        if not any(p(name) for p in DOCS_PATTERNS):
            return False
    return True


def changed_files(base: str) -> list[str]:
    out = subprocess.run(
        ["git", "diff", "--name-only", base, "HEAD"],
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        # fail open: unknown diff means run everything
        print(f"path_filter: git diff failed ({out.stderr[-300:]}) — not docs-only", file=sys.stderr)
        return []
    return [ln for ln in out.stdout.splitlines() if ln.strip()]


# --- selftest -----------------------------------------------------------

CASES: list[tuple[list[str], bool, str]] = [
    (["crates/j2r-runtime/src/heap.rs"], False, "runtime source"),
    (["crates/j2r-codegen/src/verify.rs"], False, "shared library source"),
    (["crates/j2r-cli/Cargo.toml"], False, "package manifest"),
    (["Cargo.lock"], False, "lockfile"),
    (["deny.toml"], False, "root config file"),
    (["scripts/check_doc_counts.py"], False, "root script (generated/config)"),
    ([".github/workflows/ci.yml"], False, "workflow itself"),
    ([".github/dependabot.yml"], False, "repo meta under .github"),
    (["crates/j2r-cli/tests/suite/Statics.java"], False, "java fixture"),
    (["README.md"], True, "root readme only"),
    (["docs/M4_PLAN.md", "README.md", "TODO.md"], True, "prose set"),
    (["docs/img/arch.png", "docs/RUNTIME_SAFETY.md"], True, "docs image + prose"),
    (["README.md", "crates/j2r-cli/src/main.rs"], False, "mixed docs+code"),
    (["docs/M4_PLAN.md", ".github/workflows/ci.yml"], False, "prose + workflow"),
    ([], False, "empty diff fails open"),
    (["corpus/MinimalJsonCorpus.java"], False, "corpus fixture"),
    (["LICENSE"], False, "legal file is not prose"),
]


def selftest() -> int:
    failed = 0
    for files, want, label in CASES:
        got = is_docs_only(files)
        ok = got == want
        print(f"  {'ok ' if ok else 'FAIL'} {label}: {files or '[]'} -> docs_only={got}")
        if not ok:
            failed += 1
    if failed:
        print(f"selftest FAILED ({failed}/{len(CASES)})")
        return 1
    print(f"path_filter selftest: {len(CASES)}/{len(CASES)} cases green")
    return 0


def main() -> int:
    if "--selftest" in sys.argv:
        return selftest()

    event = os.environ.get("GITHUB_EVENT_NAME", "")
    base = os.environ.get("BASE_SHA", "")
    if event != "pull_request" or not base:
        # push to main (and everything unknown): every run is full
        print("docs_only=false")
        return 0
    files = changed_files(base)
    docs = is_docs_only(files)
    print(f"# changed files ({len(files)}): {files[:20]}", file=sys.stderr)
    print(f"docs_only={'true' if docs else 'false'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
