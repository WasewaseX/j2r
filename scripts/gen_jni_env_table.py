#!/usr/bin/env python3
"""Generate crates/j2r-runtime/src/jni/tables.rs from the pinned JDK's jni.h.

T08 (M5 wave 2, M5_PLAN §3.1). The ABI of record is the Temurin 21 header
that the whole project already uses as its differential oracle:

    $JAVA_HOME/include/jni.h   (Temurin 21.0.13)

The struct JNINativeInterface_ body is parsed slot-by-slot (reserved0..3 +
every `RET (JNICALL *NAME)(ARGS)` declaration, multi-line safe), every C
typedef assumption is VERIFIED against the header (the script fails loudly
on any mismatch — differential discipline applied to the generator itself),
and three artifacts are emitted:

  1. crates/j2r-runtime/src/jni/tables.rs  — the generated Rust table
  2. scripts/evidence/jni_slot_list.txt    — (index, name, C signature) list
  3. a stdout summary (slot count)

Type-fidelity decisions (documented in jni/mod.rs, the T11 trade-off note
pre-flags two of them):
  * jni.h typedefs jclass/jstring/jthrowable/jweak and every j*Array to
    jobject — the Rust mirror keeps exactly that aliasing (verified).
  * `...` (C-varargs) slot variants are typed as their A-form (jvalue*)
    signature with the args pointer widened to *const c_void — a stub never
    reads them; the real variadic-ABI decision is T11's.
  * va_list (V variants) is a pointer-width opaque stand-in until T11.
  * Reserved slots are NULL (*const ()), exactly like the reference table.
"""

import os
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "crates/j2r-runtime/src/jni/tables.rs"
EVIDENCE = ROOT / "scripts/evidence/jni_slot_list.txt"

# Default to the JDK of record at its persistent-layer home (env.sh pins
# JAVA_HOME there). argv[1] still overrides for one-off cross-JDK runs.
# Lesson #5: never default to /tmp — a sandbox reset wipes it and the
# generator would silently lose its reproducibility anchor.
_DEFAULT_JNI_H = (
    pathlib.Path(os.environ.get("JAVA_HOME", "/home/z/my-project/tools/jdk-21.0.13+9"))
    / "include"
    / "jni.h"
)
jni_h = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else _DEFAULT_JNI_H)
if not jni_h.exists():
    sys.exit(f"FATAL: jni.h not found at {jni_h} (pass the path as argv[1])")
header = jni_h.read_text()


def struct_body(name: str) -> str:
    m = re.search(r"^struct " + name + r" \{(.*?)^\};", header, re.S | re.M)
    if not m:
        sys.exit(f"FATAL: struct {name} not found in {jni_h}")
    body = re.sub(r"/\*.*?\*/", " ", m.group(1), flags=re.S)
    return body


# ---------------------------------------------------------------- typedef verification
def has_typedef(alias: str, target: str) -> bool:
    pat = re.compile(r"typedef\s+" + re.escape(target) + r"\s+" + re.escape(alias) + r"\s*;")
    return bool(pat.search(header))


# C section of jni.h: jobject is `struct _jobject *`; jclass/jthrowable/
# jstring/jarray alias jobject directly; every j*Array aliases jarray (which
# aliases jobject — transitively the same ABI); jweak aliases jobject.
for alias in ["jclass", "jthrowable", "jstring", "jarray", "jweak"]:
    if not has_typedef(alias, "jobject"):
        sys.exit(f"FATAL: expected `typedef jobject {alias};` not found in jni.h")
for alias in ["jbooleanArray", "jbyteArray", "jcharArray", "jshortArray",
              "jintArray", "jlongArray", "jfloatArray", "jdoubleArray",
              "jobjectArray"]:
    if not has_typedef(alias, "jarray"):
        sys.exit(f"FATAL: expected `typedef jarray {alias};` not found in jni.h")
if not re.search(r"typedef struct _jobject \*jobject;", header):
    sys.exit("FATAL: `typedef struct _jobject *jobject;` not found in jni.h")
for alias in ["jmethodID", "jfieldID"]:
    if not re.search(r"typedef struct _j\w+ \*" + alias + r";", header):
        sys.exit(f"FATAL: opaque struct-pointer typedef for {alias} not found in jni.h")
