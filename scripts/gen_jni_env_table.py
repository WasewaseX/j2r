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
import subprocess
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


# ---------------------------------------------------------------- T10 slice 2: the real-slot registry
# Slot name -> wrapper fn name. Every entry MUST also have a body in
# WRAPPER_BODIES below, and must name a slot parsed from jni.h (verified
# after parsing — a renamed slot fails the generator loudly).
# Semantics live in j2r-runtime::jni::strings (slice 1); the raw-pointer
# moments go through the audited j2r-jni-ffi primitives (F-J1 resolution,
# M5_JNI_FFI.md §4 — primitive 7 `write_u8_out` was added by this slice).
REAL_SLOTS = {
    "NewString": "jni_new_string",
    "NewStringUTF": "jni_new_string_utf",
    "GetStringUTFLength": "jni_get_string_utf_length",
    "GetStringUTFChars": "jni_get_string_utf_chars",
    "ReleaseStringUTFChars": "jni_release_string_utf_chars",
    "FindClass": "jni_find_class",
    "GetObjectClass": "jni_get_object_class",
    "GetFieldID": "jni_get_field_id",
    "GetStaticFieldID": "jni_get_static_field_id",
    # ---- T10 slice 3b: the array families (docs/M5_JNI_CLASSES.md §5).
    # Slot numbers verified against the generated offset_of! table (the
    # design note's "217/218" for the critical pair was a typo: 217/218
    # are MonitorEnter/MonitorExit; the critical pair is 222/223).
    "GetArrayLength": "jni_get_array_length",
    "NewObjectArray": "jni_new_object_array",
    "GetObjectArrayElement": "jni_get_object_array_element",
    "SetObjectArrayElement": "jni_set_object_array_element",
    "NewBooleanArray": "jni_new_boolean_array",
    "NewByteArray": "jni_new_byte_array",
    "NewCharArray": "jni_new_char_array",
    "NewShortArray": "jni_new_short_array",
    "NewIntArray": "jni_new_int_array",
    "NewLongArray": "jni_new_long_array",
    "NewFloatArray": "jni_new_float_array",
    "NewDoubleArray": "jni_new_double_array",
    "GetBooleanArrayElements": "jni_get_boolean_array_elements",
    "GetByteArrayElements": "jni_get_byte_array_elements",
    "GetCharArrayElements": "jni_get_char_array_elements",
    "GetShortArrayElements": "jni_get_short_array_elements",
    "GetIntArrayElements": "jni_get_int_array_elements",
    "GetLongArrayElements": "jni_get_long_array_elements",
    "GetFloatArrayElements": "jni_get_float_array_elements",
    "GetDoubleArrayElements": "jni_get_double_array_elements",
    "ReleaseBooleanArrayElements": "jni_release_boolean_array_elements",
    "ReleaseByteArrayElements": "jni_release_byte_array_elements",
    "ReleaseCharArrayElements": "jni_release_char_array_elements",
    "ReleaseShortArrayElements": "jni_release_short_array_elements",
    "ReleaseIntArrayElements": "jni_release_int_array_elements",
    "ReleaseLongArrayElements": "jni_release_long_array_elements",
    "ReleaseFloatArrayElements": "jni_release_float_array_elements",
    "ReleaseDoubleArrayElements": "jni_release_double_array_elements",
    "GetBooleanArrayRegion": "jni_get_boolean_array_region",
    "GetByteArrayRegion": "jni_get_byte_array_region",
    "GetCharArrayRegion": "jni_get_char_array_region",
    "GetShortArrayRegion": "jni_get_short_array_region",
    "GetIntArrayRegion": "jni_get_int_array_region",
    "GetLongArrayRegion": "jni_get_long_array_region",
    "GetFloatArrayRegion": "jni_get_float_array_region",
    "GetDoubleArrayRegion": "jni_get_double_array_region",
    "SetBooleanArrayRegion": "jni_set_boolean_array_region",
    "SetByteArrayRegion": "jni_set_byte_array_region",
    "SetCharArrayRegion": "jni_set_char_array_region",
    "SetShortArrayRegion": "jni_set_short_array_region",
    "SetIntArrayRegion": "jni_set_int_array_region",
    "SetLongArrayRegion": "jni_set_long_array_region",
    "SetFloatArrayRegion": "jni_set_float_array_region",
    "SetDoubleArrayRegion": "jni_set_double_array_region",
    "GetPrimitiveArrayCritical": "jni_get_primitive_array_critical",
    "ReleasePrimitiveArrayCritical": "jni_release_primitive_array_critical",
    # ---- T11 slice 11b: the pending-exception trio, the method-ID pair,
    # and the ten virtual Call*MethodA slots (docs/M5_JNI_CALLS.md §3).
    "ExceptionOccurred": "jni_exception_occurred",
    "ExceptionClear": "jni_exception_clear",
    "ExceptionCheck": "jni_exception_check",
    "GetMethodID": "jni_get_method_id",
    "GetStaticMethodID": "jni_get_static_method_id",
    "CallObjectMethodA": "jni_call_object_method_a",
    "CallBooleanMethodA": "jni_call_boolean_method_a",
    "CallByteMethodA": "jni_call_byte_method_a",
    "CallCharMethodA": "jni_call_char_method_a",
    "CallShortMethodA": "jni_call_short_method_a",
    "CallIntMethodA": "jni_call_int_method_a",
    "CallLongMethodA": "jni_call_long_method_a",
    "CallFloatMethodA": "jni_call_float_method_a",
    "CallDoubleMethodA": "jni_call_double_method_a",
    "CallVoidMethodA": "jni_call_void_method_a",
    # ---- T11 slice 11b-2: the static/nonvirtual Call*MethodA families
    # (docs/M5_JNI_CALLS.md §3/§4) — same core, three resolution laws.
    "CallStaticObjectMethodA": "jni_call_static_object_method_a",
    "CallStaticBooleanMethodA": "jni_call_static_boolean_method_a",
    "CallStaticByteMethodA": "jni_call_static_byte_method_a",
    "CallStaticCharMethodA": "jni_call_static_char_method_a",
    "CallStaticShortMethodA": "jni_call_static_short_method_a",
    "CallStaticIntMethodA": "jni_call_static_int_method_a",
    "CallStaticLongMethodA": "jni_call_static_long_method_a",
    "CallStaticFloatMethodA": "jni_call_static_float_method_a",
    "CallStaticDoubleMethodA": "jni_call_static_double_method_a",
    "CallStaticVoidMethodA": "jni_call_static_void_method_a",
    "CallNonvirtualObjectMethodA": "jni_call_nonvirtual_object_method_a",
    "CallNonvirtualBooleanMethodA": "jni_call_nonvirtual_boolean_method_a",
    "CallNonvirtualByteMethodA": "jni_call_nonvirtual_byte_method_a",
    "CallNonvirtualCharMethodA": "jni_call_nonvirtual_char_method_a",
    "CallNonvirtualShortMethodA": "jni_call_nonvirtual_short_method_a",
    "CallNonvirtualIntMethodA": "jni_call_nonvirtual_int_method_a",
    "CallNonvirtualLongMethodA": "jni_call_nonvirtual_long_method_a",
    "CallNonvirtualFloatMethodA": "jni_call_nonvirtual_float_method_a",
    "CallNonvirtualDoubleMethodA": "jni_call_nonvirtual_double_method_a",
    "CallNonvirtualVoidMethodA": "jni_call_nonvirtual_void_method_a",
}

