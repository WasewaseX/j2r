//! GENERATED FILE — do not edit by hand. Regenerate with:
//!   python3 scripts/gen_jni_env_table.py $JAVA_HOME/include/jni.h
//!
//! T08 (M5 wave 2, M5_PLAN §3.1): the JNIEnv/JNIInvokeInterface static
//! tables, index-for-index identical to the Temurin 21 jni.h of record.
//! Every non-reserved slot holds a real function or a loud stub — never
//! a null slot a foreign library could segfault on. Layout is pinned by
//! the const assertion table at the bottom (offset_of! per slot).
//!
//! T10 slice 2: the five string slots (NewString, NewStringUTF,
//! GetStringUTFLength, GetStringUTFChars, ReleaseStringUTFChars) are
//! wired to real wrappers through the generator's REAL_SLOTS registry;
//! their bodies live in j2r-runtime::jni::strings (slice 1) and the
//! raw-pointer moments go through the audited j2r-jni-ffi primitives.
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]

use std::os::raw::{c_char, c_void};

#[repr(C)]
pub struct JObjectOpaque {
    _private: [u8; 0],
}
#[repr(C)]
pub struct VaListOpaque {
    _private: [u8; 0],
}

pub type jobject = *mut JObjectOpaque;
pub type jint = i32;
pub type jlong = i64;
pub type jbyte = i8;
pub type jboolean = u8;
pub type jchar = u16;
pub type jshort = i16;
pub type jfloat = f32;
pub type jdouble = f64;
pub type jsize = jint;
pub type jobjectRefType = i32;
pub type VaList = *mut VaListOpaque;
#[repr(C)]
pub struct JNINativeMethod {
    pub name: *mut c_char,
    pub signature: *mut c_char,
    pub fnPtr: *mut c_void,
}

// jni.h type aliases, verified against the header by the generator:
pub type jclass = jobject;
pub type jstring = jobject;
pub type jthrowable = jobject;
pub type jweak = jobject;
pub type jobjectArray = jobject;
pub type jintArray = jobject;
pub type jbooleanArray = jobject;
pub type jbyteArray = jobject;
pub type jcharArray = jobject;
pub type jshortArray = jobject;
pub type jlongArray = jobject;
pub type jfloatArray = jobject;
pub type jdoubleArray = jobject;
pub type jmethodID = *mut c_void;
pub type jfieldID = *mut c_void;

fn unimplemented_slot(index: usize, name: &'static str) -> ! {
    panic!("j2r-jni: table slot {index} ({name}) is not implemented yet; it is registered for a later wave-2 task")
}

extern "system" fn jni_get_version(_env: JNIEnv) -> jint {
    crate::jni::JNI_VERSION_1_8
}

#[repr(C)]
pub struct JNINativeInterface_ {
    pub reserved0: Option<extern "system" fn()>, // slot 0 — reserved (None == NULL, per jni.h)
    pub reserved1: Option<extern "system" fn()>, // slot 1 — reserved (None == NULL, per jni.h)
    pub reserved2: Option<extern "system" fn()>, // slot 2 — reserved (None == NULL, per jni.h)
    pub reserved3: Option<extern "system" fn()>, // slot 3 — reserved (None == NULL, per jni.h)
    pub GetVersion: extern "system" fn(*mut JNIEnv_) -> jint, // slot 4
    pub DefineClass:
        extern "system" fn(*mut JNIEnv_, *const c_char, jobject, *const jbyte, jsize) -> jobject, // slot 5
    pub FindClass: extern "system" fn(*mut JNIEnv_, *const c_char) -> jobject, // slot 6
    pub FromReflectedMethod: extern "system" fn(*mut JNIEnv_, jobject) -> c_void, // slot 7
    pub FromReflectedField: extern "system" fn(*mut JNIEnv_, jobject) -> c_void, // slot 8
    pub ToReflectedMethod: extern "system" fn(*mut JNIEnv_, jobject, c_void, jboolean) -> jobject, // slot 9
    pub GetSuperclass: extern "system" fn(*mut JNIEnv_, jobject) -> jobject, // slot 10
    pub IsAssignableFrom: extern "system" fn(*mut JNIEnv_, jobject, jobject) -> jboolean, // slot 11
    pub ToReflectedField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jboolean) -> jobject, // slot 12
    pub Throw: extern "system" fn(*mut JNIEnv_, jobject) -> jint, // slot 13
    pub ThrowNew: extern "system" fn(*mut JNIEnv_, jobject, *const c_char) -> jint, // slot 14
    pub ExceptionOccurred: extern "system" fn(*mut JNIEnv_) -> jobject, // slot 15
    pub ExceptionDescribe: extern "system" fn(*mut JNIEnv_) -> (), // slot 16
    pub ExceptionClear: extern "system" fn(*mut JNIEnv_) -> (),   // slot 17
    pub FatalError: extern "system" fn(*mut JNIEnv_, *const c_char) -> (), // slot 18
    pub PushLocalFrame: extern "system" fn(*mut JNIEnv_, jint) -> jint, // slot 19
    pub PopLocalFrame: extern "system" fn(*mut JNIEnv_, jobject) -> jobject, // slot 20
    pub NewGlobalRef: extern "system" fn(*mut JNIEnv_, jobject) -> jobject, // slot 21
    pub DeleteGlobalRef: extern "system" fn(*mut JNIEnv_, jobject) -> (), // slot 22
    pub DeleteLocalRef: extern "system" fn(*mut JNIEnv_, jobject) -> (), // slot 23
    pub IsSameObject: extern "system" fn(*mut JNIEnv_, jobject, jobject) -> jboolean, // slot 24
    pub NewLocalRef: extern "system" fn(*mut JNIEnv_, jobject) -> jobject, // slot 25
    pub EnsureLocalCapacity: extern "system" fn(*mut JNIEnv_, jint) -> jint, // slot 26
    pub AllocObject: extern "system" fn(*mut JNIEnv_, jobject) -> jobject, // slot 27
    pub NewObject:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jobject, // slot 28
    pub NewObjectV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jobject, // slot 29
    pub NewObjectA: extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jobject, // slot 30
    pub GetObjectClass: extern "system" fn(*mut JNIEnv_, jobject) -> jobject, // slot 31
    pub IsInstanceOf: extern "system" fn(*mut JNIEnv_, jobject, jobject) -> jboolean, // slot 32
    pub GetMethodID:
        extern "system" fn(*mut JNIEnv_, jobject, *const c_char, *const c_char) -> c_void, // slot 33
    pub CallObjectMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jobject, // slot 34
    pub CallObjectMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jobject, // slot 35
    pub CallObjectMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jobject, // slot 36
    pub CallBooleanMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jboolean, // slot 37
    pub CallBooleanMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jboolean, // slot 38
    pub CallBooleanMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jboolean, // slot 39
    pub CallByteMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jbyte, // slot 40
    pub CallByteMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jbyte, // slot 41
    pub CallByteMethodA: extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jbyte, // slot 42
    pub CallCharMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jchar, // slot 43
    pub CallCharMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jchar, // slot 44
    pub CallCharMethodA: extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jchar, // slot 45
    pub CallShortMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jshort, // slot 46
    pub CallShortMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jshort, // slot 47
    pub CallShortMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jshort, // slot 48
    pub CallIntMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jint, // slot 49
    pub CallIntMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jint, // slot 50
    pub CallIntMethodA: extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jint, // slot 51
    pub CallLongMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jlong, // slot 52
    pub CallLongMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jlong, // slot 53
    pub CallLongMethodA: extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jlong, // slot 54
    pub CallFloatMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jfloat, // slot 55
    pub CallFloatMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jfloat, // slot 56
    pub CallFloatMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jfloat, // slot 57
    pub CallDoubleMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jdouble, // slot 58
    pub CallDoubleMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jdouble, // slot 59
    pub CallDoubleMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jdouble, // slot 60
    pub CallVoidMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> (), // slot 61
    pub CallVoidMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> (), // slot 62
    pub CallVoidMethodA: extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> (), // slot 63
    pub CallNonvirtualObjectMethod:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, _args: *const c_void) -> jobject, // slot 64
    pub CallNonvirtualObjectMethodV:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, VaList) -> jobject, // slot 65
    pub CallNonvirtualObjectMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, *const c_void) -> jobject, // slot 66
    pub CallNonvirtualBooleanMethod: extern "system" fn(
        *mut JNIEnv_,
        jobject,
        jobject,
        c_void,
        _args: *const c_void,
    ) -> jboolean, // slot 67
    pub CallNonvirtualBooleanMethodV:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, VaList) -> jboolean, // slot 68
    pub CallNonvirtualBooleanMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, *const c_void) -> jboolean, // slot 69
    pub CallNonvirtualByteMethod:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, _args: *const c_void) -> jbyte, // slot 70
    pub CallNonvirtualByteMethodV:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, VaList) -> jbyte, // slot 71
    pub CallNonvirtualByteMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, *const c_void) -> jbyte, // slot 72
    pub CallNonvirtualCharMethod:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, _args: *const c_void) -> jchar, // slot 73
    pub CallNonvirtualCharMethodV:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, VaList) -> jchar, // slot 74
    pub CallNonvirtualCharMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, *const c_void) -> jchar, // slot 75
    pub CallNonvirtualShortMethod:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, _args: *const c_void) -> jshort, // slot 76
    pub CallNonvirtualShortMethodV:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, VaList) -> jshort, // slot 77
    pub CallNonvirtualShortMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, *const c_void) -> jshort, // slot 78
    pub CallNonvirtualIntMethod:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, _args: *const c_void) -> jint, // slot 79
    pub CallNonvirtualIntMethodV:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, VaList) -> jint, // slot 80
    pub CallNonvirtualIntMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, *const c_void) -> jint, // slot 81
    pub CallNonvirtualLongMethod:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, _args: *const c_void) -> jlong, // slot 82
    pub CallNonvirtualLongMethodV:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, VaList) -> jlong, // slot 83
    pub CallNonvirtualLongMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, *const c_void) -> jlong, // slot 84
    pub CallNonvirtualFloatMethod:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, _args: *const c_void) -> jfloat, // slot 85
    pub CallNonvirtualFloatMethodV:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, VaList) -> jfloat, // slot 86
    pub CallNonvirtualFloatMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, *const c_void) -> jfloat, // slot 87
    pub CallNonvirtualDoubleMethod:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, _args: *const c_void) -> jdouble, // slot 88
    pub CallNonvirtualDoubleMethodV:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, VaList) -> jdouble, // slot 89
    pub CallNonvirtualDoubleMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, *const c_void) -> jdouble, // slot 90
    pub CallNonvirtualVoidMethod:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, _args: *const c_void) -> (), // slot 91
    pub CallNonvirtualVoidMethodV:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, VaList) -> (), // slot 92
    pub CallNonvirtualVoidMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, jobject, c_void, *const c_void) -> (), // slot 93
    pub GetFieldID:
        extern "system" fn(*mut JNIEnv_, jobject, *const c_char, *const c_char) -> c_void, // slot 94
    pub GetObjectField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jobject, // slot 95
    pub GetBooleanField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jboolean, // slot 96
    pub GetByteField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jbyte,     // slot 97
    pub GetCharField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jchar,     // slot 98
    pub GetShortField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jshort,   // slot 99
    pub GetIntField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jint,       // slot 100
    pub GetLongField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jlong,     // slot 101
    pub GetFloatField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jfloat,   // slot 102
    pub GetDoubleField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jdouble, // slot 103
    pub SetObjectField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jobject) -> (), // slot 104
    pub SetBooleanField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jboolean) -> (), // slot 105
    pub SetByteField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jbyte) -> (), // slot 106
    pub SetCharField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jchar) -> (), // slot 107
    pub SetShortField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jshort) -> (), // slot 108
    pub SetIntField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jint) -> (),   // slot 109
    pub SetLongField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jlong) -> (), // slot 110
    pub SetFloatField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jfloat) -> (), // slot 111
    pub SetDoubleField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jdouble) -> (), // slot 112
    pub GetStaticMethodID:
        extern "system" fn(*mut JNIEnv_, jobject, *const c_char, *const c_char) -> c_void, // slot 113
    pub CallStaticObjectMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jobject, // slot 114
    pub CallStaticObjectMethodV:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jobject, // slot 115
    pub CallStaticObjectMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jobject, // slot 116
    pub CallStaticBooleanMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jboolean, // slot 117
    pub CallStaticBooleanMethodV:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jboolean, // slot 118
    pub CallStaticBooleanMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jboolean, // slot 119
    pub CallStaticByteMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jbyte, // slot 120
    pub CallStaticByteMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jbyte, // slot 121
    pub CallStaticByteMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jbyte, // slot 122
    pub CallStaticCharMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jchar, // slot 123
    pub CallStaticCharMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jchar, // slot 124
    pub CallStaticCharMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jchar, // slot 125
    pub CallStaticShortMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jshort, // slot 126
    pub CallStaticShortMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jshort, // slot 127
    pub CallStaticShortMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jshort, // slot 128
    pub CallStaticIntMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jint, // slot 129
    pub CallStaticIntMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jint, // slot 130
    pub CallStaticIntMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jint, // slot 131
    pub CallStaticLongMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jlong, // slot 132
    pub CallStaticLongMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jlong, // slot 133
    pub CallStaticLongMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jlong, // slot 134
    pub CallStaticFloatMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jfloat, // slot 135
    pub CallStaticFloatMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jfloat, // slot 136
    pub CallStaticFloatMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jfloat, // slot 137
    pub CallStaticDoubleMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> jdouble, // slot 138
    pub CallStaticDoubleMethodV:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> jdouble, // slot 139
    pub CallStaticDoubleMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> jdouble, // slot 140
    pub CallStaticVoidMethod:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, _args: *const c_void) -> (), // slot 141
    pub CallStaticVoidMethodV: extern "system" fn(*mut JNIEnv_, jobject, c_void, VaList) -> (), // slot 142
    pub CallStaticVoidMethodA:
        extern "system" fn(*mut JNIEnv_, jobject, c_void, *const c_void) -> (), // slot 143
    pub GetStaticFieldID:
        extern "system" fn(*mut JNIEnv_, jobject, *const c_char, *const c_char) -> c_void, // slot 144
    pub GetStaticObjectField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jobject, // slot 145
    pub GetStaticBooleanField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jboolean, // slot 146
    pub GetStaticByteField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jbyte, // slot 147
    pub GetStaticCharField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jchar, // slot 148
    pub GetStaticShortField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jshort, // slot 149
    pub GetStaticIntField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jint, // slot 150
    pub GetStaticLongField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jlong, // slot 151
    pub GetStaticFloatField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jfloat, // slot 152
    pub GetStaticDoubleField: extern "system" fn(*mut JNIEnv_, jobject, c_void) -> jdouble, // slot 153
    pub SetStaticObjectField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jobject) -> (), // slot 154
    pub SetStaticBooleanField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jboolean) -> (), // slot 155
    pub SetStaticByteField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jbyte) -> (), // slot 156
    pub SetStaticCharField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jchar) -> (), // slot 157
    pub SetStaticShortField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jshort) -> (), // slot 158
    pub SetStaticIntField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jint) -> (), // slot 159
    pub SetStaticLongField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jlong) -> (), // slot 160
    pub SetStaticFloatField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jfloat) -> (), // slot 161
    pub SetStaticDoubleField: extern "system" fn(*mut JNIEnv_, jobject, c_void, jdouble) -> (), // slot 162
    pub NewString: extern "system" fn(*mut JNIEnv_, *const jchar, jsize) -> jobject, // slot 163
    pub GetStringLength: extern "system" fn(*mut JNIEnv_, jobject) -> jsize,         // slot 164
    pub GetStringChars: extern "system" fn(*mut JNIEnv_, jobject, *mut jboolean) -> *const jchar, // slot 165
    pub ReleaseStringChars: extern "system" fn(*mut JNIEnv_, jobject, *const jchar) -> (), // slot 166
    pub NewStringUTF: extern "system" fn(*mut JNIEnv_, *const c_char) -> jobject, // slot 167
    pub GetStringUTFLength: extern "system" fn(*mut JNIEnv_, jobject) -> jsize,   // slot 168
    pub GetStringUTFChars:
        extern "system" fn(*mut JNIEnv_, jobject, *mut jboolean) -> *const c_char, // slot 169
    pub ReleaseStringUTFChars: extern "system" fn(*mut JNIEnv_, jobject, *const c_char) -> (), // slot 170
    pub GetArrayLength: extern "system" fn(*mut JNIEnv_, jobject) -> jsize, // slot 171
    pub NewObjectArray: extern "system" fn(*mut JNIEnv_, jsize, jobject, jobject) -> jobject, // slot 172
    pub GetObjectArrayElement: extern "system" fn(*mut JNIEnv_, jobject, jsize) -> jobject, // slot 173
    pub SetObjectArrayElement: extern "system" fn(*mut JNIEnv_, jobject, jsize, jobject) -> (), // slot 174
    pub NewBooleanArray: extern "system" fn(*mut JNIEnv_, jsize) -> jobject, // slot 175
    pub NewByteArray: extern "system" fn(*mut JNIEnv_, jsize) -> jobject,    // slot 176
    pub NewCharArray: extern "system" fn(*mut JNIEnv_, jsize) -> jobject,    // slot 177
    pub NewShortArray: extern "system" fn(*mut JNIEnv_, jsize) -> jobject,   // slot 178
    pub NewIntArray: extern "system" fn(*mut JNIEnv_, jsize) -> jobject,     // slot 179
    pub NewLongArray: extern "system" fn(*mut JNIEnv_, jsize) -> jobject,    // slot 180
    pub NewFloatArray: extern "system" fn(*mut JNIEnv_, jsize) -> jobject,   // slot 181
    pub NewDoubleArray: extern "system" fn(*mut JNIEnv_, jsize) -> jobject,  // slot 182
    pub GetBooleanArrayElements:
        extern "system" fn(*mut JNIEnv_, jobject, *mut jboolean) -> *mut jboolean, // slot 183
    pub GetByteArrayElements:
        extern "system" fn(*mut JNIEnv_, jobject, *mut jboolean) -> *mut jbyte, // slot 184
    pub GetCharArrayElements:
        extern "system" fn(*mut JNIEnv_, jobject, *mut jboolean) -> *mut jchar, // slot 185
    pub GetShortArrayElements:
        extern "system" fn(*mut JNIEnv_, jobject, *mut jboolean) -> *mut jshort, // slot 186
    pub GetIntArrayElements: extern "system" fn(*mut JNIEnv_, jobject, *mut jboolean) -> *mut jint, // slot 187
    pub GetLongArrayElements:
        extern "system" fn(*mut JNIEnv_, jobject, *mut jboolean) -> *mut jlong, // slot 188
    pub GetFloatArrayElements:
        extern "system" fn(*mut JNIEnv_, jobject, *mut jboolean) -> *mut jfloat, // slot 189
    pub GetDoubleArrayElements:
        extern "system" fn(*mut JNIEnv_, jobject, *mut jboolean) -> *mut jdouble, // slot 190
    pub ReleaseBooleanArrayElements:
        extern "system" fn(*mut JNIEnv_, jobject, *mut jboolean, jint) -> (), // slot 191
    pub ReleaseByteArrayElements: extern "system" fn(*mut JNIEnv_, jobject, *mut jbyte, jint) -> (), // slot 192
    pub ReleaseCharArrayElements: extern "system" fn(*mut JNIEnv_, jobject, *mut jchar, jint) -> (), // slot 193
    pub ReleaseShortArrayElements:
        extern "system" fn(*mut JNIEnv_, jobject, *mut jshort, jint) -> (), // slot 194
    pub ReleaseIntArrayElements: extern "system" fn(*mut JNIEnv_, jobject, *mut jint, jint) -> (), // slot 195
    pub ReleaseLongArrayElements: extern "system" fn(*mut JNIEnv_, jobject, *mut jlong, jint) -> (), // slot 196
    pub ReleaseFloatArrayElements:
        extern "system" fn(*mut JNIEnv_, jobject, *mut jfloat, jint) -> (), // slot 197
    pub ReleaseDoubleArrayElements:
        extern "system" fn(*mut JNIEnv_, jobject, *mut jdouble, jint) -> (), // slot 198
    pub GetBooleanArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *mut jboolean) -> (), // slot 199
    pub GetByteArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *mut jbyte) -> (), // slot 200
    pub GetCharArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *mut jchar) -> (), // slot 201
    pub GetShortArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *mut jshort) -> (), // slot 202
    pub GetIntArrayRegion: extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *mut jint) -> (), // slot 203
    pub GetLongArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *mut jlong) -> (), // slot 204
    pub GetFloatArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *mut jfloat) -> (), // slot 205
    pub GetDoubleArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *mut jdouble) -> (), // slot 206
    pub SetBooleanArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *const jboolean) -> (), // slot 207
    pub SetByteArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *const jbyte) -> (), // slot 208
    pub SetCharArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *const jchar) -> (), // slot 209
    pub SetShortArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *const jshort) -> (), // slot 210
    pub SetIntArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *const jint) -> (), // slot 211
    pub SetLongArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *const jlong) -> (), // slot 212
    pub SetFloatArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *const jfloat) -> (), // slot 213
    pub SetDoubleArrayRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *const jdouble) -> (), // slot 214
    pub RegisterNatives:
        extern "system" fn(*mut JNIEnv_, jobject, *const JNINativeMethod, jint) -> jint, // slot 215
    pub UnregisterNatives: extern "system" fn(*mut JNIEnv_, jobject) -> jint, // slot 216
    pub MonitorEnter: extern "system" fn(*mut JNIEnv_, jobject) -> jint,      // slot 217
    pub MonitorExit: extern "system" fn(*mut JNIEnv_, jobject) -> jint,       // slot 218
    pub GetJavaVM: extern "system" fn(*mut JNIEnv_, *mut *mut JavaVM_) -> jint, // slot 219
    pub GetStringRegion: extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *mut jchar) -> (), // slot 220
    pub GetStringUTFRegion:
        extern "system" fn(*mut JNIEnv_, jobject, jsize, jsize, *mut c_char) -> (), // slot 221
    pub GetPrimitiveArrayCritical:
        extern "system" fn(*mut JNIEnv_, jobject, *mut jboolean) -> *mut c_void, // slot 222
    pub ReleasePrimitiveArrayCritical:
        extern "system" fn(*mut JNIEnv_, jobject, *mut c_void, jint) -> (), // slot 223
    pub GetStringCritical: extern "system" fn(*mut JNIEnv_, jobject, *mut jboolean) -> *const jchar, // slot 224
    pub ReleaseStringCritical: extern "system" fn(*mut JNIEnv_, jobject, *const jchar) -> (), // slot 225
    pub NewWeakGlobalRef: extern "system" fn(*mut JNIEnv_, jobject) -> jobject, // slot 226
    pub DeleteWeakGlobalRef: extern "system" fn(*mut JNIEnv_, jobject) -> (),   // slot 227
    pub ExceptionCheck: extern "system" fn(*mut JNIEnv_) -> jboolean,           // slot 228
    pub NewDirectByteBuffer: extern "system" fn(*mut JNIEnv_, *mut c_void, jlong) -> jobject, // slot 229
    pub GetDirectBufferAddress: extern "system" fn(*mut JNIEnv_, jobject) -> *mut c_void, // slot 230
    pub GetDirectBufferCapacity: extern "system" fn(*mut JNIEnv_, jobject) -> jlong, // slot 231
    pub GetObjectRefType: extern "system" fn(*mut JNIEnv_, jobject) -> jobjectRefType, // slot 232
    pub GetModule: extern "system" fn(*mut JNIEnv_, jobject) -> jobject,             // slot 233
    pub IsVirtualThread: extern "system" fn(*mut JNIEnv_, jobject) -> jboolean,      // slot 234
}

