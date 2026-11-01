# J2R Roadmap — Java → Rust compiler, M1 → M5

Updated 2026-11-01 · every commit CI-green (runs recorded for substantive commits below)
M1 architecture review: **APPROVE** ("PARTIALLY COMPLETE / Promising Milestone 1")
M2 plan: `docs/M2_PLAN.md` — **PLAN ONLY; coding is blocked until that plan passes
architectural review** (M2_PLAN §11). Revision 2 (2026-11-01) answers the
REQUEST CHANGES verdict on the original plan — all six blockers resolved in
M2_PLAN §0; revision 2.1 (docs-only) adds the plan-time stack-footprint
calibration measurement (M2_PLAN §3.1.1: measured 208–304 B/frame, const
512 B, ceiling 131 072 frames). Awaiting re-review. This file is the
top-level map and the working todolist; the per-change specifications
live in M2_PLAN.

---

## 1. Ground rules (standing, never negotiable)

1. Nothing counts as done without automated test evidence recorded in the repo.
2. Correctness is defined by differential testing against Temurin 21, three
   channels (stdout, stderr, exit status). The harness normalizes only stack
   frame lines — never exception types, messages, or `Caused by:` chains.
3. Zero `unsafe` in the workspace; `j2r-runtime` and `j2r-classfile` keep zero
   external dependencies.
4. Contributor identity is always `WasewaseX
   <321686523+WasewaseX@users.noreply.github.com>`; strict multi-gate CI on
   every push, with an all-gates aggregator so a skipped gate fails loudly.
5. Unsupported features fail loudly at compile time — never silently miscompile.
6. Every milestone ends with an external architecture review before the next
   milestone's coding begins.
7. No Minecraft-compatibility claim of any kind until M4 produces evidence.

---

## 2. Where we are — completed work, with evidence

| Stage | Commit | CI run | Evidence |
| --- | --- | --- | --- |
| M1 core compiler (the commit the review audited) | `4e6b3a4` | 41124794339 SUCCESS | review verdict: M1 APPROVE |
| M1.5 hardening H1–H4 (3-channel stderr diffs, abstract-interpreter verifier, u32 ids, borrow audit) | `2c9ae0e` | 41180545706 SUCCESS | `docs/HARDENING.md` |
| Hardening evidence record + reentrancy census | `cda8e41` | 41191658892 SUCCESS | `docs/RUNTIME_SAFETY.md` |
| M1.6 coverage (boxing identity, multianewarray, suite → 34 fixtures) | `c07a10f` | 41201763892 SUCCESS | diff suite count guard in CI |
| M2 implementation plan published (no code) | `2dcd031` | 41212581846 SUCCESS | `docs/M2_PLAN.md` |
| M2 plan revision 2 (review response: settled root protocol, generation tags, SOE ceiling, threading boundary, spec scope, README fixes) | `942648e` | 41281330354 SUCCESS | M2_PLAN §0 response matrix |
| M2 plan revision 2.1 — plan-time stack-footprint calibration for blocker 3 (measured 208–304 B/frame; const 512 B; ceiling 131 072) | `350ae1d` | 41299060088 SUCCESS | M2_PLAN §3.1.1 |

Current test inventory (re-verified locally 2026-11-01, Temurin 21.0.13 at
`/tmp/jdk21`, after a full sandbox/toolchain rebuild): **125 tests, 0
failures**, including the 34-fixture differential suite vs the real JVM
(58 s wall-clock) and the 12 malformed-classfile e2e tests. Zero `unsafe`
(`rg "unsafe" crates/` is empty).

---

## 3. Milestone ladder

| M | Goal | Scope | Exit criteria | Status |
| --- | --- | --- | --- | --- |
| M1 | Core compiler | subset JVM: methods, objects, arrays, strings, inheritance, exceptions | all gates green + external review | **DONE — APPROVE** |
| M2 | Generated-code quality | catchable `StackOverflowError`; tracing cycle-collecting GC; typed operand-stack specialization; CFG reconstruction investigation | plan review → waves 1–4 each green with evidence → completion review | plan under review |
| M3 | JDK coverage | JAR/classpath, collections, lambdas/invokedynamic, reflection, `synchronized` | real JDK libraries load and pass differential tests | gated on M2 review |
| M4 | Minecraft readiness | JNI, Mixin transformations, mappings (SRG/MojMap), large-scale compilation | a nontrivial library jar compiles and runs differentially clean | gated on M3 review |
| M5 | Mod ecosystem | Fabric / Forge / NeoForge loaders | a real mod boots in the M4 runtime | gated on M4 review |

No milestone may start before its predecessor passes review. Scope inside a
milestone is ordered by its own task register (§4 for M2).

---

## 4. M2 task register — the working todolist (23 tasks)

Status: `[x]` done · `[~]` in progress · `[ ]` pending (blocked on plan review).
Design detail + the 7 required per-change specifications (files, Java
semantics, memory safety, performance, differential tests, regression tests,
acceptance criteria) live in the M2_PLAN section shown in each wave heading.