# Wrapper bodies, emitted verbatim (doc comment + fn) after the stubs.
# Indentation: top level, 4-space body indent.
WRAPPER_BODIES: dict[str, list[str]] = {
    "NewStringUTF": [
        "/// `NewStringUTF` (slot 167): NUL-terminated modified UTF-8 → a",
        "/// rooted local-ref `jstring` (T10 slice 2). Null input or malformed",
        "/// mUTF-8 → JNI null (the spec's NPE/NoSuchElementError shapes need the",
        "/// exception machinery — v1 returns null; registered for the T13",
        "/// harness, which judges against Temurin three-channel). The bytes",
        "/// cross through the audited `read_caller_str` primitive; the decode",
        "/// is the slice-1 classfile-mirror law.",
        'extern "system" fn jni_new_string_utf(_env: JNIEnv, utf: *const c_char) -> jobject {',
        "    let Some(bytes) = j2r_jni_ffi::ffi::read_caller_str(utf) else {",
        "        return std::ptr::null_mut();",
        "    };",
        "    match crate::jni::strings::slot_new_string_utf(&bytes) {",
        "        Some(tag) => crate::jni::bridge::tag_to_jobject(tag),",
        "        None => std::ptr::null_mut(),",
        "    }",
        "}",
    ],
    "NewString": [
        "/// `NewString` (slot 163): UTF-16 code units (native-endian — the",
        "/// ABI's own shape) → a rooted local-ref `jstring` (T10 slice 2).",
        "/// Null units or negative length → JNI null (spec NPE shape pending the",
        "/// exception machinery — documented v1 gap, T13-registered). Unpaired",
        "/// surrogates normalize to U+FFFD — the pinned lossy law. A zero",
        "/// length with non-null units is the valid empty string.",
        'extern "system" fn jni_new_string(_env: JNIEnv, units: *const jchar, len: jsize) -> jobject {',
        "    if units.is_null() || len < 0 {",
        "        return std::ptr::null_mut();",
        "    }",
        "    let bytes = j2r_jni_ffi::ffi::copy_from_caller(units as *const u8, (len as usize) * 2);",
        "    let mut packed = vec![0u16; len as usize];",
        "    for (slot16, pair) in packed.iter_mut().zip(bytes.as_chunks::<2>().0) {",
        "        *slot16 = u16::from_ne_bytes(*pair);",
        "    }",
        "    crate::jni::bridge::tag_to_jobject(crate::jni::strings::slot_new_string(&packed))",
        "}",
    ],
    "GetStringUTFLength": [
        "/// `GetStringUTFLength` (slot 168): the modified-UTF-8 byte count,",
        "/// terminator excluded (the JNI spec's own definition) — T10 slice 2.",
        "/// Null jstring → 0 (v1 gap: the spec's NPE shape needs the exception",
        "/// machinery; registered for the T13 harness). A non-String handle",
        "/// panics at resolve — the T09 loud law.",
        'extern "system" fn jni_get_string_utf_length(_env: JNIEnv, string: jobject) -> jsize {',
        "    let tag = crate::jni::bridge::jobject_to_tag(string);",
        "    if tag == 0 {",
        "        return 0;",
        "    }",
        "    crate::jni::strings::slot_get_string_utf_length(tag) as jsize",
        "}",
    ],
    "GetStringUTFChars": [
        "/// `GetStringUTFChars` (slot 169): hands the caller a NUL-terminated",
        "/// modified-UTF-8 copy, registry-pinned until `ReleaseStringUTFChars`",
        "/// (a double release or foreign pointer is a LOUD registry miss —",
        "/// across this boundary, a process abort). T10 slice 2. `isCopy`,",
        "/// when the caller asks, is written JNI_TRUE through the audited",
        "/// `write_u8_out` primitive (M5_JNI_FFI.md §4 entry 7 — this slice's",
        "/// amendment: one byte, never the pointer-word primitive). Null",
        "/// jstring → NULL, isCopy untouched (v1 gap, T13-registered).",
        'extern "system" fn jni_get_string_utf_chars(',
        "    _env: JNIEnv,",
        "    string: jobject,",
        "    is_copy: *mut jboolean,",
        ") -> *const c_char {",
        "    let tag = crate::jni::bridge::jobject_to_tag(string);",
        "    if tag == 0 {",
        "        return std::ptr::null();",
        "    }",
        "    let addr = crate::jni::strings::slot_get_string_utf_chars(tag, None);",
        "    if !is_copy.is_null() {",
        "        j2r_jni_ffi::ffi::write_u8_out(is_copy, 1); // JNI_TRUE — always a copy here",
        "    }",
        "    addr as *const c_char",
        "}",
    ],
    "ReleaseStringUTFChars": [
        "/// `ReleaseStringUTFChars` (slot 170): frees the registry-pinned",
        "/// buffer — T10 slice 2. Null string or null chars pointer → no-op",
        "/// (the slice-1 law). Anything else MUST be a pointer this slot",
        "/// handed out; a miss panics loudly, which across the extern boundary",
        "/// is the process-abort loudness shape (pinned by a child-process",
        "/// test in jni/mod.rs).",
        'extern "system" fn jni_release_string_utf_chars(_env: JNIEnv, string: jobject, chars: *const c_char) {',
        "    if crate::jni::bridge::jobject_to_tag(string) == 0 || chars.is_null() {",
        "        return;",
        "    }",
        "    crate::jni::strings::slot_release_string_utf_chars(chars as usize);",
        "}",
    ],
    "FindClass": [
        "/// `FindClass` (slot 6): a JNI-form name (internal `a/b/C`, array",
        '/// descriptor) -> the interned `java/lang/Class` mirror as a fresh',
        "/// local ref (T10 slice 3, M5_JNI_CLASSES.md section 2). The class",
        "/// must EXIST (compiled via the host dispatch, or the builtin",
        "/// tables) BEFORE interning — no garbage name ever enters the Class",
        "/// table; a miss -> JNI null (the spec's NoClassDefFoundError shape",
        "/// — v1 gap, T13-registered). Bare primitive names are not classes.",
        'extern "system" fn jni_find_class(_env: JNIEnv, name: *const c_char) -> jclass {',
        "    let Some(bytes) = j2r_jni_ffi::ffi::read_caller_str(name) else {",
        "        return std::ptr::null_mut();",
        "    };",
        "    match String::from_utf8(bytes) {",
        "        Ok(s) => match crate::jni::classes::slot_find_class(&s) {",
        "            Some(tag) => crate::jni::bridge::tag_to_jobject(tag),",
        "            None => std::ptr::null_mut(),",
        "        },",
        "        Err(_) => std::ptr::null_mut(),",
        "    }",
        "}",
    ],
    "GetObjectClass": [
        "/// `GetObjectClass` (slot 31): the receiver's runtime class (arrays",
        "/// included — an array's class IS its descriptor) as the interned",
        "/// `java/lang/Class` mirror, fresh local ref (T10 slice 3,",
        "/// M5_JNI_CLASSES.md section 2). Null -> JNI null (v1 gap,",
        "/// T13-registered).",
        'extern "system" fn jni_get_object_class(_env: JNIEnv, obj: jobject) -> jclass {',
        "    match crate::jni::classes::slot_get_object_class(crate::jni::bridge::jobject_to_tag(obj)) {",
        "        Some(tag) => crate::jni::bridge::tag_to_jobject(tag),",
        "        None => std::ptr::null_mut(),",
        "    }",
        "}",
    ],
    "GetFieldID": [
        "/// `GetFieldID` (slot 94): resolve an INSTANCE field by `(name, sig)`",
        "/// on the class — the superclass walk included (JNI searches",
        "/// inherited fields) — deduplicated into a permanent field-ID tag",
        "/// (T10 slice 3, M5_JNI_CLASSES.md section 3). Miss -> NULL (the",
        "/// spec's NoSuchFieldError shape — v1 gap, T13-registered); builtin",
        "/// classes have no field facts in v1 -> NULL (registered gap).",
        'extern "system" fn jni_get_field_id(',
        "    _env: JNIEnv,",
        "    clazz: jclass,",
        "    name: *const c_char,",
        "    sig: *const c_char,",
        ") -> jfieldID {",
        "    let (Some(name_bytes), Some(sig_bytes)) = (",
        "        j2r_jni_ffi::ffi::read_caller_str(name),",
        "        j2r_jni_ffi::ffi::read_caller_str(sig),",
        "    ) else {",
        "        return std::ptr::null_mut();",
        "    };",
        "    let (Ok(name), Ok(sig)) = (String::from_utf8(name_bytes), String::from_utf8(sig_bytes)) else {",
        "        return std::ptr::null_mut();",
        "    };",
        "    match crate::jni::classes::slot_get_field_id(",
        "        crate::jni::bridge::jobject_to_tag(clazz),",
        "        &name,",
        "        &sig,",
        "        false,",
        "    ) {",
        "        Some(id) => crate::jni::classes::field_id_to_ptr(id),",
        "        None => std::ptr::null_mut(),",
        "    }",
        "}",
    ],
    "GetStaticFieldID": [
        "/// `GetStaticFieldID` (slot 144): the STATIC twin of `GetFieldID` —",
        "/// same walk, same permanence and dedup; only fields whose own",
        "/// declaration says `static` match (T10 slice 3, M5_JNI_CLASSES.md",
        "/// section 3). Miss -> NULL (v1 gap, T13-registered).",
        'extern "system" fn jni_get_static_field_id(',
        "    _env: JNIEnv,",
        "    clazz: jclass,",
        "    name: *const c_char,",
        "    sig: *const c_char,",
        ") -> jfieldID {",
        "    let (Some(name_bytes), Some(sig_bytes)) = (",
        "        j2r_jni_ffi::ffi::read_caller_str(name),",
        "        j2r_jni_ffi::ffi::read_caller_str(sig),",
        "    ) else {",
        "        return std::ptr::null_mut();",
        "    };",
        "    let (Ok(name), Ok(sig)) = (String::from_utf8(name_bytes), String::from_utf8(sig_bytes)) else {",
        "        return std::ptr::null_mut();",
        "    };",
        "    match crate::jni::classes::slot_get_field_id(",
        "        crate::jni::bridge::jobject_to_tag(clazz),",
        "        &name,",
        "        &sig,",
        "        true,",
        "    ) {",
        "        Some(id) => crate::jni::classes::field_id_to_ptr(id),",
        "        None => std::ptr::null_mut(),",
        "    }",
        "}",
    ],
    # ---- T10 slice 3b: the object-array + length slots (the per-prim
    # families are loop-generated after this dict).
    "GetArrayLength": [
        "/// `GetArrayLength` (slot 171): the array's length — T10 slice 3b.",
        "/// Null array → 0: the §5-conceded v1 law (the spec's NPE shape",
        "/// needs the exception machinery — T13-registered). A non-array",
        "/// handle panics loud at resolve — the T09 law.",
        'extern "system" fn jni_get_array_length(_env: JNIEnv, array: jobject) -> jsize {',
        "    let tag = crate::jni::bridge::jobject_to_tag(array);",
        "    if tag == 0 {",
        "        return 0;",
        "    }",
        "    crate::jni::arrays::slot_get_array_length(tag)",
        "}",
    ],
    "NewObjectArray": [
        "/// `NewObjectArray` (slot 172): element class FROM the jclass",
        "/// argument (a foreign lib may hand any Class mirror, builtin",
        "/// included); `init`, when non-null, resolves ONCE and every slot",
        "/// stores a copy of that JRef — T10 slice 3b. Null class → JNI",
        "/// null (v1 NPE gap, T13-registered); negative length or an",
        "/// init/class mismatch aborts loudly (the §5 failure law).",
        'extern "system" fn jni_new_object_array(',
        "    _env: JNIEnv,",
        "    len: jsize,",
        "    clazz: jobject,",
        "    init: jobject,",
        ") -> jobject {",
        "    let class_tag = crate::jni::bridge::jobject_to_tag(clazz);",
        "    if class_tag == 0 {",
        "        return std::ptr::null_mut();",
        "    }",
        "    let init_tag = crate::jni::bridge::jobject_to_tag(init);",
        "    let tag = crate::jni::arrays::slot_new_object_array(len, class_tag, init_tag);",
        "    crate::jni::bridge::tag_to_jobject(tag)",
        "}",
    ],
    "GetObjectArrayElement": [
        "/// `GetObjectArrayElement` (slot 173): a rooted local ref for a",
        "/// live element, JNI null for a null element — T10 slice 3b. Null",
        "/// array → JNI null (v1 NPE gap); out-of-bounds aborts loudly",
        "/// (ArrayIndexOutOfBoundsException, the §5 law).",
        'extern "system" fn jni_get_object_array_element(',
        "    _env: JNIEnv,",
        "    array: jobject,",
        "    idx: jsize,",
        ") -> jobject {",
        "    let tag = crate::jni::bridge::jobject_to_tag(array);",
        "    if tag == 0 {",
        "        return std::ptr::null_mut();",
        "    }",
        "    match crate::jni::arrays::slot_get_object_array_element(tag, idx) {",
        "        Some(elem) => crate::jni::bridge::tag_to_jobject(elem),",
        "        None => std::ptr::null_mut(),",
        "    }",
        "}",
    ],
    "SetObjectArrayElement": [
        "/// `SetObjectArrayElement` (slot 174): the store rides array_store,",
        "/// whose ArrayStoreException check runs with the host dispatch —",
        "/// T10 slice 3b. Null array → no-op (v1 NPE gap); a null elem tag",
        "/// stores a null element (legal); out-of-bounds or a store",
        "/// violation aborts loudly (the §5 law).",
        'extern "system" fn jni_set_object_array_element(',
        "    _env: JNIEnv,",
        "    array: jobject,",
        "    idx: jsize,",
        "    value: jobject,",
        ") {",
        "    let tag = crate::jni::bridge::jobject_to_tag(array);",
        "    if tag == 0 {",
        "        return;",
        "    }",
        "    let elem_tag = crate::jni::bridge::jobject_to_tag(value);",
        "    crate::jni::arrays::slot_set_object_array_element(tag, idx, elem_tag);",
        "}",
    ],
    "GetPrimitiveArrayCritical": [
        "/// `GetPrimitiveArrayCritical` (slot 222): hands out the payload's",
        "/// OWN address through the existing critical registry — pinning",
        "/// is free because the arena never moves and payloads never",
        "/// reallocate (the M2 advantage) — T10 slice 3b. isCopy, when",
        "/// asked, is JNI_FALSE (the real storage, the faithful uncopied",
        "/// shape). Null array → NULL; a non-primitive array is loud.",
        'extern "system" fn jni_get_primitive_array_critical(',
        "    _env: JNIEnv,",
        "    array: jobject,",
        "    is_copy: *mut jboolean,",
        ") -> *mut c_void {",
        "    let tag = crate::jni::bridge::jobject_to_tag(array);",
        "    if tag == 0 {",
        "        return std::ptr::null_mut();",
        "    }",
        "    let addr = crate::jni::arrays::slot_get_primitive_array_critical(tag);",
        "    if !is_copy.is_null() {",
        "        j2r_jni_ffi::ffi::write_u8_out(is_copy, 0); // JNI_FALSE — real storage",
        "    }",
        "    addr as *mut c_void",
        "}",
    ],
    "ReleasePrimitiveArrayCritical": [
        "/// `ReleasePrimitiveArrayCritical` (slot 223): the existing",
        "/// critical registry's loud release — T10 slice 3b. The handed",
        "/// address IS the live payload, so the caller's writes are",
        "/// already in the array: there is nothing to copy back and",
        "/// JNI_ABORT cannot retroactively un-write — the mode is accepted",
        "/// and recorded as a no-op (documented v1 deviation,",
        "/// T13-registered). A miss aborts loudly (never silent).",
        'extern "system" fn jni_release_primitive_array_critical(',
        "    _env: JNIEnv,",
        "    array: jobject,",
        "    ptr: *mut c_void,",
        "    mode: jint,",
        ") {",
        "    let tag = crate::jni::bridge::jobject_to_tag(array);",
        "    if tag == 0 {",
        "        return;",
        "    }",
        "    crate::jni::arrays::slot_release_primitive_array_critical(ptr as usize, mode);",
        "}",
    ],
    # ---- T11 slice 11b: the pending-exception trio + the method-ID
    # pair (docs/M5_JNI_CALLS.md §3/§4). The ten Call*MethodA bodies are
    # loop-generated after this dict.
    "ExceptionOccurred": [
        "/// `ExceptionOccurred` (slot 15): the pending throwable as a",
        "/// FRESH local ref, or JNI null — the first faithful exception",
        "/// transport across the boundary (T11 slice 11b, the per-thread",
        "/// cell in the bridge). The cell itself is left untouched.",
        'extern "system" fn jni_exception_occurred(_env: JNIEnv) -> jobject {',
        "    match crate::jni::bridge::pending_exception_local() {",
        "        Some(tag) => crate::jni::bridge::tag_to_jobject(tag),",
        "        None => std::ptr::null_mut(),",
        "    }",
        "}",
    ],
    "ExceptionClear": [
        "/// `ExceptionClear` (slot 17): drops the pending throwable from",
        "/// the current thread's cell (T11 slice 11b).",
        'extern "system" fn jni_exception_clear(_env: JNIEnv) {',
        "    crate::jni::bridge::take_pending_exception();",
        "}",
    ],
    "ExceptionCheck": [
        "/// `ExceptionCheck` (slot 228): JNI_TRUE iff a throwable is",
        "/// pending on the current thread (T11 slice 11b) — the cheap",
        "/// guard that avoids materializing a local ref.",
        'extern "system" fn jni_exception_check(_env: JNIEnv) -> jboolean {',
        "    crate::jni::bridge::has_pending_exception() as jboolean",
        "}",
    ],
    "GetMethodID": [
        "/// `GetMethodID` (slot 33): walk the class and its ancestors for",
        "/// a declared INSTANCE method with the exact name+descriptor —",
        "/// T11 slice 11b. Not found (including builtin classes, which",
        "/// have no compiled method table — the v1 gap T12 owns): JNI",
        "/// null with a PENDING NoSuchMethodError, the oracle's shape.",
        'extern "system" fn jni_get_method_id(',
        "    _env: JNIEnv,",
        "    clazz: jobject,",
        "    name: *const c_char,",
        "    sig: *const c_char,",
        ") -> jmethodID {",
        "    let (Some(name_bytes), Some(sig_bytes)) = (",
        "        j2r_jni_ffi::ffi::read_caller_str(name),",
        "        j2r_jni_ffi::ffi::read_caller_str(sig),",
        "    ) else {",
        "        return std::ptr::null_mut();",
        "    };",
        "    let (Ok(name), Ok(sig)) = (String::from_utf8(name_bytes), String::from_utf8(sig_bytes)) else {",
        "        return std::ptr::null_mut();",
        "    };",
        "    let tag = crate::jni::bridge::jobject_to_tag(clazz);",
        "    if tag == 0 {",
        "        return std::ptr::null_mut();",
        "    }",
        "    match crate::jni::classes::slot_get_method_id(tag, &name, &sig, false) {",
        "        Some(id) => crate::jni::classes::method_id_to_ptr(id),",
        "        None => std::ptr::null_mut(),",
        "    }",
        "}",
    ],
    "GetStaticMethodID": [
        "/// `GetStaticMethodID` (slot 113): the STATIC twin of",
        "/// `GetMethodID` — T11 slice 11b. Same not-found law: JNI null",
        "/// with a pending NoSuchMethodError.",
        'extern "system" fn jni_get_static_method_id(',
        "    _env: JNIEnv,",
        "    clazz: jobject,",
        "    name: *const c_char,",
        "    sig: *const c_char,",
        ") -> jmethodID {",
        "    let (Some(name_bytes), Some(sig_bytes)) = (",
        "        j2r_jni_ffi::ffi::read_caller_str(name),",
        "        j2r_jni_ffi::ffi::read_caller_str(sig),",
        "    ) else {",
        "        return std::ptr::null_mut();",
        "    };",
        "    let (Ok(name), Ok(sig)) = (String::from_utf8(name_bytes), String::from_utf8(sig_bytes)) else {",
        "        return std::ptr::null_mut();",
        "    };",
        "    let tag = crate::jni::bridge::jobject_to_tag(clazz);",
        "    if tag == 0 {",
        "        return std::ptr::null_mut();",
        "    }",
        "    match crate::jni::classes::slot_get_method_id(tag, &name, &sig, true) {",
        "        Some(id) => crate::jni::classes::method_id_to_ptr(id),",
        "        None => std::ptr::null_mut(),",
        "    }",
        "}",
    ],
}

