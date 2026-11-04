# J2R — Master Checklist (plain text: done vs planned)

The live status board for the whole project. A box is checked ONLY with
runnable evidence (test name, CI gate, fixture, commit). `[~]` = started,
with the gap stated. If a claim and this board disagree, this board loses —
fix the board. Last updated: 2026-11-04.

Project: J2R — Java → Rust compiler, 100% safe Rust (zero `unsafe`).
North star: compile Minecraft Java Edition + mods (Fabric/Forge/NeoForge,
Mixin, LWJGL). Sole contributor: wasewasex.

---

## 1. DONE — foundations

- [x] **M1 core compiler** — .class parser, IR decoder, Rust emitter,
      Java-semantics runtime; classes/inheritance/interfaces, exceptions,
      strings + invokedynamic concat, arrays, statics, natives.
      Differential harness (stdout/stderr/exit vs Temurin), 14 fixtures.
- [x] **M1.5 engineering hardening** — three-channel comparison pinned,
      bytecode/IR verifier (36 unit + 12 malformed e2e tests), u32 ids,
      Rc<RefCell> risk register (docs/RUNTIME_SAFETY.md), loud error for
      synchronized methods.
- [x] **M2 generated-code quality** — catchable StackOverflowError
      (frame registry, hard ceiling, boundary matrix); tracing
      cycle-collecting arena GC (zero heap borrows, stable identity,
      forced-GC rooting proof); typed-storage specialization ended in an
      honest measured NO-GO (docs/M2_SHAPE_REPORT.md); CFG prototype
      behind --emit-cfg with an equivalence gate, default-emission NO-GO
      (docs/M2_CFG_REPORT.md). Suite 14 → 39; 170 tests; 10 gates green.
- [x] **M3 JDK coverage** — classpath & JAR loading (zero-dep
      zip/inflate/crc32); java.util collections (OpenJDK-faithful hash
      order pinned by HashMapOrder); lambdas & method references
      (LambdaMetafactory); synchronized monitors; reflection subset
      (interned Class objects, Field.get/set, Method.invoke,
      Temurin-exact failure strings); java.util.Arrays/Collections
      surface; 5 real multi-class programs (300–500 lines each) joined
      the suite and found the JLS §4.12.5 instance-default bug.
      Suite 39 → 77; 228 tests; 10/10 gates at ce8b44e.
- [x] **M3.5 audit response (A1–A8)** — all 7 external findings verified
      against source (docs/AUDIT_VERIFICATION.md), one commit each:
      A1 harness honesty (Suppressed: lines no longer masked) 2fc965a;
      A2 suppressed-exception machinery + 4 TWR fixtures 9d3a043;
      A3 one main-class selector 5ebbeed; A4 attribute-length
      validation d9713b2; A5+A6 zlib-exact Huffman validation +
      LazyLock CRC 6062d5b; A7 Windows-safe PATH/classpath d209f84;
      A8 machine-checked doc counts (gate-11) 34d5a72.
      The alleged `#acro_export]` corruption was disproven by compiler
      run and byte hashes — tool output alone is not evidence here.
- [x] **M3.6 strategic gap register** — the five structural review
      concerns (mixins vs static binary, static-only reflection, JNI,
      threads + memory ordering, no benchmarks) registered with design
      answers and acceptance bars in docs/STRATEGIC_GAPS.md (828c19f).
- [x] **M3.7 corpus wave 1** — minimal-json (real library, 15 Apache-2.0
      sources) vendored + mechanical assembly script + deterministic
      driver; output-identical to Temurin 21.0.13 on all three channels;
      corpus runner pinned in CI (corpus_suite.rs). Suite 83 → 84,
      256 tests, fixtures=84 tests=256 machine-checked by gate-11.

## 2. DONE — this wave: CI repair (gate-1 + gate-2 + gate-6)

The corpus wave landed with test evidence but without fmt/clippy/doc —
CI went red on three gates. Reproduced locally, all fixed. **CI status at
HEAD `e71442e`: green — all 11 check runs success, including the
all-gates aggregate (verified via the GitHub API check-runs and by a
local full-gate sweep at the same tree).**

- [x] gate-1 fmt — `cargo fmt --all` applied across the corpus wave files
- [x] gate-2 clippy — 5 lints fixed (4× map_or→is_none_or, 1× clone on Copy)
- [x] gate-6 doc — bare `<init>` in a doc comment parsed as HTML; backticked
- [x] gate-10 no-unsafe re-verified (grep + forbid attribute)
- [x] gate-11 doc-counts re-verified (fixtures=84 tests=253)
- [x] gate-3 full re-run — 218 unit tests + codegen e2e/malformed/wide_ids
      + cfg-equivalence 84/84 (556s) + differential 84 fixtures x 3 channels
      vs Temurin (281s) + corpus runner (real minimal-json vs Temurin)
      + lambda/SOE batteries — all green
