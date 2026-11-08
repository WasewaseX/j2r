# M5_PLAN — Minecraft readiness (rev 0)

**Status: rev 0 draft (2026-11-08).** Per the standing owner directive
("continue never stop"), waves start immediately after the draft lands and
every task stays unchecked until it has recorded runnable evidence, same
discipline as M2/M3/M4. Owner review of the plan can still force changes
after the fact. The milestone inherits the two iron rules: no
compatibility claims without differential evidence, zero `unsafe`
(`#![forbid(unsafe_code)]` stays in every crate root — including the JNI
wave, which is designed safe-Rust-only, §3).

**Why M5 exists.** M4 delivered threads & concurrency (all four waves,
CI-verified). The renumbering (M4_PLAN §0) makes M5 = Minecraft readiness.
Minecraft is the north star; a milestone that never measures itself
against that star is a roadmap label, not a milestone. M5's job: remove
the four registered structural blockers between "a real library compiles"
(today) and "Minecraft-scale input is tractable" — scale, JNI, mappings,
mixins — and keep the steady-state performance lever moving (inline
caches).

---

## §0 Scope

In scope (the five registered work items, TODO.md §M5):

1. **Scale** — incremental compilation for 70k+ classes.
2. **JNI bridge** for LWJGL/GLFW/OpenAL (R3, safe-Rust design).
3. **Mixin application** at the bytecode level before j2r (R1).
4. **Mojang/intermediary mappings pipeline.**
5. **Inline caches** for megamorphic call sites.

Explicit non-goals for M5 (loud-unsupported, documented per class):

* Booting the actual Minecraft client / server — that is M6 territory
  (mod ecosystem) and needs the M5 deliverables first. M5's bar is
  *readiness evidence*, not a compatibility claim. The standing law
  ("never claim Minecraft compatibility without milestone evidence")
  stays in force.
* Runtime bytecode rewriting, runtime class generation, javaagent-style
  instrumentation (R1 v1 non-goals, STRATEGIC_GAPS §R1.3).
* `sun.misc.Unsafe` native paths, JVMTI, JIT shortcuts (R3 v1 exclusions).
* `java.util.concurrent` classes — M4 registered them as M5+ candidates;
  they enter only if a wave's evidence demands one (e.g. mixin fixtures),
  and then as their own task with differential fixtures.
* Full Fabric/Forge/NeoForge loader surface — M6.

---

## §1 Architecture audit — the M5 seams, verified against source (2026-11-08)

| # | Fact (verified today) | Location |
| --- | --- | --- |
| 1 | The classpath closure worklist recomputes `referenced_user_classes` for **every** parsed class on **every** worklist iteration, and the referer lookup (`classes.iter().find(...)`) rescans each class's reference set per wanted name — quadratic minimum, no per-class cached reference set | crates/j2r-codegen/src/lib.rs:492–580 |
| 2 | Whole program = one emitted crate: `compile()` builds the full `Program`, `emit_crate` writes one Cargo project, `cmd_build` runs one `cargo build` | crates/j2r-codegen/src/lib.rs:1308–1312; crates/j2r-cli/src/main.rs:385–430 |
| 3 | The closure has a convergence guard at 100 000 processed rounds — a scale ceiling already encoded | crates/j2r-codegen/src/lib.rs:566–571 |
| 4 | rustc SIGKILLs the release build of the JZlib-generated crate at every opt level on the 4 GB sandbox (mega-functions from giant switch tables); registered as a codegen-scale work item: "M5's 70k-class target cannot be one giant emitted function per method-table monster" | docs/BENCHMARKS.md ("First steady-state comparison"); docs/CORPUS_WAVE.md §Wave 2 |
| 5 | Zero JNI surface exists in the runtime (no `jni` module; `rg -i jni crates/j2r-runtime/src` is empty) | crates/j2r-runtime/src/ |
| 6 | Zero mappings/remap surface exists (no mojmap/intermediary/Tiny-v2 parsing anywhere) | crates/ |
| 7 | Reflection is a static subset with no metadata generator (R2); M3_EVIDENCE §692 registers the M5 JDK-coverage follow-ups | docs/STRATEGIC_GAPS.md §R2; docs/M3_EVIDENCE.md |
| 8 | Steady-state lever state at M5 start: typed+raw pair at 0.936 (6.4% cumulative movement 0.992 → 0.958 → 0.936); remaining named prerequisite the block-join/entry-var layer (~140 slot-var sites) | TODO.md §boxing wave; docs/TYPED_REGISTER_READINESS.md |

