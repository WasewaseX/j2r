/* T13 13c — the ONE C harness (docs/M5_JNI_HARNESS.md §1; REVIEW_CHECKLIST
   G1.1/G1.2). Compiled ONCE against the pinned JDK's jni.h of record and
   run against BOTH runtimes: Temurin (System.loadLibrary) and the j2r
   binary (the SAME System.loadLibrary, served by the 13c loading story —
   audited primitive 12: dlopen + dlsym + the ABI-fixed JNI_OnLoad call).

   JNI_OnLoad GetEnvs the env table (VM slot 6) and RegisterNatives-binds
   every probe below; the probes then exercise the v1 surface FROM REAL C
   and return Java-visible verdicts. ALL output flows through Java's
   System.out — the C side never writes a channel directly, so the
   three-channel parity is by construction. Each probe folds its checks
   into a bit verdict; the Java driver's check() exits 3 on ANY mismatch,
   so a broken probe fails the run loudly on BOTH sides (self-checking).

   v1 registered gaps are NOT pretended here: weak global refs,
   cross-thread local-resolve, Monitor*, GetStringRegion and the
   string/array exception transports stay loud on the j2r side — 13d's
   battery asserts that loudness separately. The battery stays inside
   the primitive-11 register window (max 4 GP + 8 FP words per native).

   Build (the parity runner's job):
     cc -O1 -shared -fPIC -I$JAVA_HOME/include -I$JAVA_HOME/include/linux \
        harness.c -o libjni_harness.so
*/
#include <jni.h>
#include <string.h>
#include <stdarg.h>

static JavaVM *g_vm; /* cached by probe_version via GetJavaVM (219) */

/* JNI hygiene: NO cached jclass statics — a JNI_OnLoad local ref is
   invalid after OnLoad returns (HotSpot SIGSEGVs on the stale use; our
   interned-mirror tags happen to tolerate it, but the harness must be
   correct JNI on BOTH runtimes). Every probe that needs the class
   re-FindClasses a fresh local ref and uses it within its own frame. */

/* -- versioning / VM (inventory §3.1) ---------------------------------- */

/* Bit verdicts: 1 = GetVersion == 0x00150000 (the JDK-major form
   Temurin 21 answers — measured live; NOT the old 1.8 assumption),
   2 = GetJavaVM OK, 4 = GetEnv(1.8) OK, 8 = GetEnv(0x000b0000 = "11")
   EVERSION, 32 = the env GetEnv hands back IS the env this probe
   received. (The null-penv and version-0 arms are unit-pinned
   Rust-side; HotSpot's behavior there is not part of the battery.)
   Expect 47. */
static jint probe_version(JNIEnv *env, jclass clazz) {
    (void)clazz;
    jint verdict = 0;
    if ((*env)->GetVersion(env) == 0x00150000) verdict |= 1;
    if ((*env)->GetJavaVM(env, &g_vm) == JNI_OK && g_vm != NULL) verdict |= 2;
    void *penv = NULL;
    if ((*g_vm)->GetEnv(g_vm, &penv, JNI_VERSION_1_8) == JNI_OK && penv != NULL)
        verdict |= 4;
    penv = NULL;
    if ((*g_vm)->GetEnv(g_vm, &penv, 0x000b0000) == JNI_EVERSION) verdict |= 8;
    penv = NULL;
    if ((*g_vm)->GetEnv(g_vm, &penv, JNI_VERSION_1_8) == JNI_OK &&
        penv == (void *)env)
        verdict |= 32;
    return verdict;
}

/* -- classes/fields (inventory §3.2) ----------------------------------- */

/* instField: 41 -> 42 (SetIntField); statField: 1000 -> 2000. Returns
   the new statField; the driver re-reads BOTH fields Java-side. */
static jlong probe_fields(JNIEnv *env, jobject thiz) {
    jclass c = (*env)->GetObjectClass(env, thiz);
    jfieldID fid = (*env)->GetFieldID(env, c, "instField", "I");
    jint v = (*env)->GetIntField(env, thiz, fid);
    (*env)->SetIntField(env, thiz, fid, v + 1);
    jfieldID sfid = (*env)->GetStaticFieldID(env, c, "statField", "J");
    jlong sv = (*env)->GetStaticLongField(env, c, sfid);
    sv = sv * 2;
    (*env)->SetStaticLongField(env, c, sfid, sv);
    return sv;
}

