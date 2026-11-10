# J2R — Strict Review Checklist (the milestone-completion gate)

Derived from the external readiness review of 2026-11-10 (P0–P2 priority
order) and bound on THIS repository's coding agent. Purpose, verbatim from
the review: **a milestone cannot be claimed complete just because the code
compiles.** Every item below has explicit PASS criteria, FAIL conditions,
and an explicit list of what does NOT count as evidence.

Standing definitions used throughout:

* **Three-channel differential proof** — stdout, stderr and exit status
  byte-identical vs Temurin 21.0.13+9 (stderr with only stack-trace frame
  lines normalized). This is the ONLY definition of "behaves identically".
* **Runnable evidence** — a committed test name, CI gate, fixture, or
  measurement file (`scripts/evidence/`) that a fresh `cargo test` / script
  run can reproduce. Prose, commit messages, and this file are NOT evidence.
* **The boards win** — `console.md` + `console_summary.md` are the
  authoritative state; if a claim anywhere disagrees with them, the claim
  is wrong until the boards are updated WITH evidence.

---

## G0 — Standing gates that never pause (every landing, every priority)

| # | Gate | PASS | FAIL / does NOT count |
|---|------|------|-----------------------|
| G0.1 | fmt + clippy | `cargo fmt --all` clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` exits 0 | "mostly clean", warnings fixed "later" |
| G0.2 | Full battery | workspace tests 0 failures; `diff_suite` (109 fixtures, three-channel vs Temurin) and `cfg_equiv` zero divergences re-run on the landing commit | a suite skipped because "no codegen change" when ANY runtime/FFI/behavioral path changed |
| G0.3 | gate-10 unsafe pin | no `unsafe` outside `j2r-jni-ffi/src/ffi.rs`; occurrence count ≤ 12 and equal to `ACTUAL_UNSAFE_OCCURRENCES`; every occurrence carries a `// SAFETY:` justification naming the JNI contract | relaxing the pin without a committed design note + owner review in the SAME commit |
| G0.4 | doc-count sync | `scripts/check_doc_counts.py` exit 0 (README measured, not claimed) | hand-edited counts |
| G0.5 | boards + push | `console.md` + `console_summary.md` updated and PUSHED in the same landing; un-pushed = un-protected | local-only commits at session end |
| G0.6 | loud-unsupported law | every unimplemented feature fails loudly with a named error — never a silent wrong answer | a "probably fine" fallback |
| G0.7 | honest numbers | every perf claim carries: median protocol, same-session control pair, environment (2-core Xeon / 4 GB box class), and byte-identical-output proof; startup-dominated and steady-state numbers never quoted against each other | quoting a ratio without its regime |

## G1 — P0: Finish and prove the JNI bridge (M5 wave 2, T13c/T13d + acceptance)

State at checklist creation (2026-11-10): T08–T12 COMPLETE; T13 13a+13b
landed (primitive 11 `call_foreign_native` proven against a REAL
C-compiled probe; register-window loudness pinned through the full runtime
path; 444 green). The registered pointer/out-slot vs zero-unsafe issue
(review item) is F-J1 — RESOLVED by the audited `j2r-jni-ffi` crate under
the amended gate-10 (G0.3); it stays resolved only while G0.3 holds.

- [x] **G1.1 — the C harness exists and is ONE source.**
  PASS (2026-11-10, T13 13c): `tests/jni_harness/harness.c` compiles to
  `libjni_harness.so` with the pinned toolchain (`cc -shared -fPIC`
  against the pinned JDK's `jni.h` of record, done by the parity runner
  in `crates/j2r-cli/tests/jni_parity.rs`); the SAME `.so` runs against
  Temurin (via `System.loadLibrary`) and against the j2r binary (the
  same `System.loadLibrary`, served by the 13c loading story — audited
  primitive 12); no side-specific code paths beyond the loading
  mechanism. Byte-identical three-channel evidence: the parity test.
- [x] **G1.2 — every v1 function exercised from real C.**
  PASS (2026-11-10, T13 13c): the harness drives the inventory —
  versioning/VM (GetVersion/GetJavaVM/the GetEnv matrix),
  classes/fields (IDs + instance/static accessors, verified Java-side),
  the Call*MethodA families (virtual/static/nonvirtual) + the exception
  trio round-trip, the Call*MethodV forms (REAL-variadic save areas AND
  hand-built `__va_list_tag` records), the five mUTF-8 string slots
  both directions incl. the encoded-NUL and supplementary-pair edges,
  the array families with all three release modes + criticals, and
  RegisterNatives/UnregisterNatives (the binding IS the skeleton;
  rebind-override and the unbound UnsatisfiedLinkError law both
  demonstrated). The full inventory table with the honest exclusions
  (weak global refs, cross-thread local-resolve, Monitor*,
  GetStringRegion — still loud/documented v1 gaps) is
  M5_JNI_HARNESS.md §8; 13d asserts the loud arms.
