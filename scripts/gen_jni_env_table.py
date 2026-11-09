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

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "crates/j2r-runtime/src/jni/tables.rs"
EVIDENCE = ROOT / "scripts/evidence/jni_slot_list.txt"

jni_h = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/jdk21/include/jni.h")
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


# ---------------------------------------------------------------- C -> Rust mapping
PRIM = {
    "jint": "jint",
    "jsize": "jsize",
    "jlong": "jlong",
    "jboolean": "jboolean",
    "jchar": "jchar",
    "jbyte": "jbyte",
    "jshort": "jshort",
    "jfloat": "jfloat",
    "jdouble": "jdouble",
}
JOBJECT_ALIASES = {
    "jobject", "jclass", "jstring", "jthrowable", "jweak",
    "jobjectArray", "jintArray", "jbooleanArray", "jbyteArray",
    "jcharArray", "jshortArray", "jlongArray", "jfloatArray", "jdoubleArray",
}


def rust_base(c: str) -> str:
    c = c.strip()
    if c in PRIM:
        return PRIM[c]
    if c in JOBJECT_ALIASES or c == "jarray":
        return "jobject"
    if c in ("jmethodID", "jfieldID", "jvalue"):
        # jvalue* (A-form args) is a pointer-width pass-through for T08;
        # decoding arrives with the T11 arity shims. See jni/mod.rs.
        return "c_void"
    if c == "jobjectRefType":
        return "jobjectRefType"
    if c == "void":
        return "c_void"
    if c == "char":
        return "c_char"
    if c == "JavaVM":
        return "JavaVM_"
    if c == "JNIEnv":
        return "JNIEnv_"
    if c == "JNINativeMethod":
        return "JNINativeMethod"
    if c == "va_list":
        return "VaList"
    sys.exit(f"FATAL: no Rust mapping for C base type {c!r}")


def rust_type(ctype: str) -> str:
    """ctype like `jclass`, `const char *`, `void **`, `jint`, `JavaVM *`."""
    c = ctype.strip()
    ptr = 0
    while c.endswith("*"):
        ptr += 1
        c = c[:-1].strip()
    const = c.startswith("const ")
    base = c[6:].strip() if const else c
    if ptr == 0:
        if base == "void":
            sys.exit("FATAL: bare void outside return position")
        return rust_base(base)
    inner = "c_void" if base == "void" else rust_base(base)
    t = inner
    for _ in range(ptr - 1):
        t = f"*mut {t}"
    first = "*const" if const else "*mut"
    return f"{first} {t}"


def split_args(args_c: str) -> list[str]:
    if args_c.strip() in ("", "void"):
        return []
    return [a.strip() for a in args_c.split(",")]


def rust_sig(ret_c: str, args_c: str) -> str:
    args = []
    for a in split_args(args_c):
        if a == "...":
            # C-varargs slot typed at its A-form arity (jvalue* widened to
            # *const c_void); the stub never reads it — the real variadic
            # ABI decision is T11's. See jni/mod.rs.
            args.append("_args: *const c_void")
            continue
        am = re.match(r"^(.+?)\s*([A-Za-z_]\w*)$", a)
        if not am:
            sys.exit(f"FATAL: cannot split arg {a!r}")
        args.append(rust_type(am.group(1)))
    ret = "()" if ret_c.strip() == "void" else rust_type(ret_c)
    arg_list = ", ".join(args)
    return f"extern \"system\" fn({arg_list}) -> {ret}"


def rust_stub(ret_c: str, args_c: str, name: str, index: int) -> tuple[str, str]:
    """(fn definition, doc) for the loud stub of one slot."""
    args = []
    for k, a in enumerate(split_args(args_c)):
        if a == "...":
            args.append("_args: *const c_void")
            continue
        am = re.match(r"^(.+?)\s*([A-Za-z_]\w*)$", a)
        if not am:
            sys.exit(f"FATAL: cannot split arg {a!r}")
        aname = "_env" if am.group(2) == "env" else f"_{am.group(2)}"
        args.append(f"{aname}: {rust_type(am.group(1))}")
    ret = "" if ret_c.strip() == "void" else f" -> {rust_type(ret_c)}"
    sig_args = ", ".join(args)
    return (
        f"extern \"system\" fn stub_{name}({sig_args}){ret} {{ unimplemented_slot({index}, \"{name}\") }}",
        "",
    )


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

# real implementations first (GetVersion here; GetEnv partial in the VM table)
A("extern \"system\" fn jni_get_version(_env: JNIEnv) -> jint { crate::jni::JNI_VERSION_1_8 }")
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
A("/// Partially real: the version check needs no pointer write. A supported")
A("/// version would have to write through the caller's `penv` — impossible")
A("/// under forbid(unsafe_code); that resolution is the registered wave-2")
A("/// finding (see jni/mod.rs).")
A("extern \"system\" fn jni_get_env(_vm: JavaVM, penv: *mut *mut c_void, version: jint) -> jint {")
A("    if penv.is_null() { return crate::jni::JNI_ERR; }")
A("    if version > crate::jni::JNI_VERSION_1_8 { return crate::jni::JNI_EVERSION; }")
A("    unimplemented_slot(6, \"GetEnv(supported version)\")")
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
