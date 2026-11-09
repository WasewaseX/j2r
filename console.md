# j2r console — the live log

**Owner directive (2026-11-09):** every session writes everything that
happens into this file, like a console. `console_summary.md` is the
distilled state board; this file is the raw append-only log. **Read both
files before acting on anything** — they carry the authoritative state,
the correct milestone names, and the never-repeat mistake list. If this
file disagrees with memory, this file wins.

---

## STATE AT A GLANCE (kept true by every session)

- **Project:** j2r — Java → Rust compiler, zero `unsafe`. Repo:
  `github.com/WasewaseX/j2r` (authoritative). Local working tree:
  `/home/z/my-project/work/j2r-fresh` (reset to `origin/main` after
  sandbox reset #2; `work/j2r-gh` was lost in that reset — the tree name
  does not matter, `HEAD == origin/main` does).
- **Milestone ladder — CORRECT NAMES, do not renumber:**
  M1 core compiler ✅ · M1.5 hardening ✅ · M2 generated-code quality ✅ ·
  M3 JDK coverage ✅ · M3.5 audit response ✅ · M3.6 strategic gaps ✅ ·
  M3.7 corpus wave (in progress: minimal-json, JZlib, java-string-similarity,
  Jama waves) · M4 threads & concurrency (all waves landed, **owner review
  pending**) · **M5 = Minecraft readiness (STARTED 2026-11-08)** ·
  M6 = mod ecosystem (future).
  ⚠ M5 is Minecraft readiness, NOT M4. An old ROADMAP draft said "M4" —
  superseded; see mistake #1.
- **M5 wave 1 (scale & incremental): CLOSED 2026-11-09.** T01 census ✅ ·
  T02 design note ✅ · T03 closure cache ✅ · T04 partition differential ✅ ·
  T05 crate-partitioned emission ✅ · T06/T07 build manifest ✅ (owner
  review: **APPROVE WITH FOLLOW-UP REQUIREMENTS** 2026-11-09; follow-up =
  large-scale-compilation evidence — delivered by the 20k run) ·
  **20k census-jar confirmation LANDED** (cold ≈ 17.1 min, no-op 38.8 s,
  one-edit 55.3 s, parity PASS; findings F1–F5 in the log below).
- **M5 waves 2–5 (next):** T08–T13 JNI bridge (gate satisfied — T08
  preconditions checklist in console_summary.md §3.1) · T14–T16 mappings ·
  T17–T21 mixins · T22–T23 inline caches.
