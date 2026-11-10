# J2R — Strategic Gap Register (R1–R5)

Registered in response to external review of the roadmap: five structural concerns that
are real, acknowledged, and given design answers with acceptance bars. **Nothing in this
document claims these gaps are closed.** Each entry states the honest current state, the
design that closes it, the milestone that owns it, and the evidence bar that must be met
before the word "supported" may be used. Cross-references: `docs/AUDIT_VERIFICATION.md`
(external audit response, M3.5), `ROADMAP.md` (M3.6 registration), `docs/M4_PLAN.md`.

Iron rule restated: no claim of Minecraft compatibility, mod compatibility, or
performance is made anywhere in this repository without milestone evidence.

---

## R1 — Mixins vs. the static binary

**Current state.** Not started. No mixin support of any kind exists today.

**The tension, stated honestly.** Fabric/Forge mixins work by transforming target
classes at classload time — the transformation is a runtime bytecode rewrite, and the
set of applied transformations is only fully known once every mod in the instance has
been assembled. A static AOT compiler bakes behavior into a binary at compile time.
Runtime class transformation and a static binary are enemies by default.

**Design answer: move transformation earlier, not later.**

1. **Compile-time mixin application.** Mod assembly resolves the complete mixin set
   (from `fabric.mod.json` / mods.toml mixin configs) *before* codegen. The mixin
   applier operates on j2r's own classfile graph — we already own a reader, a writer,
   and a Code attribute parser, so the applier rewrites method bodies at the classfile
   level. Codegen then compiles the *transformed* classes. Closed world: every mixin
   target must be inside the compilation set, which production mods already satisfy
   because targets are named in the mixin config.
2. **Injection-point model.** Mixin's `@Inject`, `@Redirect`, `@ModifyArg`,
   `@ModifyVariable`, `@Accessor`, `@Invoker` reduce to a small, enumerable set of
   bytecode rewrite patterns. Each becomes a tested rewrite rule in the applier with
   its own differential fixture.
3. **Explicit non-goals (v1).** No runtime bytecode rewriting. No runtime class
   generation. No javaagent-style instrumentation. If a mod computes mixin targets at
   runtime, that is unsupported and fails loud with a named error — tracked, listed,
   never silently miscompiled.
4. **Evidence bar.** The applier is a separate pipeline stage with differential
   fixtures: apply a mixin, then compare observable behavior of the transformed class
   under Temurin (real Fabric runtime) vs. the j2r binary. Acceptance for M5: a real
   Fabric mod containing at least three mixins across `@Inject`/`@Redirect`/`@Accessor`
   behaves identically on both sides.

**Open question, flagged not solved.** Mixin's engine assumes classloader delegation
and launch-wrapper transformation timing. Our pipeline must define class-identity
equivalence before M5 design closes; this is recorded as an M5 design-phase
prerequisite, not as solved.

---

## R2 — Reflection: static subset vs. dynamic

**Current state.** A static subset only. No dynamic class loading, no runtime class
creation, no `ClassLoader.defineClass`.

**The gap, stated honestly.** Java reflection over runtime-computed names
(`Class.forName(name)` where `name` is data), `getDeclaredMethods()` over classes that
were never compiled in, `java.lang.reflect.Proxy`, and `MethodHandle` are all things
real software — including Minecraft itself — uses. A static compiler cannot answer a
question about a class it never saw.

**Design answer: the GraalVM native-image playbook, adapted to our object model.**

1. **Compile-time reflection metadata.** For every class in the compilation set, emit
   a metadata table: name → class id, field list with modifiers, method/constructor
   signatures, retained annotations. The generated binary answers all reflection
   queries from this table. No runtime classloading of unknown classes — ever.
2. **`Class.forName` semantics.** Resolved against the metadata table. Unknown name →
   loud, named `J2RUnsupportedError` naming the exact API and name string. Optional
   `--reflection=all` mode emits metadata for every classfile on the input classpath
   even if unreferenced by code — Minecraft reflects over itself heavily (mod scanning,
   config objects, registries), so this mode is expected to be mandatory for our
   milestone runs, and the metadata size cost is accepted.
3. **Dynamic proxies.** `Proxy.newProxyInstance` call sites with statically known
   interface lists (the common case) get a compile-time-generated proxy class; unknown
   lists fail loud. Proxy method dispatch routes through the same metadata tables.