#[repr(C)]
pub struct JNIEnv_ {
    pub functions: &'static JNINativeInterface_,
}
#[repr(C)]
pub struct JavaVM_ {
    pub functions: &'static JNIInvokeInterface_,
}
pub type JNIEnv = *mut JNIEnv_;
pub type JavaVM = *mut JavaVM_;

pub static ENV_TABLE: JNINativeInterface_ = JNINativeInterface_ {
    reserved0: None,
    reserved1: None,
    reserved2: None,
    reserved3: None,
    GetVersion: jni_get_version,
    DefineClass: stub_DefineClass,
    FindClass: stub_FindClass,
    FromReflectedMethod: stub_FromReflectedMethod,
    FromReflectedField: stub_FromReflectedField,
    ToReflectedMethod: stub_ToReflectedMethod,
    GetSuperclass: stub_GetSuperclass,
    IsAssignableFrom: stub_IsAssignableFrom,
    ToReflectedField: stub_ToReflectedField,
    Throw: stub_Throw,
    ThrowNew: stub_ThrowNew,
    ExceptionOccurred: stub_ExceptionOccurred,
    ExceptionDescribe: stub_ExceptionDescribe,
    ExceptionClear: stub_ExceptionClear,
    FatalError: stub_FatalError,
    PushLocalFrame: stub_PushLocalFrame,
    PopLocalFrame: stub_PopLocalFrame,
    NewGlobalRef: stub_NewGlobalRef,
    DeleteGlobalRef: stub_DeleteGlobalRef,
    DeleteLocalRef: stub_DeleteLocalRef,
    IsSameObject: stub_IsSameObject,
    NewLocalRef: stub_NewLocalRef,
    EnsureLocalCapacity: stub_EnsureLocalCapacity,
    AllocObject: stub_AllocObject,
    NewObject: stub_NewObject,
    NewObjectV: stub_NewObjectV,
    NewObjectA: stub_NewObjectA,
    GetObjectClass: stub_GetObjectClass,
    IsInstanceOf: stub_IsInstanceOf,
    GetMethodID: stub_GetMethodID,
    CallObjectMethod: stub_CallObjectMethod,
    CallObjectMethodV: stub_CallObjectMethodV,
    CallObjectMethodA: stub_CallObjectMethodA,
    CallBooleanMethod: stub_CallBooleanMethod,
    CallBooleanMethodV: stub_CallBooleanMethodV,
    CallBooleanMethodA: stub_CallBooleanMethodA,
    CallByteMethod: stub_CallByteMethod,
    CallByteMethodV: stub_CallByteMethodV,
    CallByteMethodA: stub_CallByteMethodA,
    CallCharMethod: stub_CallCharMethod,
    CallCharMethodV: stub_CallCharMethodV,
    CallCharMethodA: stub_CallCharMethodA,
    CallShortMethod: stub_CallShortMethod,
    CallShortMethodV: stub_CallShortMethodV,
    CallShortMethodA: stub_CallShortMethodA,
    CallIntMethod: stub_CallIntMethod,
    CallIntMethodV: stub_CallIntMethodV,
    CallIntMethodA: stub_CallIntMethodA,
    CallLongMethod: stub_CallLongMethod,
    CallLongMethodV: stub_CallLongMethodV,
    CallLongMethodA: stub_CallLongMethodA,
    CallFloatMethod: stub_CallFloatMethod,
    CallFloatMethodV: stub_CallFloatMethodV,
    CallFloatMethodA: stub_CallFloatMethodA,
    CallDoubleMethod: stub_CallDoubleMethod,
    CallDoubleMethodV: stub_CallDoubleMethodV,
    CallDoubleMethodA: stub_CallDoubleMethodA,
    CallVoidMethod: stub_CallVoidMethod,
    CallVoidMethodV: stub_CallVoidMethodV,
    CallVoidMethodA: stub_CallVoidMethodA,
    CallNonvirtualObjectMethod: stub_CallNonvirtualObjectMethod,
    CallNonvirtualObjectMethodV: stub_CallNonvirtualObjectMethodV,
    CallNonvirtualObjectMethodA: stub_CallNonvirtualObjectMethodA,
    CallNonvirtualBooleanMethod: stub_CallNonvirtualBooleanMethod,
    CallNonvirtualBooleanMethodV: stub_CallNonvirtualBooleanMethodV,
    CallNonvirtualBooleanMethodA: stub_CallNonvirtualBooleanMethodA,
    CallNonvirtualByteMethod: stub_CallNonvirtualByteMethod,
    CallNonvirtualByteMethodV: stub_CallNonvirtualByteMethodV,
    CallNonvirtualByteMethodA: stub_CallNonvirtualByteMethodA,
    CallNonvirtualCharMethod: stub_CallNonvirtualCharMethod,
    CallNonvirtualCharMethodV: stub_CallNonvirtualCharMethodV,
    CallNonvirtualCharMethodA: stub_CallNonvirtualCharMethodA,
    CallNonvirtualShortMethod: stub_CallNonvirtualShortMethod,
    CallNonvirtualShortMethodV: stub_CallNonvirtualShortMethodV,
    CallNonvirtualShortMethodA: stub_CallNonvirtualShortMethodA,
    CallNonvirtualIntMethod: stub_CallNonvirtualIntMethod,
    CallNonvirtualIntMethodV: stub_CallNonvirtualIntMethodV,
    CallNonvirtualIntMethodA: stub_CallNonvirtualIntMethodA,
    CallNonvirtualLongMethod: stub_CallNonvirtualLongMethod,
    CallNonvirtualLongMethodV: stub_CallNonvirtualLongMethodV,
    CallNonvirtualLongMethodA: stub_CallNonvirtualLongMethodA,
    CallNonvirtualFloatMethod: stub_CallNonvirtualFloatMethod,
    CallNonvirtualFloatMethodV: stub_CallNonvirtualFloatMethodV,
    CallNonvirtualFloatMethodA: stub_CallNonvirtualFloatMethodA,
    CallNonvirtualDoubleMethod: stub_CallNonvirtualDoubleMethod,
    CallNonvirtualDoubleMethodV: stub_CallNonvirtualDoubleMethodV,
    CallNonvirtualDoubleMethodA: stub_CallNonvirtualDoubleMethodA,
    CallNonvirtualVoidMethod: stub_CallNonvirtualVoidMethod,
    CallNonvirtualVoidMethodV: stub_CallNonvirtualVoidMethodV,
    CallNonvirtualVoidMethodA: stub_CallNonvirtualVoidMethodA,
    GetFieldID: stub_GetFieldID,
    GetObjectField: stub_GetObjectField,
    GetBooleanField: stub_GetBooleanField,
    GetByteField: stub_GetByteField,
    GetCharField: stub_GetCharField,
    GetShortField: stub_GetShortField,
    GetIntField: stub_GetIntField,
    GetLongField: stub_GetLongField,
    GetFloatField: stub_GetFloatField,
    GetDoubleField: stub_GetDoubleField,
    SetObjectField: stub_SetObjectField,
    SetBooleanField: stub_SetBooleanField,
    SetByteField: stub_SetByteField,
    SetCharField: stub_SetCharField,
    SetShortField: stub_SetShortField,
    SetIntField: stub_SetIntField,
    SetLongField: stub_SetLongField,
    SetFloatField: stub_SetFloatField,
    SetDoubleField: stub_SetDoubleField,
    GetStaticMethodID: stub_GetStaticMethodID,
    CallStaticObjectMethod: stub_CallStaticObjectMethod,
    CallStaticObjectMethodV: stub_CallStaticObjectMethodV,
    CallStaticObjectMethodA: stub_CallStaticObjectMethodA,
    CallStaticBooleanMethod: stub_CallStaticBooleanMethod,
    CallStaticBooleanMethodV: stub_CallStaticBooleanMethodV,
    CallStaticBooleanMethodA: stub_CallStaticBooleanMethodA,
    CallStaticByteMethod: stub_CallStaticByteMethod,
    CallStaticByteMethodV: stub_CallStaticByteMethodV,
    CallStaticByteMethodA: stub_CallStaticByteMethodA,
    CallStaticCharMethod: stub_CallStaticCharMethod,
    CallStaticCharMethodV: stub_CallStaticCharMethodV,
    CallStaticCharMethodA: stub_CallStaticCharMethodA,
    CallStaticShortMethod: stub_CallStaticShortMethod,
    CallStaticShortMethodV: stub_CallStaticShortMethodV,
    CallStaticShortMethodA: stub_CallStaticShortMethodA,
    CallStaticIntMethod: stub_CallStaticIntMethod,
    CallStaticIntMethodV: stub_CallStaticIntMethodV,
    CallStaticIntMethodA: stub_CallStaticIntMethodA,
    CallStaticLongMethod: stub_CallStaticLongMethod,
    CallStaticLongMethodV: stub_CallStaticLongMethodV,
    CallStaticLongMethodA: stub_CallStaticLongMethodA,
    CallStaticFloatMethod: stub_CallStaticFloatMethod,
    CallStaticFloatMethodV: stub_CallStaticFloatMethodV,
    CallStaticFloatMethodA: stub_CallStaticFloatMethodA,
    CallStaticDoubleMethod: stub_CallStaticDoubleMethod,
    CallStaticDoubleMethodV: stub_CallStaticDoubleMethodV,
    CallStaticDoubleMethodA: stub_CallStaticDoubleMethodA,
    CallStaticVoidMethod: stub_CallStaticVoidMethod,
    CallStaticVoidMethodV: stub_CallStaticVoidMethodV,
    CallStaticVoidMethodA: stub_CallStaticVoidMethodA,
    GetStaticFieldID: stub_GetStaticFieldID,
    GetStaticObjectField: stub_GetStaticObjectField,
    GetStaticBooleanField: stub_GetStaticBooleanField,
    GetStaticByteField: stub_GetStaticByteField,
    GetStaticCharField: stub_GetStaticCharField,
    GetStaticShortField: stub_GetStaticShortField,
    GetStaticIntField: stub_GetStaticIntField,
    GetStaticLongField: stub_GetStaticLongField,
    GetStaticFloatField: stub_GetStaticFloatField,
    GetStaticDoubleField: stub_GetStaticDoubleField,
    SetStaticObjectField: stub_SetStaticObjectField,
    SetStaticBooleanField: stub_SetStaticBooleanField,
    SetStaticByteField: stub_SetStaticByteField,
    SetStaticCharField: stub_SetStaticCharField,
    SetStaticShortField: stub_SetStaticShortField,
    SetStaticIntField: stub_SetStaticIntField,
    SetStaticLongField: stub_SetStaticLongField,
    SetStaticFloatField: stub_SetStaticFloatField,
    SetStaticDoubleField: stub_SetStaticDoubleField,
    NewString: jni_new_string,
    GetStringLength: stub_GetStringLength,
    GetStringChars: stub_GetStringChars,
    ReleaseStringChars: stub_ReleaseStringChars,
    NewStringUTF: jni_new_string_utf,
    GetStringUTFLength: jni_get_string_utf_length,
    GetStringUTFChars: jni_get_string_utf_chars,
    ReleaseStringUTFChars: jni_release_string_utf_chars,
    GetArrayLength: stub_GetArrayLength,
    NewObjectArray: stub_NewObjectArray,
    GetObjectArrayElement: stub_GetObjectArrayElement,
    SetObjectArrayElement: stub_SetObjectArrayElement,
    NewBooleanArray: stub_NewBooleanArray,
    NewByteArray: stub_NewByteArray,
    NewCharArray: stub_NewCharArray,
    NewShortArray: stub_NewShortArray,
    NewIntArray: stub_NewIntArray,
    NewLongArray: stub_NewLongArray,
    NewFloatArray: stub_NewFloatArray,
    NewDoubleArray: stub_NewDoubleArray,
    GetBooleanArrayElements: stub_GetBooleanArrayElements,
    GetByteArrayElements: stub_GetByteArrayElements,
    GetCharArrayElements: stub_GetCharArrayElements,
    GetShortArrayElements: stub_GetShortArrayElements,
    GetIntArrayElements: stub_GetIntArrayElements,
    GetLongArrayElements: stub_GetLongArrayElements,
    GetFloatArrayElements: stub_GetFloatArrayElements,
    GetDoubleArrayElements: stub_GetDoubleArrayElements,
    ReleaseBooleanArrayElements: stub_ReleaseBooleanArrayElements,
    ReleaseByteArrayElements: stub_ReleaseByteArrayElements,
    ReleaseCharArrayElements: stub_ReleaseCharArrayElements,
    ReleaseShortArrayElements: stub_ReleaseShortArrayElements,
    ReleaseIntArrayElements: stub_ReleaseIntArrayElements,
    ReleaseLongArrayElements: stub_ReleaseLongArrayElements,
    ReleaseFloatArrayElements: stub_ReleaseFloatArrayElements,
    ReleaseDoubleArrayElements: stub_ReleaseDoubleArrayElements,
    GetBooleanArrayRegion: stub_GetBooleanArrayRegion,
    GetByteArrayRegion: stub_GetByteArrayRegion,
    GetCharArrayRegion: stub_GetCharArrayRegion,
    GetShortArrayRegion: stub_GetShortArrayRegion,
    GetIntArrayRegion: stub_GetIntArrayRegion,
    GetLongArrayRegion: stub_GetLongArrayRegion,
    GetFloatArrayRegion: stub_GetFloatArrayRegion,
    GetDoubleArrayRegion: stub_GetDoubleArrayRegion,
    SetBooleanArrayRegion: stub_SetBooleanArrayRegion,
    SetByteArrayRegion: stub_SetByteArrayRegion,
    SetCharArrayRegion: stub_SetCharArrayRegion,
    SetShortArrayRegion: stub_SetShortArrayRegion,
    SetIntArrayRegion: stub_SetIntArrayRegion,
    SetLongArrayRegion: stub_SetLongArrayRegion,
    SetFloatArrayRegion: stub_SetFloatArrayRegion,
    SetDoubleArrayRegion: stub_SetDoubleArrayRegion,
    RegisterNatives: stub_RegisterNatives,
    UnregisterNatives: stub_UnregisterNatives,
    MonitorEnter: stub_MonitorEnter,
    MonitorExit: stub_MonitorExit,
    GetJavaVM: stub_GetJavaVM,
    GetStringRegion: stub_GetStringRegion,
    GetStringUTFRegion: stub_GetStringUTFRegion,
    GetPrimitiveArrayCritical: stub_GetPrimitiveArrayCritical,
    ReleasePrimitiveArrayCritical: stub_ReleasePrimitiveArrayCritical,
    GetStringCritical: stub_GetStringCritical,
    ReleaseStringCritical: stub_ReleaseStringCritical,
    NewWeakGlobalRef: stub_NewWeakGlobalRef,
    DeleteWeakGlobalRef: stub_DeleteWeakGlobalRef,
    ExceptionCheck: stub_ExceptionCheck,
    NewDirectByteBuffer: stub_NewDirectByteBuffer,
    GetDirectBufferAddress: stub_GetDirectBufferAddress,
    GetDirectBufferCapacity: stub_GetDirectBufferCapacity,
    GetObjectRefType: stub_GetObjectRefType,
    GetModule: stub_GetModule,
    IsVirtualThread: stub_IsVirtualThread,
};

