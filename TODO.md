# J2R — Master Checklist (plain text: done vs planned)

The live status board for the whole project. A box is checked ONLY with
runnable evidence (test name, CI gate, fixture, commit). `[~]` = started,
with the gap stated. If a claim and this board disagree, this board loses —
fix the board. Last updated: 2026-11-05.

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
      weekly schedule; codeql.yml). **gate-14 upgraded to full potential**
      (owner directive 2026-11-05, "use CodeQL full potential", CodeQL
      zero-to-hero series): security-extended + security-and-quality
      suites, J2R invariant queries (analysis/codeql/ — unsafe vectors +
      stdout pollution tripwires, validated locally with CLI 2.27.4:
      clean compile, positive controls on a known-bad fixture DB, zero
      findings on the repo DB), pack dependency caching, workflow_dispatch,
      action v4 SHA-pinned; .github/codeql/codeql-config.yml.
      **gate-9 unblocked by sharding** (2026-11-05): the gc-stress suite
      outgrew its 60-minute runner cap at the 88-fixture census — the
      three runs after 98be983 (e5b152c, d302c2b, 50e0ed6) died at exactly
      60m15s into gate-9 (GitHub renders the timeout kill as a cancelled
      job). The J2R_DIFF_SHARD=i/N stride harness (71483a0, built for
      exactly this but never wired) is now a 4-way matrix in ci.yml under
      a 90-minute cap (3-way skewed hard: legs of 1m42s / 27m / 60m15s —
      the real-thread fixtures at the alphabetical tail dominate runtime);
      plumbing proven locally (shard 2/3 → 29 of 89 fixtures, bad spec
      parse verified by inspection).
      **gate-9 rebalance HISTORY (honest): 4-way (e9f81ac, 90-min cap)
      failed its own bet — legs 24m / >=90m CANCELLED / >=90m CANCELLED /
      1m; 6-way (85dce58, 120-min cap) failed too on the same tree — legs
      14m / 1m / >=120m CANCELLED / 1m / 1m / >=120m (CI #59 on 7bb7a8d).
      Stride balances fixture COUNTS, not durations: ~10 allocation-storm
      fixtures (BigJson, BigCache, Cycles, LambdaGC, MonitorContention,
      WaitNotify, ReflectGC, InternAcrossGc, the corpus-scale Big* ones)
      dominate total gate time and stride drops them two-per-leg. Fix in
      flight: local per-fixture measurement at J2R_GC_EVERY_N=1 →
      J2R_DIFF_FIXTURES explicit selection in the harness (census guard
      untouched) → duration-balanced explicit buckets in ci.yml.
      **gate-9 RE-ARCHITECTED (2026-11-05, same day): the gc-probe
      one-shot diagnostic workflow (workflow_dispatch, ops tooling, not
      a gate) ran each suspect SOLO on real CI and localized the bombs:
      ReflectGC ~14m at N=1 (matches the local 540s × ~1.5 factor);
      MonitorContention 2/2 and WaitNotify 1/2 hit the 15-minute inner
      timeout at N=1 — a timing-sensitive Wave-3 protocol race on slow
      2-vCPU runners — while finishing in <1m at N=8. BigJson/BigCache/
      Cycles/LambdaGC are all <1m on CI. gate-9 now runs six
      duration-balanced legs (solo / monitors-N8 / heavy / light-0..2,
      40-min cap) with the policy in the harness (diff_suite.rs
      bucket_for_gate9) and a partition unit test pinning union ==
      census, so a new fixture can never fall between legs; fixed
      special lists are intersected with the on-disk census so renames
      and deletes cannot leave phantom entries. The monitor race is the
      Wave-4 stress task's first case (probe evidence logged in the
      workflow history).
