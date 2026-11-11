#!/usr/bin/env python3
"""Cross-block slot-var read census (the entry-var layer's exact target
set) — reads at a block head of a variable whose most recent write is in
a DIFFERENT block.

Blocks = the arms of the `match pc` dispatch (`N => {` ... `}`).
Within each arm, a read of a variable written EARLIER IN THE SAME ARM is
intra-block (the unification layer already owns those). A read with no
prior same-arm write is an ENTRY read — the block-join/entry-var layer's
target: today that value round-trips through the predecessor's handoff
write into the shared slot cell.

Usage: crossblock_census.py <emitted-crate-root>
"""
import re
import sys
from pathlib import Path

IDENT = re.compile(r"\b([sv]\d+e\d+)\b")
WRITE = re.compile(r"^\s*([sv]\d+e\d+)\s*=\s*(?!=)")
ARM = re.compile(r"^\s+(\d+)u32 => \{")


def census(root: Path) -> None:
    total_entry = 0
    per_file = []
    details = {}
    for f in sorted((root / "src" / "classes").glob("*.rs")):
        text = f.read_text()
        entry_reads = 0
        edges = 0
        arm = None
        written = set()
        for line in text.splitlines():
            m = ARM.match(line)
            if m:
                arm = int(m.group(1))
                written = set()
                continue
            if arm is None:
                continue
            w = WRITE.match(line)
            if w:
                written.add(w.group(1))
            for i in IDENT.findall(line):
                if w and w.group(1) == i and written == {i} or (
                    w and w.group(1) == i and i not in re.findall(r"\b[sv]\d+e\d+\b", line[w.end():])
                ):
                    # pure write, no same-line read
                    continue
                if i not in written:
                    entry_reads += 1
                    details.setdefault((f.name, arm), []).append(i)
        if entry_reads:
            per_file.append((f.name, entry_reads))
            total_entry += entry_reads
    print(f"cross-block ENTRY reads (the entry-var target set): {total_entry}")
    for name, n in per_file:
        print(f"  {name}: {n}")
    # distinct entry variables
    distinct = {}
    for (fname, arm), vars_ in details.items():
        for v in vars_:
            distinct.setdefault(v, 0)
            distinct[v] += 1
    hot = sorted(distinct.items(), key=lambda kv: -kv[1])[:12]
    print("hottest entry variables (reads):", ", ".join(f"{k}:{v}" for k, v in hot))


if __name__ == "__main__":
    census(Path(sys.argv[1]))