# ---------------------------------------------------------------- T10 slice 3b: the per-primitive array families (programmatic)
# 40 slots (New 175-182, GetElements 183-190, ReleaseElements 191-198,
# GetRegion 199-206, SetRegion 207-214) share ONE shape per family group;
# the per-primitive differences are the C type and the PrimKind constant.
# Semantics live in jni::arrays; these are thin, documented casts, exactly
# like the string family. PrimKind lives in jni::arrays (one variant per
# slot — "each slot IS one primitive", the design §5 law).
_ARRAY_PRIMS = [
    # (jni.h family name, Rust C-arg type, PrimKind variant, is_unsigned_note)
    ("Boolean", "jboolean", "Z", "0/1 — the Int-group law narrows at the payload"),
    ("Byte", "jbyte", "B", "i8 — the Int-group law narrows at the payload"),
    ("Char", "jchar", "C", "u16 — the Int-group law narrows at the payload"),
    ("Short", "jshort", "S", "i16 — the Int-group law narrows at the payload"),
    ("Int", "jint", "I", ""),
    ("Long", "jlong", "J", ""),
    ("Float", "jfloat", "F", ""),
    ("Double", "jdouble", "D", ""),
]

for _name, _cty, _kind, _note in _ARRAY_PRIMS:
    _k = f"crate::jni::arrays::PrimKind::{_kind}"
    _slot_no = 175 + _ARRAY_PRIMS.index((_name, _cty, _kind, _note))
    _new_doc = [
        f"/// `New{_name}Array` (slot {_slot_no}): the slot IS one primitive — no descriptor",
        "/// parsing (design §5) — T10 slice 3b. Negative length aborts",
        "/// loudly (NegativeArraySizeException, the v1 failure law).",
    ]
    if _note:
        _new_doc.insert(1, f"/// Element width: {_note}.")
    WRAPPER_BODIES[f"New{_name}Array"] = _new_doc + [
        f'extern "system" fn jni_new_{_name.lower()}_array(_env: JNIEnv, len: jsize) -> jobject {{',
        f"    let tag = crate::jni::arrays::slot_new_prim_array({_k}, len);",
        "    crate::jni::bridge::tag_to_jobject(tag)",
        "}",
    ]
    WRAPPER_BODIES[f"Get{_name}ArrayElements"] = [
        f"/// `Get{_name}ArrayElements` (slot {183 + _ARRAY_PRIMS.index((_name, _cty, _kind, _note))}):"
        " hands out a caller-owned registry-pinned COPY",
        "/// (design §5) — T10 slice 3b. isCopy, when asked, is JNI_TRUE",
        "/// through the audited write_u8_out primitive. Null array → NULL",
        "/// (isCopy untouched, v1 NPE gap).",
        f'extern "system" fn jni_get_{_name.lower()}_array_elements(',
        "    _env: JNIEnv,",
        "    array: jobject,",
        "    is_copy: *mut jboolean,",
        f") -> *mut {_cty} {{",
        "    let tag = crate::jni::bridge::jobject_to_tag(array);",
        "    if tag == 0 {",
        "        return std::ptr::null_mut();",
        "    }",
        f"    let addr = crate::jni::arrays::slot_get_prim_array_elements({_k}, tag);",
        "    if !is_copy.is_null() {",
        "        j2r_jni_ffi::ffi::write_u8_out(is_copy, 1); // JNI_TRUE — always a copy",
        "    }",
        f"    addr as *mut {_cty}",
        "}",
    ]
    WRAPPER_BODIES[f"Release{_name}ArrayElements"] = [
        f"/// `Release{_name}ArrayElements` (slot {191 + _ARRAY_PRIMS.index((_name, _cty, _kind, _note))}):"
        " the mode law — JNI_COMMIT (1) copies",
        "/// back without freeing; 0 copies back and frees; JNI_ABORT (2)",
        "/// frees without copying back — T10 slice 3b. A foreign pointer,",
        "/// a wrong-family release, or an array mismatch aborts loudly.",
        "/// Null array → no-op (v1 NPE gap).",
        f'extern "system" fn jni_release_{_name.lower()}_array_elements(',
        "    _env: JNIEnv,",
        "    array: jobject,",
        f"    elems: *mut {_cty},",
        "    mode: jint,",
        ") {",
        "    let tag = crate::jni::bridge::jobject_to_tag(array);",
        "    if tag == 0 {",
        "        return;",
        "    }",
        "    crate::jni::arrays::slot_release_prim_array_elements(",
        f"        {_k},",
        "        tag,",
        "        elems as usize,",
        "        mode,",
        "    );",
        "}",
    ]
    WRAPPER_BODIES[f"Get{_name}ArrayRegion"] = [
        f"/// `Get{_name}ArrayRegion` (slot {199 + _ARRAY_PRIMS.index((_name, _cty, _kind, _note))}):"
        " bulk copy into the caller's buffer,",
        "/// bounds checked UP FRONT — a violation aborts loudly with the",
        "/// ArrayIndexOutOfBoundsException name (design §5) — T10 slice",
        "/// 3b. Null array → no-op (v1 NPE gap).",
        f'extern "system" fn jni_get_{_name.lower()}_array_region(',
        "    _env: JNIEnv,",
        "    array: jobject,",
        "    start: jsize,",
        "    len: jsize,",
        f"    buf: *mut {_cty},",
        ") {",
        "    let tag = crate::jni::bridge::jobject_to_tag(array);",
        "    if tag == 0 {",
        "        return;",
        "    }",
        "    crate::jni::arrays::slot_get_prim_array_region(",
        f"        {_k},",
        "        tag,",
        "        start,",
        "        len,",
        "        buf.cast::<u8>(),",
        "    );",
        "}",
    ]
    WRAPPER_BODIES[f"Set{_name}ArrayRegion"] = [
        f"/// `Set{_name}ArrayRegion` (slot {207 + _ARRAY_PRIMS.index((_name, _cty, _kind, _note))}):"
        " bulk copy from the caller's buffer,",
        "/// bounds checked UP FRONT; narrowing follows the ARRAY",
        "/// descriptor (design §5) — T10 slice 3b. Null array → no-op",
        "/// (v1 NPE gap).",
        f'extern "system" fn jni_set_{_name.lower()}_array_region(',
        "    _env: JNIEnv,",
        "    array: jobject,",
        "    start: jsize,",
        "    len: jsize,",
        f"    buf: *const {_cty},",
        ") {",
        "    let tag = crate::jni::bridge::jobject_to_tag(array);",
        "    if tag == 0 {",
        "        return;",
        "    }",
        "    crate::jni::arrays::slot_set_prim_array_region(",
        f"        {_k},",
        "        tag,",
        "        start,",
        "        len,",
        "        buf.cast::<u8>(),",
        "    );",
        "}",
    ]


