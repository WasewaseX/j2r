# M6_PLAN — Mod ecosystem (rev 0)

**Status: rev 0 DRAFT (2026-11-11), awaiting owner review.** Written per the
plan-first discipline every milestone before it followed (T02's law: the
bottleneck analysis and plan land before any code). Every task in §6 is
unchecked and stays unchecked until the plan is accepted AND the task has
recorded evidence, same standing discipline as M2/M3/M4/M5. The plan
inherits the iron rules unchanged: no compatibility claim without
three-channel differential evidence vs Temurin 21; zero `unsafe` outside
the audited `j2r-jni-ffi` FFI core (11 occurrences, gate-10 pin ≤ 12);
every commit passes all gates; raw evidence committed under
`scripts/evidence/` in the same commit as the claim it supports.

---

## §0 Scope and the ordering law

M6 = **mod ecosystem** (ROADMAP, renumbered per M4_PLAN §0; was "M5" before
the renumbering). The ROADMAP acceptance bar is verbatim: **a real mod
boots in the M5 runtime**. The loader column names Fabric / Forge /
NeoForge; this plan scopes v1 to Fabric and records the Forge/NeoForge
boundary honestly (§5).

**The review's ordering gate, checked:** REVIEW_CHECKLIST G5.4 says
"Fabric/Forge/NeoForge work (M6) does not START until G1 and G2 are both
checked." Both gates are checked with evidence — G1.1–G1.6 closed session 8
(2026-11-10; the T08–T13 JNI surface, 162 real slots, attach lifecycle
measured, gate-22 C-harness parity) and G2.1–G2.7 closed sessions 11–13
(G2.4 release trio `262e038`; G2.5 honest ladder + ZIP64 + G2.7 budget
framing `7b6680c`; G4.2 JMM enforcement `2f795ec`). Planning may therefore
start — and this document is exactly the required plan-first step, NOT the
start of implementation.

**Inheritance, not duplication.** The mappings, mixin and inline-cache
work is DESIGNED and REGISTERED already: M5_PLAN §4/§5, task register
waves 3–5 (T14–T23), review bar G5.1–G5.3. Those stay owned by M5_PLAN §6
and land BEFORE the mod-surface waves below, because of M5_PLAN §4's
ordered dependency (mappings stage before mixin applier — mixins name
targets in the runtime namespace) and because the boot bar in §4 here
includes mixins (a real mod without its mixins is not the real mod).

**Out of scope v1 (loud-unsupported, named errors, never silent):**

* Runtime bytecode rewriting, runtime class generation, javaagent-style
  instrumentation — R1 non-goal 3, unchanged.
* Mods that compute mixin targets at runtime — closed world (R1 design
  answer 1): every mixin target must be inside the compilation set,
  resolved from the mixin configs before codegen; a target outside the
  set fails loud with a named error.
* Dynamic mod discovery after assembly (a second mod dropped in at runtime)
  — the assembly is fixed at `j2r build` time.
* Forge/NeoForge loader surface — FML's layered `SecureJarHandler`
  classloading and Forge's own remapping pipeline are a different beast
  from Fabric's flat loader; §5 designs the boundary note, no
  implementation in v1.
* Jar-in-jar nested mods (`fabric.mod.json` `jars` key) — loud-unsupported
  v1, named error, tracked in the matrix (T27).

---

## §1 Architecture audit — the M6 seams (verified against source, 2026-11-11)

Verified this session against the recovered tree at `63f34a7`. Two rows are
explicitly **to verify at wave 0** and are marked so — the plan does not
pretend an audit it has not done (the T02/T03 precedent: measure first).

| # | Seam | State today (verified unless marked) | M6 gap |
| --- | --- | --- | --- |
| 1 | CLI verbs | `j2r compile` / `j2r build` / `j2r test` exist (crates/j2r-cli/src/main.rs §33–135): build = translate + cargo build; test = javac+java vs j2r+cargo, three-channel | None of them parses mod metadata; `build` takes loose inputs + classpath, not mod-jar semantics — the verbs are EXTENDED, not created (§2) |
| 2 | Jar/zip reader | j2r-classfile zip.rs — classic + ZIP64 (locator + ZIP64 EOCD + per-entry extras), 11 inline tests incl. a true 65,537-entry archive and the real JDK jar | None for the v1 input shape (mod jar = ordinary zip) |
| 3 | Incremental manifest | SHA-256 input→emitted tracking (T06/T07) + G2.2 fast path (20k no-op emit ≈ 5 s; no-op release cargo 0.36 s) | The mod pipeline must cache at STAGE level (metadata → applier → remap → codegen) so the dev loop re-runs only changed stages (§2) |
| 4 | Partitioned emission | `--partition-classes` (T05); G2.5 ladder: 70k emit 53.14 s / 3.16 GB peak, inside the 4 GB budget; end-to-end 70k ≈ 17 GB volume (box-class boundary, recorded) | A mod compiles against Minecraft-scale classpaths → the pipeline drives partitioning + names the budget up front (G2.7 framing) |
| 5 | JNI | Wave 2 CLOSED: 162 real slots, attach lifecycle measured (EDETACHED/attach/detach), gate-22 C-harness parity battery | The loader shim dispatches entrypoints through the existing registry (T12 native-binding path) — no new FFI is expected (verify in wave 1, T29) |
| 6 | Mappings / mixin applier | DO NOT EXIST — T14–T23 all pending (M5_PLAN §6) | Ordered prerequisite: M5 waves 3–5 land first (§0 ordering law) |
| 7 | fabric.mod.json | Zero parsing anywhere (repo-wide grep: only the STRATEGIC_GAPS mention) | v1 parser + the loud-unsupported matrix (T27) |
| 8 | Reflection / resources | R2 static subset registered (STRATEGIC_GAPS §R2: metadata table, loud `J2RUnsupportedError`) | **To verify at wave 0**: how generated code resolves `getResource` for mod assets today; expected v1 = route to the mod jar's contents on disk (T31) |
| 9 | Evidence machinery | diff_suite (109 fixtures, sharded gate), cfg_equiv, three-channel law, gate-22 C-harness parity, gate-23 JMM boundary, proc_peak_sampler standing protocol | A mod-scale parity harness REUSES these; the new CI gate is numbered gate-24 (§4) |