- [x] **G1.3 — three-channel parity, both directions.**
  PASS (2026-11-10, T13 13c): `crates/j2r-cli/tests/jni_parity.rs`
  compiles the `.so` once, runs BOTH runtimes, and asserts stdout +
  stderr + exit RAW-identical (stderr expected EMPTY on both sides — no
  normalization needed on the happy path). Native→J2R calls demonstrated
  (GetEnv/attach-eligible env use from C: every probe) and J2R→native
  calls demonstrated (RegisterNatives-bound C natives invoked through
  the 12c call-through → primitive 11). The battery is SELF-CHECKING
  (the driver exits 3 on any missed verdict on BOTH runtimes), so the
  parity cannot silently preserve a wrong answer.
- [x] **G1.4 — attach lifecycle.**
  PASS (2026-11-10, T13 13d): the oracle's attach contract MEASURED first
  (scripts/evidence/attach_lifecycle_probe/ — fresh-thread GetEnv =
  JNI_EDETACHED, attach/re-attach/AsDaemon = JNI_OK idempotent, detach =
  JNI_OK ALWAYS incl. three consecutive detaches, GetEnv-after-detach =
  EDETACHED again); the VM slots 4/5/7 are real and implement exactly
  that contract (jni::attach registry + the 12b-wired detach_thread hook
  — the frame-stack sweep runs on detach); the generated main_run marks
  the main thread attached at birth; the harness's `probe_attach_lifecycle`
  spawns a pthread, walks every measured arm, runs the self-checking
  battery ON the attached env in BOTH attach cycles (verdict 2047), and
  the SAME `.so` answers identically on Temurin (measured standalone
  first) and j2r — three channels byte-identical incl. `P attach=2047`
  (`crates/j2r-cli/tests/jni_parity.rs`). Runtime-side pin:
  `attach_lifecycle_matches_the_measured_oracle` on a fresh thread.
- [x] **G1.5 — CI pinning.**
  PASS (2026-11-10, T13 13d): `gate-22-jni-c-harness-parity` — a
  dedicated CI job that compiles the harness with cc, runs BOTH sides
  (Temurin via System.loadLibrary; j2r via the 13c loading story),
  compares three channels, and is wired into the all-gates aggregate
  (17 upstream gates). A missing JDK/compiler skips LOUDLY (the
  soe_e2e law); the CI image has both, so a skip there is a visible
  anomaly, not a silent green.
- [x] **G1.6 — no casual unsafe growth.**
  PASS (verified through T13 completion, 2026-11-10): every primitive
  landing so far amended gate-10 in the SAME commit (T09 5, T10-s2 +1 =
  6, T11 11c +3 = 9, T13 13b +1 = 10, T13 13c +1 = 11) with its SAFETY
  justification and its M5_JNI_FFI.md §4 register entry; the
  source-scanned count assert passes (11 of pin 12 — one headroom slot
  left, tracked). T13 13d added ZERO new primitives (the attach
  lifecycle reuses audited primitive 3); gate-10 unchanged.

**M5 wave-2 (JNI milestone) completion = G1.1–G1.6 ALL checked with the
evidence committed. Nothing else — not "the code compiles", not "the unit
tests pass", not "the mock dispatch works".**

## G2 — P0: The 70k-class scalability path (wave-1 follow-ups F1–F4)

State: 20k demonstrated and RE-MEASURED after the G2.1 split (session
11: cold ≈17.1 min baseline → post-split trio 42.1 s no-op / 66.0 s
one-edit, parity PASS, binary ≈1.01 GiB; worst single generated-unit
rustc ≈0.99 GB at k=250; build peak 3.09 GB = rust-lld, the link stage
= G2.1b). 70k is NOT demonstrated and may not be claimed.

- [x] **G2.1 — split the app dispatch composite (F3).** CLOSED session
  11, both arms measured: (1) the Dispatch/link-table emission is split
  (per-partition `register_defaults`/`register_constants` bounded by k +
  app-side Vec composition + static per-partition route tables indexed
  by `part_of`) so no generated Rust compilation unit scales with TOTAL
  class count — MEASURED directly at 20k: every rustc ≤ 1,005,860 KB
  (partition crates, uniform), support rebuild 0.83 s / 119 MB, app
  sources ≈21 KB; (2) the 20k number re-measured: worst generated-unit
  rustc 3.49 GB → 0.99 GB (−72 %); build peak 3.49 → 3.09 GB with the
  attribution MOVED to rust-lld (registered G2.1b); partition
  differential byte-identical (k=2 + full battery green, 452 tests,
  zero divergences); 5k A/B peak 1.89 → 1.26 GB (−33 %), wall −14 %.
  Evidence: `scripts/evidence/g2_1_split/` (raw .time files,
  per-process VmHWM top table, 20k remeasure log, parity stdouts).
  Honest caveat recorded: the baseline "3.49 GB app-bin rustc" label
  conflated rustc with its lld child (RUSAGE_CHILDREN); the new
  `scripts/proc_peak_sampler.py` separates them.