if "JNIInvalidRefType" not in header or "JNINativeMethod" not in header:
    sys.exit("FATAL: jobjectRefType enum or JNINativeMethod struct not found in jni.h")

# ---------------------------------------------------------------- slot extraction
native_body = struct_body("JNINativeInterface_")
invoke_body = struct_body("JNIInvokeInterface_")

# collapse whitespace so multi-line declarations become single records
flat = re.sub(r"\s+", " ", native_body)

func_pat = re.compile(
    r"([A-Za-z_][\w \*]*?)\s*\(\s*JNICALL\s*\*\s*(\w+)\s*\)\s*\(([^;]*)\)"
)
slots: list[tuple[int, str, str, str]] = []  # (index, kind, name, ret_c, args_c)
order: list[tuple[str, str, str, str]] = []  # (kind, name, ret, args)
for token in re.split(r";", flat):
    token = token.strip()
    if not token:
        continue
    rm = re.fullmatch(r"void\s*\*\s*reserved(\d)", token)
    if rm:
        order.append(("reserved", f"reserved{rm.group(1)}", "", ""))
        continue
    fm = func_pat.search(token)
    if fm:
        order.append(("func", fm.group(2), fm.group(1).strip(), fm.group(3).strip()))
        continue
    sys.exit(f"FATAL: unparseable slot token: {token!r}")

# sanity: reserved0..3 must be the first four slots, in order
for i in range(4):
    if order[i][0] != "reserved" or order[i][1] != f"reserved{i}":
        sys.exit(f"FATAL: slot {i} is {order[i]}, expected reserved{i}")
slots = [(i, *order[i]) for i in range(len(order))]
n_funcs = sum(1 for s in slots if s[1] == "func")
n_reserved = sum(1 for s in slots if s[1] == "reserved")
print(f"parsed {len(slots)} slots ({n_reserved} reserved + {n_funcs} functions)")



# ---------------------------------------------------------------- package
# wiring (the 2026-11-19 scanaislop split: the data tables and the
# family builders live under scripts/jni_env_table/, the parse + verify
# + emit stay here; the generator's OUTPUT is byte-identical, proven at
# a pinned PYTHONHASHSEED against the pre-split run)
import os as _os
import sys as _sys

_sys.path.insert(0, _os.path.dirname(_os.path.abspath(__file__)))
from jni_env_table.mapping import rust_sig, rust_stub  # noqa: E402
from jni_env_table.data_real_slots import REAL_SLOTS  # noqa: E402
from jni_env_table.data_wrapper_bodies import WRAPPER_BODIES  # noqa: E402
from jni_env_table.families_arrays import register  # noqa: E402
from jni_env_table import families_calls  # noqa: E402,F401
from jni_env_table import families_fields  # noqa: E402,F401

# register() runs the array family emitters (the pre-r9 code extended
# WRAPPER_BODIES at import time); the field + call families take the
# parsed slot index - same key-set mutations, order-insensitive bytes
# because the emit iterates REAL_SLOTS, never WRAPPER_BODIES
register()
families_fields.build(slots)
_idx_of = {name: i for (i, kind, name, r, a) in slots if kind == "func"}
families_calls.build(_idx_of)

# ---------------------------------------------------------------- T10 slice 2/3: real-slot registry verification
# The registry must name slots that jni.h actually defines (a renamed/
# removed slot fails the generator loudly — same discipline as the
# typedef verification above) and every entry needs a body.
slot_names = {name for (_i, kind, name, _r, _a) in slots if kind == "func"}
for name in REAL_SLOTS:
    if name not in slot_names:
        sys.exit(f"FATAL: REAL_SLOTS names slot {name!r}, which jni.h does not define")
    if name not in WRAPPER_BODIES:
        sys.exit(f"FATAL: REAL_SLOTS entry {name!r} has no WRAPPER_BODIES body")
for name in WRAPPER_BODIES:
    if name not in REAL_SLOTS:
        sys.exit(f"FATAL: WRAPPER_BODIES entry {name!r} is not in REAL_SLOTS")
print(f"real slots wired: {len(REAL_SLOTS)} ({', '.join(sorted(REAL_SLOTS))})")


