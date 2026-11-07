# Benchmarks (R5) — observe-only

Policy (docs/STRATEGIC_GAPS.md R5, locked): **benchmarks observe, never
gate.** No performance sentence without a number, a date, and a
reproduction command. The harness is `scripts/bench_suite.py`; raw rows
append to `scripts/bench_results.csv`. Nothing in CI reads them.

## Method

For each fixture (stems under `crates/j2r-cli/tests/suite/`):

* `javac` — one reference compile (informational).
* `java` — median of 5 measured runs (1 warmup) of
  `java -XX:TieredStopAtLevel=1 -cp <classes> <Main>`. C1-only JIT is
  deliberate: these programs run for milliseconds, where the full
  tiered JIT would only add warmup noise.
* `j2r-gen` — the gate-5 proven pipeline: `j2r compile <All>*.class
  --main-class <Stem> --crate-name j2r_app` followed by
  `cargo build --release --offline` of the generated crate (the runtime
  is vendored into the crate). The reported number is translation +
  build, i.e. the true compile-side cost a user pays.
* `native` — median of 5 measured runs of the generated binary.

Environment knobs that change scheduling (`J2R_SCHED`,
`J2R_SCHED_SEED`, `J2R_GC_EVERY_N`, `J2R_CHAOS_US`) are stripped; every
measurement is pure default mode. Differential correctness is NOT
asserted here — that is gate-3's job.

Reproduction (this exact table):

```
python3 scripts/bench_suite.py --reps 5 \
  Arith Strings BigRpg WaitNotify ThreadHello ThreadSafepoint
```

## First recorded table — 2026-11-05, Intel Xeon 2 cores, x86_64

| fixture | javac (s) | java (ms) | j2r-gen (s) | native (ms) | native/java |
|---|---|---|---|---|---|
| Arith | 0.590 | 26.5 | 7.18 | 1.0 | 0.04x |
| Strings | 0.631 | 46.9 | 6.97 | 0.9 | 0.02x |
| BigRpg | 0.826 | 62.7 | 8.69 | 1.2 | 0.02x |
| WaitNotify | 0.596 | 196.6 | 7.64 | 152.5 | 0.78x |
| ThreadHello | 0.519 | 38.8 | 6.75 | 1.1 | 0.03x |
| ThreadSafepoint | 0.592 | 43.3 | 6.78 | 3.3 | 0.08x |

## Honest reading (all numbers above, same date)

* **Run-time, startup-dominated micros:** on compute-bound fixtures the
  generated native binary finishes in ~1ms vs ~26-63ms for the JVM —
  but these programs are so short that JVM process startup dominates
  the java column; the honest sentence is "native startup + run is
  20-50x cheaper at this scale", not "20-50x faster Java execution".
* **WaitNotify (0.78x):** the fixture's semantics include real
  millisecond sleep windows on both channels; wall time is dominated by
  those windows, not by the runtime — parity is the expected result and
  a good sanity check that the coop/monitor machinery adds no
  pathological overhead.
* **Compile-side:** the ~7-9s j2r-gen cost is dominated by the cargo
  release build of the vendored runtime (translation alone is well
  under a second — the harness isolates each fixture in a fresh
  target dir, so no incremental cache is reused). javac is ~10x
  faster at this stage; the runtime build is a one-time cost per
  generated crate and a known, tracked cost center, not a hidden one.
* **What this table is NOT:** a throughput benchmark, a JIT-vs-native
  steady-state comparison, or a Minecraft-scale claim. Those need the
  corpus expansion (F2/F4/F5) and larger real programs first — per the
  locked policy, no claim runs ahead of the evidence.

## Next measurement obligations

* Re-run after any runtime scheduling change (the coop/monitor rework
  classes are the ones that could move WaitNotify/ThreadSafepoint).
* Add one row per new corpus entry (real-program evidence, not
  micro-probes).

## Corpus entry 2 row (JZlibCorpus) — 2026-11-05, 2-core Xeon / 4 GB, recorded with a build-side limit

Repro: `python3 scripts/bench_suite.py --reps 5 JZlibCorpus` (the harness
resolves corpus stems under `corpus/` since the JZlib row landed).