Facts 1 and 3 are the measured-scale story's seed: T01 quantifies them,
wave 1 removes them. Fact 4 means scale is not only a j2r problem —
rustc's view of our emission shape is half the cliff.

---

## §2 Design — scale & incremental compilation (wave 1)

**Measured first (T01).** A deterministic synthetic class farm
(`scripts/m5_farm_gen.py` → javac → farm jar) with a realistic reference
shape: each class extends a common base, implements an interface, holds
fields, methods with string concat + arrays + statics, and references 8
other farm classes (5 forward-biased, 2 backward, 1 long jump — all mod N,
so the whole web is one component rooted at C00000 and reachable from
Main). Sizes 1k / 5k / 20k (70k only if the curve allows). Stages timed
separately:

| Stage | How measured |
| --- | --- |
| closure + parse + IR + emit | `j2r compile --classpath farm.jar --main-class farm.Main` wall time |
| cargo build | `j2r build` total minus compile stage; debug first (release known-SIGKILL risk, fact 4) |
| peak RSS | `/usr/bin/time -v` on both stages |

Deliverable: a measured stage curve + the cliff named with numbers, not
adjectives. The census table is committed with the task.

**Then the design, from the measurement:**

1. **Closure cache (mechanical, fact 1).** Parse-time extraction of each
   class's referenced-name set (stored on `ClassPlan`), worklist walks the
   delta sets; referer = the class whose cached set contains the wanted
   name. Target: closure stage O(total refs), not O(classes² × refs).
2. **Incremental recompiles (T06/T07).** `j2r build` learns a build
   manifest: per-class input hash → emitted-file hash; unchanged classes
   re-emit byte-identical output and cargo's fingerprint skips them.
   Design constraint: the emitter must be deterministic per class given
   the same `Program` inputs (verified by re-emit-identical fixtures).
   Cross-class resolution (vtable layouts, override sets) is global
   state — the manifest records the global digest that feeds emission, and
   any change to it invalidates dependents (conservative first: any
   global-digest change re-emits everything; refinement only with
   measured need).
3. **rustc-side scale (fact 4).** Split-table / smaller-function emission
   is already registered (CORPUS_WAVE §Wave 2). The census decides the
   shape: per-class modules (mod tree) vs per-class functions with
   smaller dispatch tables vs `[inline(never)]` partitioning. rustc
   codegen-units already parallelize within one crate; the measurement
   decides whether that is enough.

**Acceptance (wave 1):** the farm jar at 20k classes compiles + builds
end-to-end with recorded wall time and RSS on this sandbox; the 1k → 20k
stage curve is super-linear nowhere above the named baseline; a
no-change `j2r build` re-run does less work than the first (measured);
differential battery stays green at every step.

---

## §3 Design — JNI bridge (wave 2, the registered blocker)

STRATEGIC_GAPS §R3 is the design of record; wave 2 implements it:

