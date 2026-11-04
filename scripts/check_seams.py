#!/usr/bin/env python3
"""gate-12-runtime-seams — the CI-side seam guard (M4_PLAN §2 R3).

M4_PLAN §2 R3 promised: "CI greps for unannotated `thread_local!` in
j2r-runtime". Until 2026-11-04 that promise lived only inside a unit test
(`j2r-runtime::m4_wave1::thread_local_seams_are_annotated`, lib.rs), which
covers j2r-runtime only and runs as part of gate-3. This gate makes the
promise a visible, named check across ALL workspace crates:

  * Any file that invokes `thread_local!` must carry an `M4-SEAM`
    justification (same contract as the unit test, extended to every
    crate, and matching indented invocations the unit test misses).
  * `j2r-codegen` is held to the stricter rule: generated code must be
    shared-safe, so any `thread_local!` occurrence inside its emission
    templates (string literals — not line-start invocations) fails the
    gate outright unless the file carries an `M4-SEAM` marker.

The census is printed on every run: the seam count is a migration-progress
meter for M4 (Wave 1 left exactly one; Wave 2 (T07) is expected to retire
the frame-registry seam's per-thread design note).
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ("j2r-classfile", "j2r-codegen", "j2r-cli", "j2r-runtime")
MARKER = "M4-SEAM"


def main() -> int:
    census: list[str] = []
    failures: list[str] = []

    for crate in CRATES:
        src = ROOT / "crates" / crate / "src"
        if not src.is_dir():
            failures.append(f"missing source dir: {src}")
            continue
        for path in sorted(src.rglob("*.rs")):
            text = path.read_text(encoding="utf-8", errors="replace")
            rel = path.relative_to(ROOT)

            # Real invocations, indented or not (the unit test in lib.rs
            # only matches column-0 and only sees j2r-runtime).
            invocations = sum(
                1 for line in text.splitlines() if line.lstrip().startswith("thread_local!")
            )

            if crate == "j2r-codegen":
                # Templates emit code inside string literals, where the
                # line-start rule above cannot see them. Generated code
                # must be shared-safe: forbid any occurrence outright.
                if invocations == 0 and "thread_local!" in text and MARKER not in text:
                    failures.append(
                        f"{rel}: codegen template emits `thread_local!` "
                        "(generated code must be shared-safe; add an "
                        f"{MARKER} justification if a seam is truly needed)"
                    )
                elif invocations > 0 and MARKER not in text:
                    failures.append(
                        f"{rel}: unannotated thread_local! invocation "
                        f"(every seam needs an {MARKER} justification, "
                        "M4_PLAN §2 R3)"
                    )
                if invocations > 0 or "thread_local!" in text:
                    census.append(f"{rel}: {invocations} invocation(s), template mentions present")
                continue

            if invocations == 0:
                continue
            if MARKER not in text:
                failures.append(
                    f"{rel}: unannotated thread_local! — every remaining "
                    f"seam must carry an {MARKER} justification "
                    "(M4_PLAN §2 R3)"
                )
            census.append(f"{rel}: {invocations} invocation(s)")

    print("M4-SEAM census (files mentioning thread_local!):")
    if census:
        for line in census:
            print(f"  {line}")
    else:
        print("  (none — fully shared, zero per-thread state)")

    if failures:
        print("\nFAILURES:")
        for line in failures:
            print(f"  {line}")
        return 1

    print(
        f"\ngate-12: PASS — all thread_local! seams annotated across "
        f"{len(CRATES)} crates; codegen templates clean"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
