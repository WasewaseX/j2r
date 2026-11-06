# J2R — M2 Wave 4: CFG Reconstruction Investigation Report

**Status: investigation complete (M2_PLAN §6). The approved pc-dispatch
emission remains the default and is unchanged; the structured lift exists
only behind `--emit-cfg` (default OFF).**

Deliverables per M2_PLAN §6.2: this report (design, subset, statistics,
go/no-go), the prototype (`crates/j2r-codegen/src/cfg.rs`, 12 unit tests),
the `--emit-cfg` CLI flag (`compile`/`build`), the `j2r cfg-report`
subcommand (regenerates every number below), and the automated
output-equivalence test (`crates/j2r-cli/tests/cfg_equiv.rs`) which runs
the whole 39-fixture suite in both modes and requires byte-identical
stdout/stderr/exit status.

---

## 1. Design

The prototype lifts a method's pc-dispatch body into structured Rust
(`if`/`else`, labeled `loop`, labeled-block joins) when a conservative
analysis proves the lift exact, and reports `Unsupported(reason)`
otherwise — the emitter then silently uses the identical dispatch body.
Semantics are preserved by construction:

* The function preamble (FrameGuard, locals, unmarshal, operand stack,
  `thrown`, `'run` loop, pending-throw dispatch) is **byte-identical** to
  the dispatch emission; only the body shape differs.
* Non-branch instructions are emitted by the **same** `emit_instr`
  machinery the dispatch body uses — one instruction emitter, two body
  shapes.
* `j2r_try!` records the pending throw and `continue 'run`s; in a lifted
  body the loop top finds no handler (the supported subset has no
  exception table) and returns `Err(t)` — exactly the dispatch body's
  handler-free behaviour. (The macro's `continue` had to become labeled —
  see §4.)

### 1.1 Analysis

* **CFG construction** — leaders per the dispatch emitter's own rule
  (branch targets, conditional fall-throughs, post-terminator pcs);
  blocks end at terminators or leader boundaries; successors are remapped
  to block indices.
* **Reducibility** — retreating edges via iterative DFS; every retreating
  edge must target a block that dominates its source (iterative
  dominator computation). Any violation → `Irreducible` → fallback.
* **Loops** — natural loops per back edge, unioned per header; a loop is
  liftable only with a **single exit** and a **pure exit block** (every
  exit-block predecessor inside the loop). Body edges to the exit become
  `break 'loop_N;`; edges to the header become `continue 'loop_N;`.
* **Diamonds** — if/else over the conditional's two arms. Both arms are
  walked (pure inspection) to their meet; the meet must be dominated by
  the diamond (all its predecessors inside the two arms, or the
  diamond's own root — the root edge covers the empty-arm shape javac
  emits for `if (c) { } else { … }`). Non-trivial joins are emitted once
  behind a labeled block (`'join_N: { if __cond { arm } else { arm }
  break 'join_N; }` then the meet).

### 1.2 Unsupported shapes (exact fallback reasons)

| Reason | Meaning |
| --- | --- |
| `EmptyBody` | no instructions |
| `HasExceptionTable` | handler edges are not lifted |
| `HasSwitch` | `tableswitch`/`lookupswitch` not lifted |
| `Irreducible` | a retreating edge does not target a dominator |
| `MultiExitLoop` | a loop with more than one exit edge |
| `ImpureLoopExit` | the loop's exit block has an outside predecessor |
| `ImpureJoin` | a diamond meet with a third predecessor, or a nested diamond joining before the outer diamond |
| `BranchIntoEmittedCode` | the region walker would re-enter emitted code |
| `FallOffEnd` | a region falls off the instruction list (verifier rejects this upstream) |

Fallback is always **per method** and always silent — the dispatch body
is emitted instead, so `--emit-cfg` cannot change behaviour, only the
shape of bodies it successfully lifts.

---

## 2. Lift statistics (all 39 fixtures, 161 methods)

Regenerable with `j2r cfg-report <classes…>`; raw per-method table kept
with the evidence (M2_EVIDENCE.md).

| methods | lifted | fallback |
| --- | --- | --- |
| 161 | 127 (78.9 %) | 34 (21.1 %) |

| fallback reason | methods |
| --- | --- |
| `HasExceptionTable` | 15 |
| `Irreducible` | 13 |
| `HasSwitch` | 5 |
| `ImpureJoin` | 1 |