4. **MethodHandle / LambdaMetafactory.** Lambdas already compile to static shims in
   codegen; the explicit `MethodHandle` API surface is a separate tracked subset, not
   part of the reflection metadata work, and nothing may claim it without its own
   fixtures.
5. **Inventory discipline.** The supported reflection API set lives in a support
   matrix document, is CI-checked against the runtime's dispatch table, and every
   corpus call site either works or fails loud with the API name. A silent wrong
   answer is a bug of the same severity as a miscompile.

**Evidence bar.** A reflection matrix fixture suite: every supported API has a
differential test vs. Temurin; every unsupported API has a test asserting the loud
error. No middle states.

---

## R3 — JNI

**Current state.** Not started. Zero JNI surface exists.

**Why this is non-negotiable.** LWJGL — GLFW, OpenGL, OpenAL, stb — *is* JNI. No JNI
means no window, no rendering, no audio, no Minecraft. This is an M5 blocker, not a
nice-to-have.

**Design answer: emulate the JNIEnv vtable exactly.**

1. **Vtable layout fidelity.** `j2r-runtime` gains a `jni` module implementing the
   `JNIEnv` and `JNI_InvokeInterface` tables as static tables of `extern "system"`
   function pointers, index-for-index identical to `jni.h` ordering, because native
   libraries index the table positionally. Any drift here is a segfault in someone
   else's binary, so the layout is pinned by a compile-time assertion table.
2. **Object model bridge.** `jobject` = opaque handle into our GC handle table. The
   M2 generation-tagged handle design already models local and global refs; JNI local
   refs map to frame-pushed handles, `Push/PopLocalFrame` maps to frame pop, global
   refs to the pin set. Non-moving arena collection makes `GetPrimitiveArrayCritical`
   pinning trivial by comparison to a moving collector — an explicit design advantage,
   documented, not an accident.
3. **v1 surface.** `GetObjectClass`, `FindClass`, field ID resolution + accessors,
   the `Call*Method`/`CallStatic*Method` families (arity-shimmed; no libffi in v1),
   `RegisterNatives` (mods binding their own natives), string marshalling with correct
   UTF-8/modified-UTF-8 handling, array families, local/global ref management.
   Explicitly out of v1: `sun.misc.Unsafe` native paths, JVMTI, JIT shortcuts.
4. **Evidence bar.** A C-compiled test `.so` exercising every v1 function, run against
   both Temurin and the j2r binary, with identical observable results required. This
   harness gates the LWJGL bring-up milestone.

**Open question, flagged not solved.** Variadic method calls across the JNI boundary
have ABI subtleties per platform; the arity-shim approach avoids libffi but inflates
surface area. Decision deferred to the M5 JNI wave with a written trade-off note.

---

## R4 — Threads + memory ordering

**Current state.** Single-threaded execution only. `Thread` exists as a type; no
scheduler, no monitors, no atomics mapping.

**The gap, stated honestly.** The Java Memory Model must map onto Rust's atomics, and
three-channel stdout/stderr/exit differential testing — our truth spine — is blind to
races. A wrong ordering compiles fine, passes every deterministic diff, and explodes
as a heisenbug under load. This is the gap where quiet wrongness lives.

**Design answer: a written JMM→Rust mapping, enforced by litmus tests.**

