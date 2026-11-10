# M5 wave 2 — T13: the C harness `.so`, the attach lifecycle, and
# primitive 11 (design note)

Status: DESIGN OF RECORD for T13 (committed before any code — the
12a/11a pattern). Read with M5_PLAN §3 (the acceptance: "a C-compiled
test `.so` exercising every v1 function, run against Temurin and the
j2r binary with identical observable results, three channels;
`RegisterNatives` round-trip demonstrated; the JNI suite pinned in
CI — this harness gates all LWJGL bring-up"), M5_JNI_CALLS.md §3
(decision 3 registers this note), M5_JNI_NATIVES.md §2/§7 (primitive
11 and the frame-level parity pin) and M5_JNI_FFI.md (the audited
primitive surface, 10 today).

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
  unit tests (a C fn compiled into the test binary via `cc` — the
  first C in the repo, built only for tests).
- **13c** — the harness C source + the Temurin driver + the j2r-side
  loading story + the three-channel comparison runner.
- **13d** — the attach lifecycle battery + the CI gate + the full
  battery re-run + boards.

Wave 2 closes with 13d.
