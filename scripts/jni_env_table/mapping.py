"""C -> Rust type mapping (verbatim from the pre-split generator)."""
import re
import sys


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
    if c in ("jmethodID", "jfieldID"):
        # T10 slice 3 amendment (M5_JNI_CLASSES.md §3): these ARE opaque
        # struct-POINTER typedefs in jni.h (verified above), so the
        # ABI-faithful Rust form is the pointer alias — the T08 bare-
        # c_void stand-in made those slots uninhabited and the real
        # wrappers impossible (the same shape F-J1 called out for
        # pointer-out slots).
        return c
    if c == "jvalue":
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

