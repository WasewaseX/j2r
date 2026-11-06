# J2R — Master Checklist (plain text: done vs planned)

The live status board for the whole project. A box is checked ONLY with
runnable evidence (test name, CI gate, fixture, commit). `[~]` = started,
with the gap stated. If a claim and this board disagree, this board loses —
fix the board. Last updated: 2026-11-06.

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
      selftest 17/17, forced-GC on the new fixture green.
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
- [ ] **Wave residuals (remaining)** — array descriptor depth >= 5 in
      the verifier lattice stays loud BY DESIGN (a registered limit with
      a precise error, not undone work — docs/CORPUS_WAVE.md §gaps);
      depth 4 was closed by the depth-4 verifier wave (see the entry in
      §Corpus & quality).
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
- [ ] **R2 metadata generator (post-M4 scope)** — reflection metadata
      retention so runtime string lookups resolve (closed-world design
      in docs/STRATEGIC_GAPS.md).
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
      (0.04x) and JAMA (0.09x) with the wave-3 landing (2026-11-06,
      docs/BENCHMARKS.md); JZlibCorpus rowless BY RULE (release build is
      the registered rustc-memory finding). Remaining: steady-state/
      throughput numbers only after corpus expansion (F2/F4/F5).

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