---

## §2 Design — wave 1: the mod pipeline (`j2r build mod.jar`)

The existing `j2r build` verb gains mod semantics: when the input jar (or
classpath entry) contains `fabric.mod.json`, build engages mod mode. Plain
(non-mod) input keeps today's behavior EXACTLY — pinned by a behavioral
test, the same law the manifest fast path and the JMM markers followed.

**fabric.mod.json v1 schema subset.** `id`, `version`, `name`, `schemaVersion`
(must be 1; anything else loud); `entrypoints` (keys `main`, `client`,
`preLaunch` parsed; `main` consumed v1; unknown keys parsed and recorded,
with a census-driven promotion path — T24 decides what the corpus needs);
`mixins` (config names → resolve each config JSON: `package`, `mixins[]`,
`client[]`, `server[]`); `depends`/`breaks` (parsed and RECORDED against a
compatibility matrix; v1 does not version-solve, it reports and proceeds
unless the mod is in the recorded hard-fail list); `jars` (jar-in-jar →
loud-unsupported named error, matrix row). Every consumed key gets a unit
fixture; every non-consumed key gets a loud-unsupported row so silence is
impossible.

**Pipeline stages** (each a separate stage — R1 design answer 1, "move
transformation earlier, not later"):

1. metadata parse (fabric.mod.json + mixin configs),
2. mixin-set resolution — closed world check: every target class named by
   every mixin config must be inside the compilation set (mod classes +
   the named Minecraft/M5 runtime set), else loud named error,
3. [T15 remap stage if the mod is intermediary-named] → [T17–T20 applier] —
   both M5_PLAN-owned, listed here to show the ORDER, not re-specified,
4. j2r compile (partitioned when the class count warrants; budget named up
   front per G2.7),
5. cargo build + link with the loader shim crate (§3).

**Stage-level caching.** Each stage records its inputs' SHA-256s in the
build manifest; a dev-loop rebuild runs only the stages whose inputs
changed. The expectation is the G2.2 tier at mod scale (no-op re-runs in
seconds, not minutes) and it is MEASURED, not assumed (T28 evidence).

---

## §3 Design — wave 2: the loader surface as natives

**The shim crate** (generated alongside the app crate, like T05's support
crate) owns everything the mod expects from the launcher:

* The fabric.mod.json metadata is emitted into the binary at build time
  (consistent with R2's compile-time metadata stance — nothing is read
  from disk that was not in the compilation set).
* The `net.fabricmc.loader.api.*` v1 SUBSET is generated as ordinary Java
  classes in the compilation set, backed by native registry slots (the
  T12 path): `FabricLoader.getInstance()`, `getEntrypointContainers()`,
  `getModContainer(id)`, `isDevelopmentEnvironment()`,
  `getEnvironmentType()`. Each method = one registry slot + one
  differential fixture (Temurin side: the same shim classes backed by a
  tiny test launcher that answers from the real launcher's values).
* Entrypoint dispatch: launch constructs the mod instance and calls
  entrypoints in fabric.mod.json order through the ordinary dispatch
  machinery — the mod's `onInitialize` is just a method on a compiled
  class; no new invocation machinery is required.

**The v1 subset is census-driven, not guessed.** Wave 0's census (T24)
records exactly which loader calls the target mods make; every call
outside the implemented subset fails loud with a named error naming the
missing API, and the loud-unsupported matrix tracks it. The subset grows
only when the census says a real mod needs it — the same evidence-first
rule that grew the JNI surface (T08–T13 each landed against a measured
contract, the C probe first).

**Resources.** Mods read their own assets. Expected v1: `getResource` on
the mod's classes resolves against the mod jar's contents on disk (the
classpath entry), giving three-channel parity on a fixture that reads its
own asset (T31). The wave-0 audit (§1 row 8) verifies the current
generated-code resource path before this design freezes.

---

## §4 Design — wave 3: mixins at mod scale + the dev loop

**The boot bar** (ROADMAP verbatim, made concrete): a real Fabric mod from
the wave-0 corpus — one carrying at least three mixins across
`@Inject`/`@Redirect`/`@Accessor` (the T21/R1 bar, reused verbatim) —
compiles end-to-end through the §2 pipeline, boots in the j2r runtime,
reaches `onInitialize`, and observable behavior (stdout, stderr, exit
code, plus the mod's own visible side effects the census records) matches
the same mod under Temurin + the real Fabric launcher, three channels.

**`j2r test mod` semantics.** The existing differential test verb gains
mod mode: apply mixins → compile → run under j2r vs the same mod under
Temurin + Fabric launcher; the harness reuses diff_suite machinery
(including the J2R_DIFF_SHARD stride sharding for CI) and the three-channel
law unchanged. A masking bug in this harness would poison every mod claim,
so the harness itself gets a positive control first (a deliberately broken
mixin application must be caught — the gate-23 tamper-test pattern).

**New CI gate: gate-24-mod-boot.** Runs the smallest corpus mod end-to-end
(metadata parse → applier → codegen → cargo → boot → parity) inside the
same 25-minute leg budget gate-23 uses, `needs:`-wired into the all-gates
aggregate, docs-only PRs skip it (the changes filter, same as every leg).
Footprint claims use the proc_peak_sampler protocol (G2.1b standing rule);
compile-time claims carry the staged-logs protocol (M5 wave 1's rule).

---

## §5 Design — wave 4: Forge/NeoForge scoping (design note only, v1)

The recorded boundary, stated honestly: FML's `SecureJarHandler` layered
module classloading, Forge's own remapping stack and the event-bus model
are a different beast from Fabric's flat loader, and NeoForge forks the
same surface. v1 ships Fabric only. Wave 4 produces ONE artifact: the FML
boundary design note — what a Forge boot would need (transform-pipeline
shape, mods.toml parsing, event-bus mapping onto dispatch, the remap
interplay with T15) — and a GO/NO-GO recorded like every lever step this
project has ever taken. No implementation follows without a re-scoped plan
revision the owner reviews; a measured "the v2 surface is bigger than the
v1 runtime assumption" is a valid, registrable outcome.

---

## §6 Task register

Status: `[ ]` pending · `[~]` started (with the gap stated) · `[x]` done
(with evidence).

### Wave 0 — census & audit (measure first)

- [ ] **T24** Ecosystem census: fabric.mod.json field/key frequency,
      entrypoint kinds, mixin annotation families, loader-API call
      frequency, across ≥ 20 real Fabric mods; committed table → §2/§3.
- [ ] **T25** Seam audit finalization: verify §1 rows 5 and 8 against
      source (JNI-shim dispatch path; resource access today); promote
      §1 to "verified" or correct it → §1.
- [ ] **T26** Mod corpus + licensing: which census mods can be committed
      vs re-fetched by CI; vendoring law recorded; the smallest mod named
      as the gate-24 target → §4.

### Wave 1 — the mod pipeline

- [ ] **T27** fabric.mod.json v1 parser + loud-unsupported matrix + unit
      fixtures (consumed keys, non-consumed keys, jar-in-jar loud error) → §2.
- [ ] **T28** `j2r build mod.jar`: mod-mode engagement (plain-jar behavior
      pinned unchanged by a behavioral test), stage pipeline, stage-level
      manifest caching with MEASURED dev-loop numbers → §2.
- [ ] **T29** Loader shim crate: metadata emission, entrypoint dispatch
      through the T12 registry path (verified: no new FFI), launch order → §3.

### Wave 2 — the loader surface

- [ ] **T30** FabricLoader v1 subset (census-driven): each API method a
      registry slot + differential fixture vs a test launcher on Temurin → §3.
- [ ] **T31** Resource access parity: mod reads its own asset, three-channel → §3.
- [ ] **T32** Boot without mixins: a real corpus mod reaches `onInitialize`
      under j2r, three-channel identical vs Temurin + Fabric launcher → §4.

### Wave 3 — mixins at mod scale + the dev loop

- [ ] **T33** THE BOOT BAR: a real mod with ≥ 3 mixins across the three
      families boots and matches three-channel (T21/R1 bar at mod scale) → §4.
- [ ] **T34** `j2r test mod` verb + harness positive control + CI
      gate-24-mod-boot wired into the aggregate → §4.

### Wave 4 — Forge/NeoForge scoping

- [ ] **T35** FML boundary design note + GO/NO-GO recorded; no
      implementation without a re-scoped plan rev → §5.

---

## §7 Standing evidence rules (inherited, unchanged)

Full gate battery green before push; differential claims are three-channel
(stdout/stderr/exit) vs real Temurin 21; perf and footprint claims carry
the measurement protocol (median-of-3, staged logs, proc_peak_sampler per
process); a census or design note is committed evidence, not a private
sketch; protocol scripts are COMMITTED before their first run (lesson
#12); raw evidence lands under `scripts/evidence/` in the same commit as
the claim (lesson #11); test counts stay gate-11-synchronized; a measured
NO-GO is a valid registered outcome — the honesty rule that has governed
every wave since M2.
