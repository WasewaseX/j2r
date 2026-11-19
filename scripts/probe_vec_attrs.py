#!/usr/bin/env python3
"""T15.2b probe-first: what do the 8 committed remap vectors carry?

Prints, per vector: class-level attribute names, per-member attribute
names, the Code-internal attribute names, and every Signature/LVTT
generic-signature TEXT found. The T15.2b laws pin exactly these facts -
writing them before the probe is how the 2a area()D red happened.
"""
import struct
import sys
import os

VEC_DIR = os.path.join(
    os.path.dirname(__file__), "..", "crates", "j2r-classfile", "tests", "vectors", "remap"
)

VECTORS = [
    "RemapVec1.class",
    "RemapVec2.class",
    "RemapVec2$Inner.class",
    "RemapVec2$Box.class",
    "RemapVec2$Shape.class",
    "RemapVec3.class",
    "RemapVec3$Pair.class",
    "RemapVec3$Tone.class",
]

def read_pool(data):
    n, = struct.unpack_from(">H", data, 8)
    pos = 10
    pool = {0: None}
    i = 1
    while i < n:
        tag = data[pos]
        if tag == 1:
            ln, = struct.unpack_from(">H", data, pos + 1)
            pool[i] = ("Utf8", data[pos + 3: pos + 3 + ln].decode("utf-8", "replace"))
            pos += 3 + ln
        elif tag in (7, 8, 16, 19, 20):
            ref, = struct.unpack_from(">H", data, pos + 1)
            pool[i] = (["Class", "String", "MethodType", "Module", "Package"][ (7,8,16,19,20).index(tag) ], ref)
            pos += 3
        elif tag == 15:
            pool[i] = ("MethodHandle-kind",)
            pos += 4
        elif tag in (3, 4, 9, 10, 11, 12, 17, 18):
            pool[i] = ({3: "Int", 4: "Float", 9: "FieldRef", 10: "MethodRef",
                        11: "IfMethodRef", 12: "NameAndType", 17: "Dynamic",
                        18: "InvokeDynamic"}[tag],)
            pos += 5
        elif tag in (5, 6):
            pool[i] = ("LongOrDouble",)
            pos += 9
            i += 1  # takes two slots
        else:
            raise SystemExit(f"unknown tag {tag} at slot {i}")
        i += 1
    return pool, pos

def attrs(data, pos, pool):
    n, = struct.unpack_from(">H", data, pos)
    pos += 2
    out = []
    for _ in range(n):
        ni, ln = struct.unpack_from(">HI", data, pos)
        name = pool[ni][1]
        body = data[pos + 6: pos + 6 + ln]
        out.append((name, body))
        pos += 6 + ln
    return out, pos

def members(data, pos, pool, label):
    n, = struct.unpack_from(">H", data, pos)
    pos += 2
    for _ in range(n):
        acc, ni, di, an = struct.unpack_from(">HHHH", data, pos)
        pos += 6  # access+name+desc; the attr count belongs to attrs()
        attrs_list, pos = attrs(data, pos, pool)
        print(f"  {label} {pool[ni][1]} {pool[di][1]}: {[a[0] for a in attrs_list]}")
        for name, body in attrs_list:
            if name == "Signature":
                si, = struct.unpack_from(">H", body, 0)
                print(f"    Signature text: {pool[si][1]}")
            if name == "Code":
                clen, = struct.unpack_from(">I", body, 4)
                cpos = 8 + clen
                en, = struct.unpack_from(">H", body, cpos)
                cpos += 2 + 8 * en
                in_attrs, _ = attrs(body, cpos, pool)
                print(f"    Code-internal attrs: {[a[0] for a in in_attrs]}")
                for iname, ibody in in_attrs:
                    if iname == "LocalVariableTypeTable":
                        cnt, = struct.unpack_from(">H", ibody, 0)
                        for k in range(cnt):
                            off = 2 + 10 * k
                            _, _, sni, sgi, _ = struct.unpack_from(">HHHHH", ibody, off)
                            print(f"      LVTT[{k}] name={pool[sni][1]} sig={pool[sgi][1]}")
    return pos

def cls(pool, idx):
    e = pool.get(idx)
    if e and e[0] == "Class":
        ni = e[1]
        return pool[ni][1] if ni in pool and len(pool[ni]) > 1 else f"Class#{idx}->utf8#{ni}"
    return f"?#{idx}"

def main():
    for v in VECTORS:
        path = os.path.join(VEC_DIR, v)
        data = open(path, "rb").read()
        pool, pos = read_pool(data)
        acc, this_c, sup_c = struct.unpack_from(">HHH", data, pos)
        pos += 6
        ifc, = struct.unpack_from(">H", data, pos)
        pos += 2 + 2 * ifc
        major, = struct.unpack_from(">H", data, 6)
        print(f"== {v} (major {major}) this={cls(pool, this_c)}")
        pos = members(data, pos, pool, "field")
        pos = members(data, pos, pool, "method")
        cl_attrs, pos = attrs(data, pos, pool)
        print(f"  class attrs: {[a[0] for a in cl_attrs]}")
        for name, body in cl_attrs:
            if name == "Signature":
                si, = struct.unpack_from(">H", body, 0)
                print(f"    Signature text: {pool[si][1]}")
            if name == "InnerClasses":
                cnt, = struct.unpack_from(">H", body, 0)
                print(f"    InnerClasses entries: {cnt}")
            if name == "NestHost":
                hi, = struct.unpack_from(">H", body, 0)
                print(f"    NestHost -> {cls(pool, hi)}")
            if name == "NestMembers":
                cnt, = struct.unpack_from(">H", body, 0)
                names = [cls(pool, struct.unpack_from('>H', body, 2+2*k)[0]) for k in range(cnt)]
                print(f"    NestMembers: {names}")
            if name == "Exceptions":
                cnt, = struct.unpack_from(">H", body, 0)
                names = [cls(pool, struct.unpack_from('>H', body, 2+2*k)[0]) for k in range(cnt)]
                print(f"    Exceptions: {names}")
        print(f"  trailing bytes after members: {len(data) - pos}")

if __name__ == "__main__":
    main()
