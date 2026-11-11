#!/usr/bin/env python3
"""G4.2 arm (b) — the JMM boundary is ENFORCED, not just documented.

Re-derives the boundary field set from the INPUT .class files (fields
with ACC_VOLATILE, plus ACC_FINAL fields WITHOUT a ConstantValue —
folded constants are JMM-safe) and fails loudly unless the emitted
crate matches it exactly:

  1. `src/jmm_boundary.rs` lists every boundary field (class, field,
     kind) and NOTHING else.
  2. Every access site in `src/classes/*.rs` of a boundary field
     carries the `J2R-JMM-BOUNDARY:` marker comment; plain fields and
     folded constants carry none.

Usage:
  python3 scripts/check_jmm_boundary.py --classes <javac-out-dir> \
      --out <j2r-emit-out-dir> [--main <MainClass>]

The gate job (gate-23-jmm-boundary in ci.yml) compiles the committed
fixture with javac, runs j2r compile on it, then runs this checker.
An access-site marker text is expected once per generated access, so
the checker requires >= 1 marker per boundary field and rejects any
marker naming a non-boundary field.
"""

from __future__ import annotations

import argparse
import pathlib
import struct
import sys

ACC_VOLATILE = 0x0040
ACC_FINAL = 0x0010
ACC_STATIC = 0x0008

MARKER_PREFIX = "J2R-JMM-BOUNDARY: "


def fail(msg: str) -> None:
    print(f"JMM-BOUNDARY DRIFT: {msg}")
    sys.exit(1)


def parse_constant_pool(data: bytes, count: int) -> tuple[list[tuple[int, bytes]], int]:
    """Return ([(tag, raw_bytes)], end_offset) for constant-pool entries."""
    pool: list[tuple[int, bytes]] = [(0, b"")]
    i, p = 1, 10
    while i < count:
        tag = data[p]
        p += 1
        if tag in (1,):  # Utf8
            ln = struct.unpack_from(">H", data, p)[0]
            p += 2
            pool.append((tag, data[p : p + ln]))
            p += ln
        elif tag in (7, 8, 16, 19, 20):  # Class/String/MethodType/Module/Package
            pool.append((tag, data[p : p + 2]))
            p += 2
        elif tag in (15,):  # MethodHandle
            pool.append((tag, data[p : p + 3]))
            p += 3
        elif tag == 3 or tag == 4:  # Integer/Float
            pool.append((tag, data[p : p + 4]))
            p += 4
        elif tag == 5 or tag == 6:  # Long/Double (two slots)
            pool.append((tag, data[p : p + 8]))
            pool.append((0, b""))
            p += 8
        elif tag in (9, 10, 11, 12, 17, 18):  # refs / NameAndType / dynamic
            pool.append((tag, data[p : p + 4]))
            p += 4
        else:
            fail(f"unsupported constant-pool tag {tag} at entry {i}")
        i += 1
        if tag in (5, 6):
            i += 1
    return pool, p


def parse_fields(data: bytes) -> list[tuple[str, str, bool, bool]]:
    """Parse one .class file → [(field_name, desc, is_volatile, is_final_non_cv)]."""
    if data[:4] != b"\xCA\xFE\xBA\xBE":
        fail("input is not a class file")
    count = struct.unpack_from(">H", data, 8)[0]
    pool, p = parse_constant_pool(data, count)
    access, this_idx = struct.unpack_from(">HH", data, p)
    p += 4
    _super_idx = struct.unpack_from(">H", data, p)[0]
    p += 2
    ifc_count = struct.unpack_from(">H", data, p)[0]
    p += 2 + 2 * ifc_count

    def utf8(idx: int) -> str:
        tag, raw = pool[idx]
        assert tag == 1, f"cp entry {idx} is not Utf8 (tag {tag})"
        return raw.decode("utf-8")

    fields: list[tuple[str, str, bool, bool]] = []
    f_count = struct.unpack_from(">H", data, p)[0]
    p += 2
    for _ in range(f_count):
        f_access, name_idx, desc_idx = struct.unpack_from(">HHH", data, p)
        p += 6
        a_count = struct.unpack_from(">H", data, p)[0]
        p += 2
        has_cv = False
        for _ in range(a_count):
            a_name_idx, a_len = struct.unpack_from(">HI", data, p)
            p += 6
            if utf8(a_name_idx) == "ConstantValue":
                has_cv = True
            p += a_len
        # A static final WITH ConstantValue folds (JMM-safe); everything
        # else final is a plain slot with no freeze semantics.
        boundary_final = bool(f_access & ACC_FINAL) and not has_cv
        fields.append(
            (
                utf8(name_idx),
                utf8(desc_idx),
                bool(f_access & ACC_VOLATILE),
                boundary_final,
            )
        )
    return fields