1. **The mapping table** (`docs/JMM_IN_RUST.md`, its own deliverable):
   - `volatile` read/write → `Atomic` load/store, `Acquire`/`Release` minimum, never `Relaxed`.
   - `volatile long/double` → atomics; no tearing, full stop.
   - `final` field freeze → `Release` fence at constructor exit (JMM §17.5 semantics).
   - Default zero-initialization happens-before publication of the reference.
   - `synchronized` → real monitor (mutex + condvar), never a spinlock.
   - `wait/notify/notifyAll` → condvar on the monitor's mutex, with Java's
     interrupt-and-spurious-wakeup loop semantics.
   - `park/unpark` → permit-based parker (futex underneath), not a condvar.
   - `Thread.start()`/`join()` → explicit happens-before edges (`join` = acquire of
     the thread's writes).
2. **Safepoints.** Cooperative polling at loop back-edges and call returns: a relaxed
   atomic load of a per-thread flag plus a branch. The handler performs the fencing.
   Needed for GC stop-the-world and for any future deferred-transformation path (R1).
3. **Racy programs keep Java-defined behavior.** Unsanctioned data races in Java have
   defined semantics (no out-of-thin-air values). Our approximation: all shared mutable
   Java field access compiles to atomics or monitor-guarded access; the documented
   approximation is the Acquire/Release mapping, validated with a litmus suite (SB,
   CoRR, IRIW, message passing) run under gate-9-style stress repetition. Litmus tests
   assert the *allowed outcome set*, never a single outcome — a racy program cannot be
   diffed byte-exact and we will not pretend otherwise.
4. **Deterministic differential fixtures.** Join-visibility, volatile handshake,
   monitor ownership, executor ordering — deterministic by construction, diffed
   exactly on all three channels, added to the standing suite.
5. **Honest limit.** Full JMM causality (the draconian cases) is research-grade. We
   implement the operational core (volatile, final, monitor, start/join, park) and the
   docs say exactly that, with the approximation named.

**Evidence bar.** Mapping table merged; litmus suite green under stress repetition;
concurrency differential fixtures in the standing suite; none of this counts until
`cargo test` evidence exists in the commit.

---

## R5 — Performance and correctness >> no benchmarks

**Policy, in writing.**

1. **Correctness gates everything, forever.** No optimization lands unless the
   differential suite is unchanged-green on the same commit.
2. **No performance claim exists today.** The README makes no speed statement, and it
   stays that way until a harness produces numbers. Any future performance statement
   requires: the benchmark commit, the environment disclosure (CPU, RAM, JDK build),
   and both sides' raw outputs checked in as data files.
3. **Benchmark skeleton, deliberately deferred** until after the real-world corpus
   wave (the correctness backlog outranks it): hyperfine wall-clock over the corpus
   (j2r binary vs. `java` with TieredStopAtLevel=1 and with default C2), criterion
   micro-benches for inflate, CRC32, classfile parse, and codegen emit. Benchmarks are
   **never CI gates** — they observe, they do not certify.
4. **What we will not do:** optimize before the M4 correctness waves land; tune for
   the benchmark set instead of real programs; publish a number without its
   environment.

---

## Register summary

| ID | Concern | Honest state today | Owner milestone | Status |
|----|---------|--------------------|-----------------|--------|
| R1 | Mixins vs static binary | Not started | M5 design phase | Registered |
| R2 | Reflection subset | Static subset, no metadata gen | M4 surface + M5 | Registered |
| R3 | JNI | Not started | M5 blocker | Registered |
| R4 | Threads + memory ordering | Single-threaded only | M4 concurrency wave | Registered |
| R5 | Benchmarks | None; policy set | Post-corpus wave | Policy locked |

---

**Addendum (2026-11-10, the doc-reconciliation pass — the per-section
"Current state" texts and the table above are kept verbatim as their
dated registration records; this addendum is the live pointer):**

* **R3 — CLOSED at the v1 evidence bar.** T08–T13 complete: the JNIEnv
  table emulation (235 slots, 162 real), the audited `j2r-jni-ffi`
  crate (12 primitives, 11 `unsafe` occurrences of pin 12), the ONE C
  harness `.so` with three-channel parity vs Temurin both directions,
  and the MEASURED attach lifecycle — REVIEW_CHECKLIST G1.1–G1.6 all
  checked, gate-22 pins the parity in CI (docs/M5_PLAN.md §3,
  docs/M5_JNI_HARNESS.md; the §R3.4 evidence bar — "a C-compiled test
  `.so` exercising every v1 function, run against both Temurin and the
  j2r binary, with identical observable results" — is met and
  CI-pinned). LWJGL bring-up itself remains gated on the
  REVIEW_CHECKLIST ordering (G2 next; ecosystem work G5.4).
* **R4 — waves LANDED, owner review pending.** All four M4 waves
  implemented and CI-verified (real threads, monitors/wait-notify,
  interrupt + daemon shutdown, JMM-lite + stress; gate-13). The memory
  model is documented, not faked: docs/RUNTIME_SAFETY.md §6 is the
  live contract (no volatile, no final-field freeze — the boundary
  G4.2 will enforce). docs/M4_EVIDENCE.md §completion package awaits
  the owner verdict.
* **R2 — unchanged** (static subset, no metadata generator; the
  generator is registered post-M4 scope in TODO.md).
* **R1 — unchanged** (not started: T17–T21; the design answer and the
  flagged class-identity question stand as written; no rewrite rule
  before T17).
* **R5 — policy unchanged** (benchmarks observe, never gate; the
  harness landed 2026-11-05 and the steady-state rows carry their
  protocols in docs/BENCHMARKS.md).
