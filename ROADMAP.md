# J2R Roadmap

Milestone 1 and the post-audit hardening milestone are complete and tested.
The ordering principle: every milestone must leave the tree in a state
where `cargo test` — including differential testing against a real JVM —
passes, and unsupported features fail loudly at compile time.

## M1 — Core compiler (done)

* `.class` parser, IR decoder, Rust emitter, Java-semantics runtime
* Core instruction set, classes/inheritance/interfaces, exceptions,
  strings + `invokedynamic` concat, arrays, statics, natives layer
* Differential test harness (`j2r test`) and 14-fixture suite

## M1.5 — Engineering hardening (done, post external audit)

Findings and fixes, each with regression tests:

* **Differential harness now compares stdout, stderr *and* exit status**
  (stderr with JVM frame lines normalized — types, messages, `Caused by:`
  sections and program output are compared exactly). New fixtures:
  `ErrStream` (System.err), `Uncaught` (uncaught exception, exit code),
  `Runtimes` (arithmetic/OOB/negative-array/NPE/CCE + uncaught AIOOBE),
  `ZeroInit` (defaults of statics without `<clinit>` — a real bug found
  and fixed by this milestone).
* **Bytecode/IR verifier** (`j2r-codegen/src/verify.rs`): stack-height
  consistency at joins, category-1/2 correctness (including the exact
  `dup2_x2` forms), branch/switch targets as instruction boundaries,
  local-variable indices and category-2 half-slots, return/descriptor
  agreement, `max_stack`/`max_locals`, exception-table structure and
  catch-type resolution. Malformed bytecode is a structured
  `CodegenError::Verify` with the faulting pc — never a runtime panic.
  36 unit tests + 12 malformed-classfile e2e tests.
* **Internal ids widened to `u32`** (class ids, static-pool slots, generated
  registries); constant-pool indexes stay `u16` (a JVM-spec limit, not
  ours). Regression test compiles a synthetic two-class, 80,000-slot
  program and runs it.
* **`Rc<RefCell>` and panic-path audit** documented with precise claims in
  [docs/RUNTIME_SAFETY.md](docs/RUNTIME_SAFETY.md).
* `synchronized` methods (would previously compile without locking) are
  now a loud compile error.

## M2 — Quality of the generated code (done)

The full implementation plan — architecture audit, `Rc` risk register,
tracing-GC design, `StackOverflowError` design, typed-stack
specialization, CFG-reconstruction investigation and the ordered wave
breakdown with acceptance criteria — is
[docs/M2_PLAN.md](docs/M2_PLAN.md). All four waves shipped with
per-wave evidence in [docs/M2_EVIDENCE.md](docs/M2_EVIDENCE.md):

* **Wave 1** — catchable `StackOverflowError`: frame registry, hard
  ceiling from measured frame footprints, `J2R_MAX_JAVA_FRAMES`
  boundary matrix, suite 34 → 36.
* **Wave 2** — tracing, cycle-collecting arena GC with
  generation-tagged copy handles: zero direct heap borrows (census
  re-issued), stable identity hashes, forced-GC rooting proof
  (39/39 fixtures byte-identical with a collection at every
  allocation), suite 36 → 39.
* **Wave 3** — typed-storage specialization ended at checkpoint 3a
  with the measured honest NO-GO (89 % fallback; see
  [docs/M2_SHAPE_REPORT.md](docs/M2_SHAPE_REPORT.md)).
* **Wave 4** — CFG reconstruction prototype behind `--emit-cfg` with
  the automated 39/39 output-equivalence gate; NO-GO for default
  emission, re-evaluate with benchmarks in M3 (see
  [docs/M2_CFG_REPORT.md](docs/M2_CFG_REPORT.md)).

CI: 10 gates green at the completion commit (run 41346367440),
including `gc-stress` (forced collection) and `no-unsafe`. 170 tests,
0 failures locally.

## M3 — JDK coverage

The implementation plan with the full 23-task register and per-task
acceptance criteria is [docs/M3_PLAN.md](docs/M3_PLAN.md); evidence
lands in [docs/M3_EVIDENCE.md](docs/M3_EVIDENCE.md).

* **Wave 0 (T01–T07, done)** — plan, risk register, architecture.
* **Wave 1 (T08–T11, done)** — classpath & JAR loading: zero-dep
  zip/inflate/crc32, transitive closure, `-p/--classpath`, jar
  differential fixtures; suite 39 → 47.
* **Wave 2 (T12–T15, done)** — `java.util` collections
  (ArrayList/HashMap/HashSet, OpenJDK-faithful hash spreading and
  iteration order pinned by the `HashMapOrder` fixture) + `String`/
  `StringBuilder`/`Math`/boxing extensions; suite 47 → 51.
* **Wave 3 (T16–T18, done)** — lambdas & method references
  (`LambdaMetafactory` bootstrap resolution, functional-interface
  natives, interface-dispatch wiring for native classes); suite 51 → 60.
* **Wave 4 (T19–T20, done)** — `synchronized` monitors (heap monitor
  counters, `monitorenter/monitorexit`, `ACC_SYNCHRONIZED` methods);
  suite 60 → 65.
* **Wave 5 (T21–T22, done)** — reflection subset (interned, static-rooted
  `Class` objects, `ldc` of class constants, `Field.get/set`,
  `Method.invoke`, Temurin-exact failure strings); suite 65 → 70.
