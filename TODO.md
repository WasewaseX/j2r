# J2R — Master Checklist (plain text: done vs planned)

The live status board for the whole project. A box is checked ONLY with
runnable evidence (test name, CI gate, fixture, commit). `[~]` = started,
with the gap stated. If a claim and this board disagree, this board loses —
fix the board. Last updated: 2026-11-13 (session 19b cont. 8: R2 slice 3 landed; next G3.3 specialized-map).

Project: J2R — Java → Rust compiler, safe Rust (zero `unsafe` outside
the audited `j2r-jni-ffi` FFI core: 11 occurrences, gate-10 pin ≤ 12,
docs/M5_JNI_FFI.md §3).
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
- [x] **Gate upgrade package 2 (2026-11-06, owner directive: take the
      WasewaseX/master-ci review, check the claims, fix the true ones,
      and "customize it and add and improve our current gates — no need
      to delete current, update and improve and add more gates")** —
      five NEW gates (17–21) + one efficiency mechanism, zero existing
      gates removed:
      * **gate-17-supply-chain** — cargo-deny 0.20.2 (deny.toml: license
        compliance trimmed to the licenses actually in the resolved tree
        — MIT/Apache-2.0/Unicode-3.0, so a new license reds the gate;
        multiple-versions deny; source-registry pinning to crates.io;
        advisories as defense in depth on gate-7) + **gitleaks** v2.3.9
        secret scan (local full-history evidence: 93 commits, ~3.1 MB,
        no leaks). Local: `advisories ok, bans ok, licenses ok, sources
        ok`.
      * **gate-18-actions-security** — **zizmor** v0.8.1 action + **actionlint**
        v1.7.9 over all five workflows. zizmor's birth run found a REAL
        gap the audit class knew about and we had missed: every checkout
        persisted the GITHUB_TOKEN into the runner's .git/config
        (artipacked, 14 findings) — all 14 fixed with
        `persist-credentials: false`; CI never pushes, so the runner
        now keeps no long-lived credential. zizmor re-run: "No findings
        to report" (23 suppressed); actionlint green.
      * **gate-19-doc-links** — `scripts/check_doc_links.py`: every
        relative link and intra-file anchor in README/TODO/ROADMAP/
        examples + docs/**/*.md must resolve (deterministic: external
        URLs deliberately not checked — no network false positives).
        Ships with a synthetic-tree selftest (4/4 findings match).
      * **gate-20-code-integrity** — `scripts/check_stubs.py`: the
        master-ci "blocking-grade rules only" model — deterministic
        content defects block, stylistic prose heuristics never do (the
        external review's own verdict on AI detectors). Scans
        crates/**/*.rs for todo!(/unimplemented!(/elision markers
        ("... existing code ...")/FIXME. Birth scan: zero hits; selftest
        4/4.
      * **gate-21-dependency-hygiene (ADVISORY)** — cargo-machete 0.9.2,
        `continue-on-error: true` BY DESIGN (grep-based, can miss
        macro-only usage; reports, never blocks). Local run: "no unused
        dependencies". Promotion to blocking needs a green history +
        owner decision.
      * **`changes` path filter** — prose-only PRs skip the five heavy
        compile-and-run legs (gates 3/4/5/9/13) on pull_request ONLY;
        pushes to main always run everything (evidence chain intact).
        Fails OPEN on any uncertainty; the review's edge cases (shared
        lib, manifest, lockfile, root config, generated file, workflow
        itself, corpus fixture, empty diff) are pinned by
        `scripts/path_filter.py --selftest` — 17/17 green — which runs
        inside the changes job on EVERY invocation.
      * gate-8 aggregate extended: 11 → 16 upstream gates
        (+17/18/19/20; advisory 21 stays outside).
      * All new third-party actions SHA-pinned (gitleaks-action v2.3.9,
        zizmor-action v0.8.1, rhysd/actionlint v1.7.9); new scanners
        version-pinned via taiki-e (cargo-deny@0.20.2,
        cargo-machete@0.9.2). Every tool was run locally against the
        real tree BEFORE the push — the package landed green by
        evidence, not by hope.
      * **First-run CI evidence at 1952c10: fully green — all 32 check
        runs success, including gate-8-all-gates-green, the five new
        gates (17/18/19/20/21), the path-filter job, and every heavy leg
        (gate-3 differential, gate-9 all six buckets, gate-13 all four
        seeds).** One honest miss that CI itself caught on the first
        push (f392220): the zizmor SCAN was green but the action's
        post-processing SARIF upload 403'd under the workflow-level
        `contents: read` ("Resource not accessible by integration") —
        zizmor-action publishes findings to the Security tab via the
        CodeQL-action, which needs `security-events: write`. Fixed
        scoped to that one job only (1952c10) — the gate-16 lesson
        (7bb7a8d) replayed. gate-18 now also publishes zizmor SARIF to
        the Security tab, matching gates 14/15/16.
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
      through T18** (T14 commit 3c08842 + fixtures 3f80b0e; T15
      e331475; T16 ece590f; T17 6b6b7e3 + root-cause T17.1 640154a;
      T18 completion package — details in the Wave 4 section).
      M4 status: all four waves implemented and CI-verified; owner
      review pending (docs/M4_EVIDENCE.md §completion package).

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
      **T17.1 (640154a) — the stress harness caught the TRUE root cause
      of the gate-9 CI-timing hangs one day after landing**: a tight
      allocation-free loop (synchronized spin) can never reach an
      allocation-site collection point, so STW quiescence waited for it
      forever — not a monitor lost-wakeup. Fixed on three halves:
      backedge safepoint polls in codegen (if heap::gc_pending() {
      refresh; gc_point(); } — JVM safepoint-poll discipline), clinit-gate
      waiters now park (STW participation), every state flip notifies in
      free mode (the collector slept through parks). gate-9 monitors
      bucket back to uniform N=1 (N=8 workaround retired with evidence);
      J2R_STW_TRACE + per-thread mon_ops observability. Local evidence:
      94-fixture differential byte-identical (348s), monitors bucket 15
      at N=1 green (55.7s), cfg_equiv 94/94 sharded, corpus green.
- [x] **T18** — M4 completion package LANDED (docs/M4_EVIDENCE.md
      §completion package): per-task evidence table T01–T18, gate
      inventory 1–16, honest limitations register (no volatile/final
      freeze, ReflectGC N=1 cost, bench policy, 1:1 threads), re-entry
      triggers. Owner review pending — M4 stays open on the board until
      the owner accepts the package.

### Corpus & quality (interleave when an M4 wave closes)

- [x] **F2 verifier gap — RESOLVED by probe audit (2026-11-05)** — the
      board claim "verifier rejects clinit-initialized reference
      statics" was STALE for the pattern it named: five probes
      (singleton lazy factory; static-init-block singleton; Object-typed
      upcast static; invokedynamic-concat String statics; cross-class
      `Holder.h = new X()` in clinit) all compile, build, and run
      byte-identical vs Temurin — the wave-1 pop_exact fix and the
      JLS 12.4.2 clinit state machine closed those shapes earlier.
      The audit DID find one real remaining gap: a direct call through a
      JDK-interface-typed reference (`r.run()` on `Runnable`,
      `s.get()` on `Supplier`) — every supported SAM interface had its
      natives dispatch row EXCEPT java/lang/Runnable (added in Wave 2
      only as a Thread body, never invoked through its interface). Fix:
      the ("java/lang/Runnable", "run", "()V", lambda::n_sam) row +
      LambdaClinit fixture (lambda + final/volatile reference statics in
      <clinit>, called through the interface types). Suite 94 → 95,
      full suite byte-identical vs Temurin (346s); guard 93 → 94;
      README/gate-11 counts synced. F3 (java.io behavior parity) no
      longer blocked by F2.
- [x] **Singleton/cyclic-init wave — LANDED (2026-11-06)** — the review's
      P1 "singleton-factory and cyclic-initialization differential
      fixtures" made PERMANENT (the F2 probes proved the shapes but were
      never pinned in the suite). Two new fixtures, both byte-identical
      vs Temurin 21.0.13 on the first differential run:
      **SingletonFactory** — the three canonical singleton idioms through
      `<clinit>`: a `private static final` INSTANCE built by a static
      factory method calling the private constructor (factory identity:
      `a == b`, constructor-exactly-once via a `created` counter, state
      mutation visible through the second handle), the static-init-block
      form, and the initialization-on-demand-holder idiom WITH A
      LAZINESS PROOF (reading `HolderSingleton.created` before the first
      `getInstance()` prints 0 — the holder class's `<clinit>` must not
      fire early; prints 1 after). **CyclicInit** — two pairs of
      mutually-referencing `<clinit>` bodies pinning JLS 12.4.2
      in-progress recursion: cycle 1 (InitA assigns `s=10` BEFORE
      recursing into InitB) observes the ASSIGNED value across the cycle
      (10/110/110); cycle 2 (InitC recurses into InitD before assigning
      x) observes the DEFAULT value (0 → y=1, then x=1, own=5) —
      1/1/5. The wave-3 JLS 12.4.2 clinit state machine holds for both
      directions with zero new runtime code needed. Suite 104 → 106;
      guard 103 → 105; README counts synced (106 programs, 106/106
      cfg-identical; test count re-measured unchanged at 307 — fixtures
      add no Rust tests). CI evidence: run 42198778306 on 27489e8 fully
      green — 27/27 check runs success, including gate-3 (the 106-fixture
      differential), all six gate-9 forced-GC buckets, all four gate-13
      concurrency seeds, and the gate-11 count-sync gate.
- [x] **Depth-4 verifier wave — LANDED (2026-11-06)** — the registered
      "depth >= 4" limit closed: the verifier lattice gained
      RefPrimArray3{Int,Long,Float,Double} (the deep-arrays wave pattern
      one level deeper), so `[[[[<prim>` arrays now verify with EXACT
      typing: extracted `[[[<prim>` rows satisfy `[[[<prim>`-typed call
      sites, and wrong-kind primitive ops on a `[[[[I` fail verification
      naming the exact shape (`array element kind mismatch ... array is
      RefPrimArray3Int`) instead of the old generic Ref. The runtime
      needed ZERO changes (it was already descriptor-driven and exact —
      the limit was verifier-side precision only; a 4D program compiled
      and ran byte-identical even before this wave). Four new verifier
      unit tests (chain typing, precise mismatch message, typed call
      site, depth-5 Ref collapse still loud); codegen 67 → 71. The
      DeepArrays fixture grew ten IN-PLACE depth-4 rows (count stays
      106): full/partial/one-dim allocation, nested store/load, the
      typed call-site payoff, shallow clone (leaves share), 4D
      arraycopy, null-row NPE, instanceof + getClass().getName() shapes
      (`[[[[I`/`[[[I`/`[[I`), and the `[[[F`-into-`[[[I` ASE message —
      byte-identical vs Temurin 21.0.13. docs/CORPUS_WAVE.md §gaps
      updated; depth >= 5 is the new registered limit, pinned by
      depth5_still_ref_registered_limit. CI evidence: run 42211742976 on
      55ed691 fully green — 26/26 check runs success (incl. gate-3, all
      six gate-9 buckets, all four gate-13 seeds, gates 14-21).
- [x] **Corpus wave 3: JAMA — LANDED (2026-11-06)** — the third real-world
      corpus entry (NIST JAMA 1.0.3, public domain, 7 files / ~3.3k lines:
      Matrix + all five decompositions — a dense-2D-double-array,
      IEEE-heavy numeric shape unlike minimal-json's object graph or
      JZlib's byte streams). Vendored verbatim into corpus/jama/ (JAMA's
      self-disabled "temporary code" sections preserved — getR() and the
      (Matrix,int) ctors are commented out upstream by broken comment
      markers), merged by scripts/build_corpus_jama.sh, driver JamaCorpus
      (construction/copy/slice semantics, arithmetic, norms, all five
      decompositions, Hilbert 5x5 SVD, symmetric + nonsymmetric eigen with
      residuals, five message-exact error paths). **Output-identical to
      Temurin 21.0.13 on all three channels, first run** — 180 lines
      incl. eigen residuals at 1e-13 through thousands of interpreted
      double ops. Pinned permanently in corpus_suite.rs (2 → 3 entries).
      What it cost, each found by a loud failure: (1) verifier array
      covariance — JLS 4.10.3 (`[[D <: [Object`) now accepted by
      pop_exact for reference-element arrays (ElemKind::elements_are_refs),
      pinned differentially by the new ArrayCovarianceProbe fixture
      (10 rows, byte-identical first run, suite 106 → 107, guard 105 →
      106); (2) the verifier's dup2_x2 form classification was scrambled —
      javac's compound cat-2 assignment (`LUrowi[j] = LUcolj[i] -= s;`)
      emits JVMS form 2 (cat2-on-top-of-two-cat1) and the old verifier
      REJECTED it (a unit test even pinned the wrong shape) — rewritten
      to the runtime stack.rs table (which was differential-proven
      correct all along), three unit tests updated; (3) Math.random is
      REAL (the JDK's 48-bit LCG with uniquifier^nanoTime lazy seed);
      (4) PrintWriter is a REAL subset (JDK-source print/println/write
      semantics over native stdout/stderr + StringWriter sinks);
      (5) Vector / StreamTokenizer / DecimalFormat family registered
      LOUD (the serialization precedent: resolve + instantiate so real
      programs compile, behavior throws naming the gap — Matrix.read /
      Matrix.print are uncalled by the driver); Double.valueOf(String)
      is real; Locale.US is a real interned constant. Bench rows taken
      per the R5 gap ("one row per corpus entry"): MinimalJsonCorpus
      0.04x / JamaCorpus 0.09x native-vs-C1-java (docs/BENCHMARKS.md);
      JZlibCorpus stays rowless BY RULE — the release build's rustc peak
      RSS re-measured at 3.03-3.56 GB across five attempts and three
      profiles, above the sandbox ceiling (the registered codegen-scale
      finding). Local gates at this tree: differential 107/107 sharded,
      cfg_equiv 107/107 both modes (452.7s + 449.5s), corpus 3/3, lib
      tests 272 green, jar/lambda/soe green, fmt/clippy clean, gates
      11 (fixtures=107 tests=311) / 12 / 19 / 20 green, path-filter
      selftest 17/17, forced-GC on the new fixture green. CI evidence:
      run 42237786810 on 2943d2c fully green — 27/27 check runs success,
      including gate-3 (the 107-fixture differential), all six gate-9
      forced-GC buckets, all four gate-13 concurrency seeds, and the
      gate-11 count-sync gate.
- [x] **Typed-register readiness wave — LANDED (2026-11-07)** — the
      M2_SHAPE_REPORT's registered follow-up ("track per-position live
      ranges … re-decided with its own 3a report") delivered as
      `crates/j2r-codegen/src/readiness.rs` + the `j2r readiness-report`
      subcommand: per-position EPOCH analysis (kinds Int/Long/Float/
      Double/Ref collapsed from the verifier lattice; kills = stores to a
      local slot, depth drops for stack positions; kinds may differ
      across epochs, never within one — mixed runs count AGAINST
      readiness), combined with the REAL cfg::lift decision and an
      occupancy-weighted primitive-share metric; 9 unit tests. The
      census (107-fixture suite + JAMA + minimal-json + JZlib, before/
      after tables in docs/TYPED_REGISTER_READINESS.md §3) exposed THREE
      real lift-pipeline defects, all fixed this wave, all conservative-
      direction (fallback bodies were exact at all times):
      (1) `Cfg::idom()` seeded a node's idom with `idoms[pred]` instead
      of the predecessor itself and intersected on raw indices — EVERY
      loop with a non-entry header was mis-flagged `Irreducible` (the M2
      synthetic tests passed only because their loop header WAS the
      entry block); rewritten as Cooper–Harvey–Kennedy with an RPO
      intersect, the old `irreducible_falls_back` test replaced with a
      genuine two-entry cycle (the old graph was actually reducible).
      (2) `Builder::region()` checked loop headers only on entry, so the
      pre-loop block FALLING THROUGH into javac's peeled guard emitted
      the header inline → its guard became a diamond meeting at the
      header → `ImpureJoin` on every javac for/while; fixed with an
      in-walk header check (fresh → emit_loop, active fall-through →
      continue, the do-while back edge). (3) `join_via_label`'s purity
      check refused a loop-header meet's own back edges → every nested
      `for` kernel refused; fixed by exempting the meet's loop body
      nodes (now stored in `Loop`). Effect on real code: probe
      for/while/nested-for all LIFT; suite needs-lift 184 → 146; JAMA
      needs-lift 70 → 32 (hottest kernels now show their true named
      blockers: tred2/orthes/hqr2/LU-ctor ImpureLoopExit, tql2/isNonsin-
      gular MultiExitLoop); JamaBench --emit-cfg dispatchers 106 → 31
      (Matrix 63 → 12; LU/QR no longer fully fallback) — and the steady-
      state control run at this coverage extreme is 275.6s vs the 274.0s
      hand median, output byte-identical: control-flow structuring
      measured at BOTH extremes buys nothing at steady state, confirming
      the typed-operand model as the registered lever (GO, sequenced:
      ImpureLoopExit + MultiExitLoop lowerings are its prerequisites to
      reach the hot kernels — docs/TYPED_REGISTER_READINESS.md §5).
      Local gates at this tree: codegen tests 67 → 83, workspace 323
      green (README count synced 311 → 323), cfg_equiv 107/107 both
      modes (451.3s + 445.5s — the newly lifted bodies are output-exact),
      differential 107/107 (453.0s), corpus 3/3, jar/lambda/soe green,
      fmt/clippy clean, gates 11 (fixtures=107 tests=323) / 12 / 19 / 20
      green, path-filter selftest 17/17. CI evidence: run 42284179620 on
      29c5cc6 fully green — 30/30 check runs success, including gate-3
      (the 107-fixture differential), gate-9 forced-GC solo leg, all four
      gate-13 concurrency seeds, CodeQL / OSV / Scorecard, and the
      gate-11 count-sync gate (tests 311 → 323).
- [x] **Lift-closure wave — LANDED (2026-11-07)** — the readiness GO's
      step 1 (docs/TYPED_REGISTER_READINESS.md §5): the two named lift
      refusals closed, with two real generated-code bugs caught by
      cfg_equiv on the way and fixed:
      (1) exit edges grouped by TARGET BLOCK, not (from, to) pairs —
      javac's `break`/`continue` inside `while`/`do-while` adds edges to
      the SAME after-loop block, which the old comparison refused as
      MultiExitLoop;
      (2) terminator targets (early `return`/`athrow` blocks — excluded
      from the natural-loop node set by the preds walk) are emitted
      inline at the edge, not treated as open exits;
      (3) goto-trampoline confluence: javac routes `break` through
      single-goto blocks, so two edges reach the same continuation via
      different blocks — stops now key on the RESOLVED chain end (chains
      never walk through a loop header), with the coverage check
      exempting skipped trampolines (their `goto` is structural);
      (4) the shared-exit shape (an inner loop whose exit block is the
      enclosing loop's continue-target — JaggedNew) routes through the
      enclosing join label, and arm walks go THROUGH a fresh loop header
      to its open exit;
      (5) a structural safety net refuses any degenerate
      `loop { continue <same label>; }` body.
      cfg_equiv's catch #1: a back edge to a header that is also a stale
      join-meet stop emitted `continue 'join_...` — illegal Rust (labeled
      blocks cannot be continued); fixed by routing active-header edges
      to the loop's OWN label before any stop lookup. cfg_equiv's catch
      #2: an emit_loop body walk whose entry carries a stale stop entry
      was hijacked into a tight self-continue loop (BigRpg.mergeSort hung
      at 99.9% CPU); fixed by treating the body-walk ENTRY as emission
      (a `body_walk` flag cleared for recursion) and by checking the
      entry stop BEFORE loop construction (a fresh header that is a
      pending meet is a transfer target — the loop is emitted by the
      stop's owner). Census (cfg-report, doc basis): suite needs-lift
      146 → **126**, JAMA 32 → **21**; ImpureLoopExit GONE (2 and 7 →
      0); suite MultiExitLoop 24 → 2 (the two genuine labeled-break
      methods: LabeledLoop, BigRpg.battle), JAMA 5 → 0 — **tql2 and
      hqr2 now LIFT**. JamaBench --emit-cfg dispatchers 31 → **21**.
      Steady-state control (docs/BENCHMARKS.md addendum 2): **273.9s,
      output byte-identical** vs the 274.0s hand median — structuring
      measured flat at a THIRD coverage extreme; the residual layer is
      the registered next prerequisite (residual diamond-shape refusals
      — suite 36 / JAMA 13 ImpureJoin, the diverging arm walks of
      tred2/orthes/LU-ctor). Validation: codegen lib tests 86 → **87**
      (README 326 → 327 synced, gate-11 green), cfg_equiv **107/107
      both modes** (6 shards, 136-150s each), differential **107/107**
      vs Temurin 21.0.13 (437.9s), corpus 3/3, jar/lambda/soe green,
      fmt/clippy clean, gates 11/12/19/20 + path-filter selftest 17/17
      green locally. CI evidence: **30/30 check runs success on 98b37e3**
      (verified via the GitHub check-runs API) — including gate-3 (the
      107-fixture differential AND the cfg-equivalence suite with the new
      lowerings), all gate-9 forced-GC buckets, all four gate-13
      concurrency seeds, and the gate-11 count-sync gate (327).
- [x] **F3 java.io + collections behavior surface — was FIXED in corpus
      wave 1 (board was stale; the board loses, CORPUS_WAVE.md §F3 is the
      evidence)** — io.rs StringWriter/StringReader, Writer template
      forwarders, unmodifiable views, ArrayList(Collection), List
      hashCode/equals, arraycopy messages, radix parsing, parseFloat/
      parseDouble, String.getChars, serialization hierarchy markers —
      pinned by corpus/MinimalJsonCorpus + corpus_wave1_io_hierarchy.
- [x] **F4 hex-float literals — FIXED (2026-11-05)** — Temurin probe
      (corpus/HexFloatProbe.java, 97 cases, exact IEEE bits) pinned the
      grammar (0x REQUIRED, binary exponent REQUIRED, exponent sign
      accepted) and exact rounding (half-even on the exact binary value,
      overflow → Infinity at the max-finite + half-ulp boundary, subnormal
      grid, signed zero). Implementation: crates/j2r-runtime/src/hexfloat.rs
      — exact guard/sticky, f32 NEVER rounds through f64 (no double
      rounding). floatToIntBits/doubleToRawIntBits (+Double long variants)
      natives joined for bit-exact comparison. HexFloat differential
      fixture byte-identical vs Temurin; 8 unit tests; suite 95 → 97,
      tests 292 → 300.
- [x] **F5 arraycopy element-mismatch message — FIXED (2026-11-05)** —
      the JDK vector (corpus/ElementMismatchProbe.java + NestedElemProbe)
      pinned: `arraycopy: element type mismatch: can not cast one of the
      elements of {src component}[] to the type of the destination array,
      {dst element}` (JVM external names; src names the SOURCE ARRAY's
      component class). Confirmed: NO upfront component rejection for
      ref→ref; partial copy in destination order; nulls always copy; and
      in the `can not copy` message reference arrays print the kind name
      `object array` regardless of element class (the wave-1 dotted
      generalization was wrong — corrected). java/lang/Comparable joined
      the builtin hierarchy (boxed numbers + String + Enum implement it).
      ArraycopyMismatch fixture byte-identical vs Temurin.
- [x] **F6 enum classes end-to-end — LANDED (2026-11-05)** — Temurin probe
      (corpus/EnumProbe.java, 9 sections) pinned the full surface: ordinals/
      names/toString, values() array-clone semantics, valueOf both entry
      points (NPE `Name is null`; IAE `No enum constant <canonical name>.<name>`
      — nested enums print dotted: `No enum constant EnumProbe.Color.red`),
      compareTo/equals identity, getDeclaringClass, switch-over-enum (javac's
      $SwitchMap synthetic compiles through the GENERIC pipeline — no
      enum-specific codegen needed: the enum class itself, $VALUES included,
      flows through the ordinary path), enum fields/constructors/methods
      (Planet), static-init order (constants exist before other statics run).
      Runtime: java/lang/Enum natives gained getDeclaringClass + a REAL
      valueOf (reads the class's synthesized $VALUES pool through the
      reflection metadata: meta names the field, static_get reads the slot);
      reflect::canonical_name joined (unit-tested). EnumFixture differential
      fixture byte-identical vs Temurin; suite 97 → 98, tests 300 → 301.
- [x] **Skip/charset wave — LANDED (2026-11-05)** — two loud JZlib-wave
      gaps closed + one phase-2 candidate landed, all probe-first on
      Temurin 21.0.13:
      (1) InputStream.skip(J) DEFAULT (was a loud registered divergence):
      the JDK loop-read through a 2048-byte skip buffer via read([BII)I ON
      SELF; n<=0 returns 0; EOF-first read yields 0; partial drain yields
      the actual count. Alongside a REAL fix: InputStream.read([BII)I's
      JDK default was wrongly a loud "abstract" marker — it is CONCRETE in
      the JDK and a read()-only subclass (corpus/SkipProbe.java Tiny)
      inherits it; now the real bounds-checked loop.
      (2) ByteArrayInputStream LANDED (the JZlib wave-2 phase-2 candidate):
      slots (buf/pos/mark/count), read/read-range/skip/available/mark/
      reset/markSupported, close() is a NO-OP (pinned — use after close
      works), negative skip returns 0, skip past EOF returns the actual
      count. Codegen instantiation allowlist joined.
      (3) UTF-16 charset family (was UEE): UTF-16 (BOM+BE encode; decode
      FE FF→BE / FF FE→LE, none→BE default), UTF-16BE/LE raw, canonical
      names + JDK aliases, astral surrogate pairs, unpaired surrogate →
      U+FFFD (bytes match Temurin), dangling trailing byte → U+FFFD, exact
      UEE name for unknown charsets. AND a REAL loader bug found+fixed:
      modified UTF-8 legally encodes unpaired surrogates (JVMS 4.4.7) and
      j2r's Utf8 constant parser REJECTED the whole classfile — any javac
      file with a "\uD834"-style literal failed to load; now lossy-
      normalized to U+FFFD (the same substitution the JDK encoders emit,
      so every observable path still matches Temurin).
      Fixtures SkipDefault + Utf16 byte-identical vs Temurin; suite 98 →
      100. Standard charsets beyond the family (UTF-32, KOI8_R — Temurin
      SUCCEEDS silently) stay a registered loud gap.
- [x] **Stream wave 3: BufferedInputStream — LANDED (2026-11-05)** —
      corpus/BufferedProbe.java pins 26 rows on Temurin 21.0.13: the buffer
      state machine (inherited inner slot + buf/pos/count/markpos/
      marklimit), read-through-buffer + read([BII) across buffer edges
      (read1 with the >= marklimit direct bypass that invalidates the
      mark), the skip LAW the probe discovered — a skip over a PARTIALLY
      consumed buffer consumes ONLY the buffer (skip(20) over 7 buffered
      bytes returns 7, pos advances to count, next read refills from the
      inner stream) while a skip on an EMPTY buffer drains the inner
      stream clamped to its available() (skip(100) over 26 = 26) — mark/
      reset with readlimit growth (fill() shifts the marked region to the
      front even when the buffer is exhausted, re-anchoring markpos to 0;
      the first-draft fill reconstruction got this wrong and the probe
      caught it, twice), "Resetting to invalid mark", and the full "Stream
      closed" family (close() nulls in/buf; every getter then throws).
      Buffered fixture byte-identical vs Temurin; suite 100 → 101.
- [x] **Deep-arrays wave — LANDED (2026-11-05)** — the `[[[<prim>`
      verifier-depth gap CLOSED for depth 3 (corpus/DeepArraysProbe.java
      pinned a real 3D program FAILING TO VERIFY under the old one-level
      model — `expected Array(IntGroup) on the stack, found Ref`): the
      lattice now carries depth 1-3 (RefPrimArray + RefPrimArray2 variants,
      depth-aware array_elem_kind; depth >= 4 stays a registered loud
      limit). AND a SECOND unverified guess fixed by the same probe: the
      aastore ArrayStoreException message was "java.lang.X stored into Y"
      — a shape NO probe ever pinned; Temurin (corpus/AseProbe.java, 5
      shapes) prints the STORED object's external name ALONE (dotted plain
      classes, binary name keeps $, primitive arrays descriptor-verbatim
      `[[I`). DeepArrays fixture byte-identical vs Temurin; suite 101 → 102.
- [x] **Formatter wave — LANDED (2026-11-05)** — String.format +
      PrintStream.printf/format (the biggest remaining loud gap; corpus/
      FmtProbe.java pins 43 rows on Temurin 21.0.13): %s %S (width/
      precision/flags), %d (width, 0-pad after sign, +/space, comma
      grouping, left-justify, parenthesized negatives), %x %X %o (+ `#`,
      two's-complement negatives ffffff01), %f %e (width.precision, %e
      lowercase, HALF_UP on the SHORTEST decimal repr — the probe caught
      2.675 -> 2.68, proving the JDK rounds the shortest repr like
      Double.toString, NOT BigDecimal(double) which would give 2.67),
      %b %c %h, %%, %n, argument indexing (%2$s, %<). Varargs auto-boxing
      handled (numeric conversions auto-unbox Integer/Long/... heap
      objects; %s/%b keep the original — a boxed Boolean's truth is not
      its int). Exact errors: UnknownFormatConversionException
      "Conversion = 'q'", IllegalFormatConversionException
      "d != java.lang.String", MissingFormatArgumentException
      "Format specifier '%d'". 3 formatter unit tests + Fmt fixture
      byte-identical vs Temurin; suite 102 → 103, tests 301 → 304.
- [x] **Stream wave 4: PushbackInputStream — LANDED (2026-11-05)** — the
      LAST unmodelled stream builtin (corpus/PushbackProbe.java, 11 rows,
      byte-identical on the first run): unread(int)/([B)/([BII) with the
      LIFO re-read law, buffer exhaustion -> IOException "Push back buffer
      is full", skip consumes the pushback region FIRST then inner.skip,
      read([BII) drains the pushback then the inner stream (partial-pushback
      EOF still counts the pushed bytes), markSupported FALSE (reset falls
      through to the InputStream default "mark/reset not supported"),
      available() = pushback + inner, close() nulls in/buf ("Stream closed"
      family). Pushback fixture byte-identical vs Temurin; suite 103 → 104.
      THE STREAM BUILTINS ARE NOW COMPLETE: ByteArrayInputStream,
      ByteArrayOutputStream, BufferedInputStream, PushbackInputStream,
      Filter{In,Out}putStream, StringWriter/StringReader.
- [x] **Formatter residuals — LANDED (2026-11-06, the FmtResidualProbe
      wave)** — the registered loud gaps closed after a six-round Temurin
      probe that falsified three assumptions on its way in: (1) the
      uppercase-variant family is REAL and uppercases the WHOLE piece
      (%E of Infinity → `INFINITY`, %G of NaN → `NAN`, %A of null →
      `NULL`; S B H C X E G A all legal); (2) the ',' flag is legal ONLY
      on d/f/g — `%,x`/`%,X`/`%,o`/`%,e`/`%,a`/`%,s`/`%,c`/`%,b`/`%,h`
      throw FormatFlagsConversionMismatchException `Conversion = x,
      Flags = ,` at PARSE time, before argument resolution (the old code
      "supported" %,x grouping — wrong); (3) null prints `null`/`NULL`
      through EVERY conversion except %b (the old %d/%c paths threw IFCE
      and %h of null PANICKED). Plus: %a/%A hex float (13-nibble mantissa,
      trailing-zero strip, subnormal 0.x form with exp -1022, precision
      rounds HALF_UP on >= 8 and renormalizes carries `0x1.F8` →
      `0x1.0p1`, subnormal+precision renormalizes to the 1.x true-exp
      form), %g/%G (round to p-1 sci digits FIRST, then branch on the
      ROUNDED exponent — %.2G of 9.99 → `10`, %.3G of 999.99 stays
      scientific; NO trailing-zero stripping in the fixed branch —
      1e-4 → `0.000100000`; precision 0 → 1), the float family does NOT
      widen integral types (%f of Integer → IFCE `f != java.lang.Integer`
      — the old widening was an unpinned guess), '-' without width on
      numeric → MissingFormatWidthException with the raw spec, and a REAL
      render_e bug the %g work exposed: fixed-form reprs below 1.0
      ("0.001") got the wrong decimal exponent (%e of 0.5 printed
      `0.50000e+00`, now `5.000000e-01`). Fmt fixture extended in place
      (104 on disk, unchanged); suite counts: tests 304 → 307.
      Docs: README §runtime features + unsupported-list refresh.
- [x] **Wave residuals — CLOSED (2026-11-12, depth-5 closure wave, owner
      directive "why still not done" pulled it forward)** — array
      descriptor depth >= 5 no longer collapses loud: the enumerated
      RefPrimArray{,2,3} × {Int,Long,Float,Double} families (12 Copy
      variants; four more per future depth) were REPLACED by the decided
      recursive descriptor `Nest(Box<ElemKind>)` folded through
      array_elem_kind/elem_vty — ONE variant, ANY depth (`[[I` →
      Nest(IntGroup), `[[[I` → Nest(Nest(IntGroup)), every future depth
      one more fold). ElemKind/VTy dropped Copy (Clone retained; the
      62-error ripple was mechanical, compiler-guided, zero semantic
      changes — cfg_equiv + diff_suite prove the depth ≤ 4 decisions
      identical). The runtime needed NO change (array.rs's shape
      dispatch was always depth-agnostic — the lattice was the only
      limit). The new law test `depth5_recursive_descriptor_any_depth`
      pins: aaload on `[[[[[I` yields the exact 4-deep row shape
      satisfying a `([[[[I)V` call site; wrong-kind prim loads name the
      exact folded shape. Proof: codegen 110/110, cfg_equiv 109×3 +
      diff_suite 109 zero divergences, corpus 1/1, full battery
      **466/0**, fmt + clippy clean. The original deferral reason
      (sequenced after the typed-operand unification) had expired — the
      typed backend's G3.2 slices landed through the same seams the same
      day. Evidence: scripts/evidence/depth5_closure/
      recursive_descriptor_2026-11-12.txt; docs/CORPUS_WAVE.md gap
      entries updated.
- [x] **Corpus entry 2 LANDED (2026-11-05): JZlib** — ymnk/jzlib
      (BSD-style), 23 files / ~7.2k lines, a real DEFLATE/INFLATE
      implementation; vendored under corpus/jzlib/ + assembly script
      (scripts/build_corpus_jzlib.sh) + driver (public ZStream API
      levels 0-9 sampled, Z_FINISH/Z_NO_FLUSH round-trips, Adler32/CRC32
      checksums) — **output-identical vs Temurin 21.0.13 on all three
      channels** (locally; pinned permanently in corpus_suite.rs next to
      entry 1). PHASE 2 LANDED (skip/charset wave): the stream wrappers
      joined the differential — DeflaterOutputStream/InflaterInputStream,
      ZOutputStream/ZInputStream (getTotalIn/totalOut + JZlib's own
      decompressor-driven skip), full GZIP roundtrip (header parse +
      CRC32 trailer) over the landed ByteArrayInputStream/BAOS builtins,
      byte-identical vs Temurin (wrap-deflate n=72, zstream n=72/72/79,
      gzip n=84, zskip=20 → 'j'). The wave's real cost: the byte-stream
      JDK surface a real
      library drags in — Filter{In,Out}putStream hierarchy with named
      protected fields (new builtin-field resolution in codegen),
      ByteArrayOutputStream (JDK field model + growth contract),
      java/lang/Enum (javac's hand-rolled WrapperType; valueOf was a loud
      R2 marker here — made real in the F6 wave), array clone() on all array types (validate +
      dispatch), Object.clone default, verifier nested-primitive-array
      lattice ([[I elements are int[] values — aaload yields the
      receiver's shape, anewarray "[I" maps through the operand),
      String byte/charset natives (probe-pinned UTF-8/Latin-1/ASCII with
      JDK-exact error contracts), CloneNotSupportedException + InternalError
      hierarchy entries. Full catalog with the registered gaps:
      docs/CORPUS_WAVE.md §Wave 2. Local gates at this tree: 95/95
      differential (353s), cfg_equiv 95/95 sharded (48+47), corpus 2/2
      (31s), runtime 148 / codegen 81 green, fmt/clippy clean,
      gate-11 fixtures=95 tests=292 in sync. Note: the generated crate
      needs RUST_MIN_STACK=32MiB (rustc SIGSEGV otherwise — recorded in
      the runner and the wave doc).
- [~] **R2 metadata generator (post-M4 scope)** — reflection metadata
      retention so runtime string lookups resolve (closed-world design
      in docs/STRATEGIC_GAPS.md). **OPENING + SLICE 1 LANDED
      (2026-11-12, session 19, owner directive pulled it forward):**
      `Class.forName(String)` + the 3-arg overload now resolve against
      the closed world (the generated crate's metadata registry via
      ctx.class_info + the new `builtin_class_known` test) —
      Temurin-exact contracts (CNFE message IS the name; null → NPE
      "null"; "int" unresolvable; `[I`/`[Ljava.lang.String;` array
      forms), the inert `ClassLoader.getSystemClassLoader()` (the
      system loader IS the closed world), and the 110th fixture
      ReflectForName **byte-identical vs Temurin on the first
      differential run**. The census corrected the premise: the M3
      subset already retained the per-class metadata statics
      (UserClassInfo fields/methods tables) — forName was the loud
      string-lookup gap. Registered next slices: getDeclared
      Constructors; retained annotations; static proxies;
      `--reflection=all`. Evidence: scripts/evidence/r2_opening/
      forname_slice1_2026-11-12.txt.
      **SLICE 2 LANDED 2026-11-13** (evidence
      scripts/evidence/r2_slice2/static_field_modifiers_2026-11-13.txt):
      the register's "Field.get on a static returns null" premise was
      CORRECTED by the probe — the static path already worked; the real
      gaps were Class.getField (the public-member search law incl. the
      inherited-public shadow quirk, NSFE/null-NPE contracts; the
      interface-constants leg is a registered residual),
      Field/Method.getModifiers (the raw ACC_* masks now flow
      parse-time → FieldPlan/MethodPlan → UserField/UserMethod → the
      Field object's new slot 6 / Method's slot 4, both metadata
      emitters in lockstep), Field.getDeclaringClass, and the typed
      getter/setter family with the JDK's widening table + the
      Temurin-exact wrong-narrow message. New fixture ReflectStatic
      (the 113th) byte-identical vs Temurin; diff_suite 113/0,
      cfg_equiv 0, battery 468/0, fmt+clippy clean, doc counts
      113/468 in sync.
- [~] **R5 benchmark harness** — policy locked (benchmarks observe,
      never gate; no performance sentence without a number, a date, and
      a reproduction command). **Harness LANDED 2026-11-05** (owner
      request, pulled forward from "after the corpus wave"):
      scripts/bench_suite.py — per fixture, javac / java (C1-only,
      startup-scale honest) / j2r-gen (translate + offline cargo build
      of the generated crate, the gate-5 proven pipeline) / native run,
      median of 5 with 1 warmup, machine metadata, CSV append to
      scripts/bench_results.csv. First table recorded
      (docs/BENCHMARKS.md, 2026-11-05): native run 20-50x cheaper on
      startup-dominated micros, WaitNotify parity 0.78x (real sleep
      windows dominate — sanity check passed), compile-side ~10x slower
      than javac (vendored-runtime build dominates; translation is
      sub-second). Gap status: corpus bench rows TAKEN for minimal-json
      (0.04x), JAMA (0.09x), and stringsim (0.058 startup + 103.8x
      steady-state, 2026-11-08, docs/BENCHMARKS.md addenda 4-5);
      JZlibCorpus rowless BY RULE (release build is
      the registered rustc-memory finding). **FIRST STEADY-STATE ROW
      TAKEN (2026-11-06, owner asked "why is it so much slower" — the
      honest answer needed the number)**: new corpus stem JamaBench
      (scripts/build_corpus_jamabench.sh, real JAMA 1.0.3 code, 1200
      iterations of the full LU/QR/Cholesky/Eigen/SVD battery at
      40x40/40x24, fixed xorshift seed, 3-line deterministic output),
      harness extended with --fulljit (the DEFAULT full tiered JVM as a
      second java channel) and --native-reps/--native-warmups (the
      274s/run native channel cannot fit a sandbox session at reps=5 —
      recorded per row). Result: native 274.2s vs full-JIT 3.18s =
      **86.2x slower at steady state** (vs C1-only 1.31s = 208.8x),
      WHILE the ms-scale corpus rows stay 0.03-0.09x (native 12-33x
      ahead in wall time) — both true, different regimes, neither
      quotable without the other. Attribution: GC-deferred control run
      identical (273.4s vs 274.0s median) → the interpreter-shaped
      default emission (match-pc dispatch over Vec<Value> stacks,
      per-access runtime helpers, stringly-typed dispatch, per-block
      safepoint refresh), not the collector. Every channel byte-
      identical on JamaBench output (observed differential). Full table
      + protocol + JVM-side caveats (full-JIT slower than C1 at this
      run length on 2 cores): docs/BENCHMARKS.md §First steady-state
      comparison. Same-day addendum: the --emit-cfg speed question is
      ANSWERED — no steady-state gain (274.1s vs 274.0s, partial lift
      under an unchanged Vec<Value> operand model) → the registered
      performance lever is the typed-register/typed-operand emission
      model, with CFG structuring as prerequisite, not payload.
      Remaining: JZlib release row still blocked by the rustc-memory
      finding. CI evidence: run 42256131915 on 926b6ca fully green —
      27/27 check jobs success (incl. gate-3 107-fixture differential,
      gate-9 forced-GC solo leg, CodeQL / OSV / Scorecard).
- [x] **Math-semantics wave — LANDED (2026-11-07)** — the owner's second
      external conversion report (java-string-similarity, below) named
      Math.round as a silent-divergence class ("Math.round = floor(x+0.5),
      NOT f64::round" — its B18). The audit found the SAME class live in
      j2r's own natives: `n_math_round_d/f` used literal
      `floor(a + 0.5)` in binary FP, but every JDK since Java 7
      (JDK-8010430) computes the MATHEMATICAL floor(a + 1/2) by a bit
      trick. Temurin 21.0.13 ground truth first: round(0.49999999999999994)
      = 0 and round(0.49999997f) = 0 (a + 0.5 rounds up to 1.0 in binary
      FP, yet a is still below one half). New fixture **MathSemantics**
      (60 rows): the round_d/round_f edge battery (just-below-half at both
      arities, ties, the mathematical-integer cast branch above 2^52/2^23
      — round(4503599627370497.0) must return ...497 through the (long)
      cast, not ...498 via a tied a+0.5 — NaN -> 0, ±inf, Long/Int
      saturation), the min/max signed-zero law (min(+0,-0) = -0,
      max(-0,+0) = +0, bit-visible via doubleToLongBits), min/max
      NaN-ness under the canonicalizing bit-conversion contract, and
      abs's wrapping law (abs(Integer.MIN_VALUE) = MIN_VALUE,
      abs(-0.0) = +0.0). TDD evidence: the fixture ran RED first with
      EXACTLY the four predicted rows (rd_just_below_half 1 vs 0,
      rd_just_above_2p52_int ...498 vs ...497, and both rf rows) — no
      other row differed, which differentially CONFIRMS the rest of the
      audited surface was already Temurin-exact (min/max signed zeros and
      NaN-ness, abs, saturation). Fix: `n_math_round_d`/`n_math_round_f`
      (langext.rs) port OpenJDK 21's algorithm — shift = 1074 -
      biased_exp (double) / 149 - biased_exp (float); in [0, 63]/[0, 31]
      the significand bit trick computes the exact mathematical
      floor(a + 1/2); outside it Java's (long)/(int) cast applies, which
      is also Rust's `as` cast contract (NaN -> 0, saturating). GREEN:
      all 60 rows byte-identical vs Temurin. One documented non-goal:
      the RAW payload of a NaN returned by Math.min/max is a HotSpot
      C2-intrinsic artifact (measured on Temurin 21 x86-64:
      max(NaN,1.0) returns 1.0's bits via maxsd while min returns the
      NaN operand's bits — operand-order-dependent), so raw-payload
      equality is deliberately not pinned; the Java spec only requires
      NaN-ness, and our comparison contract (doubleToLongBits)
      canonicalizes. Suite 107 -> 108; guard 106 -> 107; README counts
      synced (108 programs, 108/108 cfg-identical; test count
      re-measured unchanged at 327 — the fixture adds no Rust tests);
      gate-11 green locally on a clean worktree at the commit state.
      CI evidence: (this commit).
- [x] **External converter-report ingestion — java-string-similarity
      (2026-11-07, owner-supplied evidence)** — the owner converted
      tdebatty/java-string-similarity v0.24 (2.5k-star, zero-dependency,
      17 classes + 5 interfaces, ~2k LOC) with cguz/java-to-rust
      (java2rust.jar, a regex-shaped SOURCE transpiler) and filed
      bugs.md + opt.md: 24 bugs (B01-B24), 309 rustc errors on the raw
      output, 7 silent-semantic classes (B16-B21) that only bit-level
      differential testing against the JVM exposed, and a 4,074-case
      doubleToLongBits corpus proving the manually fixed port bit-exact.
      Positioning recorded (honest): j2r is architecturally immune to
      the compile-blocker class (B01-B16-for) — JDK symbols resolve
      through the runtime native table (no java.* leaks), overloads
      resolve at source level before emission (no rename pass),
      interfaces/inheritance are structural (vtable traits, real
      hierarchies), arrays are runtime-allocated vec!s, the module tree
      + Cargo crate are emitted, and unsupported builtins fail LOUDLY at
      compile time (the documented JZlib/JAMA wave-gap pattern) instead
      of emitting broken code silently. Corpus evidence: JZlib (7.2k
      lines) and JAMA (3.3k lines) compile clean and run output-identical
      vs Temurin. The report's real value — the silent-semantic classes —
      mapped against pinned evidence: **B16** (UTF-16 code units, astral
      chars) = Utf16 fixture + the UTF-16 charset family (already pinned
      through the JZlib wave); **B17** (float must stay f32) = the
      bit-exact literal laws (FPProbe/HexFloat) + typed IR carrying
      Java's float/double distinction; **B18** (Math.round) = CLOSED by
      this wave; **B19** (implicit widening) = typed-IR types make the
      E0308 class unrepresentable; **B20** (HashMap iteration-order
      float aggregation) = HashMapOrder fixture + the org.json suite's
      order-emulation evidence; **B21** (regex \s ASCII-vs-Unicode) =
      java.util.regex is NOT a j2r builtin yet — unknown builtins fail
      loudly, and the ASCII-[ \t\n\x0B\f\r] law is REGISTERED to pin
      differentially when Pattern/Matcher lands (new known-gap line in
      this file). Adopted reaffirmations: bit-exact differential via
      doubleToLongBits is already the house contract; the report's
      "fixed-port corpus as permanent regression suite" is already the
      diff suite's standing policy (this wave is exactly that policy
      acting on external evidence, and it caught a real divergence the
      same day it was reported).
- [x] **Runtime-io wave — LANDED (2026-11-07)** — the org.json conversion
      report's three runtime findings, implemented as one wave:
      **StringWriter quadratic -> linear** (report finding #2): the old
      sw_append cloned the whole payload per append (clone -> push_str
      -> re-store = O(n^2) over n writes); it now appends IN PLACE via
      Arc::make_mut inside with_obj_mut — the same linear pattern
      StringBuilder already used in the same crate. **StringReader
      quadratic + scalar contract -> cursor model + UTF-16 units**
      (findings #1 and #3): the remaining-string payload model cloned
      the tail per read (O(n^2) drain) and read() returned Unicode
      scalar values on astral input; replaced with the
      ByteArrayInputStream precedent — own slots text/pos/mark/closed
      (4 own, 5 flattened with the inherited lock) — and read() now
      returns UTF-16 CODE UNITS: a supplementary-plane character
      arrives as its surrogate pair over two reads, mark/reset is
      unit-precise, retiring the module's scalar convention (a
      documented divergence from the JDK reader contract). The full
      JDK surface joined, with THREE post-Java-8 behavior changes the
      probe caught empirically before the natives shipped: skip(n)
      is clamped and a NEGATIVE n returns 0 WITHOUT throwing; reset()
      before any mark() resets to the BEGINNING (mark initializes to
      0 — the old IOException "Mark invalid" is gone from the modern
      JDK); bulk-read range errors carry the modern
      Objects.checkFromIndexRange message ("Range [off, off + len) out
      of bounds for length n") and are checked BEFORE the len==0
      short-circuit. Also: mark(readAheadLimit) still rejects negative
      limits ("Read-ahead limit < 0"); close() is idempotent and every
      subsequent op throws IOException "Stream closed" — except
      markSupported(), a plain `return true` with no open check (its
      JDK signature declares no checked exception; javac rejected the
      throws-assumption mid-wave, which is exactly what the probe rows
      are for). StringWriter.write(int) now keeps only the 16
      low-order bits (the JDK's `(char) c` law — the old code turned
      write(0x1F340) into the astral scalar instead of the single unit
      0xF340); lone-surrogate writes degrade to U+FFFD (Rust-String
      payload limit, documented in io.rs, deliberately unpinned).
      New fixture **StringIoSemantics** — byte-identical vs Temurin
      21.0.13 on the first full pass after the empirical-law
      alignment; cfg-equivalent in all three emission modes. The real
      StringReader consumers stayed byte-identical: corpus_suite
      (minimal-json/JZlib/JAMA) green, Buffered/SkipDefault/Pushback/
      Utf16 neighbors green; builtin.rs StringReader own-slot row
      1 -> 4 with the corpus_wave1_io_hierarchy unit test co-updated
      (total 5 flattened); runtime 163/163. Suite 108 -> 109; guard
      107 -> 108; README counts synced (109 programs, 109/109
      cfg-identical). CI evidence: (this commit).
- [x] **Corpus wave 4: java-string-similarity — LANDED (2026-11-07)** —
      the fourth real-world corpus entry: tdebatty/java-string-similarity
      v0.24 (Apache-2.0, 15 algorithm classes + 5 interfaces +
      experimental/Sift4, ~2k LOC) — the SAME library the owner's
      external report converted with cguz/java-to-rust into 27 broken
      .rs files and 309 rustc errors. Through j2r it compiles clean and
      runs OUTPUT-IDENTICAL vs Temurin 21.0.13 (driver
      StringsSimCorpus, vendored corpus/stringsim/, merged by
      scripts/build_corpus_stringsim.sh; corpus_suite 3 -> 4 entries,
      70.9s local). The driver pins the report's silent-semantic rows
      differentially: Levenshtein(emoji, "x") = 2.0 (UTF-16 code-unit
      law), JaroWinkler's f32-fidelity bits 0.996666669845581 (the
      report's B17), the NBSP-survives-whitespace-collapse profile
      (B21/ASCII-\\s), entrySet-ordered profile prints (HashMap order
      law), Sift4's method-local class + LinkedList, interface dispatch
      through user impls, and the message-exact null/k<=0 error paths.
      What the wave closed (each by a loud failure or the differential):
      Map.entrySet + Map.Entry (the JDK's iterator hands out the
      internal Node as Map.Entry; rows at the concrete AND interface
      levels; the Node-implements-Map$Entry subtype law for the
      enhanced-for's checkcast), Collections.unmodifiableMap (read-
      forwarding wrapper, message-less UnsupportedOperationException on
      put), Set/HashSet.addAll (Collection-dispatch iteration — and a
      caught-in-the-act L3 violation: the first draft nested heap guards
      and deadlocked at 0% CPU; flattened per the module lock law),
      the java/util/Map interface-level entrySet row, and
      java.util.regex LANDED NARROW — Pattern.compile("\\s+") +
      Matcher.replaceAll with the ASCII-[ \\t\\n\\x0B\\f\\r] law hardcoded
      (crates/j2r-runtime/src/regex.rs), the registered gap's landing
      condition, NBSP pinned differentially, every other pattern loud.
      Bench rows NOT taken this wave (correctness first; the JamaBench
      steady-state protocol applies when they are). CI evidence: (this
      commit).
- [x] **Wave-4 verification + two defects closed (2026-11-08)** — the
      landing session died before the suite battery ran; this session ran
      it and the cfg_equiv harness caught a LIVE silent-semantic bug in
      the new typed-operand backend: BigJson `Parser.skipWs()V` panicked
      "expected int value, got Void" (exit 101) — the typed preamble read
      EVERY entry-live local from the unmarshal Vec, but only incoming
      argument slots hold values there; skipWs's loop-live `c` local is
      not an argument, so `v1e0 = __loc_tmp[1].expect_int()` read Void.
      Fix (typed.rs): arg-slot count computed from the descriptor
      (parse_method_desc + type_slots, slot-widened, +1 for `this`) and
      BOTH the Var::Cell and Var::Typed preamble branches now read
      __loc_tmp only for j < arg_slots; non-arg locals take the JVM entry
      default (verifier law: store before read). type_slots re-exported
      from the runtime root. Second fix: the wave made cfg_equiv 3× the
      work (base+cfg+typed) with per-fixture cold crate dirs → ~20 s per
      fixture, ~23 min per suite (three 10-min tool windows died on it);
      now stable per-mode crate dirs + ONE shared CARGO_TARGET_DIR +
      content-idempotent runtime vendoring (emit.rs copy_file_idempotent:
      copy only on content change, prune stale upstream files, mtimes —
      hence cargo fingerprints — survive regeneration; self-healing on
      partial writes). Measured: 19.5 s → 4.9 s per fixture warm,
      shard ≈ 294 s. Full battery green: cfg_equiv 109 × {base,cfg,typed}
      zero divergences (shards 0/2 293.6 s, 1/2 296.1 s), diff_suite
      109 fixtures (368.4 s + 360.7 s), corpus_suite 4/4 vs Temurin
      (69.9 s), runtime 163/163, codegen 98+12+1, jar 3, lambda 2,
      soe 1. Shrink guard 108 = 109 on disk − 1; README counts
      untouched (109 programs, 109/109). CI evidence: (this commit).
- [x] **Typed-backend payload measurement (2026-11-08)** — the registered
      lever's first slice, measured honestly and the answer is NO
      steady-state gain yet: JamaBench controlled same-session pair —
      default 358.7 s (fresh harness control row; the 274.0 s baseline of
      2026-11-06 is 1.31x stale machine-state, which is WHY the control
      exists) vs `--emit-typed` 355.9 s (ratio 0.992), both byte-
      identical to the JVM. Coverage is real: 75/106 methods typed,
      `Matrix.times` (the hot kernel) typed — but the slice types LOCALS
      only; the operand stack still runs on `Value` (2,457 `expect_*`
      sites, 367 per-block safepoints, obj_get/obj_set + invoke_virtual
      per access). Fourth coverage point with the same verdict as the
      three CFG addenda: operand EXTRACTION, not control-flow shape or
      local boxing, is the residual cost; the lever's remaining
      prerequisite is operand-stack/register unification
      (docs/TYPED_REGISTER_READINESS.md §5). Harness extended with
      `--emit-typed` passthrough + `[typed]` CSV row suffix; full table +
      repro in docs/BENCHMARKS.md addendum 3. CI evidence: (this commit).
- [x] **External report v1.1 audit (2026-11-08, owner directive: check
      the claims)** — the owner's THIRD external conversion run ingested:
      java-string-similarity v2.0.0 (info.debatty, 17 files / 1,970 LOC)
      through cguz/java2rust — raw output 432 rustc errors on 18 files
      (bugs_v1.1 B01-B20), hand-fixed port verified bit-exact on 14
      workloads / ~1.6M calls (results_v1.1), improvement plan opt_v1.1
      (O1-O17). Reconciliation against j2r — the report characterizes a
      SOURCE-level transpiler emitting Java-shaped Rust; j2r compiles
      .class BYTECODE through a typed IR + verifier, so the classes are
      either unrepresentable here or already pinned differentially:
      B01/B02 (bare null/throw emitted) + B12 (for-let-in hybrid) + B16
      (token soup) = parse-model artifacts of a text-to-text pass, no
      counterpart (typed IR, verifier store-before-read law); B03
      (implements -> #[derive]) = j2r emits real impls (interface
      dispatch through user impls pinned by corpus wave 4); B04
      (Serializable supertrait) = marker interfaces resolve to no-ops or
      loud unknown-class errors, never bare identifiers; B05/B13 (JDK
      API passed through) = the stringsim corpus ITSELF compiles clean
      through j2r (HashMap profiles, LinkedList, String API, entrySet —
      wave-4 evidence); B06 (same-package use) = per-class modules from
      bytecode references, pinned by every multi-class corpus; B07/B08
      (inheritance erased, parent ctor dropped = SILENT k-uninitialized)
      = <init> bytecode CONTAINS the super call — the silent class
      cannot exist; vtable dispatch + ShingleBased subclasses pinned;
      B09 (static prefix dropped) = invokestatic carries the class ref;
      B10 (overload collision) = bytecode (name,descriptor) pairs are
      distinct, distance(s1,s2) vs distance(s1,s2,int) coexist in the
      corpus driver; B11 (new T[n] -> const-size array) = anewarray
      carries a runtime length -> heap arrays (DeepArraysProbe pins
      depth 3/4); B14 (int literal where f64) = typed IR makes E0308
      unrepresentable (same finding as the v0.24 mapping); B15 (&String
      vs &str) = generated-crate-internal ABI, ergonomics of hand-
      written callers is a non-goal (NAIL); B17 (comment mojibake) = j2r
      reads binary class files (modified UTF-8, JVMS 4.4.7, pinned by
      Utf16Probe) — no source-charset round-trip exists (NAIL); B18
      (CLI -o silently ignored) = checked on j2r itself: --out is
      exercised by EVERY corpus build (build_corpus_*.sh + bench_suite
      pass --out work/crate and consume the crate there) — flag honored
      (NAIL with evidence); B19 (anonymous classes) = Enclosing$1 are
      real classes at bytecode level with real implements; B20 (println
      &f64) = println(Object) goes through runtime boxing +
      String.valueOf (Fmt fixtures). Carries REGISTERED from opt_v1.1:
      O14-O16 perf evidence (short-string per-call allocation vs
      charAt; one-String-per-shingle vs borrow-slices; UNBOXED primitive
      map values = the port's 2.34x W7b win) feeds the operand-stack
      unification lever — collection VALUE boxing in j2r's generated
      maps is a real question to measure once the typed backend lands
      (added to docs/TYPED_REGISTER_READINESS.md scope); O17 numeric-
      fidelity policy = already the house law (f32 stays f32,
      bit-exactness pinned by FPProbe/HexFloat + wave-4 rows). The
      14-workload table itself is the hand-port's evidence, not j2r's;
      the j2r-side action it names — the missing StringsSimCorpus bench
      row — is TAKEN this session (R5 entry + docs/BENCHMARKS.md
      addendum 4).
- [x] **AI-slop scanner audit (2026-11-08, owner directive "ai slop
      scanner said these check the claims")** — every claim checked
      against the tree; ONE true positive fixed, the rest verified as
      corpus-by-design or upstream-verbatim:
      * swallowed-exception x6 corpus/jzlib/{ZOutputStream,GZIPInputStream,
        Deflate}.java — UPSTREAM-VERBATIM jzlib bodies (the vendored
        library the differential pins: Deflate.clone's
        catch(CloneNotSupportedException){}, GZIPInputStream's
        read-swallow-then-break-on--1 are the upstream control flow);
        "fixing" them would stop pinning the library they exist to pin.
        NO ACTION (fidelity).
      * swallowed-exception x3 corpus/JZlibCorpus.java:2051/2852/6703 —
        the SAME upstream fragments inside the mechanically merged
        driver (build_corpus_jzlib.sh must not hand-edit library
        bodies). NO ACTION (fidelity).
      * scripts/bench_suite.py:83 `except Exception: pass` — TRUE
        POSITIVE, fixed: narrowed to `except OSError` (the only failure
        mode of Path.read_text) with the rationale inline, so a
        programming error in report metadata can no longer hide. (The
        scanner's "bare except" wording was imprecise — it was
        `except Exception` — the narrowing is right regardless.)
      * function-too-long x8 + file-too-large + deep-nesting x4
        (JZlibCorpus.java 7,259 lines, JamaBench.java 3,363 lines) —
        corpus drivers + vendored upstream, NOT application code: the
        self-contained single-file driver is the differential design
        (mechanical merge of vendored library + deterministic
        workloads), and upstream jzlib's own deflate() is famously
        huge; an 80-line application limit misfires on every corpus
        file by construction. NO ACTION (by-design).
- [x] **Stringsim steady-state wave + two JDK/codegen gaps closed
      (2026-11-08)** — building the stringsim bench stem found the gaps
      the honest way (the compiler refused loudly, never silently), and
      each one closed with a differential pin:
      * **Math.floorDiv(long,int) / Math.floorMod(long,int)** — the JDK 9+
        mixed overloads were absent (first loud failure: the new driver's
        PRNG). OpenJDK's asymmetric shape preserved EXACTLY: floorDiv
        returns LONG (widen-and-delegate) but floorMod returns INT
        (javap-pinned descriptor `(JI)I`; `(int) floorMod(x, (long) y)` —
        the narrowing is always exact for an int divisor). New fixture
        rows in MathExact (mixed signs, negative divisors, Long.MIN_VALUE
        numerator, Long.MAX_VALUE/Integer.MIN_VALUE, zero-divisor
        messages e9/e10, and the floorDiv*y + floorMod == x identity);
        MathExact's old multiplyExact row — previously pinned as a
        byte-identical CRASH through the normalized-stderr channel — is
        now a caught-message row (e11), which pins the same "integer
        overflow" text on stdout, a stricter channel, and lets the new
        rows actually run. Harness-green: J2R_DIFF_FIXTURES=MathExact
        byte-identical vs Temurin 21.0.13.
      * **ANewArray accepts builtin INTERFACE element types** — the
        second loud failure: `new Map[1000]` ("unknown array element
        type java/util/Map"). The F4/F5 wave had registered Comparable
        by hand in builtin_parent_of; the general law replaces the
        special case (the element check now also accepts
        builtin_is_interface) — interface-typed arrays are ordinary ref
        arrays, and aastore's ASE check already walks the interface
        subtype graph, so stores are checked at full strength of the
        type law. Exercised for real by the bench driver's Map[] of
        1,000 unmodifiable profiles.
      * **StringsSimBench** (corpus stem, scripts/
        build_corpus_stringsimbench.sh) — the same library merge as
        StringsSimCorpus + a loop-sized xorshift64* driver mirroring the
        owner's results_v1.1 shapes (W1 200k Levenshtein short, W5 500k
        JaroWinkler, W7a 1,000 getProfile docs, W7b 10k Cosine over
        PREBUILT profiles — float aggregation deliberately under HashMap
        iteration, the order law — W8 10k NGram(2)+(4) DNA). Observed
        differential BEFORE any row: C1, full-tiered, and native all
        print byte-identical checksums (w7b = 4024.3242675735173 with
        HashMap-order float aggregation — bit-exact).
      * **Two bench rows TAKEN** (docs/BENCHMARKS.md addenda 4+5):
        StringsSimCorpus 0.058 nat/C1 (startup-dominated regime, the
        wave-4 deferral closed) and StringsSimBench nat/full **103.8x**
        nat/C1 73.1x (native 281.3s vs full-JIT 2.71s) — the SECOND
        family-scale steady-state baseline, landing in the JamaBench
        regime (98x) on completely different kernels; combined with the
        owner's hand-port at 0.5-2.3x of the JVM, the ~100x is the
        interpreter-shaped emission model, not translation — the
        typed-operand unification lever's evidence base grows to two
        families.
- [x] **Operand-stack/register unification — the registered lever's
      second slice — LANDED (2026-11-08, owner directive "continue never
      stop")** — the readiness §5 GO's step-2 continuation, behind the
      same `--emit-typed` flag with per-method fallback unchanged. The
      evidence that motivated it: the typed-locals slice measured flat
      (0.992) because the STACK positions demote to Value cells almost
      everywhere in the kernels — branch-merging unions kinds across the
      loop span (LU/EVD: 1,855 expect sites on 1 typed stack var) — so
      the payload had to decouple stack values from slot-index storage.
      Model: within one block, a stack value keeps a Rust EXPRESSION
      (the def's VERIFIED kind decides the read law; the slot's
      Cell-or-Typed kind only decides the materialization write);
      materialization at exactly three sites — GC refresh, a shuffle
      (dup family; a def may be read twice), the block handoff — so
      GC-root parity is by construction (the refresh reads the same slot
      vars at the same sites with the same values). Census: 5,530
      symbolic pushes elided in the JAMA crate, expect sites 2,457 →
      1,596 (EVD 1,855 → 1,020). Measurement (the registered protocol,
      same-session pair, byte-identical output on all runs): default
      273.8s vs typed 262.4s = **0.958 — the FIRST nonzero steady-state
      movement across five coverage points** (addendum 6). THREE real
      defects the proof harness caught on the way, each the exact class
      cfg_equiv exists for:
      * `desc_kind_of` keyed on the descriptor's LAST byte — `()[[D`
        typed as a DOUBLE not a reference; latent since the first typed
        slice (Cell targets carry no kind assert), loud the moment the
        unification hoisted the expect to the def (LU.getArray panicked
        `expected double value, got Ref` at runtime). Fixed: first-byte
        parse (B/C/S/Z → Int, J/F/D, `[`/`L` → Ref).
      * composed results kept strings binding the shared `__a`/`__b` —
        a pending entry that survives past the NEXT op's rebinding
        re-evaluates against wrong values; every composed result now
        materializes into a method-unique `__t{n}` temp (and `__v`/`__r`
        reuse is banned: `foo(f(x)); g(y); use(x_result)`).
      * the load-snapshot law: javac's post-increment lowering
        (`iload 7; iinc 7,1; use-the-old-7`) mutates the local BEHIND a
        leaf capture — loads snapshot into a def-time temp like every
        other def.
      Proof battery at this tree: cfg_equiv 109 × {base,cfg,typed}
      zero divergences (345.6s + 380.2s clean-tree re-run), diff_suite
      109 vs Temurin (463.7s), corpus_suite 4/4 (47.9s), runtime 163,
      codegen 98, jar/lambda/soe green, fmt/clippy -D warnings clean.
      Debug scaffolding (bisection env switches, store tracing) removed
      before landing. Next named prerequisite layer: the block-join/
      entry-var model (branch-live values in successor-entry variables
      instead of slot indices) — register unchanged in
      docs/TYPED_REGISTER_READINESS.md §5; collection-VALUE-boxing
      measurement sequenced after it. CI evidence: (this commit).
- [x] **Collection VALUE-boxing measurement — readiness §5 step 3 —
      TAKEN (2026-11-08, owner directive "continue never stop")** — the
      owner's opt_v1.1 evidence registered the question (their hand-
      port's HashMap<String,i32> beat the JVM's boxed
      HashMap<String,Integer> 2.34x on W7b); after the unification
      slices landed, the honest number is now on the board. Instrument:
      a corpus stem PAIR with identical arithmetic —
      corpus/BoxingBoxedBench.java (Map<String,Double>, two boxed map
      ops per iteration) vs corpus/BoxingPrimBench.java (a double[]
      indexed by the loop id) — same xorshift64* stream, same order per
      key, checksums in FIXED key order; **four-way differential:
      byte-identical output on both stems on both channels** (a real
      fixture pair, not just a bench pair). CSV rows (reps=3,
      native-reps=3): boxed 7387.2 ms / prim 2657.1 ms native,
      93.1 / 52.7 ms full-JIT → **j2r container tax 2.78x, JVM
      container tax 1.77x** (the JIT scalar-replaces most boxes; j2r
      has MORE to gain from a primitive-specialized map path than the
      JVM does — 2.78x is the ceiling the specialization lever recovers
      on this family); j2r/JVM 79.4x boxed vs 50.4x specialized (the
      ABSOLUTE steady-state gap shrinks 36% from the container swap
      alone — j2r's map ops are dispatch+boxing-heavy, its array ops
      helper-light). Per-pair: j2r ≈ 1.82 µs/pair extra container cost
      vs the JVM ≈ 16 ns — ~115x, the steady-state order. REGISTERED
      (not scheduled): the primitive-specialized map path composes with
      the operand/register lever (the unbox moves to the native
      boundary) and lands after the block-join/entry-var layer.
      docs/BENCHMARKS.md addendum 7 has the full table + protocol. CI
      evidence: (this commit).
- [x] **Typed-payload array representation — the boxing wave LANDED
      (2026-11-08, owner directive "continue never stop")** — addendum
      7's census showed arrays pay the boxing disease HARDER than maps
      (334 `array_load` unwraps in the typed JAMA crate; a `Value` enum
      per element = clone + expect per access + ~2x memory). Primitive
      arrays now carry a TYPED PAYLOAD: `JObj.prim: Option<PrimData>`
      (I/J/F/D; the Int-group companions share the i32 payload),
      `slots` holds exactly the length (array_length + the GC child
      walk stay uniform; prim payloads have no ref children — the
      collector scans less). New RAW accessors
      (`array_load_i/j/f/d`, `array_store_*_raw`: NPE + bounds identical,
      then the bare number — no `Value` construction); the generic
      accessors dispatch (fresh wrap instead of a boxed-slot clone — the
      DEFAULT emission benefits with zero emitter change); the
      Int-group store narrows by the array's own descriptor (the JVMS
      bastore/castore/sastore law, defensive — javac always pre-narrows,
      the corpus cannot distinguish); `object_shallow_clone` carries the
      payload; 26 JObj construction sites updated (mechanical,
      compiler-guided). Typed-backend xXaload/xXastore call the raw
      accessors (536 raw sites in the JAMA crate); reference stores keep
      the ASE-checking boxed path. Proof: cfg_equiv 109 × 3 modes zero
      divergences (361.7s), diff_suite 109 vs Temurin (473.3s),
      corpus_suite 4/4, runtime 163 + codegen 98, fmt/clippy clean.
      Measurement (addendum 8): default 274.1s vs typed+raw 256.5s =
      **0.936 — 6.4% cumulative lever movement** (0.992 → 0.958 →
      0.936); expect census 2,457 → 873. Remaining named prerequisite:
      the block-join/entry-var layer at its MEASURED ceiling (~140
      slot-var sites — small), then dispatch/obj-helper costs. CI
      evidence: (this commit).

      * **Gate repairs found on the way in**: main was fmt-red
        (cargo fmt applied — zero-tolerance gate-1) and clippy-red
        under -D warnings (4 sites: my unused binding + 3 pre-existing
        in readiness.rs/typed.rs, all mechanical: enumerate-instead-of-
        counter, no-op usize cast, &mut Vec -> &mut [usize]; the CLI's
        cmd_build carries a documented too_many_arguments allow — 8 args
        is the build command's natural surface). Local: fmt clean,
        clippy --all-targets --all-features -D warnings clean, runtime
        163/163, codegen 98+12+1+1, doc counts 109/338 in sync. CI
        evidence: (this commit).

### M5 Minecraft readiness (was M4; no compatibility claim without evidence)

**STARTED 2026-11-08** — plan: docs/M5_PLAN.md rev 0 (§6 = 23-task
register T01–T23; wave order: 0 census → 1 scale/incremental → 2 JNI →
3 mappings → 4 mixins → 5 inline caches). Audit facts verified in
M5_PLAN §1: closure worklist is quadratic (codegen lib.rs:492–580),
whole-program = one crate (lib.rs:1308), rustc SIGKILLs the JZlib
release mega-functions (BENCHMARKS.md), zero JNI / zero mappings surface.

Wave 0 — measurement & baseline:
- [x] **T01** scale census LANDED (2026-11-08, scripts/m5_farm_gen.py +
      m5_census.sh + m5_census.csv; Temurin 21.0.13+9, stable rustc,
      4 GB sandbox). Stage curve (wall s / peak RSS MB):
      N=100 → javac 1.83/105, j2r compile **0.06/13**, cargo debug
      11.99/655 · N=250 → 2.31/119, **0.09/18**, 26.29/1256 ·
      N=500 → 3.71/162, **0.22/28**, 45.55/2106 · N=1000 → 4.67/188,
      **0.40/49**, cargo **SIGKILL** (peak 3.66 GB, 48 s in).
      Isolation probes: no-Main probe (giant 639 KB farm_Main stubbed
      out) still SIGKILLs at 3.64 GB — the mega-function is NOT the
      driver; metadata-only rustc on the full 1k-class lib exceeds
      600 s / OOMs — the wall is the pre-LLVM metadata/MIR phase on
      ~3 MB expanded source per class. **The named cliff: rustc burns
      ~4.2 MB per emitted class (linear: +2.1 GB per 500 classes) — the
      4 GB box caps ONE rustc process at ~900 classes; 70k classes
      would need ~300 GB in a single process.** javac: 0.5 MB/class;
      j2r translation: 0.4 ms + 48 KB/class, flat. Translation is not
      the bottleneck — rustc's view of our emission is (M5_PLAN fact 4
      confirmed with numbers). Census CSV: scripts/m5_census.csv.
- [x] **T02** census-driven scale design note LANDED (docs/M5_SCALE.md):
      crate-partitioned emission chosen (link-table partitioning, K=500
      classes/crate — the measured 2.1 GB point; cross-partition calls
      through a registration table so class-web cycles stop mattering;
      same-partition calls stay direct paths). Order fixed by the curve:
      T05 partitioning first (rustc wall), T03 closure cache second
      (real but behind the wall: 0.40 s at 1k vs cargo death), T06/T07
      manifest rides the partitioning. Rejected with reasons: tuned-down
      single crate (measured: still 3.68 GB at 1k), reference-DAG
      layering (the class web is one SCC — collapses to one crate).

Waves 1–5 (register with acceptance bars lives in M5_PLAN §6):
- [x] **T05** crate-partitioned emission LANDED (2026-11-08, per
      M5_SCALE.md §3). `--partition-classes K` on `j2r compile`/`build`
      (default OFF — single-crate path byte-identical, proven by
      cfg_equiv 109×3 modes zero divergences 371.6s + diff_suite 109
      fixtures 488.3s + corpus_suite + unit batteries post-refactor).
      Workspace: `support` crate (id-keyed statics pool verbatim +
      global link table + dispatch slot), `p0..pN` (K name-sorted
      classes each; same-partition calls keep the direct path;
      cross-partition calls go through `support::link::call(GID,…)`
      with deterministic global method ids; cross-partition clinit
      fn pointers via const-generic `tramp::<GID>`), `app` (composite
      Dispatch routed by PART_START partition_point + link registration
      + install sequence before any Java code). **One deadlock found
      and fixed on the way: the link table was a Mutex held across the
      callee — a nested cross-partition call self-deadlocked (0 CPU,
      sleeping); now a OnceLock, fn pointer copied out before the
      call.** Evidence (4 GB box): N=1000 k=500 -j1 → build 101.99s /
      **1.91 GB peak** (single-crate SIGKILLed at 3.66 GB on the same
      input), run BYTE-IDENTICAL vs Temurin; N=5000 k=300 -j2 → javac
      13.23s, emit 7.98s, build 282.72s / **1.89 GB peak** (single-crate
      extrapolation ~21 GB — impossible here), run BYTE-IDENTICAL vs
      Temurin. CSV: scripts/m5_census_partitioned.csv.
- [x] **T04** partition differential LANDED (2026-11-08): the farm was
      BYTE-IDENTICAL vs Temurin at N=250 (k=100), N=1000 (k=500), N=5000
      (k=300) during T05; the seams are now REGRESSION-GUARDED in CI —
      new tests/partition_suite.rs runs 6 multi-class fixtures
      (Cycles, ForEachColl, LambdaInCollections, SingletonFactory,
      CyclicInit, StringIoSemantics) through `j2r test
      --partition-classes 2` (forced-tiny budget → every web spans 2+
      crates: cross-partition direct calls, cross-partition <clinit>
      fn pointers via tramp, the shared statics pool, the composite
      Dispatch) with the same three-channel comparison as the
      differential suite. 25.9s locally, green; doc counts 109/339.
- [x] **T03** closure cache LANDED (2026-11-08): `ClassPlan.refs` — the
      referenced-user-class set extracted ONCE at parse time; the closure
      worklist walks the cached delta sets and the referer lookup reads
      the cache instead of re-walking every class's constant pool on
      every round (the measured quadratic term). Emission order
      unchanged (same BTreeSet, same class order — deterministic).
      Measured compile-stage (closure+parse+emit) wall: N=1000
      0.40s → **0.29s**, N=5000 7.98s → **2.02s (−75%)** — the stage is
      near-linear now (5× classes → 7× time, was superlinear). Gates:
      fmt/clippy clean, codegen lib 98/98, runtime 163/163,
      corpus_suite 49.9s green (the jar-classpath closure path
      end-to-end); full battery rides CI on push.
- [x] **T06/T07 build manifest LANDED** (2026-11-09): `out/j2r-manifest.tsv`
      — per-class input-hash → emitted-hash records under a global gate
      (emitter version, emit_cfg, emit_typed, signature-surface hash,
      partition layout hash). Soundness: a class's emission is a pure
      function of its own bytecode + every class's resolution surface +
      the global slot/link-table allocation + the options — never other
      classes' method *bodies* — so a verified-current class is reused
      byte-for-byte and its rendering skipped; every file (including the
      vendored runtime and the workspace glue) is written only when its
      bytes change, which keeps cargo's fingerprints valid and turns a
      no-op rebuild into hash checks. Corrupt/hand-edited manifest falls
      back to a full re-emit (loud, never stale). Single-crate AND
      partitioned paths both maintain it. 10 manifest integration tests +
      3 sha256 unit tests (FIPS vectors). Measured (401-class calibration
      crate, this box): full `j2r build --release` 123.9 s → no-op
      **0.46 s** (268×), binary re-verified correct; body-only edit of one
      class re-emits exactly that class (400 reused / 1 re-emitted,
      emission 112 ms) and — in partitioned mode — cargo then recompiles
      only that class's part crate (the other parts' files keep their
      mtimes). Also fixed on the way in: `emit_app` double-wrote
      `app/Cargo.toml` (short placeholder via write_member_cargo, then the
      full file) — every workspace re-emit churned that mtime, which would
      have rebuilt the app crate on every no-op. Found and fixed during
      the farm measurement: the reuse gate originally keyed on
      `opts.inputs.len()` and broke for `--classpath` builds (empty input
      list ⇒ gate refused every run); input hashes now live on
      `ClassPlan.input_sha` (SHA-256 over the exact bytes `parse_and_add`
      loaded — explicit inputs and classpath closure alike), so both load
      paths are covered by one mechanism. **Farm measurement (1,003-class
      M5 farm, k=200, 2 cores/4 GB):** full `j2r build --release` 357.5 s
      → no-op **0.535 s (668×)** with all 1,003 classes reused and 0 files
      written; a one-class body edit re-emits exactly that class and
      rebuilds in **71.0 s** (5× — cargo recompiles only the edited
      class's partition + app; the other partitions' files keep their
      mtimes), output byte-identical to Temurin on all four printed
      channels.
- [x] **incremental compilation for 70k+ classes — wave 1 CLOSED**
      (2026-11-09): T05 partitioning + T03 closure cache + T06/T07 build
      manifest, now confirmed at the 20k census jar (N=20000, k=250, 80
      part crates + support + app, cargo -j2, dev/debug=0 — disk-forced,
      recorded): cold end-to-end ≈ 17.1 min (javac 26.15 s / 1.55 GB;
      emit 34.12 s / 858 MB; cargo ≈ 967 s; peak rustc 3.49 GB = the
      app-bin Dispatch composite); no-op rebuild 38.8 s with manifest
      20,003 reused / 0 written + cargo 0.28 s; one-body-edit 55.3 s
      with 20,002 reused / 1 re-emitted / 1 written (only its partition
      + app rebuilt); **parity PASS — stdout/stderr/exit identical,
      0 == 0** vs Temurin; binary ≈ 1.01 GiB. Findings registered
      (console.md F1–F5): F1 emit-stage super-linearity at 20k (needs
      profile), F2 no-op emit is SHA-256-rehash-bound (mtime/size fast
      path), F3 app/Dispatch composite = rustc near-wall (split-table
      emission, already-registered follow-up family), F4 release-20k
      trio, F5 artifact sizes for the download-size story. Evidence:
      work farm20k run, scripts/m5_20k_confirm.sh, console.md
      §2026-11-09 08:34–09:25.
- [x] JNI bridge for LWJGL/GLFW/OpenAL (R3, safe-Rust design; §3,
      T08–T13, C-harness parity is the blocker's evidence bar)
      — **WAVE 2 (JNI) CLOSED 2026-11-10** — T08–T13 all COMPLETE
      — T08 LANDED (2026-11-09): `j2r-runtime::jni` tables generated
      from the Temurin 21 jni.h (235 slots, const offset_of! assertions
      per slot, loud abort stubs, 7 unit tests; findings F-J1
      pointer-out-slots-vs-forbid(unsafe_code) and F-J2 varargs-at-A-form
      registered in jni/mod.rs for T09/T11); vendoring made
      directory-recursive (suite-caught e2e regression).
      — T09 LANDED (2026-11-09): the audited FFI crate `j2r-jni-ffi`
      (6 primitives, 5 unsafe occurrences, pin ≤ 12, gate-10 amended in
      the same commit) + the safe generation-pinned handle-table bridge;
      379 green.
      — T10 COMPLETE (2026-11-10): the five mUTF-8 string slots wired
      through the generator real-slot registry (FFI 6 → 7,
      `write_u8_out`); slice 3a host dispatch + jclass mirror + jfieldID
      registry + 36 accessors; slice 3b the 46 array slots (91 real
      slots) with the mode law and validate-before-remove discipline;
      child-process abort pins; 414 green, full battery zero
      divergences.
      — T11 COMPLETE (2026-11-10): docs/M5_JNI_CALLS.md (the F-J2
      resolution — A-forms workhorse, V-forms via the SysV save-area
      walk, bare forms loud); the pending-exception cell; the method-ID
      registry; all thirty Call*MethodA AND all thirty Call*MethodV
      across virtual/static/nonvirtual; FFI 7 → 10 primitives
      (occurrences 6 → 9); 432 green.
      — T12 COMPLETE (2026-11-10): docs/M5_JNI_NATIVES.md; the binding
      registry + slots 215/216; the dispatch-walk consult; ACC_NATIVE
      accepted with the emitted call-through body; the REAL
      three-channel acceptance e2e vs Temurin (byte-identical
      UnsatisfiedLinkError header); 440 green.
      — T13 IN PROGRESS (docs/M5_JNI_HARNESS.md): 13a the note landed;
      13b primitive 11 `call_foreign_native` (the fixed SysV
      register-window call) + gate-10 amended (occurrences 9 → 10) +
      the Foreign path switch + the REAL C probe via build.rs (test
      binaries only — the first C in the repo) + the window loudness
      pinned through the full runtime path; 444 green, full battery
      zero divergences.
      — 13c LANDED (2026-11-10, session 7): the ONE harness `.so`
      (`tests/jni_harness/harness.c`, compiled ONCE, runs against BOTH
      runtimes) + the ONE self-checking driver fixture + the loading
      story (VM `GetEnv` supported path + `GetJavaVM` wired through
      audited primitive 3 — 159 real slots; `System.loadLibrary` as a
      runtime native over primitive 12 `load_foreign_jni_library` =
      dlopen + dlsym + the ABI-fixed JNI_OnLoad call, gate-10 amended
      10 → 11 in the same commit; GetVersion corrected to the MEASURED
      Temurin answer 0x0015_0000 — T08's 1.8 assumption falsified by
      the harness, the oracle's measured value is the law) + the
      three-channel parity runner (`tests/jni_parity.rs`) — **Temurin
      vs j2r BYTE-IDENTICAL on stdout/stderr/exit, the full v1 battery
      (versioning/VM, classes/fields, the A-form families + the
      exception round-trip, V-forms real-variadic AND hand-built, the
      mUTF-8 string edges both directions, arrays incl. all three
      release modes + criticals, the RegisterNatives rebind/unbind
      round-trip), both directions**. 448 green, full battery
      (diff_suite 464.16 s + cfg_equiv 347.11 s) zero divergences.
      Remaining: 13d the attach-lifecycle battery + the CI gate →
      wave 2 closes.
      — 13d LANDED (2026-11-10, session 8) — **T13 COMPLETE, WAVE 2
      (JNI) CLOSED**: the MEASURED attach contract (the C probe in
      scripts/evidence/attach_lifecycle_probe/ — fresh-thread GetEnv =
      JNI_EDETACHED pre-attach AND post-detach; attach/re-attach/
      AsDaemon = JNI_OK idempotent; detach = JNI_OK ALWAYS, three
      consecutive detaches measured JNI_OK) → VM AttachCurrentThread
      (4) / DetachCurrentThread (5) / AttachCurrentThreadAsDaemon (7)
      real (162 real slots; DestroyJavaVM stays loud), GetEnv
      attach-aware, main_run marks the main thread attached at birth,
      the harness pthread battery (verdict 2047: every measured arm +
      the self-checking battery on the attached env in BOTH attach
      cycles) byte-identical three-channel vs Temurin, the loud-gap
      child-process pins (NewGlobalRef/MonitorEnter/MonitorExit/
      GetStringUTFRegion abort naming the slot), and the dedicated CI
      gate gate-22-jni-c-harness-parity wired into the all-gates
      aggregate (17 upstream gates). REVIEW_CHECKLIST G1.1–G1.6 ALL
      checked; 452 green. Next per REVIEW_CHECKLIST ordering: G2 (the
      70k path — G2.1 CLOSED session 11 with the 20k re-measure: worst
      generated-unit rustc 3.49 → 0.99 GB, residual build peak =
      rust-lld 3.09 GB registered as G2.1b; G2.6 CLOSED session 11 via
      arm (a): the array-literal peephole — JZlib release SIGKILL at
      3.25 GB → rc=0 in 50.34 s / 678 MB, byte-identical vs Temurin;
      G2.2 CLOSED session 11: manifest v2 (mtime,size) fast path — 20k
      no-op emit 41.74 → 29.93 s, 20,003 reused / 0 written; G2.3
      CLOSED session 11: the stage profile named two O(N²) hot
      functions in the load stage (referer attribution + module
      collision scan), both fixed — per-class 1.30 → 0.111 ms (FLAT
      across 4× N), 20k no-op emit ≈ 5 s; G2.4 CLOSED session 13:
      release-profile 20k trio — cold release cargo 7537.36 s over 14
      chunks, peak VmHWM 3.29/4.0 GB (final link), 1.01 GiB binary,
      parity PASS vs Temurin, no-op release cargo 0.36 s, one-edit
      100.88 s; G2.1b CLOSED session 13 via arm (b) — the link-stage
      boundary accepted in writing (3.29/4.0 GB measured, sampler
      wired into the standing protocol, a ~70k release link EXPECTED
      to exceed the budget and recorded if it fires); G2.5 CLOSED
      session 13 as the honest ladder, 70k NOT claimed — 35k: emit
      23.89 s / 1.58 GB, ALL 141 partition units compiled (rustc ≤
      1.0 GB each), first link ran (lld 3.28/4.0 GB INSIDE budget,
      sub-linear) and died rc=101 ENOSPC at 263 MB of binary — the
      disk wall triple-measured (0 B free; cargo refuses missing-
      member workspaces; best packing 0.1 GB vs 2.1 GB needed);
      50k emit 36.35 s / 2.23 GB; 70k emit 53.14 s / 3.16 GB INSIDE
      the 4 GB budget at Minecraft scale (javac needed -J-Xmx3g;
      the 70,003-entry jar is ZIP64 — the reader rejected it, zip.rs
      now reads ZIP64 with 3 new tests (battery 465/0 at that moment;
      the final session-13 battery is 466, below), gate-11 in
      sync); G2.7 CLOSED session 13 (budget named in the ladder
      scripts + 70k record); G2.1–G2.7 all checked; G2.5-ladder-continuation
      (a rung above 20k end-to-end)
      waits for a bigger box)
      is the remaining P0; T14–T16
      mappings (G5) start only after G1 AND G2 are checked.
      — 4.2 LANDED (2026-11-11 session 15 verification: the session-13
      tail is recorded here in full) — **G4.2 CLOSED via arm (b)
      (`2f795ec`)**: the JMM boundary is machine-enforced, not just
      documented — every generated access to a volatile /
      non-folded-final field carries a J2R-JMM-BOUNDARY marker at the
      use site (default + typed paths + registry rows), every emitted
      crate gains the deterministic src/jmm_boundary.rs sidecar
      (EMITTER_VERSION 2→3), CI gate-23-jmm-boundary re-derives the
      expected set from the input .class flags and fails loudly on
      divergence, tamper-tested rc=1, marked crate vs Temurin
      three-channel 52/3/7/42 identical; battery 465→466 (the G4.2
      sidecar+marker test), gate-11 in sync (README 466). **G3.1
      baseline registered (`cbcbbcb`)**: the readiness census committed
      (106 methods: 76 typed-ready, 13 liftable-all-ref, 13
      needs-cfg-lift — ImpureJoin 4, HasExceptionTable 8 — mixed-epoch 4;
      hqr2 26/33 clean positions at 93.2% prim occupancy); the
      deferred-join mechanism is the named next layer (DESIGNED entry
      below). **G3.1 CLOSED (session 17, 2026-11-11, entry below): the
      deferred-join mechanism now lifts ALL FOUR JamaBench kernels —
      tred2, tql2, orthes AND hqr2 are TypedReady; the census control
      pair: 76→77 typed-ready, ImpureJoin 1→0, zero CFG-shape
      refusals; cfg_equiv 109×{base,cfg,typed} ZERO divergences with
      the mechanism live. **Sequencing now: G3.2 (block-join layer +
      primitive-specialized collections vs the 2.78× tax) next;
      G4.1 (owner verdict) open with enforcement evidence ready; G5
      (T14–T23) UNBLOCKED — G1 AND G2 both checked — ordered
      after G3/G4 per the review; M6 planning drafted per owner request:
      docs/M6_PLAN.md rev 0 (owner review pending).
- [ ] Mojang/intermediary mappings pipeline (§4, T14–T16, Tiny v2 +
      constant-pool remap stage before j2r)
- [ ] mixin application at the bytecode level before j2r (R1; §4,
      T17–T21; class-identity design answer T17 gates all rewrites;
      acceptance = real Fabric mod, ≥3 mixins, identical both sides)
- [ ] inline caches for megamorphic call sites (§5, T22–T23; composes
      with the typed-register lever at 0.936)

### M6 mod ecosystem

- [~] planning: **docs/M6_PLAN.md rev 0 drafted (2026-11-11, owner
      review pending)** — §0 ordering law (G5.4 satisfied: G1+G2 checked;
      T14–T23 stay M5_PLAN-owned and land first), §1 seam audit
      (verified 2026-11-11 at `63f34a7`), §2 the mod pipeline
      (`j2r build mod.jar`), §3 the loader surface as natives, §4 the
      boot bar + `j2r test mod` + gate-24, §5 the Forge/NeoForge
      boundary note, §6 task register T24–T35.
- [ ] Fabric/Forge/NeoForge surface as natives + compiled mods (§3;
      v1 Fabric, Forge/NeoForge = boundary note only per §5)
- [ ] `j2r build mod.jar` (§2; the EXISTING verb gains mod semantics —
      fabric.mod.json v1 parse, stage pipeline, stage-level manifest
      caching; plain-jar behavior pinned unchanged)
- [ ] `j2r test mod` dev loop (§4; reuses diff_suite + sharding;
      gate-24-mod-boot; harness positive control first)

## 4. Standing law (always, no exceptions)

- Zero `unsafe` outside the audited `j2r-jni-ffi` FFI core (11
  occurrences, pin ≤ 12, docs/M5_JNI_FFI.md §3) — compiler-enforced
  forbid (every other crate root) + amended gate-10 grep.
- Every commit passes all gates; gates run locally before push.
- Never claim "done" without runnable evidence; never claim Minecraft
  compatibility without milestone evidence.
- Test with bigger real-world files, not dummy fixtures — every wave
  adds real-program corpus evidence.
- Test counts synchronized with docs, mechanically (gate-11).
- Differential evidence is three-channel (stdout/stderr/exit) vs a real
  Temurin 21 — a masking bug in the harness poisons every green claim.
- [x] **Divergent-join lift wave (2026-11-08)** — the readiness census's
      next refusal layer ("arm walks diverge to different joins") landed
      as TWO new diamond laws in cfg.rs, both found by J2R_CFG_DEBUG
      block-table/loop-table dumps on the JamaBench kernels and both
      verified by the batteries:
      (1) TRANSFER-ARM free-walk: a divergent diamond (m1 != m2) whose
      at-least-one arm starts at or walks to an EXISTING stop target
      (loop exit / enclosing join) lowers as a plain if/else — the
      transfer arm ends in that stop's own break/continue, the other
      arm's nested machinery consumes its own side; unplaceable joins
      still fail LOUDLY (BranchIntoEmittedCode / FallOffEnd), never
      silently. tql2 is the shape: header diamond 4, exit edge (4,25) IS
      the true arm — `if c { break 'loop } else { <whole body> }`.
      (2) MEET-ALREADY-STOPPED: a nested diamond whose meet is an
      enclosing structure's stop does not re-join it — arms break with
      the OWNER's label, the owner emits the meet exactly once (the old
      path double-joined and refused; the pinned law test
      join_with_third_predecessor_falls_back became
      join_with_third_predecessor_lifts_via_owner_stop with the emitted
      structure traced semantically identical: c0 -> 12 -> 20; !c0 ->
      (c4 ? 20 : 8 -> 20)).
      Effect: tql2 NeedsCfgLift -> TypedReady (92.2% prim traffic, the
      hottest Eigen kernel); JamaBench census 93/106 lifted (13
      fallback, was 21). tred2/orthes still refuse: their shared
      continue-tails (block 39: `d[i]=...; i--; goto header`) have
      outside-diamond predecessors — that needs a DEFERRED-JOIN
      mechanism (register the meet as a loop-scoped stop, emit its body
      once at the loop-body end before the back edge) — the registered
      next layer. hqr2 refuses elsewhere (next census row).
      Battery: cfg_equiv 109x{base,cfg,typed} zero divergences (295.9s +
      297.9s), diff_suite 109 (361.9s + 350.8s), corpus 4/4 (69.3s),
      codegen 98 (1 law updated with justification), runtime 163/163.
      Typed payload with tql2 in: 358.5s byte-identical vs same-session
      default 358.7s — flat at a FIFTH coverage point; the operand-
      extraction verdict (BENCHMARKS addendum 3) stands. Diagnostics:
      J2R_CFG_DEBUG=1 unresolved-join lines, =2 adds block/loop tables.
      CI evidence: (this commit).
- [x] **Deferred-join mechanism (G3.1 — CLOSED: all FOUR kernels lift,
      session 17, 2026-11-11)** — session 16 landed the mechanism
      (builder-owned stop stack, deferrals at both refusal sites,
      diamond-scoped drain, zero-divergence cfg_equiv) but the three
      kernels still refused deeper (tred2/orthes: un-emitted blocks at
      lift end; hqr2: one more exit shape). Session 17 found and fixed
      THREE walk-order defects, one a pre-existing latent bug the
      mechanism unmasked:
      (1) FRAME-ID STOP STACK — emit_loop popped its exit stop with a
      POSITIONAL `truncate(saved)`, but the diamond frames pop by
      label-retain, which can remove entries BELOW the mark and shift
      the stack: in transpose the inner diamond's retain shortened the
      stack so the truncate KEPT the exit stop `(5,'loop_3_2')`,
      region(5) short-circuited to stop_transfer and block 5 was never
      emitted (missing=[5]) — this ALSO regressed ~30 simple Matrix
      methods that were TypedReady in the baseline (cfg_equiv could not
      see it: they are not in the fixture set). The label-retain had a
      second latent collision: an active-meet diamond's frame label is
      the ENCLOSING LOOP's label, which is also the loop's exit-stop
      entry label — the retain killed the exit stop mid-body. Fix: every
      stop entry carries a globally unique frame id (`StopEntry.sid`,
      monotonic `stop_seq`); ALL frame pops retain by sid — immune to
      position shifts and label collisions. This alone lifted tred2,
      tql2 AND orthes and reverted the regression (76 typed-ready
      restored).
      (2) DELEGATED-EXIT ORDER — emit_loop refused on `exit_emitted`
      BEFORE the delegated check: hqr2's inner loop(72) has exit=15, the
      still-active outer loop's HEADER (the shared-exit shape: the inner
      loop's exit edge IS the outer loop's continue) — the header is
      legitimately in `emitted` AND a stop. Fix: compute `delegated`
      first; refuse exit_emitted only for an exit that is emitted AND
      unowned.
      (3) DEEP CROSS-MEET ATTEMPT — walk_meet stops at the FIRST join of
      each arm, hiding the arms' TRUE convergence: hqr2's diamond(76)
      reported m1=83/m2=80 (each arm's first internal join) while both
      arms converge at the loop latch 105 (the f-arm via the 81
      fall-through, the t-arm through the notlast diamond 85 whose own
      arms converge at 105 too, and the inner loop(91)'s exit joins
      there as well); block 82 FALLS INTO 83, so 83 is both the true
      arm's start and the convergence point (the empty-arm meet). The
      shallow divergent path then spuriously deferred 83 (its "outside
      pred" was its own chain successor) and emitted 105 inline in the
      f-arm, so diamond(85) hit emitted code. Fix: `walk_deep_set` (a
      bounded DFS through internal joins, transfer boundaries at
      stops/active-or-emitted headers, walk-through at FRESH open-exit
      headers so loop-internal preds of the exit pass the purity check)
      finds the cross-arm join; an ARM-START candidate becomes the meet
      via the battle-tested join_via_label, REMAINING cross-arm joins
      (the latch 105 — one pred is the else arm's own `goto`, the others
      live inside the meet body) are PRE-REGISTERED so their label opens
      before the if/else and every break site is inside it; the attempt
      is OPTIMISTIC with FULL ROLLBACK (out/brace_trace/deferred/stops/
      active_loops/emitted/body_walk restored by mark or sid) — on
      refusal the shallow path still runs and its ImpureJoin refusal
      class is the pinned law. The law test walk_meet_maps... was
      updated: its shape (the ret block IS the outer join — an
      empty-arm meet) now LIFTS with both joins label-scoped; the
      test's primary purpose (the no-panic index mapping) is preserved.
      EVIDENCE: the census control pair committed
      (scripts/evidence/g3_1/readiness_before_s17_fixes_2026-11-11.txt:
      76 typed-ready / ImpureJoin 1 — vs
      readiness_after_s17_fixes_2026-11-11.txt: **77 typed-ready,
      needs-cfg-lift 9 = HasExceptionTable 8 + HasSwitch 1, ZERO
      CFG-shape refusals** — tred2 83.4%, tql2 92.2%, orthes 86.5%,
      hqr2 93.2% prim, all TypedReady); **cfg_equiv 109x{base,cfg,typed}
      ZERO divergences** with the mechanism live (332.69 s); full
      battery 466/0; fmt + clippy `-D warnings` clean; gates
      11/19/20/12 green. The typed-payload re-measure on the four
      kernels (G3.4's framing) is the natural next measurement for
      G3.2's block-join/primitive-collections ladder.
- [x] **Typed-payload re-measure — the G3.2 opening measurement TAKEN
      (2026-11-12, session 18)** — the registered protocol (addendum 8:
      median of 2, fresh emits both twins, byte-identical stdout vs the
      same-session Temurin run) re-run at the all-four-kernels-typed
      census (76→77 typed-ready made tred2/orthes/hqr2 typed; addendum
      8's 0.936 had ONLY tql2 typed of the four). Same-session control
      pair: **default 279.23 s vs typed 256.31 s = 0.918** — the lever
      ledger moves 0.936 → 0.918 (8.2% cumulative); the 1.8pp is the
      tred2/orthes/hqr2 typing margin. All five processes exit 0, all
      four native runs byte-identical to the JVM on stdout. Honesty
      notes recorded (G3.5): absolute times drift — only the
      same-session ratio is claimed; this box's steady-state JVM ref is
      2.60 s → typed gap ~98.6x, so the "~100x" phrasing stays quoted.
      Evidence: scripts/evidence/g3_2/ (pair table + the five raw run
      logs). Next: **G3.2 — the block-join/entry-var layer** (~140
      slot-var reads, the named prerequisite at its measured ceiling);
      G3.3 (primitive-specialized collections, the 2.78× container tax)
      sequenced after the entry-var layer per the register.
- [x] **G3.2 slice 1 — the REF-ONLY REFRESH PROJECTION — LANDED
      (2026-11-12, session 18, same session as the re-measure)** — the
      mechanism: `FrameGuard::refresh` consumes ONLY `Value::Ref(Some(_))`
      entries; the typed body's refresh arrays wrapped EVERY prim local +
      Void-filled every HighHalf + wrapped every prim stack position —
      5119 wraps+clones across 1023 refresh sites in the JAMA typed
      crate, all pure construction + scan waste (they can never
      contribute a GC handle). The typed refresh now carries only the
      positions that can root (`Var::Cell` locals = ref or mixed;
      REF-kind live stack positions); typed local positions are
      all-prim by plan construction, Dead positions hold no value. The
      handle list is IDENTICAL (GC-root parity by construction), sites
      unchanged (the safepoint discipline is per-site).
      MEASURED (same-session, registered protocol, median of 2,
      byte-identical): default 279.23 s / typed 256.31 s = 0.918 →
      **typed 255.06 s = 0.913** (+0.5pp; the two run sets do not
      overlap — real but small; refresh-array traffic −60.3% yet the
      steady state is dominated by the register's third family). Lever
      ledger: 0.992 → 0.958 → 0.936 → 0.918 → **0.913** (8.7%
      cumulative). **Register correction (honest re-measurement of the
      G3.2 premise):** the entry-var layer's named target ("~140
      slot-var reads", measured 2026-11-08 pre-typed-payload) is down
      to **20 stack-entry reads** on the JAMA typed crate — the
      unification + payload waves consumed the stack-entry round trip;
      the measured residual is the per-access invoke/arg-vec family (a
      fresh `vec![Value...]` per call) + obj_get/invoke helpers — the
      register (TYPED_REGISTER_READINESS §5) updated with the correction
      and the layer's next slice re-aimed at that family. Proof:
      cfg_equiv 109×{base,cfg,typed} ZERO divergences (343.07 s),
      diff_suite 109 (458.93 s), corpus 4/4, full battery **466 tests,
      0 failures**, fmt + clippy clean; the comment-only clippy
      rewording verified emission-neutral. Evidence:
      scripts/evidence/g3_2/refonly_refresh_projection_2026-11-12.txt
      (+ census tools slotvar_census.py / crossblock_census.py + the v2
      raw logs).
- [x] **G3.2 slice 2 — the INVOKE/ARG-VEC STACK-ARRAY PROJECTION —
      LANDED (2026-11-12, session 18b)** — the re-aimed slice, executed
      the same day. The mechanism: every invoke in the typed backend
      built a fresh heap `Vec<Value>` per call (`let __a =
      vec![sN.clone()];`) — census on the JAMA typed crate: **228
      sites** (67 empty, 161 up to 4 args), every one verified one-shot
      into one of two consumers, both taking `&[Value]`
      (`natives::invoke_exact` and direct compiled-method calls
      `crate::classes::X::m_yyy(recv, &__a)`). The three invoke arms in
      typed.rs (InvokeVirtual/Interface, InvokeSpecial, InvokeStatic)
      now emit fixed-size STACK arrays — `let __a = [args...];` /
      `let __a: [::j2r_runtime::Value; 0] = [];` — which unsized-coerce
      to `&[Value]` at both consumer shapes: zero per-call heap
      allocation, zero runtime change, clones unchanged (GC-root parity
      by construction: the same clones root the same refs; only the
      container moved from heap to stack). Default backend's arg vecs
      are the same shape — registered as a follow-up candidate,
      separately measured if taken. MEASURED (same-session pair,
      registered protocol, median of 2, byte-identical): default
      280.44 s / typed 250.72 s = **0.894** — the ledger moves 0.913 →
      **0.894** (10.6% cumulative; 0.992 → 0.958 → 0.936 → 0.918 →
      0.913 → 0.894). Typed-backend residual: obj_get/invoke helper
      dispatch inside the runtime + the default backend's arg vecs +
      the G3.3 container tax (2.78x, register step 3). Proof: cfg_equiv
      109×{base,cfg,typed} ZERO divergences (337.15 s), diff_suite 109
      (463.65 s), corpus 1/1, full battery **466 tests, 0 failures**,
      fmt + clippy clean. Honesty notes (G3.5): this box's JVM ref
      2.87 s → typed gap 87.4x this session (the absolute gap drifts
      with box load; only the ratio is claimed). ALSO (owner directive,
      flagged leftovers verified): Wave residual depth >= 5 trigger NOT
      fired (the only `[[[[[I` on disk is the registered-limit test
      itself, verify.rs:2202/2235 — stays deferred per the DECIDED
      design); R2 metadata generator unchanged, registered post-M4
      scope. Evidence:
      scripts/evidence/g3_2/argvec_stack_array_2026-11-12.txt (+ five
      raw run logs s18b_*).
- [ ] **G3.3 OPENING — the primitive-specialized map path: census +
      design CONCRETIZED (2026-11-12, session 18b)** — the register's
      step 3 (the 2.78× container tax), teed up the same session. The
      census (the addendum-7 instrument itself, BoxingBoxedBench):
      javac lowers the hot loop as Map.get → doubleValue → +w →
      Double.valueOf → Map.put (pc 99/107/113/116); the generated
      typed Rust pays per op: 1 heap alloc (valueOf — doubles have no
      JLS cache) + 2 invoke_virtual dispatches + 2 obj natives, all
      replaced by the JVM's scalar replacement. The design, now
      concrete: (1) signature-pinned specialization (the generic
      signature attribute pins Map<String,Double>); (2) two rewrites —
      put: valueOf→put direct flow ⇒ `map_put_prim` (raw prim in the
      node's Value slot, no box, no valueOf dispatch); get:
      box→doubleValue-only ⇒ `map_get_prim` (direct unwrap); (3) the
      faithfulness guard: a dataflow window (no branch/phi/other use
      between box birth and box consume) makes both rewrites
      unobservable-by-construction; identity-through-the-map
      (`m.get(k) == v` after put) excluded by the same guard; generic
      exits (entry.getValue(), values()) keep the boxing path or
      materialize lazily, cached in the node slot (get-twice-returns-
      the-same-ref stays TRUE); (4) the node slot stays a Value (the
      M3 heap-resident law stands — zero new GC machinery, the prim
      slot is one fewer child to scan); (5) HashMap first,
      TreeMap/LinkedHashMap registered follow-ons. Honest residual:
      one native call per op instead of two dispatches — the gap to
      the JVM's 1.77x is the hash walk both pay. STATUS: registered,
      NOT scheduled — implementation waves after the G3.2 layer's
      slices conclude (slices 1+2 landed this day: 0.918 → 0.913 →
      0.894). Evidence:
      scripts/evidence/g3_3/opening_census_2026-11-12.txt.
- [x] **EXTERNAL PORT REPORT PERSISTED — jbox2d 2.2.1.1 (2026-11-13,
      session 19b, owner directive: "add these in todo list and roadmap
      to survive sandbox reset")** — the owner's independent port test
      (run OUTSIDE this repo on a 2 vCPU / 2 GB box, against j2r @
      c38fcdb plus ~1,100 lines of owner-written port-wave patches) is
      now authoritative repo content: docs/external-port/{README.md,
      jbox2d_result.md, jbox2d_bug.md, jbox2d_opt.md}. The result:
      156 classes / 140,359 generated lines / 16.5 MB binary; 6/6
      workloads byte-identical to Temurin 21 through 1,980 chaotic
      physics steps + 1e6 trig evals; steady-state 6.9x–84x slower
      than the JVM (median 25x, the gap grows with hot-loop time — the
      same regime as our published JamaBench numbers); peak RSS
      1.74x–4.80x LOWER; startup 26x faster; translate 156 ms vs javac
      969 ms, then 111.8 s of cargo. The corroboration value: (a) the
      default-backend cost ledger (Opt.md §1: refresh 13.3 M calls,
      obj_get 58.7 M, ~337 M stack pops in pendulum-300; six runtime
      primitives ≈ 50% of the run) independently measures the SAME
      families register §5 attacks; (b) the owner's micro-bench (typed
      u64 slots 0.60 ns vs Vec<Value> 3.05 ns = 5.1x) corroborates the
      typed-register premise; (c) the port needed NO depth >= 5 array
      descriptors and no reflection beyond Class.forName-shaped
      resolution — both prior deferrals validated by a real program
      (and both have since MOVED anyway: e6ace40 closed depth-5,
      ac4af2d landed R2 slice 1). Evidence: docs/external-port/ (the
      reports themselves are the evidence).
- [x] **EXTERNAL-B1 — the formatter %f small-magnitude fix LANDED
      (2026-11-13, session 19b)** — render_f (crates/j2r-runtime/src/
      formatter.rs) derived digits from javafmt::double_to_string and
      split on '.', but the JVM's shortest repr goes scientific below
      1e-3 and at/above 1e7, so every %f/%.Nf/%,f of a value in those
      bands dropped the exponent (6.149717374e-4 printed "6.149717374"
      where the JVM prints "0.000614972") — the owner-reported silent
      wrong-output bug (jbox2d_bug.md B1). The mechanism: normalize the
      repr to a plain digit string + decimal-point position BEFORE the
      precision rounding (the same parse render_e already performs);
      point < 0 shifts leading zeros into the digits; a HALF_UP carry
      that grows the result PAST round_idx shifts the point right by
      the overflow. **A SECOND latent bug found by the new pins en
      route:** the old fixed split_at printed an extra fraction digit
      on rounding carry (9.99 at %.1f -> "1.00" not "10.0") and
      truncated the integer part at precision 0 ("1" not "10") — the
      first implementation repeated it and its own new pin caught it
      before anything landed. O4 fixture: the 19-row section X appended
      to corpus/FmtResidualProbe.java AND tests/suite/Fmt.java (no new
      fixture, count 110/466 unchanged), oracle captured from JDK
      21.0.13 BEFORE the rows landed. Proof: targeted end-to-end
      differential of the probe — 135 rows BYTE-IDENTICAL through a
      generated binary; runtime 243/0 (19 new pin assertions);
      diff_suite 110 ZERO divergences (475.83 s); cfg_equiv 109x3 ZERO
      (350.25 s); full battery **466/0**; fmt + clippy clean.
      Evidence: scripts/evidence/external_b1/
      render_f_band_fix_2026-11-13.txt.
- [x] **EXTERNAL-B2 — the partitioned-emission registry-hook fix LANDED
      (2026-11-13, session 19b)** — emit.rs emits the exception-matcher
      helper with `crate::registry::parents_of` (the single-crate
      shape); partition.rs::rewrite_class_source had NO arm for it, so
      any partitioned class carrying an exception table failed E0433
      ("cannot find `registry` in the crate root") — reproduced on this
      tree BEFORE the fix (three-class probe incl. a `<clinit>` that
      catches, the jbox2d World$1 shape; p0 and p1 both failed). The
      fix: the rewrite arm `crate::registry::` →
      `support::registry_hook::`; a generated support/src/
      registry_hook.rs — two OnceLock fn-pointer slots (parents + meta,
      the dispatch_slot/link pattern), double-install and
      use-before-install both panic loud, never a wrong subtype answer;
      the app main template installs `parents_of`/`meta_of` (already
      composed at app crate root) BEFORE any Java code runs; single-
      crate emission untouched. O5 fixture: NEW tests/suite/
      PartitionCatch.java (the 111th differential fixture; try/catch +
      finally + a `<clinit>` catch + cross-partition calls), registered
      in partition_suite at k=2 — the exception-table path now runs on
      every partition test (first draft hit String.join(Iterable) — a
      loud gap, recorded under B3..B12; replaced with concatenation).
      Count bumps: README 111/111 + 111 programs, diff_suite guard 110.
      Proof: the probe builds, runs exit 0, byte-identical vs the JDK;
      partition_suite 7/7 (29.80 s); diff_suite 111 ZERO divergences
      (485.48 s); cfg_equiv 109x3 ZERO (354.65 s); full battery
      **466/0**; gate-11 fixtures=111 tests=466 + doc-links + stubs
      green; fmt + clippy clean. Evidence:
      scripts/evidence/external_b2/registry_hook_fix_2026-11-13.txt.
- [ ] **EXTERNAL-B3..B12 — the natives/jdk-surface wave measured by a
      real game-shaped program** — the nine loud gaps jbox2d hit (each
      a compile error — j2r's law, correct behavior; each specified in
      jbox2d_bug.md): B4 NoSuchFieldError hierarchy row (JLS 13.4.9
      bridge catch); B5 StrictMath hierarchy row + 35 natives sharing
      the Math implementations; B6 the 8 boxed classes instantiable,
      <init> rows aliased to valueOf so the JDK identity cache applies
      (new Integer(127) == new Integer(127) TRUE on HotSpot too); B7
      AssertionError(Object) via the Throwable message-slot machinery;
      B8 the four IEEE bit-reinterpretation statics rows (incl. the
      floatToIntBits-NaN-canonicalizes vs floatToRawIntBits-does-not
      distinction that Float.equals depends on); B9 the transcendental
      surface as a faithful fdlibm module (sin/cos/tan/atan/atan2 from
      musl's fdlibm, plain f64, NO FMA contraction, registered for
      Math AND StrictMath; Payne–Hanek __rem_pio2_large loud-panic
      residual stated in the module doc; honest O1 residual: HotSpot
      itself differs from fdlibm on ~6.5 inputs per 100k); B10
      java.util.Random as a real builtin object (seed slot + gaussian
      cache slots, 12 natives: the JDK 48-bit LCG,
      power-of-two/rejection nextInt(bound), polar nextGaussian,
      nextBytes, JDK-exact exception strings); B11 ArrayList.toArray()
      / toArray(T[]) with the JDK's two-branch contract; B12
      Arrays.sort(T[],from,to[,Comparator]) range forms with JDK-exact
      exception messages + Class.desiredAssertionStatus() → false.
      Sequencing: B9's fdlibm module is the biggest piece; B4/B5/B7/B8
      are table rows; B6/B10 are builtin-class machinery; B11/B12 are
      jdkutil natives. STATUS: **LANDED 2026-11-13** (commit
      external-b3..b12; evidence
      scripts/evidence/external_b3_b12/natives_surface_2026-11-13.txt):
      fdlibm.rs transliterated verbatim from musl (rem_pio2 medium path,
      sin/cos/tan/atan/atan2) + the JDK's own FdLibm.Hypot; Math+StrictMath
      12 trans rows + 38 StrictMath mirror rows; NoSuchFieldError row;
      AssertionError family (7 rows); reverse bit statics; 8 boxed
      classes with FRESH-box ctors (owner's cache claim CORRECTED vs the
      pinned JVM vector: new Integer(127) == new Integer(127) is FALSE);
      java.util.Random builtin (3 slots, 12 natives, 48-bit LCG
      bit-identical incl. nextGaussian via f64::ln == HotSpot's log);
      ArrayList.toArray both forms; Arrays.sort ranges + null-comparator
      fallback + reverseOrder + desiredAssertionStatus. NEW fixture
      Externals (112th) byte-identical vs Temurin; diff_suite 112/0
      (471.62 s), cfg_equiv 0 (355.83 s), battery 467/0, fmt+clippy
      clean, doc counts 112/467 in sync. EN-ROUTE BUG FIXED: a main()
      touching none of its own class's statics ran WITHOUT the <clinit>
      (JLS 12.4.2 launcher law) — both main templates now ensure the
      main class; diff_suite proves no regressions.
- [x] **EXTERNAL-HARNESS — the four process items (Opt.md §6 + O6)**
      — (1) `j2r cfg-report --surface`: the sorted, deduplicated JDK
      member set the input needs, diffed against the NATIVES table
      (would have shown B4–B12 in one page instead of ten compile
      cycles); (2) a collect-all-diagnostics mode (walk every class,
      report every unresolved member, then fail — the first unresolved
      reference is only the first); (3) a --partition-classes 2
      variant in the corpus/partition runner, and the partition
      fixtures must include an exception table (B14's law); (4) the
      differential harness pins its reference charset itself
      (-J-Dfile.encoding=UTF-8) instead of inheriting the ambient
      locale (O6 — five phantom failures on a C locale,
      owner-verified). STATUS: **LANDED 2026-11-13** (evidence
      scripts/evidence/external_harness/harness_items_2026-11-13.txt):
      surface.rs walks all invoke/field/new/class-ref shapes + catch
      types and classifies with the compiler's OWN resolution fns;
      compile() split into the unchanged hard gate + compile_unchecked
      (the walk sees past the first failure); the probe proof names
      Math.asin — deliberately unimplemented — as the ONE missing site
      in one page (26 sites); Externals joins the partition runner at
      k=2 (8/8); the reference channel pins -Dfile.encoding=UTF-8;
      durable pin tests/surface_report.rs; battery 468/0, diff_suite
      112/0, cfg_equiv 0, partition 8/8.
- [x] **CLINIT-THROW — the JLS 12.4.2 ExceptionInInitializerError
      machinery (found 2026-11-13 by the EXTERNAL-B wave, LANDED
      2026-11-13)** — clinit.rs discarded a <clinit>'s Throw and marked
      the class Done: a throwing clinit silently proceeded. LANDED: the
      Temurin contract probed (ClinitProbe/ClinitProbe2) — Errors (EIIE
      included) rethrow as-is, everything else wraps in a message-null
      EIIE with the original as cause (the JDK's EIIE(Throwable) ctor
      shape); the class goes Errant with two pre-formatted strings (never
      a JRef — the state table is GC-invisible) and every later first-use
      throws NCDFE "Could not initialize class <binary>" with a fresh-EIIE
      cause "Exception <E> [in thread \"<failing thread>\"]" (name captured
      at failure time). ensure() returns Result; all four bytecode ensure
      sites (getstatic/putstatic/invokestatic/new) ride j2r_try; both
      launchers route a throwing main-class clinit into the uncaught
      path; register_class_names feeds the binary-name message. Two
      builtin parent rows forced: EIIE → LinkageError (user `new EIIE`
      was unsupported) and NoClassDefFoundError → LinkageError (the
      errant NCDFE was invisible to catch(Throwable)). Fixture
      **ClinitThrow** (the 114th) pins five shapes x two accesses,
      byte-identical vs Temurin 21.0.13+9 on all three channels.
      Residual registered: FORNAME-INIT (Class.forName does not RUN the
      clinit — pre-existing, separate wave).
- [x] **SORT-NULL-CMP — Arrays.sort(T[], Comparator) null fallback
      (spotted 2026-11-13, FIXED 2026-11-13)** — the pre-existing
      whole-array comparator row NPE'd on a null comparator; OpenJDK
      falls back to natural ordering (the JDK source's first branch).
      FIXED: n_arrays_sort_obj_cmp now delegates to the natural-ordering
      row on a null comparator (the same delegation the range row
      already implemented); ArraysSurface extended IN PLACE with two
      null-comparator pins (String + Integer arrays), byte-identical vs
      Temurin 21.0.13 on all three channels.
- [ ] **FORNAME-INIT — Class.forName does not RUN the <clinit>
      (registered 2026-11-13 during CLINIT-THROW)** — forName resolves
      the closed world and returns the Class object without ensuring
      initialization; on Temurin, forName(name) = forName(name, true,
      loader) and a first-use through forName RUNS the clinit (and a
      throwing one surfaces the EIIE law). n_class_for_name needs the
      ensure walk for user classes (the id → clinit fn mapping lives in
      the generated statics registry; the runtime needs a
      class-id-for-name hook or the metadata registry gains the clinit
      fn). ReflectForName's fixture never noticed: its classes were
      already initialized by earlier first-uses. STATUS: registered,
      not started.
- [x] **R2 slice 3 — getDeclaredFields/getFields +
      getDeclaredConstructors (2026-11-13)** — the CTORS_{tag} metadata
      tables (desc + modifiers, CLASSFILE order) ride both emitters; the
      Temurin REVERSAL (last-declared ctor prints first) is a runtime
      law. getFields: own publics then the superclass chain's, NO shadow
      dedupe (oracle ReflectProbe); the interface-constants leg stays a
      residual. getDeclaredConstructor/getConstructor: exact param match
      up to the closing paren; the NSME message "Owner.<init>(float)";
      varargs descriptors are `([Ljava/lang/Class;)...`. Constructor's
      read-only surface (getName/getModifiers/getParameterTypes/
      getDeclaringClass/toString) + Modifier.toString(int). Fixture
      **ReflectDeclared** (the 115th) byte-identical vs Temurin
      21.0.13+9; cfg_equiv 0 (it caught the TYPED emitter's three ensure
      sites still discarding the clinit throw — now try_wrap-wrapped);
      units 473/0.
- [ ] **NEW-INSTANCE — Constructor.newInstance / the user-layout hook
      (registered 2026-11-13 during slice 3)** — the runtime cannot
      allocate a user class by name (the slot spec lives in generated
      code at each `new` site; JNI's NewObject is an unimplemented stub,
      slot 28). Design: a Dispatch hook (`user_layout: fn(&str) ->
      Option<(&'static str, usize)>` — spec + total slots) emitted into
      j2r_dispatch by both modes, then newInstance = ensure (the clinit
      law! errant NCDFE propagates) + new_obj + set_defaults + the
      <init> call through find_method; Temurin's caller-sensitivity
      (same-package private ctor SUCCEEDS for a same-package caller) is
      unmodelable without a caller concept — register the honest
      residual. STATUS: registered, not started.
- [ ] **EXTERNAL-CORROB — Opt.md R1–R9 mapped onto the register (the
      cross-reference recorded so the roadmap cites it; no new
      claims)** — R1 (FrameGuard monitors lazy, owner prototype
      MEASURED −4.7%/−4.9%) + R2 (refresh writes the slot in place,
      needs the M4_PLAN L1/L2/L3 lock-scope audit FIRST) = runtime-
      frame candidates, zero semantic change, recommended first pull
      of the perf wave after the B1/B2 correctness fixes; R3 (refresh
      only on reference change, 10–15% expected, >90% avoidable in
      solver loops) = our ref-only refresh lever's next step (0.913
      landed) — the owner's independent estimate corroborates the
      projection and demands the same safety protocol (rooting
      tripwires + forced-GC-every-N stress); R4 (typed operand stack,
      block-linear emission, structured recovery) = the typed-register
      track's endgame (ledger at 0.894; the owner's 5.1x stack-cost
      micro and ~18%/35% stack-op shares measure the same family);
      R5 (codegen-time invokevirtual resolution + a real vtable for
      the polymorphic residue) = the registered obj_get/invoke-dispatch
      residual; R6(c) (per-basic-block heap borrow region, removes
      58.7 M lock/unlock pairs ≈ 9%) = NEW runtime-track candidate,
      honest under the current architecture (no unsafe, no
      memory-model change); R7 (perfect-hash dispatch table over the
      2,169 string-tuple arms) = registered follow-up shape after R5;
      R8 (target-cpu/codegen-units defaults; FMA stays OFF for JVM
      parity) = cheap single-digit default flip; R9 (build cost:
      111.8 s per 156 classes scales to ~14 h at 70k classes) = the
      G2.6/M5 wall, now with a 2 GB-box OOM data point (B13:
      single-crate rustc SIGKILL at just 156 classes). Opt.md §4's
      do-NOT list (no copying GC, no NaN-boxing of Value, no
      computed-goto, no manual vectorization, no platform-libm switch)
      agrees with our standing laws. STATUS: mapping recorded.
