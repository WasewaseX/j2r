# G2.1 — the app/Dispatch composite split (console.md F3)

Date: 2026-11-10, session 10. Checklist bar: REVIEW_CHECKLIST.md §G2.1.
Landing: emitter change in `crates/j2r-codegen/src/{emit.rs,partition.rs}`.

## Diagnosis (why the app bin was the 20k peak)

`emit_app` embedded TWO total-class-scaled literals into
`app/src/main.rs`, making `main_run` one fat function body — the same
disease as the JZlib mega-functions (fact 4), in dispatch-table form:

* `apply_defaults(&[(u32,u32,Df); RUNS])` — one tuple per run of
  same-kind static slots across ALL classes (run-merge only joins
  consecutive same-kind slots, so interleaved field kinds ≈ one tuple
  per class);
* `set_constants(&[(u32,Value); N])` — one tuple per `ConstantValue`
  static final across ALL classes.

Attribution cross-check with the committed census
(`scripts/m5_census_partitioned.csv`): at 1k/k=500 the peak (1.91 GB)
matches the 500-class partition-crate compile (census single-crate:
500 classes = 2.1 GB); at 20k/k=250 the partition crates bound at
~1.1–1.3 GB while the recorded peak was 3.49 GB in the app-bin compile
— the difference is the app bin's literals.

## The split

1. Per-partition `register_defaults` / `register_constants` fns in each
   `p{i}/src/lib.rs` (literals bounded by k; slot-sorted per partition;
   partition order == slot order, and `apply_defaults`/`set_constants`
   coverage is order-independent → identical pool state).
2. `app/src/main.rs` composes: one `p{i}::register_*(&mut v);` line per
   partition, then passes `&v` (slice) — O(partitions), never O(classes).
3. The three composite fns (`lookup`/`parents_of`/`meta_of`) in
   `app/src/lib.rs` now route through STATIC per-partition fn-pointer
   tables (`ROUTE_LOOKUP`/`ROUTE_PARENTS_OF`/`ROUTE_META_OF`) indexed by
   the existing `part_of` binary search — no per-partition match arms.

## 5k A/B measurement (same protocol as the committed census row:
N=5000, k=300, cargo -j 2, CARGO_INCREMENTAL=0, dev profile debug=0,
peak = getrusage(RUSAGE_CHILDREN) max single process, farm = m5_farm_gen)

| metric | BEFORE (census row, pre-split) | AFTER (this landing) |
| --- | --- | --- |
| cargo wall | 282.72 s | **242.75 s** (−14 %) |
| peak rustc RSS | 1,888,504 KB (1.89 GB) | **1,260,656 KB (1.26 GB)** (−33 %) |

Raw file: `farm5k_cargo_after.time` (`WALL_S PEAK_RSS_KB`).
Post-split peak = the largest PARTITION-crate compile (300 classes),
i.e. the bounded unit — the app bin no longer sets the peak.
Emitted unit sizes at 5k after the split: app/src/main.rs **4,973 B**
(34 register lines), app/src/lib.rs 4,021 B, p0/src/lib.rs 1,050,908 B
(per-partition bounded).

## Behavior preservation

* Three-channel parity at 5k: stdout/stderr/exit IDENTICAL vs
  Temurin 21.0.13+9 (raw stdout: `farm5k_native_stdout.txt` vs
  `farm5k_jvm_stdout.txt`).
* Partition differential (`cargo test -p j2r-cli --test partition_suite`,
  forced k=2): `partitioned_emission_matches_jvm` ok (25.26 s).
* Full battery on the landing tree: workspace remainder 449 passed /
  0 failed; `diff_suite` 2 passed, 478.60 s, zero divergences;
  `cfg_equiv` 1 passed, 356.03 s, zero divergences; gate-11 re-measured
  `fixtures=109 tests=452` ✓; gate-19 links ✓; gate-20 stubs ✓;
  fmt + clippy `-D warnings` ✓.

## 20k re-measurement (session 11, 2026-11-10) — the checkbox closer

Protocol = the 2026-11-09 baseline, exactly (m5_20k_confirm.sh
semantics: N=20000, k=250 → 81 part crates + support + app; cargo -j 2;
CARGO_INCREMENTAL=0; dev profile debug=0; peak =
getrusage(RUSAGE_CHILDREN) max single process). New tooling:
`scripts/g2_1_20k_remeasure.sh` (stage-based; sandbox lesson #9 forbids
background processes — one stage per tool call, cargo fingerprints
resume, the chunk counter survives kills) and
`scripts/proc_peak_sampler.py` (per-process VmHWM from
/proc/<pid>/status polled every 60 ms — rustc vs rust-lld attribution
that RUSAGE_CHILDREN cannot provide).

| metric | baseline (pre-split) | post-split (this re-measure) |
| --- | --- | --- |
| javac 20,003 classes | 26.15 s / 1.55 GB | 29.65 s / 0.91 GB |
| cold emit (manifest created) | 34.12 s / 858 MB | 34.30 s / 902 MB |
| cold cargo | ≈ 967 s (3 chunks, 1 kill) / 3.49 GB | ≈ 1,581 s (3 chunks, **2 kills**) / **3.09 GB = rust-lld**; worst single rustc **0.99 GB** |
| no-op (emit + cargo) | 38.55 + 0.28 = 38.8 s | 41.74 + 0.39 = 42.1 s |
| one-body-edit (C00042) | 27.16 + 28.16 = 55.3 s | 33.91 + 32.07 = 66.0 s (cargo peak 3.06 GB = lld relink) |
| parity | PASS | **PASS** (stdout+stderr+exit identical, 0 == 0) |
| binary | 1,084,126,264 B | 1,085,784,872 B (≈ 1.01 GiB) |

Wall honesty: the post-split cold cargo wall is NOT comparable to the
baseline — the sandbox reaped TWO chunks mid-compile (585 s + 595 s
lost; a killed rustc redoes its whole work on resume), vs ONE kill in
the baseline run. The completed resume chunk ran 401.37 s. Peaks are
unaffected by kills (VmHWM is a per-process high-water mark).

Attribution (`probe_resume.time`, per-process VmHWM top table):
**rust-lld 3,158,788 KB** — the final link of the ≈1.08 GiB debug=0
binary; EVERY rustc ≤ 1,005,860 KB (the partition crates, uniformly
bounded by k). The baseline label "3.49 GB (app-bin rustc)" conflated
rustc with its lld child (RUSAGE_CHILDREN rolls the child up); with the
split, the worst generated-unit rustc dropped 3.49 → ≈0.99 GB (−72 %)
and the residual build peak MOVED to the link stage.

Support-crate control: touch + `cargo build -p farm20000-support` =
0.83 s / 119 MB (support/src is 9.8 KB — runtime-filled tables; not a
compile unit that scales with total classes).

Registered follow-up: REVIEW_CHECKLIST **G2.1b — the link-stage wall**
(rust-lld 3.09 GB, still total-class-scaled via binary size; 3.09 <
4.0 GB, so G2.7 headroom = 0.91 GB on this box class).

Raw files here: `farm20k_remeasure.log` (full stage log),
`javac.time`, `build1.compile.time`, `build1.cargo.chunk2.time`
(chunk 1 was killed mid-compile — logged honestly, no .time file),
`build2.compile.time`, `build2.cargo.time`, `build3.compile.time`,
`build3.cargo.time`, `probe_support.time`, `probe_resume.time` (with
the per-process top table), `farm20k_native_stdout.txt`,
`farm20k_jvm_stdout.txt`.