# ---------------------------------------------------------------- T11 slice 11b: the virtual Call*MethodA family (programmatic)
# Ten slots (36..63, step 3 — verified against the generated offset_of!
# assertion table) share ONE shape; the per-kind differences are the C
# return type and the slot body name. Semantics live in jni::calls;
# these are thin, documented casts.

# A-form slot numbers, verified against the generated offset_of!
# assertion table (Object 36, then step 3 through Void 63).
_A_SLOT_NOS = {
    "Object": 36, "Boolean": 39, "Byte": 42, "Char": 45, "Short": 48,
    "Int": 51, "Long": 54, "Float": 57, "Double": 60, "Void": 63,
}

_CALL_A_KINDS = [
    ("Object", "jobject", "slot_call_object_method_a"),
    ("Boolean", "jboolean", "slot_call_boolean_method_a"),
    ("Byte", "jbyte", "slot_call_byte_method_a"),
    ("Char", "jchar", "slot_call_char_method_a"),
    ("Short", "jshort", "slot_call_short_method_a"),
    ("Int", "jint", "slot_call_int_method_a"),
    ("Long", "jlong", "slot_call_long_method_a"),
    ("Float", "jfloat", "slot_call_float_method_a"),
    ("Double", "jdouble", "slot_call_double_method_a"),
    ("Void", None, "slot_call_void_method_a"),
]

