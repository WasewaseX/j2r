# M2 Wave 3 — 3a shape report and go/no-go decision (T19/T20)

Generated with `j2r shape-report` over all 39 differential-fixture classes
(161 methods) on 2026-11-01. The report is regenerable: `javac -d /tmp/shapes
crates/j2r-cli/tests/suite/*.java && j2r shape-report /tmp/shapes/*.class`.

## Aggregate decision table

| methods | specialized | fallback | stack positions (prim/total) | local slots (prim/total) |
| --- | --- | --- | --- | --- |
| 161 | 17 | 144 | 76/355 | 127/402 |

Per-method rows — the 17 specialized methods (decision `SPEC`, reason
`None`, stack prim/total, local prim/total):

| class | method | decision | reason | stack prim/total | local prim/total |
| --- | --- | --- | --- | --- | --- |
| Circle | <init> | SPEC | None | 1/3 | 1/3 |
| Config | bump | SPEC | None | 2/2 | 0/0 |
| Cycles$Node | <init> | SPEC | None | 1/2 | 1/2 |
| Exceptions | risky | SPEC | None | 2/2 | 2/2 |
| MutualRecursion | isEven | SPEC | None | 2/2 | 1/1 |
| MutualRecursion | isOdd | SPEC | None | 2/2 | 1/1 |
| Objects | <init> | SPEC | None | 2/3 | 0/1 |
| Objects | <clinit> | SPEC | None | 1/1 | 0/0 |
| Rect | <init> | SPEC | None | 1/3 | 2/5 |
| Recursion | isEven | SPEC | None | 2/2 | 1/1 |
| Recursion | isOdd | SPEC | None | 2/2 | 1/1 |
| Recursion | ack | SPEC | None | 4/4 | 2/2 |
| Square | <init> | SPEC | None | 1/3 | 1/3 |
| StackDepth | down | SPEC | None | 2/2 | 1/1 |
| SwitchFallthrough | small | SPEC | None | 1/1 | 1/1 |
| UncaughtSOE | down | SPEC | None | 2/2 | 1/1 |
| ZeroInit | <clinit> | SPEC | None | 1/1 | 0/0 |

The remaining 144 methods fall back (72 × `KindVaries`, 72 ×
`NoPrimitives`); the full table is reproducible with the command above —
the committed generator makes the report an artifact, not a claim.

## Reading the data

* **17/161 methods (10.6 %) take the specialized path** under the settled
  rule (M2_PLAN §5.1: every bottom-indexed stack position and every local
  slot kind-stable across all pcs where live; references never leave
  `Vec<Value>`).
* **72 methods fall back with `KindVaries`**: in typical compiled code the
  *bottom* operand-stack position carries an object at one program point
  (`aload`/`invokevirtual`) and an int at another (`bipush`/`iadd`) — the
  position is not kind-stable, so the whole method falls back. This is the
  conservatism the all-or-nothing rule buys: one frame shape per method,
  one GC scan surface, no partial mixes.
* **72 methods fall back with `NoPrimitives`**: constructors, accessors and
  object-graph plumbing — the fixture suite is deliberately
  semantics-heavy (identity, dispatch, exceptions), so reference-only
  methods dominate the count.
* Position-level view: 21.4 % of stack positions and 31.6 % of local slots
  are primitive-stable — the benefit is real but concentrated.

## 3a checkpoint decision (M2_PLAN §5.2) — recorded, not silent

The plan's own threshold for continuing to 3b (hybrid emission) was
*"if 3a's report shows the combined complexity is out of proportion (e.g.
half the methods fall back)"*. The measured fallback rate is **89 %** —
far past that line. Landing 3b would re-touch every instruction-emission
arm immediately after the GC wave, for a benefit confined to ~11 % of
methods in the suite (and an unmeasured — likely smaller — share of real
workloads, where object-graph code dominates even more than in fixtures).

**Decision: the wave ends at the 3a checkpoint, as §5.2 provides.**
Unspecialized emission is unchanged; M2 remains complete without 3b since
§3 (StackOverflowError) and §4 (the GC) carry the milestone's semantic
weight. The benchmark gate (≥ 1.3× or honest no-gain) is therefore
moot for M2 — recorded honestly rather than forced.

## What 3a delivered anyway (reusable infrastructure)

* `verify.rs` exports the verified per-pc shapes (`MethodShapes` /
  `PcShape`, stack **and** locals) — the exact input a future
  specialization pass needs, no longer discarded after validation.
* `specialize.rs`: the full decision pass (kind-stability, cat2-as-one-
  slot, all-or-nothing, reason codes) with 8 unit tests pinning every
  fallback class; `plan_program` + `render_report` + the `j2r shape-report`
  CLI subcommand make the decision *measurable at any time*, not a
  one-off.
* The 48-test codegen suite (40 pre-existing + 8 new) stays green, as do
  all other suites.

## Recommendation for a future milestone

The `KindVaries` fallback is dominated by bottom-indexed position reuse
across disjoint live ranges. A future pass could track per-position *live
ranges* (from the verifier's dataflow) instead of whole-method position
stability, and specialize per-range — a strictly larger analysis with the
same all-or-nothing per-method safety rule. With 21–32 % of positions
primitive-stable, the refined analysis plausibly pushes the specialized-
method share well past the 60 % acceptance line; it should be re-decided
with its own 3a report, not assumed.