#[repr(C)]
pub struct JNIInvokeInterface_ {
    pub reserved0: Option<extern "system" fn()>,
    pub reserved1: Option<extern "system" fn()>,
    pub reserved2: Option<extern "system" fn()>,
    pub DestroyJavaVM: extern "system" fn(JavaVM) -> jint, // slot 3
    pub AttachCurrentThread: extern "system" fn(JavaVM, *mut *mut c_void, *mut c_void) -> jint, // slot 4
    pub DetachCurrentThread: extern "system" fn(JavaVM) -> jint, // slot 5
    pub GetEnv: extern "system" fn(JavaVM, *mut *mut c_void, jint) -> jint, // slot 6
    pub AttachCurrentThreadAsDaemon:
        extern "system" fn(JavaVM, *mut *mut c_void, *mut c_void) -> jint, // slot 7
}

extern "system" fn stub_destroy_java_vm(_vm: JavaVM) -> jint {
    unimplemented_slot(3, "DestroyJavaVM")
}
extern "system" fn stub_attach_current_thread(
    _vm: JavaVM,
    _penv: *mut *mut c_void,
    _args: *mut c_void,
) -> jint {
    unimplemented_slot(4, "AttachCurrentThread")
}
extern "system" fn stub_detach_current_thread(_vm: JavaVM) -> jint {
    unimplemented_slot(5, "DetachCurrentThread")
}
extern "system" fn stub_attach_as_daemon(
    _vm: JavaVM,
    _penv: *mut *mut c_void,
    _args: *mut c_void,
) -> jint {
    unimplemented_slot(7, "AttachCurrentThreadAsDaemon")
}
/// Partially real: the version check needs no pointer write. A supported
/// version would have to write through the caller's `penv` — impossible
/// under forbid(unsafe_code); that resolution is the registered wave-2
/// finding (see jni/mod.rs).
extern "system" fn jni_get_env(_vm: JavaVM, penv: *mut *mut c_void, version: jint) -> jint {
    if penv.is_null() {
        return crate::jni::JNI_ERR;
    }
    if version > crate::jni::JNI_VERSION_1_8 {
        return crate::jni::JNI_EVERSION;
    }
    unimplemented_slot(6, "GetEnv(supported version)")
}

pub static VM_TABLE: JNIInvokeInterface_ = JNIInvokeInterface_ {
    reserved0: None,
    reserved1: None,
    reserved2: None,
    DestroyJavaVM: stub_destroy_java_vm,
    AttachCurrentThread: stub_attach_current_thread,
    DetachCurrentThread: stub_detach_current_thread,
    GetEnv: jni_get_env,
    AttachCurrentThreadAsDaemon: stub_attach_as_daemon,
};

pub static JNI_ENV: JNIEnv_ = JNIEnv_ {
    functions: &ENV_TABLE,
};
pub static JAVA_VM: JavaVM_ = JavaVM_ {
    functions: &VM_TABLE,
};
/// The pointer a native library receives. Creating a raw pointer without
/// dereferencing it is safe; the foreign side's FFI is theirs (M5_PLAN §3.1).
pub fn jni_env() -> JNIEnv {
    &JNI_ENV as *const JNIEnv_ as JNIEnv
}
pub fn java_vm() -> JavaVM {
    &JAVA_VM as *const JavaVM_ as JavaVM
}

