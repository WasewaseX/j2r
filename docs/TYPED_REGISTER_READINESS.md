# Typed-register readiness — the M2 shape report's registered follow-up

Date: 2026-11-07. Tree: the readiness wave (see TODO.md §Corpus & quality).

The registered performance lever (docs/BENCHMARKS.md, "First steady-state
comparison") is the **typed-register / typed-operand emission model**, with
CFG structuring as prerequisite. docs/M2_SHAPE_REPORT.md §Recommendation
required that any such decision be re-made with a **live-range** kind
analysis — "per-position live ranges … re-decided with its own 3a report,
not assumed", and measured on real code. This wave delivers exactly that:
the analysis (`j2r readiness-report`), the census over the 107-fixture
differential suite and all three real corpus programs, and — because the
census is only honest if the lift decisions underneath it are — **three
real defects in the CFG lift pipeline that the census exposed**, each
fixed and re-proven.

## 1. The analysis (`crates/j2r-codegen/src/readiness.rs`)

Inputs: the verifier's exported per-pc shapes (`MethodShapes`, stack and
locals) + the instruction stream (for store boundaries) + the REAL
`cfg::lift` decision (no-op emitter, so the report can never claim more
than emission would deliver).

* Kinds collapse to Int/Long/Float/Double/Ref (`Unset`/`Poison`/`HighHalf`
  = dead — the verifier rejects every load of them; `Top`/`Null`/arrays
  collapse to Ref).
* A **kill** ends an occupied run (epoch): for a local slot, every store
  to it (`xstore`/`iinc` — the verifier's own documentation of javac's
  disjoint-range slot reuse); for a stack position, the depth dropping
  to/below it. Kinds may differ ACROSS epochs; a kind change with no kill
  between marks the run **mixed** — counted against readiness, never for
  it (stack shuffles land here: conservative by construction).
* Per method: positions epoch-clean / pool-stable (the M2 3a rule, for
  continuity), epochs (prim/total), occupancy-weighted primitive share,
  and a verdict — `TypedReady` (liftable + all positions epoch-clean + ≥1
  primitive epoch), `LiftableAllRef`, `NeedsCfgLift(reason)`, or
  `MixedEpoch`. 9 unit tests pin every rule.

## 2. Three defects the census exposed in the lift pipeline

All three were **conservative-direction** (fallback to the exact dispatch
body — no wrong code ever ran), and all three hid the true state of the
lift from every earlier report:

1. **`Cfg::idom()` seeded a node's idom with `idoms[pred]` instead of the
   predecessor itself, and intersected on raw block indices instead of
   reverse-postorder numbers.** A single-predecessor loop-body block got
   its parent's idom instead of the parent; every dominator chain through
   a loop body skipped the header; the back-edge dominance test in
   `analyse` mis-flagged **every loop whose header is not the entry
   block** as `Irreducible`. The M2 synthetic tests passed only because
   their loop header WAS the entry block. Fixed: Cooper–Harvey–Kennedy
   with an RPO walk (`javac_while_loop_with_non_entry_header_lifts` and
   `sequential_loops_lift` pin it; the old `irreducible_falls_back` test
   used a graph that is actually reducible and was replaced with a
   genuine two-entry cycle).
2. **`Builder::region()` checked for loop headers only on entry, not on
   fall-through.** javac peels the first-iteration test to the top, so
   the pre-loop block FALLS THROUGH into the guard; emitting the header
   inline turned its guard into a diamond whose meet is the header
   itself, and the join-purity check then saw the entry-side predecessor
   as a third edge → `ImpureJoin` for every javac `for`/`while` loop.
   Fixed: an in-walk check — fresh header → `emit_loop`; active header
   reached by fall-through → `continue` (the do-while back edge).
3. **`join_via_label`'s purity check refused a loop-header meet's own
   back edges** — a nested loop's header is met by the enclosing
   diamond's arm AND re-entered by its body's back edge, which the loop
   emission itself lowers to `continue`. Fixed: the meet's loop body
   nodes (now stored in `Loop`) are exempt; every javac nested-`for`
   numeric kernel refused here before.

Proof obligations re-run after the fixes: codegen lib tests 67 → 83
green; **cfg_equiv 107/107 both modes (451.3s + 445.5s)** — the newly
lifted bodies are output-exact across the whole suite; differential
107/107 (453.0s, default emission unchanged); workspace tests 323 green;
corpus 3/3; jar/lambda/soe green; gates 11/12/19/20 green; path-filter
17/17.

## 3. The census — before → after (same tree, same day)

`j2r readiness-report` (repro in §5). Aggregates:

| group | methods | needs-lift before → after | typed-ready before → after | mixed-epoch before → after |
| --- | --- | --- | --- | --- |
| differential suite (107 fixtures) | 662 | 184 → 146 | 130 → 134 | 129 → 163 |
| JAMA (real numeric library) | 106 | 70 → 32 | 5 → 7 | 18 → 54 |
| minimal-json (real parser) | 292 | 38 → 28 | 42 → 42 | 102 → 112 |
| JZlib (real codec) | 308 | 85 → 74 | 63 → 64 | 126 → 136 |