- **Toolchain:** `source /home/z/my-project/env.sh` → rustc/cargo 1.99.1,
  Temurin javac 21.0.13 (the pinned CI versions). Sandbox: 2 vCPU,
  4 GiB RAM, no swap. Persistent state lives under `/home/z/my-project/`;
  `/tmp` does NOT survive sandbox recycles (see mistake #4).
- **The rustc cliff (measured, T01):** one rustc process burns ~4.2 MB peak
  RSS per emitted class → ~900-class ceiling on 4 GiB → 70k classes ≈
  ~300 GB in ONE process. Fix landed (T05): `--partition-classes K`
  link-table partitioning; 1k/5k farms build at ~1.9 GB peak,
  byte-identical vs Temurin.
- **Compile-time memory ≠ download size:** 70k classes ≈ ~3.2 GiB of
  emitted Rust source (48 KB/class) and a smaller binary; the hundreds-of-GB
  figures are transient single-process build memory, not distribution size.

---

## LOG (append-only, newest at the bottom)

### [2026-11-08, previous sessions — backfill from repo evidence]
- M5 plan rev 0 published (docs/M5_PLAN.md); census T01 landed
  (scripts/m5_census.csv): cliff named with numbers (above); T02 design
  note landed (docs/M5_SCALE.md): link-table crate partitioning chosen,
  DAG/SCC layering rejected with the measured reason (the farm reference
  web is ONE strongly connected component — the Minecraft case).
- T05 partitioned emission landed: farm 1k k=500 -j1 → 101.99 s / 1.91 GB
  peak (single crate SIGKILLed at 3.66 GB on the same input); 5k k=300 -j2
  → build 282.72 s / 1.89 GB peak; runs BYTE-IDENTICAL vs Temurin;
  one deadlock (link-table Mutex) found and fixed (now OnceLock).
- T03 closure cache landed: compile stage 0.40→0.29 s at 1k,
  7.98→2.02 s at 5k (−75 %, near-linear now).
- T04 partition differential landed: 6 fixtures × forced k=2 CI-pinned;
  farm byte-identical at 250/1000/5000.
- T06/T07 build manifest landed (commit 6f79a10): j2r-manifest.tsv,
  SHA-256 input→emitted hashes, global gate (emitter version, options,
  signature surface, partition layout), write-if-changed everywhere,
  corrupt/tampered manifest falls back to full re-emit. Measured
  (1,003-class farm, k=200, release): cold 357.5 s → no-op **0.535 s**
  (668×) → one-body-edit **71.0 s** (5×), all four channels byte-identical
  vs Temurin. 10 manifest integration tests + 3 SHA-256 unit tests.

### [2026-11-09 ~07:30 UTC] session start — state recovery
- Sandbox recycled between sessions; recovered the previous session's
  state from `/tmp/my-project` into `/home/z/my-project` (rsync -u):
  download reports, T07 measurement scripts, m5invest experiment dirs,
  t07-fixtures. Fixed `env.sh` to point at the persistent
  `/home/z/my-project/tools` (the old copy pointed at `/tmp`).
- Freed 772 MB of stale build artifacts (j2r-fresh/j2r-audit/j2r-wt
  target dirs — rebuildable), 6.6 GB free.
- Cloned the authoritative repo to `work/j2r-gh` (HEAD 6f79a10,
  "feat(m5-t06/t07): build manifest"). Note: the local trees `work/j2r`,
  `work/j2r-wt`, `work/j2r-fresh` are STALE snapshots; `work/j2r-gh` is
  the only current tree.
- Owner correction registered: **M5 = Minecraft readiness** (checked
  github ROADMAP.md + TODO.md today). Console files created per owner
  directive (this file + console_summary.md).
- Owner pasted the external-tool v1.2 Minicraft test (results/bugs/opt,
  M01–M20 catalog, 1,438 raw errors, silent-defect classes). Analysis
  written to `docs/EXTERNAL_V12_MINICRAFT_ANALYSIS.md` (repo) — digest in
  console_summary.md §6. Decision taken: Minicraft becomes corpus-wave
  candidate (real 76-class web, deep Entity→Mob hierarchies, instanceof-
  heavy, statics — exactly the "realistic workload" shapes the owner
  demanded for scale benchmarks).
- **Launched the last open wave-1 item in background:**
  `scripts/m5_20k_confirm.sh` — N=20000, k=250, debug, cargo -j1,
  CARGO_INCREMENTAL=0; records cold compile+cargo wall/peak-RSS, no-op
  rebuild, one-body-edit rebuild, three-channel parity vs Temurin.
  Log: `work/m5invest/farm20k/confirm.log`. (Disk guard: no release leg
  on this 6.6 GB box; release-20k trio stays registered follow-up.)
- **Sandbox lesson (never-repeat #9): background processes are reaped
  when a tool call ends** — both `nohup` and `setsid nohup … & disown`
  died between calls. Long builds must run as foreground calls ≤10 min,
  resuming via cargo fingerprints (cargo resumes cleanly; wall time is
  summed across chunks and recorded honestly).
- Execution hiccups recorded for honesty: first jar packed the SOURCE
  tree (farm/*.java) not the classes → j2r "main class not found";
  fixed by using the classes dir as classpath throughout. First cargo
  pass (default debuginfo) filled the disk (100%, ld SIGBUS at the
  final link) → rebuilt with `CARGO_PROFILE_DEV_DEBUG=0` (deviation
  recorded) and freed caches + stale artifacts.

### [2026-11-09 08:34–09:25 UTC] 20k census-jar confirmation — COMPLETE, wave 1 CLOSED

Run: `work/m5invest/farm20k/` (N=20000, k=250 → 80 part crates + support
+ app; cargo `-j 2`; `CARGO_INCREMENTAL=0`; profile dev with
`debug=0` — disk-constrained deviation). Raw time/peak files:
`work/m5invest/farm20k/*.time` (format WALL_S PEAK_RSS_KB, semantics =
getrusage max single child, mistake-list #2).

| stage | wall | peak RSS (single largest process) |
| --- | ---: | ---: |
| javac 20,003 classes | 26.15 s | 1.55 GB |
| j2r emit (cold, manifest created) | 34.12 s | 858 MB |
| cargo debug=0 (3 chunks: ~578s killed + 388.81 s + 0.47 s link) | ≈ 967 s | **3.49 GB** (app-bin rustc) |
| **cold end-to-end** | **≈ 17.1 min** | — |
| no-op rebuild: emit + cargo | 38.55 s + 0.28 s = **38.8 s** | 858 MB + 37 MB |
| one-body-edit rebuild (C00042): emit + cargo | 27.16 s + 28.16 s = **55.3 s** | — |
| run: native binary vs Temurin | 0.11 s vs 0.63 s | 252 MB vs 61 MB |

- **Manifest correctness at 20k: no-op = 20,003 reused / 0 written;
  one-body-edit = 20,002 reused / 1 re-emitted / 1 written** — exactly
  the T06/T07 soundness contract, now demonstrated 20× beyond the 1k
  farm evidence.
- **PARITY: PASS — stdout (84 B) + stderr + exit identical, 0 == 0**,
  native binary vs Temurin 21.0.13. Binary size 1,084,126,264 B
  (≈ 1.01 GiB, debug=0).
- Acceptance bars (M5_PLAN §2): end-to-end at 20k ✅ recorded; no-change
  rebuild measurably cheaper ✅ (≈ 26× at this scale); parity ✅. The
  "super-linear nowhere" bar has ONE honest hit, in the EMIT stage, not
  cargo: 5k 2.02 s → 20k 34.12 s (per-class 0.40 ms → 1.71 ms) — F1 below.
- New findings registered (each a follow-up, none blocking wave 2):
  - **F1** emit-stage super-linearity at 20k (closure cache fixed the
    worklist; something else grows — support-crate table build and/or
    per-file hashing; needs a profile).
  - **F2** no-op emit cost is SHA-256-rehash-bound (~900 MB of emitted
    sources → 38.55 s; ≈ 22 MB/s). An (mtime, size) fast path before
    hashing makes no-ops sub-second and restores the big ratios.
  - **F3** the app/Dispatch composite is the new rustc near-wall at 20k
    (3.49 GB peak, < 0.5 GB under this box's ceiling) — the same disease
    as fact 4's mega-functions, now in dispatch-table form. Fix is the
    already-registered split-table emission: per-partition route tables
    + GID-range binary search, never one giant match.
  - **F4** release-profile 20k trio still registered (disk: the
    debuginfo build filled the box; debug=0 binary is already 1.01 GiB).
  - **F5** binaries at this scale: emitted source 854 MB (48 KB→43 KB/
    class here), binary ≈ 1.01 GiB — record for the download-size story.

### [2026-11-09 ~11:00 UTC] session 2 — sandbox reset #2 SURVIVED by the boards (this file is the proof they work)
- The sandbox was recycled between 09:25 and ~11:00 UTC. The persistent
  layer came back REVERTED to the 2026-11-01 snapshot: `work/j2r-gh` (the
  tree session 1 worked in), `work/m5invest/farm20k` raw `.time` files,
  `env.sh`, `tools/` and the installed toolchain are all gone locally;
  `work/j2r-fresh` and `work/j2r-audit` remained only as stale Nov-1
  snapshots.
- **Recovery took one command**: `git fetch origin && git reset --hard
  origin/main` in `work/j2r-fresh` → HEAD `cb70af5` (the
  20k-confirmation commit), all three workflows green on it (CI /
  CodeQL / OSV, runs 42593128330 / 42593128239 / 42593128937). NOTHING
  of record was lost: every number, decision and finding of the dead
  session lives in console.md, console_summary.md, TODO.md and the docs
  on GitHub — because session 1 pushed them. The raw farm20k `.time`
  files are the only non-recoverable artifacts; their values are
  transcribed in the 08:34–09:25 log entry above (new lesson #11:
  commit raw measurement files, not just their summaries).
- Working-tree rule updated (supersedes old mistake #6): after any
  reset, pin ANY local tree with `git fetch origin && git reset --hard
  origin/main`; verify `git rev-parse HEAD` equals `origin/main` before
  working. This session works in `work/j2r-fresh`.
- Registered while recovering (not yet acted on): two open dependabot
  PRs — #1 `actions/checkout` 4.4.0→7.0.1, #2 `actions/setup-java`
  4.9.1→6.1.0 (opened 2026-11-05, CI verdict pending rebootstrap) — and
  21 open code-scanning alerts (zizmor ref-version-mismatch ×3,
  dependabot-cooldown ×2, j2r/stdout-print ×14+; known, tracked, not
  wave-2 blockers).
- Toolchain rebootstrap next (protocol: console_summary.md §9):
  rustup/cargo, Temurin 21 → `/tmp/jdk21`, `env.sh`, `tools/` — then the
  full local baseline is re-proven before any T08 code (never assume
  green: T05's law, environments rot).
- Then M5 wave 2 (gate satisfied): T08 preconditions checklist in
  console_summary.md §3.1 → T08 `j2r-runtime::jni` JNIEnv/
  InvokeInterface static tables + compile-time index assertions
  (M5_PLAN §3.1).

<!-- APPEND NEW ENTRIES BELOW THIS LINE -->
