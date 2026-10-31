# Hardening milestone (M1.5) — evidence record

The external audit ("SUBSTANTIAL IMPROVEMENT / STILL INCOMPLETE") approved
M1 for a limited prototype scope and required a hardening milestone of four
fixes. This document records, for each fix: the original bug with a
reproduction, the exact source changes, the new automated regression tests,
the JVM vs native results, and the CI result.

Hardening commit: `2c9ae0e` ("M1.5 engineering hardening: verifier, stderr
diffs, u32 ids, safety docs"), parent `4e6b3a4` (audited M1 baseline).

Architecture is unchanged: same four-crate workspace, same PC-dispatch
code-generation strategy, no tracing GC, no threading, no JNI, no
Minecraft support.

## H1 — Differential harness ignored stderr

**Original bug.** `crates/j2r-cli/src/main.rs` (baseline `4e6b3a4`) captured
`output.stdout` and the exit code and **discarded `output.stderr`** — the
doc comment even said "comparing stdout and exit code". Any stderr
difference (wrong exception type, wrong message, missing exception
entirely) shipped undetected.

**Reproduction (executed, not hypothetical).** `Uncaught.java` throws
`IllegalStateException("uncaught-boom-42")` out of `main`. The runtime's
`throwable_display` was corrupted in a scratch copy to print
`java.lang.WrongException: injected-false-negative` instead; a native
binary was built from the corrupted runtime and its stderr compared with
the JVM's:

```
JVM    (stderr, normalized): Exception in thread "main" java.lang.IllegalStateException: uncaught-boom-42   exit=1
native (stderr, corrupted) : Exception in thread "main" java.lang.WrongException: injected-false-negative   exit=1

baseline harness (4e6b3a4): j2r test Uncaught.java  ->  PASS   (exit 0)  <-- false negative
hardened  harness (2c9ae0e): j2r test Uncaught.java ->  FAIL, diagnostic:

  stderr (JVM frames normalized) differs:
    line 1: expected: "Exception in thread \"main\" java.lang.IllegalStateException: uncaught-boom-42"
    line 1: actual:   "Exception in thread \"main\" java.lang.WrongException: injected-false-negative"
```

**Source changes.**

* `crates/j2r-cli/src/compare.rs` (new, 224 lines): `RunResult { stdout,
  stderr, code }`, `normalize_stderr` (removes ONLY JVM stack-trace frame
  lines — `\tat ...`, `\t... N more`, `Suppressed: ...` — never exception
  types, messages, `Caused by:` chains, or program output), and
  `streams_match` comparing all three channels with a line-level diff
  diagnostic.
* `crates/j2r-cli/src/main.rs`: both run paths capture stderr into
  `RunResult`; comparison goes through `streams_match`.

**New regression tests.**

* 10 unit tests in `compare.rs` pin the normalization semantics: different
  exception types fail, different messages fail, different exit codes
  fail, different stderr content fails, empty stderr must stay empty; only
  frame lines may differ.
* 4 new differential fixtures (suite now 18, and `diff_suite.rs` asserts
  the count never shrinks below 18):
  * `ErrStream.java` — `System.err` output of every primitive type,
    interleaved with stdout, plus an unflushed `print` / `println()` pair;
  * `Uncaught.java` — uncaught exception: same type + message on stderr,
    same exit code 1;
  * `Runtimes.java` — caught `ArithmeticException`,
    `ArrayIndexOutOfBoundsException` (exact OpenJDK message
    `Index 5 out of bounds for length 3`), `NegativeArraySizeException`,
    `NullPointerException`, `ClassCastException`, then an uncaught
    `ArrayIndexOutOfBoundsException` at the end (stdout prefix, stderr
    type/message and exit code all compared);
  * `ZeroInit.java` — statics without `<clinit>` get JVM default values.

**JVM vs native result.** All 18 fixtures PASS against Temurin 21
(`cargo test -p j2r-cli --test diff_suite`, 29.5 s locally, 18.1 s in CI).

## H2 — No bytecode verification (runtime panics on hostile input)

**Original bug.** The IR decoder stated "no stack simulation, no
verification". A hand-assembled class with `aload 7` and `max_locals = 1`
compiled on the baseline and the generated binary panicked at runtime:

```
thread '<unnamed>' panicked at src/classes/T.rs:17:34:
index out of bounds: the len is 1 but the index is 7
NATIVE EXIT=101
```

The JVM rejects the same class with `java.lang.VerifyError: Local variable
table overflow`. Worse, `pop` on an empty operand stack compiled to a
**silent miscompile** (exit 0, no output) instead of a rejection.

**Reproduction.** Both hostile classes are constructible with the checked-in
test builder (`crates/j2r-codegen/tests/common/builder.rs`); the
`malformed.rs` e2e tests below run the full pipeline (parse → decode →
verify) on them.

**Source changes.**

* `crates/j2r-codegen/src/verify.rs` (new, 1903 lines): an abstract
  interpreter that runs before code emission and proves, per method:
  operand-stack height consistency across every control-flow join,
  category-1/category-2 correctness (all four canonical JVMS `dup2_x2`
  forms), branch/switch targets as instruction boundaries, local-variable
  indices in range with category-2 two-slot discipline, entry locals from
  the descriptor, return/descriptor agreement, `max_stack`/`max_locals`
  limits, exception-table structure and catch-type resolution, array
  element kinds, and handler-entry stack shape.
* `crates/j2r-codegen/src/lib.rs`: `compile_to_crate` runs the verifier on
  every method body before emission; failures surface as a structured
  `CodegenError::Verify` naming the method, the faulting `pc` and the
  rule violated.
* `crates/j2r-codegen/src/ir.rs`: typed `ALoad`/`AStore` instructions
  carrying the array element kind, so element-type mismatches are a
  compile error.

**New regression tests.**

* 36 unit tests in `verify.rs` (stack underflow, `swap` of a long, all
  `dup2_x2` forms, mid-instruction branch targets, out-of-range and
  category-2-half locals, uninitialized locals, join height/type
  conflicts, return/descriptor mismatches, `max_stack` overflow, handler
  entry shape, unknown catch types, unreachable code, falling off the end,
  ...).
* 8 e2e tests in `crates/j2r-codegen/tests/malformed.rs`: hand-assembled
  malformed class files must be rejected with a `verification error`
  diagnostic containing the expected rule name — and one positive control
  proving well-formed code still compiles.

**JVM vs native result.**

```
hostile class                 JVM (Temurin 21)            baseline j2r            hardened j2r
aload 7, max_locals=1         java.lang.VerifyError       panic, exit 101         compile error, exit 1:
                              (rejected)                                          "verification error in
                                                                                   T.main(...)V at pc 0: local
                                                                                   variable index 7 out of range
                                                                                   (max_locals 1)"
pop on empty stack            VerifyError (rejected)      silent miscompile,      compile error, exit 1:
                                                          exit 0                  "operand stack underflow"
```

## H3 — 16-bit identifiers capped the design below the roadmap

**Original bug.** `crates/j2r-codegen/src/lib.rs` allocated static-pool
slots with `let mut static_slots = 0u16`. A program whose combined static
pool exceeds 65,535 slots hit `attempt to add with overflow` (debug) /
silently wrapped (release) at `static_slots += 1` — while ROADMAP.md
promised >70,000 classes. Reproduced on the baseline by compiling two
synthetic 40,000-static classes:

```
thread 'static_pool_beyond_65535_slots_compiles_and_runs' panicked at
crates/j2r-codegen/src/lib.rs:321:17: attempt to add with overflow
```

**Source changes.**

* Class ids (`ClassPlan::id`), static slots (`FieldPlan::static_slot`)
  and the slot counter are `u32`; `total_statics` is `usize`.
* The generated per-app `statics` registry is widened: `get(idx: u32)`,
  `ensure(class_id: u32, ...)`, `init_pool(statics: usize, classes:
  usize)`, run-length default initialization.
* Constant-pool indexes deliberately stay `u16` — that is the JVM-spec
  limit (JVMS §4.4, u2 entries), not an implementation choice.

**New regression test.** `crates/j2r-codegen/tests/wide_ids.rs` assembles
two synthetic class files (40,000 static fields each — under the JVM's
per-class 65,535 cap), compiles the 80,000-slot program, asserts the
generated registry is u32-addressed and contains no `u16`, asserts the
main body references `statics::get(76543u32)` (slot 76,543 — past the old
u16 boundary), builds the generated crate and runs it, asserting stdout
`0\n0\n` and exit 0.

**JVM vs native result.** The same two-class program runs on the JVM and
on the hardened native binary with identical output (`0\n0\n`, exit 0).
On the baseline compiler the same program failed to compile (overflow
panic). CI runs this test in gate 3 (1.2 s).

## H4 — Rc<RefCell<JObj>> heap: audited and documented, not silently trusted

**Original issue.** Shared object identity is `Rc<RefCell<JObj>>`. Cycles
are never collected, and `RefCell` double-borrows plus `expect`-style
accessors are panic paths. The audit required a precise statement of what
the single-threaded milestone may claim — explicitly *without*
implementing a tracing GC in this milestone.

**Source changes.** `docs/RUNTIME_SAFETY.md` (new, 124 lines) records:

* the ownership model (`Rc<RefCell<JObj>>`, `Value::Str(Rc<String>)` with
  `Rc::make_mut` copy-on-write, interning);
* the honest cycle-leak boundary: cyclic graphs stay reachable and correct
  (no double-free, no wrong values), but unbounded cyclic allocation
  exhausts memory and aborts the process — which is why a tracing,
  cycle-collecting GC is scheduled at M2, before any long-running or
  threaded workload;
* the single-threaded invariant as an architectural property (all shared
  runtime state is `thread_local`; Java-level `Thread`/`synchronized`/
  `wait`/`notify` are compile errors, so no generated program can depend
  on concurrency that does not exist);
* a panic-path table: every `expect`/`assert`/indexing site in the runtime
  mapped to its guard (the H2 verifier for stack/type/local invariants,
  compile-time layouts for field slots) and classified as reachable-only-
  by-a-codegen-bug; Java-program errors (NPE, OOB, CCE, ...) are `Throw`s,
  never Rust panics;
* a reentrancy census: 30 non-test `.borrow()`/`.borrow_mut()` sites
  (natives.rs 15, object.rs 6, lib.rs 5, array.rs 4; concat.rs borrows
  only through `object::string_value`), every scope statement-local or
  dropped before dispatching into user code — no double-borrow path
  exists in the current instruction set.

**Regression tests.** H4 is a documentation deliverable; its claims are
mechanically enforced by the H2 verifier tests (panics unreachable from
accepted bytecode) and the differential suite (Java errors behave as
catchable exceptions, matching the JVM).

**JVM vs native result.** `Runtimes.java` demonstrates the contract: five
caught runtime exceptions produce identical stdout on both engines, and
the trailing uncaught AIOOBE produces the same stderr type/message and
exit code 1 on both.

## Found and fixed during the milestone (not requested, discovered by the work)

* `ConstantValue` static finals were never written to the static pool
  (synthesized stores landed after the `<clinit>` return). Now set at pool
  startup, matching the JVM's preparation phase (JVMS §5.1).
* Statics without `<clinit>` kept `Value::Void` instead of typed defaults;
  the pool is now initialized with typed default values (run-length
  ranges) — covered by the `ZeroInit` fixture.
* `synchronized` methods compiled silently without locking; they are now a
  compile error ("no monitor support"), consistent with the documented
  single-threaded invariant.

## CI evidence

All eight gates green on GitHub Actions for commit `2c9ae0e` (run
41180545706) and re-executed locally on a clean toolchain (rustc 1.99.1,
Temurin 21.0.13): fmt, clippy `-D warnings`, 111 tests (unit + e2e +
malformed + wide-ids + 18-fixture differential), release build, offline
standalone generated-crate build, doc lint with `-D warnings`, RustSec
audit (25 dependencies, 0 advisories), aggregate gate.
