# J2R

**A Java-to-Rust compiler, written in Rust.** J2R translates JVM bytecode
into native Rust programs with Java-exact semantics — the long-term goal is
compiling real Java applications, including Minecraft: Java Edition and its
mods, to native code.

[![CI](https://github.com/WasewaseX/j2r/actions/workflows/ci.yml/badge.svg)](https://github.com/WasewaseX/j2r/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Status: Milestone 3 (JDK coverage) — five of five waves done

Every feature is validated by **differential testing**: fixtures
are compiled with `javac`, run on the JVM (Temurin 21), compiled with J2R, run natively,
and **stdout, stderr and exit codes** must match (stderr with JVM
stack-trace frames — and nothing else — normalized away).
**109 fixture programs** cover arithmetic and integer/long edge cases,
shifts, casts and
saturating conversions, control flow (switch fallthrough, labeled
break/continue), strings, string building, one- and two-dimensional arrays,
inheritance, constructor/initializer ordering, static-initialization order,
exceptions (`finally` ordering, multi-catch, rethrow, uncaught), boxing
identity, `instanceof` dispatch, recursion and mutual recursion, catchable
`StackOverflowError`, cyclic graphs, aliasing across collections, interning
across GC churn, `System.err`,
`java.util` collections with **byte-identical `HashMap` iteration order**,
classpath/JAR loading, lambdas and method references, `synchronized`
monitors, and a reflection subset (`X.class`, `Field.get/set`,
`Method.invoke`) — all green, plus 228 unit and integration tests,
12 malformed-classfile tests and JDK-free end-to-end tests.

A dedicated **hardening milestone** (post-audit) added:

* **Bytecode verification** before codegen — stack shape, category-1/2
  correctness, control-flow joins, branch targets, local-variable bounds
  and array element kinds. Malformed class files now fail with a structured
  `verification error ... at pc N` diagnostic instead of reaching
  panic-prone runtime paths (36 verifier unit tests + 12 malformed-classfile
  e2e tests).
* **Wider internal identifiers** — class ids and static-pool slots are
  `u32`, tested with a synthetic 80,000-slot program (past the old 65,535
  `u16` boundary).
* **Java default values for statics** without `<clinit>` (`0`, `false`,
  `null`) — a real semantic bug the wide-identifier test uncovered, now
  fixed and differentially tested (`ZeroInit` fixture).
* `synchronized` methods are compile errors instead of silent miscompiles.
* A precise statement of runtime guarantees and panic-path guards:
  [docs/RUNTIME_SAFETY.md](docs/RUNTIME_SAFETY.md).

A **coverage & robustness pass** (M1.6) added on top:

* **JLS §5.1.7 boxing identity** — `valueOf` returns the mandatory cached
  objects for `Integer`/`Long`/`Short`/`Byte`/`Character`/`Boolean` (a
  differential probe caught `Integer(127) == Integer(127)` being `false`),
  plus value-based `equals` and JDK-contract `hashCode` for all boxed
  classes (`Float.equals` compares canonicalized IEEE bits: NaN equals NaN,
  `+0.0` differs from `-0.0`).
* **`multianewarray`** — full multidimensional array allocation
  (`new int[r][c]`, jagged `new int[3][]`, negative dimension →
  `NegativeArraySizeException`), with verifier rules, canonical array
  descriptors at every level (`[[I`, not the broken `[L[I;`) and array-store
  subtype checks for nested element types. Rejected inputs are structured
  compile errors, never runtime panics.
* Differential suite grown from 18 to 34 fixtures; a shrink-guard asserts
  the count never regresses.
* **Wave 1 of M2 — catchable `StackOverflowError`**: deep recursion now
  throws a real, catchable `StackOverflowError` (never a process abort).
  Every generated method registers a `FrameGuard`; the depth limit
  defaults to 8192 and is overridable via `J2R_MAX_JAVA_FRAMES` up to a
  hard ceiling of 131,072 derived from a *measured* per-frame native-stack
  footprint (208–304 B measured; see `docs/M2_PLAN.md` §3.1.1). Invalid
  override values (`0`, over-ceiling, non-numeric) are rejected with exit
  2 before any Java code runs. Differential fixtures `StackDepth` (caught
  SOE, thread continues) and `UncaughtSOE` (uncaught, exit 1) pin the
  JVM-exact behaviour; the suite is now 36 fixtures.
* **Wave 2 of M2 — tracing, cycle-collecting arena GC**: the `Rc` heap is
  replaced by a non-moving mark-sweep arena with generation-tagged `Copy`
  handles (`JRef { idx, gen }`). Unreachable cyclic graphs are reclaimed
  (200 000-cycle stress test keeps the arena under 64 live slots with 10
  live objects); stale handles cannot silently alias a recycled object —
  every access checks the generation and panics loudly on mismatch.
  Collection runs only at explicit safepoints before allocating opcodes;
  roots come from per-frame snapshots, the statics pool and the runtime
  caches. **The whole 39-fixture differential suite runs byte-identical
  with a forced collection at every allocation** (CI gate `gc-stress`),
  and `Object.hashCode` is now stable across collections. Zero `unsafe` is
  mechanically enforced by `#![forbid(unsafe_code)]` in every crate root
  plus a CI grep gate. The re-issued borrow census shows zero direct object
  borrows outside the heap kernel (`docs/RUNTIME_SAFETY.md` §4).
* **Waves 3–4 of M2 — investigations with recorded decisions**: the
  typed-storage pass kept the verifier's per-pc shapes and measured the
  specialization opportunity (`j2r shape-report`: 89 % of fixture methods
  are kind-unstable per position, so the wave ended honestly at its
  designed checkpoint); the CFG prototype lifts 127/161 fixture methods
  into structured bodies behind `--emit-cfg` (default off) with
  **39/39 fixtures byte-identical in both emission modes** and a NO-GO
  recommendation for changing the default (`docs/M2_CFG_REPORT.md`).

A **JDK-coverage milestone** (M3, [docs/ROADMAP.md](ROADMAP.md)) landed on
 top, wave by wave:

* **Wave 1 — classpath & JAR loading**: zero-dependency zip/inflate/crc32,
  transitive class-path closure, `-p/--classpath`, JAR fixtures
  (suite 39 → 47).
* **Wave 2 — `java.util` collections + `java.lang` extensions**
  (details in *What works today* below; suite 47 → 51).
* **Wave 3 — lambdas & method references**: `LambdaMetafactory` bootstrap
  resolution, functional-interface natives, interface dispatch for native
  classes (suite 51 → 60).
* **Wave 4 — `synchronized`**: reentrant heap monitors,
  `monitorenter`/`monitorexit`, `ACC_SYNCHRONIZED` methods, structured
  release on exception unwind (suite 60 → 65).
* **Wave 5 — reflection subset**: interned, static-rooted `Class` objects
  whose identity survives GC (proven under 200k-node garbage churn +
  forced collection), `ldc` of class constants, `Field.get/set` on the
  real slots and static pool, `Method.invoke` through the compiled
  dispatch path, `InvocationTargetException` wrapping, and Temurin-exact
  failure strings everywhere (suite 65 → 70).

### What works today

* Full `.class` parsing (constant pool, methods, `Code`, `BootstrapMethods`,
  modified UTF-8, two-slot constants)
* The core JVM instruction set: constants, loads/stores, the complete
  arithmetic set (with Java wrapping, `/ by zero`, NaN and signed-zero
  semantics), conversions (Java's saturating float→int casts), comparisons,
  all branch forms, `tableswitch`/`lookupswitch`, all four `dup2_x2`-family
  stack forms per JVMS §6.5
* Classes, inheritance, virtual dispatch, `super` calls, interfaces,
  `instanceof`/`checkcast` with full hierarchy resolution across compiled +
  builtin classes
* Exceptions: real exception tables, custom exception classes, exact
  OpenJDK messages (`Index 5 out of bounds for length 3`, `/ by zero`)
* Strings with JVM identity interning, UTF-16 `length`/`charAt`/`hashCode`,
  and `invokedynamic makeConcatWithConstants` (how javac compiles `+`)
* Arrays (primitive + reference, covariance, `ArrayStoreException`)
* Statics with lazy `<clinit>` in superclass-first order, `ConstantValue`
  synthesis
* A natives layer: `System.out/err`, `Math`, `StringBuilder`, boxed types,
  `Integer.parseInt`, `System.exit`, …
* `java.util` collections (M3 Wave 2): `ArrayList`, `HashMap`, `HashSet`,
  their iterators and views, and `Objects` — **heap-resident** so the
  tracing GC scans them with zero new root machinery, with
  algorithm-faithful OpenJDK `HashMap` behaviour (hash spreading,
  `tableSizeFor`, Java-8 lo/hi resize split, tail insertion) pinned
  differentially to Temurin 21 byte-for-byte, `modCount`-based
  `ConcurrentModificationException` on iterator invalidation, and Java-exact
  bounds/overflow message texts (`Range [5, 2) out of bounds for length 12`,
  `integer overflow`, `For input string: "8" under radix 8`)
* `java.lang` extensions (M3 Wave 2): `String` (`substring`, `indexOf`,
  `trim` with Java's `<= U+0020` semantics, UTF-16 `compareTo`, `repeat`,
  …), `StringBuilder` (`reverse`, `insert`, `delete`, `replace`, …),
  `Math` (`floorDiv/floorMod/round/sqrt/pow/…Exact` with JDK exception
  messages), `Integer`/`Long` bit manipulation and unsigned formatting,
  `Character` classification
* Suppressed exceptions & try-with-resources (M3.5 audit response):
  `Throwable.addSuppressed`/`getSuppressed`/`getCause`, cause-chain
  constructors, uncaught-handler printing of `Suppressed:`/`Caused by:`
  sections in JVM print order — differentially pinned against Temurin 21
* Lambdas & method references (M3 Wave 3): `LambdaMetafactory` bootstraps,
  captured and captureless lambdas, method references, functional
  interfaces — differentially pinned against Temurin 21
* `synchronized` (M3 Wave 4): reentrant object/class monitors with
  JVM-faithful release on unwind
* Reflection subset (M3 Wave 5): `X.class`/`getClass()` interning,
  class metadata queries, `Field.get/set`, `Method.invoke` with
  Temurin-exact failure messages
* Java-exact `Double.toString`/`Float.toString` including the subnormal
  digit-count quirk (`4.9E-324`, not `5e-324`) — verified against OpenJDK 21
* `String.format` / `PrintStream.printf` (Formatter wave + residuals,
  corpus/FmtResidualProbe.java): %s %d %x %o %f %e %g %a with flags,
  width/precision and argument indexing, the whole uppercase-variant
  family (%S %B %H %C %X %E %G %A — `%E` of Infinity is `INFINITY`),
  subnormal hex-float forms, and the JDK's exact parse-time error
  contracts (`FormatFlagsConversionMismatchException`,
  `MissingFormatWidthException`, null → `null`/`NULL`) — differentially
  pinned to Temurin 21 byte-for-byte
* Catchable `StackOverflowError` and a tracing, cycle-collecting GC with
  generation-tagged handles (M2 Waves 1–2; see the milestone section above)
* Investigation tooling: `j2r shape-report` (typed-storage decisions) and
  `j2r cfg-report` / `--emit-cfg` (CFG lift prototype, default off —
  109/109 fixtures output-identical in both modes)
* Classpath & JAR loading (M3 Wave 1): `-p/--classpath` with directories
  and `.jar`/`.zip` archives, transitive closure loading from a
  `--main-class`, a zero-dependency zip/inflate/CRC-32 reader with a
  real-JDK test-vector battery, and a 4-fixture jar differential suite
  (first-match-wins precedence, determinism vs exploded dirs, loud
  missing-class errors)

### What is intentionally not supported (yet)

Unsupported features are **compile errors with a precise message** — never
silent miscompiles. Currently: `jsr`/`ret` (pre-Java-6 bytecode), native
methods, and most of the JDK beyond the natives/collections/reflection
list — the JMM's volatile/final-freeze semantics (threads, monitors,
wait/notify, interrupt and daemon shutdown ARE supported — M4; see
RUNTIME_SAFETY.md §6 for what the concurrency model does and does not
guarantee), the stream classes beyond the byte-stream builtins the corpus
entries pulled in (ByteArray/Buffered/Pushback/Filter streams,
StringReader/Writer), `Optional`, `java.time`,
regex, serialization, `assert`.
(`multianewarray`, `monitorenter`/`monitorexit`, `synchronized` methods
and `ldc` of class constants were implemented across M1.6 and M3
Waves 4–5 — see the feature list above.)

See [ROADMAP.md](ROADMAP.md) for the plan to close these gaps and reach
Minecraft.

## Quick start

```console
$ git clone https://github.com/WasewaseX/j2r && cd j2r
$ cargo build --release

# compile a .class file to a Rust crate
$ javac examples/Hello.java
$ ./target/release/j2r compile Hello.class -o out
generated Rust crate: out
build it with: cargo build --manifest-path out/Cargo.toml

# or in one step: translate + build a native binary
$ ./target/release/j2r build Hello.class -o out --release
native binary: out/target/release/j2r_app

# differential test against the JVM (needs a JDK)
$ ./target/release/j2r test examples/Hello.java
PASS examples/Hello.java
```

The generated crate is self-contained: it vendors the `j2r-runtime` crate
and builds with plain `cargo`, no JVM required at run time.

## Architecture

```
┌──────────────┐   ┌───────────────┐   ┌──────────────┐   ┌─────────────┐
│  .class files │→│  j2r-classfile │→│  j2r-codegen  │→│ Rust crate  │
│  (javac/JAR)  │   │  parser        │   │  IR + emitter │   │ + j2r-      │
└──────────────┘   └───────────────┘   └──────────────┘   │   runtime   │
                                                          └─────────────┘
```

* **`j2r-classfile`** — dependency-free `.class` parser with byte-offset
  error reporting.
* **`j2r-codegen`** — bytecode → typed IR → Rust source. Runs a **bytecode
  verifier** (stack heights, category-1/2 correctness, control-flow joins,
  branch targets, local indices, array element kinds) before emitting any
  code, so malformed class files are structured compile errors, never
  runtime panics. Validates every symbolic reference (classes, methods,
  fields, bootstrap methods) against the combined universe of compiled
  classes + runtime natives *at compile time*. Computes flattened
  field-slot layouts across whole class hierarchies, assigns static-pool
  slots (`u32`), synthesizes `<clinit>` bodies for `ConstantValue` fields.
* **`j2r-runtime`** — the Java-semantics runtime generated code links
  against: the `Value` model, the heap, arrays, strings/interning,
  Java-exact number formatting, the natives table, builtin JDK hierarchy
  knowledge, and the `j2r_try!` exception macro.
* **`j2r` (CLI)** — `compile` / `build` / `test` (differential testing
  against `javac` + `java`).

### Code generation strategy (milestone 1)

Each Java method becomes a Rust function with an explicit operand stack and
a `pc`-dispatch loop over basic blocks — a *typed* threaded interpreter
compiled natively. This is correct for arbitrary control flow and lets the
first milestone be small and provable; a later pass will lift blocks into
structured Rust (`if`/`loop`/`match`) and specialize the operand stack per
method. Exceptions use the method's real exception table through the
`j2r_try!` macro; dispatch uses per-crate method/hierarchy registries plus
the runtime natives table.

## Testing

```console
$ cargo test                       # 338 tests: unit + differential/forced-GC/SOE-e2e/cfg-equivalence/jar suites
$ J2R_GC_EVERY_N=1 cargo test -p j2r-cli --test diff_suite   # rooting proof: collection at every allocation
$ ./target/debug/j2r test crates/j2r-cli/tests/suite/Arrays.java
$ ./target/debug/j2r compile -p app.jar --main-class com.example.Main -o out   # compile straight from a jar
```

The differential suite (`crates/j2r-cli/tests/suite/*.java`, 109 programs)
skips itself gracefully when no JDK is present; CI installs Temurin 21.
Runtime guarantees and the panic-path inventory live in
[docs/RUNTIME_SAFETY.md](docs/RUNTIME_SAFETY.md).

## CI gates

Every push to `main` runs the full evidence chain — every job, every run
(prose-only pull requests skip the five heavy compile-and-run legs; a
`main` push never does). The gate inventory:

| # | Gate | What it proves |
|---|------|----------------|
| 1 | fmt | rustfmt, zero tolerance |
| 2 | clippy | all targets/features, warnings are errors |
| 3 | test | unit + e2e + differential suite vs a real JVM |
| 4 | build-release | clean `--release --locked` build |
| 5 | standalone | generated crate builds `--offline`, runs byte-identical |
| 6 | doc | rustdoc builds with `-D warnings` |
| 7 | security-audit | cargo-audit: any RustSec advisory fails |
| 8 | all-gates | the aggregate: 16 upstream gates green |
| 9 | gc-stress | differential at a collection per allocation (6 duration-balanced legs) |
| 10 | no-unsafe | zero `unsafe` — compiler-enforced forbid + grep evidence |
| 11 | doc-counts | README counts match measured reality (machine-checked) |
| 12 | runtime-seams | every `thread_local!` seam annotated |
| 13 | concurrency | thread fixtures × 4 deterministic schedules + stress |
| 14 | CodeQL | security-extended + quality suites (own workflow) |
| 15 | Scorecard | OSSF supply-chain posture (own workflow) |
| 16 | OSV | dependency vulnerability scan (own workflow) |
| 17 | supply-chain | cargo-deny licenses/bans/sources + gitleaks secret scan |
| 18 | actions-security | zizmor workflow audit + actionlint over all workflows |
| 19 | doc-links | every local markdown link and anchor resolves |
| 20 | code-integrity | no stubs, elision markers or FIXME tokens in `crates/` |
| 21 | dependency-hygiene | unused-dependency report (advisory by design) |

Supply-chain posture: every third-party action pinned by commit SHA,
workflow-level `permissions: contents: read` only, `persist-credentials:
false` on every checkout, per-job timeouts, `--locked` on all cargo
invocations, toolchain pinned to the exact local-evidence versions
(rust 1.99.1, Temurin 21.0.13).

## License

MIT — see [LICENSE](LICENSE).
