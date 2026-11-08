# M5_SCALE — the census-driven scale design note (T02)

Date: 2026-11-08. Evidence: TODO.md §M5 T01 (scripts/m5_census.csv),
docs/M5_PLAN.md §1–§2.

## 1. What the census measured

| N | javac (s / MB) | j2r compile (s / MB) | cargo debug (s / MB) |
| --- | --- | --- | --- |
| 100 | 1.83 / 105 | 0.06 / 13 | 11.99 / 655 |
| 250 | 2.31 / 119 | 0.09 / 18 | 26.29 / 1256 |
| 500 | 3.71 / 162 | 0.22 / 28 | 45.55 / 2106 |
| 1000 | 4.67 / 188 | 0.40 / 49 | **SIGKILL** (3.66 GB, 48 s) |

Isolations: stubbing the 639 KB `farm_Main` mega-function out of the
1k-class crate still SIGKILLs at 3.64 GB (the mega-function is not the
driver); metadata-only rustc on the full crate OOMs without ever reaching
LLVM (the wall is the pre-codegen MIR/metadata phase).

**The cliff, with numbers**: rustc peak memory is linear at
**~4.2 MB per emitted class** (~400–500 MB process baseline + ~3 MB per
class). One rustc process on this 4 GB box caps out near **~900 classes**.
The registered 70k-class target would need **~300 GB** in a single rustc
process. j2r's own translation is flat (0.4 ms + 48 KB per class) and
javac is flat (0.5 MB/class) — the emitted-code build is the only scale
wall, confirming M5_PLAN fact 4 with a measured curve.

## 2. Options considered

(a) **One crate, tuned down** (`-C incremental=off`, debuginfo=0, fewer
    MIR opts). Measured: incremental off + debuginfo 0 still dies at
    3.68 GB on 1k classes. Buys a constant factor at best; 100× needed.
    Rejected.

(b) **Layered crate partitioning by the class reference DAG.** Cycles
    inside a layer must stay in one crate, and Rust crates cannot be
    cyclic. The farm's reference web (mod-N back-edges) is ONE strongly
    connected component — collapses to a single crate, no win. Real
    javac output has near-DAG package structure, but the giant-SCC case
    is exactly the Minecraft case (one big connected mod graph).
    Rejected as the primary mechanism.

(c) **Link-table partitioning (chosen).** Partition classes into
    fixed-size deterministic chunks (sorted class list, K per crate —
    v1: K = 500, the measured 2.1 GB point). Cross-chunk class calls go
    through a **link table**: a per-partition `static [fn; N]`-style
    registration the runtime already knows how to fill (the same
    pattern as the registry and the planned JNI `RegisterNatives`);
    same-chunk calls stay direct paths. Memory per rustc process is
    bounded by chunk size, cargo parallelizes independent crates, and
    cycles stop mattering because nothing references across crates by
    path. Cost: one indirect call at chunk boundaries — the same cost
    class as vtable dispatch the runtime already pays, and T23's inline
    caches can cache hot cross-chunk targets later.

(d) **Emit thinner code per class** (smaller dispatch bodies). Real but
    secondary: it lowers the per-class constant, it does not bound the
    process. Runs AFTER (c) as a lever on chunk size (T05 follow-up).

## 3. Decided order (changes M5_PLAN §2.1–§2.3 execution order only)

1. **T05 (re-aimed): crate-partitioned emission.** `emit_crate` learns
   `--partition-classes K`: a workspace with `classes-<i>` crates
   (K classes each), a `link` crate holding the cross-partition link
   table, `statics`/`registry`/`main` as today. Acceptance bar: farm
   N=1000 builds end-to-end under 4 GB; N=5000 builds; the curve is
   re-measured and the CSV updated.
2. **T03 closure cache** (the quadratic worklist, codegen lib.rs:492–580)
   — real but behind the rustc wall (0.40 s at 1k vs cargo death); lands
   after T05 so the 20k census exercises both.
3. **T06/T07 build manifest** rides the partitioning (per-chunk input
   hashes; unchanged chunks are cargo no-ops).

Risk register: the link table must preserve the exact vtable/override
semantics (direct `crate::classes::X::m_*` calls bypass the runtime's
dispatch — the table entry IS the same fn pointer, so semantics are
identical; proven by the differential battery on the farm + the 109-fixture
suite); determinism of chunk boundaries (sorted class list, no HashMap
iteration); `farm differential` (T04) re-pins behavior end-to-end.
