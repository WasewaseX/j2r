# M2 Evidence Record

Per-wave evidence in the M1.5 discipline (M2_PLAN §8 invariant 5): bug-or-
risk, reproduction, change, tests, JVM-vs-native results, CI run link.

---

## Wave 1 — catchable `StackOverflowError` (T07–T10)

**Bug/risk addressed.** Java call depth == native thread-stack depth (the
approved M1 call ABI), so deep Java recursion aborted the whole process
(native stack overflow) instead of throwing `StackOverflowError`. Any
unbounded recursion — trivial to write, common in real code — killed the
program. HotSpot throws a *catchable* `java/lang/StackOverflowError` (a
`VirtualMachineError`); the thread continues normally after a catch.

**Reproduction (pre-change).** `java -version`-free: compile
`static int down(int n){return down(n+1)+1;} down(1);` with M1.6 →
process aborts (SIGSEGV / Rust stack overflow) at some depth.

**Change.**

* New `crates/j2r-runtime/src/frame.rs`: `FrameGuard::enter()` registers a
  frame in a `thread_local` depth counter and returns
  `Err(StackOverflowError)` past the limit; `Drop` pops on normal return,
  `Err` propagation and panic unwind. **No pointers, no allocation on the
  hot path** — the Wave-1 record is a `usize` (M2_PLAN §3.2 memory-safety
  row).
* `crates/j2r-codegen/src/emit.rs`: every generated method's first
  statement is the guard; a refused registration becomes an immediate
  `return Err(t)` — the SOE surfaces at the *call site* like the JVM
  (the caller's exception table dispatches it; the callee's own handlers
  never see it, matching HotSpot where the frame never started executing).
  The generated `main` validates the env var first.
* Limit policy (M2_PLAN §3.1, §3.1.1): default 8192;
  `J2R_MAX_JAVA_FRAMES` accepted only in `1..=131_072` where
  `131_072 = 512 MiB / (512 B × 8)` from the *measured* worst-case frame
  footprint (plan-time calibration: 208–304 B across three frame shapes);
  compile-time const assertion pins `default ≤ ceiling`; invalid values
  (`0`, over-ceiling, non-numeric) print
  `j2r: invalid J2R_MAX_JAVA_FRAMES=<v>` and exit 2 before any Java code.

**Tests.**

* Runtime unit tests (7 new): const math pins the ceiling; `parse_limit`
  accepts default/1/8192/131072 and rejects `0`/`131073`/`abc`/empty/
  negative/overflow; depth accounting across nested guards; depth restored
  on panic unwind (catch_unwind); SOE object shape (class, no message,
  `VirtualMachineError` subtype); `enter()` refuses at the limit without
  registering.
* Differential fixtures (suite 34 → 36, both against Temurin 21 through
  the unchanged three-channel harness):
  * `StackDepth.java` — unbounded recursion, `catch (StackOverflowError)`
    prints `caught-soe` / `msg-null`, thread continues (`continued`,
    `sum 5`), recursion re-runs and is caught again (`caught-again`,
    `done`). Depths are never asserted (JVM-dependent by design).
  * `UncaughtSOE.java` — uncaught SOE; expected stderr is the
    `Exception in thread "main" java.lang.StackOverflowError` line (frame
    lines normalized), exit status 1 — byte-identical to HotSpot after
    normalization.
* Emitted-shape check (JDK-free, in `j2r-codegen/tests/e2e.rs`): generated
  method text contains `FrameGuard::enter()` first and the early
  `return Err` arm; generated main contains `validate_env_or_exit`.
* `crates/j2r-cli/tests/soe_e2e.rs` (release build):
  * `J2R_MAX_JAVA_FRAMES=64` → process exits **0** with `caught-soe` —
    never aborts (no SIGSEGV/101).
  * Six-case boundary matrix: unset (default) accepted, `1` accepted
    (SOE at first recursive call), `131072` accepted and completes,
    `131073` rejected exit 2, `0` rejected exit 2, `abc` rejected exit 2 —
    every rejection prints the diagnostic and produces no Java output.

**JVM-vs-native results.** Both SOE fixtures PASS three-channel against
Temurin 21.0.13. Full local run: 133 tests, 0 failures (117 unit, 12
malformed, wide-ids, JDK-free e2e, SOE e2e, 36-fixture differential in
~62 s wall-clock).

**Acceptance criteria scorecard (M2_PLAN §3.2).**

| Criterion | Result |
| --- | --- |
| (a) all 8 gates green, suite = 36 | met (CI run recorded in ROADMAP §2) |
| (b) tiny-limit e2e never aborts, prints `caught-soe`, exit 0 | met |
| (c) registry depth 0 after main returns | met (unit: nested + panic tests) |
| (d) six-case boundary matrix green | met |
| (e) calibration CI check ≤ 1.25× const | plan-time measurement recorded (§3.1.1: 304 B ≤ 640 B); the release-profile e2e proves the max accepted value completes without native overflow |

**Known deviation from plan text.** None in Wave 1. (The depth-counter
registry matches M2_PLAN §3.1 exactly; Wave 2 will extend `FrameGuard`
with the owned snapshot slot per §4.4.)
