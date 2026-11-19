"""The static/nonvirtual Call*MethodA + the V-form families + the
T12 native-binding pair + the T13 GetJavaVM slot (verbatim from the
pre-split generator; extends WRAPPER_BODIES)."""
from .data_wrapper_bodies import WRAPPER_BODIES


def build(_idx_of):
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


    # ---------------------------------------------------------------- T11 slice 11c: the V-form families (programmatic)
    # Thirty slots: the three families with the jvalue* replaced by a
    # `va_list` — on x86-64 SysV a POINTER to the caller's 24-byte
    # __va_list_tag record (ABI §3.5.6). The save-area walk lives in
    # jni::calls (safe code over the audited read primitives 8-10); these
    # are thin, documented casts. Slot numbers verified MECHANICALLY
    # against the parsed header — a renumbered jni.h fails the generator
    # loudly.

    _V_SLOT_NOS = {
        "Object": 35, "Boolean": 38, "Byte": 41, "Char": 44, "Short": 47,
        "Int": 50, "Long": 53, "Float": 56, "Double": 59, "Void": 62,
    }
    _STATIC_V_SLOT_NOS = {
        "Object": 115, "Boolean": 118, "Byte": 121, "Char": 124, "Short": 127,
        "Int": 130, "Long": 133, "Float": 136, "Double": 139, "Void": 142,
    }
    _NV_V_SLOT_NOS = {
        "Object": 65, "Boolean": 68, "Byte": 71, "Char": 74, "Short": 77,
        "Int": 80, "Long": 83, "Float": 86, "Double": 89, "Void": 92,
    }
    for _name in _V_SLOT_NOS:
        for _prefix, _nos in (("", _V_SLOT_NOS), ("Static", _STATIC_V_SLOT_NOS), ("Nonvirtual", _NV_V_SLOT_NOS)):
            _sname = f"Call{_prefix}{_name}MethodV"
            if _idx_of[_sname] != _nos[_name]:
                sys.exit(
                    f"FATAL: {_sname} slot drifted: jni.h says {_idx_of[_sname]}, "
                    f"template says {_nos[_name]}"
                )

    _VFORM_DOC = (
        "/// — the V-form: the caller's saved argument list (design of",
        "/// record: docs/M5_JNI_CALLS.md §3 decision 2): the SysV",
        "/// __va_list_tag record is consumed through the audited read",
        "/// primitives into a LOCAL cursor (the caller's record is left",
        "/// untouched — the documented v1 law), the words are marshalled per",
        "/// the method's own descriptor and the call dispatches per the",
        "/// family law. A thrown answer is PENDED",
        "/// (ExceptionOccurred/Check/Clear) and the zero value returned —",
        "/// the JNI-faithful shape. A null mid is a loud foreign-side bug",
        "/// (the T09 registry law).",
    )

    for _name, _ret_cty in _CALL_A2_KINDS:
        _ret_decl = f" -> {_ret_cty}" if _ret_cty else ""
        _low = _name.lower()
        # VIRTUAL V: (env, obj, mid, ap)
        WRAPPER_BODIES[f"Call{_name}MethodV"] = [
            f"/// `Call{_name}MethodV` (slot {_V_SLOT_NOS[_name]})",
            *_VFORM_DOC,
            f'extern "system" fn jni_call_{_low}_method_v(',
            "    _env: JNIEnv,",
            "    obj: jobject,",
            "    mid: jmethodID,",
            "    ap: VaList,",
            f"){_ret_decl} {{",
            "    let tag = crate::jni::bridge::jobject_to_tag(obj);",
            *(
                [
                    f"    crate::jni::calls::slot_call_{_low}_method_v(",
                    "        crate::jni::classes::method_id_from_ptr(mid),",
                    "        tag,",
                    "        ap as usize,",
                    "    )",
                ]
                if _ret_cty != "jobject"
                else [
                    "    crate::jni::bridge::tag_to_jobject(",
                    f"        crate::jni::calls::slot_call_{_low}_method_v(",
                    "            crate::jni::classes::method_id_from_ptr(mid),",
                    "            tag,",
                    "            ap as usize,",
                    "        ),",
                    "    )",
                ]
            ),
            "}",
        ]
        # STATIC V: (env, clazz, mid, ap)
        WRAPPER_BODIES[f"CallStatic{_name}MethodV"] = [
            f"/// `CallStatic{_name}MethodV` (slot {_STATIC_V_SLOT_NOS[_name]})",
            *_VFORM_DOC,
            f'extern "system" fn jni_call_static_{_low}_method_v(',
            "    _env: JNIEnv,",
            "    clazz: jclass,",
            "    mid: jmethodID,",
            "    ap: VaList,",
            f"){_ret_decl} {{",
            "    let tag = crate::jni::bridge::jobject_to_tag(clazz);",
            *(
                [
                    f"    crate::jni::calls::slot_call_static_{_low}_method_v(",
                    "        crate::jni::classes::method_id_from_ptr(mid),",
                    "        tag,",
                    "        ap as usize,",
                    "    )",
                ]
                if _ret_cty != "jobject"
                else [
                    "    crate::jni::bridge::tag_to_jobject(",
                    f"        crate::jni::calls::slot_call_static_{_low}_method_v(",
                    "            crate::jni::classes::method_id_from_ptr(mid),",
                    "            tag,",
                    "            ap as usize,",
                    "        ),",
                    "    )",
                ]
            ),
            "}",
        ]
        # NONVIRTUAL V: (env, obj, clazz, mid, ap)
        WRAPPER_BODIES[f"CallNonvirtual{_name}MethodV"] = [
            f"/// `CallNonvirtual{_name}MethodV` (slot {_NV_V_SLOT_NOS[_name]})",
            *_VFORM_DOC,
            f'extern "system" fn jni_call_nonvirtual_{_low}_method_v(',
            "    _env: JNIEnv,",
            "    obj: jobject,",
            "    clazz: jclass,",
            "    mid: jmethodID,",
            "    ap: VaList,",
            f"){_ret_decl} {{",
            "    let obj_tag = crate::jni::bridge::jobject_to_tag(obj);",
            "    let clazz_tag = crate::jni::bridge::jobject_to_tag(clazz);",
            *(
                [
                    f"    crate::jni::calls::slot_call_nonvirtual_{_low}_method_v(",
                    "        crate::jni::classes::method_id_from_ptr(mid),",
                    "        obj_tag,",
                    "        clazz_tag,",
                    "        ap as usize,",
                    "    )",
                ]
                if _ret_cty != "jobject"
                else [
                    "    crate::jni::bridge::tag_to_jobject(",
                    f"        crate::jni::calls::slot_call_nonvirtual_{_low}_method_v(",
                    "            crate::jni::classes::method_id_from_ptr(mid),",
                    "            obj_tag,",
                    "            clazz_tag,",
                    "            ap as usize,",
                    "        ),",
                    "    )",
                ]
            ),
            "}",
        ]


    # ---------------------------------------------------------------- T12 slice 12b: the native-binding pair (programmatic)
    # Two slots (docs/M5_JNI_NATIVES.md §3). The fn pointers RegisterNatives
    # receives are stored as FOREIGN (the C-array path can only yield C
    # fn pointers; Rust-shape bindings ride the Rust API — the 12b shape
    # law). Slot numbers verified MECHANICALLY against the parsed header.

    _NATIVES_SLOT_NOS = {"RegisterNatives": 215, "UnregisterNatives": 216}
    for _sname, _no in _NATIVES_SLOT_NOS.items():
        if _idx_of[_sname] != _no:
            sys.exit(
                f"FATAL: {_sname} slot drifted: jni.h says {_idx_of[_sname]}, template says {_no}"
            )

    WRAPPER_BODIES["RegisterNatives"] = [
        "/// `RegisterNatives` (slot 215): binds implementations to the",
        "/// class's declared `native` methods (design of record:",
        "/// docs/M5_JNI_NATIVES.md §3). Each `{name, sig, fnPtr}` triple is",
        "/// validated against the class's own declared rows (class-SCOPED,",
        "/// no ancestor walk); a mismatch is JNI_ERR + a PENDING",
        "/// NoSuchMethodError naming the entry; the fn pointers are stored",
        "/// as FOREIGN (calling one needs the registered T13 primitive 11 —",
        "/// a call LOUDLY aborts, never a silent wrong answer).",
        'extern "system" fn jni_register_natives(',
        "    _env: JNIEnv,",
        "    clazz: jclass,",
        "    methods: *const JNINativeMethod,",
        "    n: jint,",
        ") -> jint {",
        "    let tag = crate::jni::bridge::jobject_to_tag(clazz);",
        "    crate::jni::natives::slot_register_natives(tag, methods as usize, n)",
        "}",
    ]
    WRAPPER_BODIES["UnregisterNatives"] = [
        "/// `UnregisterNatives` (slot 216): clears ALL bindings of the class",
        "/// (design of record: docs/M5_JNI_NATIVES.md §3.4); unregistering a",
        "/// class with no bindings is still JNI_OK.",
        'extern "system" fn jni_unregister_natives(',
        "    _env: JNIEnv,",
        "    clazz: jclass,",
        ") -> jint {",
        "    let tag = crate::jni::bridge::jobject_to_tag(clazz);",
        "    crate::jni::natives::slot_unregister_natives(tag)",
        "}",
    ]

    # ---- T13 slice 13c: GetJavaVM (the loading story's VM-out slot).
    # The §6 inventory (M5_JNI_FFI.md) listed it among the pointer-out slots
    # from day one; the write rides audited primitive 3. Slot number verified
    # mechanically against the parsed header below (the _idx_of check).
    if _idx_of["GetJavaVM"] != 219:
        sys.exit(
            f"FATAL: GetJavaVM slot drifted: jni.h says {_idx_of['GetJavaVM']}, template says 219"
        )
    WRAPPER_BODIES["GetJavaVM"] = [
        "/// `GetJavaVM` (slot 219, T13 13c): hands the caller the VM table",
        "/// pointer (docs/M5_JNI_HARNESS.md §7 — the loading story: a foreign",
        "/// library caches the VM here, GetEnvs the env table, binds its",
        "/// natives). The write goes through audited primitive 3; a null",
        "/// out-slot is JNI_ERR (the caller-bug law, same as GetEnv).",
        'extern "system" fn jni_get_java_vm(_env: JNIEnv, vm: *mut *mut JavaVM_) -> jint {',
        "    if vm.is_null() {",
        "        return crate::jni::JNI_ERR;",
        "    }",
        "    j2r_jni_ffi::ffi::write_ptr_out(vm.cast::<*mut c_void>(), java_vm() as usize);",
        "    crate::jni::JNI_OK",
        "}",
    ]