### Wave 0 — planning & baseline (this phase)

- [x] **T01** Audit operand stack, typed IR, CFG representation, heap
  ownership, generated Rust — verified against source, file:line citations.
  → M2_PLAN §1
- [x] **T02** `Rc`-model semantic risk register R1–R8 (cycles leak, no heap
  ceiling, refcount overhead, `!Send`, identity-hash instability,
  `RefCell` reentrancy, string payloads outside the graph, no rooting
  discipline). → M2_PLAN §2
- [x] **T03** Tracing-GC architecture + Java-threads readiness design
  (arena mark-sweep, safepoint-shaped collection points, per-thread roots
  plan). → M2_PLAN §4, §7
- [x] **T04** M2 plan published with all gates green (`2dcd031`, run
  41212581846). → M2_PLAN
- [x] **T05** Full local green baseline re-established after sandbox recycle:
  rustup + Temurin 21 reinstalled; 125 tests + 34-fixture differential green
  (2026-11-01). This todo exists because environments rot; the baseline
  claim is re-proven, not assumed.
- [x] **T06** Roadmap published with ≥15-task register (commit `10d7b5e`).
- [x] **T23** M2 plan revision 2 submitted in response to the REQUEST
  CHANGES review: settled pointer-free GC root protocol, generation-tagged
  handles, SOE hard ceiling, threading boundary statement, stacks-and-
  locals specialization scope, README contradictions fixed. Revision 2.1
  added the plan-time calibration measurement for the SOE ceiling
  (blocker 3's "measured native-stack budget": worst shape 304 B/frame,
  const 512 B, ceiling 131 072 — M2_PLAN §3.1.1). Awaiting
  architectural re-approval before Wave 1.

### Wave 1 — catchable `StackOverflowError` (M2_PLAN §3)

- [ ] **T07** `crates/j2r-runtime/src/frame.rs`: frame registry, depth
  counter, `StackOverflowError` constructor, `J2R_MAX_JAVA_FRAMES`
  validation against the **hard ceiling** derived from the measured
  per-frame native footprint (`0`, `>ceiling`, non-numeric rejected with
  exit 2; const assertion `default ≤ ceiling`), footprint-calibration
  test, unit tests for depth accounting across return/throw/panic.
- [ ] **T08** `crates/j2r-codegen/src/emit.rs`: emit `FrameGuard` in
  `emit_method`, depth check → early `return Err(soe)`, plus a codegen unit
  check of the emitted shape.
- [ ] **T09** Differential fixtures `StackDepth.java` (caught SOE, continues
  normally) + `UncaughtSOE.java` (uncaught, exit 1) — suite 34 → 36 — plus
  an e2e test at `J2R_MAX_JAVA_FRAMES=64` asserting the process **never
  aborts** (no SIGSEGV/101) and prints `caught-soe`, plus the six-case
  override-boundary matrix (default/min/max/max+1/0/invalid).
- [ ] **T10** Wave-1 evidence record in `docs/M2_EVIDENCE.md` (repro,
  change, tests, JVM-vs-native results, CI link) + all CI gates green.

### Wave 2 — tracing, cycle-collecting arena GC (M2_PLAN §4)

- [ ] **T11** `crates/j2r-runtime/src/heap.rs`: `Vec<Entry>` arena,
  generation-tagged `JRef { idx, gen }` (Copy; stale dereference panics
  loudly — no silent aliasing), free list, non-recursive worklist
  mark/sweep, collection thresholds, `J2R_GC_EVERY_N` forced-GC mode; unit
  tests (cycle collected/kept, array children, root classes, stale-handle
  detection + free-list reuse non-aliasing, 1 M-node worklist, re-entrancy
  assertion, threshold math).
- [ ] **T12** Root plumbing (pointer-free protocol, M2_PLAN §4.4):
  `FrameGuard`-owned `RootStack` snapshot slots refreshed at every emitted
  call boundary; direct current-frame scan via
  `GcFrame::current(&locals, &stack, &thrown)`; `NativeRoots` scopes at
  every native→Java dispatch site; emitted `statics.rs` gains
  `gc_scan_roots`; runtime roots (intern cache, boxed cache,
  `System.out`) scanned; registration in emitted `main`.
- [ ] **T13** Ownership conversion: `JRef` → Copy handle, ~45
  `.borrow()/.borrow_mut()` sites → scoped `with_obj` accessors across
  `lib.rs, object.rs, array.rs, natives.rs, concat.rs`; re-issued borrow
  census in `RUNTIME_SAFETY.md` §4.
- [ ] **T14** Identity & hashing: `Object.hashCode`/`System.identityHashCode`
  become the handle value (stable across collections); interned-literal and
  boxed-cache identity preserved; unit tests.
- [ ] **T15** `emit.rs`: `gc_point()` emitted only before the 4 allocating
  opcodes (`new`, `newarray`, `anewarray`, `multianewarray`); debug
  assertion that natives never collect.
