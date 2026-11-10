#include <jni.h>
#include <pthread.h>
static JavaVM *g_vm;
static void *worker(void *arg) {
    long *v = (long *)arg;
    JNIEnv *env = NULL;
    if ((*g_vm)->AttachCurrentThread(g_vm, (void **)&env, NULL) != JNI_OK) { *v = -100; return NULL; }
    jint d1 = (*g_vm)->DetachCurrentThread(g_vm);
    jint d2 = (*g_vm)->DetachCurrentThread(g_vm);
    jint d3 = (*g_vm)->DetachCurrentThread(g_vm);
    /* GetEnv AFTER full detach — EDETACHED again? */
    void *penv = NULL;
    jint ge = (*g_vm)->GetEnv(g_vm, &penv, JNI_VERSION_1_8);
    /* and a THREE-cycle attach/detach idempotence check */
    JNIEnv *e2 = NULL;
    jint a2 = (*g_vm)->AttachCurrentThread(g_vm, (void **)&e2, NULL);
    jint d4 = (*g_vm)->DetachCurrentThread(g_vm);
    *v = (long)d1 * 100000 + (long)d2 * 10000 + (long)d3 * 1000 + (long)ge * 100 + (long)a2 * 10 + (long)d4;
    return NULL;
}
JNIEXPORT jint JNICALL Java_Att2_probe(JNIEnv *env, jclass clazz) {
    (void)clazz;
    if ((*env)->GetJavaVM(env, &g_vm) != JNI_OK) return -1;
    long v = 0;
    pthread_t t;
    if (pthread_create(&t, NULL, worker, &v) != 0) return -2;
    pthread_join(t, NULL);
    return (jint)v;
}
