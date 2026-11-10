#include <jni.h>
#include <stdio.h>
#include <pthread.h>

static JavaVM *g_vm;

/* Runs on a fresh (never-attached) pthread: records the measured codes. */
static void *worker(void *arg) {
    int *v = (int *)arg;
    void *penv = NULL;
    /* 1: pre-attach GetEnv answers EDETACHED (-2) */
    jint ge = (*g_vm)->GetEnv(g_vm, &penv, JNI_VERSION_1_8);
    if (ge == JNI_EDETACHED) *v |= 1;
    /* 2: attach OK */
    JNIEnv *env = NULL;
    if ((*g_vm)->AttachCurrentThread(g_vm, (void **)&env, NULL) == JNI_OK && env != NULL)
        *v |= 2;
    /* 4: post-attach GetEnv answers OK and hands back the SAME env */
    penv = NULL;
    if ((*g_vm)->GetEnv(g_vm, &penv, JNI_VERSION_1_8) == JNI_OK && penv == (void *)env)
        *v |= 4;
    /* 8: re-attach while attached still answers OK (idempotent?) */
    JNIEnv *env2 = NULL;
    jint ra = (*g_vm)->AttachCurrentThread(g_vm, (void **)&env2, NULL);
    if (ra == JNI_OK) *v |= 8;
    /* 16: detach OK */
    if ((*g_vm)->DetachCurrentThread(g_vm) == JNI_OK) *v |= 16;
    /* 32: double detach — measured code */
    jint dd = (*g_vm)->DetachCurrentThread(g_vm);
    if (dd == JNI_ERR) *v |= 32;
    /* 64: re-attach OK, battery-capable env */
    env = NULL;
    if ((*g_vm)->AttachCurrentThread(g_vm, (void **)&env, NULL) == JNI_OK && env != NULL) {
        *v |= 64;
        /* 128: the re-attached env actually works (FindClass + GetVersion) */
        jclass c = (*env)->FindClass(env, "java/lang/String");
        if (c != NULL && (*env)->GetVersion(env) == 0x00150000) *v |= 128;
        /* 256: AsDaemon on an already-attached thread — measured */
        JNIEnv *denv = NULL;
        if ((*g_vm)->AttachCurrentThreadAsDaemon(g_vm, (void **)&denv, NULL) == JNI_OK) *v |= 256;
        /* 512: detach at the end of the second cycle OK */
        if ((*g_vm)->DetachCurrentThread(g_vm) == JNI_OK) *v |= 512;
    }
    return NULL;
}

JNIEXPORT jint JNICALL Java_Att_probe(JNIEnv *env, jclass clazz) {
    (void)clazz;
    if ((*env)->GetJavaVM(env, &g_vm) != JNI_OK) return -1;
    int v = 0;
    pthread_t t;
    if (pthread_create(&t, NULL, worker, &v) != 0) return -2;
    pthread_join(t, NULL);
    return (jint)v;
}