const _: () = {
    use std::mem::{offset_of, size_of};
    const W: usize = size_of::<*const ()>();
    assert!(offset_of!(JNINativeInterface_, reserved0) == 0); // slot 0
    assert!(offset_of!(JNINativeInterface_, reserved1) == W); // slot 1
    assert!(offset_of!(JNINativeInterface_, reserved2) == 2 * W); // slot 2
    assert!(offset_of!(JNINativeInterface_, reserved3) == 3 * W); // slot 3
    assert!(offset_of!(JNINativeInterface_, GetVersion) == 4 * W); // slot 4
    assert!(offset_of!(JNINativeInterface_, DefineClass) == 5 * W); // slot 5
    assert!(offset_of!(JNINativeInterface_, FindClass) == 6 * W); // slot 6
    assert!(offset_of!(JNINativeInterface_, FromReflectedMethod) == 7 * W); // slot 7
    assert!(offset_of!(JNINativeInterface_, FromReflectedField) == 8 * W); // slot 8
    assert!(offset_of!(JNINativeInterface_, ToReflectedMethod) == 9 * W); // slot 9
    assert!(offset_of!(JNINativeInterface_, GetSuperclass) == 10 * W); // slot 10
    assert!(offset_of!(JNINativeInterface_, IsAssignableFrom) == 11 * W); // slot 11
    assert!(offset_of!(JNINativeInterface_, ToReflectedField) == 12 * W); // slot 12
    assert!(offset_of!(JNINativeInterface_, Throw) == 13 * W); // slot 13
    assert!(offset_of!(JNINativeInterface_, ThrowNew) == 14 * W); // slot 14
    assert!(offset_of!(JNINativeInterface_, ExceptionOccurred) == 15 * W); // slot 15
    assert!(offset_of!(JNINativeInterface_, ExceptionDescribe) == 16 * W); // slot 16
    assert!(offset_of!(JNINativeInterface_, ExceptionClear) == 17 * W); // slot 17
    assert!(offset_of!(JNINativeInterface_, FatalError) == 18 * W); // slot 18
    assert!(offset_of!(JNINativeInterface_, PushLocalFrame) == 19 * W); // slot 19
    assert!(offset_of!(JNINativeInterface_, PopLocalFrame) == 20 * W); // slot 20
    assert!(offset_of!(JNINativeInterface_, NewGlobalRef) == 21 * W); // slot 21
    assert!(offset_of!(JNINativeInterface_, DeleteGlobalRef) == 22 * W); // slot 22
    assert!(offset_of!(JNINativeInterface_, DeleteLocalRef) == 23 * W); // slot 23
    assert!(offset_of!(JNINativeInterface_, IsSameObject) == 24 * W); // slot 24
    assert!(offset_of!(JNINativeInterface_, NewLocalRef) == 25 * W); // slot 25
    assert!(offset_of!(JNINativeInterface_, EnsureLocalCapacity) == 26 * W); // slot 26
    assert!(offset_of!(JNINativeInterface_, AllocObject) == 27 * W); // slot 27
    assert!(offset_of!(JNINativeInterface_, NewObject) == 28 * W); // slot 28
    assert!(offset_of!(JNINativeInterface_, NewObjectV) == 29 * W); // slot 29
    assert!(offset_of!(JNINativeInterface_, NewObjectA) == 30 * W); // slot 30
    assert!(offset_of!(JNINativeInterface_, GetObjectClass) == 31 * W); // slot 31
    assert!(offset_of!(JNINativeInterface_, IsInstanceOf) == 32 * W); // slot 32
    assert!(offset_of!(JNINativeInterface_, GetMethodID) == 33 * W); // slot 33
    assert!(offset_of!(JNINativeInterface_, CallObjectMethod) == 34 * W); // slot 34
    assert!(offset_of!(JNINativeInterface_, CallObjectMethodV) == 35 * W); // slot 35
    assert!(offset_of!(JNINativeInterface_, CallObjectMethodA) == 36 * W); // slot 36
    assert!(offset_of!(JNINativeInterface_, CallBooleanMethod) == 37 * W); // slot 37
    assert!(offset_of!(JNINativeInterface_, CallBooleanMethodV) == 38 * W); // slot 38
    assert!(offset_of!(JNINativeInterface_, CallBooleanMethodA) == 39 * W); // slot 39
    assert!(offset_of!(JNINativeInterface_, CallByteMethod) == 40 * W); // slot 40
    assert!(offset_of!(JNINativeInterface_, CallByteMethodV) == 41 * W); // slot 41
    assert!(offset_of!(JNINativeInterface_, CallByteMethodA) == 42 * W); // slot 42
    assert!(offset_of!(JNINativeInterface_, CallCharMethod) == 43 * W); // slot 43
    assert!(offset_of!(JNINativeInterface_, CallCharMethodV) == 44 * W); // slot 44
    assert!(offset_of!(JNINativeInterface_, CallCharMethodA) == 45 * W); // slot 45
    assert!(offset_of!(JNINativeInterface_, CallShortMethod) == 46 * W); // slot 46
    assert!(offset_of!(JNINativeInterface_, CallShortMethodV) == 47 * W); // slot 47
    assert!(offset_of!(JNINativeInterface_, CallShortMethodA) == 48 * W); // slot 48
    assert!(offset_of!(JNINativeInterface_, CallIntMethod) == 49 * W); // slot 49
    assert!(offset_of!(JNINativeInterface_, CallIntMethodV) == 50 * W); // slot 50
    assert!(offset_of!(JNINativeInterface_, CallIntMethodA) == 51 * W); // slot 51
    assert!(offset_of!(JNINativeInterface_, CallLongMethod) == 52 * W); // slot 52
    assert!(offset_of!(JNINativeInterface_, CallLongMethodV) == 53 * W); // slot 53
    assert!(offset_of!(JNINativeInterface_, CallLongMethodA) == 54 * W); // slot 54
    assert!(offset_of!(JNINativeInterface_, CallFloatMethod) == 55 * W); // slot 55
    assert!(offset_of!(JNINativeInterface_, CallFloatMethodV) == 56 * W); // slot 56
    assert!(offset_of!(JNINativeInterface_, CallFloatMethodA) == 57 * W); // slot 57
    assert!(offset_of!(JNINativeInterface_, CallDoubleMethod) == 58 * W); // slot 58
    assert!(offset_of!(JNINativeInterface_, CallDoubleMethodV) == 59 * W); // slot 59
    assert!(offset_of!(JNINativeInterface_, CallDoubleMethodA) == 60 * W); // slot 60
    assert!(offset_of!(JNINativeInterface_, CallVoidMethod) == 61 * W); // slot 61
    assert!(offset_of!(JNINativeInterface_, CallVoidMethodV) == 62 * W); // slot 62
    assert!(offset_of!(JNINativeInterface_, CallVoidMethodA) == 63 * W); // slot 63
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualObjectMethod) == 64 * W); // slot 64
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualObjectMethodV) == 65 * W); // slot 65
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualObjectMethodA) == 66 * W); // slot 66
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualBooleanMethod) == 67 * W); // slot 67
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualBooleanMethodV) == 68 * W); // slot 68
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualBooleanMethodA) == 69 * W); // slot 69
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualByteMethod) == 70 * W); // slot 70
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualByteMethodV) == 71 * W); // slot 71
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualByteMethodA) == 72 * W); // slot 72
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualCharMethod) == 73 * W); // slot 73
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualCharMethodV) == 74 * W); // slot 74
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualCharMethodA) == 75 * W); // slot 75
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualShortMethod) == 76 * W); // slot 76
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualShortMethodV) == 77 * W); // slot 77
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualShortMethodA) == 78 * W); // slot 78
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualIntMethod) == 79 * W); // slot 79
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualIntMethodV) == 80 * W); // slot 80
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualIntMethodA) == 81 * W); // slot 81
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualLongMethod) == 82 * W); // slot 82
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualLongMethodV) == 83 * W); // slot 83
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualLongMethodA) == 84 * W); // slot 84
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualFloatMethod) == 85 * W); // slot 85
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualFloatMethodV) == 86 * W); // slot 86
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualFloatMethodA) == 87 * W); // slot 87
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualDoubleMethod) == 88 * W); // slot 88
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualDoubleMethodV) == 89 * W); // slot 89
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualDoubleMethodA) == 90 * W); // slot 90
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualVoidMethod) == 91 * W); // slot 91
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualVoidMethodV) == 92 * W); // slot 92
    assert!(offset_of!(JNINativeInterface_, CallNonvirtualVoidMethodA) == 93 * W); // slot 93
    assert!(offset_of!(JNINativeInterface_, GetFieldID) == 94 * W); // slot 94
    assert!(offset_of!(JNINativeInterface_, GetObjectField) == 95 * W); // slot 95
    assert!(offset_of!(JNINativeInterface_, GetBooleanField) == 96 * W); // slot 96
    assert!(offset_of!(JNINativeInterface_, GetByteField) == 97 * W); // slot 97
    assert!(offset_of!(JNINativeInterface_, GetCharField) == 98 * W); // slot 98
    assert!(offset_of!(JNINativeInterface_, GetShortField) == 99 * W); // slot 99
    assert!(offset_of!(JNINativeInterface_, GetIntField) == 100 * W); // slot 100
    assert!(offset_of!(JNINativeInterface_, GetLongField) == 101 * W); // slot 101
    assert!(offset_of!(JNINativeInterface_, GetFloatField) == 102 * W); // slot 102
    assert!(offset_of!(JNINativeInterface_, GetDoubleField) == 103 * W); // slot 103
    assert!(offset_of!(JNINativeInterface_, SetObjectField) == 104 * W); // slot 104
    assert!(offset_of!(JNINativeInterface_, SetBooleanField) == 105 * W); // slot 105
    assert!(offset_of!(JNINativeInterface_, SetByteField) == 106 * W); // slot 106
    assert!(offset_of!(JNINativeInterface_, SetCharField) == 107 * W); // slot 107
    assert!(offset_of!(JNINativeInterface_, SetShortField) == 108 * W); // slot 108
    assert!(offset_of!(JNINativeInterface_, SetIntField) == 109 * W); // slot 109
    assert!(offset_of!(JNINativeInterface_, SetLongField) == 110 * W); // slot 110
    assert!(offset_of!(JNINativeInterface_, SetFloatField) == 111 * W); // slot 111
    assert!(offset_of!(JNINativeInterface_, SetDoubleField) == 112 * W); // slot 112
    assert!(offset_of!(JNINativeInterface_, GetStaticMethodID) == 113 * W); // slot 113
    assert!(offset_of!(JNINativeInterface_, CallStaticObjectMethod) == 114 * W); // slot 114
    assert!(offset_of!(JNINativeInterface_, CallStaticObjectMethodV) == 115 * W); // slot 115
    assert!(offset_of!(JNINativeInterface_, CallStaticObjectMethodA) == 116 * W); // slot 116
    assert!(offset_of!(JNINativeInterface_, CallStaticBooleanMethod) == 117 * W); // slot 117
    assert!(offset_of!(JNINativeInterface_, CallStaticBooleanMethodV) == 118 * W); // slot 118
    assert!(offset_of!(JNINativeInterface_, CallStaticBooleanMethodA) == 119 * W); // slot 119
    assert!(offset_of!(JNINativeInterface_, CallStaticByteMethod) == 120 * W); // slot 120
    assert!(offset_of!(JNINativeInterface_, CallStaticByteMethodV) == 121 * W); // slot 121
    assert!(offset_of!(JNINativeInterface_, CallStaticByteMethodA) == 122 * W); // slot 122
    assert!(offset_of!(JNINativeInterface_, CallStaticCharMethod) == 123 * W); // slot 123
    assert!(offset_of!(JNINativeInterface_, CallStaticCharMethodV) == 124 * W); // slot 124
    assert!(offset_of!(JNINativeInterface_, CallStaticCharMethodA) == 125 * W); // slot 125
    assert!(offset_of!(JNINativeInterface_, CallStaticShortMethod) == 126 * W); // slot 126
    assert!(offset_of!(JNINativeInterface_, CallStaticShortMethodV) == 127 * W); // slot 127
    assert!(offset_of!(JNINativeInterface_, CallStaticShortMethodA) == 128 * W); // slot 128
    assert!(offset_of!(JNINativeInterface_, CallStaticIntMethod) == 129 * W); // slot 129
    assert!(offset_of!(JNINativeInterface_, CallStaticIntMethodV) == 130 * W); // slot 130
    assert!(offset_of!(JNINativeInterface_, CallStaticIntMethodA) == 131 * W); // slot 131
    assert!(offset_of!(JNINativeInterface_, CallStaticLongMethod) == 132 * W); // slot 132
    assert!(offset_of!(JNINativeInterface_, CallStaticLongMethodV) == 133 * W); // slot 133
    assert!(offset_of!(JNINativeInterface_, CallStaticLongMethodA) == 134 * W); // slot 134
    assert!(offset_of!(JNINativeInterface_, CallStaticFloatMethod) == 135 * W); // slot 135
    assert!(offset_of!(JNINativeInterface_, CallStaticFloatMethodV) == 136 * W); // slot 136
    assert!(offset_of!(JNINativeInterface_, CallStaticFloatMethodA) == 137 * W); // slot 137
    assert!(offset_of!(JNINativeInterface_, CallStaticDoubleMethod) == 138 * W); // slot 138
    assert!(offset_of!(JNINativeInterface_, CallStaticDoubleMethodV) == 139 * W); // slot 139
    assert!(offset_of!(JNINativeInterface_, CallStaticDoubleMethodA) == 140 * W); // slot 140
    assert!(offset_of!(JNINativeInterface_, CallStaticVoidMethod) == 141 * W); // slot 141
    assert!(offset_of!(JNINativeInterface_, CallStaticVoidMethodV) == 142 * W); // slot 142
    assert!(offset_of!(JNINativeInterface_, CallStaticVoidMethodA) == 143 * W); // slot 143
    assert!(offset_of!(JNINativeInterface_, GetStaticFieldID) == 144 * W); // slot 144
    assert!(offset_of!(JNINativeInterface_, GetStaticObjectField) == 145 * W); // slot 145
    assert!(offset_of!(JNINativeInterface_, GetStaticBooleanField) == 146 * W); // slot 146
    assert!(offset_of!(JNINativeInterface_, GetStaticByteField) == 147 * W); // slot 147
    assert!(offset_of!(JNINativeInterface_, GetStaticCharField) == 148 * W); // slot 148
    assert!(offset_of!(JNINativeInterface_, GetStaticShortField) == 149 * W); // slot 149
    assert!(offset_of!(JNINativeInterface_, GetStaticIntField) == 150 * W); // slot 150
    assert!(offset_of!(JNINativeInterface_, GetStaticLongField) == 151 * W); // slot 151
    assert!(offset_of!(JNINativeInterface_, GetStaticFloatField) == 152 * W); // slot 152
    assert!(offset_of!(JNINativeInterface_, GetStaticDoubleField) == 153 * W); // slot 153
    assert!(offset_of!(JNINativeInterface_, SetStaticObjectField) == 154 * W); // slot 154
    assert!(offset_of!(JNINativeInterface_, SetStaticBooleanField) == 155 * W); // slot 155
    assert!(offset_of!(JNINativeInterface_, SetStaticByteField) == 156 * W); // slot 156
    assert!(offset_of!(JNINativeInterface_, SetStaticCharField) == 157 * W); // slot 157
    assert!(offset_of!(JNINativeInterface_, SetStaticShortField) == 158 * W); // slot 158
    assert!(offset_of!(JNINativeInterface_, SetStaticIntField) == 159 * W); // slot 159
    assert!(offset_of!(JNINativeInterface_, SetStaticLongField) == 160 * W); // slot 160
    assert!(offset_of!(JNINativeInterface_, SetStaticFloatField) == 161 * W); // slot 161
    assert!(offset_of!(JNINativeInterface_, SetStaticDoubleField) == 162 * W); // slot 162
    assert!(offset_of!(JNINativeInterface_, NewString) == 163 * W); // slot 163
    assert!(offset_of!(JNINativeInterface_, GetStringLength) == 164 * W); // slot 164
    assert!(offset_of!(JNINativeInterface_, GetStringChars) == 165 * W); // slot 165
    assert!(offset_of!(JNINativeInterface_, ReleaseStringChars) == 166 * W); // slot 166
    assert!(offset_of!(JNINativeInterface_, NewStringUTF) == 167 * W); // slot 167
    assert!(offset_of!(JNINativeInterface_, GetStringUTFLength) == 168 * W); // slot 168
    assert!(offset_of!(JNINativeInterface_, GetStringUTFChars) == 169 * W); // slot 169
    assert!(offset_of!(JNINativeInterface_, ReleaseStringUTFChars) == 170 * W); // slot 170
    assert!(offset_of!(JNINativeInterface_, GetArrayLength) == 171 * W); // slot 171
    assert!(offset_of!(JNINativeInterface_, NewObjectArray) == 172 * W); // slot 172
    assert!(offset_of!(JNINativeInterface_, GetObjectArrayElement) == 173 * W); // slot 173
    assert!(offset_of!(JNINativeInterface_, SetObjectArrayElement) == 174 * W); // slot 174
    assert!(offset_of!(JNINativeInterface_, NewBooleanArray) == 175 * W); // slot 175
    assert!(offset_of!(JNINativeInterface_, NewByteArray) == 176 * W); // slot 176
    assert!(offset_of!(JNINativeInterface_, NewCharArray) == 177 * W); // slot 177
    assert!(offset_of!(JNINativeInterface_, NewShortArray) == 178 * W); // slot 178
    assert!(offset_of!(JNINativeInterface_, NewIntArray) == 179 * W); // slot 179
    assert!(offset_of!(JNINativeInterface_, NewLongArray) == 180 * W); // slot 180
    assert!(offset_of!(JNINativeInterface_, NewFloatArray) == 181 * W); // slot 181
    assert!(offset_of!(JNINativeInterface_, NewDoubleArray) == 182 * W); // slot 182
    assert!(offset_of!(JNINativeInterface_, GetBooleanArrayElements) == 183 * W); // slot 183
    assert!(offset_of!(JNINativeInterface_, GetByteArrayElements) == 184 * W); // slot 184
    assert!(offset_of!(JNINativeInterface_, GetCharArrayElements) == 185 * W); // slot 185
    assert!(offset_of!(JNINativeInterface_, GetShortArrayElements) == 186 * W); // slot 186
    assert!(offset_of!(JNINativeInterface_, GetIntArrayElements) == 187 * W); // slot 187
    assert!(offset_of!(JNINativeInterface_, GetLongArrayElements) == 188 * W); // slot 188
    assert!(offset_of!(JNINativeInterface_, GetFloatArrayElements) == 189 * W); // slot 189
    assert!(offset_of!(JNINativeInterface_, GetDoubleArrayElements) == 190 * W); // slot 190
    assert!(offset_of!(JNINativeInterface_, ReleaseBooleanArrayElements) == 191 * W); // slot 191
    assert!(offset_of!(JNINativeInterface_, ReleaseByteArrayElements) == 192 * W); // slot 192
    assert!(offset_of!(JNINativeInterface_, ReleaseCharArrayElements) == 193 * W); // slot 193
    assert!(offset_of!(JNINativeInterface_, ReleaseShortArrayElements) == 194 * W); // slot 194
    assert!(offset_of!(JNINativeInterface_, ReleaseIntArrayElements) == 195 * W); // slot 195
    assert!(offset_of!(JNINativeInterface_, ReleaseLongArrayElements) == 196 * W); // slot 196
    assert!(offset_of!(JNINativeInterface_, ReleaseFloatArrayElements) == 197 * W); // slot 197
    assert!(offset_of!(JNINativeInterface_, ReleaseDoubleArrayElements) == 198 * W); // slot 198
    assert!(offset_of!(JNINativeInterface_, GetBooleanArrayRegion) == 199 * W); // slot 199
    assert!(offset_of!(JNINativeInterface_, GetByteArrayRegion) == 200 * W); // slot 200
    assert!(offset_of!(JNINativeInterface_, GetCharArrayRegion) == 201 * W); // slot 201
    assert!(offset_of!(JNINativeInterface_, GetShortArrayRegion) == 202 * W); // slot 202
    assert!(offset_of!(JNINativeInterface_, GetIntArrayRegion) == 203 * W); // slot 203
    assert!(offset_of!(JNINativeInterface_, GetLongArrayRegion) == 204 * W); // slot 204
    assert!(offset_of!(JNINativeInterface_, GetFloatArrayRegion) == 205 * W); // slot 205
    assert!(offset_of!(JNINativeInterface_, GetDoubleArrayRegion) == 206 * W); // slot 206
    assert!(offset_of!(JNINativeInterface_, SetBooleanArrayRegion) == 207 * W); // slot 207
    assert!(offset_of!(JNINativeInterface_, SetByteArrayRegion) == 208 * W); // slot 208
    assert!(offset_of!(JNINativeInterface_, SetCharArrayRegion) == 209 * W); // slot 209
    assert!(offset_of!(JNINativeInterface_, SetShortArrayRegion) == 210 * W); // slot 210
    assert!(offset_of!(JNINativeInterface_, SetIntArrayRegion) == 211 * W); // slot 211
    assert!(offset_of!(JNINativeInterface_, SetLongArrayRegion) == 212 * W); // slot 212
    assert!(offset_of!(JNINativeInterface_, SetFloatArrayRegion) == 213 * W); // slot 213
    assert!(offset_of!(JNINativeInterface_, SetDoubleArrayRegion) == 214 * W); // slot 214
    assert!(offset_of!(JNINativeInterface_, RegisterNatives) == 215 * W); // slot 215
    assert!(offset_of!(JNINativeInterface_, UnregisterNatives) == 216 * W); // slot 216
    assert!(offset_of!(JNINativeInterface_, MonitorEnter) == 217 * W); // slot 217
    assert!(offset_of!(JNINativeInterface_, MonitorExit) == 218 * W); // slot 218
    assert!(offset_of!(JNINativeInterface_, GetJavaVM) == 219 * W); // slot 219
    assert!(offset_of!(JNINativeInterface_, GetStringRegion) == 220 * W); // slot 220
    assert!(offset_of!(JNINativeInterface_, GetStringUTFRegion) == 221 * W); // slot 221
    assert!(offset_of!(JNINativeInterface_, GetPrimitiveArrayCritical) == 222 * W); // slot 222
    assert!(offset_of!(JNINativeInterface_, ReleasePrimitiveArrayCritical) == 223 * W); // slot 223
    assert!(offset_of!(JNINativeInterface_, GetStringCritical) == 224 * W); // slot 224
    assert!(offset_of!(JNINativeInterface_, ReleaseStringCritical) == 225 * W); // slot 225
    assert!(offset_of!(JNINativeInterface_, NewWeakGlobalRef) == 226 * W); // slot 226
    assert!(offset_of!(JNINativeInterface_, DeleteWeakGlobalRef) == 227 * W); // slot 227
    assert!(offset_of!(JNINativeInterface_, ExceptionCheck) == 228 * W); // slot 228
    assert!(offset_of!(JNINativeInterface_, NewDirectByteBuffer) == 229 * W); // slot 229
    assert!(offset_of!(JNINativeInterface_, GetDirectBufferAddress) == 230 * W); // slot 230
    assert!(offset_of!(JNINativeInterface_, GetDirectBufferCapacity) == 231 * W); // slot 231
    assert!(offset_of!(JNINativeInterface_, GetObjectRefType) == 232 * W); // slot 232
    assert!(offset_of!(JNINativeInterface_, GetModule) == 233 * W); // slot 233
    assert!(offset_of!(JNINativeInterface_, IsVirtualThread) == 234 * W); // slot 234
    assert!(size_of::<JNINativeInterface_>() == 235 * W);
    assert!(offset_of!(JNIEnv_, functions) == 0);
    assert!(offset_of!(JavaVM_, functions) == 0);
    assert!(size_of::<JNIEnv>() == W);
    assert!(size_of::<JavaVM>() == W);
    assert!(offset_of!(JNIInvokeInterface_, DestroyJavaVM) == 3 * W);
    assert!(offset_of!(JNIInvokeInterface_, GetEnv) == 6 * W);
    assert!(size_of::<JNIInvokeInterface_>() == 8 * W);
};