# ---------------------------------------------------------------- emit
L: list[str] = []
A = L.append
A("//! GENERATED FILE — do not edit by hand. Regenerate with:")
A("//!   python3 scripts/gen_jni_env_table.py $JAVA_HOME/include/jni.h")
A("//!")
A("//! T08 (M5 wave 2, M5_PLAN §3.1): the JNIEnv/JNIInvokeInterface static")
A("//! tables, index-for-index identical to the Temurin 21 jni.h of record.")
A("//! Every non-reserved slot holds a real function or a loud stub — never")
A("//! a null slot a foreign library could segfault on. Layout is pinned by")
A("//! the const assertion table at the bottom (offset_of! per slot).")
A("//!")
A("//! T10 + T11: the real slots are wired through the generator's")
A("//! REAL_SLOTS registry (the count prints at generation time): the five")
A("//! string slots (bodies in jni::strings, slice 1), FindClass/")
A("//! GetObjectClass + the field-ID pair + the 36 field accessors + the")
A("//! 46 array slots (bodies in jni::classes/jni::arrays, T10), the")
A("//! pending-exception trio + the method-ID pair + all thirty")
A("//! Call*MethodA slots (virtual 11b-1, static/nonvirtual 11b-2; bodies")
A("//! in jni::calls); raw-pointer moments go through the audited")
A("//! j2r-jni-ffi primitives.")
A("#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]")
A("")
A("use std::os::raw::{c_char, c_void};")
A("")
A("#[repr(C)] pub struct JObjectOpaque { _private: [u8; 0] }")
A("#[repr(C)] pub struct VaListOpaque { _private: [u8; 0] }")
A("")
A("pub type jobject = *mut JObjectOpaque;")
A("pub type jint = i32;")
A("pub type jlong = i64;")
A("pub type jbyte = i8;")
A("pub type jboolean = u8;")
A("pub type jchar = u16;")
A("pub type jshort = i16;")
A("pub type jfloat = f32;")
A("pub type jdouble = f64;")
A("pub type jsize = jint;")
A("pub type jobjectRefType = i32;")
A("pub type VaList = *mut VaListOpaque;")
A("#[repr(C)] pub struct JNINativeMethod { pub name: *mut c_char, pub signature: *mut c_char, pub fnPtr: *mut c_void }")
A("")
A("// jni.h type aliases, verified against the header by the generator:")
A("pub type jclass = jobject;")
A("pub type jstring = jobject;")
A("pub type jthrowable = jobject;")
A("pub type jweak = jobject;")
for t in ["jobjectArray", "jintArray", "jbooleanArray", "jbyteArray",
          "jcharArray", "jshortArray", "jlongArray", "jfloatArray", "jdoubleArray"]:
    A(f"pub type {t} = jobject;")
A("pub type jmethodID = *mut c_void;")
A("pub type jfieldID = *mut c_void;")
A("")
A("fn unimplemented_slot(index: usize, name: &'static str) -> ! {")
A("    panic!(\"j2r-jni: table slot {index} ({name}) is not implemented yet; it is registered for a later wave-2 task\")")
A("}")
A("")

# real implementations first (GetVersion here; GetEnv in the VM table)
A("/// The Temurin 21.0.13+9 oracle's literal GetVersion answer (measured")
A("/// live by the T13 C harness: the JDK-major form 0x0015_0000 — the old")
A("/// 1.8 assumption was falsified by the harness and corrected to the")
A("/// measured value, per the three-channel differential law).")
A("extern \"system\" fn jni_get_version(_env: JNIEnv) -> jint { crate::jni::JNI_VERSION_21_0 }")
A("")

env_fields: list[str] = []
env_inits: list[str] = []
stub_defs: list[str] = []

for index, kind, name, ret_c, args_c in slots:
    if kind == "reserved":
        env_fields.append(f"    pub {name}: Option<extern \"system\" fn()>, // slot {index} — reserved (None == NULL, per jni.h)")
        env_inits.append(f"        {name}: None,")
        continue
    field_ty = rust_sig(ret_c, args_c)
    env_fields.append(f"    pub {name}: {field_ty}, // slot {index}")
    if name == "GetVersion":
        env_inits.append(f"        {name}: jni_get_version,")
    elif name in REAL_SLOTS:
        env_inits.append(f"        {name}: {REAL_SLOTS[name]},")
    else:
        s, _ = rust_stub(ret_c, args_c, name, index)
        stub_defs.append(s)
        env_inits.append(f"        {name}: stub_{name},")