All fallbacks but one are in `main` methods (24 of 34) — the fixture
mains use try/catch and switch heavily because that is what the suite
tests. The `ImpureJoin` case is a nested diamond whose join precedes the
outer diamond's join (conservative refusal; the dispatch body is exact).

---

## 3. The output-equivalence gate (and what it caught)

`cfg_equiv.rs` compiles every suite fixture **twice** (default and
`--emit-cfg`) and requires byte-identical stdout, stderr and exit status.
It caught three real defects during development — the gate works:

1. **E0695 in lifted bodies** — `j2r_try!`'s unlabeled `continue` diverges
   through a labeled *block* (the join), which modern rustc rejects.
   Fix: the macro now takes the `'run` label as an explicit parameter
   (`macro_rules!` hygiene forbids referencing a call-site label from the
   macro body). Default emission is unchanged — a labeled continue to the
   same loop.
2. **Index-type confusion** — `walk_meet`'s nested-diamond recursion
   passed an *instruction* index where a *block* index was required
   (`len 7, index 9` panic on a fixture method). Fix: map through
   `block_of` before recursing; the shape itself is now a conservative
   `ImpureJoin` refusal, never a panic.
3. **Dropped last instruction** — a region block ending at a leader
   boundary (no terminator) skipped its final instruction (the
   `iconst_0` arm of `a != b` vanished; the operand stack then underflowed
   at the println). Fix: the fall-through arm emits the last instruction
   before advancing, matching the dispatch emitter. Three fixtures
   (`LongOps`, `Recursion`, `StringOps`) went from wrong to byte-identical.

After the fixes: **39/39 fixtures byte-identical in both modes**, unit
tests pin the regressions (`walk_meet_maps_instruction_indices_to_blocks`,
`empty_arm_diamond_lifts` with an instruction-completeness recorder).

---

## 4. Go/no-go recommendation

**NO-GO for making CFG emission the default in M2 — recommend re-evaluate
in M3 with per-method cost data.** Reasons:

1. **No measured benefit yet.** The dispatch `match` over basic blocks is
   compiled by rustc into a jump table; no benchmark in M2 shows the
   structured body is faster (M2_PLAN §6 scopes this wave as
   investigation, and Wave 3's checkpoint already showed shape-based
   wins require per-live-range analysis — 89 % of methods were
   kind-unstable). A default-emission change must be justified by a
   benchmark, and none exists.
2. **The conservative subset excludes exactly the hard code.** Methods
   with exception tables, switches, irreducible loops or nested joins —
   21 % here, and realistically much more in real library code (every
   synchronized/try block) — keep the dispatch body, so the default
   would be a hybrid with two body shapes per crate regardless.
3. **Risk asymmetry.** The dispatch body is the audited M1 architecture;
   the lift adds a second code path whose bugs (§3 caught three) are
   semantic, not syntactic. Keeping it opt-in preserves the audit while
   the investigation matures.
4. **The infrastructure is delivered and regenerable.** `j2r cfg-report`,
   the equivalence gate and the `Unsupported` inventory mean a future
   decision can be made with data: extend the subset (handler edges,
   switches), benchmark both shapes, then re-apply for review.

**Standing safety property delivered regardless of the go/no-go:**
`--emit-cfg` is provably output-equivalent to the default across the
whole suite (39/39, automated in CI as part of the differential gate
jobs' workspace test run), and default emission is byte-for-byte
unchanged versus Wave 3.

## Addendum 2026-11-07 — three lift defects found by the readiness census

The typed-register readiness wave (docs/TYPED_REGISTER_READINESS.md)
re-measured this pipeline on real corpus code and found that this
report's §2 census understated the lift's limits: **every loop whose
header is not the entry block was mis-flagged `Irreducible`** by an
immediate-dominator defect (seed = `idoms[pred]` instead of the
predecessor, intersect on raw indices instead of RPO) — this report's
13 `Irreducible` fallbacks were that bug, not real irreducibility; two
further builder gaps (fall-through into a fresh loop header emitted the
header inline → `ImpureJoin` on every javac `for`/`while`; the join
purity check refused a loop-header meet's own back edges → every nested
`for` refused) did the rest. All three fixed, all conservative-direction
(fallback bodies were exact at all times), cfg_equiv re-proven 107/107
both modes, codegen tests 67 → 83. The §2 numbers above are preserved as
the historical record of the wave as it ran; the corrected census and
the remaining named lift limits (handlers, `MultiExitLoop`, residual
`ImpureJoin`, `ImpureLoopExit`) live in
docs/TYPED_REGISTER_READINESS.md §3.