1. **`j2r-runtime::jni`**: `JNIEnv` + `JNI_InvokeInterface` as static
   tables of `extern "system"` fn pointers, index-for-index identical to
   `jni.h`; layout pinned by a compile-time assertion table (a
   `const`-evaluated index check per slot — drift is a compile error, not
   a segfault in someone else's binary). No `unsafe` in *our* code; the
   `extern "system"` pointers point at safe Rust fns. Native libraries
   calling *into* us receive plain function pointers — their FFI is
   theirs, our side stays safe.
2. **Object model bridge**: `jobject` = opaque handle into the GC handle
   table; local refs = frame-pushed handles, `Push/PopLocalFrame` = frame
   pop, global refs = pin set (the M2 generation-tagged design). Arena
   non-movement makes `GetPrimitiveArrayCritical` pinning trivial —
   documented advantage, not accident.
3. **v1 surface**: `GetObjectClass`, `FindClass`, field IDs + accessors,
   `Call*Method`/`CallStatic*Method` (arity-shimmed, no libffi),
   `RegisterNatives`, modified-UTF-8 string marshalling, array families,
   local/global ref management.
4. **Evidence**: a C-compiled test `.so` exercising every v1 function,
   run against Temurin and the j2r binary with identical observable
   results. This harness gates all LWJGL bring-up.

The two flagged-open questions get written answers as their own tasks:
variadic `Call*Method` ABI (arity-shim trade-off note, T11) and nothing
else — the class-identity question belongs to the mixin wave (§4).

**Acceptance (wave 2):** the C harness green on both sides, three
channels; `RegisterNatives` round-trip demonstrated (a native fn bound
from Rust called from Java code); the JNI suite pinned in CI.

---

## §4 Design — mappings + mixins (waves 3–4, ordered dependencies)

Mixins name their targets in the runtime namespace (intermediary in the
Fabric pipeline), so the mappings stage must exist before the mixin
applier can be tested against real-shaped input.

**Wave 3 — mappings pipeline.** Tiny v2 parser (file format is
line-oriented, versioned header), two loaded maps (intermediary ↔ mojmap,
class + field + method + descriptor rows), and a remap stage that rewrites
classfile constant-pool `CONSTANT_Class`/`NameAndType` entries **before**
j2r proper (a separate pipeline stage, like the mixin applier — never a
compiler hack). Evidence: a real-shaped mappings file (Fabric's published
intermediary map is the target; a synthetic Tiny v2 file is the unit
fixture), remap parity vs a reference remapper on a fixture jar
(name-for-name), and the remapped jar still compiles + runs
differentially clean under j2r.

**Wave 4 — mixin applier.** Bytecode-level rewrites before j2r:
`@Inject` (head/tail insertion via copied insn lists + localvar
remapping), `@Redirect` (target call-site swap), `@Accessor`/`@Invoker`
(synthetic accessor generation). Each rewrite rule is a separate task
with its own differential fixture: apply the mixin, then compare
observable behavior of the transformed class under Temurin (real Fabric
runtime) vs the j2r binary. **Class-identity equivalence** — Mixin's
classloader-delegation/timing assumptions vs our ahead-of-time pipeline —
gets its written design answer as the wave's first task (the
STRATEGIC_GAPS flagged-open question); no rewrite rule lands before it.

**Acceptance (waves 3–4):** a real Fabric mod containing at least three
mixins across `@Inject`/`@Redirect`/`@Accessor` behaves identically on
both sides (the STRATEGIC_GAPS R1 evidence bar, verbatim).

---

## §5 Design — inline caches (wave 5)

The steady-state lever's next slice (fact 8: 0.936 and moving). Megamorphic
call sites: profile-free, static policy first — per-site dispatch cache
guarded by the receiver's vtable identity (the vtable pointer IS the
type check: compare, then call the cached target; miss falls to the
existing dispatch path). Sites chosen by the census (T22: static
fan-in per call site across the corpus + farm). The bench battery
(BENCHMARKS protocol) decides GO/NO-GO with the same honesty rule
that has governed every lever step: a measured NO-GO is a valid
registered outcome.

**Acceptance (wave 5):** the megamorphic-site census table committed;
either a measured win on the bench rows (recorded, kept) or a measured
NO-GO (recorded, registered reason) — both close the wave honestly.

---

## §6 Task register

Status: `[ ]` pending · `[~]` started · `[x]` done (with evidence).

### Wave 0 — measurement & baseline

- [x] **T01** Scale census: farm generator + 1k/5k/20k stage-curve
      measurement (compile / cargo / RSS per stage), cliff named with
      numbers, table committed. → §2 (LANDED 2026-11-08: cliff = rustc
      ~4.2 MB/class, one-process ceiling ~900 classes on 4 GB; evidence
      in TODO.md §M5 T01 + scripts/m5_census.csv)