for _name, _ret_cty, _slot_fn in _CALL_A_KINDS:
    _ret_decl = f" -> {_ret_cty}" if _ret_cty else ""
    # The Object kind answers a handle-table TAG: the wrapper dresses it
    # as a jobject on the way out (tag_to_jobject), the C-kind slots
    # return the number directly.
    WRAPPER_BODIES[f"Call{_name}MethodA"] = [
        f"/// `Call{_name}MethodA` (slot {_A_SLOT_NOS[_name]}): the A-form — the v1",
        "/// workhorse (design of record: docs/M5_JNI_CALLS.md §3): the",
        "/// jvalue words are decoded per the method's own descriptor and",
        "/// the call dispatches by the receiver's runtime class. A thrown",
        "/// answer is PENDED (ExceptionOccurred/Check/Clear) and the zero",
        "/// value returned — the JNI-faithful shape. A null mid is a",
        "/// loud foreign-side bug (the T09 registry law).",
        f'extern "system" fn jni_call_{_name.lower()}_method_a(',
        "    _env: JNIEnv,",
        "    obj: jobject,",
        "    mid: jmethodID,",
        "    args: *const c_void,",
        f"){_ret_decl} {{",
        "    let tag = crate::jni::bridge::jobject_to_tag(obj);",
        *(
            [
                f"    crate::jni::calls::{_slot_fn}(",
                "        crate::jni::classes::method_id_from_ptr(mid),",
                "        tag,",
                "        args as usize,",
                "    )",
            ]
            if _ret_cty != "jobject"
            else [
                "    crate::jni::bridge::tag_to_jobject(",
                f"        crate::jni::calls::{_slot_fn}(",
                "            crate::jni::classes::method_id_from_ptr(mid),",
                "            tag,",
                "            args as usize,",
                "        ),",
                "    )",
            ]
        ),
        "}",
    ]