/// (slot name, fn pointer as usize) for every non-reserved slot — the
/// no-null-slots evidence; a native lib must never meet a null slot.
/// A runtime fn: fn-pointer-to-usize casts are not const-evaluable.
pub fn env_slot_addrs() -> Vec<(&'static str, usize)> {
    vec![
        ("GetVersion", ENV_TABLE.GetVersion as usize),
        ("DefineClass", ENV_TABLE.DefineClass as usize),
        ("FindClass", ENV_TABLE.FindClass as usize),
        (
            "FromReflectedMethod",
            ENV_TABLE.FromReflectedMethod as usize,
        ),
        ("FromReflectedField", ENV_TABLE.FromReflectedField as usize),
        ("ToReflectedMethod", ENV_TABLE.ToReflectedMethod as usize),
        ("GetSuperclass", ENV_TABLE.GetSuperclass as usize),
        ("IsAssignableFrom", ENV_TABLE.IsAssignableFrom as usize),
        ("ToReflectedField", ENV_TABLE.ToReflectedField as usize),
        ("Throw", ENV_TABLE.Throw as usize),
        ("ThrowNew", ENV_TABLE.ThrowNew as usize),
        ("ExceptionOccurred", ENV_TABLE.ExceptionOccurred as usize),
        ("ExceptionDescribe", ENV_TABLE.ExceptionDescribe as usize),
        ("ExceptionClear", ENV_TABLE.ExceptionClear as usize),
        ("FatalError", ENV_TABLE.FatalError as usize),
        ("PushLocalFrame", ENV_TABLE.PushLocalFrame as usize),
        ("PopLocalFrame", ENV_TABLE.PopLocalFrame as usize),
        ("NewGlobalRef", ENV_TABLE.NewGlobalRef as usize),
        ("DeleteGlobalRef", ENV_TABLE.DeleteGlobalRef as usize),
        ("DeleteLocalRef", ENV_TABLE.DeleteLocalRef as usize),
        ("IsSameObject", ENV_TABLE.IsSameObject as usize),
        ("NewLocalRef", ENV_TABLE.NewLocalRef as usize),
        (
            "EnsureLocalCapacity",
            ENV_TABLE.EnsureLocalCapacity as usize,
        ),
        ("AllocObject", ENV_TABLE.AllocObject as usize),
        ("NewObject", ENV_TABLE.NewObject as usize),
        ("NewObjectV", ENV_TABLE.NewObjectV as usize),
        ("NewObjectA", ENV_TABLE.NewObjectA as usize),
        ("GetObjectClass", ENV_TABLE.GetObjectClass as usize),
        ("IsInstanceOf", ENV_TABLE.IsInstanceOf as usize),
        ("GetMethodID", ENV_TABLE.GetMethodID as usize),
        ("CallObjectMethod", ENV_TABLE.CallObjectMethod as usize),
        ("CallObjectMethodV", ENV_TABLE.CallObjectMethodV as usize),
        ("CallObjectMethodA", ENV_TABLE.CallObjectMethodA as usize),
        ("CallBooleanMethod", ENV_TABLE.CallBooleanMethod as usize),
        ("CallBooleanMethodV", ENV_TABLE.CallBooleanMethodV as usize),
        ("CallBooleanMethodA", ENV_TABLE.CallBooleanMethodA as usize),
        ("CallByteMethod", ENV_TABLE.CallByteMethod as usize),
        ("CallByteMethodV", ENV_TABLE.CallByteMethodV as usize),
        ("CallByteMethodA", ENV_TABLE.CallByteMethodA as usize),
        ("CallCharMethod", ENV_TABLE.CallCharMethod as usize),
        ("CallCharMethodV", ENV_TABLE.CallCharMethodV as usize),
        ("CallCharMethodA", ENV_TABLE.CallCharMethodA as usize),
        ("CallShortMethod", ENV_TABLE.CallShortMethod as usize),
        ("CallShortMethodV", ENV_TABLE.CallShortMethodV as usize),
        ("CallShortMethodA", ENV_TABLE.CallShortMethodA as usize),
        ("CallIntMethod", ENV_TABLE.CallIntMethod as usize),
        ("CallIntMethodV", ENV_TABLE.CallIntMethodV as usize),
        ("CallIntMethodA", ENV_TABLE.CallIntMethodA as usize),
        ("CallLongMethod", ENV_TABLE.CallLongMethod as usize),
        ("CallLongMethodV", ENV_TABLE.CallLongMethodV as usize),
        ("CallLongMethodA", ENV_TABLE.CallLongMethodA as usize),
        ("CallFloatMethod", ENV_TABLE.CallFloatMethod as usize),
        ("CallFloatMethodV", ENV_TABLE.CallFloatMethodV as usize),
        ("CallFloatMethodA", ENV_TABLE.CallFloatMethodA as usize),
        ("CallDoubleMethod", ENV_TABLE.CallDoubleMethod as usize),
        ("CallDoubleMethodV", ENV_TABLE.CallDoubleMethodV as usize),
        ("CallDoubleMethodA", ENV_TABLE.CallDoubleMethodA as usize),
        ("CallVoidMethod", ENV_TABLE.CallVoidMethod as usize),
        ("CallVoidMethodV", ENV_TABLE.CallVoidMethodV as usize),
        ("CallVoidMethodA", ENV_TABLE.CallVoidMethodA as usize),
        (
            "CallNonvirtualObjectMethod",
            ENV_TABLE.CallNonvirtualObjectMethod as usize,
        ),
        (
            "CallNonvirtualObjectMethodV",
            ENV_TABLE.CallNonvirtualObjectMethodV as usize,
        ),
        (
            "CallNonvirtualObjectMethodA",
            ENV_TABLE.CallNonvirtualObjectMethodA as usize,
        ),
        (
            "CallNonvirtualBooleanMethod",
            ENV_TABLE.CallNonvirtualBooleanMethod as usize,
        ),
        (
            "CallNonvirtualBooleanMethodV",
            ENV_TABLE.CallNonvirtualBooleanMethodV as usize,
        ),
        (
            "CallNonvirtualBooleanMethodA",
            ENV_TABLE.CallNonvirtualBooleanMethodA as usize,
        ),
        (
            "CallNonvirtualByteMethod",
            ENV_TABLE.CallNonvirtualByteMethod as usize,
        ),
        (
            "CallNonvirtualByteMethodV",
            ENV_TABLE.CallNonvirtualByteMethodV as usize,
        ),
        (
            "CallNonvirtualByteMethodA",
            ENV_TABLE.CallNonvirtualByteMethodA as usize,
        ),
        (
            "CallNonvirtualCharMethod",
            ENV_TABLE.CallNonvirtualCharMethod as usize,
        ),
        (
            "CallNonvirtualCharMethodV",
            ENV_TABLE.CallNonvirtualCharMethodV as usize,
        ),
        (
            "CallNonvirtualCharMethodA",
            ENV_TABLE.CallNonvirtualCharMethodA as usize,
        ),
        (
            "CallNonvirtualShortMethod",
            ENV_TABLE.CallNonvirtualShortMethod as usize,
        ),
        (
            "CallNonvirtualShortMethodV",
            ENV_TABLE.CallNonvirtualShortMethodV as usize,
        ),
        (
            "CallNonvirtualShortMethodA",
            ENV_TABLE.CallNonvirtualShortMethodA as usize,
        ),
        (
            "CallNonvirtualIntMethod",
            ENV_TABLE.CallNonvirtualIntMethod as usize,
        ),
        (
            "CallNonvirtualIntMethodV",
            ENV_TABLE.CallNonvirtualIntMethodV as usize,
        ),
        (
            "CallNonvirtualIntMethodA",
            ENV_TABLE.CallNonvirtualIntMethodA as usize,
        ),
        (
            "CallNonvirtualLongMethod",
            ENV_TABLE.CallNonvirtualLongMethod as usize,
        ),
        (
            "CallNonvirtualLongMethodV",
            ENV_TABLE.CallNonvirtualLongMethodV as usize,
        ),
        (
            "CallNonvirtualLongMethodA",
            ENV_TABLE.CallNonvirtualLongMethodA as usize,
        ),
        (
            "CallNonvirtualFloatMethod",
            ENV_TABLE.CallNonvirtualFloatMethod as usize,
        ),
        (
            "CallNonvirtualFloatMethodV",
            ENV_TABLE.CallNonvirtualFloatMethodV as usize,
        ),
        (
            "CallNonvirtualFloatMethodA",
            ENV_TABLE.CallNonvirtualFloatMethodA as usize,
        ),
        (
            "CallNonvirtualDoubleMethod",
            ENV_TABLE.CallNonvirtualDoubleMethod as usize,
        ),
        (
            "CallNonvirtualDoubleMethodV",
            ENV_TABLE.CallNonvirtualDoubleMethodV as usize,
        ),
        (
            "CallNonvirtualDoubleMethodA",
            ENV_TABLE.CallNonvirtualDoubleMethodA as usize,
        ),
        (
            "CallNonvirtualVoidMethod",
            ENV_TABLE.CallNonvirtualVoidMethod as usize,
        ),
        (
            "CallNonvirtualVoidMethodV",
            ENV_TABLE.CallNonvirtualVoidMethodV as usize,
        ),
        (
            "CallNonvirtualVoidMethodA",
            ENV_TABLE.CallNonvirtualVoidMethodA as usize,
        ),
        ("GetFieldID", ENV_TABLE.GetFieldID as usize),
        ("GetObjectField", ENV_TABLE.GetObjectField as usize),
        ("GetBooleanField", ENV_TABLE.GetBooleanField as usize),
        ("GetByteField", ENV_TABLE.GetByteField as usize),
        ("GetCharField", ENV_TABLE.GetCharField as usize),
        ("GetShortField", ENV_TABLE.GetShortField as usize),
        ("GetIntField", ENV_TABLE.GetIntField as usize),
        ("GetLongField", ENV_TABLE.GetLongField as usize),
        ("GetFloatField", ENV_TABLE.GetFloatField as usize),
        ("GetDoubleField", ENV_TABLE.GetDoubleField as usize),
        ("SetObjectField", ENV_TABLE.SetObjectField as usize),
        ("SetBooleanField", ENV_TABLE.SetBooleanField as usize),
        ("SetByteField", ENV_TABLE.SetByteField as usize),
        ("SetCharField", ENV_TABLE.SetCharField as usize),
        ("SetShortField", ENV_TABLE.SetShortField as usize),
        ("SetIntField", ENV_TABLE.SetIntField as usize),
        ("SetLongField", ENV_TABLE.SetLongField as usize),
        ("SetFloatField", ENV_TABLE.SetFloatField as usize),
        ("SetDoubleField", ENV_TABLE.SetDoubleField as usize),
        ("GetStaticMethodID", ENV_TABLE.GetStaticMethodID as usize),
        (
            "CallStaticObjectMethod",
            ENV_TABLE.CallStaticObjectMethod as usize,
        ),
        (
            "CallStaticObjectMethodV",
            ENV_TABLE.CallStaticObjectMethodV as usize,
        ),
        (
            "CallStaticObjectMethodA",
            ENV_TABLE.CallStaticObjectMethodA as usize,
        ),
        (
            "CallStaticBooleanMethod",
            ENV_TABLE.CallStaticBooleanMethod as usize,
        ),
        (
            "CallStaticBooleanMethodV",
            ENV_TABLE.CallStaticBooleanMethodV as usize,
        ),
        (
            "CallStaticBooleanMethodA",
            ENV_TABLE.CallStaticBooleanMethodA as usize,
        ),
        (
            "CallStaticByteMethod",
            ENV_TABLE.CallStaticByteMethod as usize,
        ),
        (
            "CallStaticByteMethodV",
            ENV_TABLE.CallStaticByteMethodV as usize,
        ),
        (
            "CallStaticByteMethodA",
            ENV_TABLE.CallStaticByteMethodA as usize,
        ),
        (
            "CallStaticCharMethod",
            ENV_TABLE.CallStaticCharMethod as usize,
        ),
        (
            "CallStaticCharMethodV",
            ENV_TABLE.CallStaticCharMethodV as usize,
        ),
        (
            "CallStaticCharMethodA",
            ENV_TABLE.CallStaticCharMethodA as usize,
        ),
        (
            "CallStaticShortMethod",
            ENV_TABLE.CallStaticShortMethod as usize,
        ),
        (
            "CallStaticShortMethodV",
            ENV_TABLE.CallStaticShortMethodV as usize,
        ),
        (
            "CallStaticShortMethodA",
            ENV_TABLE.CallStaticShortMethodA as usize,
        ),
        (
            "CallStaticIntMethod",
            ENV_TABLE.CallStaticIntMethod as usize,
        ),
        (
            "CallStaticIntMethodV",
            ENV_TABLE.CallStaticIntMethodV as usize,
        ),
        (
            "CallStaticIntMethodA",
            ENV_TABLE.CallStaticIntMethodA as usize,
        ),
        (
            "CallStaticLongMethod",
            ENV_TABLE.CallStaticLongMethod as usize,
        ),
        (
            "CallStaticLongMethodV",
            ENV_TABLE.CallStaticLongMethodV as usize,
        ),
        (
            "CallStaticLongMethodA",
            ENV_TABLE.CallStaticLongMethodA as usize,
        ),
        (
            "CallStaticFloatMethod",
            ENV_TABLE.CallStaticFloatMethod as usize,
        ),
        (
            "CallStaticFloatMethodV",
            ENV_TABLE.CallStaticFloatMethodV as usize,
        ),
        (
            "CallStaticFloatMethodA",
            ENV_TABLE.CallStaticFloatMethodA as usize,
        ),
        (
            "CallStaticDoubleMethod",
            ENV_TABLE.CallStaticDoubleMethod as usize,
        ),
        (
            "CallStaticDoubleMethodV",
            ENV_TABLE.CallStaticDoubleMethodV as usize,
        ),
        (
            "CallStaticDoubleMethodA",
            ENV_TABLE.CallStaticDoubleMethodA as usize,
        ),
        (
            "CallStaticVoidMethod",
            ENV_TABLE.CallStaticVoidMethod as usize,
        ),
        (
            "CallStaticVoidMethodV",
            ENV_TABLE.CallStaticVoidMethodV as usize,
        ),
        (
            "CallStaticVoidMethodA",
            ENV_TABLE.CallStaticVoidMethodA as usize,
        ),
        ("GetStaticFieldID", ENV_TABLE.GetStaticFieldID as usize),
        (
            "GetStaticObjectField",
            ENV_TABLE.GetStaticObjectField as usize,
        ),
        (
            "GetStaticBooleanField",
            ENV_TABLE.GetStaticBooleanField as usize,
        ),
        ("GetStaticByteField", ENV_TABLE.GetStaticByteField as usize),
        ("GetStaticCharField", ENV_TABLE.GetStaticCharField as usize),
        (
            "GetStaticShortField",
            ENV_TABLE.GetStaticShortField as usize,
        ),
        ("GetStaticIntField", ENV_TABLE.GetStaticIntField as usize),
        ("GetStaticLongField", ENV_TABLE.GetStaticLongField as usize),
        (
            "GetStaticFloatField",
            ENV_TABLE.GetStaticFloatField as usize,
        ),
        (
            "GetStaticDoubleField",
            ENV_TABLE.GetStaticDoubleField as usize,
        ),
        (
            "SetStaticObjectField",
            ENV_TABLE.SetStaticObjectField as usize,
        ),
        (
            "SetStaticBooleanField",
            ENV_TABLE.SetStaticBooleanField as usize,
        ),
        ("SetStaticByteField", ENV_TABLE.SetStaticByteField as usize),
        ("SetStaticCharField", ENV_TABLE.SetStaticCharField as usize),
        (
            "SetStaticShortField",
            ENV_TABLE.SetStaticShortField as usize,
        ),
        ("SetStaticIntField", ENV_TABLE.SetStaticIntField as usize),
        ("SetStaticLongField", ENV_TABLE.SetStaticLongField as usize),
        (
            "SetStaticFloatField",
            ENV_TABLE.SetStaticFloatField as usize,
        ),
        (
            "SetStaticDoubleField",
            ENV_TABLE.SetStaticDoubleField as usize,
        ),
        ("NewString", ENV_TABLE.NewString as usize),
        ("GetStringLength", ENV_TABLE.GetStringLength as usize),
        ("GetStringChars", ENV_TABLE.GetStringChars as usize),
        ("ReleaseStringChars", ENV_TABLE.ReleaseStringChars as usize),
        ("NewStringUTF", ENV_TABLE.NewStringUTF as usize),
        ("GetStringUTFLength", ENV_TABLE.GetStringUTFLength as usize),
        ("GetStringUTFChars", ENV_TABLE.GetStringUTFChars as usize),
        (
            "ReleaseStringUTFChars",
            ENV_TABLE.ReleaseStringUTFChars as usize,
        ),
        ("GetArrayLength", ENV_TABLE.GetArrayLength as usize),
        ("NewObjectArray", ENV_TABLE.NewObjectArray as usize),
        (
            "GetObjectArrayElement",
            ENV_TABLE.GetObjectArrayElement as usize,
        ),
        (
            "SetObjectArrayElement",
            ENV_TABLE.SetObjectArrayElement as usize,
        ),
        ("NewBooleanArray", ENV_TABLE.NewBooleanArray as usize),
        ("NewByteArray", ENV_TABLE.NewByteArray as usize),
        ("NewCharArray", ENV_TABLE.NewCharArray as usize),
        ("NewShortArray", ENV_TABLE.NewShortArray as usize),
        ("NewIntArray", ENV_TABLE.NewIntArray as usize),
        ("NewLongArray", ENV_TABLE.NewLongArray as usize),
        ("NewFloatArray", ENV_TABLE.NewFloatArray as usize),
        ("NewDoubleArray", ENV_TABLE.NewDoubleArray as usize),
        (
            "GetBooleanArrayElements",
            ENV_TABLE.GetBooleanArrayElements as usize,
        ),
        (
            "GetByteArrayElements",
            ENV_TABLE.GetByteArrayElements as usize,
        ),
        (
            "GetCharArrayElements",
            ENV_TABLE.GetCharArrayElements as usize,
        ),
        (
            "GetShortArrayElements",
            ENV_TABLE.GetShortArrayElements as usize,
        ),
        (
            "GetIntArrayElements",
            ENV_TABLE.GetIntArrayElements as usize,
        ),
        (
            "GetLongArrayElements",
            ENV_TABLE.GetLongArrayElements as usize,
        ),
        (
            "GetFloatArrayElements",
            ENV_TABLE.GetFloatArrayElements as usize,
        ),
        (
            "GetDoubleArrayElements",
            ENV_TABLE.GetDoubleArrayElements as usize,
        ),
        (
            "ReleaseBooleanArrayElements",
            ENV_TABLE.ReleaseBooleanArrayElements as usize,
        ),
        (
            "ReleaseByteArrayElements",
            ENV_TABLE.ReleaseByteArrayElements as usize,
        ),
        (
            "ReleaseCharArrayElements",
            ENV_TABLE.ReleaseCharArrayElements as usize,
        ),
        (
            "ReleaseShortArrayElements",
            ENV_TABLE.ReleaseShortArrayElements as usize,
        ),
        (
            "ReleaseIntArrayElements",
            ENV_TABLE.ReleaseIntArrayElements as usize,
        ),
        (
            "ReleaseLongArrayElements",
            ENV_TABLE.ReleaseLongArrayElements as usize,
        ),
        (
            "ReleaseFloatArrayElements",
            ENV_TABLE.ReleaseFloatArrayElements as usize,
        ),
        (
            "ReleaseDoubleArrayElements",
            ENV_TABLE.ReleaseDoubleArrayElements as usize,
        ),
        (
            "GetBooleanArrayRegion",
            ENV_TABLE.GetBooleanArrayRegion as usize,
        ),
        ("GetByteArrayRegion", ENV_TABLE.GetByteArrayRegion as usize),
        ("GetCharArrayRegion", ENV_TABLE.GetCharArrayRegion as usize),
        (
            "GetShortArrayRegion",
            ENV_TABLE.GetShortArrayRegion as usize,
        ),
        ("GetIntArrayRegion", ENV_TABLE.GetIntArrayRegion as usize),
        ("GetLongArrayRegion", ENV_TABLE.GetLongArrayRegion as usize),
        (
            "GetFloatArrayRegion",
            ENV_TABLE.GetFloatArrayRegion as usize,
        ),
        (
            "GetDoubleArrayRegion",
            ENV_TABLE.GetDoubleArrayRegion as usize,
        ),
        (
            "SetBooleanArrayRegion",
            ENV_TABLE.SetBooleanArrayRegion as usize,
        ),
        ("SetByteArrayRegion", ENV_TABLE.SetByteArrayRegion as usize),
        ("SetCharArrayRegion", ENV_TABLE.SetCharArrayRegion as usize),
        (
            "SetShortArrayRegion",
            ENV_TABLE.SetShortArrayRegion as usize,
        ),
        ("SetIntArrayRegion", ENV_TABLE.SetIntArrayRegion as usize),
        ("SetLongArrayRegion", ENV_TABLE.SetLongArrayRegion as usize),
        (
            "SetFloatArrayRegion",
            ENV_TABLE.SetFloatArrayRegion as usize,
        ),
        (
            "SetDoubleArrayRegion",
            ENV_TABLE.SetDoubleArrayRegion as usize,
        ),
        ("RegisterNatives", ENV_TABLE.RegisterNatives as usize),
        ("UnregisterNatives", ENV_TABLE.UnregisterNatives as usize),
        ("MonitorEnter", ENV_TABLE.MonitorEnter as usize),
        ("MonitorExit", ENV_TABLE.MonitorExit as usize),
        ("GetJavaVM", ENV_TABLE.GetJavaVM as usize),
        ("GetStringRegion", ENV_TABLE.GetStringRegion as usize),
        ("GetStringUTFRegion", ENV_TABLE.GetStringUTFRegion as usize),
        (
            "GetPrimitiveArrayCritical",
            ENV_TABLE.GetPrimitiveArrayCritical as usize,
        ),
        (
            "ReleasePrimitiveArrayCritical",
            ENV_TABLE.ReleasePrimitiveArrayCritical as usize,
        ),
        ("GetStringCritical", ENV_TABLE.GetStringCritical as usize),
        (
            "ReleaseStringCritical",
            ENV_TABLE.ReleaseStringCritical as usize,
        ),
        ("NewWeakGlobalRef", ENV_TABLE.NewWeakGlobalRef as usize),
        (
            "DeleteWeakGlobalRef",
            ENV_TABLE.DeleteWeakGlobalRef as usize,
        ),
        ("ExceptionCheck", ENV_TABLE.ExceptionCheck as usize),
        (
            "NewDirectByteBuffer",
            ENV_TABLE.NewDirectByteBuffer as usize,
        ),
        (
            "GetDirectBufferAddress",
            ENV_TABLE.GetDirectBufferAddress as usize,
        ),
        (
            "GetDirectBufferCapacity",
            ENV_TABLE.GetDirectBufferCapacity as usize,
        ),
        ("GetObjectRefType", ENV_TABLE.GetObjectRefType as usize),
        ("GetModule", ENV_TABLE.GetModule as usize),
        ("IsVirtualThread", ENV_TABLE.IsVirtualThread as usize),
    ]
}