- [ ] **G2.1b — the link-stage wall (registered session 11 from the
  G2.1 re-measure).**
  With every rustc bounded, the 20k build peak is `rust-lld` at
  3.09 GB linking the ≈1.08 GiB debug=0 binary — link memory scales
  with BINARY size, i.e. still with TOTAL class count, in a different
  process. PASS: the 20k link peak is measured under the 4 GB budget
  (now 3.09/4.0 GB) AND either (a) a link-memory mitigation lands
  (linker choice/flags measured, or binary-size reduction) with the 20k
  link peak re-measured, or (b) the boundary is accepted in writing
  with the measured number and the per-process sampler wired into the
  standing 20k/70k protocol. FAIL: silence, or running the G2.5 ladder
  into this wall unmeasured.
- [ ] **G2.2 — no-op rebuild fast path (F2).**
  PASS: the SHA-256-rehash no-op cost drops via the (mtime,size) fast path
  with the manifest remaining a CACHE (corrupt/tampered → full re-emit,
  loud); the 20k no-op number re-measured and recorded with raw files in
  `scripts/evidence/`.
- [ ] **G2.3 — emission-stage profile (F1).**
  PASS: a profile of the 20k emit stage exists (named hot function, not
  adjectives) and either a fix lands or the super-linearity is accepted in
  writing with its measured constant.
- [ ] **G2.4 — release-profile 20k trio (F4).**
  PASS: the cold/no-op/one-edit trio re-measured under `--release` on a
  disk-guarded protocol, raw evidence committed.
- [ ] **G2.5 — progressive scaling, honest ladder.**
  PASS: N is raised stepwise (e.g. 35k → 50k → 70k) with per-step stage
  tables; the 70k target is claimed ONLY when a reproducible run on THIS
  box class completes end-to-end with (a) JVM three-channel differential
  verification, (b) no OOM/SIGKILL at any stage, (c) peak memory recorded
  per stage, (d) raw measurement files committed. FAIL: extrapolating the
  curve and writing "~70k should fit".
- [ ] **G2.6 — the fat-method regression the review flagged (registered
  here as F6).**
  Context: partitioning bounds CRATE size; it does NOT split one enormous
  generated method. The JZlib release build SIGKILLed rustc at every opt
  level (peak 3.03–3.56 GB, borrow-check/MIR phase) on this 4 GB box —
  never closed, only bounded by machine memory.
  PASS (either arm, honestly recorded): (a) split-table/smaller-function
  emission lands and the JZlib generated crate completes `--release -O3`
  on this box with byte-identical output, or (b) the limit is registered
  as a machine-bound boundary with the measured number, a named follow-up
  (the emission fix), AND a CI-visible regression test that fails loudly
  the day the emitted function grows past a committed size/shape threshold
  (a proxy test on the emitter side — e.g. per-method instruction /
  switch-table census with a pinned ceiling). FAIL: silence, or a claim
  that "partitioning fixed the fat-method problem" (it did not).
- [ ] **G2.7 — the review's memory-budget framing.**
  PASS: the 70k acceptance names a memory budget up front (this box class:
  4 GB/process, disk-guarded) and the run is measured against THAT budget,
  not against a bigger machine.

## G3 — P1: Typed-codegen performance (the ~100× steady-state gap)

State: lever ledger structuring-flat → typed-locals 0.992 → operand
unification 0.958 → typed-payload arrays 0.936 (6.4% cumulative); steady
state ~66× behind the full-tier JVM on JamaBench; ~73–104× on StringsSimBench.

- [ ] **G3.1 — the deferred-join mechanism** (tred2/orthes/hqr2 refusals,
  the registered next CFG layer). PASS: lands behind `--emit-cfg`, the
  three kernels lift, cfg_equiv stays zero-divergence on the whole suite,
  and the same-session controlled pair is recorded EITHER WAY (a measured
  NO-GO is a valid recorded outcome; a silent no-measurement is not).
- [ ] **G3.2 — the block-join/entry-var layer** (~140 slot-var sites, the
  named next prerequisite). Same PASS shape as G3.1.