# ---------------------------------------------------------------- T10 slice 3: the field-accessor families (programmatic)
# The 36 accessor slots (instance 95-112, static 145-162) share ONE
# shape; the per-primitive differences are the C type and the Value<->C
# arm. Each wrapper asserts the value shape against ITS OWN slot
# descriptor — a mismatch is a foreign-side bug: loud abort (Temurin:
# UB). Semantics live in jni::classes; these are thin casts, exactly
# like the string family. Built here (AFTER the jni.h parse) so every
# doc comment can carry the true slot number.
_idx_of = {name: i for (i, kind, name, r, a) in slots if kind == "func"}

FIELD_PRIMS = [
    # (Suffix, C type, Value variant, get match arm, set Value ctor)
    ("Object", "jobject", "Ref", None, None),  # special-cased bodies
    ("Boolean", "jboolean", "Int", "(i != 0) as jboolean",
     "crate::Value::Int((value != 0) as i32)"),
    ("Byte", "jbyte", "Int", "i as jbyte", "crate::Value::Int(value as i32)"),
    ("Char", "jchar", "Int", "i as jchar", "crate::Value::Int(value as i32)"),
    ("Short", "jshort", "Int", "i as jshort", "crate::Value::Int(value as i32)"),
    ("Int", "jint", "Int", "i", "crate::Value::Int(value)"),
    ("Long", "jlong", "Long", "l", "crate::Value::Long(value)"),
    ("Float", "jfloat", "Float", "f", "crate::Value::Float(value)"),
    ("Double", "jdouble", "Double", "d", "crate::Value::Double(value)"),
]


