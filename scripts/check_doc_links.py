#!/usr/bin/env python3
"""Doc-link integrity gate (master-ci-inspired gate-upgrade package, 2026-11-06).

Every relative link and intra-file anchor in the repository's markdown
must resolve. External http(s) links are NOT verified here (network
flakiness = false positives; that class belongs to a scheduled advisory
job, not a blocking gate). This gate is deterministic: it only checks
what is in the tree.

Checks, per markdown file (README.md, TODO.md, ROADMAP.md,
examples/README.md, docs/**/*.md):

  * `[text](target)` and `![alt](target)` with a relative target:
    the file/directory must exist (URL-decoded, `#anchor` stripped)
  * `[text](some.md#anchor)`: the anchor must match a heading in the
    target file (GitHub slugging)
  * `[text](#anchor)`: the anchor must match a heading in this file

GitHub slugging: lowercase; drop every char that is not a letter,
digit, space, `-` or `_`; spaces (incl. runs) -> `-`.

Self-test (synthetic tree, runs inside the gate job before the scan):

  python3 scripts/check_doc_links.py --selftest
"""

from __future__ import annotations

import pathlib
import re
import sys
import tempfile
import urllib.parse

ROOT = pathlib.Path(__file__).resolve().parent.parent

LINK_RE = re.compile(r"\[(?:[^\[\]]*)\]\(([^)\s]+)\)")
HEADING_RE = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")

# Repo-relative roots scanned (README.md is the project's front door;
# docs/ is the evidence trail; examples/README.md is user-facing).
SCAN_PATTERNS = ["README.md", "TODO.md", "ROADMAP.md", "examples/README.md"]
SCAN_GLOBS = ["docs/**/*.md"]


def slugify(heading: str) -> str:
    s = heading.strip().lower()
    out = []
    for ch in s:
        if ch.isalnum() or ch in "-_ ":
            out.append(ch)
    return "".join(out).replace(" ", "-")


def headings_of(text: str) -> set[str]:
    found = set()
    for line in text.splitlines():
        m = HEADING_RE.match(line)
        if m and not line.lstrip().startswith("`"):
            found.add(slugify(re.sub(r"`[^`]*`", "", m.group(2))))
    return found


def check_repo() -> list[str]:
    problems: list[str] = []

    files: list[pathlib.Path] = [ROOT / p for p in SCAN_PATTERNS if (ROOT / p).exists()]
    for pat in SCAN_GLOBS:
        files.extend(sorted((ROOT / pat).parent.glob(pat)))

    cache: dict[pathlib.Path, set[str]] = {}

    def heads(path: pathlib.Path) -> set[str]:
        if path not in cache:
            try:
                cache[path] = headings_of(path.read_text(encoding="utf-8"))
            except OSError:
                cache[path] = set()
        return cache[path]

    for md in files:
        text = md.read_text(encoding="utf-8", errors="replace")
        rel = md.relative_to(ROOT)
        # fenced code blocks are not links; blank them out
        stripped = re.sub(r"```.*?```", "", text, flags=re.S)
        stripped = re.sub(r"`[^`]*`", "", stripped)
        for m in LINK_RE.finditer(stripped):
            raw = m.group(1)
            if raw.startswith(("http://", "https://", "mailto:", "ftp://")):
                continue
            target, _, anchor = raw.partition("#")
            target = urllib.parse.unquote(target)
            if not target:
                # intra-file anchor
                if anchor and anchor not in heads(md):
                    problems.append(f"{rel}: broken intra-file anchor #{anchor}")
                continue
            if target.startswith("/"):
                candidate = ROOT / target.lstrip("/")
            else:
                candidate = (md.parent / target).resolve()
            if not candidate.exists():
                problems.append(f"{rel}: broken link target {raw}")
                continue
            if anchor and candidate.is_file() and candidate.suffix == ".md":
                if anchor not in heads(candidate):
                    problems.append(f"{rel}: broken anchor {raw}")
    return problems


# --- selftest -----------------------------------------------------------


def selftest() -> int:
    with tempfile.TemporaryDirectory() as td:
        root = pathlib.Path(td)
        (root / "README.md").write_text(
            "# T\n\n## intro\n\n"
            "[ok file](docs/guide.md)\n"
            "[ok anchor](docs/guide.md#setup-steps)\n"
            "[ok self](#intro)\n"
            "[broken file](docs/nope.md)\n"
            "[broken anchor](docs/guide.md#missing-here)\n"
            "[broken self](#ghost)\n"
            "[ext ignored](https://example.com/x#y)\n"
            "![img](docs/pic.png)\n"
            "![broken img](docs/pic.jpg)\n"
            "```\n[fenced, not a link](nope-2.md)\n```\n"
        )
        g = root / "docs"
        g.mkdir()
        (g / "guide.md").write_text("# G\n\n## Setup steps\n\nbody\n")
        (g / "pic.png").write_bytes(b"\x89PNG\r\n")

        global ROOT
        real_root, ROOT = ROOT, root
        try:
            problems = check_repo()
        finally:
            ROOT = real_root

        got = sorted(p.split(": ", 1)[1] for p in problems)
        want = sorted(
            [
                "broken link target docs/nope.md",
                "broken anchor docs/guide.md#missing-here",
                "broken intra-file anchor #ghost",
                "broken link target docs/pic.jpg",
            ]
        )
        ok = got == want
        for p in got:
            print(f"  found: {p}")
        print(f"doc-links selftest: {'GREEN' if ok else 'FAIL'} "
              f"({len(got)}/{len(want)} findings match)")
        return 0 if ok else 1


def main() -> int:
    if "--selftest" in sys.argv:
        return selftest()
    problems = check_repo()
    if problems:
        print(f"DOC-LINK DRIFT: {len(problems)} broken local links/anchors:")
        for p in problems:
            print("  - " + p)
        return 1
    print("doc links: every local link and anchor resolves")
    return 0


if __name__ == "__main__":
    sys.exit(main())