The `Irreducible` bucket (75 suite / 58 JAMA / 18 minimal-json / 41 JZlib)
is **gone** — redistributed to the reasons the methods actually have. The
newly liftable methods surface as `MixedEpoch`: the operand-model
question the wave was built to ask is now visible for them instead of
being masked by lift refusals.

Post-fix blocker census (the registered remaining lift work, all exact
and per-method):

| reason | suite | JAMA | meaning |
| --- | --- | --- | --- |
| HasExceptionTable | 82 | 9 | handler edges (genuine, registered) |
| ImpureJoin | 30 | 10 | residual diamond-shape refusals |
| MultiExitLoop | 24 | 6 | Java `break` out of a loop (labeled-break lowering) |
| ImpureLoopExit | 2 | 6 | exit block shared with an outside predecessor |
| HasSwitch | 7 | 1 | switch lowering |
| BranchIntoEmittedCode | 1 | 0 | region-walker re-entry |

The hot numeric kernels (primitive occupancy 83–93%):

| method | prim share | positions clean | verdict |
| --- | --- | --- | --- |
| EigenvalueDecomposition.hqr2 | 93.2% | 28/33 | NeedsCfgLift(ImpureLoopExit) |
| EigenvalueDecomposition.tql2 | 92.2% | 24/29 | NeedsCfgLift(MultiExitLoop) |
| EigenvalueDecomposition.orthes | 86.5% | 13/18 | NeedsCfgLift(ImpureLoopExit) |
| EigenvalueDecomposition.tred2 | 83.4% | 13/19 | NeedsCfgLift(ImpureLoopExit) |
| LUDecomposition.<init> | 58.4% | 10/15 | NeedsCfgLift(ImpureLoopExit) |

## 4. Steady-state control measurement — structuring is (still) not the payload

With the fixes, `--emit-cfg` lift coverage on the JamaBench workload
jumped from the 2026-11-06 addendum's census to **31 remaining match-pc
dispatchers vs 106 default** (Matrix 63 → 12, LUDecomposition 8 → 4,
QR 6 → 5, SVD 8 → 1, Eigen 10 → 6, Cholesky 4 → 2, driver 5 → 0) —
the earlier addendum could only reach 63 → 40 with LU and QR fully
fallback. Release build 50.1s. Run (2026-11-07, same box/protocol as the
274.0s hand median): **275.6s, output byte-identical**
(`acc=5.3624293456203656E14`, `seedTail=-5877300977697633972`).

So control-flow structuring buys nothing at steady state **at both lift
coverage extremes** (~40 lifted methods then, ~75 now). Two reasons, both
measured: the hottest inner loops (tred2/tql2/orthes/hqr2, LU ctor) are
exactly the still-refused shapes above, and every lifted body still runs
on the `Vec<Value>` operand model. This is the cleanest possible
confirmation that the **typed-operand model is the lever** and that the
named lift-closure items are its prerequisites, not alternatives.

## 5. Verdict: GO, sequenced

1. **Lift closure** (small, named): `ImpureLoopExit` shared-exit lowering
   and `MultiExitLoop` labeled-break lowering — together they unblock
   tred2/orthes/hqr2/tql2/LU-ctor, the kernels that dominate JamaBench.
2. **Typed-operand emission** behind `--emit-typed` (same discipline as
   `--emit-cfg`): per-epoch typed storage from this module's analysis,
   cfg_equiv-style output equivalence on the full suite, then one
   `--fulljit` JamaBench command for the cost claim.
   Honest design note: the method-level verdict is all-or-nothing
   (mirroring M2), but the per-position columns show partial coverage on
   the hot kernels (hqr2 28/33 clean positions) — a per-position hybrid
   (typed epochs where clean, `Value` where mixed) is a design option the
   typed wave should decide with this census as its baseline.

## 6. Reproduction

```bash
export PATH="$HOME/.cargo/bin:/path/to/jdk-21.0.13+9/bin:$PATH"
# fixture suite (two javac batches: SyncStatic.java and MultiMain.java
# both define a package-private class Other and cannot share a -d dir)
javac -d /tmp/shapes2 crates/j2r-cli/tests/suite/*.java        # minus SyncStatic.java
javac -d /tmp/shapes2 crates/j2r-cli/tests/suite/SyncStatic.java
# corpus (the assembled single-file corpus programs)
javac -d /tmp/corpus_jama corpus/JamaBench.java
cargo build -p j2r-cli
./target/debug/j2r readiness-report /tmp/shapes2/*.class        # §3 suite rows
./target/debug/j2r readiness-report /tmp/corpus_jama/*.class    # §3 JAMA rows
./target/debug/j2r cfg-report /tmp/corpus_jama/*.class          # per-method lift rows
# §4 dispatcher census + steady-state control run
javac -d /tmp/jb_classes corpus/JamaBench.java
./target/debug/j2r compile /tmp/jb_classes/*.class --main-class JamaBench \
  --crate-name j2r_jamabench_cfg --emit-cfg -o /tmp/jb_cfg
rg -c "match pc" /tmp/jb_cfg/src/classes/*.rs                   # 31 vs 106 default
cd /tmp/jb_cfg && cargo build --release --offline && time ./target/release/j2r_jamabench_cfg
```
