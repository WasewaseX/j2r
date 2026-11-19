"""The 36 field-accessor families (verbatim from the pre-split
generator, wrapped in build(slots) - its only cross-section
dependency is the parsed slot index).

The r9 host-scan decomposition split the pre-split 129-line build()
into the FIELD_PRIMS/_GET_VAR constants, the _field_docs helper and
one emitter per direction (Get/Set, each under the 80-line house
limit). Every emitted string is byte-identical to the pre-split
body; the generated table proves it by byte-parity."""
from .data_real_slots import REAL_SLOTS  # noqa: F401  (the fields
# section registers its own REAL_SLOTS entries)
from .data_wrapper_bodies import WRAPPER_BODIES


# ---------------------------------------------------------------- T10 slice 3: the field-accessor families (programmatic)
# The 36 accessor slots (instance 95-112, static 145-162) share ONE
# shape; the per-primitive differences are the C type and the Value<->C
# arm. Each wrapper asserts the value shape against ITS OWN slot
# descriptor — a mismatch is a foreign-side bug: loud abort (Temurin:
# UB). Semantics live in jni::classes; these are thin casts, exactly
# like the string family. Built here (AFTER the jni.h parse) so every
# doc comment can carry the true slot number.
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


def _get_families(idx_of):
    for (_suffix, _cty, _variant, _arm, _ctor) in FIELD_PRIMS:
        for _is_static in (False, True):
            _sname = f"Get{'Static' if _is_static else ''}{_suffix}Field"
            _fname = "jni_" + _sname.lower()
            REAL_SLOTS[_sname] = _fname
            _lines = _field_docs(_sname, idx_of[_sname], _is_static, _suffix)
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


def _set_families(idx_of):
    for (_suffix, _cty, _variant, _arm, _ctor) in FIELD_PRIMS:
        for _is_static in (False, True):
            _sname = f"Set{'Static' if _is_static else ''}{_suffix}Field"
            _fname = "jni_" + _sname.lower()
            REAL_SLOTS[_sname] = _fname
            _lines = _field_docs(_sname, idx_of[_sname], _is_static, _suffix)
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


def build(slots):
    """The pre-split entry point: the parsed slot index feeds both
    directions, in the same Get-then-Set order as the monolith."""
    idx_of = {name: i for (i, kind, name, r, a) in slots if kind == "func"}
    _get_families(idx_of)
    _set_families(idx_of)