- [ ] **G3.3 — primitive-specialized collection paths** (the measured
  2.78× container tax, addendum 7). PASS: design lands after the
  entry-var layer per the register; the BoxingBoxed/BoxingPrim pair
  re-measured; output byte-identical.
- [ ] **G3.4 — the acceptance bar.** PASS: repeatable (≥2 sessions,
  same protocol) improvements on BOTH JamaBench and StringsSimBench with
  byte-identical output AND zero diff-suite regressions on every landing;
  every number carries its same-session control pair (G0.7). FAIL: a win
  quoted from a stale baseline (the 1.31× machine-drift lesson).
- [ ] **G3.5 — honesty guard.** The gap may be reported as "improving" but
  the ~100× (now ~66×) figure stays quoted until a measured order-of-
  magnitude change; no "approaching JVM performance" phrasing without the
  number.

## G4 — P1: Close the concurrency review (M4 + the JMM boundary)

State: all four M4 waves implemented and CI-verified; the owner completion
review is PENDING (registered); volatile and final-field freeze semantics
do NOT exist (RUNTIME_SAFETY.md §6 documents the boundary).

- [ ] **G4.1 — the M4 owner review completes** with a recorded verdict
  (approve / approve-with-follow-ups / request-changes) in the boards.
- [ ] **G4.2 — the memory-model boundary is enforced, not just
  documented.** PASS (either arm): (a) volatile/final-freeze implemented
  with litmus-suite evidence per STRATEGIC_GAPS R4 (the JMM→Rust mapping
  table + SB/CoRR/IRIW/message-passing litmus tests asserting the ALLOWED
  outcome set under stress), or (b) the boundary stays as documented AND
  gains machine enforcement — a CI check that fails loudly when a field
  marked `volatile`/`final` in input bytecode is compiled to a plain
  non-atomic access WITHOUT the documented boundary warning in the
  generated crate (the incompatibility must be visible at the use site,
  not only in a doc). FAIL: dropping the documentation, or claiming
  JMM conformance that §6 does not support.

## G5 — P2: Mappings, mixins, inline caches (ordered AFTER G1/G2 per the review)

- [ ] **G5.1 — T14–T16 mappings.** PASS: Tiny v2 parser with unit
  fixtures; the remap stage rewrites constant-pool names BEFORE j2r as a
  separate pipeline stage; name-for-name parity vs a reference remapper
  on a fixture jar; the remapped jar compiles and runs differentially
  clean; a real Fabric intermediary map loads.
- [ ] **G5.2 — T17–T21 mixins.** PASS: the class-identity equivalence
  design answer (T17) lands BEFORE any rewrite rule; @Inject/@Redirect/
  @Accessor each carry a differential fixture vs Temurin; the wave's
  acceptance is a real Fabric mod with ≥3 mixins across the three
  families behaving identically both sides (three channels). FAIL: any
  rewrite rule landing before T17; runtime bytecode rewriting (v1
  non-goal).
- [ ] **G5.3 — T22–T23 inline caches.** PASS: the megamorphic-site census
  table committed; vtable-guarded caches either measured-and-kept or
  measured-NO-GO-recorded on the bench battery; never a speculative
  landing.
- [ ] **G5.4 — ecosystem gating.** Fabric/Forge/NeoForge work (M6) does
  not START until G1 and G2 are both checked. The review's words are the
  gate: finish the JNI bridge and scalability gates first.

## G6 — Documentation and evidence discipline (the review's §4, standing)

- [ ] **G6.1 — the machine-checked status register.** The doc-count gate
  (G0.4) covers counts; the SAME discipline extends to milestone status:
  M5_PLAN §6, TODO.md, docs/ROADMAP.md and README's status line may only
  change in the same commit as the evidence they record (this checklist's
  first enforcement pass fixed the T11/T12/T13 drift — kept true by
  G0.5's boards discipline). A drift found later is a bug in the process,
  fixed at the source.
- [ ] **G6.2 — two meanings of "T08" never conflated** (the M2-era
  fat-method T08-a vs the JNI function tables T08): every task reference
  in prose carries its milestone context.
- [ ] **G6.3 — evidence files outlive the sandbox:** raw measurement
  files land in `scripts/evidence/` in the claim's commit (lesson #11).
- [ ] **G6.4 — the review cadence.** Each P0/P1 area above gets its
  verdict recorded in the boards when its checks complete; the checklist
  itself is updated in the same commit (never re-written from memory).

---

## What this checklist is

The pass/fail contract between the owner and the coding agent for the
rest of M5. It converts the external review's priorities into the repo's
own evidence discipline: nothing is complete because it compiles; nothing
is complete because a unit test passes; it is complete when the named
runnable evidence exists, the differential battery is green, the boards
say so, and the work is pushed.