- [ ] **T16** Differential fixtures `Cycles.java`, `GcIdentity.java`,
  `InternAcrossGc.java` — suite 36 → 39 — and the **forced-GC full-suite
  proof**: all 39 fixtures byte-identical with a collection at every
  allocation.
- [ ] **T17** Stress acceptance: 200 000 build-and-drop cycle allocations
  with 10 live objects asserts `Heap::high_water() < 64`; differential
  suite wall-clock within ±20 % of the Wave-1 baseline, recorded.
- [ ] **T18** CI gate-9 `gc-stress` (forced-GC suite) + gate-10 `no-unsafe`
  grep, both wired into the all-gates job.

### Wave 3 — typed operand-stack specialization (M2_PLAN §5)

- [ ] **T19** `verify.rs` exports per-pc stack *and* locals shapes
  (`StackShape`); new `specialize.rs` decision pass (per-method
  all-or-nothing over stack positions and local slots) + **written shape
  report** (per-method: stack positions, local slots, fallback reason;
  go/no-go checkpoint 3a — if complexity is out of proportion, the wave
  ends here honestly, or records the reduced scope: stacks-only).
- [ ] **T20** Hybrid emission (3b): typed primitive pools for kind-stable
  stack positions and local slots (references stay in `Vec<Value>` — the
  GC scan surface is unchanged), per-method fallback to uniform emission;
  GcFrame scans specialized frames; "no `Value::` on hot path" unit
  check; deterministic benchmark ≥ 1.3× on arithmetic fixtures **or** an
  honest no-gain finding.

### Wave 4 — CFG reconstruction investigation (M2_PLAN §6)

- [ ] **T21** `crates/j2r-codegen/src/cfg.rs` prototype behind `--emit-cfg`
  (default **off**; default emission unchanged) + `docs/M2_CFG_REPORT.md`
  (dominator-region lifting, irreducible-loop fallback, handler edges,
  switch lowering, go/no-go recommendation) + automated output-equivalence
  test across the whole suite.
- [ ] **T22** M2 completion package: milestone evidence, all gates green,
  external architecture review submission for the milestone (same process
  M1 went through).

### Dependency chain

T01–T06 (done) → T07 → T08 → T09 → T10 → T11 → T12 → T13 → T14 → T15 →
T16 → T17 → T18 → T19 → T20 → T21 → T22. Wave 2 needs Wave 1's frame
registry as its root infrastructure; Wave 3 needs Wave 2's FrameGuard scan;
Wave 4 needs Waves 1–3 stable. **No wave begins before the previous one's
CI evidence exists, and every existing test stays green at every step.**

---

## 5. CI gate evolution

| Gate | Today | M2 change |
| --- | --- | --- |
| 1 fmt / 2 clippy / 3 test / 4 release / 5 standalone / 6 doc / 7 audit / 8 all-gates | green | unchanged, always required |
| 9 gc-stress | — | Wave 2: full differential suite under `J2R_GC_EVERY_N=1` |
| 10 no-unsafe | — | Wave 2: grep gate, mechanical enforcement of zero-`unsafe` |

New gates are always added together with the all-gates job, so a silently
skipped gate is impossible.

---

## 6. Risk register (top items; full detail M2_PLAN §2)

| # | Risk | Mitigation |
| --- | --- | --- |
| R1 | Cyclic graphs leak until OOM abort | Wave 2 tracing GC (T11–T17) |
| R2 | No heap ceiling / catchable OOME | explicitly out of M2 scope — no deterministic differential test exists; revisit with a defensible protocol |
| R3 | Refcount traffic on every stack operation | handles make `Value` clone cheap (Wave 2); typed stacks remove boxing (Wave 3) |
| R4 | `Rc` is `!Send` — blocks Java threads forever | handle-based arena is the thread interface (M2_PLAN §7) |
| R5 | Identity hash = raw address, unstable under reuse | handle-value identity hash, stable for live objects (T14) |
| R6 | `RefCell` reentrancy panics | audited census; re-issued after T13; statement-local borrows only |
| R7 | String payloads outside the object graph | non-moving collector + COW semantics pinned by tests (T16) |
| R8 | No rooting discipline under tracing GC; stale handles could alias recycled objects | Wave-1 frame registry + owned root snapshots are the root infrastructure (pointer-free, M2_PLAN §4.4); generation tags make stale handles loud; forced-GC mode is the proof (T12, T16) |

---

## 7. After M2 (preview only — no promises, no dates)

- **M3 JDK coverage**: real JAR loading, `java.util` collections,
  lambdas/method refs (invokedynamic), reflection, `synchronized` — each
  with differential fixtures and its own task register published before
  coding starts.
- **M4 Minecraft readiness**: JNI, Mixin transformation pipeline, mappings,
  compiling a large library jar differentially clean.
- **M5 mod ecosystem**: Fabric / Forge / NeoForge boot paths.

Discipline stays identical: plan → review → ordered waves → evidence →
external review. Nothing sloppy, nothing untested, nothing claimed without
a CI run behind it.
