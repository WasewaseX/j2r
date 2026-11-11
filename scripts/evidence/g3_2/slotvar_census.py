#!/usr/bin/env python3
"""Census of slot-variable traffic in an emitted typed crate (G3.2's
block-join/entry-var layer baseline).

Classifies every occurrence of a typed slot variable (s{n}e{m}, v{n}e{m})
in the generated method bodies:
  write   — LHS of an assignment (`s2e3 = ...;`)
  clone   — a `.clone()` read (a Value-cell round trip)
  read    — any other read (a bare-prim or typed-var use)
Also counts the `.expect(` Value unwraps per file (the cell-local +
invoke-result unwraps named by addendum 8's census).

Usage: slotvar_census.py <emitted-crate-root>
"""
import re
import sys
from pathlib import Path

IDENT = re.compile(r"\b([sv]\d+e\d+)\b")
WRITE = re.compile(r"^\s*([sv]\d+e\d+)\s*=\s*(?!=)")


def census(root: Path) -> dict:
    out = {"writes": 0, "clones": 0, "reads": 0, "expects": 0,
           "per_file": [], "read_names": {}}
    for f in sorted((root / "src" / "classes").glob("*.rs")):
        text = f.read_text()
        w = c = r = 0
        names = {}
        for line in text.splitlines():
            m = WRITE.match(line)
            idents = IDENT.findall(line)
            if m and m.group(1) in idents:
                w += 1
                rest = line[m.end():]
                others = [i for i in idents if i != m.group(1)]
                for i in others:
                    r += 1
                    names[i] = names.get(i, 0) + 1
                # a self-referential update (`s2e3 = s2e3 + 1;`) also reads
                if m.group(1) in rest:
                    r += 1
                    names[m.group(1)] = names.get(m.group(1), 0) + 1
                continue
            for i in idents:
                r += 1
                names[i] = names.get(i, 0) + 1
        clones = len(re.findall(r"\b[sv]\d+e\d+\.clone\(\)", text))
        exp = text.count(".expect(")
        out["writes"] += w
        out["clones"] += clones
        out["reads"] += r
        out["expects"] += exp
        out["per_file"].append((f.name, w, clones, r, exp))
        for k, v in names.items():
            out["read_names"][k] = out["read_names"].get(k, 0) + v
    return out


def main() -> None:
    root = Path(sys.argv[1])
    c = census(root)
    print(f"crate: {root.name}")
    print(f"slot-var writes: {c['writes']}")
    print(f"slot-var reads:  {c['reads']}  (of which .clone() Value round trips: {c['clones']})")
    print(f".expect( sites:  {c['expects']}")
    print(f"{'file':40s} {'writes':>7s} {'clones':>7s} {'reads':>7s} {'expects':>8s}")
    for name, w, cl, r, e in c["per_file"]:
        print(f"{name:40s} {w:7d} {cl:7d} {r:7d} {e:8d}")
    hot = sorted(c["read_names"].items(), key=lambda kv: -kv[1])[:15]
    print("hottest read variables:", ", ".join(f"{k}:{v}" for k, v in hot))


if __name__ == "__main__":
    main()
