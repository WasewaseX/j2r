# M5 wave 2 — T13: the C harness `.so`, the attach lifecycle, and
# primitive 11 (design note)

Status: DESIGN OF RECORD for T13 (committed before any code — the
12a/11a pattern; §9 records the 13d precision of record — T13 is
COMPLETE and wave 2 is CLOSED with it). Read with M5_PLAN §3 (the
acceptance: "a C-compiled test `.so` exercising every v1 function, run
against Temurin and the j2r binary with identical observable results,
three channels; `RegisterNatives` round-trip demonstrated; the JNI
suite pinned in CI — this harness gates all LWJGL bring-up"),
M5_JNI_CALLS.md §3 (decision 3 registers this note), M5_JNI_NATIVES.md
§2/§7 (primitive 11 and the frame-level parity pin) and M5_JNI_FFI.md
(the audited primitive surface, 10 today).

## 1. The parity shape, stated exactly

One C harness (`tests/jni_harness/harness.c` → `libjni_harness.so`)
plus one Java driver exercise the v1 surface on BOTH runtimes:

- **Temurin side**: `System.loadLibrary("jni_harness")`; the library's
  `JNI_OnLoad` (or a static initializer) calls `RegisterNatives` to
  bind its C natives; the driver then invokes each bound native, which
  calls back through the `JNIEnv` it received — every v1 function in
  the inventory (§3) — and returns Java-visible results the driver
  prints.
- **j2r side**: the SAME fixture is compiled by j2r. The generated
  program binds the same C natives through slot 215 (the 12b path)
  and, when a bound native runs, primitive 11 calls the C fn; the C
  fn calls back through OUR generated env table. The driver prints
  the same lines.
- **Three channels**: stdout, stderr and exit are compared raw
  (the frame-normalization law covers the trace bodies; the T12 12c
  e2e already proved header-level parity end to end).

The harness is ONE C source compiled once and run against both — no
side-specific code paths beyond the loading mechanism itself.

## 2. Primitive 11 — `call_foreign_native` (the foreign-call thunk)

Calling a bare-form C native (`jint f(JNIEnv*, jobject, jint, jint)`)
from a `forbid(unsafe_code)` runtime is the last of the three
variadic-ABI questions (M5_JNI_CALLS.md §2 answered the INBOUND ones;
this is the OUTBOUND one). The audited-crate answer, honestly scoped:

- ONE thunk: `call_foreign_native(ptr, env, recv, gp_words, fp_words,
  gp_len, fp_len)` — transmutes `ptr` to the FIXED SysV max-register
  signature `(env, recv, usize × 4, f64 × 8)` and calls it. On
  x86-64 SysV the first 6 integer-class args land in
  rdi/rsi/rdx/rcx/r8/r9 (env + recv consume the first two) and the
  SSE-class args land in xmm0–7 in order — so a bare C native whose
  varargs fit 4 GP + 8 FP register words is called CORRECTLY,
  descriptor-driven: the A-form decode classifies each parameter
  (the marshalling law is shared with the `Call*MethodA` core) and
  fills the word arrays.
- **Beyond the window = LOUD** (7+ GP words would need stack
  construction — libffi's exact problem, still banned by both
  standing laws). The abort names the descriptor and the window law;
  the harness fixtures stay inside the window (documented per
  fixture).
- The gate-10 count is amended IN THE SAME COMMIT (T09/11c precedent;
  9 → 10 of pin 12).
- The 12b `Binding::Foreign` loud abort becomes THIS call — the
  `as_method_fn` panic disappears and the walk consult gains the
  working foreign path.

## 3. The v1 function inventory the harness exercises

Everything real today (158 of 231 slots), driven from C:

- versioning/VM: `GetVersion`, VM `GetEnv` (the supported-version path
  is F-J1-pinned — the harness asserts the JNI_ERR/`JNI_EVERSION`
  paths it CAN serve); `GetJavaVM` (219).
- classes/fields: `FindClass`, `GetObjectClass`, `GetFieldID`,
  `GetStaticFieldID` + the accessor families on a fixture object.
- methods: `GetMethodID`/`GetStaticMethodID` + the `Call*MethodA`
  families (the workhorse form; the V forms from C via a hand-built
  `va_list` — one fixture records the save-area layout Temurin
  produces and asserts our walk answers identically); `Exception*`
  trio for the throw round-trip.
- strings: the five modified-UTF-8 slots both directions (the fixture
  includes the mUTF-8 edge bytes: encoded NUL, supplementary pair).
- arrays: `New*Array`/`Get*ArrayElements`/`Release*ArrayElements`
  (all three modes)/regions/criticals on a small fixture of each
  primitive kind + an object array.
- natives: `RegisterNatives`/`UnregisterNatives` (the binding round
  trip IS the harness skeleton — §1).
- attach lifecycle: the library spawns a plain pthread that calls
  `AttachCurrentThread` on the cached `JavaVM` (from `GetJavaVM`),
  runs the same assertion battery on the attached env, then
  `DetachCurrentThread` — on our side this exercises the 12b-wired
  `detach_thread` hook and the frame-stack lifecycle end to end. The
  v1 registered gaps stay honest in the harness output: weak global
  refs and cross-thread local-resolve are loud/documented, so the
  harness asserts the LOUD behavior rather than pretending.

## 4. CI pinning

A new CI gate compiles the harness with cc, runs the Temurin driver
and the j2r binary, and compares the three channels — the wave-2
acceptance gate. Runs only where a JDK + toolchain exist (the
existing CI image qualifies); a missing JDK skips loudly in local
runs (the soe_e2e law).

## 5. Evidence bar (acceptance)

The harness battery green on both runtimes, three channels, raw
bytes; the attach-lifecycle assertions (attach → battery → detach →
re-attach) identical; `RegisterNatives` round-trip with a REAL C
native demonstrated on both sides (the M5_PLAN acceptance sentence,
now fully honest); gate-10 amended in the same commit as primitive
11; fmt, clippy `-D warnings`, doc-count sync, full battery, boards
+ push.

## 6. Slice order

- **13a** — this note.
- **13b** — primitive 11 (`call_foreign_native`) + the gate-10
  amendment in the same commit + the `Binding::Foreign` path switch +
  unit tests (a C fn compiled into the test binary — the first C in
  the repo, built only for tests). **LANDED, one precision of record
  vs this note:** the C probe compiles via the crate's own `build.rs`
  invoking the PLATFORM C compiler directly (`$CC`/`cc`), NOT the `cc`
  crate — a `[build-dependencies]` entry would enter the vendored
  manifest (every member manifest is copied verbatim into generated
  projects) and break their `--offline` builds, and gate-5 outranks
  the convenience. The probe object links into the package's TEST
  binaries only (`cargo:rustc-link-arg-tests`); the library target
  never references the symbols, build.rs itself is never vendored, and
  a compiler-less box cfgs the real-C tests out (the Rust `extern "C"`
  probes still cover the window math). The full runtime path
  (`call_native` → `invoke_binding` → `foreign_call` → primitive 11)
  is pinned, including the register-window loudness (5 GP words →
  panic BEFORE the fnPtr word is used). 444 green, full battery
  (diff_suite 454.98 s + cfg_equiv 353.99 s) zero divergences.
- **13c** — the harness C source + the Temurin driver + the j2r-side
  loading story + the three-channel comparison runner. **LANDED
  (2026-11-10, session 7)** — see §7 (the loading story, precision of
  record) and §8 (what the battery exercises and what it honestly
  does not). 448 green, full battery (diff_suite 464.16 s +
  cfg_equiv 347.11 s) zero divergences.
- **13d** — the attach lifecycle battery + the CI gate + the full
  battery re-run + boards. **LANDED (2026-11-10, session 8)** — §9 is
  the precision of record; wave 2 CLOSES with 13d.

## 7. The loading story (13c, precision of record)

How the SAME `.so` comes to run against BOTH runtimes — the one piece
this note deliberately left open:

1. **The VM-out slots are real.** VM-table `GetEnv` (slot 6) now serves
   its supported-version path: the supported set is `{1.1, 1.2, 1.4,
   1.6, 1.8, 9, 10}` — the exact set HotSpot 21 accepts (measured live
   by the harness; 0x000b0000 and 0 answer JNI_EVERSION, and nothing is
   written on those paths) — and writes the env table pointer through
   audited primitive 3. `GetJavaVM` (env slot 219) is wired the same
   way. Real slots: 158 → 159. The old "supported version aborts" pin
   (F-J1's last unwired arm) is replaced by success-path tests.
2. **`System.loadLibrary` is a runtime native** (`java/lang/System`,
   `(Ljava/lang/String;)V`): it maps the name to the platform filename
   (`lib<name>.so` — Linux-only in v1, loud unsupported elsewhere),
   then calls **primitive 12** `load_foreign_jni_library(path, vm)` in
   `j2r-jni-ffi`: ONE audited unsafe block doing dlopen (RTLD_NOW |
   RTLD_LOCAL) + dlsym("JNI_OnLoad") + the ABI-fixed `jint (*)(void*,
   void*)` call. The handle is NEVER dlclosed (a JNI library lives for
   the process — HotSpot semantics; a close would invalidate the
   bindings the natives registry holds). A dlopen miss aborts LOUDLY
   naming the loader's error text (the UnsatisfiedLinkError-on-load-
   failure transport stays a registered v1 gap; the primitive's own
   tests pin the loudness). `JNI_OnLoad`'s version answer is validated
   safe-side: 0 (no symbol) or a supported version = success; anything
   else = `UnsatisfiedLinkError` (the oracle's family).
3. **gate-10 amended in the same commit**: 10 → 11 `unsafe`
   occurrences of pin 12 (G1.6's tracked headroom: one slot left).
4. **GetVersion corrected to the MEASURED oracle answer.** T08 shipped
   `GetVersion = JNI_VERSION_1_8` as an assumption. The harness's very
   first oracle run falsified it: Temurin 21.0.13+9 answers the
   JDK-major form **0x0015_0000** (probed live, Ver.c + ver.java, the
   experiment committed to the session log). Our slot now returns the
   measured value (`JNI_VERSION_21_0`), because the three-channel
   differential contract is against THIS oracle, byte for byte. This
   is the harness working exactly as designed: it judges, not decorates.
5. **Harness JNI hygiene (a real oracle crash taught it):** a
   `jclass` captured in `JNI_OnLoad` is a LOCAL ref that dies when
   OnLoad returns — the first rebind attempt used the stale jclass and
   HotSpot **SIGSEGVed** in `jni_RegisterNatives` (our interned-mirror
   tags happened to tolerate it — tolerance is not correctness). The
   harness now holds NO class statics: every rebind/unregister probe
   re-FindClasses a fresh local and uses it within its own frame.
   `NewGlobalRef` (21) stays a loud stub — pinned by a 13d loud-gap
   child-process test; the harness battery needs no global refs (every
   probe stays within its own thread and frame).
6. **The parity runner** (`crates/j2r-cli/tests/jni_parity.rs`)
   compiles the `.so` ONCE (`cc -shared -fPIC` against the pinned
   JDK's `jni.h` of record), javac's the ONE driver source, runs
   Temurin (`-Djava.library.path` + `LD_LIBRARY_PATH`), runs the j2r
   binary (`LD_LIBRARY_PATH` — the same dlopen search), and asserts
   stdout + stderr + exit RAW-identical, with stderr expected EMPTY on
   both sides. Skips loudly (the soe_e2e law) when no JDK or no C
   compiler is present. The CI gate wiring is 13d.

## 8. What the battery exercises (G1.2, the honest inventory)

`tests/jni_harness/harness.c` — ONE source, 13 bound probes (12 from
13c + the 13d attach-lifecycle probe) + the helper native, all
self-checking (the driver's `check()` exits 3 on
ANY missed verdict, on BOTH runtimes — a broken probe can never
"preserve parity" with a wrong answer):

* versioning/VM: GetVersion (the measured 0x0015_0000), GetJavaVM →
  cached VM → the GetEnv matrix (1.8 OK; "11" EVERSION; the served env
  IS the received env).
* classes/fields: GetObjectClass, GetFieldID/GetIntField/SetIntField,
  GetStaticFieldID/GetStaticLongField/SetStaticLongField — verified
  Java-side after the call (41→42, 1000→2000).
* methods: GetMethodID/GetStaticMethodID; CallIntMethodA (virtual —
  the override), CallObjectMethodA (returned string re-read through
  GetStringUTFChars), CallVoidMethodA (the callback PRINTS — stdout
  flows Java-side), CallStaticIntMethodA, CallNonvirtualIntMethodA
  (the DECLARED-class dispatch: Base's 1005 vs the override's 12);
  the exception round-trip (boom() → zero return → ExceptionOccurred →
  the message read through a COMPILED bridge method — a builtin class
  has no compiled facts, so GetMethodID on it would honestly answer
  null → ExceptionClear → ExceptionCheck false).
* V-forms: REAL-variadic C helpers (the save area the C compiler
  itself produces — 8 ints pin the register→overflow split; (I D)D
  pins the GP/FP interleave) AND hand-built `__va_list_tag` records
  (the deterministic layout of record, filled at the ABI's own
  offsets).
* strings both directions: GetStringUTFLength/GetStringUTFChars/
  ReleaseStringUTFChars over the mUTF-8 edges (the encoded NUL
  0xC0 0x80 and the supplementary pair), NewStringUTF ("j2r" + NUL +
  "ok"), NewString (surrogate pair + an embedded NUL jchar) — all
  verified by byte-compare in C and by Java-side length/charAt.
* arrays: NewIntArray + Set/Get regions, GetIntArrayElements + ALL
  THREE release modes, GetPrimitiveArrayCritical/Release, GetArrayLength.
  HONEST LIMIT (in-comment): the JNI_ABORT arm CALLS the path but
  asserts only the COMMIT result — with a direct-pointer Get
  (HotSpot) the mutation is already in the array, so asserting the
  ABORT-drops semantics would diverge BY DESIGN; that law is
  unit-pinned Rust-side (T10 slice 3b, copy-mode registry).
* natives: RegisterNatives/UnregisterNatives — the binding IS the
  skeleton (JNI_OnLoad binds all 12 + the helper); the rebind-override
  law (nativeVersion → 777 → restored 47) and the unbound law
  (UnregisterNatives → UnsatisfiedLinkError, caught Java-side, the
  CLASS NAME printed — the message shape is not in the v1 bar).

Registered NOT exercised (still loud/documented v1 gaps): weak global
refs, cross-thread local-resolve, Monitor*, GetStringRegion/
GetStringUTFRegion, the string/array exception transports. 13d's
loud-gap probes pin the loud arms Rust-side (child-process aborts
naming the slot: NewGlobalRef, MonitorEnter, MonitorExit,
GetStringUTFRegion). Precision correction of record: "InvokeInterface"
was named here as a v1 gap — the Temurin 21 jni.h has NO such function
(it was dropped from the header); the loud v1 gaps above are the
header's real slots, mechanically verified.

## 9. The attach lifecycle (13d, precision of record)

**The MEASURED oracle contract first** (the project law — the harness
judges, not decorates): the standalone C probe
(`scripts/evidence/attach_lifecycle_probe/`, Temurin 21.0.13+9 live,
2026-11-10) recorded every arm:

* a fresh (never-attached) thread's VM `GetEnv` answers
  **JNI_EDETACHED**; after a detach it answers EDETACHED again;
* `AttachCurrentThread` answers **JNI_OK** (env non-null); a re-attach
  of an already-attached thread answers **JNI_OK** (idempotent);
  `AttachCurrentThreadAsDaemon` on an attached thread answers **JNI_OK**;
* `DetachCurrentThread` answers **JNI_OK ALWAYS** — measured THREE
  consecutive detaches all JNI_OK: the oracle tolerates extra detaches,
  it never answers JNI_ERR (the probe's raw verdicts: V=991, V=-200 —
  both committed with their decode);
* the main thread is attached at birth (the 13c battery's GetEnv arm
  proves it on the oracle).

**The slots' fate (T13 13d):** VM `AttachCurrentThread` (4),
`DetachCurrentThread` (5) and `AttachCurrentThreadAsDaemon` (7) are now
REAL (159 → **162 real slots**); `DestroyJavaVM` (3) stays a loud stub
(no generated program ever destroys the VM; honestly registered). The
registry (`jni::attach`) is one global set of attached thread ids;
detach runs the 12b-wired `bridge::detach_thread` hook — the frame-
stack sweep is the end-to-end lifecycle proof G1.4 names. `GetEnv`
became attach-aware (EDETACHED on a never-attached thread, in that
check order: null-penv JNI_ERR → EVERSION → EDETACHED). Daemon-ness
has no v1 representation (no VM-exit join exists); documented, not
pretended. The generated `main_run` marks the main thread attached at
birth (one emission next to `begin_main_java_thread`).

**The battery (harness `probe_attach_lifecycle`, G1.4):** ONE pthread,
spawned and JOINED by the probe (deterministic stdout), walks every
measured arm and runs a compact self-checking battery ON the attached
env twice (GetVersion / FindClass / the NewStringUTF-UTFChars
round-trip / NewIntArray + region + length / ExceptionCheck clean —
expect 31 per cycle). Verdict expected **2047** (eleven bits); the
driver checks it; the SAME `.so` answers 2047 on Temurin (measured
first, standalone) and on j2r (the parity run) — three channels
byte-identical including `P attach=2047`.

**The loud-gap probes:** the registered v1 gaps with a loud arm are
pinned Rust-side (child-process aborts through the REAL env table,
naming the slot): NewGlobalRef (21), MonitorEnter (217), MonitorExit
(218), GetStringUTFRegion (221). The oracle implements these for real,
so they stay OUT of the parity battery by design.

**The CI gate (G1.5):** `gate-22-jni-c-harness-parity` — a dedicated
job that compiles the harness with cc, runs BOTH runtimes, asserts the
three channels raw-identical, and is wired into the all-gates
aggregate (17 upstream gates). A missing JDK/compiler skips LOUDLY in
local runs (the soe_e2e law); the CI image has both, so a skip there
is a visible anomaly, not a silent green.

**Evidence:** 452 tests, 0 failures; the parity battery green with the
attach arm; gate-11 in sync (fixtures=109, tests=452). Wave 2 (JNI) is
CLOSED — G1.1–G1.6 all carry their evidence.