- [x] **T02** Census-driven scale design note: the measured curve decides
      the closure-cache, manifest, and emission-shape order. → §2
      (LANDED: docs/M5_SCALE.md — link-table crate partitioning chosen)

### Wave 1 — scale & incremental compilation

- [x] **T03** Closure cache LANDED (2026-11-08): ClassPlan.refs cached
      at parse time; worklist over deltas; referer from cache. Compile
      stage 0.40→0.29s at 1k, 7.98→2.02s at 5k (−75%, near-linear). → §2.1
- [x] **T04** Partition differential LANDED (2026-11-08):
      tests/partition_suite.rs — 6 fixtures × forced k=2, three-channel
      vs Temurin, CI-pinned; farm rows byte-identical at 250/1000/5000
      (recorded in T05).
- [x] **T05** Crate-partitioned emission LANDED (2026-11-08):
      `--partition-classes K` workspace (support/p0..pN/app), link table
      for cross-partition calls, composite Dispatch. Acceptance met:
      farm 1000 builds 1.91 GB (<4 GB, was SIGKILL single-crate), 5000
      builds 1.89 GB; BYTE-IDENTICAL vs Temurin at 250/1000/5000;
      default path re-proven (cfg_equiv zero divergences, diff_suite
      109, corpus_suite). Link-table Mutex→OnceLock deadlock fix
      recorded in TODO.md §T05. Split-table/JZlib re-test next. → §2.3
- [ ] **T06** Build manifest: per-class input-hash → emitted-hash, no-op
      rebuild measured cheaper. → §2.2
- [ ] **T07** Global-digest invalidation: cross-class resolution changes
      invalidate dependents conservatively; correctness pinned by
      dirty-rebuild fixtures. → §2.2

### Wave 2 — JNI (the blocker)

- [ ] **T08** `j2r-runtime::jni`: JNIEnv/InvokeInterface static tables,
      compile-time index assertions. → §3.1
- [ ] **T09** Handle-table bridge: jobject ↔ GC handles, local/global
      refs, Push/PopLocalFrame, Critical pinning note. → §3.2
- [ ] **T10** v1 surface: GetObjectClass/FindClass/field
      accessors/arrays/modified-UTF-8 strings. → §3.3
- [ ] **T11** Arity-shim trade-off note (variadic `Call*Method` ABI, the
      flagged-open question) + the `Call*Method`/`CallStatic*Method`
      families. → §3
- [ ] **T12** `RegisterNatives` + round-trip demo. → §3
- [ ] **T13** C test harness `.so`: every v1 function, Temurin vs j2r
      parity, pinned in CI. → §3.4

### Wave 3 — mappings pipeline

- [ ] **T14** Tiny v2 parser + unit fixtures. → §4
- [ ] **T15** Remap stage before j2r (constant-pool rewrite),
      name-for-name parity vs reference remapper on a fixture jar. → §4
- [ ] **T16** Real intermediary-map load + remapped jar compiles and runs
      differentially clean. → §4

### Wave 4 — mixin applier

- [ ] **T17** Class-identity equivalence design answer (the
      flagged-open question; no rewrite rule before it). → §4
- [ ] **T18** `@Inject` rewrite + differential fixture. → §4
- [ ] **T19** `@Redirect` rewrite + differential fixture. → §4
- [ ] **T20** `@Accessor`/`@Invoker` rewrite + differential fixture. → §4
- [ ] **T21** Real Fabric mod, ≥3 mixins across the three families,
      identical behavior both sides (R1 evidence bar). → §4

### Wave 5 — inline caches

- [ ] **T22** Megamorphic-site census (static fan-in per site, corpus +
      farm). → §5
- [ ] **T23** Vtable-guarded dispatch cache; bench battery GO/NO-GO
      recorded either way. → §5

Standing evidence rules for every task: full gate battery green before
push; differential claims are three-channel vs Temurin 21; perf claims
carry the measurement protocol (median-of-3, staged logs); a census or
design note is committed evidence, not a private sketch.