A("#[repr(C)] pub struct JNINativeInterface_ {")
L.extend(env_fields)
A("}")
A("")
A("#[repr(C)] pub struct JNIEnv_ { pub functions: &'static JNINativeInterface_ }")
A("#[repr(C)] pub struct JavaVM_ { pub functions: &'static JNIInvokeInterface_ }")
A("pub type JNIEnv = *mut JNIEnv_;")
A("pub type JavaVM = *mut JavaVM_;")
A("")
A("pub static ENV_TABLE: JNINativeInterface_ = JNINativeInterface_ {")
L.extend(env_inits)
A("};")
A("")

# ---- InvokeInterface (8 slots)
A("#[repr(C)] pub struct JNIInvokeInterface_ {")
A("    pub reserved0: Option<extern \"system\" fn()>,")
A("    pub reserved1: Option<extern \"system\" fn()>,")
A("    pub reserved2: Option<extern \"system\" fn()>,")
A("    pub DestroyJavaVM: extern \"system\" fn(JavaVM) -> jint, // slot 3")
A("    pub AttachCurrentThread: extern \"system\" fn(JavaVM, *mut *mut c_void, *mut c_void) -> jint, // slot 4")
A("    pub DetachCurrentThread: extern \"system\" fn(JavaVM) -> jint, // slot 5")
A("    pub GetEnv: extern \"system\" fn(JavaVM, *mut *mut c_void, jint) -> jint, // slot 6")
A("    pub AttachCurrentThreadAsDaemon: extern \"system\" fn(JavaVM, *mut *mut c_void, *mut c_void) -> jint, // slot 7")
A("}")
A("")
A("extern \"system\" fn stub_destroy_java_vm(_vm: JavaVM) -> jint { unimplemented_slot(3, \"DestroyJavaVM\") }")
A("extern \"system\" fn stub_attach_current_thread(_vm: JavaVM, _penv: *mut *mut c_void, _args: *mut c_void) -> jint { unimplemented_slot(4, \"AttachCurrentThread\") }")
A("extern \"system\" fn stub_detach_current_thread(_vm: JavaVM) -> jint { unimplemented_slot(5, \"DetachCurrentThread\") }")
A("extern \"system\" fn stub_attach_as_daemon(_vm: JavaVM, _penv: *mut *mut c_void, _args: *mut c_void) -> jint { unimplemented_slot(7, \"AttachCurrentThreadAsDaemon\") }")
A("/// Real as of T13 13c (the loading story, docs/M5_JNI_HARNESS.md §7):")
A("/// a supported version (1.1..1.8, 9, 10 — the same set HotSpot 21")
A("/// accepts, probed live by the T13 C harness) writes the env table")
A("/// pointer through audited primitive 3 (write_ptr_out — the F-J1")
A("/// resolution's own primitive) and answers JNI_OK; any other version")
A("/// answers JNI_EVERSION and writes nothing; a null penv is JNI_ERR.")
A("extern \"system\" fn jni_get_env(_vm: JavaVM, penv: *mut *mut c_void, version: jint) -> jint {")
A("    if penv.is_null() { return crate::jni::JNI_ERR; }")
A("    if !matches!(")
A("        version,")
A("        crate::jni::JNI_VERSION_1_1")
A("            | crate::jni::JNI_VERSION_1_2")
A("            | crate::jni::JNI_VERSION_1_4")
A("            | crate::jni::JNI_VERSION_1_6")
A("            | crate::jni::JNI_VERSION_1_8")
A("            | crate::jni::JNI_VERSION_9")
A("            | crate::jni::JNI_VERSION_10")
A("    ) {")
A("        return crate::jni::JNI_EVERSION;")
A("    }")
A("    j2r_jni_ffi::ffi::write_ptr_out(penv, jni_env() as usize);")
A("    crate::jni::JNI_OK")
A("}")
A("")
A("pub static VM_TABLE: JNIInvokeInterface_ = JNIInvokeInterface_ {")
A("    reserved0: None,")
A("    reserved1: None,")
A("    reserved2: None,")
A("    DestroyJavaVM: stub_destroy_java_vm,")
A("    AttachCurrentThread: stub_attach_current_thread,")
A("    DetachCurrentThread: stub_detach_current_thread,")
A("    GetEnv: jni_get_env,")
A("    AttachCurrentThreadAsDaemon: stub_attach_as_daemon,")
A("};")
A("")
A("pub static JNI_ENV: JNIEnv_ = JNIEnv_ { functions: &ENV_TABLE };")
A("pub static JAVA_VM: JavaVM_ = JavaVM_ { functions: &VM_TABLE };")
A("/// The pointer a native library receives. Creating a raw pointer without")
A("/// dereferencing it is safe; the foreign side's FFI is theirs (M5_PLAN §3.1).")
A("pub fn jni_env() -> JNIEnv { &JNI_ENV as *const JNIEnv_ as JNIEnv }")
A("pub fn java_vm() -> JavaVM { &JAVA_VM as *const JavaVM_ as JavaVM }")
A("")