/* -- strings, both directions, the mUTF-8 edges (inventory §3.4) ------- */

/* s1 = "A\0B" (the ENCODED NUL 0xC0 0x80); s2 = U+1F600 (the
   supplementary pair, two 3-byte mUTF-8 sequences). Expect 3. */
static jint probe_strings(JNIEnv *env, jobject thiz, jstring s1, jstring s2) {
    (void)thiz;
    jint verdict = 0;
    jsize l1 = (*env)->GetStringUTFLength(env, s1);
    const char *p1 = (*env)->GetStringUTFChars(env, s1, NULL);
    static const unsigned char e1[] = {'A', 0xC0, 0x80, 'B'};
    if (l1 == 4 && p1 != NULL && memcmp(p1, e1, 4) == 0 && p1[4] == 0)
        verdict |= 1;
    (*env)->ReleaseStringUTFChars(env, s1, p1);
    jsize l2 = (*env)->GetStringUTFLength(env, s2);
    const char *p2 = (*env)->GetStringUTFChars(env, s2, NULL);
    static const unsigned char e2[] = {0xED, 0xA0, 0xBD, 0xED, 0xB8, 0x80};
    if (l2 == 6 && p2 != NULL && memcmp(p2, e2, 6) == 0) verdict |= 2;
    (*env)->ReleaseStringUTFChars(env, s2, p2);
    return verdict;
}

/* NewStringUTF over an encoded NUL: "j2r" + U+0000 + "ok" — 6 chars (7
   mUTF-8 bytes); the driver prints .length() == 6. */
static jstring probe_string_back(JNIEnv *env, jclass clazz) {
    (void)clazz;
    static const char mutf[] = {'j', '2', 'r', (char)0xC0, (char)0x80,
                                'o', 'k', 0};
    return (*env)->NewStringUTF(env, mutf);
}

/* NewString (UTF-16) with a supplementary pair + an embedded NUL jchar:
   the driver prints .length() == 4 and codePointCount == 3. */
static jstring probe_utf16(JNIEnv *env, jclass clazz) {
    (void)clazz;
    static const jchar u[] = {0xD83D, 0xDE00, 0x0000, (jchar)'A'};
    return (*env)->NewString(env, u, 4);
}

/* -- arrays: elements + all three release modes + regions + criticals
   (inventory §3.5) ------------------------------------------------------ */

/* Expect 63. NOTE the honest JNI_ABORT arm: it CALLS the abort mode
   (the call path is the parity point) but does NOT assert that the
   mutation vanished — with a direct-pointer Get (HotSpot) the mutation
   is already in the array, so that assertion would diverge by design.
   The ABORT-drops-mutation law is unit-pinned Rust-side (T10 slice 3b,
   the copy-mode registry). */
static jint probe_arrays(JNIEnv *env, jclass clazz) {
    (void)clazz;
    jint verdict = 0;
    jintArray a = (*env)->NewIntArray(env, 5);
    if (a == NULL) return 0;
    jint setBuf[5] = {5, 4, 3, 2, 1};
    (*env)->SetIntArrayRegion(env, a, 0, 5, setBuf);
    jboolean isCopy = JNI_FALSE;
    jint *elems = (*env)->GetIntArrayElements(env, a, &isCopy);
    if (elems != NULL && elems[0] == 5 && elems[4] == 1) verdict |= 1;
    elems[0] = 100;
    (*env)->ReleaseIntArrayElements(env, a, elems, 0); /* copy back + free */
    jint getBuf[5] = {0, 0, 0, 0, 0};
    (*env)->GetIntArrayRegion(env, a, 0, 5, getBuf);
    if (getBuf[0] == 100 && getBuf[4] == 1) verdict |= 2;
    jint *e2 = (*env)->GetIntArrayElements(env, a, NULL);
    e2[1] = 200;
    (*env)->ReleaseIntArrayElements(env, a, e2, JNI_COMMIT); /* back, mapped */
    jint *e3 = (*env)->GetIntArrayElements(env, a, NULL);
    e3[2] = 300;
    (*env)->ReleaseIntArrayElements(env, a, e3, JNI_ABORT); /* path only */
    jint getBuf2[5] = {0, 0, 0, 0, 0};
    (*env)->GetIntArrayRegion(env, a, 0, 5, getBuf2);
    if (getBuf2[1] == 200) verdict |= 4;
    jint *crit = (jint *)(*env)->GetPrimitiveArrayCritical(env, a, NULL);
    if (crit != NULL && crit[0] == 100) verdict |= 8;
    crit[3] = 400;
    (*env)->ReleasePrimitiveArrayCritical(env, a, crit, 0);
    jint getBuf3[5] = {0, 0, 0, 0, 0};
    (*env)->GetIntArrayRegion(env, a, 0, 5, getBuf3);
    if (getBuf3[3] == 400) verdict |= 16;
    if ((*env)->GetArrayLength(env, a) == 5) verdict |= 32;
    return verdict;
}

