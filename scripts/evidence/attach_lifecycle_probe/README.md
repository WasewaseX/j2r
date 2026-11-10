# Attach-lifecycle measurement probe (T13 13d evidence)

MEASURED on the oracle — Temurin 21.0.13+9, live run 2026-11-10 (the
project law: the oracle's measured behavior is the contract; never
assume). Each pair (`Att*.java` + `Att*.c`) builds with:

```console
$ javac Att.java && cc -O1 -shared -fPIC -I$JAVA_HOME/include \
    -I$JAVA_HOME/include/linux Att.c -o libatt.so
$ LD_LIBRARY_PATH=. java -Djava.library.path=. Att
```

## Run 1 — `Att` (the verdict battery): printed V=991

Bit arms (all measured): pre-attach GetEnv == JNI_EDETACHED; attach OK;
post-attach GetEnv OK with the SAME env; re-attach of an attached
thread OK (idempotent); detach OK; **double detach == JNI_ERR is
FALSE**; re-attach OK; the re-attached env works (FindClass +
GetVersion 0x00150000); AsDaemon on an attached thread OK; final
detach OK.

## Run 2 — `Att2` (the exact codes): printed V=-200

The verdict decodes uniquely as: d1=d2=d3=**JNI_OK** (three consecutive
detaches of the same thread — the oracle TOLERATES extra detaches, it
never answers JNI_ERR), ge=**JNI_EDETACHED** (GetEnv after the detach),
a2=**JNI_OK** (re-attach), d4=**JNI_OK**. Encoding: d1*100000 +
d2*10000 + d3*1000 + ge*100 + a2*10 + d4.

## The law these measurements pinned

* fresh/never-attached thread: VM GetEnv answers JNI_EDETACHED
* AttachCurrentThread / AttachCurrentThreadAsDaemon: JNI_OK, env
  non-null; re-attach idempotent
* DetachCurrentThread: JNI_OK **always** — including detaches of an
  already-detached thread (measured three in a row)
* GetEnv after detach: JNI_EDETACHED again
* the main thread is attached at birth (the 13c battery's GetEnv arm)

`j2r`'s VM-table slots implement exactly this measured contract
(`crates/j2r-runtime/src/jni/attach.rs` + `tables.rs`); the C harness's
`probe_attach_lifecycle` battery asserts the same arms through the env
table on BOTH runtimes (`tests/jni_harness/harness.c`, expected verdict
2047) — three-channel parity vs Temurin, `tests/jni_parity.rs`.
