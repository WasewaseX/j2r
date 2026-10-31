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

## M2 — Quality of the generated code

* Lift the pc-dispatch loops into structured Rust control flow
  (`if`/`loop`/`match`) via condition-based CFG reconstruction
* Specialize operand stacks and locals per method (drop the `Value` enum on
  the stack; unboxed arithmetic)
* Replace `Rc`-based heap with a real tracing GC (cycle collecting), then
  enable true Java threads on top of it
* `StackOverflowError` instead of process abort on deep recursion

## M3 — JDK coverage

* Grow the natives layer into a JDK-subset implementation: collections
  (`ArrayList`, `HashMap`, ...), `java.util.function`, lambdas/method
  references (already compile to `invokedynamic` — need
  `LambdaMetafactory` support), streams
* `ldc` of class constants, `multianewarray`, `synchronized`
  (monitor enter/exit over the M2 runtime), reflection subset
* Parsing class files straight from JARs + a classpath option
* `assert` support, stack traces with `LineNumberTable`

## M4 — Minecraft readiness

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

## M5 — Mod ecosystem

* Fabric Loader / Forge / NeoForge API surface as natives + compiled mods
* `j2r build mod.jar` pipeline: translate the mod JAR + its dependencies,
  link against compiled Minecraft, emit a native loader
* Dev-loop tooling: `j2r test mod` differential runs against a real
  Minecraft + Fabric dev environment
