# External port validation — jbox2d 2.2.1.1 (owner-run, 2026-11-12)

Provenance: an independent port test executed by the owner outside this
repo, on a different box (2 vCPU Xeon / 2 GB RAM), against
`WasewaseX/j2r @ c38fcdb` (main, 2026-11-11) plus ~1,100 lines of
"port wave 1" patches the owner wrote to carry the port (formatter fix,
partition registry hook, fdlibm module, nine natives-surface rows). The
three documents below are the owner's report, persisted here essentially
verbatim (light markdown normalization only) so they survive sandbox
resets — the authoritative copies live in the repo, not in any chat
scrollback.

- `jbox2d_result.md` — the benchmark: 6/6 workloads byte-identical to
  Temurin 21 through 1,980 chaotic physics steps; steady-state
  6.9x-84x slower than the JVM (median 25x); 1.74x-4.80x lower peak
  RSS; 26x faster startup; translate 156 ms vs javac 969 ms, then
  111.8 s of cargo.
- `jbox2d_bug.md` — B1-B17: two real j2r bugs found by the port
  (B1 formatter %f destroys magnitudes < 1e-3; B2 partitioned emission
  wires exception handlers to a nonexistent registry module), one
  silent-divergence trap (B3 Math sin/cos vs platform libm), nine loud
  coverage gaps (B4-B12), two scale blockers (B13-B14), owner process
  notes (B15-B16), open items O1-O6.
- `jbox2d_opt.md` — the measured cost ledger for the DEFAULT emission
  backend on a real program, six structural cost centers, ranked
  recommendations R1-R9, and the process suggestions (surface probing,
  collect-all-diagnostics, partition fixture law).

## Why this file set matters to the roadmap

1. **Independent corroboration of the lever ledger.** The owner's
   default-backend numbers (median 25x, gap growing with hot-loop time)
   land in the same regime as our JamaBench measurements, and Opt.md's
   micro-benchmarks (typed u64 slots 0.60 ns vs Vec<Value> 3.05 ns =
   5.1x) independently corroborate the typed-register track's premise.
   R3 (refresh only on reference change) is the same lever our
   ref-only refresh projection (0.913) moved first, with the same
   safety protocol demand (forced-GC stress).
2. **Two real bugs exist in this repo too.** B1 and B2 were found at
   c38fcdb and the underlying code is unchanged on main: B1 lives in
   `crates/j2r-runtime/src/formatter.rs` (render_f), B2 in
   `crates/j2r-codegen/src/partition.rs` (rewrite_class_source has no
   `crate::registry::` arm). Both carry concrete fix specs in
   jbox2d_bug.md. Landing them + the O4/O5 fixtures are registered
   waves in TODO.md §"EXTERNAL PORT (jbox2d 2026-11-12)".
3. **The natives surface is measured against a real game-shaped
   program.** B4-B12 name the exact rows jbox2d needed (boxed
   constructors, Random, ArrayList.toArray, Arrays.sort ranges, IEEE
   bit-reinterpretation statics, StrictMath hierarchy, AssertionError,
   NoSuchFieldError parentage, transcendental surface).
4. **The deferral audits were right.** The port needed NO array
   descriptors of depth >= 5 and NO reflection-metadata lookups beyond
   Class.forName-shaped resolution — corroborating both prior deferral
   decisions (and the depth-5 closure + R2 slice 1 have since landed on
   main: e6ace40, ac4af2d).

Caveat: the owner's patches are NOT in this repo — the reports describe
them, the code lives in the owner's sandbox. Every "land" wave below is
a reimplementation from the written spec, verified by our own gates.