/* Object array: NewObjectArray + Set/GetObjectArrayElement; returned to
   Java, which prints its contents. */
static jobjectArray probe_obj_array(JNIEnv *env, jclass clazz) {
    (void)clazz;
    jclass strCls = (*env)->FindClass(env, "java/lang/String");
    jobjectArray arr = (*env)->NewObjectArray(env, 2, strCls, NULL);
    (*env)->SetObjectArrayElement(env, arr, 0,
                                  (*env)->NewStringUTF(env, "one"));
    (*env)->SetObjectArrayElement(env, arr, 1,
                                  (*env)->NewStringUTF(env, "two"));
    return arr;
}

/* -- methods: the A-form families (inventory §3.3) ---------------------- */

/* Expect 255. Covers: CallIntMethodA (virtual dispatch — the override),
   CallObjectMethodA (a returned string, re-read via GetStringUTFChars),
   CallVoidMethodA (the callback PRINTS — stdout flows Java-side),
   the exception round-trip (ExceptionOccurred/Check/Clear + the message
   read through env), CallStaticIntMethodA, and the nonvirtual declared-
   class dispatch (Base's body) against the virtual override. */
static jint probe_calls_a(JNIEnv *env, jobject thiz) {
    jint verdict = 0;
    jclass c = (*env)->GetObjectClass(env, thiz);
    jmethodID cb = (*env)->GetMethodID(env, c, "callback", "(I)I");
    jvalue a1;
    a1.i = 42;
    if ((*env)->CallIntMethodA(env, thiz, cb, &a1) == 126) verdict |= 1;
    jmethodID gr = (*env)->GetMethodID(env, c, "greet", "()Ljava/lang/String;");
    jstring g = (jstring)(*env)->CallObjectMethodA(env, thiz, gr, NULL);
    if (g != NULL) {
        const char *gp = (*env)->GetStringUTFChars(env, g, NULL);
        if (gp != NULL && strcmp(gp, "c-greet") == 0) verdict |= 2;
        (*env)->ReleaseStringUTFChars(env, g, gp);
    }
    jmethodID an = (*env)->GetMethodID(env, c, "announce",
                                       "(Ljava/lang/String;)V");
    jvalue av;
    av.l = (*env)->NewStringUTF(env, "via-jni-a");
    (*env)->CallVoidMethodA(env, thiz, an, &av); /* stdout: cb:via-jni-a */
    jmethodID bm = (*env)->GetMethodID(env, c, "boom", "()I");
    (*env)->CallIntMethodA(env, thiz, bm, NULL); /* throws; zero return */
    jthrowable t = (*env)->ExceptionOccurred(env);
    if (t != NULL && (*env)->ExceptionCheck(env)) {
        verdict |= 4;
        /* The message is read through a COMPILED bridge method — a
           builtin class (java/lang/IllegalStateException) has no
           compiled facts, so a GetMethodID on it would honestly answer
           JNI null and the marker panic would be the loud law. */
        jmethodID gm = (*env)->GetMethodID(env, c, "boomMessage",
                                           "(Ljava/lang/Throwable;)Ljava/lang/String;");
        jvalue mv;
        mv.l = t;
        jstring msg = (jstring)(*env)->CallObjectMethodA(env, thiz, gm, &mv);
        const char *mp = (*env)->GetStringUTFChars(env, msg, NULL);
        if (mp != NULL && strcmp(mp, "boom-marker") == 0) verdict |= 8;
        (*env)->ReleaseStringUTFChars(env, msg, mp);
        (*env)->ExceptionClear(env);
        if (!(*env)->ExceptionCheck(env)) verdict |= 16;
    }
    jmethodID sh = (*env)->GetStaticMethodID(env, c, "staticHalf", "(I)I");
    jvalue sv;
    sv.i = 21;
    if ((*env)->CallStaticIntMethodA(env, c, sh, &sv) == 10) verdict |= 32;
    jmethodID bo = (*env)->GetMethodID(env, c, "baseOp", "(I)I");
    jvalue bv;
    bv.i = 5;
    if ((*env)->CallIntMethodA(env, thiz, bo, &bv) == 12) verdict |= 64;
    jclass base = (*env)->FindClass(env, "HarnessBase");
    jmethodID bbo = (*env)->GetMethodID(env, base, "baseOp", "(I)I");
    if ((*env)->CallNonvirtualIntMethodA(env, thiz, base, bbo, &bv) == 1005)
        verdict |= 128;
    return verdict;
}