| fixture | javac(s) | java(ms) | j2r-gen(s) | native(ms) | native/java |
|---|---|---|---|---|---|
| JZlibCorpus | 1.592 | 50.1 | **release build does not fit 4 GB — row incomplete** | 124.4 (DEBUG binary — NOT comparable to the release rows above) | — |

The honest state:

* **Translation is 75ms** for the 7.2k-line library — the compile-side
  story stays "translation is cheap, the vendored-runtime cargo build
  dominates" exactly as the first table recorded.
* **The release build of the JZlib generated crate is SIGKILLed by the
  sandbox at every opt level (3, 1, even codegen-units=16) on this
  4 GB machine** — rustc's own MIR passes on the mega-functions the
  giant switch tables produce (debug builds pass with
  RUST_MIN_STACK=32 MiB, which the harness now sets for all child
  builds). This is a REAL codegen-scale finding, not a bench footnote:
  M5's 70k-class target cannot be one giant emitted function per
  method-table monster — split-table / smaller-function emission is a
  registered codegen work item (see docs/CORPUS_WAVE.md §Wave 2 and
  TODO.md M5).
* The 124.4 ms figure is the DEBUG binary's run time, recorded only as
  a scale reference ("a full deflate/inflate round-trip battery runs
  end-to-end in ~0.12s even unoptimized"); it is NOT the release
  comparison and must not be quoted as one. The release row lands when
  the generated crate fits a standard runner's memory (either after the
  codegen scale-up or on a bigger machine).

* **2026-11-06 re-confirmation, quantified:** five fresh attempts at the
  release build of the same crate (opt-level 3, opt-level 2,
  codegen-units=16, three quiet-system retries) all died SIGKILL with
  rustc peak RSS measured at **3.03–3.56 GB** — the borrow-check/MIR
  phase, not codegen, dominates (opt-level 2 peaked HIGHER than 3). The
  sandbox has ~3.3 GB available; the limit is real and stable.

## Corpus bench rows — 2026-11-06, 2-core Xeon / 4 GB (wave 3)

Repro (the harness resolves corpus driver stems under `corpus/`):

```
python3 scripts/bench_suite.py --reps 5 MinimalJsonCorpus JamaCorpus
```

| fixture | javac (s) | java (ms) | j2r-gen (s) | native (ms) | native/java |
|---|---|---|---|---|---|
| MinimalJsonCorpus | 1.450 | 74.3 | 17.66 | 2.6 | 0.04x |
| JamaCorpus | 1.663 | 72.3 | 52.41 | 6.2 | 0.09x |

* These are REAL programs, not micro-probes: minimal-json parses,
  mutates and pretty-prints JSON documents; JAMA runs the full
  decomposition battery (LU/QR/Cholesky/SVD/Eigen over the Hilbert 5x5
  ill-conditioned matrix, thousands of IEEE double ops through the
  interpreter). Both natives finish startup+run in 2.6-6.2 ms vs
  72-74 ms C1 JVM runs — the same startup-dominated regime as the
  first table, now on real-world code shapes.
