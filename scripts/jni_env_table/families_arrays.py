"""The per-primitive array families + the virtual Call*MethodA family
(verbatim from the pre-split generator; extends WRAPPER_BODIES)."""
from .data_wrapper_bodies import WRAPPER_BODIES


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