* **Real-program tier (post-review policy, done)** — five 300–500 line
  multi-class programs (JSON parser, Huffman codec, RPG battle sim with
  `Comparator` lambdas, mini-language interpreter, hand-rolled hash
  table + LRU + reflection) join the suite; suite 70 → 75. Found and
  fixed the JLS §4.12.5 instance-default bug the 70 micro-fixtures had
  never probed (M3_EVIDENCE.md addendum). Standing policy: every future
  wave adds real-program fixtures alongside the micro-probes.
* **Real-program JDK surface (post-review wave, done)** — `java.util.Arrays`
  (toString/fill/copyOf/copyOfRange/equals/sort with stable Object sorts),
  `java.util.Collections` (sort/reverse/swap/addAll), `ArrayList.subList`
  live views (modCount discipline, JDK-exact messages), compareTo bridge
  rows, `Character.compare`/`Double.compare`; verifier checkcast now
  refines array descriptors. Suite 75 → 77 (ArraysSurface +
  BigGradebook). M3_EVIDENCE.md addendum 2.
* **T23 (done)** — completion package + review submission: 10/10 gates
  green at `ce8b44e` (run 41484339575); suite 39 → 70; 228 tests;
  deviations disclosed in M3_EVIDENCE.md.

Explicitly out of M3 scope (loud errors stay; deferred with rationale
in M3_PLAN §0): threads and concurrency (single-threaded boundary
carried from M2), streams/`Optional`/`java.time`, regex, serialization,
`assert`, and `LineNumberTable` stack traces — the latter two are
candidates for the follow-up quality milestone alongside the M4
threads work.

## M3.5 — audit response (post external review)

An external review of a partial source dump produced seven findings.
Every claim was re-verified against the actual source before being
accepted — two were directionally right but overweighted, none were pure
hallucination, and two hid more than the reviewer saw. Verdicts,
line-level evidence and severity re-ratings:
[docs/AUDIT_VERIFICATION.md](docs/AUDIT_VERIFICATION.md). Register:

* **A1 (P0)** — `compare.rs`: stop stripping `Suppressed:` header lines
  and bare `...` output; normalize only tab-prefixed frame lines. The
  differential harness must not be able to mask a semantic divergence.
* **A2 (P0)** — runtime suppressed-exception machinery:
  `Throwable.addSuppressed`/`getSuppressed`, uncaught-handler printing,
  try-with-resources differential fixture with a throwing `close()`.
* **A3 (P1)** — one main-class selection shared by CLI and codegen:
  source-stem preference, single-candidate rule, loud error on
  ambiguity; no more `read_dir`-order luck.
* **A4 (P2)** — `j2r-classfile` attribute-length validation:
  `ConstantValue` length must be 2, `0 < code_length < 65536`.
* **A5 (P2)** — `inflate.rs` zlib-exact table validation (incomplete
  codes rejected unless empty or a single 1-bit code); fixed distance
  table becomes the complete RFC 32×5-bit form.
* **A6 (P2)** — `crc32.rs`: table built once (`LazyLock`).
* **A7 (P2)** — Windows portability: separator-safe `PATH`/classpath
  handling, `/usr/lib/jvm` scan behind `#[cfg(unix)]`.
* **A8 (P2)** — README counts corrected and machine-checked:
  `scripts/check_doc_counts.py` wired into CI; hand-edited prose counts
  banned. Standing policy from this audit: test-count updates stay
  synchronized with the documentation, mechanically.

Status (2026-11-03): **A1 done** — masking rule deleted from
`normalize_stderr`, five honesty tests pin the new semantics, and the
first differential run *after* the fix is green: 78/78 fixtures,
stdout/stderr/exit all matching Temurin 21.0.13+9, 232 workspace tests
passing. A2–A8 in progress; a fourth internal "finding" (alleged
`#acro_export]` syntax corruption at `j2r-runtime/src/lib.rs:421`) was
**disproven** by compiler run and byte-hash evidence — see the
verification log in AUDIT_VERIFICATION.md for why tool output alone is
not evidence in this repo.

## M4 — threads & concurrency (draft plan)

The threads milestone M2_PLAN §7 promised: shared heap/statics/identity
caches, real `Thread`, STW safepoints, monitor contention,
`wait`/`notify`, interrupt, a cooperative-scheduler determinism strategy
for differential testing, and a concurrency CI gate. Draft with an
18-task register (all unchecked) at
[docs/M4_PLAN.md](docs/M4_PLAN.md) — rev 0, pending review; no task
starts before the plan is accepted. On acceptance, the milestone labels
renumber: Minecraft readiness → M5, mod ecosystem → M6 — the renumbering
is applied below in this same change so the register stays internally
consistent.

## M5 — Minecraft readiness (was M4)

Minecraft is a large, ordinary Java program — the risks are its bootstrap
(natives), class rewriting, and sheer size:

* Incremental/module compilation for 70k+ class codebases
* Java Native Interface (JNI) bridge for LWJGL/GLFW/OpenAL — the native
  side stays native, J2R code calls it through generated shims
* A `TransformingClassLoader`-equivalent: apply Fabric Mixins at the
  *bytecode* level (before j2r), so mixin semantics are unchanged
* mappings support (Mojang official + intermediary) at the class-file level
* String-switch-heavy hot paths and megamorphic call sites: inline caches
  in the dispatch registry

## M6 — Mod ecosystem (was M5)

* Fabric Loader / Forge / NeoForge API surface as natives + compiled mods
* `j2r build mod.jar` pipeline: translate the mod JAR + its dependencies,
  link against compiled Minecraft, emit a native loader
* Dev-loop tooling: `j2r test mod` differential runs against a real
  Minecraft + Fabric dev environment
