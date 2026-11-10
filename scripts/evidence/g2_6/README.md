# G2.6 — the fat-method regression closed via the array-literal peephole
# (arm a: split-table / smaller-function emission, byte-identical)

Date: 2026-11-10, session 11. Checklist bar: REVIEW_CHECKLIST.md §G2.6.
Landing: `crates/j2r-codegen/src/peephole.rs` (new IR pass wired into
`lib.rs` compile step 4) + `Instr::NewIntArrayFilled{prim,vals}` (ir.rs)
+ emit/verify/typed coverage + `array_int_group_filled` (j2r-runtime
array.rs). Census tool: `scripts/emitted_fn_census.py`.

## The disease, measured

docs/BENCHMARKS.md §2026-11-06: the JZlib generated crate's release
build SIGKILLed rustc at every opt level (3.03–3.56 GB, borrow-check /
MIR phase) on this 4 GB box — machine-bound, never closed. Partitioning
bounds CRATE size but cannot split ONE enormous method body.

Reproduction with the per-process sampler on 2026-11-10 (box: 4,041 MB
total, ~3.16 GB available), `cargo build --release` opt-level 3:

* `jzlib_release_before_sigkill.time`: rc=101, **rustc VmHWM
  3,253,300 KB (3.25 GB), SIGKILL** at 23.18 s — the single
  `jzlibcorpus` lib rustc; every other process ≤ 336 MB.

The census (`census_before.txt`) attributes the MIR blowup: the three
`<clinit>` static initializers are the monsters — javac's array-literal
initializers expand to straight-line per-element fills:

| fn (pre-pass) | emitted size | shape |
| --- | --- | --- |
| InfTree `<clinit>` | 1,038,405 B / 12,354 lines | 1536-element `[I` fill + others |
| Tree `<clinit>` | 550,507 B / 6,562 lines | mixed `[I`/`[S`/`[B` fills |
| StaticTree `<clinit>` | 382,755 B / 4,562 lines | `[S`/`[I` fills |

(the giant switch-table methods — InfBlocks::proc 217 KB etc. — are a
SECOND, smaller shape; they did not block the release build once the
clinit fills collapsed.)

## The fix (arm a — the emission fix, exactly as the bar names it)

`peephole.rs::collapse_array_fill_runs` recognizes javac's exact
array-literal fill shape over int-group prims (Boolean/Char/Byte/Short/
Int — all `PrimData::I` payloads):

```text
const(len); NewArray(p); ( Dup; const(idx); const(val); AStore(IntGroup) )*
```

with idx ascending from 0 and exactly len groups, and replaces the run
with ONE `NewIntArrayFilled{prim, vals}` instruction. Emission = a
nested `static [i32; N]` (a separate const-eval item — the enclosing
fn's MIR sees one call, not N aggregate operands) + one
`array_int_group_filled(desc, &STATIC)` call whose per-prim narrowing
is exactly `array_store_i_raw`'s table → byte-identical payload.

Byte-identical by construction (peephole.rs header): identical stack
effect; a collapsed fill cannot throw (constant len ≥ 8 > 0, constant
in-range indices, primitive stores — NPE/AIOOBE/ASE/NASE unreachable),
so dropped fault points change nothing; GC safepoints inside the run
become one-per-instruction (collection timing delays, never moves);
every refused shape (interleaved instrs, non-const values, out-of-order
indices, length ≠ groups, branch/handler target inside the span,
non-int-group prim) keeps the old per-element form. Guarded by 8 unit
tests in peephole.rs (110 total in j2r-codegen, all green).

## Results

Emission: InfTree.rs 1.2 MB → **124 KB** (−90 %); StaticTree.rs
387 KB → **16 KB** (−96 %); all three clinit giants leave the census
top-10 (`census_after.txt`).

| release build of the JZlib crate | BEFORE | AFTER |
| --- | --- | --- |
| result | **SIGKILL** (rustc 3.25 GB, MIR phase) | **rc=0** |
| wall | dead at 23.18 s | **50.34 s** |
| max single process | 3,253,300 KB | **678,312 KB (−79 %)** |

Raw: `jzlib_release_after.time` (with the per-process top table),
`jzlib_debug_after.time` (debug build 7.91 s / 464 MB).

## Byte-identical proof

The optimized release binary vs Temurin 21.0.13+9 (the full JZlib
differential driver: deflate/inflate levels 0–9, Adler32, CRC32,
version string, wrap-deflate):

* exit codes 0 == 0; **stdout and stderr IDENTICAL** —
  `jzlib_native_release_stdout.txt` vs `jzlib_jvm_stdout.txt`.

Battery on the landing tree (all green, zero divergences):
j2r-codegen 110 (incl. 8 peephole) ✓; partition differential k=2
28.29 s ✓; corpus_suite (all 4 real-world corpora) 34.22 s ✓;
diff_suite 109 fixtures 511.39 s ✓; cfg_equiv 376.57 s ✓;
j2r-runtime 243 ✓; fmt + clippy `-D warnings` ✓.

## Honest scope note

The peephole closes the array-literal clinit shape — the shape that
measured as the release-blocker (3.25 GB SIGKILL → 678 MB, −79 %).
The giant switch-table methods remain a second, smaller fat-function
shape (InfBlocks::proc 217 KB emitted; they compile fine at opt 3 and
did not threaten the 4 GB budget in any measured build). If a future
corpus regresses past a budget, the registered ceiling probe is the
per-process sampler + `emitted_fn_census.py` over the crate — the same
tooling that caught this one.