/* -- methods: the V-forms (inventory §3.3) ------------------------------ */

/* The REAL-variadic arm: the va_list comes from the C compiler's own
   va_start, so the save-area layout Temurin produces is exactly the one
   our walk answers — the same .so, the same ABI, both runtimes. */

static jint vsum_via_va(JNIEnv *env, jobject obj, jmethodID mid, int n, ...) {
    va_list ap;
    va_start(ap, n);
    jint r = (*env)->CallIntMethodV(env, obj, mid, ap);
    va_end(ap);
    return r;
}

static jdouble vmix_via_va(JNIEnv *env, jobject obj, jmethodID mid,
                           int dummy, ...) {
    (void)dummy;
    va_list ap;
    va_start(ap, dummy);
    jdouble r = (*env)->CallDoubleMethodV(env, obj, mid, ap);
    va_end(ap);
    return r;
}

/* The HAND-BUILT arm: the SysV __va_list_tag record + save area filled
   by hand — the deterministic layout of record. The record's fields live
   at the ABI's fixed offsets inside the compiler's own va_list type
   (gp_offset @0, fp_offset @4, overflow_arg_area @8, reg_save_area @16):
   gp words at area[0..48], FP slots 16 bytes apart from area[48]. */

/* Expect 15: 1 = real-variadic vsum (8 ints: register -> overflow split),
   2 = real-variadic vmix (GP/FP interleaved), 4 = hand-built (I I)I,
   8 = hand-built (I D)D. */
static jint probe_calls_v(JNIEnv *env, jobject thiz) {
    jint verdict = 0;
    jclass c = (*env)->GetObjectClass(env, thiz);
    jmethodID vs = (*env)->GetMethodID(env, c, "vsum", "(IIIIIIII)I");
    if (vsum_via_va(env, thiz, vs, 8, 1, 2, 3, 4, 5, 6, 7, 8) == 36)
        verdict |= 1;
    jmethodID mx = (*env)->GetMethodID(env, c, "vmix", "(IDID)D");
    if (vmix_via_va(env, thiz, mx, 0, 1, 2.5, 3, 4.5) == 11.0) verdict |= 2;

    /* hand-built (I I)I: callback(7) -> 21 */
    jmethodID cb = (*env)->GetMethodID(env, c, "callback", "(I)I");
    unsigned char area[176];
    memset(area, 0, sizeof area);
    *(long long *)(area + 0) = 7;
    va_list tag;
    *(unsigned int *)&tag = 0;                     /* gp_offset  */
    ((unsigned int *)&tag)[1] = 48;                /* fp_offset  */
    *(void **)((char *)&tag + 8) = NULL;           /* overflow   */
    *(void **)((char *)&tag + 16) = area;          /* save area  */
    if ((*env)->CallIntMethodV(env, thiz, cb, tag) == 21) verdict |= 4;

    /* hand-built (I D)D: vmix(1, 2.5, 3, 4.5) -> 11.0 — the Int words ride
       the GP slots, the Doubles the 16-byte SSE slots */
    unsigned char area2[176];
    memset(area2, 0, sizeof area2);
    *(long long *)(area2 + 0) = 1;
    *(long long *)(area2 + 8) = 3;
    *(double *)(area2 + 48) = 2.5;
    *(double *)(area2 + 64) = 4.5;
    va_list tag2;
    *(unsigned int *)&tag2 = 0;                    /* gp_offset  */
    ((unsigned int *)&tag2)[1] = 48;               /* fp_offset  */
    *(void **)((char *)&tag2 + 8) = NULL;          /* overflow   */
    *(void **)((char *)&tag2 + 16) = area2;        /* save area  */
    if ((*env)->CallDoubleMethodV(env, thiz, mx, &tag2[0]) == 11.0)
        verdict |= 8;
    return verdict;
}