def expected_boundary(classes_dir: pathlib.Path) -> set[tuple[str, str, str]]:
    expected: set[tuple[str, str, str]] = set()
    for cf in sorted(classes_dir.rglob("*.class")):
        cls = cf.stem
        for name, _desc, is_volatile, boundary_final in parse_fields(cf.read_bytes()):
            if is_volatile:
                expected.add((cls, name, "volatile"))
            if boundary_final:
                expected.add((cls, name, "final"))
    return expected


def sidecar_rows(out_dir: pathlib.Path) -> set[tuple[str, str, str]]:
    text = (out_dir / "src" / "jmm_boundary.rs").read_text()
    rows: set[tuple[str, str, str]] = set()
    for line in text.splitlines():
        line = line.strip()
        if line.startswith('("') and line.endswith("),"):
            parts = [p.strip().strip('"') for p in line[1:-2].split(",")]
            if len(parts) != 3:
                fail(f"unparseable sidecar row: {line}")
            rows.add((parts[0], parts[1], parts[2]))
    return rows


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--classes", required=True, help="javac output dir with the input .class files")
    ap.add_argument("--out", required=True, help="j2r compile output dir")
    args = ap.parse_args()

    classes_dir = pathlib.Path(args.classes)
    out_dir = pathlib.Path(args.out)
    if not classes_dir.is_dir():
        fail(f"classes dir missing: {classes_dir}")
    if not (out_dir / "src").is_dir():
        fail(f"emitted crate missing: {out_dir}")

    expected = expected_boundary(classes_dir)
    rows = sidecar_rows(out_dir)
    if rows != expected:
        missing = expected - rows
        extra = rows - expected
        fail(
            "sidecar diverges from the input facts — "
            f"missing={sorted(missing)} extra={sorted(extra)}"
        )

    # Access-site markers: every boundary field >= 1 marker in the
    # generated class sources (src/classes/** — the true access sites;
    # the sidecar AND the registry are excluded so a listed field cannot
    # satisfy its own check via a secondary site); no marker names a
    # non-boundary field.
    class_sources = ""
    for rs in sorted((out_dir / "src").rglob("*.rs")):
        if rs.name == "jmm_boundary.rs":
            continue
        if rs.parent.name == "classes":
            class_sources += rs.read_text()
    for cls, name, kind in sorted(expected):
        needle = f"{MARKER_PREFIX}{cls}.{name} is {kind}"
        if needle not in class_sources:
            fail(f"boundary field {cls}.{name} ({kind}) has NO marker at its access site")
    # Any marker line naming something outside the expected set is drift
    # (swept over every non-sidecar generated source).
    other_sources = ""
    for rs in sorted((out_dir / "src").rglob("*.rs")):
        if rs.name != "jmm_boundary.rs":
            other_sources += rs.read_text()
    for line in other_sources.splitlines():
        s = line.strip()
        if not s.startswith("//") or MARKER_PREFIX not in s:
            continue
        body = s[s.index(MARKER_PREFIX) + len(MARKER_PREFIX) :]
        head, _, tail = body.partition(" is ")
        kind = tail.split()[0].rstrip(".,") if tail else ""
        cls, _, fname = head.rpartition(".")
        if (cls, fname, kind) not in expected:
            fail(f"marker names a NON-boundary field: {s}")

    print(f"JMM boundary enforced: {len(expected)} boundary field(s), all marked; sidecar in sync")
    return 0


if __name__ == "__main__":
    sys.exit(main())