- [x] gate-4 release build green (RUSTFLAGS -D warnings)
- [x] gate-9 gc-stress — CI-verified green at e71442e (the all-gates
      aggregate only succeeds when gate-9 passes, and it did). Local
      sharded evidence retained: 83/84 fixtures green with
      J2R_DIFF_SHARD=i/N (new harness capability, corpus guard untouched).
      ReflectGC is an O(n^2) torture case under a-collection-at-every-
      allocation (200k live nodes) and exceeds the 10-minute local tool
      cap; the authoritative unfiltered gate-9 run is CI's.

## 3. IN PROGRESS / NEXT (ordered)

- [x] **Gate upgrade package (2026-11-04, "improve the gates")** —
      least-privilege workflow permissions; PR-only cancel-in-progress
      (main keeps every run as the evidence chain); per-job
      `timeout-minutes` everywhere (a hang fails loudly); toolchains
      pinned to the exact local-evidence versions (rust 1.99.1, Temurin
      21.0.13) so CI reproduces local with zero drift; third-party
      actions pinned by commit SHA; `--locked` on every workspace cargo
      invocation; **new gate-12-runtime-seams** (M4_PLAN §2-R3's promised
      CI grep for unannotated `thread_local!`, all four crates, printed
      census — `scripts/check_seams.py`; local run PASS, census = 1
      annotated seam in frame.rs); gate-5 standalone proof upgraded from
      a 34-line probe to **BigRpg** (real 302-line multi-class program,
      8 classes — local evidence: compiled to an isolated crate, built
      `--offline`, all three channels byte-identical vs Temurin,
      exit 0 == 0). M4_PLAN Wave-4 gate renumbered gate-11 → **gate-13**
      (11 and 12 taken), plan rev 1.1. **gate-14-codeql added** (owner
      directive: GitHub Code Quality / CodeQL security analysis — Rust,
      build-mode none, SARIF to the Security tab, SHA-pinned actions,
      weekly schedule; codeql.yml).
- [~] **M4 threads & concurrency — ACCEPTED (owner directive
      2026-11-04, plan rev 1)** — 18-task register at docs/M4_PLAN.md §6.
      **Wave 1 shared substrate LANDED** (T02–T05, commit 4ddfe1b; CI
      aggregate green on that push). **Wave 2 real threads LANDED**
      (T06–T09: Thread natives + registry + STW safepoints + coop
      scheduler; two new real-thread fixtures byte-identical vs Temurin;
      docs/M4_EVIDENCE.md §Wave 2). Next: Wave 3 monitor contention +
      wait/notify (T10–T13). Later: interrupt/daemon/JMM-lite/stress
      (T14–T18, gate-13).

### M4 Wave 2 — real threads (T06–T09) — LANDED, CI pending

- [x] **T06** — `j2r-runtime/src/thread.rs` (new): Thread natives —
      `currentThread`, `start`, `join`, `sleep`, `yield`, `setDaemon`/
      `isDaemon`, `getName`/`setName`, `isAlive`, `activeCount`, `run` +
      four `<init>` shapes; per-thread interned Thread object; JDK-exact
      failure strings; `java/lang/Runnable` joined the codegen SAM set
      (0-arity `()V` shape).
- [x] **T07** — per-thread frame registries moved into the global
      `ThreadRecord` registry (frame.rs keeps owner-only depth/limit
      scalars); main registered first thing in the generated `main_run`;
      `await_non_daemons` JVM shutdown rule; worker-stack sizing
      (`J2R_THREAD_STACK`, M2-calibration default 32 MiB).
- [x] **T08** — STW safepoint protocol: `GC_REQUESTED` + `StwGuard`
      (publish → wait all Parked/Exited → mark/sweep → release);
      collector scans ALL threads' registries + Thread objects;
      **born-survivor marking** closes the allocate/GC race; lock law
      L4–L6; four deadlock classes found by the first test runs and
      closed (docs/M4_EVIDENCE.md §Wave 2).
- [x] **T09** — coop scheduler (pass-if-held/acquire-otherwise yield
      shape, ring participation, turn recovery, seed pinning) + fixtures
      **ThreadHello / ThreadSafepoint byte-identical vs Temurin**
      (scheduler-deterministic by construction; free-mode differential)
      + runtime tests 113 → **123 green** (handshake, cross-thread
      rooting, identity, join HB, coop determinism, registry stability).
      Suite 86 on disk; doc counts 86/266 in sync (gate-11).