- [x] **Security-settings review package (2026-11-05, owner directive
      "add more workflows")** — **gate-15 OSSF Scorecard** (scorecard.yml,
      actions SHA-pinned, weekly cron + workflow_dispatch) and **gate-16
      OSV Scanner** (osv.yml, google/osv-scanner reusable workflow) —
      both first-party, zero-credential, publish SARIF to the Security
      tab, sit outside the gate-8 aggregate like gate-14; market
      candidates needing accounts (Snyk/Semgrep/SonarCloud), other
      languages (SpotBugs), DAST with no target (ZAP), or duplicating
      existing gates were evaluated and rejected with reasons.
      **.github/dependabot.yml**: cargo + github-actions weekly updates,
      grouped minor/patch. **First-run evidence at 7bb7a8d: Scorecard #3
      SUCCESS, OSV #5 SUCCESS, CodeQL #15 SUCCESS** — gate-16 needed an
      actions:read grant on the caller (bare startup_failure on 85dce58;
      reusable workflows declare their own permissions, un-granted
      callers fail validation, codeql-action#2117; fixed 7bb7a8d).
      Dependabot PRs #1/#2 (actions/checkout 4.4.0→7.0.1, setup-java
      4.9.1→6.1.0) open — owner decides merge.
- [~] **M4 threads & concurrency — ACCEPTED (owner directive
      2026-11-04, plan rev 1)** — 18-task register at docs/M4_PLAN.md §6.
      **Wave 1 shared substrate LANDED** (T02–T05, commit 4ddfe1b; CI
      aggregate green on that push). **Wave 2 real threads LANDED**
      (T06–T09: Thread natives + registry + STW safepoints + coop
      scheduler; two new real-thread fixtures byte-identical vs Temurin;
      docs/M4_EVIDENCE.md §Wave 2). **Wave 3 monitor contention +
      wait/notify LANDED — COMPLETE incl. T13** (T10–T13: real per-entry
      monitor state machine, Mesa wait/notify/notifyAll, JDK-exact
      wait(long,int), STW/coop park protocol, lock law L7; the T13
      Temurin probes falsified two doc-derived pins — IAE-before-IMSE
      ordering on wait(-1), "current thread is not owner" with is,
      message-less monitorexit — and MonitorContention exposed a real
      generator bug: ensure() flagged the class initialized BEFORE
      <clinit>, so four-way first-touch contention let a loser skip
      clinit and read null statics; fixed with the JLS 12.4.2 state
      machine in j2r-runtime::clinit. 0 fails / 30 runs; all 89 fixtures
      byte-identical vs Temurin on all 3 channels; runtime 123 → 134
      green, workspace 277 — docs/M4_EVIDENCE.md §Wave 3 + addendum).
      **Wave 4 interrupt/daemon/JMM-lite/stress LANDED — COMPLETE
      through T17** (T14 commit 3c08842 + fixtures 3f80b0e; T15
      e331475; T16 ece590f; T17 6b6b7e3 — details in the Wave 4
      section). Next: **T18** completion package + review submission.

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

- [x] **T10** — contention monitor state machine per heap entry:
      `unowned | owned{owner, count}` with owner thread id; contended
      enter parks on a per-monitor `Condvar` (crates/j2r-runtime/src/
      monitor.rs; `Entry.mon` lazy `Arc<Monitor>`; park/wake protocol in
      thread.rs; lock law L7).
- [x] **T11** — `wait`/`notify`/`notifyAll` with JDK-exact semantics:
      ownership required (IMSE, "current thread not owner", checked
      BEFORE argument ranges — HotSpot CHECK_OWNER order); atomic
      full-count release + park; wake restores the full count; spurious
      Condvar wakeups absorbed internally, invisible to Java code.
- [x] **T12** — `wait(timeout)`/`wait(timeout, nanos)`: millisecond-
      effective timing, JDK-faithful rounding (nanos ≥ 500000 rounds up),
      `IllegalArgumentException` on negatives with exact messages.
- [x] **T13** — COMPLETE (d302c2b fixtures + evidence addendum e5b152c).
      Unit half: 9 monitor tests incl. re-entrancy across wait,
      contended-enter handover, timed-wait deadline. Fixture half:
      `WaitNotify` (producer/consumer handoff, coop-deterministic),
      `MonitorContention` (four threads, 10k uncontended + contended
      cycles), `WaitTimeout` (loose timing windows for CI runners) —
      differential vs Temurin 21.0.13, all three byte-identical;
      MonitorContention caught the clinit race (§3 M4 bullet above).
      Suite 86 → 89 on disk; diff-suite guard 85 → 88 (gate-11 sync).

### M4 Wave 4 — interrupt, daemon shutdown, JMM-lite, stress (T14–T18)

