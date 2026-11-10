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

## Still owed for the G2.1 checkbox (honest gap)

The checklist bar says "the **20k** peak-rustc number is re-measured
and drops materially". The 5k A/B above demonstrates the mechanism with
a committed control row; the 20k re-measurement (farm + emit + watched
cargo run, raw files here) is the next chunk and G2.1 stays `[~]` until
it lands. Expected: peak ≈ the 250-class partition-crate compile
(~1.1–1.3 GB) vs the recorded 3.49 GB.