extern "system" fn stub_DefineClass(
    _env: *mut JNIEnv_,
    _name: *const c_char,
    _loader: jobject,
    _buf: *const jbyte,
    _len: jsize,
) -> jobject {
    unimplemented_slot(5, "DefineClass")
}
extern "system" fn stub_FindClass(_env: *mut JNIEnv_, _name: *const c_char) -> jobject {
    unimplemented_slot(6, "FindClass")
}
extern "system" fn stub_FromReflectedMethod(_env: *mut JNIEnv_, _method: jobject) -> c_void {
    unimplemented_slot(7, "FromReflectedMethod")
}
extern "system" fn stub_FromReflectedField(_env: *mut JNIEnv_, _field: jobject) -> c_void {
    unimplemented_slot(8, "FromReflectedField")
}
extern "system" fn stub_ToReflectedMethod(
    _env: *mut JNIEnv_,
    _cls: jobject,
    _methodID: c_void,
    _isStatic: jboolean,
) -> jobject {
    unimplemented_slot(9, "ToReflectedMethod")
}
extern "system" fn stub_GetSuperclass(_env: *mut JNIEnv_, _sub: jobject) -> jobject {
    unimplemented_slot(10, "GetSuperclass")
}
extern "system" fn stub_IsAssignableFrom(
    _env: *mut JNIEnv_,
    _sub: jobject,
    _sup: jobject,
) -> jboolean {
    unimplemented_slot(11, "IsAssignableFrom")
}
extern "system" fn stub_ToReflectedField(
    _env: *mut JNIEnv_,
    _cls: jobject,
    _fieldID: c_void,
    _isStatic: jboolean,
) -> jobject {
    unimplemented_slot(12, "ToReflectedField")
}
extern "system" fn stub_Throw(_env: *mut JNIEnv_, _obj: jobject) -> jint {
    unimplemented_slot(13, "Throw")
}
extern "system" fn stub_ThrowNew(_env: *mut JNIEnv_, _clazz: jobject, _msg: *const c_char) -> jint {
    unimplemented_slot(14, "ThrowNew")
}
extern "system" fn stub_ExceptionOccurred(_env: *mut JNIEnv_) -> jobject {
    unimplemented_slot(15, "ExceptionOccurred")
}
extern "system" fn stub_ExceptionDescribe(_env: *mut JNIEnv_) {
    unimplemented_slot(16, "ExceptionDescribe")
}
extern "system" fn stub_ExceptionClear(_env: *mut JNIEnv_) {
    unimplemented_slot(17, "ExceptionClear")
}
extern "system" fn stub_FatalError(_env: *mut JNIEnv_, _msg: *const c_char) {
    unimplemented_slot(18, "FatalError")
}
extern "system" fn stub_PushLocalFrame(_env: *mut JNIEnv_, _capacity: jint) -> jint {
    unimplemented_slot(19, "PushLocalFrame")
}
extern "system" fn stub_PopLocalFrame(_env: *mut JNIEnv_, _result: jobject) -> jobject {
    unimplemented_slot(20, "PopLocalFrame")
}
extern "system" fn stub_NewGlobalRef(_env: *mut JNIEnv_, _lobj: jobject) -> jobject {
    unimplemented_slot(21, "NewGlobalRef")
}
extern "system" fn stub_DeleteGlobalRef(_env: *mut JNIEnv_, _gref: jobject) {
    unimplemented_slot(22, "DeleteGlobalRef")
}
extern "system" fn stub_DeleteLocalRef(_env: *mut JNIEnv_, _obj: jobject) {
    unimplemented_slot(23, "DeleteLocalRef")
}
extern "system" fn stub_IsSameObject(
    _env: *mut JNIEnv_,
    _obj1: jobject,
    _obj2: jobject,
) -> jboolean {
    unimplemented_slot(24, "IsSameObject")
}
extern "system" fn stub_NewLocalRef(_env: *mut JNIEnv_, _ref: jobject) -> jobject {
    unimplemented_slot(25, "NewLocalRef")
}
extern "system" fn stub_EnsureLocalCapacity(_env: *mut JNIEnv_, _capacity: jint) -> jint {
    unimplemented_slot(26, "EnsureLocalCapacity")
}
extern "system" fn stub_AllocObject(_env: *mut JNIEnv_, _clazz: jobject) -> jobject {
    unimplemented_slot(27, "AllocObject")
}
extern "system" fn stub_NewObject(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jobject {
    unimplemented_slot(28, "NewObject")
}
extern "system" fn stub_NewObjectV(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jobject {
    unimplemented_slot(29, "NewObjectV")
}
extern "system" fn stub_NewObjectA(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jobject {
    unimplemented_slot(30, "NewObjectA")
}
extern "system" fn stub_GetObjectClass(_env: *mut JNIEnv_, _obj: jobject) -> jobject {
    unimplemented_slot(31, "GetObjectClass")
}
extern "system" fn stub_IsInstanceOf(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
) -> jboolean {
    unimplemented_slot(32, "IsInstanceOf")
}
extern "system" fn stub_GetMethodID(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _name: *const c_char,
    _sig: *const c_char,
) -> c_void {
    unimplemented_slot(33, "GetMethodID")
}
extern "system" fn stub_CallObjectMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jobject {
    unimplemented_slot(34, "CallObjectMethod")
}
extern "system" fn stub_CallObjectMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jobject {
    unimplemented_slot(35, "CallObjectMethodV")
}
extern "system" fn stub_CallObjectMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jobject {
    unimplemented_slot(36, "CallObjectMethodA")
}
extern "system" fn stub_CallBooleanMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jboolean {
    unimplemented_slot(37, "CallBooleanMethod")
}
extern "system" fn stub_CallBooleanMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jboolean {
    unimplemented_slot(38, "CallBooleanMethodV")
}
extern "system" fn stub_CallBooleanMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jboolean {
    unimplemented_slot(39, "CallBooleanMethodA")
}
extern "system" fn stub_CallByteMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jbyte {
    unimplemented_slot(40, "CallByteMethod")
}
extern "system" fn stub_CallByteMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jbyte {
    unimplemented_slot(41, "CallByteMethodV")
}
extern "system" fn stub_CallByteMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jbyte {
    unimplemented_slot(42, "CallByteMethodA")
}
extern "system" fn stub_CallCharMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jchar {
    unimplemented_slot(43, "CallCharMethod")
}
extern "system" fn stub_CallCharMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jchar {
    unimplemented_slot(44, "CallCharMethodV")
}
extern "system" fn stub_CallCharMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jchar {
    unimplemented_slot(45, "CallCharMethodA")
}
extern "system" fn stub_CallShortMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jshort {
    unimplemented_slot(46, "CallShortMethod")
}
extern "system" fn stub_CallShortMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jshort {
    unimplemented_slot(47, "CallShortMethodV")
}
extern "system" fn stub_CallShortMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jshort {
    unimplemented_slot(48, "CallShortMethodA")
}
extern "system" fn stub_CallIntMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jint {
    unimplemented_slot(49, "CallIntMethod")
}
extern "system" fn stub_CallIntMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jint {
    unimplemented_slot(50, "CallIntMethodV")
}
extern "system" fn stub_CallIntMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jint {
    unimplemented_slot(51, "CallIntMethodA")
}
extern "system" fn stub_CallLongMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jlong {
    unimplemented_slot(52, "CallLongMethod")
}
extern "system" fn stub_CallLongMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jlong {
    unimplemented_slot(53, "CallLongMethodV")
}
extern "system" fn stub_CallLongMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jlong {
    unimplemented_slot(54, "CallLongMethodA")
}
extern "system" fn stub_CallFloatMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jfloat {
    unimplemented_slot(55, "CallFloatMethod")
}
extern "system" fn stub_CallFloatMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jfloat {
    unimplemented_slot(56, "CallFloatMethodV")
}
extern "system" fn stub_CallFloatMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jfloat {
    unimplemented_slot(57, "CallFloatMethodA")
}
extern "system" fn stub_CallDoubleMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jdouble {
    unimplemented_slot(58, "CallDoubleMethod")
}
extern "system" fn stub_CallDoubleMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jdouble {
    unimplemented_slot(59, "CallDoubleMethodV")
}
extern "system" fn stub_CallDoubleMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jdouble {
    unimplemented_slot(60, "CallDoubleMethodA")
}
extern "system" fn stub_CallVoidMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) {
    unimplemented_slot(61, "CallVoidMethod")
}
extern "system" fn stub_CallVoidMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: VaList,
) {
    unimplemented_slot(62, "CallVoidMethodV")
}
extern "system" fn stub_CallVoidMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _methodID: c_void,
    _args: *const c_void,
) {
    unimplemented_slot(63, "CallVoidMethodA")
}
extern "system" fn stub_CallNonvirtualObjectMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jobject {
    unimplemented_slot(64, "CallNonvirtualObjectMethod")
}
extern "system" fn stub_CallNonvirtualObjectMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jobject {
    unimplemented_slot(65, "CallNonvirtualObjectMethodV")
}
extern "system" fn stub_CallNonvirtualObjectMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jobject {
    unimplemented_slot(66, "CallNonvirtualObjectMethodA")
}
extern "system" fn stub_CallNonvirtualBooleanMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jboolean {
    unimplemented_slot(67, "CallNonvirtualBooleanMethod")
}
extern "system" fn stub_CallNonvirtualBooleanMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jboolean {
    unimplemented_slot(68, "CallNonvirtualBooleanMethodV")
}
extern "system" fn stub_CallNonvirtualBooleanMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jboolean {
    unimplemented_slot(69, "CallNonvirtualBooleanMethodA")
}
extern "system" fn stub_CallNonvirtualByteMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jbyte {
    unimplemented_slot(70, "CallNonvirtualByteMethod")
}
extern "system" fn stub_CallNonvirtualByteMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jbyte {
    unimplemented_slot(71, "CallNonvirtualByteMethodV")
}
extern "system" fn stub_CallNonvirtualByteMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jbyte {
    unimplemented_slot(72, "CallNonvirtualByteMethodA")
}
extern "system" fn stub_CallNonvirtualCharMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jchar {
    unimplemented_slot(73, "CallNonvirtualCharMethod")
}
extern "system" fn stub_CallNonvirtualCharMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jchar {
    unimplemented_slot(74, "CallNonvirtualCharMethodV")
}
extern "system" fn stub_CallNonvirtualCharMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jchar {
    unimplemented_slot(75, "CallNonvirtualCharMethodA")
}
extern "system" fn stub_CallNonvirtualShortMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jshort {
    unimplemented_slot(76, "CallNonvirtualShortMethod")
}
extern "system" fn stub_CallNonvirtualShortMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jshort {
    unimplemented_slot(77, "CallNonvirtualShortMethodV")
}
extern "system" fn stub_CallNonvirtualShortMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jshort {
    unimplemented_slot(78, "CallNonvirtualShortMethodA")
}
extern "system" fn stub_CallNonvirtualIntMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jint {
    unimplemented_slot(79, "CallNonvirtualIntMethod")
}
extern "system" fn stub_CallNonvirtualIntMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jint {
    unimplemented_slot(80, "CallNonvirtualIntMethodV")
}
extern "system" fn stub_CallNonvirtualIntMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jint {
    unimplemented_slot(81, "CallNonvirtualIntMethodA")
}
extern "system" fn stub_CallNonvirtualLongMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jlong {
    unimplemented_slot(82, "CallNonvirtualLongMethod")
}
extern "system" fn stub_CallNonvirtualLongMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jlong {
    unimplemented_slot(83, "CallNonvirtualLongMethodV")
}
extern "system" fn stub_CallNonvirtualLongMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jlong {
    unimplemented_slot(84, "CallNonvirtualLongMethodA")
}
extern "system" fn stub_CallNonvirtualFloatMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jfloat {
    unimplemented_slot(85, "CallNonvirtualFloatMethod")
}
extern "system" fn stub_CallNonvirtualFloatMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jfloat {
    unimplemented_slot(86, "CallNonvirtualFloatMethodV")
}
extern "system" fn stub_CallNonvirtualFloatMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jfloat {
    unimplemented_slot(87, "CallNonvirtualFloatMethodA")
}
extern "system" fn stub_CallNonvirtualDoubleMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jdouble {
    unimplemented_slot(88, "CallNonvirtualDoubleMethod")
}
extern "system" fn stub_CallNonvirtualDoubleMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jdouble {
    unimplemented_slot(89, "CallNonvirtualDoubleMethodV")
}
extern "system" fn stub_CallNonvirtualDoubleMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jdouble {
    unimplemented_slot(90, "CallNonvirtualDoubleMethodA")
}
extern "system" fn stub_CallNonvirtualVoidMethod(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) {
    unimplemented_slot(91, "CallNonvirtualVoidMethod")
}
extern "system" fn stub_CallNonvirtualVoidMethodV(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) {
    unimplemented_slot(92, "CallNonvirtualVoidMethodV")
}
extern "system" fn stub_CallNonvirtualVoidMethodA(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) {
    unimplemented_slot(93, "CallNonvirtualVoidMethodA")
}
extern "system" fn stub_GetFieldID(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _name: *const c_char,
    _sig: *const c_char,
) -> c_void {
    unimplemented_slot(94, "GetFieldID")
}
extern "system" fn stub_GetObjectField(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _fieldID: c_void,
) -> jobject {
    unimplemented_slot(95, "GetObjectField")
}
extern "system" fn stub_GetBooleanField(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _fieldID: c_void,
) -> jboolean {
    unimplemented_slot(96, "GetBooleanField")
}
extern "system" fn stub_GetByteField(_env: *mut JNIEnv_, _obj: jobject, _fieldID: c_void) -> jbyte {
    unimplemented_slot(97, "GetByteField")
}
extern "system" fn stub_GetCharField(_env: *mut JNIEnv_, _obj: jobject, _fieldID: c_void) -> jchar {
    unimplemented_slot(98, "GetCharField")
}
extern "system" fn stub_GetShortField(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _fieldID: c_void,
) -> jshort {
    unimplemented_slot(99, "GetShortField")
}
extern "system" fn stub_GetIntField(_env: *mut JNIEnv_, _obj: jobject, _fieldID: c_void) -> jint {
    unimplemented_slot(100, "GetIntField")
}
extern "system" fn stub_GetLongField(_env: *mut JNIEnv_, _obj: jobject, _fieldID: c_void) -> jlong {
    unimplemented_slot(101, "GetLongField")
}
extern "system" fn stub_GetFloatField(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _fieldID: c_void,
) -> jfloat {
    unimplemented_slot(102, "GetFloatField")
}
extern "system" fn stub_GetDoubleField(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _fieldID: c_void,
) -> jdouble {
    unimplemented_slot(103, "GetDoubleField")
}
extern "system" fn stub_SetObjectField(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _fieldID: c_void,
    _val: jobject,
) {
    unimplemented_slot(104, "SetObjectField")
}
extern "system" fn stub_SetBooleanField(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _fieldID: c_void,
    _val: jboolean,
) {
    unimplemented_slot(105, "SetBooleanField")
}
extern "system" fn stub_SetByteField(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _fieldID: c_void,
    _val: jbyte,
) {
    unimplemented_slot(106, "SetByteField")
}
extern "system" fn stub_SetCharField(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _fieldID: c_void,
    _val: jchar,
) {
    unimplemented_slot(107, "SetCharField")
}
extern "system" fn stub_SetShortField(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _fieldID: c_void,
    _val: jshort,
) {
    unimplemented_slot(108, "SetShortField")
}
extern "system" fn stub_SetIntField(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _fieldID: c_void,
    _val: jint,
) {
    unimplemented_slot(109, "SetIntField")
}
extern "system" fn stub_SetLongField(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _fieldID: c_void,
    _val: jlong,
) {
    unimplemented_slot(110, "SetLongField")
}
extern "system" fn stub_SetFloatField(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _fieldID: c_void,
    _val: jfloat,
) {
    unimplemented_slot(111, "SetFloatField")
}
extern "system" fn stub_SetDoubleField(
    _env: *mut JNIEnv_,
    _obj: jobject,
    _fieldID: c_void,
    _val: jdouble,
) {
    unimplemented_slot(112, "SetDoubleField")
}
extern "system" fn stub_GetStaticMethodID(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _name: *const c_char,
    _sig: *const c_char,
) -> c_void {
    unimplemented_slot(113, "GetStaticMethodID")
}
extern "system" fn stub_CallStaticObjectMethod(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jobject {
    unimplemented_slot(114, "CallStaticObjectMethod")
}
extern "system" fn stub_CallStaticObjectMethodV(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jobject {
    unimplemented_slot(115, "CallStaticObjectMethodV")
}
extern "system" fn stub_CallStaticObjectMethodA(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jobject {
    unimplemented_slot(116, "CallStaticObjectMethodA")
}
extern "system" fn stub_CallStaticBooleanMethod(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jboolean {
    unimplemented_slot(117, "CallStaticBooleanMethod")
}
extern "system" fn stub_CallStaticBooleanMethodV(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jboolean {
    unimplemented_slot(118, "CallStaticBooleanMethodV")
}
extern "system" fn stub_CallStaticBooleanMethodA(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jboolean {
    unimplemented_slot(119, "CallStaticBooleanMethodA")
}
extern "system" fn stub_CallStaticByteMethod(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jbyte {
    unimplemented_slot(120, "CallStaticByteMethod")
}
extern "system" fn stub_CallStaticByteMethodV(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jbyte {
    unimplemented_slot(121, "CallStaticByteMethodV")
}
extern "system" fn stub_CallStaticByteMethodA(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jbyte {
    unimplemented_slot(122, "CallStaticByteMethodA")
}
extern "system" fn stub_CallStaticCharMethod(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jchar {
    unimplemented_slot(123, "CallStaticCharMethod")
}
extern "system" fn stub_CallStaticCharMethodV(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jchar {
    unimplemented_slot(124, "CallStaticCharMethodV")
}
extern "system" fn stub_CallStaticCharMethodA(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jchar {
    unimplemented_slot(125, "CallStaticCharMethodA")
}
extern "system" fn stub_CallStaticShortMethod(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jshort {
    unimplemented_slot(126, "CallStaticShortMethod")
}
extern "system" fn stub_CallStaticShortMethodV(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jshort {
    unimplemented_slot(127, "CallStaticShortMethodV")
}
extern "system" fn stub_CallStaticShortMethodA(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jshort {
    unimplemented_slot(128, "CallStaticShortMethodA")
}
extern "system" fn stub_CallStaticIntMethod(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jint {
    unimplemented_slot(129, "CallStaticIntMethod")
}
extern "system" fn stub_CallStaticIntMethodV(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jint {
    unimplemented_slot(130, "CallStaticIntMethodV")
}
extern "system" fn stub_CallStaticIntMethodA(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jint {
    unimplemented_slot(131, "CallStaticIntMethodA")
}
extern "system" fn stub_CallStaticLongMethod(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jlong {
    unimplemented_slot(132, "CallStaticLongMethod")
}
extern "system" fn stub_CallStaticLongMethodV(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jlong {
    unimplemented_slot(133, "CallStaticLongMethodV")
}
extern "system" fn stub_CallStaticLongMethodA(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jlong {
    unimplemented_slot(134, "CallStaticLongMethodA")
}
extern "system" fn stub_CallStaticFloatMethod(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jfloat {
    unimplemented_slot(135, "CallStaticFloatMethod")
}
extern "system" fn stub_CallStaticFloatMethodV(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jfloat {
    unimplemented_slot(136, "CallStaticFloatMethodV")
}
extern "system" fn stub_CallStaticFloatMethodA(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jfloat {
    unimplemented_slot(137, "CallStaticFloatMethodA")
}
extern "system" fn stub_CallStaticDoubleMethod(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jdouble {
    unimplemented_slot(138, "CallStaticDoubleMethod")
}
extern "system" fn stub_CallStaticDoubleMethodV(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: VaList,
) -> jdouble {
    unimplemented_slot(139, "CallStaticDoubleMethodV")
}
extern "system" fn stub_CallStaticDoubleMethodA(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methodID: c_void,
    _args: *const c_void,
) -> jdouble {
    unimplemented_slot(140, "CallStaticDoubleMethodA")
}
extern "system" fn stub_CallStaticVoidMethod(
    _env: *mut JNIEnv_,
    _cls: jobject,
    _methodID: c_void,
    _args: *const c_void,
) {
    unimplemented_slot(141, "CallStaticVoidMethod")
}
extern "system" fn stub_CallStaticVoidMethodV(
    _env: *mut JNIEnv_,
    _cls: jobject,
    _methodID: c_void,
    _args: VaList,
) {
    unimplemented_slot(142, "CallStaticVoidMethodV")
}
extern "system" fn stub_CallStaticVoidMethodA(
    _env: *mut JNIEnv_,
    _cls: jobject,
    _methodID: c_void,
    _args: *const c_void,
) {
    unimplemented_slot(143, "CallStaticVoidMethodA")
}
extern "system" fn stub_GetStaticFieldID(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _name: *const c_char,
    _sig: *const c_char,
) -> c_void {
    unimplemented_slot(144, "GetStaticFieldID")
}
extern "system" fn stub_GetStaticObjectField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
) -> jobject {
    unimplemented_slot(145, "GetStaticObjectField")
}
extern "system" fn stub_GetStaticBooleanField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
) -> jboolean {
    unimplemented_slot(146, "GetStaticBooleanField")
}
extern "system" fn stub_GetStaticByteField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
) -> jbyte {
    unimplemented_slot(147, "GetStaticByteField")
}
extern "system" fn stub_GetStaticCharField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
) -> jchar {
    unimplemented_slot(148, "GetStaticCharField")
}
extern "system" fn stub_GetStaticShortField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
) -> jshort {
    unimplemented_slot(149, "GetStaticShortField")
}
extern "system" fn stub_GetStaticIntField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
) -> jint {
    unimplemented_slot(150, "GetStaticIntField")
}
extern "system" fn stub_GetStaticLongField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
) -> jlong {
    unimplemented_slot(151, "GetStaticLongField")
}
extern "system" fn stub_GetStaticFloatField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
) -> jfloat {
    unimplemented_slot(152, "GetStaticFloatField")
}
extern "system" fn stub_GetStaticDoubleField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
) -> jdouble {
    unimplemented_slot(153, "GetStaticDoubleField")
}
extern "system" fn stub_SetStaticObjectField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
    _value: jobject,
) {
    unimplemented_slot(154, "SetStaticObjectField")
}
extern "system" fn stub_SetStaticBooleanField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
    _value: jboolean,
) {
    unimplemented_slot(155, "SetStaticBooleanField")
}
extern "system" fn stub_SetStaticByteField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
    _value: jbyte,
) {
    unimplemented_slot(156, "SetStaticByteField")
}
extern "system" fn stub_SetStaticCharField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
    _value: jchar,
) {
    unimplemented_slot(157, "SetStaticCharField")
}
extern "system" fn stub_SetStaticShortField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
    _value: jshort,
) {
    unimplemented_slot(158, "SetStaticShortField")
}
extern "system" fn stub_SetStaticIntField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
    _value: jint,
) {
    unimplemented_slot(159, "SetStaticIntField")
}
extern "system" fn stub_SetStaticLongField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
    _value: jlong,
) {
    unimplemented_slot(160, "SetStaticLongField")
}
extern "system" fn stub_SetStaticFloatField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
    _value: jfloat,
) {
    unimplemented_slot(161, "SetStaticFloatField")
}
extern "system" fn stub_SetStaticDoubleField(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _fieldID: c_void,
    _value: jdouble,
) {
    unimplemented_slot(162, "SetStaticDoubleField")
}
extern "system" fn stub_GetStringLength(_env: *mut JNIEnv_, _str: jobject) -> jsize {
    unimplemented_slot(164, "GetStringLength")
}
extern "system" fn stub_GetStringChars(
    _env: *mut JNIEnv_,
    _str: jobject,
    _isCopy: *mut jboolean,
) -> *const jchar {
    unimplemented_slot(165, "GetStringChars")
}
extern "system" fn stub_ReleaseStringChars(
    _env: *mut JNIEnv_,
    _str: jobject,
    _chars: *const jchar,
) {
    unimplemented_slot(166, "ReleaseStringChars")
}
extern "system" fn stub_GetArrayLength(_env: *mut JNIEnv_, _array: jobject) -> jsize {
    unimplemented_slot(171, "GetArrayLength")
}
extern "system" fn stub_NewObjectArray(
    _env: *mut JNIEnv_,
    _len: jsize,
    _clazz: jobject,
    _init: jobject,
) -> jobject {
    unimplemented_slot(172, "NewObjectArray")
}
extern "system" fn stub_GetObjectArrayElement(
    _env: *mut JNIEnv_,
    _array: jobject,
    _index: jsize,
) -> jobject {
    unimplemented_slot(173, "GetObjectArrayElement")
}
extern "system" fn stub_SetObjectArrayElement(
    _env: *mut JNIEnv_,
    _array: jobject,
    _index: jsize,
    _val: jobject,
) {
    unimplemented_slot(174, "SetObjectArrayElement")
}
extern "system" fn stub_NewBooleanArray(_env: *mut JNIEnv_, _len: jsize) -> jobject {
    unimplemented_slot(175, "NewBooleanArray")
}
extern "system" fn stub_NewByteArray(_env: *mut JNIEnv_, _len: jsize) -> jobject {
    unimplemented_slot(176, "NewByteArray")
}
extern "system" fn stub_NewCharArray(_env: *mut JNIEnv_, _len: jsize) -> jobject {
    unimplemented_slot(177, "NewCharArray")
}
extern "system" fn stub_NewShortArray(_env: *mut JNIEnv_, _len: jsize) -> jobject {
    unimplemented_slot(178, "NewShortArray")
}
extern "system" fn stub_NewIntArray(_env: *mut JNIEnv_, _len: jsize) -> jobject {
    unimplemented_slot(179, "NewIntArray")
}
extern "system" fn stub_NewLongArray(_env: *mut JNIEnv_, _len: jsize) -> jobject {
    unimplemented_slot(180, "NewLongArray")
}
extern "system" fn stub_NewFloatArray(_env: *mut JNIEnv_, _len: jsize) -> jobject {
    unimplemented_slot(181, "NewFloatArray")
}
extern "system" fn stub_NewDoubleArray(_env: *mut JNIEnv_, _len: jsize) -> jobject {
    unimplemented_slot(182, "NewDoubleArray")
}
extern "system" fn stub_GetBooleanArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _isCopy: *mut jboolean,
) -> *mut jboolean {
    unimplemented_slot(183, "GetBooleanArrayElements")
}
extern "system" fn stub_GetByteArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _isCopy: *mut jboolean,
) -> *mut jbyte {
    unimplemented_slot(184, "GetByteArrayElements")
}
extern "system" fn stub_GetCharArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _isCopy: *mut jboolean,
) -> *mut jchar {
    unimplemented_slot(185, "GetCharArrayElements")
}
extern "system" fn stub_GetShortArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _isCopy: *mut jboolean,
) -> *mut jshort {
    unimplemented_slot(186, "GetShortArrayElements")
}
extern "system" fn stub_GetIntArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _isCopy: *mut jboolean,
) -> *mut jint {
    unimplemented_slot(187, "GetIntArrayElements")
}
extern "system" fn stub_GetLongArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _isCopy: *mut jboolean,
) -> *mut jlong {
    unimplemented_slot(188, "GetLongArrayElements")
}
extern "system" fn stub_GetFloatArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _isCopy: *mut jboolean,
) -> *mut jfloat {
    unimplemented_slot(189, "GetFloatArrayElements")
}
extern "system" fn stub_GetDoubleArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _isCopy: *mut jboolean,
) -> *mut jdouble {
    unimplemented_slot(190, "GetDoubleArrayElements")
}
extern "system" fn stub_ReleaseBooleanArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _elems: *mut jboolean,
    _mode: jint,
) {
    unimplemented_slot(191, "ReleaseBooleanArrayElements")
}
extern "system" fn stub_ReleaseByteArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _elems: *mut jbyte,
    _mode: jint,
) {
    unimplemented_slot(192, "ReleaseByteArrayElements")
}
extern "system" fn stub_ReleaseCharArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _elems: *mut jchar,
    _mode: jint,
) {
    unimplemented_slot(193, "ReleaseCharArrayElements")
}
extern "system" fn stub_ReleaseShortArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _elems: *mut jshort,
    _mode: jint,
) {
    unimplemented_slot(194, "ReleaseShortArrayElements")
}
extern "system" fn stub_ReleaseIntArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _elems: *mut jint,
    _mode: jint,
) {
    unimplemented_slot(195, "ReleaseIntArrayElements")
}
extern "system" fn stub_ReleaseLongArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _elems: *mut jlong,
    _mode: jint,
) {
    unimplemented_slot(196, "ReleaseLongArrayElements")
}
extern "system" fn stub_ReleaseFloatArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _elems: *mut jfloat,
    _mode: jint,
) {
    unimplemented_slot(197, "ReleaseFloatArrayElements")
}
extern "system" fn stub_ReleaseDoubleArrayElements(
    _env: *mut JNIEnv_,
    _array: jobject,
    _elems: *mut jdouble,
    _mode: jint,
) {
    unimplemented_slot(198, "ReleaseDoubleArrayElements")
}
extern "system" fn stub_GetBooleanArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _l: jsize,
    _buf: *mut jboolean,
) {
    unimplemented_slot(199, "GetBooleanArrayRegion")
}
extern "system" fn stub_GetByteArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *mut jbyte,
) {
    unimplemented_slot(200, "GetByteArrayRegion")
}
extern "system" fn stub_GetCharArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *mut jchar,
) {
    unimplemented_slot(201, "GetCharArrayRegion")
}
extern "system" fn stub_GetShortArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *mut jshort,
) {
    unimplemented_slot(202, "GetShortArrayRegion")
}
extern "system" fn stub_GetIntArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *mut jint,
) {
    unimplemented_slot(203, "GetIntArrayRegion")
}
extern "system" fn stub_GetLongArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *mut jlong,
) {
    unimplemented_slot(204, "GetLongArrayRegion")
}
extern "system" fn stub_GetFloatArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *mut jfloat,
) {
    unimplemented_slot(205, "GetFloatArrayRegion")
}
extern "system" fn stub_GetDoubleArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *mut jdouble,
) {
    unimplemented_slot(206, "GetDoubleArrayRegion")
}
extern "system" fn stub_SetBooleanArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _l: jsize,
    _buf: *const jboolean,
) {
    unimplemented_slot(207, "SetBooleanArrayRegion")
}
extern "system" fn stub_SetByteArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *const jbyte,
) {
    unimplemented_slot(208, "SetByteArrayRegion")
}
extern "system" fn stub_SetCharArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *const jchar,
) {
    unimplemented_slot(209, "SetCharArrayRegion")
}
extern "system" fn stub_SetShortArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *const jshort,
) {
    unimplemented_slot(210, "SetShortArrayRegion")
}
extern "system" fn stub_SetIntArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *const jint,
) {
    unimplemented_slot(211, "SetIntArrayRegion")
}
extern "system" fn stub_SetLongArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *const jlong,
) {
    unimplemented_slot(212, "SetLongArrayRegion")
}
extern "system" fn stub_SetFloatArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *const jfloat,
) {
    unimplemented_slot(213, "SetFloatArrayRegion")
}
extern "system" fn stub_SetDoubleArrayRegion(
    _env: *mut JNIEnv_,
    _array: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *const jdouble,
) {
    unimplemented_slot(214, "SetDoubleArrayRegion")
}
extern "system" fn stub_RegisterNatives(
    _env: *mut JNIEnv_,
    _clazz: jobject,
    _methods: *const JNINativeMethod,
    _nMethods: jint,
) -> jint {
    unimplemented_slot(215, "RegisterNatives")
}
extern "system" fn stub_UnregisterNatives(_env: *mut JNIEnv_, _clazz: jobject) -> jint {
    unimplemented_slot(216, "UnregisterNatives")
}
extern "system" fn stub_MonitorEnter(_env: *mut JNIEnv_, _obj: jobject) -> jint {
    unimplemented_slot(217, "MonitorEnter")
}
extern "system" fn stub_MonitorExit(_env: *mut JNIEnv_, _obj: jobject) -> jint {
    unimplemented_slot(218, "MonitorExit")
}
extern "system" fn stub_GetJavaVM(_env: *mut JNIEnv_, _vm: *mut *mut JavaVM_) -> jint {
    unimplemented_slot(219, "GetJavaVM")
}
extern "system" fn stub_GetStringRegion(
    _env: *mut JNIEnv_,
    _str: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *mut jchar,
) {
    unimplemented_slot(220, "GetStringRegion")
}
extern "system" fn stub_GetStringUTFRegion(
    _env: *mut JNIEnv_,
    _str: jobject,
    _start: jsize,
    _len: jsize,
    _buf: *mut c_char,
) {
    unimplemented_slot(221, "GetStringUTFRegion")
}
extern "system" fn stub_GetPrimitiveArrayCritical(
    _env: *mut JNIEnv_,
    _array: jobject,
    _isCopy: *mut jboolean,
) -> *mut c_void {
    unimplemented_slot(222, "GetPrimitiveArrayCritical")
}
extern "system" fn stub_ReleasePrimitiveArrayCritical(
    _env: *mut JNIEnv_,
    _array: jobject,
    _carray: *mut c_void,
    _mode: jint,
) {
    unimplemented_slot(223, "ReleasePrimitiveArrayCritical")
}
extern "system" fn stub_GetStringCritical(
    _env: *mut JNIEnv_,
    _string: jobject,
    _isCopy: *mut jboolean,
) -> *const jchar {
    unimplemented_slot(224, "GetStringCritical")
}
extern "system" fn stub_ReleaseStringCritical(
    _env: *mut JNIEnv_,
    _string: jobject,
    _cstring: *const jchar,
) {
    unimplemented_slot(225, "ReleaseStringCritical")
}
extern "system" fn stub_NewWeakGlobalRef(_env: *mut JNIEnv_, _obj: jobject) -> jobject {
    unimplemented_slot(226, "NewWeakGlobalRef")
}
extern "system" fn stub_DeleteWeakGlobalRef(_env: *mut JNIEnv_, _ref: jobject) {
    unimplemented_slot(227, "DeleteWeakGlobalRef")
}
extern "system" fn stub_ExceptionCheck(_env: *mut JNIEnv_) -> jboolean {
    unimplemented_slot(228, "ExceptionCheck")
}
extern "system" fn stub_NewDirectByteBuffer(
    _env: *mut JNIEnv_,
    _address: *mut c_void,
    _capacity: jlong,
) -> jobject {
    unimplemented_slot(229, "NewDirectByteBuffer")
}
extern "system" fn stub_GetDirectBufferAddress(_env: *mut JNIEnv_, _buf: jobject) -> *mut c_void {
    unimplemented_slot(230, "GetDirectBufferAddress")
}
extern "system" fn stub_GetDirectBufferCapacity(_env: *mut JNIEnv_, _buf: jobject) -> jlong {
    unimplemented_slot(231, "GetDirectBufferCapacity")
}
extern "system" fn stub_GetObjectRefType(_env: *mut JNIEnv_, _obj: jobject) -> jobjectRefType {
    unimplemented_slot(232, "GetObjectRefType")
}
extern "system" fn stub_GetModule(_env: *mut JNIEnv_, _clazz: jobject) -> jobject {
    unimplemented_slot(233, "GetModule")
}
extern "system" fn stub_IsVirtualThread(_env: *mut JNIEnv_, _obj: jobject) -> jboolean {
    unimplemented_slot(234, "IsVirtualThread")
}

