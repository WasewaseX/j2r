# J2R Roadmap

Milestone 1 is complete and tested. The ordering principle: every milestone
must leave the tree in a state where `cargo test` — including differential
testing against a real JVM — passes, and unsupported features fail loudly
at compile time.

## M1 — Core compiler (done)

* `.class` parser, IR decoder, Rust emitter, Java-semantics runtime
* Core instruction set, classes/inheritance/interfaces, exceptions,
  strings + `invokedynamic` concat, arrays, statics, natives layer
* Differential test harness (`j2r test`) and 14-fixture suite

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
