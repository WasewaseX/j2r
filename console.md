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

### [2026-11-09 ~11:00–13:30 UTC] session 2 (cont.) — baseline re-proven, T08 LANDED (wave 2 started)
- Toolchain rebootstrap done (console_summary.md §9 protocol): rustup
  rustc/cargo 1.99.1 + Temurin 21.0.13 at /tmp/jdk21 + env.sh — exact
  pinned versions back. rustup components rustfmt/clippy added.
- **Full baseline re-proven before any T08 code** (T05's law): 352 tests
  0 failures — classfile 38, runtime 163, codegen 125, cli 26 incl.
  cfg_equiv 303.29 s (zero divergences), diff_suite 319.54 s (109
  fixtures, three-channel vs Temurin), partition/jar/soe/lambda/corpus
  all green.
- **T08 LANDED** (M5 wave 2, M5_PLAN §3.1): `j2r-runtime::jni` —
  JNIEnv/JNIInvokeInterface static tables generated mechanically from
  the Temurin 21 jni.h (the differential oracle's own header): 235
  slots = 4 reserved NULL + 231 functions; generator
  (scripts/gen_jni_env_table.py) VERIFIES every typedef assumption
  against the header and fails loudly on drift; slot list committed as
  evidence (scripts/evidence/jni_slot_list.txt). Layout pinned by a
  const-evaluated offset_of! assertion per slot (drift = compile
  error). Every non-reserved slot: real fn (GetVersion =
  JNI_VERSION_1_8; GetEnv's unsupported-version path = JNI_EVERSION)
  or loud slot-named stub — never null. 7 unit tests (runtime lib 163
  -> 170; workspace total 359).
- **Two wave-2 findings registered in jni/mod.rs** (owned by T09/T11):
  F-J1 — pointer-OUT slots (GetEnv success path, Attach*, the region
  writers) cannot be implemented under forbid(unsafe_code); resolution
  = dedicated dependency-free FFI crate with an audited tiny unsafe
  surface; registered now because it shapes T09. F-J2 — C-varargs
  slots typed at A-form arity (never-read pointer) until T11's
  trade-off note.
- **Semantics discovery, pinned by test:** a stub panic cannot unwind
  across the extern "system" boundary (Rust >= 1.81) — the loud failure
  is a process ABORT carrying the slot-named message. Observed the
  soe_e2e way: child process, assert signal 6 + message contains the
  slot name.
- **Suite-caught real bug (the law pays):** runtime VENDORING
  (copy_runtime in codegen emit.rs) was a flat file copy — the new
  jni/ directory never reached vendored crates and the codegen e2e
  regressed ("generated crate must build", E0583 module not found).
  Fixed recursively (copy + stale sweep + manifest recording all walk
  subdirs); full battery re-run green after the fix: cfg_equiv
  310.26 s, diff_suite 320.12 s, partition 17.4 s, jar 13.6 s, corpus
  32.6 s, soe 9.0 s, lambda 1.1 s, runtime lib 170, release build
  green, clippy workspace 0 warnings, fmt clean, doc counts in sync
  (359).
- Session lesson: a python replace that "succeeds" can still no-op on
  fmt-reformatted text — ALWAYS grep-verify a scripted edit (caught
  twice this session: the abort test and the doc bullet).
- Boards updated; M5_PLAN/TODO registers now mark T08 landed.

### [2026-11-09 ~13:30–14:30 UTC] session 2 (cont.) — T08 CI GREEN, wave-2 F-J1 design note of record
- **CI green on T08 commit 69eee72**: main CI run 42616529578 completed
  SUCCESS (all 21 gates — incl. gate-3 differential, gate-5 standalone,
  gate-10 no-unsafe, gate-11 doc-counts — plus CodeQL 42616529566, OSV
  42616530150, Scorecard 42616529573, all success). T08 is LANDED with
  full evidence, not just local.
- **F-J1 resolution design note published: docs/M5_JNI_FFI.md** (T09's
  gate): the scoped FFI crate `j2r-jni-ffi` chosen (option (a)); the
  zero-unsafe law amendment written exactly (exception = one audited
  crate, ≤ 6 primitive fns, ≤ 12 pinned unsafe occurrences, gate-10
  exception list + count assertion shipping in the same commit as the
  first unsafe line); the safe handle-tag trick documented (jobject =
  integer tag, `ptr as usize` is safe, table lookup is safe — no deref
  of foreign memory on the hot path); option (b) (35 slots abort
  forever) registered as the fallback if the owner rejects the
  amendment. Slot inventory mechanically verified against the T08
  evidence file (indices read off jni_slot_list.txt, not recalled —
  three from-memory drafts were wrong and got corrected by the file).
  M5_PLAN §3 now links the note as T09's design of record.

### [2026-11-09 ~14:45–15:40 UTC] session 3 — sandbox reset #3 recovered; baseline re-proven (359 green); boards near-miss registered
- Sandbox recycled again after session 2. Recovery per the boards'
  protocol (§9): `git fetch origin && git reset --hard origin/main` in
  `work/j2r-fresh` → HEAD `a2576c1`, clean, == origin/main. This is the
  first recovery where the fresh assistant ALSO found the local tree
  three milestones stale (b4badea, pre-M3) — the boards + remote are the
  only truth; the protocol caught it in one command.
- **Near-miss registered (lesson #12):** the fresh assistant, not yet
  aware the boards existed (the local tree predated them), created
  duplicate `CONSOLE.MD`/`CONSOLE_SUMMARY.MD` (uppercase) at the repo
  root from the stale local view — containing the superseded "M4 =
  Minecraft readiness" naming (mistake #1 exactly). Caught by checking
  origin before pushing; the duplicates were discarded via the §9 pin
  protocol. **Rule: the boards are `console.md` + `console_summary.md`
  (lowercase, repo root) — NEVER create duplicate/parallel state files;
  if a local tree is stale, §9 recovers, don't rewrite.**
- Toolchain rebootstrap (§3.1 item 1), with one improvement: Temurin now
  lives in the PERSISTENT layer, not /tmp — `/home/z/my-project/tools/
  jdk-21.0.13+9` (javac 21.0.13 verified, `J2R_JAVAC` + JAVA_HOME +
  PATH exported by a recreated `/home/z/my-project/env.sh`; env.sh also
  sets `CARGO_PROFILE_DEV_DEBUG=0` per mistake #10). rustup pinned
  toolchain 1.99.1 (+ rustfmt 1.10.0, clippy 0.1.99) reinstalled to
  `~/.cargo`+`~/.rustup`. **Measured rebootstrap download, for the
  standing download-size answer: rustup minimal profile ≈ 230 MB
  downloaded / 614 MB installed; Temurin 21.0.13 JDK ≈ 198 MB
  downloaded / 346 MB installed; workspace crate deps ≈ 2–3 MB
  (clap tree only). Full environment ≈ 430 MB download, ~960 MB disk.**
- **Full local baseline re-proven BEFORE any T09 code (T05's law):**
  359 tests, 0 failures — classfile 38, runtime 170, codegen 125
  (lib 101 + e2e 1 + malformed 12 + manifest 10 + wide_ids 1), cli 26
  (main 15 + cfg_equiv 1 + corpus 1 + diff_suite 2 + jar 1 + lambda 2 +
  partition 1 + soe 3). Long-suite walls this session: diff_suite
  412.47 s (fresh javac compile of all 109 fixtures) then 340.24 s,
  cfg_equiv 346.63 s / 340.24 s, corpus 38.31 / 36.71 s — zero
  divergences anywhere. fmt clean, clippy workspace 0 warnings.
- Standing owner questions — all answered on the boards, do not redo:
  M4/M-scale (§2 + mistake #1: M4 = threads & concurrency, M5 =
  Minecraft readiness; "large-scale compilation" is M5 wave-1 scope,
  CLOSED with the 20k evidence), download size (§5 + this entry's
  measured rebootstrap numbers; ~300 GB figures are single-process
  build memory, never artifact size), T08 prerequisites (§3.1 —
  satisfied; T08 itself landed with CI run 42616529578).
- **Next: T09** — `j2r-jni-ffi` scoped FFI crate + handle-table bridge
  (jobject ↔ GC handles, local/global refs, Push/PopLocalFrame) per
  docs/M5_JNI_FFI.md (design of record); crate + primitives + safe
  bridge + gate-10 amendment must land in the SAME commit.

### [2026-11-09 ~15:40–16:50 UTC] session 3 (cont.) — T09 LANDED: audited FFI crate + handle-table bridge, 379 green
- **T09 (M5 wave 2) landed per docs/M5_JNI_FFI.md**, the design of
  record: new workspace member `crates/j2r-jni-ffi` (zero deps, no
  forbid — THE one audited exception) + safe bridge
  `j2r-runtime::jni::bridge` under the runtime's forbid(unsafe_code).
- **The audited surface: 6 primitives, 5 actual `unsafe` occurrences**
  (pin ≤ 12): copy_from_caller / copy_to_caller / write_ptr_out /
  read_caller_str / copy_back_elements (one unsafe block each, every
  one a TOTAL function — null/zero-len degrades to no-op, never UB) +
  direct_buffer_ok (zero unsafe — address arithmetic is safe). Signature
  refinement registered in the note: read_caller_str returns raw bytes
  (`Option<Vec<u8>>`), mUTF-8 decode stays safe-side in the runtime.
  An in-crate test mirrors the gate's grep and pins the count, so drift
  fails locally before CI.
- **gate-10 amended in the SAME commit** (the gate-and-exception-ship-
  together discipline): rg over crates/ EXCLUDING ffi.rs (any hit
  fails) + the exception file's presence asserted + its count asserted
  ≤ 12. Verified locally: no unsafe outside, count = 5.
- **The safe bridge:** jobject = integer tag = (per-slot gen)<<32 |
  (slot+1) into ONE process-global handle table (tag 0 = JNI null; the
  int→ptr cast at the ABI boundary is safe — foreign code never
  dereferences our tags). Local refs = per-thread frame stacks
  (Push/PopLocalFrame with promote + full-frame sweep — the sweep bug
  was caught by the tests before landing, the law pays), global refs,
  DeleteLocal/DeleteGlobal owner-checked (cross-thread/wrong-kind =
  loud), resolve() loud on stale/foreign tags (generation mismatch —
  silent aliasing structurally impossible), detach hook for T13,
  direct-critical registry (address-keyed, loud miss; the pinning note:
  the arena never moves and array payloads never reallocate, so
  Critical pinning is free — the registry makes the root explicit and
  release verified). scan_roots = heap::collect root source 5 (all
  locals of every thread + globals + criticals), pointer-free protocol.
  Lock laws L-J1/L-J2/L-J3 documented (frames→table order; no heap, no
  safepoint, no Java dispatch under bridge locks).
- **Vendoring generalized**: copy_runtime now ships BOTH crates
  (`runtime/` + `j2r-jni-ffi/`) into generated projects — the runtime's
  internal path dep (`../j2r-jni-ffi`) resolves in both layouts; the
  codegen e2e ("generated crate must build") proves the two-crate
  layout. Cargo.lock updated (committed, CI runs --locked).
- **Evidence: 379 tests, 0 failures** — classfile 38, runtime 183
  (+13 bridge), codegen 125, j2r-jni-ffi 7, cli 26; fmt clean; clippy
  `-D warnings` clean; doc-counts in sync (gate-11); diff_suite 434.0 s
  (109 fixtures, three-channel vs Temurin 21.0.13) and cfg_equiv
  348.2 s (zero divergences) re-run AFTER the landing. README census
  359 → 379.
- M5_PLAN register: T09 → [x] with the evidence summary. Next: **T10**
  — the v1 surface (GetObjectClass/FindClass/field accessors/arrays/
  modified-UTF-8 strings) per M5_PLAN §3.3.

<!-- APPEND NEW ENTRIES BELOW THIS LINE -->