* j2r-gen grew with corpus size: JAMA's 3.5k-line merged file costs
  52.4 s of translate+release-build vs 17.7 s for minimal-json's 4.0k
  lines (JAMA's dense numeric loops emit far more per-line IR than a
  parser's control flow). Translation itself stays sub-second; the
  vendored-runtime cargo build dominates both.
* **JZlibCorpus stays rowless on purpose** — the release build is the
  registered memory limit above (re-confirmed and quantified today);
  its debug-binary scale reference from 2026-11-05 remains the only
  JZlib number, and per the locked policy no claim is made from it.

## First steady-state comparison — 2026-11-06, 2-core Xeon / 4 GB (JamaBench)

The R5 register's remaining item ("steady-state/throughput numbers") is
now measured for the first time. `corpus/JamaBench.java` (assembled by
`scripts/build_corpus_jamabench.sh` from the same verbatim NIST JAMA
1.0.3 sources) runs 1200 iterations of a real decomposition battery —
LU, QR, Cholesky on an SPD product, symmetric Eigen, rectangular SVD,
LU solve, over 40x40 / 40x24 problems — with a fixed xorshift seed and
a 3-line deterministic output. At K=1200 the full tiered JVM reaches
steady state (startup + JIT warmup are noise at ~2.5s of hot-loop
execution).

Repro (three harness invocations; javaC1 = -XX:TieredStopAtLevel=1 as
always, javaFull = the DEFAULT full tiered JVM C1→C2):

```
python3 scripts/bench_suite.py --reps 5 --warmups 2 --fulljit \
    --native-reps 1 --native-warmups 0 JamaBench
python3 scripts/bench_suite.py --reps 5 --warmups 2 --fulljit \
    JamaCorpus MinimalJsonCorpus
```

| fixture | javac (s) | java C1 (ms) | java full JIT (ms) | j2r-gen (s) | native (ms) | native/C1 | native/full |
|---|---|---|---|---|---|---|---|
| JamaBench (steady state) | 1.224 | 1313.4 | 3181.9 | 49.81 | 274223.1 | **208.8x** | **86.2x** |
| JamaCorpus (startup-dominated context) | 1.244 | 81.0 | 72.6 | 55.65 | 5.9 | 0.07x | 0.08x |
| MinimalJsonCorpus (startup-dominated context) | 1.279 | 71.0 | 81.0 | 18.33 | 2.4 | 0.03x | 0.03x |

Raw rows: `scripts/bench_results_fulljit.csv` (the `native_reps` /
`native_warmups` columns record exactly how many runs produced each
native median).

Honest reading (all numbers above, same day, same box):

* **Steady-state hot loops: the native binary is ~86x slower than the
  full tiered JIT and ~209x slower than C1-only on real dense numeric
  code.** The startup-dominated corpus rows taken in wave 3 (native
  0.03-0.09x, i.e. 12-33x ahead in wall time) coexist with this: BOTH
  are true, they measure different things. On millisecond-scale
  programs the JVM's startup + JIT warmup dominate and the AOT binary
  wins big; on seconds-scale hot loops the JIT's compiled code wins by
  two orders of magnitude. Neither number may be quoted without its
  regime.
* **Attribution — it is the execution model, not the GC.** The default
  emission is a bytecode-level interpreter per method: a `match pc`
  dispatch loop over a `Vec<Value>` operand stack and `Vec<Value>`
  locals (every operand is a cloned enum), runtime helper calls for
  every array access, stringly-typed virtual dispatch
  (`invoke_virtual(.., "Matrix", "getArray", "()[[D", ..)`), and
  `frame.refresh` + `gc_point` safepoint bookkeeping at block
  boundaries. C2 compiles the same loops to register machine code with
  bounds-check elimination, escape analysis and vectorization — the
  classic interpreter-vs-JIT gap, now measured on real code instead of
  assumed. Control experiment: `J2R_GC_EVERY_N=999999999` (GC cadence
  effectively disabled) runs in 273.4s vs the 274.0s hand median —
  removing the collector changes nothing; the interpreter does.
* **Native-channel protocol note.** A single steady-state native run is
  ~274s, so the sandbox's hard per-process time cap cannot fit
  reps=5+warmup in one session: the harness row carries the one
  mechanically-produced native run (274.2s, native_reps=1), and four
  additional runs of the SAME binary under the SAME protocol were taken
  by hand the same hour: 274.1s / 273.5s / 273.9s / 275.7s (median
  274.0s, spread ±0.4%). The harness run sits 0.1% off the hand median.
* **JVM-side caveat (do not overclaim in the JIT's favor either):** on
  this 2-core box at this run length the full tiered JVM is SLOWER than
  C1-only (3.18s vs 1.31s) — C2 compile threads and profiling overhead
  cost more than C2's better code earns back in ~3s of wall time, and
  the allocation-heavy battery gives escape analysis little room. The
  86x figure is honest for THIS box and run length; a long-running
  server workload would amortize C2 better and widen the gap further.
* **Correctness came free:** every JamaBench run on every channel
  (java C1, java full JIT, native, GC-deferred native) printed the
  byte-identical 3-line output (`acc=5.3624293456203656E14`,
  `seedTail=-5877300977697633972`) — an observed differential proof on
  the bench workload itself. (Observation, not a gate: differential
  correctness remains gate-3's job; pinning JamaBench into
  corpus_suite is a cheap follow-up since the output is deterministic
  and tiny.)
* **Where this closes:** M2's CFG reconstruction prototype
  (`--emit-cfg`) is provably output-equivalent across the whole suite
  (cfg_equiv gate, 107/107 both modes, 452.7s + 449.5s at the current
  tree) and its default-emission NO-GO (docs/M2_CFG_REPORT.md) was a
  scope decision, not a performance measurement. Lifting structured
  register emission to default is now the highest-leverage performance
  item on the board; with this harness, its cost claim is one
  `--fulljit` command away. Per the locked policy these numbers observe
  and never gate.
* **Same-day addendum — the CFG prototype measured, and it is NOT the
  lever.** The same JamaBench workload translated with `--emit-cfg`
  (M2's output-equivalent structured-CFG emission; j2r-gen 47.4s) and
  release-built runs in **274.1s — indistinguishable from the 274.0s
  hand median of the default emission** (output byte-identical again).
  The lift was partial (Matrix.rs 63 → 40 match-pc dispatchers,
  Eigen 10 → 6, SVD 8 → 3, Cholesky 4 → 2; LU and QR fully fallback —
  their bodies carry handler edges), so this measures a mix of lifted
  and fallback bodies under an UNCHANGED operand model (`Vec<Value>`
  stacks, per-access runtime helpers, stringly dispatch, per-block
  safepoint refresh). Honest conclusion: control-flow structuring
  alone buys nothing at steady state; the registered performance lever
  is the typed-register / typed-operand emission model (locals in
  machine registers, direct field access, direct calls), with CFG
  structuring as its prerequisite rather than its payload.

## 2026-11-07 addendum — the lever confirmed at a second lift-coverage extreme

The typed-register readiness wave (docs/TYPED_REGISTER_READINESS.md)
found and fixed three lift-pipeline defects that had silently zeroed the
lift's real-loop coverage (every javac `for`/`while` loop was mis-flagged
`Irreducible` or refused as `ImpureJoin`). With the fixes, `--emit-cfg`
lift coverage on the same JamaBench workload nearly tripled — **31
remaining match-pc dispatchers vs 106 default** (Matrix 63 → 12; LU 8 → 4
and QR 6 → 5, no longer fully fallback; SVD 8 → 1; Eigen 10 → 6;
Cholesky 4 → 2; driver 5 → 0), release build 50.1s — and the steady-state
run is **275.6s, output byte-identical, no gain** (vs the 274.0s hand
median of 2026-11-06, same box and protocol). Control-flow structuring
now measured at BOTH coverage extremes buys nothing at steady state: the
hottest kernels (tred2/tql2/orthes/hqr2, LU ctor) are exactly the
still-refused shapes (`ImpureLoopExit` / `MultiExitLoop`), and lifted
bodies still run on the `Vec<Value>` operand model. The typed-operand
model remains the registered lever; its named prerequisites and the
GO sequencing are in docs/TYPED_REGISTER_READINESS.md §5.

## 2026-11-07 addendum 2 — lift closure (ImpureLoopExit + MultiExitLoop) landed; structuring still flat at a third coverage point

The readiness GO's step 1 (docs/TYPED_REGISTER_READINESS.md §5) is
landed: exit edges grouped by TARGET block (javac `break`/`continue`
inside `while`/`do-while` add edges to the SAME after-loop block),
terminator targets (early `return`/`athrow` blocks) excluded from the
open-exit set (emitted inline at the edge), goto-trampoline confluence
resolution (javac routes `break` through single-goto blocks), the
shared-exit shape routed through the enclosing join label, and a
structural safety net refusing any degenerate self-continue loop.
Two real generated-code bugs were caught by cfg_equiv ON THE WAY
(both in shapes newly unlocked by the lowerings, both fixed):
`continue <join-label>` (illegal Rust — labeled blocks cannot be
`continue`d) and a body walk hijacked into a tight self-continue loop.

Census (cfg-report, same measurement basis as the readiness wave):
suite needs-lift 146 → **126**, JAMA 32 → **21**; `ImpureLoopExit` is
GONE from both (2 and 7 → 0), suite `MultiExitLoop` 24 → 2 (the two
genuine labeled-break methods), JAMA 5 → 0 — tql2 and hqr2 now LIFT.
JamaBench `--emit-cfg` dispatchers 31 → **21** (Matrix 12 → 9, LU 4 → 2,
QR 5 → 2, Cholesky 2 → 1, Eigen 6 → 5). Steady-state control run at
this third coverage point: **273.9s, output byte-identical** (vs the
274.0s hand median) — structuring STILL buys nothing, now measured at
three coverage extremes. The residual refusals are the registered
next layer: residual diamond-shape refusals (suite 36 / JAMA 13
`ImpureJoin` — the arm walks of tred2/orthes/LU-ctor diverge to
different joins), exception-table bodies (82/9), and switch lowering
(7/1). The typed-operand model remains the lever; validation evidence:
cfg_equiv 107/107 both modes (6 shards), differential 107/107 vs
Temurin 21.0.13 (437.9s), codegen lib tests 87 green, corpus 3/3.

## 2026-11-08 addendum 3 — the typed-operand backend (first slice) measured; structuring + typed LOCALS still flat at a fourth coverage point

The typed-register backend's first slice is LANDED (`typed.rs`, off by
default, `--emit-typed`): per-method live-range epochs give JVM locals
REAL Rust types (`v1e0: f64`, no `Value` box, no per-access `expect_*`)
with entry values taken from the descriptor's argument slots only —
cfg_equiv caught the first live bug on the way (BigJson `skipWs()V`
panicked "expected int value, got Void": the preamble read a loop-live
NON-argument local from the unmarshal Vec; fixed to read only
`j < arg_slots`, slot-widened per the descriptor, `this` included) —
and the backend is verified suite-wide: cfg_equiv 109 × {base, cfg,
typed} zero divergences, diff_suite 109, corpus 4/4 vs Temurin.

JamaBench typed census: 75 of 106 methods emit typed bodies (31 fall
back: 27 `NoPrimitiveEpoch`, 1 `HasInvokeDynamic`, 1 `HasSwitch`, 2
driver); the HOTTEST kernel (`Matrix.times`) IS typed. Controlled
same-session pair (the 2026-11-06 274.0s baseline is 1.31x stale on
today's machine state — the control run re-measured it at 358.7 s,
CSV stamp 2026-11-07T18:38, so only same-session pairs are quotable):

* default emission: **358.7 s** (harness row) / 357.4 s (hand run)
* `--emit-typed`: **355.9 s** (hand run) — ratio 0.992, NO steady-state
  gain; nat/full 97.3x at the same-session JVM (3.657 s full tiered)

Both runs output byte-identical to the JVM (observed differential).
Why flat, precisely: the slice types the LOCALS but the operand stack
still runs on `Value` — 2,457 `expect_*` extraction sites and 367
per-block `gc_point()` safepoints remain across JamaBench's classes,
plus `obj_get`/`obj_set` per field access and `invoke_virtual` per
call. This is the SAME verdict the CFG addenda measured at three
coverage points, now confirmed for typed locals at a fourth: operand
EXTRACTION cost, not control-flow shape or local boxing, is the
residual steady-state cost. The registered lever's remaining
prerequisite is operand-stack/register unification — typed stack slots
feeding operators directly — per docs/TYPED_REGISTER_READINESS.md §5.

Repro (2-core Xeon / 4 GB sandbox, Temurin 21.0.13+9):

    # same-session default control (CSV row 2026-11-07T18:38:29):
    python3 scripts/bench_suite.py --fulljit --reps 3 \
        --native-warmups 0 --native-reps 1 JamaBench
    # typed run (the harness row needs >600 s for gen+build+run on this
    # box; the hand pair fits):
    javac -d /tmp/jb-cls corpus/JamaBench.java
    j2r compile --emit-typed -o /tmp/jb-typed /tmp/jb-cls/*.class \
        --main-class JamaBench --crate-name j2r_app
    cargo build --release --manifest-path /tmp/jb-typed/Cargo.toml
    time /tmp/jb-typed/target/release/j2r_app   # 355.9 s, byte-identical

## 2026-11-08 addendum 4 — StringsSimCorpus bench row taken (the wave-4 deferral closed)

Corpus wave 4 landed the java-string-similarity differential WITHOUT a
bench row ("correctness first; the JamaBench steady-state protocol
applies when they are"). Row taken this session, same protocol as the
2026-11-06 corpus rows (median of 5, 1 warmup; the harness's fulljit
channel raises the java-full warmups to 2 itself), 2-core Xeon / 4 GB
sandbox, Temurin 21.0.13+9, release j2r at 485297f + this commit's tree:

    fixture             javac(s) javaC1(ms) javaFull(ms) j2r-gen(s) native(ms) nat/C1 nat/full
    StringsSimCorpus       1.119       68.7         70.5      15.91        4.0   0.058   0.057

CSV: scripts/bench_results_fulljit.csv stamp 2026-11-07T20:41:42
(native_reps=5, native_warmups=1). Reading, honestly and by the
standing rule that neighboring rows must travel together: this driver
run is STARTUP-DOMINATED (the algorithm battery runs once, ms-scale),
so 0.058 sits on the same line as MinimalJsonCorpus 0.034 and
JamaCorpus ~0.09 — native is ~17x ahead in wall time at this run
length. It is NOT a steady-state claim and must never be quoted
against the JamaBench steady-state row (nat/full 98.1x the other
direction, docs above): different regimes, neither quotable without
the other. j2r-gen 15.91 s = translation + offline release build of
the generated crate (the compile-side cost, same shape as the other
corpus rows). The owner's independent hand-port evidence
(results_v1.1: 4/14 workloads to Rust, 1.8-2.3x on reuse-heavy,
0.6-0.77x on short-string per-call) is a STEADY-STATE-REGIME data
point for the typed-operand lever, not for this row — see
docs/TYPED_REGISTER_READINESS.md §5 item 3 for the registered
collection-boxing measurement it motivates.

Repro:

    export PATH="$HOME/.cargo/bin:$PATH"
    python3 scripts/bench_suite.py StringsSimCorpus --reps 5 --warmups 1 --fulljit

## 2026-11-08 addendum 5 — StringsSimBench: the FIRST steady-state row for the
## stringsim family (and it lands in the JamaBench regime)

Corpus wave 4 pinned the java-string-similarity differential; this
addendum adds the LOOP-SIZED stem (StringsSimBench, merged by
scripts/build_corpus_stringsimbench.sh — the SAME library code as
StringsSimCorpus plus a deterministic xorshift64* driver) mirroring the
owner's results_v1.1 workload shapes at bench scale: W1 Levenshtein
short pairs 200k, W5 JaroWinkler 500k, W7a getProfile x1,000 docs +
W7b Cosine over PREBUILT profiles x10,000 (float aggregation
deliberately under HashMap iteration — the order law), W8 NGram(2)+(4)
x10,000 DNA pairs. Observed differential BEFORE the row: java (C1 and
full tiered) and the native binary print byte-identical checksums —
w1=2449167.0, w5=192374.1393435562, w7a=811272, w7b=4024.3242675735173,
w8=12044.130793690681 (the w7b/w8 doubles are aggregated over HashMap
iteration order and still match bit-for-bit).

Row (2-core Xeon / 4 GB, Temurin 21.0.13+9, CSV stamp
2026-11-07T21:08:25, native_reps=1 native_warmups=0 per the JamaBench
protocol):

    fixture          javac(s) javaC1(ms) javaFull(ms) j2r-gen(s) native(ms)  nat/C1 nat/full
    StringsSimBench     1.003     3848.6       2710.1      14.34   281325.1  73.10  103.81

Reading, honestly: j2r's steady state is 73-104x behind the JIT — the
SAME regime as JamaBench (86.2x/98.1x, addendum above), now confirmed
on an algorithm family with completely different kernels (DP tables +
float aggregation + HashMap churn instead of dense LU/QR loops). The
two regimes stay non-quotable against each other: startup-dominated
rows (StringsSimCorpus 0.058, addendum 4) and steady-state rows (this
addendum) answer different questions. The attribution carries over
unchanged: the interpreter-shaped default emission (match-pc dispatch
over Vec<Value> stacks, per-access expect_*/obj helpers), NOT the
collector and NOT the algorithm translation — the owner's HAND port of
the same library lands at 0.5-2.3x of the JVM (results_v1.1), so the
~100x j2r gap is almost entirely the execution model. That is the
registered typed-operand unification lever
(docs/TYPED_REGISTER_READINESS.md §5), and this row is its second
family-scale baseline. The compilation-side number stays healthy:
j2r-gen 14.34 s (translate + offline release build of the corpus crate).

Repro:

    export PATH="$HOME/.cargo/bin:$PATH"
    bash scripts/build_corpus_stringsimbench.sh
    python3 scripts/bench_suite.py StringsSimBench --fulljit \
        --reps 3 --warmups 1 --native-reps 1 --native-warmups 0
