# G2.5 rung N=35000 — the measured wall record (2026-11-10, session 13)

Standing protocol: k=250, cargo -j 2, CARGO_INCREMENTAL=0, 550 s chunk
cap with fingerprint resume, proc_peak_sampler.py on every cargo chunk,
dev profile (CARGO_PROFILE_DEV_DEBUG=0), disk-guarded.

## What completed (measured)
- farm gen + javac + jar: javac 43.28 s / 1.20 GB peak; jar 43 MB.
- JVM parity capture (three channels): exit=0, stdout 82 B, stderr 0 B.
- COLD EMIT: 23.89 s, peak RSS 1.58 GB, sources 1.5 GB (k=250 → 141
  partition crates + support + app; manifest v2).
- CARGO COLD BUILD chunks 1–3: 45–50 crate units per 550 s chunk
  (dev profile is much faster per unit than the G2.4 release build);
  every rustc VmHWM ≤ ~1.0 GB (two units in parallel, -j 2) — the
  per-unit budget holds at 35k exactly as G2.1 designed it.
- Chunk 4: the last units compiled — **ALL 141 partition rlibs +
  support + j2r-runtime + j2r-jni-ffi present on disk** (deps 5.1 GB,
  census in rlib_census.txt) — and the FIRST LINK ATTEMPT ran:
  rust-lld peaked at **3,275,384 KB VmHWM (3.28/4.0 GB budget —
  INSIDE the budget; the G2.1b linear extrapolation 3.09 × 1.75 ≈
  5.4 GB was too pessimistic: lld scales sub-linearly)**, wrote
  262,900 KB of the binary, then rc=101 — **ENOSPC, the DISK wall,
  not the memory wall.**

## Why the link cannot complete on this box class (triple-measured)
Volume: 9.8 GiB total (df); fixed floor = system + tools + repo +
~/.local ≈ 3.6 GB; the 35k run needs co-resident: sources 1.5 GB +
deps 5.1 GB + binary ~1.9 GB (projected from the measured 1.08 GB
at 20k, binary scales with N) ≈ 8.5 GB > available ~6.2 GB.
1. Packing 1 (original): 0 bytes free at the link; partial binary
   263 MB; all 145 rlibs intact.
2. Packing 2 (partition sources deleted to make room): cargo refuses
   to even LOAD the workspace manifest ("failed to read p0/Cargo.toml")
   — the workspace structurally requires every member source present.
3. Packing 3 (fresh re-emit to restore sources): the emit itself
   ENOSPC'd twice (reemit.time, reemit2.time); at the best achievable
   packing the post-deps headroom is ≈ 0.1 GB vs ≈ 2.1 GB needed for
   the app crate + binary.

## Ladder verdict for this rung
- Emit stage at 35k: PASS (23.89 s, 1.58 GB — linear, inside budget).
- Compile stage at 35k: PASS (all 141 units, per-unit ≤ 1.0 GB).
- Link stage at 35k: **WALL — disk-bound on THIS box class** (lld
  itself measured inside the 4 GB budget). No parity claim, no
  binary claim. LADDER_PARITY deliberately absent.

## What this means for 70k (arithmetic from measured constants —
## NOT a claim)
Binary ≈ 1.9 GB × 2 ≈ 3.8 GB; deps ≈ 10.2 GB; sources ≈ 3.0 GB
≈ 17 GB > 9.8 GiB volume. The 70k rung requires a bigger volume and
(probably) a link-memory mitigation — the ladder records this as the
box-class boundary, not a compiler-design boundary (no emitted unit
exceeds 1.0 GB rustc; the G2.1 split holds at 35k).