# ---- const assertion table over every slot
A("const _: () = {")
A("    use std::mem::{offset_of, size_of};")
A("    const W: usize = size_of::<*const ()>();")
for index, kind, name, _r, _a in slots:
    expr = {0: "0", 1: "W"}.get(index, f"{index} * W")
    A(f"    assert!(offset_of!(JNINativeInterface_, {name}) == {expr}); // slot {index}")
A("    assert!(size_of::<JNINativeInterface_>() == " + str(len(slots)) + " * W);")
A("    assert!(offset_of!(JNIEnv_, functions) == 0);")
A("    assert!(offset_of!(JavaVM_, functions) == 0);")
A("    assert!(size_of::<JNIEnv>() == W);")
A("    assert!(size_of::<JavaVM>() == W);")
A("    assert!(offset_of!(JNIInvokeInterface_, DestroyJavaVM) == 3 * W);")
A("    assert!(offset_of!(JNIInvokeInterface_, GetEnv) == 6 * W);")
A("    assert!(size_of::<JNIInvokeInterface_>() == 8 * W);")
A("};")
A("")

# ---- positional non-null census (safe: fn-pointer-to-usize casts at runtime)
non_reserved = [n for (i, k, n, _r, _a) in slots if k == "func"]
A("/// (slot name, fn pointer as usize) for every non-reserved slot — the")
A("/// no-null-slots evidence; a native lib must never meet a null slot.")
A("/// A runtime fn: fn-pointer-to-usize casts are not const-evaluable.")
A("pub fn env_slot_addrs() -> Vec<(&'static str, usize)> {")
A("    vec![")
for name in non_reserved:
    A('        ("' + name + '", ENV_TABLE.' + name + ' as usize),')
A("    ]")
A("}")
A("")

L.extend(stub_defs)

# T10 slice 2: the real-slot wrappers, emitted after the stubs (each with
# its doc comment; the registry verified them against the parsed header).
A("")
A("// ---- T10 slices 2-3: real slot wrappers (string family + the")
A("// class/field surface). The REAL_SLOTS registry above points these")
A("// slots at them; the loud-stub default still covers every")
A("// not-yet-wired slot. ----")
for name in REAL_SLOTS:
    A("")
    for line in WRAPPER_BODIES[name]:
        A(line)

OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_text("\n".join(L) + "\n")
EVIDENCE.parent.mkdir(parents=True, exist_ok=True)
with EVIDENCE.open("w") as f:
    f.write(f"# JNI slot list — extracted from {jni_h}\n")
    f.write(f"# {len(slots)} slots: {n_reserved} reserved + {n_funcs} functions\n")
    for index, kind, name, ret_c, args_c in slots:
        if kind == "reserved":
            f.write(f"{index:3}  {name}: NULL\n")
        else:
            f.write(f"{index:3}  {name}: {ret_c.strip()} ({args_c})\n")
print(f"wrote {OUT} ({len(slots)} slots)")
print(f"wrote {EVIDENCE}")
