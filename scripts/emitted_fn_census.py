#!/usr/bin/env python3
"""Census of generated top-level function sizes in an emitted crate
(G2.6 arm-b evidence: the per-method size/shape census).

Parses every .rs file under a root, extracts top-level `fn` items with
brace matching, and reports the largest ones (bytes, lines). Optionally
counts match-arm density per function.

Usage: emitted_fn_census.py SRC_ROOT [--top N]
"""
import argparse
import os
import re
import sys

FN_RE = re.compile(r"^    (?:pub )?(?:unsafe )?(?:extern \"[^\"]*\" )?fn (\w+)", re.M)
TOP_FN_RE = re.compile(r"^(?:pub )?(?:unsafe )?fn (\w+)", re.M)


def extract_fns(text):
    """Yield (name, start, end) for every top-level `fn` item via brace matching."""
    out = []
    for m in re.finditer(r"^(?:pub )?(?:unsafe )?(?:extern [\"\w ]+ )?fn\s+(\w+)[^\n;{]*\{", text, re.M):
        start = m.end() - 1
        depth = 0
        i = start
        n = len(text)
        while i < n:
            c = text[i]
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    break
            elif c == '"':
                i += 1
                while i < n and text[i] != '"':
                    if text[i] == "\\":
                        i += 1
                    i += 1
            i += 1
        if i < n:
            out.append((m.group(1), m.start(), i + 1))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("root")
    ap.add_argument("--top", type=int, default=20)
    a = ap.parse_args()
    rows = []
    for dirpath, _, files in os.walk(a.root):
        if "target" in dirpath:
            continue
        for f in files:
            if not f.endswith(".rs"):
                continue
            p = os.path.join(dirpath, f)
            text = open(p, encoding="utf-8", errors="replace").read()
            for name, s, e in extract_fns(text):
                body = text[s:e]
                matches = len(re.findall(r"\bmatch\b", body))
                rows.append((e - s, body.count("\n"), matches, os.path.relpath(p, a.root), name))
    rows.sort(reverse=True)
    print(f"# {len(rows)} fns censused under {a.root}")
    print("# size_B  lines  matches  file  fn")
    for size, lines, matches, p, name in rows[: a.top]:
        print(f"{size:>9} {lines:>6} {matches:>8}  {p}  {name}")


if __name__ == "__main__":
    main()