_GET_VAR = {"Int": "i", "Long": "l", "Float": "f", "Double": "d"}


def _field_docs(slot_name, slot_no, is_static, suffix):
    kind = "static" if is_static else "instance"
    lines = [
        f"/// `{slot_name}` (slot {slot_no}): {kind} access through the",
        "/// field-ID table (T10 slice 3, M5_JNI_CLASSES.md section 4). The",
        "/// value must carry the slot's own descriptor shape — a mismatch",
        "/// is a foreign-side bug: loud abort (Temurin: UB).",
    ]
    if not is_static and suffix != "Object":
        lines.append("/// Null receiver -> 0/null (v1 gap, T13-registered).")
    if is_static:
        lines.append("/// The ID is the sole authority; the jclass argument is not")
        lines.append("/// re-validated in v1 (registered note, M5_JNI_CLASSES.md §4).")
    if suffix == "Object":
        lines.append("/// Object values come back as fresh local refs; a JNI-null")
        lines.append("/// field reads back as JNI null.")
    return lines


for (_suffix, _cty, _variant, _arm, _ctor) in FIELD_PRIMS:
    for _is_static in (False, True):
        _sname = f"Get{'Static' if _is_static else ''}{_suffix}Field"
        _fname = "jni_" + _sname.lower()
        REAL_SLOTS[_sname] = _fname
        _lines = _field_docs(_sname, _idx_of[_sname], _is_static, _suffix)
        _lines.append(f'extern "system" fn {_fname}(')
        _lines.append("    _env: JNIEnv,")
        _lines.append("    _clazz: jclass," if _is_static else "    obj: jobject,")
        _lines.append("    fid: jfieldID")
        _lines.append(f") -> {_cty} {{")
        if _is_static:
            _lines.append("    match crate::jni::classes::slot_get_static_field(")
            _lines.append("        crate::jni::classes::field_id_from_ptr(fid),")
            _lines.append("    ) {")
        else:
            _lines.append("    match crate::jni::classes::slot_get_instance_field(")
            _lines.append("        crate::jni::bridge::jobject_to_tag(obj),")
            _lines.append("        crate::jni::classes::field_id_from_ptr(fid),")
            _lines.append("    ) {")
        if _suffix == "Object":
            _lines += [
                "        Some(crate::Value::Ref(Some(r))) => {",
                "            crate::jni::tag_to_jobject(crate::jni::new_local(r))",
                "        }",
                "        Some(crate::Value::Ref(None)) => std::ptr::null_mut(),",
                f'        Some(other) => crate::jni::classes::loud_field_mismatch("{_sname}", &other),',
                "        None => std::ptr::null_mut(),",
            ]
        else:
            _v = _GET_VAR[_variant]
            _none = {"jfloat": "0 as jfloat", "jdouble": "0 as jdouble"}.get(_cty, "0")
            _lines += [
                f"        Some(crate::Value::{_variant}({_v})) => {_arm},",
                f'        Some(other) => crate::jni::classes::loud_field_mismatch("{_sname}", &other),',
                f"        None => {_none},",
            ]
        _lines.append("    }")
        _lines.append("}")
        WRAPPER_BODIES[_sname] = _lines

for (_suffix, _cty, _variant, _arm, _ctor) in FIELD_PRIMS:
    for _is_static in (False, True):
        _sname = f"Set{'Static' if _is_static else ''}{_suffix}Field"
        _fname = "jni_" + _sname.lower()
        REAL_SLOTS[_sname] = _fname
        _lines = _field_docs(_sname, _idx_of[_sname], _is_static, _suffix)
        _lines.append(f'extern "system" fn {_fname}(')
        _lines.append("    _env: JNIEnv,")
        _lines.append("    _clazz: jclass," if _is_static else "    obj: jobject,")
        _lines.append("    fid: jfieldID,")
        _lines.append(f"    value: {_cty},")
        _lines.append(") {")
        if _suffix == "Object":
            _lines += [
                "    let tag = crate::jni::bridge::jobject_to_tag(value);",
                "    let v = crate::Value::Ref(if tag == 0 {",
                "        None",
                "    } else {",
                "        Some(crate::jni::resolve(tag))",
                "    });",
            ]
        else:
            _lines.append(f"    let v = {_ctor};")
        if _is_static:
            _lines += [
                "    crate::jni::classes::slot_set_static_field(",
                "        crate::jni::classes::field_id_from_ptr(fid),",
                "        v,",
                "    );",
            ]
        else:
            _lines += [
                "    crate::jni::classes::slot_set_instance_field(",
                "        crate::jni::bridge::jobject_to_tag(obj),",
                "        crate::jni::classes::field_id_from_ptr(fid),",
                "        v,",
                "    );",
            ]
        _lines.append("}")
        WRAPPER_BODIES[_sname] = _lines


