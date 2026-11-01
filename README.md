# J2R

**A Java-to-Rust compiler, written in Rust.** J2R translates JVM bytecode
into native Rust programs with Java-exact semantics — the long-term goal is
compiling real Java applications, including Minecraft: Java Edition and its
mods, to native code.

[![CI](https://github.com/WasewaseX/j2r/actions/workflows/ci.yml/badge.svg)](https://github.com/WasewaseX/j2r/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Status: Milestone 1 + hardening — a working, tested compiler

Milestone 1 compiles JVM bytecode to a standalone Rust crate and runs it
natively. Every feature is validated by **differential testing**: fixtures
are compiled with `javac`, run on the JVM, compiled with J2R, run natively,
and **stdout, stderr and exit codes** must match (stderr with JVM
stack-trace frames — and nothing else — normalized away). 36 fixture
programs cover arithmetic and integer/long edge cases, shifts, casts and
saturating conversions, control flow (switch fallthrough, labeled
break/continue), strings, string building, one- and two-dimensional arrays,
inheritance, constructor/initializer ordering, static-initialization order,
exceptions (`finally` ordering, multi-catch, rethrow, uncaught), boxing
identity, `instanceof` dispatch, recursion and mutual recursion, catchable
`StackOverflowError`, `System.err`,
and runtime exceptions with exact OpenJDK messages — all green, plus 117
unit tests, 12 malformed-classfile tests and a JDK-free end-to-end test.

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
* Java-exact `Double.toString`/`Float.toString` including the subnormal
  digit-count quirk (`4.9E-324`, not `5e-324`) — verified against OpenJDK 21

### What is intentionally not supported (yet)

Unsupported features are **compile errors with a precise message** — never
silent miscompiles. Currently: `jsr`/`ret` (pre-Java-6 bytecode),
`monitorenter`/`exit`, `synchronized` methods, `ldc` of class constants,
native methods, most of the JDK beyond the natives list, and true threads.
(`multianewarray` was implemented in M1.6 — see the feature list above.)

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
$ cargo test                       # 117 unit tests + malformed/e2e/SOE-e2e + 36-fixture differential suite
$ ./target/debug/j2r test crates/j2r-cli/tests/suite/Arrays.java
```

The differential suite (`crates/j2r-cli/tests/suite/*.java`, 36 programs)
skips itself gracefully when no JDK is present; CI installs Temurin 21.
Runtime guarantees and the panic-path inventory live in
[docs/RUNTIME_SAFETY.md](docs/RUNTIME_SAFETY.md).

## License

MIT — see [LICENSE](LICENSE).