// ---- T10 slice 2: real string-family slot wrappers. The REAL_SLOTS
// registry above points the five string slots at these; the loud-stub
// default still covers every not-yet-wired slot. ----

/// `NewString` (slot 163): UTF-16 code units (native-endian — the
/// ABI's own shape) → a rooted local-ref `jstring` (T10 slice 2).
/// Null units or negative length → JNI null (spec NPE shape pending the
/// exception machinery — documented v1 gap, T13-registered). Unpaired
/// surrogates normalize to U+FFFD — the pinned lossy law. A zero
/// length with non-null units is the valid empty string.
extern "system" fn jni_new_string(_env: JNIEnv, units: *const jchar, len: jsize) -> jobject {
    if units.is_null() || len < 0 {
        return std::ptr::null_mut();
    }
    let bytes = j2r_jni_ffi::ffi::copy_from_caller(units as *const u8, (len as usize) * 2);
    let mut packed = vec![0u16; len as usize];
    for (slot16, pair) in packed.iter_mut().zip(bytes.as_chunks::<2>().0) {
        *slot16 = u16::from_ne_bytes(*pair);
    }
    crate::jni::bridge::tag_to_jobject(crate::jni::strings::slot_new_string(&packed))
}

/// `NewStringUTF` (slot 167): NUL-terminated modified UTF-8 → a
/// rooted local-ref `jstring` (T10 slice 2). Null input or malformed
/// mUTF-8 → JNI null (the spec's NPE/NoSuchElementError shapes need the
/// exception machinery — v1 returns null; registered for the T13
/// harness, which judges against Temurin three-channel). The bytes
/// cross through the audited `read_caller_str` primitive; the decode
/// is the slice-1 classfile-mirror law.
extern "system" fn jni_new_string_utf(_env: JNIEnv, utf: *const c_char) -> jobject {
    let Some(bytes) = j2r_jni_ffi::ffi::read_caller_str(utf) else {
        return std::ptr::null_mut();
    };
    match crate::jni::strings::slot_new_string_utf(&bytes) {
        Some(tag) => crate::jni::bridge::tag_to_jobject(tag),
        None => std::ptr::null_mut(),
    }
}