# ---------------------------------------------------------------- T11 slice 11b-2: the static/nonvirtual Call*MethodA families (programmatic)
# Twenty slots sharing the virtual family's one shape (docs/
# M5_JNI_CALLS.md §3/§4). The static family's C signature is
# (env, clazz, mid, args) — the RECEIVER position carries the jclass —
# and the nonvirtual family's is (env, obj, clazz, mid, args) — BOTH
# receiver and declared class ride the call. Semantics live in
# jni::calls (one invoke core, three families); these are thin,
# documented casts. Slot numbers verified MECHANICALLY against the
# parsed header below — a renumbered jni.h fails the generator loudly.

_NV_A_SLOT_NOS = {
    "Object": 66, "Boolean": 69, "Byte": 72, "Char": 75, "Short": 78,
    "Int": 81, "Long": 84, "Float": 87, "Double": 90, "Void": 93,
}
_STATIC_A_SLOT_NOS = {
    "Object": 116, "Boolean": 119, "Byte": 122, "Char": 125, "Short": 128,
    "Int": 131, "Long": 134, "Float": 137, "Double": 140, "Void": 143,
}
for _name in _NV_A_SLOT_NOS:
    if _idx_of[f"CallNonvirtual{_name}MethodA"] != _NV_A_SLOT_NOS[_name]:
        sys.exit(
            f"FATAL: CallNonvirtual{_name}MethodA slot drifted: jni.h says "
            f"{_idx_of[f'CallNonvirtual{_name}MethodA']}, template says {_NV_A_SLOT_NOS[_name]}"
        )
    if _idx_of[f"CallStatic{_name}MethodA"] != _STATIC_A_SLOT_NOS[_name]:
        sys.exit(
            f"FATAL: CallStatic{_name}MethodA slot drifted: jni.h says "
            f"{_idx_of[f'CallStatic{_name}MethodA']}, template says {_STATIC_A_SLOT_NOS[_name]}"
        )

# (kind, C return type) — the same ten kinds as the virtual family.
_CALL_A2_KINDS = [
    ("Object", "jobject"),
    ("Boolean", "jboolean"),
    ("Byte", "jbyte"),
    ("Char", "jchar"),
    ("Short", "jshort"),
    ("Int", "jint"),
    ("Long", "jlong"),
    ("Float", "jfloat"),
    ("Double", "jdouble"),
    ("Void", None),
]

for _name, _ret_cty in _CALL_A2_KINDS:
    _ret_decl = f" -> {_ret_cty}" if _ret_cty else ""
    _low = _name.lower()
    # STATIC: the receiver ABI position carries the jclass; no receiver.
    WRAPPER_BODIES[f"CallStatic{_name}MethodA"] = [
        f"/// `CallStatic{_name}MethodA` (slot {_STATIC_A_SLOT_NOS[_name]}): the A-form —",
        "/// the v1 workhorse (design of record: docs/M5_JNI_CALLS.md",
        "/// §3/§4): the jvalue words are decoded per the method's own",
        "/// descriptor and the call resolves on the DECLARED class — no",
        "/// receiver (the C signature carries the jclass in the receiver",
        "/// position). A thrown answer is PENDED",
        "/// (ExceptionOccurred/Check/Clear) and the zero value returned —",
        "/// the JNI-faithful shape. A null mid is a loud foreign-side bug",
        "/// (the T09 registry law).",
        f'extern "system" fn jni_call_static_{_low}_method_a(',
        "    _env: JNIEnv,",
        "    clazz: jclass,",
        "    mid: jmethodID,",
        "    args: *const c_void,",
        f"){_ret_decl} {{",
        "    let tag = crate::jni::bridge::jobject_to_tag(clazz);",
        *(
            [
                f"    crate::jni::calls::slot_call_static_{_low}_method_a(",
                "        crate::jni::classes::method_id_from_ptr(mid),",
                "        tag,",
                "        args as usize,",
                "    )",
            ]
            if _ret_cty != "jobject"
            else [
                "    crate::jni::bridge::tag_to_jobject(",
                f"        crate::jni::calls::slot_call_static_{_low}_method_a(",
                "            crate::jni::classes::method_id_from_ptr(mid),",
                "            tag,",
                "            args as usize,",
                "        ),",
                "    )",
            ]
        ),
        "}",
    ]
    # NONVIRTUAL: BOTH the receiver and the declared class ride the call
    # (the C signature is (env, obj, clazz, mid, args)).
    WRAPPER_BODIES[f"CallNonvirtual{_name}MethodA"] = [
        f"/// `CallNonvirtual{_name}MethodA` (slot {_NV_A_SLOT_NOS[_name]}): the A-form —",
        "/// the v1 workhorse (design of record: docs/M5_JNI_CALLS.md",
        "/// §3/§4): the jvalue words are decoded per the method's own",
        "/// descriptor and the call dispatches by the DECLARED class (the",
        "/// jclass argument) — the `super.m()` shape, never the receiver's",
        "/// runtime class. A thrown answer is PENDED",
        "/// (ExceptionOccurred/Check/Clear) and the zero value returned —",
        "/// the JNI-faithful shape. A null mid is a loud foreign-side bug",
        "/// (the T09 registry law).",
        f'extern "system" fn jni_call_nonvirtual_{_low}_method_a(',
        "    _env: JNIEnv,",
        "    obj: jobject,",
        "    clazz: jclass,",
        "    mid: jmethodID,",
        "    args: *const c_void,",
        f"){_ret_decl} {{",
        "    let obj_tag = crate::jni::bridge::jobject_to_tag(obj);",
        "    let clazz_tag = crate::jni::bridge::jobject_to_tag(clazz);",
        *(
            [
                f"    crate::jni::calls::slot_call_nonvirtual_{_low}_method_a(",
                "        crate::jni::classes::method_id_from_ptr(mid),",
                "        obj_tag,",
                "        clazz_tag,",
                "        args as usize,",
                "    )",
            ]
            if _ret_cty != "jobject"
            else [
                "    crate::jni::bridge::tag_to_jobject(",
                f"        crate::jni::calls::slot_call_nonvirtual_{_low}_method_a(",
                "            crate::jni::classes::method_id_from_ptr(mid),",
                "            obj_tag,",
                "            clazz_tag,",
                "            args as usize,",
                "        ),",
                "    )",
            ]
        ),
        "}",
    ]


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