/* -- the natives round-trip: rebind / restore / unregister (§3.6) ------- */

static jint rebind_impl_two(JNIEnv *env, jclass clazz) {
    (void)env;
    (void)clazz;
    return 777;
}

/* Rebinds nativeVersion -> impl_two; the driver calls nativeVersion
   again and must see 777 (the rebind-override law). Expect 1. */
static jint probe_rebind(JNIEnv *env, jobject thiz) {
    (void)thiz;
    jclass c = (*env)->FindClass(env, "HarnessDriver");
    if (c == NULL) return 0;
    JNINativeMethod m;
    m.name = (char *)"nativeVersion";
    m.signature = (char *)"()I";
    m.fnPtr = (void *)rebind_impl_two;
    return (*env)->RegisterNatives(env, c, &m, 1) == JNI_OK ? 1 : 0;
}

/* Restores the original binding. Expect 1. */
static jint probe_restore(JNIEnv *env, jobject thiz) {
    (void)thiz;
    jclass c = (*env)->FindClass(env, "HarnessDriver");
    if (c == NULL) return 0;
    JNINativeMethod m;
    m.name = (char *)"nativeVersion";
    m.signature = (char *)"()I";
    m.fnPtr = (void *)probe_version;
    return (*env)->RegisterNatives(env, c, &m, 1) == JNI_OK ? 1 : 0;
}

/* Unregisters HarnessHelper; the driver's next helperAdd call must
   throw UnsatisfiedLinkError (caught Java-side; the CLASS NAME prints —
   the message shape is not part of the v1 parity bar). Expect 1. */
static jint probe_unregister_helper(JNIEnv *env, jobject thiz) {
    (void)thiz;
    jclass c = (*env)->FindClass(env, "HarnessHelper");
    if (c == NULL) return 0;
    return (*env)->UnregisterNatives(env, c) == JNI_OK ? 1 : 0;
}

/* The helper's bound native (registered on HarnessHelper in OnLoad). */
static jint helper_add(JNIEnv *env, jobject thiz, jint a, jint b) {
    (void)env;
    (void)thiz;
    return a + b;
}

/* -- the binding skeleton itself (§1: RegisterNatives IS the skeleton) -- */

JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM *vm, void *reserved) {
    (void)reserved;
    JNIEnv *env = NULL;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_8) != JNI_OK ||
        env == NULL)
        return JNI_ERR;
    jclass driver = (*env)->FindClass(env, "HarnessDriver");
    if (driver == NULL) return JNI_ERR;
    jclass helper = (*env)->FindClass(env, "HarnessHelper");
    if (helper == NULL) return JNI_ERR;
    JNINativeMethod d[] = {
        {(char *)"nativeVersion", (char *)"()I", (void *)probe_version},
        {(char *)"probeFields", (char *)"()J", (void *)probe_fields},
        {(char *)"probeStrings",
         (char *)"(Ljava/lang/String;Ljava/lang/String;)I",
         (void *)probe_strings},
        {(char *)"probeStringBack", (char *)"()Ljava/lang/String;",
         (void *)probe_string_back},
        {(char *)"probeUtf16", (char *)"()Ljava/lang/String;",
         (void *)probe_utf16},
        {(char *)"probeArrays", (char *)"()I", (void *)probe_arrays},
        {(char *)"probeObjArray", (char *)"()[Ljava/lang/String;",
         (void *)probe_obj_array},
        {(char *)"probeCallsA", (char *)"()I", (void *)probe_calls_a},
        {(char *)"probeCallsV", (char *)"()I", (void *)probe_calls_v},
        {(char *)"probeRebind", (char *)"()I", (void *)probe_rebind},
        {(char *)"probeRestore", (char *)"()I", (void *)probe_restore},
        {(char *)"probeUnregisterHelper", (char *)"()I",
         (void *)probe_unregister_helper},
    };
    if ((*env)->RegisterNatives(env, driver, d, 12) != JNI_OK)
        return JNI_ERR;
    JNINativeMethod h[] = {
        {(char *)"helperAdd", (char *)"(II)I", (void *)helper_add},
    };
    if ((*env)->RegisterNatives(env, helper, h, 1) != JNI_OK)
        return JNI_ERR;
    return JNI_VERSION_1_8;
}
