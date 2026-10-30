# J2R

**A Java-to-Rust compiler, written in Rust.** J2R translates JVM bytecode
into native Rust programs with Java-exact semantics — the long-term goal is
compiling real Java applications, including Minecraft: Java Edition and its
mods, to native code.

[![CI](https://github.com/WasewaseX/j2r/actions/workflows/ci.yml/badge.svg)](https://github.com/WasewaseX/j2r/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Status: Milestone 1 — a working, tested compiler

Milestone 1 compiles JVM bytecode to a standalone Rust crate and runs it
natively. Every feature is validated by **differential testing**: fixtures
are compiled with `javac`, run on the JVM, compiled with J2R, run natively,
and stdout + exit codes must match byte-for-byte. 14 fixture programs cover
arithmetic, control flow, strings, arrays, inheritance, exceptions
(`finally`, multi-catch, rethrow), boxing, recursion, statics, interfaces
and long edge cases — all green, plus 56 unit tests and a JDK-free
end-to-end test.

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
`multianewarray`, `monitorenter`/`exit`, `ldc` of class constants, native
methods, most of the JDK beyond the natives list, and true threads.

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
* **`j2r-codegen`** — bytecode → typed IR → Rust source. Validates every
  symbolic reference (classes, methods, fields, bootstrap methods) against
  the combined universe of compiled classes + runtime natives *at compile
  time*. Computes flattened field-slot layouts across whole class
  hierarchies, assigns static-pool slots, synthesizes `<clinit>` bodies for
  `ConstantValue` fields.
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
$ cargo test                       # 56 unit tests + JDK-free e2e + differential suite
$ ./target/debug/j2r test crates/j2r-cli/tests/suite/Arrays.java
```

The differential suite (`crates/j2r-cli/tests/suite/*.java`) skips itself
gracefully when no JDK is present; CI installs Temurin 21.

## License

MIT — see [LICENSE](LICENSE).