/// `GetStringUTFLength` (slot 168): the modified-UTF-8 byte count,
/// terminator excluded (the JNI spec's own definition) — T10 slice 2.
/// Null jstring → 0 (v1 gap: the spec's NPE shape needs the exception
/// machinery; registered for the T13 harness). A non-String handle
/// panics at resolve — the T09 loud law.
extern "system" fn jni_get_string_utf_length(_env: JNIEnv, string: jobject) -> jsize {
    let tag = crate::jni::bridge::jobject_to_tag(string);
    if tag == 0 {
        return 0;
    }
    crate::jni::strings::slot_get_string_utf_length(tag) as jsize
}

/// `GetStringUTFChars` (slot 169): hands the caller a NUL-terminated
/// modified-UTF-8 copy, registry-pinned until `ReleaseStringUTFChars`
/// (a double release or foreign pointer is a LOUD registry miss —
/// across this boundary, a process abort). T10 slice 2. `isCopy`,
/// when the caller asks, is written JNI_TRUE through the audited
/// `write_u8_out` primitive (M5_JNI_FFI.md §4 entry 7 — this slice's
/// amendment: one byte, never the pointer-word primitive). Null
/// jstring → NULL, isCopy untouched (v1 gap, T13-registered).
extern "system" fn jni_get_string_utf_chars(
    _env: JNIEnv,
    string: jobject,
    is_copy: *mut jboolean,
) -> *const c_char {
    let tag = crate::jni::bridge::jobject_to_tag(string);
    if tag == 0 {
        return std::ptr::null();
    }
    let addr = crate::jni::strings::slot_get_string_utf_chars(tag, None);
    if !is_copy.is_null() {
        j2r_jni_ffi::ffi::write_u8_out(is_copy, 1); // JNI_TRUE — always a copy here
    }
    addr as *const c_char
}

/// `ReleaseStringUTFChars` (slot 170): frees the registry-pinned
/// buffer — T10 slice 2. Null string or null chars pointer → no-op
/// (the slice-1 law). Anything else MUST be a pointer this slot
/// handed out; a miss panics loudly, which across the extern boundary
/// is the process-abort loudness shape (pinned by a child-process
/// test in jni/mod.rs).
extern "system" fn jni_release_string_utf_chars(
    _env: JNIEnv,
    string: jobject,
    chars: *const c_char,
) {
    if crate::jni::bridge::jobject_to_tag(string) == 0 || chars.is_null() {
        return;
    }
    crate::jni::strings::slot_release_string_utf_chars(chars as usize);
}