### M4 Wave 3 — monitor contention + wait/notify (T10–T13)

- [ ] **T10** — contention monitor state machine per heap entry:
      `unowned | owned{owner, count}` with owner thread id; contended
      enter parks on a per-monitor `Condvar`.
- [ ] **T11** — `wait`/`notify`/`notifyAll` with JDK-exact semantics:
      ownership required (else `IllegalMonitorStateException`, existing
      message pinned); atomic full-count release + park; wake restores
      the full count; spurious Condvar wakeups absorbed internally,
      invisible to Java code.
- [ ] **T12** — `wait(timeout)`/`wait(timeout, nanos)`: millisecond-
      effective timing, JDK-faithful rounding (nanos ≥ 500000 rounds up),
      `IllegalArgumentException` on negatives with exact messages.
- [ ] **T13** — fixtures `WaitNotify` (producer/consumer handoff,
      coop-deterministic), `MonitorContention` (two threads, 10k
      uncontended + contended cycles), `WaitTimeout` (loose timing
      windows for CI runners); unit tests incl. re-entrancy across wait.

### M4 Wave 4 — interrupt, daemon shutdown, JMM-lite, stress (T14–T18)

- [ ] **T14** — interrupt semantics: per-thread status flag;
      `sleep`/`join`/`wait` wake with `InterruptedException` (exact
      messages); monitor-enter NOT interruptible (JDK behaviour).
- [ ] **T15** — daemon shutdown rules: `main` exits when only daemons
      remain; daemon death during GC cooperation handled by the registry
      (unregister-before-park discipline, unit-tested).
- [ ] **T16** — JMM-lite statement in RUNTIME_SAFETY.md §6: guaranteed
      (monitor release→acquire happens-before, `Thread.start`/`join`
      edges, `Thread.sleep` guarantees nothing) vs NOT guaranteed
      (volatile, final-field freeze, data-race freedom for racy
      programs); stale handles stay impossible (gen check → loud panic).
- [ ] **T17** — concurrency stress suites (coop: N producer/consumer
      pairs × randomized seeds, fixed iterations; free-running:
      invariant assertions only, no output compare) + **new
      gate-13-concurrency** (coop differential fixtures + stress,
      per-seed timeout that fails loudly).
- [ ] **T18** — M4 completion package + review submission.

### Corpus & quality (interleave when an M4 wave closes)

- [ ] **F2 verifier gap (blocks corpus expansion)** — verifier rejects
      clinit-initialized reference statics (the singleton-factory
      pattern minimal-json and real libraries use). High priority.
- [ ] **F3 java.io behavior natives** — StringWriter/StringReader
      behavior parity follows F2 by design.
- [ ] **F4, F5** — open corpus gaps, loud (docs/CORPUS_WAVE.md).
- [ ] **Corpus entry 2** — next real-world library/program; each corpus
      wave must add differential evidence, not micro-probes.
- [ ] **R2 metadata generator (post-M4 scope)** — reflection metadata
      retention so runtime string lookups resolve (closed-world design
      in docs/STRATEGIC_GAPS.md).
- [ ] **R5 benchmark harness** — policy locked (benchmarks observe,
      never gate); harness lands after the corpus wave; no performance
      sentence without a number, a date, and a reproduction command.

### M5 Minecraft readiness (was M4; no compatibility claim without evidence)

- [ ] incremental compilation for 70k+ classes
- [ ] JNI bridge for LWJGL/GLFW/OpenAL (R3, safe-Rust design)
- [ ] mixin application at the bytecode level before j2r (R1)
- [ ] Mojang/intermediary mappings pipeline
- [ ] inline caches for megamorphic call sites

### M6 mod ecosystem

- [ ] Fabric/Forge/NeoForge surface as natives + compiled mods
- [ ] `j2r build mod.jar`
- [ ] `j2r test mod` dev loop

## 4. Standing law (always, no exceptions)

- Zero `unsafe` — compiler-enforced forbid + gate-10 grep.
- Every commit passes all gates; gates run locally before push.
- Never claim "done" without runnable evidence; never claim Minecraft
  compatibility without milestone evidence.
- Test with bigger real-world files, not dummy fixtures — every wave
  adds real-program corpus evidence.
- Test counts synchronized with docs, mechanically (gate-11).
- Differential evidence is three-channel (stdout/stderr/exit) vs a real
  Temurin 21 — a masking bug in the harness poisons every green claim.
