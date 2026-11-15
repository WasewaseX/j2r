# STATUS — G2 / G3 / G4: the single evidence-backed report

Requested by the owner's external review (2026-11-15): *have the agent
produce a single evidence-backed G2/G3/G4 status report, reconciling
the README and all status boards before starting another feature
wave.* This file is that report. Every claim below names the artifact
that re-proves it (the gate script, the evidence directory, or the
test to run). Written 2026-11-15 at commit `fb129b3` (fixtures=122,
tests=545, gate-11 in sync — `python3 scripts/check_doc_counts.py`).

**One-line verdict:** the architecture is scaling past the original
box (emission proven to 70k), correctness residuals are being closed
probed-first (two latent divergences found and fixed by the latest
probe alone), but 70k END-TO-END native compilation is not
demonstrated (a box-class disk boundary, precisely recorded), and
generated-code performance is still ~66× behind the tiered JVM —
attacked only through measured, controlled levers so far.

---

## G2 — the 70k-class scalability path (P0)

**Claim discipline:** emission is demonstrated to 70k; native
compilation is demonstrated to 20k end-to-end and 35k up to the link
stage. The checklist refuses to claim 70k and this report refuses
with it.

### What is proven, per rung

| Rung | Demonstrated | Measured | Evidence |
| --- | --- | --- | --- |
| 20k | FULL release build end-to-end | cold cargo ≈7,537 s; no-op 0.36 s (stat fast path); one-edit 100.88 s; release binary ≈1.01 GiB; worst single generated-unit rustc ≤ 0.99 GB (G2.1 split, −72%); release link peak 3.29/4.0 GB | `scripts/evidence/g2_4_release_trio/`, G2.1 split `scripts/evidence/g2_1_split/` |
| 35k | farm gen + javac + all 141 partition rlibs + support/runtime/jni-ffi; FIRST LINK ran | rustc units uniform ≤ ~1.0 GB; rust-lld peaked 3.28/4.0 GB (INSIDE budget — the linear extrapolation was too pessimistic), wrote 263 MB of the binary, then **rc=101 ENOSPC — the DISK wall, not the memory wall**; triple-measured packing (best leaves ≈0.1 GB vs ≈2.1 GB needed) | `scripts/evidence/g2_5_35000/` (WALL_RECORD.md) |
| 50k | emission + JVM channel capture | emit 36.35 s / 2.23 GB | `scripts/evidence/g2_5_50000/` |
| 70k | emission + JVM channel capture (Minecraft scale) | emit 53.14 s / peak RSS 3.16 GB / 3.0 GB sources — INSIDE the 4 GB budget; javac needs `-J-Xmx3g` (74.32 s / 2.95 GB); the 70,003-entry jar is ZIP64 — reader fixed this wave | `scripts/evidence/g2_5_70000/` |

### The honest boundary

The 70k target is NOT claimed. The bar requires end-to-end completion
on this box class; the volume arithmetic from measured constants:
binary ≈3.8 GB + deps ≈10.2 GB + sources ≈3.0 GB ≈ 17 GB > 9.8 GiB
volume. This is recorded as a **box-class boundary, NOT a
compiler-design boundary** — no emitted unit exceeds 1.0 GB of rustc,
the G2.1 split holds at 35k, and the link stage measured sub-linear
(3.28 GB at 35k, inside 4.0). The next rung needs a >9.8 GiB volume
box (`G2.5-ladder-continuation`, registered, unchecked). Until that
rung runs, "emission demonstrated" must not be abbreviated to
"compiles".

### Closed sub-gates (evidence-linked in REVIEW_CHECKLIST)

G2.1 (dispatch split, −72% worst unit) · G2.1b (link-wall attribution
accepted with the measured number + the per-process sampler) · G2.2
(no-op 668×) · G2.3 (emission profile) · G2.4 (release trio) · G2.5
(honest ladder) · G2.6 (fat-method regression) · G2.7 (memory-budget
framing). Open: `G2.5-ladder-continuation` (the bigger box).

---

## G3 — typed-codegen performance (P1)

**The number stands: ~66× behind the fully tiered JVM on JamaBench,
~73–104× on StringsSimBench** (the honesty guard G3.5 forbids
"approaching JVM" phrasing until a measured order-of-magnitude
change; the figure is quoted every time).

### The lever ledger (each entry a same-session controlled pair, G0.7)

structuring-flat → typed-locals 0.992 → operand unification 0.958 →
typed-payload arrays 0.936 (6.4% cumulative so far). The R5/R7
dispatch work is measured too: direct-call folding + codegen-time
vtable resolution eliminated 18 of 56 dynamic sites (−32.1%); the R7
hash-dispatch selector was a **measured NO-GO** — rustc 1.99 already
lowers the string-tuple match to a balanced tree (~27 ns/call vs
~216 ns for both proposed selector shapes at N=2,169) — kept runnable
behind `--emit-hash-dispatch`, default OFF, default path proven
byte-identical via a pre-R7 worktree diff. The typed-storage
specialization and the default-path CFG emitter are the earlier
measured NO-GOs (docs/M2_SHAPE_REPORT.md, docs/M2_CFG_REPORT.md).

### Sub-gate status

- **G3.1 deferred joins** (tred2/orthes/hqr2 refusals) — OPEN; the
  registered next CFG layer, PASS shape = lands behind `--emit-cfg`,
  the three kernels lift, cfg_equiv stays zero, controlled pair
  recorded either way.
