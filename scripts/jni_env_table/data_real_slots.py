"""The REAL_SLOTS registry (verbatim from the pre-split generator)."""

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
    # ---- T11 slice 11c: the V-form families (docs/M5_JNI_CALLS.md §3
    # decision 2) — the SysV __va_list_tag save-area walk; the audited
    # FFI surface grew 7 → 10 read primitives in the same commit.
    "CallObjectMethodV": "jni_call_object_method_v",
    "CallBooleanMethodV": "jni_call_boolean_method_v",
    "CallByteMethodV": "jni_call_byte_method_v",
    "CallCharMethodV": "jni_call_char_method_v",
    "CallShortMethodV": "jni_call_short_method_v",
    "CallIntMethodV": "jni_call_int_method_v",
    "CallLongMethodV": "jni_call_long_method_v",
    "CallFloatMethodV": "jni_call_float_method_v",
    "CallDoubleMethodV": "jni_call_double_method_v",
    "CallVoidMethodV": "jni_call_void_method_v",
    "CallStaticObjectMethodV": "jni_call_static_object_method_v",
    "CallStaticBooleanMethodV": "jni_call_static_boolean_method_v",
    "CallStaticByteMethodV": "jni_call_static_byte_method_v",
    "CallStaticCharMethodV": "jni_call_static_char_method_v",
    "CallStaticShortMethodV": "jni_call_static_short_method_v",
    "CallStaticIntMethodV": "jni_call_static_int_method_v",
    "CallStaticLongMethodV": "jni_call_static_long_method_v",
    "CallStaticFloatMethodV": "jni_call_static_float_method_v",
    "CallStaticDoubleMethodV": "jni_call_static_double_method_v",
    "CallStaticVoidMethodV": "jni_call_static_void_method_v",
    "CallNonvirtualObjectMethodV": "jni_call_nonvirtual_object_method_v",
    "CallNonvirtualBooleanMethodV": "jni_call_nonvirtual_boolean_method_v",
    "CallNonvirtualByteMethodV": "jni_call_nonvirtual_byte_method_v",
    "CallNonvirtualCharMethodV": "jni_call_nonvirtual_char_method_v",
    "CallNonvirtualShortMethodV": "jni_call_nonvirtual_short_method_v",
    "CallNonvirtualIntMethodV": "jni_call_nonvirtual_int_method_v",
    "CallNonvirtualLongMethodV": "jni_call_nonvirtual_long_method_v",
    "CallNonvirtualFloatMethodV": "jni_call_nonvirtual_float_method_v",
    "CallNonvirtualDoubleMethodV": "jni_call_nonvirtual_double_method_v",
    "CallNonvirtualVoidMethodV": "jni_call_nonvirtual_void_method_v",
    # ---- T12 slice 12b: the native-binding pair (docs/M5_JNI_NATIVES.md
    # §3) — RegisterNatives validates per the declared-native law and
    # stores foreign fnPtrs; Rust-shape bindings ride the Rust API.
    "RegisterNatives": "jni_register_natives",
    "UnregisterNatives": "jni_unregister_natives",
    # ---- T13 slice 13c: the VM-out slot (docs/M5_JNI_HARNESS.md §7 —
    # the loading story: a foreign library caches the VM here, then
    # GetEnv's the env table and binds its natives). The out-param write
    # rides audited primitive 3 (write_ptr_out — the M5_JNI_FFI.md §6
    # inventory listed this slot from day one).
    "GetJavaVM": "jni_get_java_vm",
}

# Wrapper bodies, emitted verbatim (doc comment + fn) after the stubs.
# Indentation: top level, 4-space body indent.