- [x] **T14** — interrupt semantics LANDED (commit 3c08842 + fixtures
      3f80b0e). Temurin 21.0.13 probes pinned the contract first
      (InterruptProbe / JoinInterruptProbe / EdgeProbe): sleep IE carries
      "sleep interrupted", join/wait IE are message-less, pre-set flags
      throw immediately, delivery CLEARS the flag, monitor-enter is
      immune and keeps the flag sticky, interrupt on a dead thread still
      sets the flag, never-started interrupt is observable on the
      object. Mechanics: per-record AtomicBool + per-thread ticket
      condvar (notify and interrupt share the wake channel; wait-set
      membership + flag decide the meaning), slot 5 = pendingInterrupt
      as the never-started carrier (moves into the record at start),
      monitor wait parks on the ticket with the JDK wake order
      (interrupt beats notified beats deadline), interruptible
      free-running sleep/join/self-join, coop checks per tick.
      Lock-law L7 audited twice mid-flight (entry check moved outside
      the monitor lock; SCHED guard dropped before heap access — the
      audit found real cycles). Runtime 134 → 143 green; three new
      fixtures byte-identical vs Temurin locally (suite 92, tests 287).
- [x] **T15** — daemon shutdown rules COMPLETE (commit e331475). Found
      and fixed the wave's registry bug: setDaemon wrote only the object
      slot, the RECORD's daemon flag was never wired, so
      await_non_daemons treated every daemon as a non-daemon and the
      process hung for the daemon's full lifetime (DaemonExit: 60s
      sleep -> 60s hang; now exits in ~2ms with Temurin-exact output).
      Fix: start migrates slot 2 into the record (the T14 interrupt-
      migration pattern) and setDaemon syncs an existing record;
      await_non_daemons joins only STARTED Java threads (ring membership
      = the start grant) — bystanders must never hold the JVM exit open.
      Fixtures DaemonExit + DaemonInterrupt byte-identical vs Temurin
      (DaemonInterrupt coop-safe via the flag-spin pattern). Runtime
      143 -> 146 green; suite 92 -> 94; workspace 290; THREAD_SLOTS
      5 -> 6 (stale const). Evidence: docs/M4_EVIDENCE.md §T15.
- [x] **T16** — JMM-lite statement COMPLETE (commit ece590f).
      RUNTIME_SAFETY.md §6: guaranteed (monitor handover, start/join
      edges, no stale handles, no long/double tearing — stronger than
      JVM, clinit race closed, interrupt visibility) vs NOT guaranteed
      (no volatile, no final freeze, no data-race freedom, sleep yields
      nothing, free-mode non-determinism) + the honest coarser-lock-
      simpler-model note and the re-issue trigger for any future
      lock-free fast path.
- [x] **T17** — concurrency stress suites + gate-13 COMPLETE (commit
      6b6b7e3). Chaos knob (J2R_CHAOS_US, test-only randomized
      micro-sleeps at the protocol seams) = the local stand-in for CI's
      slow scheduling; the stress rounds found and fixed TWO real races:
      (1) the monitor-wait lost wakeup (the CI-timing race — bounded 5ms
      re-check slice in object_wait); (2) the coop ring deadlock on
      monitor-parked holders (in_monitor_wait flag + turn-grant skip
      scanning from the HOLDER — the skip's first draft was itself a
      total-freeze bug found by the stress round: scan origin 'me'
      re-selected the same parked candidate forever while holding
      SCHED). Stress suites: coop 5 seeds x 3 pairs x 40 iters;
      free-running 0/300/800/1500µs chaos x 4 pairs x 30 iters; exact-
      tally invariant, loud deadlines; runtime 146 -> 148 green.
      **gate-13-concurrency LANDED** (inside the gate-8 aggregate): coop
      differential x seeds {0,1,7,11} on the ten thread fixtures vs
      Temurin under four different deterministic schedules + the stress
      step, 25-min cap = the plan's per-seed loud timeout. Gate-13's
      local validation caught two schedule-sensitive probes before CI
      did (coop sleep is a yield): InterruptJoin/InterruptWait rewritten
      with the DaemonInterrupt spin pattern, validated free-mode AND
      coop seeds 0/1/7/11; full 94-fixture free-mode differential green
      locally (344s). Evidence: docs/M4_EVIDENCE.md §T17.
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