- **G3.2 block-join / entry-var layer** (~140 slot-var sites) — OPEN;
  the named prerequisite for G3.3's full payoff.
- **G3.3 primitive-specialized collection paths** — SLICES LANDED
  (the measured 2.78× container tax attack): slice 2 (put/get rewrites
  + the closure law), slice 3 (class-level provenance scan, node-stamped
  class tag, lazy box materialization, `hm_get_prim` as the fused
  checkcast+unbox law); the specialized rewrites now also ride the two
  NEW map storages (LinkedHashMap landed 2026-11-15; TreeMap is the
  next queue item — the classes did not exist and were probed as loud
  unsupported before their waves started).
- **G3.4 the acceptance bar** — unchanged: repeatable (≥2 sessions)
  gains on BOTH benchmarks, byte-identical output, zero diff-suite
  regressions, every number with its same-session control pair.
- **G3.5 honesty guard** — enforced in this document by quotation.

### What would actually move the number

The interpreter-shaped execution model (`match pc` / `Vec<Value>`) is
the fundamental cost; G3.1+G3.2 (deferred joins, block-join/entry
vars) are the registered path into it. No optimization rewrite starts
without its controlled before/after pair — three NO-GOs so far prove
the discipline cuts both ways.

---

## G4 — concurrency review + the JMM boundary (P1)

**State: all four M4 waves implemented and CI-verified; the OWNER
review is still PENDING (G4.1 unchecked).** Since session 13 the
review has enforcement evidence to examine:

- **G4.2 CLOSED via arm (b)** — the boundary is enforced, not merely
  documented: every generated access to a volatile field (and every
  non-folded final) carries a `J2R-JMM-BOUNDARY:` marker at the use
  site; every emitted crate gains a deterministic `src/jmm_boundary.rs`
  sidecar; CI gate-23 re-derives the expected set from the INPUT
  `.class` flags and fails loudly on sidecar divergence, a missing
  marker, or a marker naming a non-boundary field; tamper-tested.
- **Arm (a) — real volatile / final-freeze JMM semantics — is FUTURE
  work and must be described as such.** The boundary is identified
  and reported; its Java memory-model BEHAVIOR is not implemented.
  No document may imply otherwise.
- **G4.1 next action:** the owner (or a delegated reviewer) records a
  verdict — approve / approve-with-follow-ups / request-changes — in
  the boards, reviewing plan + 4 waves + the new enforcement
  evidence (docs/M4_PLAN.md, docs/M4_EVIDENCE.md §completion package,
  gate-23 output).

---

## Documentation reconciliation (the review's §5, verified at HEAD)

The review compared an EARLIER public snapshot (its drift table cites
109 fixtures / 241 tests / a Nov-10 board). The reconciliation at the
current HEAD (`fb129b3`, 2026-11-15) is:

| Document | Review saw | State at HEAD | Verdict |
| --- | --- | --- | --- |
| README.md | 109 fixtures / 241 tests; 20k milestone | **122 fixtures / 545 tests; the scale ladder + 35k ENOSPC + 70k emission recorded** (this report's numbers) | in sync |
| TODO.md | last updated Nov 10 | **updated 2026-11-15** (LinkedHashMap landing + the residual wave + this report's queue) | in sync |
| console.md / console_summary.md | Nov 15 work | Nov 15 work, including the run-4 continuation entry | in sync |
| REVIEW_CHECKLIST | G2/G3 results | all G2 sub-gates evidence-linked; G3/G4 states current (G3.3 slices recorded) | in sync |
| ROADMAP.md | older narrative | has M3.7/M4/M5 but **stops mid-M5** — no waves-2..5 narrative, no G-gate register pointer | DRIFT (minor, registered below) |

Remaining doc follow-up (registered, G6.1): a ROADMAP refresh
carrying the M5 waves 2–5 narrative + a pointer to the G-gate register
and this report. Everything the review asked to reconcile is
machine-checked at HEAD except that one narrative refresh.

## The correctness residuals, explicit (not hidden by test counts)

1. **HashMap hash-iterator `remove()`** — CLOSED 2026-11-15 (`fb129b3`):
   the ArrayList-shaped rows (the slot-2 type confusion, latent since
   the stringsim wave) replaced by `n_hi_remove`; probe FIRST found a
   SECOND latent divergence — the LinkedHashMap link-iterator remove
   checked CME before ISE, Temurin is ISE-first when both conditions
   hold (probed, cases 1/5) — both fixed, fixture ItrOrderProbe (the
   122nd) + 3 law tests, three-channel zero.
2. **User-interface DEFAULT-method dispatch** (Probe5) — OPEN.
3. **HELPFUL-NPE** — OPEN (registered in the boards).
4. **Different-package private-constructor access** — OPEN.
5. **TreeMap** — the next queue item (the class does not exist yet;
   opens with the class: red-black storage + the Comparable/Comparator
   dispatch law, probed FIRST).

## Verification (re-run, don't trust)

```console
$ python3 scripts/check_doc_counts.py        # fixtures=122 tests=545
$ cargo test --workspace --lib               # 486 passed, 0 failed
$ J2R_DIFF_SHARD=0/2 cargo test -p j2r-cli --test diff_suite
$ J2R_DIFF_SHARD=1/2 cargo test -p j2r-cli --test diff_suite   # 122 fixtures, zero divergences
$ cargo test -p j2r-cli --test cfg_equiv     # zero divergences
$ cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings
```
