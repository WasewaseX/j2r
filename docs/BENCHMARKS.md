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
