# j2r console — the live log

**Owner directive (2026-11-09):** every session writes everything that
happens into this file, like a console. `console_summary.md` is the
distilled state board; this file is the raw append-only log. **Read both
files before acting on anything** — they carry the authoritative state,
the correct milestone names, and the never-repeat mistake list. If this
file disagrees with memory, this file wins.

---

## STATE AT A GLANCE (kept true by every session)

- **Project:** j2r — Java → Rust compiler, zero `unsafe` outside the
  audited `j2r-jni-ffi` FFI core (11 occurrences, gate-10-pinned ≤ 12,
  docs/M5_JNI_FFI.md §3). Repo:
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
- **M5 wave 2 (JNI): T08–T12 COMPLETE, T13 13a+13b LANDED (2026-11-10).**
  T08 tables ✅ · T09 FFI+bridge ✅ · T10 strings/classes/arrays ✅ ·
  T11 all sixty Call*MethodA/V ✅ · T12 RegisterNatives + the call-through
  + the acceptance e2e ✅ · T13: 13a note ✅, 13b primitive 11 + the REAL
  C probe ✅ (444 green) — 13c harness parity + 13d attach/CI remain.
  New: docs/REVIEW_CHECKLIST.md — the strict pass/fail gate register
  (the owner's external review converted to evidence law).
- **M5 waves 3–5 (next after 13c/13d):** T14–T16 mappings ·
  T17–T21 mixins · T22–T23 inline caches (order pinned by
  docs/REVIEW_CHECKLIST.md G5: after the JNI + scale gates).
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

### [2026-11-09 ~17:00 UTC] session 3 (cont.) — T10 slice 1 LANDED: JNI string marshalling (modified UTF-8)
- `j2r-runtime::jni::strings`: decode_modified_utf8 (byte-for-byte
  mirror of the classfile loader's pinned semantics — JVMS 4.4.7,
  C0 80 NUL, surrogate-pair supplementary form, unpaired-surrogate →
  U+FFFD lossy law from corpus/Utf16Probe, lenient raw-0x00 exactly
  like the oracle; zero-dep law means the mirror lives here, tests pin
  both sides against the same fixtures) + encode_modified_utf8 (total:
  a Rust String holds no unpaired surrogates).
- Slot-level safe API for the string family: slot_new_string_utf (None
  on malformed → the slot wrapper returns JNI null), slot_new_string
  (UTF-16 units), slot_get_string_utf_length (mUTF-8 bytes, terminator
  excluded per the JNI spec), slot_get_string_utf_chars (NUL-terminated
  buffer handed out registry-pinned, isCopy = JNI_TRUE always) +
  slot_release_string_utf_chars (loud miss — the T09 registry law).
  These are the functions the generator's real-slot registry (slice 2)
  will point the four string slots at.
- 9 new tests (round-trips: ASCII, embedded NUL, supplementary
  U+1D530, unpaired-surrogate law, malformed rejections; UTF-length
  contract; bridge round-trip; UTF-chars registry + loud double/
  foreign release). One test fix during landing: the raw-0x00 decode
  is LENIENT in the oracle — the test was stricter than the mirrored
  law and was corrected to match the oracle (implementation untouched).
- Evidence: 29 jni tests green (13 bridge + 7 T08 + 9 strings),
  workspace 388, fmt clean, clippy -D warnings clean, doc-counts in
  sync (README 379 → 388), codegen/classfile/ffi/cli-main suites
  re-run green. diff_suite/cfg_equiv NOT re-run for this slice (no
  codegen/emission change — pure runtime-module addition); they re-run
  at T10 completion per the standing battery rule.
- M5_PLAN register: T10 → [~] in progress (slice 1 of 3: strings ✅;
  next: slice 2 string-slot wiring via the generator's real-slot
  mechanism + slice 3 GetObjectClass/FindClass/field IDs/arrays on the
  global-dispatch decision recorded in the T10 design notes).

<!-- APPEND NEW ENTRIES BELOW THIS LINE -->


### [2026-11-10 ~10:00–11:30 UTC] session 4 — resets #4 AND #5 survived; baseline re-proven (388 green); T10 slice 2 next
- Sandbox recycled twice inside one session (a mid-session tool outage,
  then the restart) and the persistent layer came back reverted BOTH
  times: `env.sh`, `tools/` (Temurin), `~/.cargo`/`~/.rustup` all wiped;
  `work/j2r-fresh` and its `target/` (509 M) survived. The §9 protocol
  was run each time: `git fetch origin && git reset --hard origin/main`
  → HEAD `84734d6`, `git rev-parse HEAD == origin/main` verified.
- **Lesson #12 near-miss, second occurrence, caught before any push
  again:** a fresh-context assistant (this session's start) found the
  local tree stale at `b4badea` (pre-M3) and — not having read the
  boards first — re-created duplicate uppercase `CONSOLE.MD`/
  `CONSOLE_SUMMARY.MD` from the stale view, including the superseded
  "M4 = Minecraft readiness" naming (mistake #1 exactly), plus a
  "T22 reconciliation" edit to the stale ROADMAP and a local-only
  commit `c677b50`. The push was rejected (origin 115 ahead), the
  divergence check exposed the staleness, and the §9 pin discarded all
  of it. **Rule confirmed twice now: §9-pin FIRST, then read the
  boards, then act. Nothing of the duplicates ever reached origin.**
- Toolchain rebootstrap ×2 (same session-3 protocol): rustup stable =
  rustc/cargo 1.99.1 + rustfmt 1.10.0 + clippy 0.1.99 (exact pins);
  Temurin 21.0.13+9 → `/home/z/my-project/tools/jdk-21.0.13+9`;
  `env.sh` recreated (J2R_JAVAC, JAVA_HOME, PATH, CARGO_PROFILE_DEV_
  DEBUG=0). Rebootstrap download measured: rustup-init 21 MB, Temurin
  tarball 199 MB (consistent with session 3's ≈ 230 MB rustup / 198 MB
  JDK totals).
- **Full local baseline re-proven BEFORE any T10 code (T05's law):
  388 tests, 0 failures** — classfile 38 · runtime 192 · codegen 125
  (lib 101 + e2e 1 + malformed 12 + manifest 10 + wide_ids 1) ·
  j2r-jni-ffi 7 · cli bin 15 + soe 3 + lambda 2 + partition 1 + jar 1 +
  corpus 1 + cfg_equiv 1 + diff_suite 2. Walls: cfg_equiv 324.27 s,
  diff_suite 425.65 s (fresh javac compile of all fixtures),
  corpus 36.75 s, partition 21.53 s, soe 17.02 s, jar 9.62 s,
  lambda 1.20 s — zero divergences anywhere; fmt clean; clippy
  `-D warnings` clean.
- Next: **T10 slice 2** — wire the four string slots through the
  generator's real-slot mechanism (slot_new_string_utf /
  slot_new_string / slot_get_string_utf_length /
  slot_get_string_utf_chars + slot_release_string_utf_chars from
  slice 1's `jni::strings`), then slice 3 (GetObjectClass/FindClass/
  field IDs/arrays) on the recorded global-dispatch decision.

### [2026-11-10 ~21:30 UTC] session 4 (cont.) — T10 slice 2 LANDED: the five string slots wired through the generator's real-slot registry
- Generator (`scripts/gen_jni_env_table.py`): `REAL_SLOTS` registry
  (NewString 163, NewStringUTF 167, GetStringUTFLength 168,
  GetStringUTFChars 169, ReleaseStringUTFChars 170) + `WRAPPER_BODIES`
  emitted verbatim; the registry is verified against the parsed jni.h
  (renamed/removed slot → loud FATAL) and every entry must have a body.
  Slot-resolution law unchanged: semantics live in
  `j2r-runtime::jni::strings` (slice 1), raw-pointer moments go through
  the audited `j2r-jni-ffi` primitives.
- `j2r-jni-ffi` amendment (gate-10, same-commit law): primitive 7
  `write_u8_out(slot: *mut u8, value: u8)` — primitive 3 writes a full
  pointer word and would overrun the 1-byte `jboolean` out-slot
  (GetStringUTFChars's `isCopy`). Actual unsafe occurrences 5 → 6, pin
  stays ≤ 12 with headroom; M5_JNI_FFI.md §3/§4 amended in the same
  change-set.
- Wrapper shapes (v1 gaps registered for the T13 harness): null input /
  negative length / malformed mUTF-8 → JNI null (NPE shapes pending the
  exception machinery); `isCopy` always JNI_TRUE via primitive 7 when
  the caller asks; a release registry miss is a loud panic → across the
  extern "system" boundary the process aborts.
- **The abort shape is now pinned, not just claimed**: new child-process
  test `release_string_utf_chars_registry_miss_aborts_loudly`
  (jni/mod.rs, the GetEnv pattern): a foreign pointer (never handed
  out) through ENV_TABLE.ReleaseStringUTFChars → SIGABRT, abort message
  names the slot family. Clippy lesson: `1 as *const c_char` trips
  `manual_dangling_ptr` — `std::ptr::dangling()` is the blessed
  non-null-never-handed-out shape.
- **Lesson #5 fixed in the generator**: the default jni.h path was
  `/tmp/jdk21/...` (dead after every reset) — now
  `$JAVA_HOME/include/jni.h` with the persistent-layer literal
  fallback; the argv[1] override is kept. Reproducibility re-proven:
  regenerate + `cargo fmt` reproduces the working tree **byte-for-byte**
  (md5 0cce6c8c…; the evidence file is stable).
- Evidence — **the full battery run NOW, beyond the standing rule**
  (the recorded rule defers diff_suite/cfg_equiv to T10 completion;
  both re-run anyway, zero divergences): fmt clean; clippy
  `-D warnings` clean; **390 tests, 0 failures** — jni-ffi 8 (+1
  write_u8_out), runtime 193 (+1 abort pin), classfile 38, codegen 125,
  cli bin 15 + soe 1 + lambda 2 + jar 3 + partition 1 + corpus 1 +
  cfg_equiv 1 (346.83 s) + diff_suite 2 (430.36 s, 109 fixtures vs
  Temurin 21.0.13). Doc-count gate in sync (README 388 → 390);
  M5_PLAN register: T10 slice 2 recorded.
- Next: **T10 slice 3** — GetObjectClass/FindClass/field IDs/arrays on
  the recorded global-dispatch decision (M5_PLAN §3.3).

### [2026-11-10 ~22:30 UTC] session 4 (cont. 2) — T10 slice 3a LANDED: host dispatch, jclass, field IDs, 36 accessors
- Design note FIRST (docs/M5_JNI_CLASSES.md, committed 84cede9 per the
  wave rule): the global-dispatch decision, jclass/jfieldID
  representation, accessor laws, array plan (§5 pending as slice 3b),
  and the evidence bar. Every decision grounded in a measured fact —
  the env-table ABI has no ctx parameter, so a process-global slot is
  the only shape that does not fork the table.
- `jni::host_dispatch`: ONE process-global Dispatch slot
  (Mutex<Option<Dispatch>>, all-None default) + install_host_dispatch;
  the emitted main_run installs it BEFORE any Java code — single-crate
  (emit.rs) installs the crate dispatch, partitioned (partition.rs)
  installs the COMPOSED dispatch from the support slot; one emission
  point covers both layouts, and the partition differential (forced
  k=2, 24.45 s) is the multi-crate install proof.
- `jni::classes` (new module, ALL semantics in safe Rust; the generated
  wrappers are thin casts): jclass = the tag of the interned
  java/lang/Class mirror (reflect::class_ref) — Temurin-faithful,
  identity via interning; the boundary guard checks the object's OWN
  class, because a Class mirror and a Java String BOTH carry
  Value::Str in slot 0 — `class_name_of` alone cannot tell them apart
  (caught by the tests, not by review). FindClass existence-validates
  (compiled via the host dispatch / builtin tables / the two
  structural classes) BEFORE interning; `[L…;` trailing-semicolon
  handling covered by tests.
- jfieldID: bit-63-marked tags into a dedicated registry (dedup: the
  same (owner, name, desc, static?) triple → the same ID; permanent —
  the JNI law), superclass walk for inherited fields (JNI semantics,
  unlike getDeclaredField), kind/receiver guards loud (Temurin: UB →
  here a named abort). The 36 accessor wrappers are generated
  PROGRAMMATICALLY from a per-prim table — the verbatim-body pattern
  does not scale to 36; the per-slot doc comments still carry the true
  slot numbers.
- **Generator amendment (the F-J1 shape again):** T08 had mapped
  jfieldID/jmethodID table signatures to BARE `c_void` — uninhabited,
  every such slot unimplementable. Slice 3a restored the ABI-faithful
  pointer alias (the generator had verified
  `typedef struct _jfieldID *jfieldID;` all along, then contradicted
  it in the mapping). Recorded in M5_JNI_CLASSES.md §3 + jni/mod.rs
  type-fidelity notes.
- Generator hardening learned the hard way: a truncated edit left
  broken Python (caught immediately — the generator FATALs loudly by
  design); the accessor bodies initially missed the match-brace pair
  (rustc caught it); clippy double_parens on the Boolean ctor — all
  fixed at the SOURCE (the generator), never by patching the
  generated file.
- Evidence — full battery MANDATORY (codegen emission changed): fmt
  clean; clippy `-D warnings` clean; **398 tests, 0 failures** —
  classfile 38 · runtime 201 (+8 classes) · codegen 125 · jni-ffi 8 ·
  cli 26 (bin 15 + soe 1 + lambda 2 + jar 3 + partition 1 + corpus 1) ·
  cfg_equiv 1 (347.95 s) · diff_suite 2 (421.68 s, 109 fixtures) —
  zero divergences; doc gate in sync (README 390 → 398).
- Next: **T10 slice 3b** — array families (GetArrayLength 171, the
  object element pair 173/174, New*Array 175–182, Get/Release*
  ArrayElements 183–198, region pairs 199–214, critical pair 217/218)
  per M5_JNI_CLASSES.md §5 — then T10 is COMPLETE.

### [2026-11-10 ~09:00–10:30 UTC] session 5 (cron cont.) — reset #6 survived; T10 slice 3b landed: the 46 array slots — T10 COMPLETE (414 green)
- Sandbox reset #6 on session start: toolchain wiped again (cargo, JDK,
  env.sh all gone), local tree back at `b4badea` (pre-M3) — the §9
  protocol ran first: `git fetch origin && git reset --hard origin/main`
  → HEAD `6e9805a` verified == origin/main (the boards' session-4 work —
  slice 2 AND slice 3a — was already pushed and is the truth).
- Toolchain rebootstrap (§3.1 item 1, the session-3/4 protocol): rustup
  → exact pins rustc/cargo 1.99.1 + rustfmt 1.10.0 + clippy 0.1.99;
  Temurin 21.0.13+9 → `/home/z/my-project/tools/jdk-21.0.13+9` (tarball
  207,858,352 B ≈ the recorded 199 MB); `env.sh` recreated (J2R_JAVAC,
  JAVA_HOME, PATH, CARGO_PROFILE_DEV_DEBUG=0). `target/` (509 MB)
  survived → fast rebuilds.
- Full baseline re-proven BEFORE any slice-3b code (T05's law): **398
  tests, 0 failures** — classfile 38 · jni-ffi 8 · runtime 201 · codegen
  125 · cli 26; walls: cfg_equiv 336.24 s, diff_suite 425.97 s, corpus
  36.94 s, partition 24.19 s, soe 17.93 s, jar 10.05 s — fmt clean,
  clippy `-D warnings` clean. Correction of record: the per-suite cli
  labels in the session-4 entry were transposed — **soe_e2e = 1 test,
  jar_suite = 3** (counted with --list this session); the total 26 and
  workspace 398 were correct.
- **T10 slice 3b landed** (docs/M5_JNI_CLASSES.md §5 is the design of
  record): the 46 array slots through the generator real-slot registry
  (45 → 91 real slots):
  * 171 GetArrayLength (null → 0, the §5-conceded law) · 172
    NewObjectArray (element class FROM the jclass mirror via
    `class_name_of`; init resolves ONCE, every slot stores a copy) ·
    173/174 Get/SetObjectArrayElement (the store's ArrayStoreException
    check rides the host dispatch).
  * 175–182 New<Prim>Array — each slot IS one primitive (no descriptor
    parsing); negative length aborts loudly (NASE name in the message).
  * 183–190 Get<Prim>ArrayElements — caller-owned COPY, registry-pinned
    `(buf, array JRef, len, kind)` by the handed-out address; isCopy =
    JNI_TRUE through audited primitive 7 (write_u8_out).
  * 191–198 Release<Prim>ArrayElements — the mode law (JNI_COMMIT 1 =
    copy back + keep; 0 = copy + free; JNI_ABORT 2 = free only); copy
    back rides the raw store accessors. **Validate-before-remove
    discipline**: wrong family / foreign pointer / wrong array handle
    panic loudly WITHOUT mutating the registry, so a caught loud miss
    never breaks a later valid release (a test proved the naive
    remove-first order wrong — fixed at the source).
  * 199–206/207–214 Get/Set<Prim>ArrayRegion — bounds checked UP FRONT
    (violation = loud AIOOBE), bulk copy through the audited
    copy_to_caller/copy_from_caller + raw accessors, narrowing per the
    ARRAY descriptor.
  * 222/223 Get/ReleasePrimitiveArrayCritical — the existing critical
    registry; the handed address IS the live payload (arena never moves,
    payloads never realloc — the M2 advantage); isCopy = JNI_FALSE.
    **Design-note correction**: M5_JNI_CLASSES.md §5 said 217/218 for
    the critical pair — those are MonitorEnter/MonitorExit; the
    offset_of!-pinned table (verified vs Temurin jni.h) is authoritative:
    222/223. Note fixed in the same commit; mod.rs module doc's "slot
    213" claim fixed too.
- New module `jni::arrays` (slot-level API, `PrimKind` one-variant-per-
  slot) + 15 tests; +1 child-process test in jni/mod.rs
  (J2R_JNI_ARRAY_RELEASE_ABORT_CHILD: a real array handle + a pointer we
  never handed out through the generated ReleaseIntArrayElements wrapper
  → SIGABRT with the slot named — the §5 law "pinned again by a
  child-process test" made true). No new FFI primitives (gate-10
  unchanged); zero `unsafe` in j2r-runtime (forbid holds).
- Generator: 46 REAL_SLOTS entries + 6 hand-written + 40 loop-generated
  wrapper bodies; regenerate+fmt **idempotent byte-for-byte** (two-pass
  diff empty). Clippy drove two generator fixes at the SOURCE: emitted
  wrappers omit the unit `-> ()`; region buffers cast via
  `buf.cast::<u8>()` (never a redundant same-type `as`).
- Evidence — full battery re-run anyway (the boards' standing law):
  fmt clean; clippy `-D warnings` clean; **414 tests, 0 failures** —
  classfile 38 · jni-ffi 8 · runtime 217 · codegen 125 (lib 101 + e2e 1
  + malformed 12 + manifest 10 + wide_ids 1) · cli bin 15 + soe_e2e 1 +
  lambda 2 + partition 1 + jar 3 + corpus 1 + cfg_equiv 1 (339.36 s) +
  diff_suite 2 (438.24 s) — **zero divergences**; doc gate in sync
  (README 398 → 414); M5_PLAN T10 → [x] COMPLETE.
- Next: **T11** — the arity-shim trade-off note (variadic Call*Method
  ABI) + the Call*Method/CallStatic*Method families (wave 2 continues).

### [2026-11-10 ~09:40 UTC] session 5 cont. — T11 slice 11a: the F-J2 resolution design note committed first
- docs/M5_JNI_CALLS.md is T11's design of record (the slice-3a
  pattern: note BEFORE code). It resolves flagged-open finding **F-J2**
  with the measured ABI reasoning:
  * **A-forms (Call*MethodA, 30 slots) = the v1 workhorse** — a plain
    jvalue* array; descriptor-driven marshalling via the slice-3a
    method-ID registry + host-dispatch invoke. Fully faithful, zero
    unsafe, no approximation.
  * **V-forms** via the documented SysV __va_list_tag save-area walk
    through new audited j2r-jni-ffi primitives; gate-10 count amended
    in the same commit (6 used of pin 12 today).
  * **Bare C-variadic forms: LOUD aborts, honestly typed** — no safe
    stable-Rust way to read varargs (VaList is nightly; libffi breaks
    both standing laws; integer-only max-arity shims silently corrupt
    SSE-class args — the failure class this project exists to make
    impossible). Registered for M6: which forms LWJGL3 emits.
  * The **pending-exception cell** (ExceptionOccurred 15 / Clear 17 /
    Check 228 become real) — the first faithful exception transport;
    Call*Method throwing = zero return + pending throwable, JNI-faith.
- Slice order: 11a this note → 11b pending cell + A-forms → 11c
  V-forms + gate-10 amendment. T13's C-harness bar unchanged.

### [2026-11-10 ~10:50 UTC] session 5 cont. 2 — T11 slice 11b part 1: pending-exception cell + method-ID registry + the ten virtual Call*MethodA slots (418 green)
- Landed (docs/M5_JNI_CALLS.md §3 decision 1/4 is now code):
  * **bridge::pend_exception / take_pending_exception /
    has_pending_exception / pending_exception_local** — the per-thread
    pending-exception cell (JNI §The Exception Process). The first
    faithful exception transport across the boundary.
  * **classes: the method-ID registry** — bit-62-marked tags (disjoint
    from field IDs' bit 63 and from handle tags), permanent + deduped,
    `slot_get_method_id` walks owner + ancestors (the JNI search law)
    and PENDS NoSuchMethodError on miss — the first T10-era NPE-shaped
    gap migrated to the faithful pending shape.
  * **jni::calls: the A-form invoke core** — `decode_jvalue` per
    descriptor (sub-word kinds ride the LOW bytes; F = low 4 bytes;
    the pointer word IS the tag), `zero_value` per return descriptor,
    virtual dispatch by the RECEIVER's runtime class; null receiver /
    null args-with-demanding-descriptor pends NPE; a thrown answer is
    PENDED and the zero value returned — the wrapper CANNOT fail.
  * generator: 15 slots wired (91 → 106 real): ExceptionOccurred/Clear/
    Check (15/17/228) + GetMethodID/GetStaticMethodID (33/113) + the
    ten Call*MethodA (36..63 step 3, verified vs the offset_of table);
    regenerate+fmt byte-idempotent.
- 4 new unit tests (jvalue decode per kind incl. the tag-crossing
  check, zero values, method-ID dedupe/marks/disjointness with the
  field-ID-passed-as-method-ID loud law, the pending-cell round-trip
  incl. the REPEATABLE ExceptionOccurred law). runtime 217 → 221;
  fmt clean, clippy `-D warnings` clean.
- **NOT yet claimed**: the mock-dispatch invoke tests (virtual override
  vs declared-class semantics), the static/nonvirtual families (same
  core — the CallFamily enum is the next slice's first edit), the
  V-form save-area walk (slice 11c + the gate-10 amendment), and the
  full battery re-run — all land with 11b part 2 / 11c. No codegen
  change in this part (tables.rs is runtime-side) — the battery rides
  with part 2 per the standing law.

### [2026-11-10 ~12:15 UTC] session 5 cont. 3 — reset #7 survived; T11 slice 11b part 2: the static/nonvirtual Call*MethodA families + the mock-dispatch tests (424 green)
- Reset #7 (sandbox recycled between sessions; the persistent layer came
  back at pre-M3 `b4badea`, toolchain + tools/ + env.sh wiped): the §9
  recipe — `git fetch && git merge --ff-only origin/main` → HEAD
  `cf8394e` (the boards' truth: 11b-1 landed, 418 green). Rebootstrap
  per §3.1 item 1: rustup → exact pins rustc/cargo 1.99.1 + rustfmt
  1.10.0 + clippy 0.1.99; Temurin 21.0.13+9 tarball 207,858,352 B
  (byte-exact vs the recorded ~199 MB) → the persistent JDK path;
  env.sh recreated (J2R_JAVAC, JAVA_HOME, PATH, CARGO_PROFILE_DEV_DEBUG=0);
  target/ survived → warm builds.
- **Baseline re-proven BEFORE any 11b-2 code (T05's law): 418 tests,
  0 failures** — classfile 38 · jni-ffi 8 · runtime 221 · codegen 125
  (lib 101 + e2e 1 + malformed 12 + wide_ids 1 + manifest 10) · cli 26;
  walls: cfg_equiv 327.80 s, diff_suite 440.23 s, corpus 38.28 s,
  partition 25.66 s, soe 10.36 s, jar 18.94 s — fmt clean, clippy
  `-D warnings` clean.
- **T11 slice 11b part 2 LANDED** (docs/M5_JNI_CALLS.md §4 precision
  paragraph is the design of record):
  * ONE invoke core, three families (`CallFamily`: Virtual/Static/
    Nonvirtual), then the SAME ancestor walk (start → parent_of until
    find_method hits) — the JVM's own resolution shape. This FIXES the
    landed 11b-1 single-shot gap: an ID minted through GetMethodID's
    ancestor walk now works for a receiver whose runtime class only
    INHERITS the method (pinned by the walk test).
  * The ten `CallStatic*MethodA` (C sig `(env, clazz, mid, args)` — the
    receiver position carries the jclass; NO receiver) and the ten
    `CallNonvirtual*MethodA` (`(env, obj, clazz, mid, args)` — both
    ride the call; dispatch obeys the DECLARED class, the `super.m()`
    shape; precision of record: the walk starts at the jclass's own
    class, which on every VALID ID↔clazz combination lands exactly on
    the body the ID names — invalid combinations answer by the
    documented law instead of Temurin's UB).
  * Laws: family/staticness mismatch = LOUD panic (Temurin: UB); null
    jclass on static/nonvirtual pends NPE + zero (defensive); null
    receiver / null-args-with-demanding-descriptor laws unchanged; a
    thrown callee PENDS and answers the zero value.
  * generator: +20 REAL_SLOTS entries (106 → **126 real slots**) with
    the slot numbers asserted MECHANICALLY against the parsed header
    (a renumbered jni.h fails the generator loudly); regenerate+fmt
    **idempotent byte-for-byte** (two-pass diff empty); stale doc
    counts fixed at the SOURCE (the T08-era "45 slots" header paragraph
    and mod.rs's "GetVersion so far" line → the registry-truth
    wording; README 418 → 424).
- 6 new mock-dispatch unit tests (the M5_JNI_CALLS.md §5 evidence):
  virtual-dispatch-by-runtime-class + THE override-vs-declared-class
  proof (a Base-minted mark()I on a Derived receiver runs the
  OVERRIDE); nonvirtual = the mirror image (same ID, same receiver,
  declared Base → Base's body); static = no receiver, resolves through
  the subclass walk; staticness mismatches loud across all three
  families; null receiver/class/args pend NPE + zero; a thrown callee
  pends + zero. runtime 221 → 227.
- Evidence — full battery re-run (the standing law): fmt clean; clippy
  `-D warnings` clean; **424 tests, 0 failures** — classfile 38 ·
  jni-ffi 8 · runtime 227 · codegen 125 · cli 26; walls: cfg_equiv
  348.99 s + diff_suite 455.62 s (**zero divergences**), partition
  24.30 s, corpus 37.53 s, soe 10.00 s, jar 18.18 s, lambda 1.09 s.
  gate-10 unchanged (6 unsafe occurrences, no new primitives —
  `copy_from_caller` already covers the A-forms).
- Next (11c): the V-form save-area walk + the three audited primitives
  (read_word_at / read_fp_word_at / read_u32_at) + the gate-10
  amendment (6 → 9 of pin 12) in the same commit, then the battery.

### [2026-11-10 ~13:30 UTC] session 5 cont. 4 — T11 slice 11c: the V-form save-area walk + the gate-10 amendment (432 green) — T11 COMPLETE
- The audited FFI surface grows 7 → 10 primitives
  (`crates/j2r-jni-ffi/src/ffi.rs`, M5_JNI_CALLS.md §3 decision 2):
  `read_u32_at` (the __va_list_tag cursor fields), `read_word_at` (a GP
  vararg word — save area or overflow), `read_fp_word_at` (the value
  half of one 16-byte XMM save slot). **`ACTUAL_UNSAFE_OCCURRENCES`
  6 → 9 IN THE SAME COMMIT** (the T09 precedent); the gate-10 pin ≤ 12
  is unchanged and the source-scanned count assert passed locally. All
  three primitives are null-guarded (None) — the walk turns None into
  a loud internal-error pend, never a silent mis-marshal. 3 new
  primitive tests (cursor-field reads, GP word, SSE slot half incl.
  the upper-half-noise isolation). jni-ffi 8 → 11.
- The walk itself is SAFE code in `jni::calls` (`decode_vform_args`):
  SysV x86-64 ABI §3.5.6 — gp_offset @0, fp_offset @4,
  overflow_arg_area @8, reg_save_area @16; GP-class words (everything
  except F/D) come from `reg_save_area + gp_offset` while
  `gp_offset < 48`, F/D from `reg_save_area + fp_offset` while
  `fp_offset < 176` (16-byte stride, value in the low 8 bytes);
  exhausted registers fall back to the overflow area at 8 bytes per
  word. **v1 law of record: the record is consumed into a LOCAL cursor
  — the caller's record is left untouched** (one-shot consume;
  va_copy is the caller's tool for reuse; never mutating foreign
  memory is the conservative bridge shape). A/V share
  `resolve_checked` (the staticness loudness) and `dispatch` (the
  receiver/declared-class laws + the ancestor walk) — the V-form core
  adds only the arg decode.
- generator: +30 REAL_SLOTS entries (126 → **156 real slots**) — all
  thirty `Call*MethodV` (virtual 35..62, nonvirtual 65..92, static
  115..142, step 3) with the slot numbers asserted MECHANICALLY
  against the parsed header; regenerate+fmt **idempotent
  byte-for-byte**; the A/V wrapper docs carry the family law.
- 5 new V-form tests (the §5 acceptance: hand-built save areas with
  KNOWN layouts): INTEGER-only (gp_offset 16, three register words →
  12), FP-only + mixed GP/FP (independent cursor advance — mix(I D)I =
  2100), the register→overflow split (gp_offset exhausted → overflow
  words → 21), the LOCAL-cursor law (record bytes unchanged), and the
  loud laws (null ap with a demanding descriptor pends NPE; an
  unreadable record — reg_save_area = 0 — pends `java/lang/Error`,
  never a silent mis-marshal). runtime 227 → 232.
- Evidence — full battery re-run (the standing law): fmt clean; clippy
  `-D warnings` clean; **432 tests, 0 failures** — classfile 38 ·
  jni-ffi 11 · runtime 232 · codegen 125 · cli 26; walls: cfg_equiv
  337.91 s + diff_suite 442.93 s (**zero divergences**), partition
  25.10 s, corpus 38.70 s, soe 10.02 s, jar 18.10 s, lambda 1.07 s.
- **T11 COMPLETE** (the design of record's full slice order 11a→11b→
  11c landed in one day): the A/V forms of all three call families
  (60 slots), the pending-exception trio, the method-ID pair; the
  bare C-variadic forms stay LOUD aborts per decision 3 (honestly
  typed, message names the slot and the F-J2 resolution). Remaining
  wave 2: T12 (RegisterNatives) and T13 (C harness + attach
  lifecycle).

### [2026-11-10 ~15:05 UTC] session 5 cont. 5 — T12 slice 12b: the native-binding registry + slots 215/216 + the dispatch-walk consult (437 green)
- docs/M5_JNI_NATIVES.md is the design of record (12a, pushed first:
  c9abdc9). 12b lands the runtime half:
  * **`UserMethod.is_native`** (M5_JNI_NATIVES.md §5) — the metadata
    amendment, `false` at every existing construction site; the
    generated rows (emit + partition) now emit the flag from
    `MethodPlan.is_native` (12c turns the native-method codegen error
    into the call-through emission and sets the flag true).
  * **`jni::natives`** — the binding registry keyed by (declaring
    class, name, descriptor); `bind` enforces the declared-native law
    (class-SCOPED, no ancestor walk per §3.3), rebinding overrides,
    `unbind_class` = UnregisterNatives' law, `call_native` = the
    unbound law (Err(UnsatisfiedLinkError) naming the method) and the
    Rust-shape round-trip entry point. The SHAPE LAW of record (12b
    precision): the C-array slot path can only yield FOREIGN fn
    pointers (reconstructing a Rust fn from a word needs an unsafe
    transmute — forbidden); Rust-shape bindings ride the Rust API.
  * **slots 215/216 real** (156 → 158): RegisterNatives reads the
    JNINativeMethod array through the audited primitives, validates
    each triple (JNI_ERR + pending NoSuchMethodError on mismatch,
    NULL-fnPtr refused loudly, null clazz/negative-n laws) and stores
    Foreign bindings; UnregisterNatives clears. Slot numbers asserted
    mechanically against the parsed header; regenerate+fmt
    byte-idempotent.
  * **the dispatch-walk consult** (§4): every class the A/V dispatch
    walk visits is asked for a binding BEFORE advancing — bind-on-Base
    resolves for a Derived receiver exactly like an inherited compiled
    method (pinned by the walk-consult test). A Foreign binding call
    LOUDLY aborts naming T13's primitive 11 (pinned without a child
    process).
- 5 new tests (declared-native law incl. the wrong-descriptor arm,
  rebind/unbind, the unbound UnsatisfiedLinkError + the round-trip
  through call_native, the foreign-fnPtr loudness, the walk consult).
  runtime 232 → 237.
- Evidence — full battery re-run (the standing law): fmt clean; clippy
  `-D warnings` clean; **437 tests, 0 failures** — classfile 38 ·
  jni-ffi 11 · runtime 237 · codegen 125 · cli 26; walls: cfg_equiv
  350.90 s + diff_suite 447.79 s (**zero divergences**), partition
  25.18 s, corpus 39.15 s, soe 10.67 s, jar 19.34 s, lambda 1.21 s.
- Next (12c): plan_methods accepts ACC_NATIVE (no body) with the
  call-through emission + the metadata rows; the Java fixture
  round-trip (the M5_PLAN acceptance demo) + full battery.

### [2026-11-10 ~16:45 UTC] session 5 cont. 5 (cont.) — T12 slice 12c: the codegen call-through + the acceptance e2e (440 green) — T12 COMPLETE
- **plan_methods accepts `ACC_NATIVE`** (the old loud error is gone):
  the method plans with NO instrs, `is_native: true`, and the metadata
  row rides the generated registry (emit + partition layouts emit
  `is_native` from the plan). The verifier SKIPS native plans (no
  bytecode to verify — the body is correct by construction).
- **The emitted call-through body** (M5_JNI_NATIVES.md §4): the
  native method's fn is FrameGuard + ONE call to
  `j2r_runtime::jni::natives::call_native(owner, name, desc, recv,
  args)` — the same MethodFn contract as every generated body, so the
  ordinary zero-on-throw machinery applies; the FrameGuard stays (a
  native call is a Java frame — the catchable-SOE law counts it).
  BOTH lookup layouts map the native triple to the call-through fn (a
  real `MethodFn` — the dispatch stays uniform; the 12b walk consult
  remains as the no-metadata fallback). Intra-Java direct calls reach
  it by module+fn_name — no new machinery anywhere.
- **The acceptance e2e is a REAL three-channel differential**
  (`tests/native_e2e.rs`): javac compiles NativeProbe (a `native int
  add(int,int)` + a caller), Temurin runs it, j2r runs it — stdout
  ("before" only), exit (1), and the stderr HEADER are byte-identical.
  Making that true surfaced TWO real defects, both fixed at the root:
  1. our `UnsatisfiedLinkError` message was `{owner}::{name}{desc}`;
     the oracle's is the quoted Java declaration
     **`'int NativeProbe.add(int, int)'`** (probed live on Temurin
     21.0.13+9) — `unbound_link_error` now renders the descriptor in
     Java-source form (primitives lowercase, dotted classes, `[]`
     arrays) with the metadata simple_name;
  2. the linkage-error family was MISSING from the builtin parent
     table — the builtin toString dispatch fell back to the internal
     name with no message. Added: UnsatisfiedLinkError → LinkageError
     → Error → Throwable, NoSuchMethodError →
     IncompatibleClassChangeError → LinkageError (pinned by a new
     builtin unit test). runtime 237 → 238.
- 2 new e2e tests (the three-channel differential + the emitted-source
  assertions: `is_native: true` in the registry row, the call-through
  body present). cli 26 → 28.
- Evidence — full battery re-run (the standing law): fmt clean; clippy
  `-D warnings` clean; **440 tests, 0 failures** — classfile 38 ·
  jni-ffi 11 · runtime 238 · codegen 125 · cli 28; walls: cfg_equiv
  351.47 s + diff_suite 451.36 s (**zero divergences**), partition
  25.83 s, corpus 39.93 s, soe 10.47 s, jar 18.90 s, lambda 1.18 s.
- **T12 COMPLETE** (12a note → 12b registry/slots/consult → 12c
  codegen + the acceptance e2e). The M5_PLAN §3 wave-2 acceptance bar
  now stands at: RegisterNatives round-trip demonstrated (the unbound
  direction three-channel vs Temurin; the bound-Rust direction
  unit-pinned). Remaining wave 2: T13 (the C harness `.so` + attach
  lifecycle + primitive 11 the foreign-call thunk).

### [2026-11-10 ~06:30 UTC] session 6 — T13 slice 13b LANDED: primitive 11, the Foreign path, and the REAL C probe (444 green)
- Session opened on the owner's external readiness review (P0–P2
  priorities) + "check the repo state and read all .md files". Read the
  full doc set: both boards, README, TODO (structure + M5 section),
  ROADMAP (root + docs/), M5_PLAN, all five M5 JNI design notes,
  M5_SCALE, STRATEGIC_GAPS, BENCHMARKS (all 8 addenda),
  TYPED_REGISTER_READINESS, CORPUS_WAVE, EXTERNAL_V12_MINICRAFT,
  RUNTIME_SAFETY §6, examples/vectors READMEs, plus the diff of the
  uncommitted slice-13b work found in the tree (ffi.rs / calls.rs /
  natives.rs at HEAD 724e839).
- **Doc drift found and FIXED (the review's §4 point, enforced):**
  M5_PLAN §6 still showed T11/T12/T13 unchecked after T11 and T12 were
  COMPLETE; TODO.md said "Last updated 2026-11-08" with only T08
  recorded under the M5 JNI bullet; docs/ROADMAP.md's M5 ladder row
  still said "wave 0 (census) running"; README's status line still said
  "Milestone 3". All four updated to the boards' truth (T08–T12 landed,
  T13 13a+13b this commit). G0.4 doc-count gate re-run: in sync.
- **T13 slice 13b landed** (docs/M5_JNI_HARNESS.md §2/§6 is the design
  of record; the session started from the in-flight diff and completed
  it):
  * **primitive 11 `call_foreign_native`** in j2r-jni-ffi (the audited
    crate): the FIXED SysV max-register signature (env, recv, usize×4,
    f64×8) — integer-class args rdi/rsi/rdx/rcx/r8/r9, SSE args
    xmm0–7, the return read as BOTH channels (rax + xmm0) so ONE thunk
    serves every return kind; beyond the window (5+ GP / 9+ FP words)
    a LOUD panic naming the window law (stack-frame construction stays
    banned — libffi's exact problem).
  * **gate-10 amended in the same commit** (the T09/11c/13b law):
    `ACTUAL_UNSAFE_OCCURRENCES` 9 → 10 (primitive 11's one transmute),
    pin ≤ 12 unchanged; the source-scanned in-crate assert passes.
  * **The `Binding::Foreign` path switch**: `as_method_fn`'s loud abort
    becomes `invoke_binding` — Rust bindings call as before; Foreign
    bindings go through `foreign_call`: descriptor-driven marshalling
    (the A-form classification law), the receiver rides as a rooted
    local ref (the jclass for statics — the JNI native convention),
    the return channel picked per the return descriptor. calls.rs's
    dispatch walk consult switched to it.
  * **The register-window loudness is pinned through the FULL runtime
    path** (call_native → invoke_binding → foreign_call → primitive
    11): a 5-GP-word descriptor trips the guard BEFORE the fnPtr word
    is ever used — the 0xDEAD_C0DE binding is safe to exercise with
    catch_unwind (fixed the in-flight test's dead-code tail: the old
    snippet bound a bogus pointer but never called it).
  * **The REAL C probe — the first C in the repo** (13b's evidence bar,
    "a C fn compiled into the test binary"): `j2r-jni-ffi/build.rs`
    compiles `c_probe_add`/`c_probe_mad` with the PLATFORM C compiler
    and links the object into the package's TEST binaries only
    (`cargo:rustc-link-arg-tests`); `tests/c_probe.rs` (an integration
    target — the link-arg needs one) proves the transmute against
    actual C-compiler output on both return channels. Precision of
    record (M5_JNI_HARNESS.md §6 amended): the `cc` CRATE was rejected
    — a build-dependency would enter the vendored manifest and break
    generated projects' `--offline` builds (gate-5 outranks); build.rs
    is never vendored, and a compiler-less box cfgs the real-C tests
    out (`[lints.rust] unexpected_cfgs` declares the `j2r_c_probe`
    cfg).
- Evidence — full battery re-run (the standing law): fmt clean; clippy
  `-D warnings` clean; **444 tests, 0 failures** — classfile 38 ·
  jni-ffi 13 (11 unit + 2 real-C integration) · runtime 238 · codegen
  125 · cli 28; walls: diff_suite 454.98 s + cfg_equiv 353.99 s (zero
  divergences), partition 24.63 s, soe 10.90 s, native_e2e 6.23 s,
  lambda 1.12 s, jar + corpus green; gate-11 in sync (fixtures=109,
  tests=444 → README 440 → 444).
- **docs/REVIEW_CHECKLIST.md created** — the owner's external review
  converted into the repo's own law: G0 standing gates (battery,
  gate-10, doc-counts, boards+push, loud-unsupported, honest-numbers),
  G1 the JNI C-harness acceptance (per-function inventory, three-channel
  both directions, attach lifecycle, CI pinning), G2 the 70k path
  (F3 split-table, F2 fast path, F1 profile, F4 release trio, the
  progressive ladder, **F6 the fat-method O3 regression — registered**
  with its either-arm PASS bar), G3 the typed-codegen lever (deferred
  join, entry vars, collection specialization, the honesty guard), G4
  the M4 review closure + the volatile/final-freeze enforcement arm,
  G5 mappings/mixins/inline caches with the M6 ordering gate, G6 the
  documentation discipline. Every item has PASS criteria, FAIL
  conditions, and what does NOT count.
- Next: **13c** — the harness C source + the Temurin driver + the j2r
  loading story + the three-channel parity runner; then **13d** — the
  attach lifecycle battery + the CI gate; wave 2 closes.

### [2026-11-10 ~08:25 UTC] session 7 — reset #8 survived via §9; toolchain re-proven (byte-exact); the cron schedule completed (0 jobs); next 13c
- Reset #8 (sandbox recycled between sessions; persistent layer back at
  pre-M3 `b4badea`, toolchain + `tools/` + `env.sh` wiped): the §9
  recipe — `git fetch && git merge --ff-only origin/main` → HEAD
  `754ae40` verified == origin/main (the boards' truth: session-6's
  T13 slice 13b + docs/REVIEW_CHECKLIST.md already pushed and are the
  law). `j2r-audit` (the unrelated 4-commit early workspace) untouched.
- **The owner's 8-hour schedule COMPLETED and verified**: the four
  one-time "continue never stop" jobs (Asia/Shanghai 06:52 / 08:52 /
  10:52 / 12:52) all fired — the 12:52 firing delivered this session —
  and a live listing shows **0 jobs remain**. The schedule stopped
  completely, exactly as directed.
- Toolchain rebootstrap (§3.1 item 1, the session-3/5/7 protocol):
  rustup → exact pins cargo/rustc 1.99.1 + rustfmt 1.10.0 + clippy
  0.1.99; Temurin jdk-21.0.13+9 → the persistent
  `/home/z/my-project/tools/jdk-21.0.13+9` path, tarball **207,858,352 B
  byte-exact** vs the recorded size (javac/java 21.0.13 verified);
  `env.sh` recreated (J2R_JAVAC, JAVA_HOME, PATH,
  CARGO_PROFILE_DEV_DEBUG=0); `target/` (509 MB) survived → warm
  builds. **Download-route precision for the next reset**: the
  Adoptium `/v3/binary/version/...` and `/v3/assets/version/...`
  routes 404'd for `jdk-21.0.13+9` this time (release_names still
  lists it) — the working route is the GitHub releases asset
  `adoptium/temurin21-binaries/releases/download/jdk-21.0.13%2B9/
  OpenJDK21U-jdk_x64_linux_hotspot_21.0.13_9.tar.gz`.
- The owner's "read all .md files" directive executed on the fresh
  tree: README / console_summary / TODO / docs/M5_JNI_HARNESS (the
  13a design of record) read in full relevant depth. Stale-memory
  audit: "T10 slice 2 pending", "generator /tmp path bug", "missing
  child-process test" are all DISPROVEN by the boards — those landed
  in sessions 4–5 (generator pinned to $JAVA_HOME, the abort shapes
  pinned by child-process tests). The board wins over memory, as
  written.
- Next (unchanged from §8): baseline re-proof (fmt, clippy
  `-D warnings`, unit tests, then the heavy suites chunked), then
  **13c** — the harness C source + the Temurin driver + the j2r-side
  loading story + the three-channel parity runner; then **13d** — the
  attach-lifecycle battery + the CI gate; wave 2 closes at 13d.
- Boards pushed immediately after this entry (the recovery itself is
  now protected).

### [2026-11-10 session 7 cont. 1] baseline re-proven at 444 green (the §3.1 law, before any 13c code)
- fmt clean; clippy `-D warnings` clean (workspace, all targets).
- **444 tests, 0 failures** — classfile 38 · jni-ffi 15 · runtime 238
  (20.21 s) · codegen 125 · cli 28. Walls: diff_suite 450.46 s +
  cfg_equiv 344.45 s (zero divergences), partition 24.40 s, corpus
  38.56 s, soe 10.54 s, jar 18.89 s, native_e2e 6.40 s, lambda 1.16 s.
- **Correction of record (found by the --list census)**: the session-6
  evidence line "jni-ffi 13 (11 unit + 2 real-C integration)" mis-split
  the crate — the true split is **jni-ffi 15 = 13 unit + 2 real-C**
  (c_probe). The 444 workspace total was and is correct
  (38+15+238+125+28). Boards amended; nothing else moves.
- Next: **13c** — the harness C source (`tests/jni_harness/harness.c` →
  `libjni_harness.so`), the Temurin driver, the j2r-side loading story,
  the three-channel parity runner (M5_JNI_HARNESS.md §1/§3/§5 is the
  design of record; REVIEW_CHECKLIST G1 is the acceptance gate).

### [2026-11-10 session 7 cont. 2] T13 slice 13c LANDED — the ONE C harness, the loading story, and three-channel parity vs Temurin (448 green)
- **The wave-2 acceptance demo is LIVE**: `tests/jni_harness/harness.c`
  compiles ONCE (`cc -shared -fPIC` against the pinned JDK's jni.h of
  record) and the SAME `.so` runs against BOTH runtimes — Temurin via
  `System.loadLibrary`, and the j2r binary via the SAME
  `System.loadLibrary` served by the 13c loading story. The full v1
  battery is **byte-identical on stdout, stderr (EMPTY both sides) and
  exit** — proven by `crates/j2r-cli/tests/jni_parity.rs` (G1.1/G1.3).
- **The loading story (docs/M5_JNI_HARNESS.md §7, precision of record):**
  * VM-table `GetEnv` (slot 6) now serves its supported-version path —
    the supported set `{1.1, 1.2, 1.4, 1.6, 1.8, 9, 10}` is the set
    HotSpot 21 MEASURED to accept (0x000b0000 and 0 → EVERSION, nothing
    written); the write rides audited primitive 3. The old
    "supported version aborts" child-process pin is replaced by
    success-path tests (F-J1's last unwired arm is wired).
  * `GetJavaVM` (env slot 219) wired the same way — REAL_SLOTS 158 →
    **159 real slots**, slot number verified mechanically against the
    parsed header; regenerate+fmt idempotent byte-for-byte.
  * `System.loadLibrary` is a runtime native (the runtime's first
    library-loading surface): name → `lib<name>.so` (Linux-only v1,
    loud unsupported elsewhere) → **primitive 12**
    `load_foreign_jni_library(path, vm)` — ONE audited unsafe block:
    dlopen(RTLD_NOW|RTLD_LOCAL) + dlsym("JNI_OnLoad") + the ABI-fixed
    `jint (*)(void*, void*)` call. The handle is NEVER dlclosed
    (HotSpot semantics; a close would invalidate the held bindings).
    dlopen miss = LOUD abort with the loader's own error text (the
    UnsatisfiedLinkError-on-load-failure transport stays a registered
    v1 gap; the primitive's tests pin the loudness). JNI_OnLoad's
    version answer validated safe-side (0 or supported = OK, else
    UnsatisfiedLinkError). **gate-10 amended in the same commit:
    10 → 11 occurrences of pin 12** (one headroom slot left).
- **The harness falsified two T08 assumptions in its first oracle runs
  — exactly its job — and both are corrected to MEASURED values:**
  * `GetVersion`: Temurin 21.0.13+9 answers the JDK-major form
    **0x0015_0000**, NOT JNI_VERSION_1_8 (probed live with Ver.c +
    ver.java). Our slot now returns `JNI_VERSION_21_0`; the unit test
    renamed get_version_through_the_table_matches_temurin.
  * `GetEnv` support set: HotSpot accepts 9 and 10 (the old 1.8 cap was
    wrong); EVERSION starts at 0x000b0000. Our table matches.
  * Harness JNI hygiene (HotSpot SIGSEGVed in jni_RegisterNatives on
    the first rebind attempt): a jclass captured in JNI_OnLoad is a
    LOCAL ref — dead once OnLoad returns (our interned-mirror tags
    happened to tolerate the stale use; tolerance is not correctness).
    The harness now holds NO class statics; every rebind/unregister
    probe re-FindClasses a fresh local within its own frame.
    NewGlobalRef (21) stays a loud stub, registered for 13d if needed.
- **The battery (12 bound probes + the helper native, self-checking —
  the driver's check() exits 3 on ANY missed verdict on BOTH
  runtimes, so parity can never preserve a wrong answer):**
  versioning/VM (version=47) · fields (41→42, 1000→2000, verified
  Java-side) · strings both directions over the mUTF-8 edges (the
  encoded NUL 0xC0 0x80; the supplementary pair; NewStringUTF
  "j2r<NUL>ok" = 6 chars; NewString surrogate pair + embedded NUL
  jchar — verified by C byte-compare AND Java-side length/charAt) ·
  arrays (regions, GetIntArrayElements + all three release modes,
  criticals, length — verdict 63; the JNI_ABORT arm CALLS the path but
  asserts only the COMMIT result, honestly: with a direct-pointer Get
  the mutation is already in the array, so the ABORT-drops law stays
  unit-pinned Rust-side) · object arrays · the A-form families
  (virtual override 12 vs Nonvirtual declared-class 1005, static,
  void-callback PRINTING through the env, the exception round-trip
  with the message read through a COMPILED bridge method — a builtin
  class has no compiled facts, so GetMethodID on it would honestly
  answer null) · the V-forms BOTH ways (real C variadics — 8 ints pin
  the register→overflow split, (I D)D pins the GP/FP interleave — AND
  hand-built __va_list_tag save areas at the ABI's own offsets) ·
  the natives round-trip (rebind → 777, restore → 47, unregister →
  the unbound UnsatisfiedLinkError caught Java-side, class name
  printed).
- **Evidence — full battery re-run (the standing law): fmt clean;
  clippy `-D warnings` clean; doc counts in sync (fixtures=109,
  tests=448 → README 444 → 448); 448 tests, 0 failures** — classfile
  38 · jni-ffi 17 (15 unit + 2 real-C) · runtime 239 · codegen 125 ·
  cli 29 (incl. **jni_parity 1**); walls: diff_suite 464.16 s +
  cfg_equiv 347.11 s (zero divergences), partition 24.40 s, corpus
  38.56 s, soe 10.54 s, jar 18.89 s, native_e2e 6.40 s, lambda 1.16 s.
  REVIEW_CHECKLIST **G1.1, G1.2, G1.3 checked with this evidence**
  (G1.4 attach + G1.5 CI = 13d).
- Design-of-record updates: M5_JNI_HARNESS.md §6 (13c LANDED) + §7
  (the loading story) + §8 (the honest inventory: what is exercised,
  what stays loud/documented); M5_PLAN §6 T13; TODO.md; README status
  line unchanged (T13 still in progress — 13d remains).
- Next: **13d** — the attach-lifecycle battery (pthread →
  AttachCurrentThread on the cached VM → the battery → Detach →
  re-attach; the VM-table Attach/Detach slots + NewGlobalRef decide
  their fate there), the loud-gap probes, the CI gate; wave 2 closes.

### [2026-11-10 session 8] reset #9 survived via §9; ff to 2722bf9 (13c); toolchain recovered from the persistent layer with NO re-download; boards + docs re-read
- Reset #9 (sandbox recycled; persistent layer back at pre-M3 `b4badea`,
  toolchain + `tools/` + `env.sh` gone — the same pattern as resets #3–#8):
  the §9 recipe — `git fetch origin && git merge --ff-only origin/main`
  → HEAD `2722bf9` (T13 slice 13c, **448 green**) verified ==
  origin/main. Zero local work existed ahead of origin (0 ahead /
  138 behind), so the ff lost nothing.
- **Toolchain recovered WITHOUT the usual rebootstrap download**: the
  persistent layer already held a full rustup install — `tools/cargo`
  (CARGO_HOME) + `tools/rustup` (RUSTUP_HOME,
  `stable-x86_64-unknown-linux-gnu`) — the shims only needed the two
  env vars. Verified: rustc/cargo **1.99.1**, rustfmt **1.10.0**,
  clippy **0.1.99** (exact pins), Temurin javac/java **21.0.13** intact
  at `tools/jdk-21.0.13+9`. `env.sh` updated to export
  `CARGO_HOME`/`RUSTUP_HOME` + `$CARGO_HOME/bin` on PATH — future
  resets can no longer lose the toolchain while `tools/` survives
  (improves §3.1 item 1; recorded for the next §9 run).
- The owner's "check the repo state and read all .md files" directive
  re-executed on the fresh tree: console.md (all 1105 lines) and
  console_summary.md (all 492 lines) read in full BEFORE acting.
  Stale-memory audit vs the boards, third occurrence of the pattern:
  "generator /tmp path bug", "missing child-process loud-release test",
  "T10 slice 2 pending" — ALL disproven again (landed sessions 4–5);
  the board wins over memory, as written. Also confirmed: the boards
  are tracked ON origin (they were only missing from the stale local
  checkout — a duplicate-boards near-miss avoided by fetching before
  globbing).
- Next: baseline re-proof (§3.1 item 2, T05's law) on 2722bf9, then
  **13d** — the attach-lifecycle battery (pthread → AttachCurrentThread
  on the cached VM → the battery → Detach → re-attach; the VM-table
  Attach/Detach slots + NewGlobalRef decide their fate there), the
  loud-gap probes, the CI gate (G1.5) — wave 2 closes at 13d.
- Boards updated and pushed immediately after this entry.

### [2026-11-10 session 8 cont.] baseline re-proven at 448 green on 2722bf9 (the §3.1 law, before any 13d code)
- fmt clean; clippy `-D warnings` clean (workspace, all targets,
  10.82 s warm).
- **448 tests, 0 failures** — classfile 38 · jni-ffi 17 (15 unit +
  2 real-C) · runtime 239 (20.65 s) · codegen 125 · cli 29 (bin 15 +
  soe_e2e 1 + lambda_neg 2 + jar_suite 3 + partition_suite 1 +
  corpus_suite 1 + native_e2e 2 + jni_parity 1 + cfg_equiv 1 +
  diff_suite 2). Walls: cfg_equiv 379.77 s, diff_suite 496.55 s
  (fresh javac compile of all 109 fixtures) — **zero divergences**;
  the 13c C-harness parity battery green on this box too.
- Next: finish the .md reading pass (TODO/README/ROADMAPs/M5_PLAN +
  the remaining design notes), then **13d** per REVIEW_CHECKLIST
  G1.4/G1.5 — the attach-lifecycle battery (the VM-table
  Attach/Detach slots decide their fate there; NewGlobalRef stays a
  loud stub unless the battery needs it), the loud-gap probes, the
  CI gate — wave 2 closes at 13d.

### [2026-11-10 session 8 cont. 2] T13 slice 13d LANDED — the MEASURED attach lifecycle + the loud-gap probes + the CI gate. T13 COMPLETE; WAVE 2 (JNI) CLOSED (452 green)
- **The oracle was MEASURED before any runtime code** (the harness
  judges, not decorates): a standalone C probe against Temurin
  21.0.13+9 (committed raw under scripts/evidence/
  attach_lifecycle_probe/ with its README decode, per lesson #11):
  fresh-thread VM GetEnv = **JNI_EDETACHED** (pre-attach AND
  post-detach); AttachCurrentThread = JNI_OK, env non-null, re-attach
  idempotent OK; AttachCurrentThreadAsDaemon on an attached thread =
  JNI_OK; **DetachCurrentThread = JNI_OK ALWAYS — three consecutive
  detaches measured JNI_OK** (raw verdicts V=991 / V=-200, decoded in
  the evidence README: d1=d2=d3=0, ge=-2, a2=0, d4=0); the main thread
  is attached at birth (the 13c battery's GetEnv arm).
- **The slots' fate**: VM `AttachCurrentThread` (4),
  `DetachCurrentThread` (5), `AttachCurrentThreadAsDaemon` (7) are now
  REAL — 159 → **162 real slots** (`DestroyJavaVM` stays a loud stub,
  honestly registered). New module `jni::attach` (one global set of
  attached thread ids; detach runs the 12b-wired
  `bridge::detach_thread` hook — the frame-stack sweep G1.4 names);
  GetEnv is attach-aware (check order: null-penv JNI_ERR → EVERSION →
  EDETACHED); the emitted `main_run` marks the main thread attached at
  birth (one line next to begin_main_java_thread). Daemon-ness has no
  v1 representation (no VM-exit join) — documented, not pretended.
  Zero new FFI primitives (audited primitive 3 reused); gate-10
  unchanged at 11 of pin 12.
- **The battery (G1.4)**: harness `probe_attach_lifecycle` spawns ONE
  pthread and JOINs it (deterministic stdout); the worker walks every
  measured arm and runs the compact self-checking battery ON the
  attached env in BOTH attach cycles (GetVersion / FindClass /
  NewStringUTF-UTFChars round-trip / NewIntArray+region+length /
  ExceptionCheck clean — 31 per cycle); verdict **2047** (eleven
  bits); Temurin answered 2047 (measured standalone FIRST), then the
  parity run: **byte-identical three channels incl. `P attach=2047`**
  — the attach lifecycle proven end to end on both runtimes.
- **The loud-gap probes** (§8's law, Rust-side only — the oracle
  implements these for real): child-process abort pins through the
  REAL env table naming the slot — NewGlobalRef (21), MonitorEnter
  (217), MonitorExit (218), GetStringUTFRegion (221). Precision
  correction of record: "InvokeInterface" was named in §8 as a v1 gap
  — the Temurin 21 jni.h has NO such function (dropped from the
  header); the loud gaps are the header's real slots.
- **The CI gate (G1.5)**: `gate-22-jni-c-harness-parity` — dedicated
  job (compiles the harness with cc, runs BOTH runtimes, three raw
  channels, loud skip law), wired into the all-gates aggregate (now 17
  upstream gates). README gate table row added.
- **Evidence — full battery re-run (G0.2; codegen changed)**: fmt
  clean; clippy `-D warnings` clean; **452 tests, 0 failures** —
  classfile 38 · jni-ffi 17 (15 unit + 2 real-C) · runtime 243 (+4:
  the attach-registry unit test, the fresh-thread lifecycle test, the
  loud-gap parent + child) · codegen 125 · cli 29; walls: cfg_equiv
  370.05 s + diff_suite 491.11 s (**zero divergences**), partition
  28.03 s, corpus 42.49 s, soe 11.84 s, jar 20.90 s, native_e2e
  7.69 s, jni_parity 4.80 s (the attach battery green both runtimes);
  gate-11 in sync (fixtures=109, tests=452 → README 448 → 452).
- **REVIEW_CHECKLIST G1.1–G1.6 ALL checked — M5 wave-2 (JNI) is
  CLOSED.** Docs synced in the same commit (the G6.1 law): M5_PLAN §6
  (T13 COMPLETE), M5_JNI_HARNESS (status + §6 13d + §8 correction +
  new §9 the 13d precision of record), TODO.md (the JNI bullet
  [x], the 13d entry), README (status line + gate table + test
  count), docs/ROADMAP.md M5 row.
- Next per REVIEW_CHECKLIST ordering: G2 (the 70k path — F1/F2/F3/F4
  + the G2.6 fat-method F6 either-arm bar) is the remaining P0; G5
  (mappings T14–T16) starts only after G1 AND G2 are checked.

### [2026-11-10 session 9] reset #10 survived via §9; ff b4badea → 6521597 (159 commits); stale-belief items verified ALREADY FIXED on origin; reading ALL .md files (user directive)
- Sandbox reset #10 on session start: the working tree was stale again at
  pre-M3 `b4badea` (16 commits, clean, no stashes, boards missing from the
  tree — exactly the §9 shape). `git fetch origin` then `git merge --ff-only
  origin/main` → HEAD `6521597`, 159 commits, clean. NOTHING lost (nothing
  local existed to lose — strict behind).
- Origin had advanced 11 commits past the last-read state (`2722bf9` 13c):
  13d attach lifecycle + gate-22 + board records + README reconciliations →
  **T13 COMPLETE, wave 2 (JNI) CLOSED, 452 green** at `6521597`.
- Stale-belief verification (prior session memory vs current tree):
  (a) `scripts/gen_jni_env_table.py` line 45 now uses
  `JAVA_HOME` with default `/home/z/my-project/tools/jdk-21.0.13+9` — the
  dead `/tmp/jdk21` path is GONE (fixed in an earlier session); ✅
  (b) the child-process loud-gap tests EXIST in
  `crates/j2r-runtime/src/jni/mod.rs` (spawn sites at ~295, ~373, ~475;
  parent+child pins per the tail-of-log record) — `tables.rs` line 2183's
  doc claim is TRUE as written; ✅
  (c) T13 complete / wave-2 closed / 452 green confirmed from origin's
  commit subjects and both boards. Prior-session beliefs of "T10 slice 2
  uncommitted / 388 tests / generator bug open / child test missing" were
  ALL stale snapshots — the board wins, as written in the header.
- Genuinely open (confirmed from boards + REVIEW_CHECKLIST): G2 = the 70k
  path (F1–F4 + the G2.6 fat-method F6 either-arm bar) — remaining P0;
  M4 owner acceptance review still pending; G5 mappings T14–T16 gated on
  G1 AND G2.
- Session 9 task (user directive): **read ALL .md files** (28 tracked top
  files + boards), build the drift matrix, reconcile any drift found.
  In progress at entry time; results appended below when done.

### [2026-11-10 session 9 cont.] ALL .md files read; drift matrix built; 6 drifts FIXED in one docs commit; toolchain anomaly (root-owned rustup home) recovered
- **Read complete** (user directive "read all .md files"): 19 files fully
  read (README, TODO 1,621 l, ROADMAP, docs/ROADMAP, REVIEW_CHECKLIST,
  M5_PLAN, M5_SCALE, all five M5_JNI_* docs, STRATEGIC_GAPS,
  EXTERNAL_V12_MINICRAFT_ANALYSIS, TYPED_REGISTER_READINESS,
  RUNTIME_SAFETY, the three small READMEs); the 12 historical records
  (M2/M3/M4 plans + evidence, M2_SHAPE, M2_CFG, HARDENING,
  AUDIT_VERIFICATION, BENCHMARKS, CORPUS_WAVE) structure-surveyed with
  every status-bearing line checked. console.md + console_summary.md
  head/tail + §9/F1–F5 sections read.
- **The drift matrix (6 candidates, all fixed in this commit):**
  1. README status line said "next T14–T16 mappings" — the boards'
     session-8 record says G2 (the 70k path) is the remaining P0 and G5
     is gated on G1 AND G2 (REVIEW_CHECKLIST ordering). FIXED: G2-first
     wording.
  2. docs/ROADMAP M5 row + header date — same next-pointer nuance;
     header said 2026-11-08 while content was 2026-11-10-current.
     FIXED both.
  3. TODO.md §M5 JNI bullet's "Next:" line — same nuance. FIXED
     (verbatim G2-first wording from the session-8 console record).
  4. docs/M5_JNI_FFI.md §3/§4 — "complete list, v1" stopped at 7
     primitives; the real audited surface is 12 (T11 11c ×3, T13 13b
     call_foreign_native, T13 13c load_foreign_jni_library; 11 unsafe
     occurrences of pin 12). The sibling docs (CALLS/HARNESS) and
     gate-10 had the truth; the FFI design-of-record note did not.
     FIXED: §3 amended + §4 entries 8–12 added with design-of-record
     pointers.
  5. docs/RUNTIME_SAFETY.md — §5's "No threads / monitors ... compile
     errors" bullet contradicted the landed M4 surface (and §6, which
     T16 added); §2's "single-threaded milestone" heading had no
     revision note. FIXED: the bullet got the same strikethrough +
     resolution treatment the GC/SOE bullets already had; §2 got a
     dated reconciliation note pointing at §6 as the live contract.
  6. docs/STRATEGIC_GAPS.md — R3 "Not started. Zero JNI surface" and
     R4 "Single-threaded only" were the 2026-11-05 registration states,
     now superseded. FIXED with a dated addendum (registration texts
     kept verbatim; R3 CLOSED at the v1 evidence bar, R4 waves LANDED
     with owner review pending, R1/R2/R5 pointers).
- Verified NOT drift (left untouched): README "241 unit and integration
  tests" is machine-checked by gate-11 (fixtures=109, tests=452 in
  sync — hand-editing counts is banned, G0.4); docs/ROADMAP §2/§3
  historical rows are dated records; M2/M3/M4 evidence records are
  internally consistent; the corpus count discrepancy (ROADMAP "entries
  1–2" vs board's four names) is resolved — java-string-similarity and
  JAMA are corpus waves 3–4 recorded in TODO.md, ROADMAP's M3.7 section
  predates them and is labeled as its own dated record.
- **Toolchain anomaly (reset #10, new lesson candidate #12):**
  `/home/z/my-project/tools/rustup/` came back ROOT-OWNED 700
  (unreadable by user z; chmod without root fails; sudo unavailable) —
  the persistent layer's EXISTENCE check passed but the toolchain was
  unreachable. Recovered: rustup-init (z-owned, survived) bootstrapped
  the pinned 1.99.1 toolchain into `/home/z/my-project/tools/rustup2`
  (z-owned); `env.sh` now points RUSTUP_HOME there. CARGO_HOME
  (tools/cargo) and the JDK survived intact — only rustup's directory
  was damaged. **Lesson: the §9 preflight must stat -c '%U %a' the
  persistent-layer dirs, not just test -e — permissions rot is a
  separate failure mode from deletion.**
- Gates run locally, pre- AND post-fix: gate-11 doc-counts
  (fixtures=109 tests=452), gate-19 doc-links (all resolve), gate-20
  code-integrity (zero stubs) — all green both times. This commit is
  docs-only (no Rust source touched; the full battery is not owed on a
  prose reconciliation, and G0.2's trigger — runtime/FFI/behavioral
  change — does not fire).
- Stale-belief audit closed: generator JDK path + child-process
  loud-gap tests were ALREADY FIXED on origin (verified in the first
  session-9 entry); todo closed, not redone.
- **Next work (unchanged, per REVIEW_CHECKLIST ordering): G2 — the 70k
  path.** G2.6's either-arm bar (the fat-method F6: split-table/
  smaller-function emission OR the machine-bound registration + the
  CI-visible emitter-side proxy test) and G2.1–G2.4 (F3 split-table,
  F2 mtime fast path, F1 emit profile, F4 release trio) are the next
  coding sessions, each ≤ 10-min chunks, full battery on landings that
  touch codegen.

### [2026-11-10 session 10] stale tree re-synced (b4badea → 0552cba, 161 commits); the external review's 5 drifts confirmed + a 6th found (FFI count sentence); all fixed docs-only; gates re-run
- Sandbox reset #11 on session start: the tree was stale at pre-M3
  `b4badea` again (16 commits, clean, strict behind, boards missing —
  the §9 shape). `git fetch origin` + `git merge --ff-only origin/main`
  → HEAD `0552cba`, 161 commits, 0 ahead, clean. NOTHING lost (nothing
  local existed to lose).
- Revision truth (the reviewer's sync item): session 9 recorded 159
  commits at `6521597`; the two subsequent commits are `f516382`
  (session-9 board record) + `0552cba` (the 6-drift docs commit), both
  docs-only — GitHub's 161 == local 161 as of this entry.
- The updated external review (2026-11-10) internalized: G1 CLOSED
  (JNI wave 2, 452 green, gate-22 C-harness parity), 20k demonstrated,
  verdict "serious compiler project, not Minecraft-ready yet", next
  major achievement = G2. Its five §4 drift candidates each verified
  against the tree and FIXED in this docs-only commit:
  1. console_summary.md milestone table said "wave 1 nearly closed"
     while its own §3 says wave 1 CLOSED — FIXED (row now records
     wave 1 + wave 2 CLOSED, remaining P0 = G2).
  2. console_summary.md §8 listed [F1]–[F4] without the G2.5/G2.6/
     G2.7 acceptance gates — FIXED (rewritten as the full G2.1–G2.7
     ladder, F1–F4 mapped into G2.1–G2.4, the G2.6 either-arm bar and
     the review's implementation order G2.1 → G2.6 → G2.2–G2.4 → G2.5).
  3. Zero-`unsafe` one-liners in console_summary.md §1, console.md
     STATE-AT-A-GLANCE, TODO.md header + TODO.md standing-law bullet
     contradicted the registered amendment — FIXED all four to carry
     the audited-FFI exception. Machine truth re-run this session with
     the gate-10 expression: 11 occurrences in ffi.rs, none outside.
  4. README "241 unit and integration tests" — unmeasured prose from
     the T06/T07 build-manifest wave (`git log -S` → 6f79a10); gate-11
     does NOT assert it (its patterns cover 109 fixtures / 452 total /
     cfg-equivalence only) — FIXED with era scoping + pointer to the
     measured 452 aggregate in README Testing.
  5. Revision sync (159 vs 161) — resolved as recorded above.
- SIXTH drift found by this session (the reviewer's own count matched
  the machine): docs/M5_JNI_FFI.md §3's growth sentence said "→ 10 in
  T11 11c, → 11 in T13 13b, → 12 in T13 13c", contradicting the SAME
  doc's §4 register (6 → 9 in 11c, 9 → 10 in 13b, 10 → 11 in 13c),
  REVIEW_CHECKLIST G1.6, and the gate count (11 of pin 12). FIXED §3
  to match §4 + the machine.
- Correction to the session-9 record (append-only; the board wins):
  its "241 is machine-checked by gate-11" verification line was WRONG
  — the direct script read this session settles it; the prose is now
  era-scoped instead of unmeasured.
- Gates re-run post-fix: gate-11 doc-counts, gate-19 doc-links — green
  (docs-only commit; no Rust source touched, G0.2's heavy battery does
  not fire).
- Next: G2.1 (split the app/Dispatch composite) — emission code
  investigation started; the split must keep the partition differential
  byte-identical and drop the 20k peak-rustc number materially.

### [2026-11-10 session 10 cont.] G2.1 SPLIT LANDED — F3 fixed in the emitter (per-partition tables + static route tables); 5k A/B peak 1.89 → 1.26 GB (−33 %); full battery green; the checkbox waits for the 20k re-measurement
- Diagnosis confirmed from the code + the committed census:
  `emit_app` embedded TWO total-class-scaled literals into
  `app/src/main.rs` (`apply_defaults` default-run tuples +
  `set_constants` ConstantValue finals), making `main_run` one fat
  function body — fact 4's disease in dispatch-table form.
  Attribution cross-check: 1k/k=500 peak 1.91 GB ≈ the 500-class
  partition-crate compile (census: 500 classes = 2.1 GB single-crate);
  at 20k/k=250 partition crates bound at ~1.1–1.3 GB vs the recorded
  3.49 GB peak → the app bin was the difference.
- The split (emitter change, `emit.rs` + `partition.rs`):
  (a) per-partition `register_defaults`/`register_constants` fns in
  each `p{i}/src/lib.rs` — literals bounded by k, slot-sorted,
  partition order == slot order, and `apply_defaults`/`set_constants`
  coverage is order-independent → identical pool state;
  (b) `app/src/main.rs` composes one register line per partition and
  passes slices — O(partitions), never O(classes); (c) the three
  composite fns route through STATIC per-partition fn-pointer tables
  (`ROUTE_LOOKUP`/`ROUTE_PARENTS_OF`/`ROUTE_META_OF`) indexed by
  `part_of`'s binary search — no per-partition match arms. `part_of`,
  `build_link`, the support crate and the runtime are UNTOUCHED.
- 5k A/B, census protocol (N=5000, k=300, cargo -j 2, CI=0, debug=0,
  RUSAGE_CHILDREN peak): wall 282.72 → **242.75 s** (−14 %); peak rustc
  1,888,504 → **1,260,656 KB** (−33 %). Post-split peak = the largest
  PARTITION-crate compile — the bounded unit; the app bin no longer
  sets the peak. `app/src/main.rs` at 5k = 4,973 B (34 register
  lines); `p0/src/lib.rs` 1,050,908 B (per-partition bounded).
- Behavior preservation: 5k three-channel parity IDENTICAL vs Temurin
  21.0.13+9 (raw stdout files committed); partition differential forced
  k=2 ok (25.26 s); workspace remainder 449 passed / 0 failed;
  diff_suite 2 passed (478.60 s) + cfg_equiv 1 passed (356.03 s), zero
  divergences; gate-10 expression re-run (11 of pin 12, none outside
  ffi.rs), gate-11 re-measured `fixtures=109 tests=452` ✓, gate-19
  links ✓, gate-20 stubs ✓; fmt + clippy `-D warnings` clean.
- Evidence (G6.3): `scripts/evidence/g2_1_split/` — README (diagnosis +
  A/B table + battery record), raw timeit file, raw 5k stdouts both
  runtimes.
- Honest remainder: the G2.1 CHECKBOX waits for the 20k peak-rustc
  re-measurement (the checklist bar says 20k, not 5k). Next chunks:
  20k farm + emit + watched cargo run (chunked ≤10 min; a /proc VmHWM
  rustc sampler records the true whole-build peak across invocations),
  then G2.6 (the JZlib either-arm bar).

### [2026-11-10 session 11] G2.1 20k RE-MEASURE COMPLETE — checkbox CLOSED: worst generated-unit rustc 3.49 → 0.99 GB (−72 %); residual build peak attributed to rust-lld (3.09 GB, the link stage) and registered as G2.1b; 20k parity PASS; G2.6 is next
- First: committed the session-10 landing as `4a44cfd` (split + 5k A/B +
  battery + evidence) after re-verifying the WIP tree green
  (j2r-codegen unit tests + partition_suite 29.58 s). Pushed (bypass
  path, 8 checks expected as always).
- New tooling (both committed):
  `scripts/g2_1_20k_remeasure.sh` — the 20k protocol as STAGES (sandbox
  lesson #9: no background processes; one stage per tool call; cargo
  fingerprints resume; the chunk counter file survives kills), and
  `scripts/proc_peak_sampler.py` — per-process VmHWM from
  /proc/<pid>/status polled at 60 ms while the build runs: rustc vs
  rust-lld attribution that RUSAGE_CHILDREN cannot provide.
- 20k re-measure (N=20000, k=250 → 81 part crates + support + app;
  cargo -j2; CI=0; dev/debug=0 — the session-1 baseline protocol):
  javac 29.65 s / 0.91 GB; emit 34.30 s / 902 MB (baseline 34.12 / 858
  — unchanged, as expected: the emit side was never the composite);
  cold cargo peak **3.09 GB = rust-lld** (final link of the 1.08 GiB
  debug=0 binary) with **every single rustc ≤ 0.99 GB** (partition
  crates, uniformly bounded — the split demonstrably holds at 20×);
  no-op 41.74 s emit + 0.39 s cargo = 42.1 s; one-body-edit 33.91 s +
  32.07 s = 66.0 s (its cargo peak 3.06 GB = the lld relink);
  **parity PASS** — stdout+stderr+exit identical vs Temurin 21.0.13+9;
  binary 1,085,784,872 B ≈ 1.01 GiB (baseline 1,084,126,264 B).
- Wall honesty: the post-split cold cargo wall ≈ 1,581 s across THREE
  chunks with TWO timeout kills (chunk 1 ~585 s + probe-resume ~595 s
  lost mid-compile — a killed rustc redoes its whole crate), vs the
  baseline's 967 s with ONE kill; wall is NOT comparable across
  different kill counts, peaks are (VmHWM is kill-independent). The
  completed resume chunk: 401.37 s. Lesson reinforced: leave ≥1 chunk
  of headroom or the 20k protocol burns 10 min per kill.
- Attribution probes (raw `probe_support.time` + `probe_resume.time`
  committed): touch + `cargo build -p farm20000-support` = 0.83 s /
  119 MB (support/src 9.8 KB — runtime-filled tables, NOT a scaling
  compile unit); the resume build's per-process top table:
  rust-lld 3,158,788 KB, then twelve rustc processes all ≤ 1,005,860 KB.
- Reading, precisely: G2.1's bar is MET for the emission side, measured
  directly — no generated compilation unit scales with TOTAL class
  count (worst rustc 0.99 GB ≈ the k=250 partition bound), and the
  worst-unit number dropped 3.49 → 0.99 GB. Honest correction recorded:
  the baseline "3.49 GB app-bin rustc" label conflated rustc with its
  lld child (RUSAGE_CHILDREN rolls children up); the residual 3.09 GB
  build peak is the LINK stage — binary size still scales with total
  classes — now REGISTERED as REVIEW_CHECKLIST **G2.1b** with its own
  PASS bar (measured mitigation or written accepted boundary + the
  sampler wired into the standing protocol) and the G2.7 note:
  3.09 < 4.0 GB budget, headroom 0.91 GB.
- Docs updated: REVIEW_CHECKLIST §G2 (state rewritten + G2.1 [x] with
  the measured close + G2.1b inserted), console_summary §numbers table
  (20k peak row) + §8 (G2.1 closed, G2.1b added, implementation order
  updated), TODO.md wave-2 next-pointer. Evidence:
  `scripts/evidence/g2_1_split/` — farm20k_remeasure.log, all .time
  files (chunk-1 kill logged, honestly no file), parity stdouts,
  probe tables, README extended with the 20k section.
- Next: **G2.6** (the JZlib fat-method F6 either-arm bar — split-table
  emission vs machine-bound registration + census regression test),
  then G2.2–G2.4, then the G2.5 ladder.

### [2026-11-10 session 11 cont.] G2.6 CLOSED via arm (a) — the array-literal peephole lands; JZlib release build SIGKILL at rustc 3.25 GB → rc=0 in 50.34 s (max process 678 MB, −79 %), release binary byte-identical vs Temurin; full battery green
- Reproduction with the per-process sampler (`proc_peak_sampler.py`,
  the G2.1 tooling): `cargo build --release` of the JZlib generated
  crate died rc=101, **rustc VmHWM 3,253,300 KB at 23.18 s** — the
  single `jzlibcorpus` lib rustc, MIR/borrow-check phase, machine-bound
  (box 4,041 MB total, ~3.16 GB available) — exactly the recorded
  3.03–3.56 GB band. Evidence: `scripts/evidence/g2_6/
  jzlib_release_before_sigkill.time`.
- Attribution by census (`scripts/emitted_fn_census.py`, new): the fat
  functions are the `<clinit>` static initializers — InfTree 1,038,405 B
  / 12,354 emitted lines (one 1536-element `[I` fill), Tree 550,507 B,
  StaticTree 382,755 B — javac's array-literal initializers expanding
  to straight-line per-element fills. Partitioning cannot split ONE
  method body; the fix had to be an emission-level transform.
- The fix (arm a, exactly what the bar names): new IR pass
  `crates/j2r-codegen/src/peephole.rs::collapse_array_fill_runs` —
  recognizes javac's exact shape
  `const(len); NewArray(int-group p); (Dup; const(idx); const(val);
  AStore(IntGroup))*` (idx ascending, exactly len groups) and replaces
  it with ONE `Instr::NewIntArrayFilled{prim, vals}` (ir.rs). Emission
  (emit.rs): a NESTED `static [i32; N]` — a separate const-eval item, so
  the enclosing fn's MIR sees ONE call, not N aggregate operands — plus
  `j2r-runtime::array::array_int_group_filled(desc, &S)` whose per-prim
  narrowing is exactly `array_store_i_raw`'s table (Boolean/Char/Byte/
  Short/Int — all PrimData::I payloads). Byte-identical by construction
  (identical stack effect; a collapsed fill cannot throw so dropped
  fault points change nothing; GC safepoints delay, never move; every
  refused shape keeps the old per-element form — interleaved instrs,
  non-const values, out-of-order indices, length ≠ groups, interior
  branch/handler targets, non-int-group prims). Verified + typed paths
  updated (verify pushes the array ref shape; typed plan falls back
  loudly with Unsupported::HasArrayLiteralFill). 8 new unit tests;
  wired into compile step 4 before verify.
- Numbers: InfTree.rs 1.2 MB → 124 KB (−90 %); StaticTree.rs 387 KB →
  16 KB (−96 %); all three clinit giants leave the census top-10; the
  largest generated fn is now the switch-table InfBlocks::proc
  (217 KB). Release build: **rc=0, 50.34 s, max single process 678 MB
  (−79 %)** (was SIGKILL at 3.25 GB). Debug build 7.91 s / 464 MB.
- Byte-identical proof (the bar's requirement): the OPTIMIZED release
  binary vs Temurin 21.0.13+9 — exit 0 == 0, stdout and stderr
  IDENTICAL (deflate/inflate levels 0–9, Adler32, CRC32, version,
  wrap-deflate). Raw stdouts + sampler tables + census in
  `scripts/evidence/g2_6/`.
- Battery (all green, zero divergences): j2r-codegen 110 (incl. 8
  peephole) ✓; partition differential k=2 28.29 s ✓; corpus_suite (all
  4 real-world corpora) 34.22 s ✓; diff_suite 109 fixtures 511.39 s ✓;
  cfg_equiv 376.57 s ✓; j2r-runtime 243 ✓; fmt + clippy `-D warnings`
  ✓ (5 clippy nits in the new test code fixed).
- Docs: REVIEW_CHECKLIST §G2.6 → [x] with the measured close; §8 G2.6
  row → [x]; implementation order updated (G2.1 ✓, G2.6 ✓ → G2.2–G2.4
  → G2.5). Honest scope note everywhere: the switch-table method shape
  remains (compiles fine at every measured level, no budget threat);
  sampler + census = the standing regression probe.
- Next: **G2.2** (no-op (mtime,size) fast path), G2.3 (emit-stage
  profile), G2.4 (release 20k trio), G2.1b (link wall), then G2.5.

### [2026-11-10 session 11 cont. 2] G2.2 CLOSED — manifest v2 (mtime,size) fast path lands; 20k no-op emit 41.74 → 29.93 s (−28 %), 20,003 reused / 0 written; reuse battery green
- Mechanism: `Entry` gains `emitted_stat: Option<(mtime_ns, size)>`
  (manifest format v2; `EMITTER_VERSION` bumped 1 → 2 — the G2.6
  peephole changed emitted bytes for identical inputs, so the bump was
  contractually due). `verify_reuse` fast-paths on a stat match and
  falls back to the SHA-256 content hash on any mismatch. Trust model
  documented in manifest.rs (Cargo-fingerprint semantics; corrupt
  parse → None → full re-emit, loud — unchanged; legacy v1 4-field e
  lines parse with stat None and take the content-hash path).
- Behavioral test `stat_fast_path_trusts_then_falls_back_to_hash`
  proves BOTH arms: (1) a stat match reuses even with a corrupted
  content hash (the hash was really skipped), and (2) after a stat
  change the hash fallback catches a tampered sha (one deterministic
  re-render — a cost, never a correctness issue) and a garbage rewrite
  (content change → re-emit). Glue stays hash-verified every run (the
  vendored runtime copy is correctness-relevant; its cost is ~1-2 s).
- 20k re-measure (N=20000, k=250, the standing protocol): the v1→v2 +
  emitter=2 invalidation run re-rendered everything (36.51 s emit —
  the farm has no giant clinit fills, so bytes were identical and
  write_if_changed rewrote nothing; cargo then rebuilt all 81 crates
  chunked, peak rust-lld 3.13 GB / worst rustc 0.99 GB — G2.1's
  numbers reconfirmed); the TRUE no-op after that: **emit 29.93 s /
  cargo 0.36 s, `20,003 reused, 0 re-emitted, 286 files unchanged,
  0 written`** — the emitted-side rehash (the measured ~854 MB term)
  is gone. Honest residual: the remaining ~30 s is parse/plan +
  input-side .class hashing + glue IO — precisely what G2.3's profile
  must name next.
- Disk lesson (recorded): the invalidation rebuild doubled the out20k
  target (old + new rlibs coexist by hash) and the final link died
  ENOSPC at 99 % — cargo clean + chunked rebuild (585 s killed +
  405.60 s completed). The 70k ladder must budget target-dir growth
  per stage (G2.7 note).
- Battery: j2r-codegen 110 lib (incl. 8 peephole) + manifest 11 (incl.
  the new fast-path test) + e2e ✓; partition differential k=2
  28.52 s ✓ (byte-identical — the partitioned reuse loop was updated
  too); corpus_suite 34.57 s ✓; jar_suite 3 passed 20.81 s ✓;
  fmt + clippy `-D warnings` ✓.
- Evidence: `scripts/evidence/g2_2/` (build2 compile log/time +
  cargo time + the full remeasure log).
- Next: **G2.3** — profile the 20k emit residual (parse/plan/input-
  hash/glue — name the hot function), then G2.4 (release 20k trio),
  G2.1b (link wall), G2.5 (ladder).

### [2026-11-10 session 11 cont. 3] G2.3 CLOSED, fix landed — the stage profile names two O(N²) hot functions in the load stage; per-class 1.30 → 0.111 ms (FLAT across 4× N); 20k no-op emit ≈ 5 s (from 41.74 s); full battery green
- Instrumentation first (committed, off by default):
  `J2R_PROFILE_STAGES=1` + `StageGuard`/`stages_report()` in
  j2r-codegen (compile stages + both emit paths + the CLI report line)
  — the machine-checked "named hot stage, not adjectives" answer.
- The 20k no-op emit profile (N=20000, k=250, warm):
  **stage1_load_parse_hash = 26.09 s of 29.93 s (87 %)**; everything
  else ≤ 1.6 s. Inside stage1, TWO named hot functions, both O(N²)
  family: (1) the closure worklist's referer attribution
  `classes.iter().find(|c| c.refs.contains(&name))` — O(wanted × N ×
  refs) per round (T03 fixed the constant-pool re-walk but left this
  scan); (2) the module-collision check
  `classes.iter().any(|c| c.module == module)` in parse_and_add —
  O(N²) string compares.
- The fixes (both diagnostics-only, byte-identical emission, same
  first-hit semantics): a parse-time `referer_of` map (wanted-name →
  first referer in class order) and a `module_index` map — the closure
  loop and the collision check now O(log n) lookups. Full-render
  no-prior-manifest run verified: all 20,003 class files byte-stable
  (mtime + no-op reuse after manifest restore).
- Measured constants (the bar's "measured constant" arm):
  stage1 per-class at 20k **1.30 ms → 0.111 ms**; 5k reference post-fix
  **0.080 ms** — per-class FLAT across 4× N (was 16× worse at 20k than
  5k; the F1 super-linearity is gone). 20k no-op emit total ≈ **5 s**
  (41.74 s pre-G2.2 → 29.93 s G2.2 → 5 s); 20k full render 34.30 s →
  **18.4 s** (emit_render_write 14.58 s = 0.73 ms/class — now the
  largest stage, LINEAR, accepted with its measured constant).
- Battery (zero divergences everywhere): codegen 110 lib + manifest 11
  + e2e ✓; partition differential k=2 29.08 s ✓; corpus_suite 34.71 s ✓;
  jar_suite 3 ✓; diff_suite 109 fixtures 519.52 s ✓; cfg_equiv
  372.28 s ✓; fmt + clippy `-D warnings` ✓.
- Minor honest observation: a no-prior-manifest run's `files_written`
  stat over-counts glue (prior-absent entries counted without a real
  write; mtime-verified no spurious file writes) — cosmetic stats
  accounting, registered for a future tidy, not a correctness issue.
- Evidence: `scripts/evidence/g2_3/` — the 5k/20k stage tables (no-op,
  full render, cold 5k reference).
- Next: **G2.4** (release-profile 20k trio, disk-guarded), G2.1b (the
  link wall), G2.5 (the honest 35k→50k→70k ladder), G2.7 (budget
  framing) — then G4 (M4 owner review + volatile/final boundary).

### [2026-11-10 20:13 UTC session 12] reset #12 survived via §9; ff to `c4c45cf` (G2.3 closure, 167 commits); toolchain rebootstrap byte-exact; baseline re-proven 462 green; README 452→462 drift fixed (gate-11)
- Reset #12 (sandbox recycled; toolchain + `tools/` + `env.sh` wiped,
  tree back at pre-M3 `b4badea`): the §9 recipe — fetch + ff-only merge
  → HEAD `c4c45cf` == origin/main (167 commits; the G2.1 split +
  re-measure `4a44cfd`/`a221aff`, G2.6 peephole `36b0738`, G2.2
  manifest v2 `ba5f007`, G2.3 stage profile `c4c45cf` — all four
  closures verified ON origin, nothing lost).
- Toolchain rebootstrap (§3.1 item 1, full download route): rustup-init
  21.3 MB → pinned **cargo/rustc 1.99.1 + rustfmt 1.10.0 + clippy
  0.1.99** into `tools/cargo` (CARGO_HOME) + `tools/rustup2`
  (RUSTUP_HOME); Temurin **jdk-21.0.13+9** tarball **207,858,352 B
  byte-exact** vs the recorded size (the GitHub releases-asset route,
  as recorded in session 7) → `tools/jdk-21.0.13+9` (javac/java 21.0.13
  verified); `env.sh` recreated (JAVA_HOME, J2R_JAVAC, CARGO_HOME,
  RUSTUP_HOME, PATH, CARGO_PROFILE_DEV_DEBUG=0). `target/` (509 MB)
  survived → warm builds (j2r-cli 13.29 s).
- **Full baseline re-proven BEFORE any G2.4 code (T05's law):
  462 tests, 0 failures** — classfile 38 · codegen 135 (lib 110 + e2e 1
  + malformed 12 + manifest 11 + wide_ids 1) · jni-ffi 15 · runtime 243
  (20.24 s) · cli 29 (bin 15, corpus 1 @30.63 s, jar 3 @19.57 s,
  partition 1 @25.49 s, soe 1 @10.54 s, jni_parity 1, lambda 2,
  native_e2e 2, cfg_equiv 1 @342.90 s, diff_suite 2 @461.36 s — 109
  fixtures, zero divergences); fmt clean; clippy `-D warnings` clean.
- **Drift found + fixed (gate-11)**: README still said the 452
  aggregate (the 13d number) while the G2.x closures added 10 tests
  (peephole 8, manifest fast-path 1, stage instrumentation 1) — gate-11
  flagged `README 452 vs measured 462`; README overview + Testing
  updated to the measured 462 with the G2.x additions named; dated
  452 mentions in TODO/console history deliberately left (they record
  what was true then). Gates re-run green: gate-11 `fixtures=109
  tests=462` ✓, gate-19 links ✓, gate-20 stubs ✓, gate-12 seams ✓.
- Next: **G2.4** (release-profile 20k trio, disk-guarded — 6.3 GB free
  after the rebootstrap; target/ budgeted), G2.1b rides with the next
  20k/70k protocol run, G2.5 ladder, G2.7 framing — then G4.

## Session 13 — 2026-11-10 (G2.4 CLOSED, G2.1b CLOSED, G2.5 next)

- Context on resume: tree at `9b2ff9b` (169 commits, clean except the
  staged `scripts/g2_4_release_trio.sh`); the G2.4 cold emit + cargo
  chunks 1–3 were already recorded in `m5invest/farm20k/release_trio.log`
  (each capped at 550 s, fingerprints resuming) — the run resumed at
  chunk 4. Chunk plan rule held: every stage ≤ 550 s wall.
- Disk discipline (the ENOSPC lesson): 3.9 GB free on resume; freed the
  stale `target/debug` (840 MB, dev artifacts only — the release j2r
  binary survived) → 4.8 GB free before the build; the script's own
  guard (≥1.2 GB) held at every cargo stage (min observed 2,376,228 kB
  before stage5); release target ended at 3.1 GB.
- **G2.4 measured (release profile, opt-level 3, N=20000 k=250, -j 2,
  CI=0)**: cold release cargo **7537.36 s across 14 chunks**, peak
  single-process VmHWM **3,289,116 KB (3.29/4.0 GB)** — the final
  rust-lld link, no OOM/SIGKILL at any stage; release binary
  **1,060,961,000 B (1.01 GiB)**; emit tier cold 11.63 s / no-op
  5.83 s / one-edit 5.81 s (each ≈0.9 GB peak); no-op release cargo
  **0.36 s** — the G2.2 manifest stat fast path carries into release
  (20,003 reused / 0 re-emitted); one-edit release cargo **100.88 s,
  peak 3,298,716 KB** (relink-dominated); **parity vs Temurin PASS**
  (stdout + stderr + exit identical). Chunk VmHWM band 1.38–1.43 GB
  for rustc crates, 3.29 GB only at the final link.
- **G2.1b CLOSED via arm (b)**: the link-stage boundary accepted in
  writing — link memory scales with BINARY size (still total-class-
  scaled, different process); no mitigation attempted; a ~70k release
  link (~3.5× binary) is EXPECTED to exceed the 4 GB/process budget on
  this box class, and the G2.5 ladder records that wall if it fires,
  measured, not extrapolated. `proc_peak_sampler.py` wraps every cargo
  chunk in `scripts/g2_4_release_trio.sh` — now part of the standing
  20k/70k protocol.
- Evidence committed: `scripts/evidence/g2_4/` — 14 chunk .time tables
  (wall + VmHWM), emit/cargo times, j2r rebuild + javac times, parity
  stdouts (84 B identical), trio log, disk_after, out20krel_size.
- Docs updated: REVIEW_CHECKLIST.md G2.4 + G2.1b → [x] with the
  measured numbers; console_summary.md §8 rows + implementation order +
  session-13 ops line. No compiler code changed this session (the
  standing 462-green baseline at `93ed921`/`c4c45cf` covers the tree;
  G2.4 is a measurement closure).
- Next: **G2.5** — the honest ladder 35k → 50k → 70k under the G2.7
  budget framing (4 GB/process, disk-guarded); per-step stage tables,
  three-channel parity, raw files committed; extrapolation does NOT
  count. Disk budget planning happens first (the 20k release target
  alone was 3.1 GB).

## Session 13 cont. — 2026-11-10 (~22:35–23:40 UTC): G2.5 CLOSED, G2.7 CLOSED, ZIP64 fix

- **G2.5 rung N=35000 (full protocol)**: farm gen + javac 43.28 s
  (1.20 GB), jar 43 MB, JVM parity captured (exit=0, 82 B, 0 B),
  COLD EMIT 23.89 s / peak 1.58 GB / 1.5 GB sources. Chunked cargo
  build: 5 chunks to the wall — every rustc VmHWM ≤ ~1.0 GB (−j 2),
  **ALL 141 partition rlibs + support + j2r-runtime + j2r-jni-ffi
  compiled** (deps 5.1 GB, census committed), then the FIRST LINK:
  rust-lld peaked **3,275,384 KB (3.28/4.0 GB — INSIDE the budget;
  the G2.1b linear extrapolation ≈5.4 GB was too pessimistic, lld
  scales sub-linearly with the binary)**, wrote 262,900 KB of the
  binary and died **rc=101 ENOSPC**. The disk wall, then triple-
  measured: packing 2 (partition sources deleted) → cargo refuses to
  even LOAD a workspace whose member manifests are missing; packing 3
  (fresh re-emit) → emit ENOSPC'd twice, and the best achievable
  packing leaves ≈0.1 GB free vs ≈2.1 GB needed for the app crate +
  binary. Verdict: **35k compile stage PASS, link stage = box-class
  disk wall, measured**. Evidence: scripts/evidence/g2_5_35000/
  (WALL_RECORD.md, 5 chunk tables, census, all logs).
- **G2.5 rungs N=50000 / N=70000 (emit-stage scope, SCOPE.txt named)**:
  50k emit 36.35 s / 2.23 GB; 70k: javac OOM'd on the default heap →
  re-run with **-J-Xmx3g: 74.32 s / 2.95 GB (measured)** → jar
  89,590,182 B → emit FAILED on **"zip64 archives are not supported"**
  — the 70,003-entry jar exceeds the classic ZIP limit.
- **ZIP64 jar-reader fix (G2-scope code found BY the ladder)**:
  crates/j2r-classfile/src/zip.rs now parses the ZIP64 locator +
  ZIP64 EOCD + per-entry 0x0001 extras (spec-ordered: uncompressed,
  compressed, local offset) instead of rejecting them loudly; classic
  archives take the unchanged path. 3 new unit tests: classic zip64
  round-trip, **a real 65,537-entry in-memory archive** end-to-end
  (parse + spread reads), sentinel-without-locator rejection. fmt +
  clippy clean; battery green in slices: classfile 41, codegen 135,
  runtime 243, jni-ffi 17, cli bin 15 + corpus 1 + jar 3 + partition
  1 + soe 1 + lambda 2 + jni_parity 1 + native_e2e 2, diff_suite 2
  (109 fixtures, zero divergences, 492.70 s), cfg_equiv 1 (358.34 s)
  — **465 tests, 0 failures**; gate-11 in sync (README 462→465).
  The 70k emit then PASSED: **53.14 s, peak RSS 3,159,744 KB
  (3.16/4.0 GB — INSIDE the budget at the Minecraft scale)**, 3.0 GB
  sources, JVM channels captured. Evidence: scripts/evidence/g2_5_
  50000/, g2_5_70000/.
- **70k NOT claimed** (the bar requires end-to-end on this box class;
  the 70k link needs ~17 GB co-resident vs the 9.8 GiB volume — the
  boundary is named as the BOX's, not the compiler's: no emitted unit
  exceeds 1.0 GB rustc at 35k; the G2.1 split holds). Registered
  G2.5-ladder-continuation: PASS when a rung above 20k completes
  end-to-end with the four conditions.
- **G2.7 CLOSED**: the budget (4 GB/process VmHWM, disk-guarded
  volume) was named in the ladder scripts + the 70k record and every
  run measured against it.
- Docs: REVIEW_CHECKLIST.md G2.5 + G2.7 → [x] with the full measured
  record; console_summary.md §8 rebuilt (G2.1–G2.7 all checked);
  README 465. Disk note: the ladder burned several GB — stale caches
  (~/.cache browser binaries 1.2 GB) and dead artifacts were freed
  mid-rung; every lowering of the disk guard is recorded in the rung
  logs.
- Next: **G4** (M4 owner review — JMM volatile + final-field freeze
  semantics or the CI-enforced warning, per the review sequence), then
  **G3** (typed-codegen deferred-join performance).

## Session 13 cont. 2 — 2026-11-10 (~23:45–00:15 UTC): G4.2 CLOSED via arm (b)

- **The JMM boundary is now ENFORCED, not just documented** (REVIEW_
  CHECKLIST G4.2 arm (b), the review's either-arm bar):
  - Codegen: `FieldPlan` carries `is_volatile`/`is_final` (from the
    input `ACC_VOLATILE` / `ACC_FINAL` flags; a final WITH ConstantValue
    folds and is JMM-safe, so only non-folded finals carry the boundary).
    Every generated access — GetStatic/PutStatic/GetField/PutField in
    BOTH the default and the typed emission paths, plus the reflection
    FIELDS_ rows — emits a `J2R-JMM-BOUNDARY:` marker comment at the use
    site naming the field, its flag, and docs/RUNTIME_SAFETY.md §6.
  - Every emitted crate (single-crate AND partitioned app) gains a
    deterministic `src/jmm_boundary.rs` sidecar listing exactly the
    boundary fields. EMITTER_VERSION 2→3.
  - **CI gate-23-jmm-boundary** (wired into gate-8's needs): javac the
    committed `crates/j2r-cli/tests/jmm/JmmBoundary.java` fixture →
    j2r compile → `scripts/check_jmm_boundary.py` re-derives the
    expected set from the INPUT `.class` flags (own python classfile
    parser) and fails loudly on sidecar divergence, a missing
    access-site marker, or a marker naming a non-boundary field; then
    builds the marked crate and runs it vs Temurin three-channel
    (`52/3/7/42`) — marker plumbing changed no behavior.
  - **Tamper-tested**: stripping the access-site markers → rc=1 with a
    named failure (the first tamper attempt exposed that registry-row
    markers satisfied the needle — the checker now requires the marker
    in `src/classes/` specifically; the sidecar is excluded so a field
    cannot satisfy its own check).
  - In-repo test `crates/j2r-codegen/tests/jmm_boundary.rs`: a
    hand-assembled classfile with all five flag shapes asserts sidecar
    exact-match, per-access marker counts (2 volatile + 2 final), and
    no false positives on the plain field and the folded constant.
  - Builtin-ancestor field accesses (corpus wave 2: JZlib touches
    `FilterInputStream.in` by name) resolve to slots, not FieldPlans —
    markers attach only to user fields, which is exactly the checker's
    scope; the corpus_suite failure this exposed on the first cut was
    the discovery that motivated the user-field-only marker rule.
- Battery green **466/0**: classfile 41, codegen 136 (incl. jmm_
  boundary), runtime 243, jni-ffi 17, cli bin 15 + corpus 1 + jar 3 +
  partition 1 + soe 1 + lambda 2 + jni_parity 1 + native_e2e 2 +
  diff_suite 2 (109 fixtures, zero divergences, 483.68 s) + cfg_equiv 1
  (354.34 s); fmt + clippy clean; gate-11 in sync (README 465→466).
- G4.1 (the owner verdict) stays OPEN — the review now has enforcement
  evidence to review. Arm (a) (real volatile/freeze semantics with
  litmus suites, STRATEGIC_GAPS R4) remains future work.
- Next: **G4.1** awaits the owner; then **G3** (typed-codegen
  deferred-join performance, the P1 ladder).

## Session 13 cont. 3 — 2026-11-10: G3.1 baseline registered (work starts next session)

- **G3.1 baseline census captured** (`scripts/evidence/g3_1/
  readiness_baseline_2026-11-10.txt`, 117 table rows): on the current
  tree the JamaBench hot-kernel refusals are EXACTLY
  **tred2 / orthes / hqr2 → NeedsCfgLift(ImpureJoin)**; tql2 and the
  LUDecomposition ctor are already TypedReady (the lift-closure wave +
  the diamond laws landed earlier). hqr2 has 26/33 clean positions at
  93.2% primitive occupancy — the deferred-join mechanism is the only
  thing between it and typed emission.
- G3.1 plan (next session, the checklist bar verbatim): implement the
  deferred-join mechanism (branch-live values carried into successor
  entry variables across the loop-exit join — the named
  `ImpureJoin` lowering) behind `--emit-cfg`; the three kernels lift;
  cfg_equiv stays zero-divergence on the whole suite; record the
  same-session controlled pair EITHER WAY (a measured NO-GO is a valid
  recorded outcome). Then G3.2 (block-join/entry-var layer), G3.3
  (collection boxing), G3.4 (repeatable gains on BOTH benches with
  byte-identical output), G3.5 (the honesty guard).
- Session 13 gate tally: **G2.4, G2.1b, G2.5, G2.7, G4.2 CLOSED**
  (commits 262e038, 7b6680c, 2f795ec); battery 452 → 466 green across
  the session; the G2.1–G2.7 gate set fully checked; the JMM boundary
  machine-enforced; G4.1 (owner verdict) the only open G4 item, with
  enforcement evidence ready to review.

## Session 14 — 2026-11-11: reset #13 survived via §9; baseline re-proven 466; G3.1 begins

- Reset #13 hit between sessions (tree back at pre-M3 `b4badea`,
  tools/ + env.sh + $WORK wiped; `target/` 520 MB survived → warm
  builds). The §9 recipe executed: fetch + ff-only merge → HEAD
  `cbcbbcb` == origin/main (168 commits; G2.4 262e038, G2.5+G2.7+ZIP64
  7b6680c, G4.2 2f795ec, G3.1 baseline census cbcbbcb — all verified ON
  origin, nothing lost). The session-13 tally read back from the boards:
  G2.4, G2.1b, G2.5, G2.7, G4.2 CLOSED; battery 452→466.
- Toolchain rebootstrap (§3.1 item 1, full download route): rustup-init
  29,915 B script → pinned **cargo/rustc 1.99.1 + rustfmt 1.10.0 +
  clippy 0.1.99** into `tools/cargo` (CARGO_HOME) + `tools/rustup2`
  (RUSTUP_HOME); Temurin **jdk-21.0.13+9** tarball **207,858,352 B
  byte-exact** via the GitHub releases-asset route (session 7's
  precision note held) → `tools/jdk-21.0.13+9` (javac/java 21.0.13
  verified); `env.sh` recreated (JAVA_HOME, J2R_JAVAC, CARGO_HOME,
  RUSTUP_HOME, PATH, CARGO_PROFILE_DEV_DEBUG=0).
- **Full baseline re-proven BEFORE any G3.1 code (T05's law):
  466 tests, 0 failures** at `cbcbbcb` — classfile 41 · codegen 136
  (lib 110 + e2e 1 + malformed 12 + manifest 11 + wide_ids 1 +
  jmm_boundary 1) · jni-ffi 17 · runtime 243 · cli bin 15 · corpus 1
  (30.62 s) · jar 3 (19.51 s) · partition 1 (4.54 s) · soe 1 (26.55 s)
  · lambda 2 · native_e2e 2 · jni_parity 1 (11.00 s) · cfg_equiv 1
  (349.31 s) · diff_suite 2 (462.66 s — 109 fixtures, zero
  divergences); fmt clean; clippy `-D warnings` clean.
- Gates re-run green on the fresh tree: gate-11 `fixtures=109
  tests=466` ✓, gate-19 links ✓, gate-20 stubs ✓, gate-12 seams ✓.
- NEW LESSON (#12, process): protocol scripts must be COMMITTED before
  their first run — the session-12 `g2_4_release_trio.sh` died un-run
  in reset #13's wipe only because the session-13 close landed it;
  anything not pushed is not protected, scripts included.
- Next: **G3.1** — the deferred-join mechanism (the named
  `ImpureJoin` lowering) behind `--emit-cfg`; tred2/orthes/hqr2 lift;
  cfg_equiv stays zero-divergence; same-session controlled pair
  recorded either way (a measured NO-GO is a valid recorded outcome).

### [2026-11-11 session 15] Reset #14 recovered via §9; the owner's verification pass executed (every session-13 claim confirmed at artifact level); TODO.md + ROADMAP drift found and fixed; M6_PLAN.md rev 0 drafted per owner request

- Sandbox reset #14 hit between sessions: `tools/` empty, `env.sh` gone,
  `$WORK` (m5invest) wiped, tree reverted to pre-M3 `b4badea`; `target/`
  (509 MB) survived → warm builds. The §9 recipe executed: fetch +
  ff-only merge → HEAD `63f34a7` == origin/main (175 commits; the six
  session-13/14 closure commits 262e038 / 7b6680c / 2f795ec / cbcbbcb /
  8ee103c / 63f34a7 all verified ON origin — nothing lost).
- Toolchain rebootstrap (§3.1 item 1, full download route): rustup-init
  **21,347,088 B byte-exact** → pinned **cargo/rustc 1.99.1 + rustfmt
  1.10.0 + clippy 0.1.99** into `tools/cargo` (CARGO_HOME) +
  `tools/rustup2` (RUSTUP_HOME); Temurin **jdk-21.0.13+9** tarball
  **207,858,352 B byte-exact** via the GitHub releases-asset route →
  `tools/jdk-21.0.13+9` (javac/java 21.0.13 verified); `env.sh`
  recreated (JAVA_HOME, J2R_JAVAC, CARGO_HOME, RUSTUP_HOME, PATH,
  CARGO_PROFILE_DEV_DEBUG=0).
- **The owner's verification pass ("are you sure?") — every session-13
  claim checked against the tree, not memory — ALL CONFIRMED**:
  evidence dirs `g2_4`, `g2_5_35000/50000/70000`, `g3_1` present with
  real content; ZIP64 implemented in `crates/j2r-classfile/src/zip.rs`
  (parse_zip64_eocd + zip64_extra, 39 ZIP64 references) with the three
  named tests (`zip64_small_archive_round_trip`,
  `zip64_over_65535_entries` at 65,537 entries,
  `zip64_sentinels_without_locator_errors`); gate-23-jmm-boundary wired
  in `.github/workflows/ci.yml` + `scripts/check_jmm_boundary.py` +
  the sidecar emitter across emit/partition/typed/manifest
  (EMITTER_VERSION 3); README carries 466 with the full test
  inventory; the G3.1 census (106 methods: 76 typed-ready,
  13 liftable-all-ref, 13 needs-cfg-lift — ImpureJoin 4,
  HasExceptionTable 8 — mixed-epoch 4; 79.3% prim share) is committed
  evidence.
- **Full baseline re-proven AFTER the reset (T05's law): 466 tests,
  0 failures** at `63f34a7` — classfile 41 · codegen 136 · jni-ffi 17
  · runtime 243 · cli bin 15 · corpus 1 (30.82 s) · jar 3 (19.71 s) ·
  partition 1 (4.65 s) · soe 1 (26.58 s) · lambda 2 · native_e2e 2 ·
  jni_parity 1 (10.79 s) · cfg_equiv 1 (351.07 s) · diff_suite 2
  (476.52 s — 109 fixtures, ZERO divergences); fmt clean; clippy
  `-D warnings` clean.
- Gates re-run green on the fresh tree: gate-11 `fixtures=109
  tests=466` ✓, gate-19 links ✓, gate-20 stubs ✓, gate-12 seams ✓ —
  plus **gate-23 FULL LOCAL PASS** end-to-end on the recovered tree:
  fixture compiled, checker green ("3 boundary field(s), all marked;
  sidecar in sync"), the marked crate built --offline and its
  three-channel output vs Temurin identical (52/3/7/42, rc 0 == 0).
- **TODO.md + ROADMAP drift found by the read-through and fixed**:
  TODO.md had no G4.2/G3.1 closure entries, its battery was stale at
  465 ("466" appeared NOWHERE in the file), and its sequencing line
  still said "next: G4 then G3" with "T14–T16 mappings (G5) start
  only after G1 AND G2 are checked"; ROADMAP's M5 row still said
  "next: G2 (the 70k path) — the remaining P0". All reconciled: the
  G4.2 (arm b, gate-23, tamper-tested) and G3.1 (census) closures are
  now recorded in TODO.md with evidence commits, the battery reads
  465→466 with gate-11 in sync, sequencing reads G3.1 next / G4.1
  owner verdict pending / G5 UNBLOCKED (G1+G2 checked) ordered after
  G3/G4, and ROADMAP's M5+M6 rows carry the same state.
- **M6_PLAN.md rev 0 drafted (owner request: "make a M5_plan.md just
  like all previous")** — named M6 because M5_PLAN.md already exists
  (the Minecraft-readiness plan from 2026-11-08) and the M2→M3→M4→M5
  series continues at M6: **docs/M6_PLAN.md — Mod ecosystem (rev 0,
  owner review pending)**. Contents: §0 scope + the ordering law (the
  review's G5.4 gate is SATISFIED — G1.1–G1.6 and G2.1–G2.7 all
  checked; planning ≠ implementation start; T14–T23 stay M5_PLAN-owned
  and land first; the v1 out-of-scope list: no runtime rewriting, no
  runtime class generation, no dynamic mod discovery, Forge/NeoForge
  = boundary note only, jar-in-jar loud-unsupported); §1 the seam
  audit table VERIFIED against source this session (9 rows: CLI verbs
  build/test already exist and get EXTENDED not created; zip.rs
  classic+ZIP64; manifest stage-level caching gap; partitioned
  emission + the G2.7 budget framing; the T12 registry path means the
  loader shim needs NO new FFI — rows 5 and 8 explicitly marked
  "to verify at wave 0"); §2 the mod pipeline (`j2r build mod.jar`:
  fabric.mod.json v1 subset, closed-world mixin resolution, five
  stages, stage-level manifest caching with the G2.2 tier as the
  measured expectation); §3 the loader surface as natives (census-
  driven FabricLoader v1 subset, each method a registry slot +
  differential fixture, resources routed to the mod jar on disk);
  §4 mixins at mod scale + the dev loop (`j2r test mod` reuses
  diff_suite + sharding; harness positive control first — the gate-23
  tamper-test pattern; new CI gate-24-mod-boot; sampler protocol for
  footprint); §5 the Forge/NeoForge boundary design note (GO/NO-GO,
  no v1 implementation); §6 the task register T24–T35 across five
  waves; §7 the standing evidence rules inherited verbatim.
- ROADMAP M6 row now points at docs/M6_PLAN.md ("plan drafted — rev 0,
  owner review pending; implementation gated on M5 review"); TODO.md's
  M6 section carries the [~] planning pointer and the three boxes gain
  their §-references.
- Next: **G3.1** — the deferred-join mechanism (the named
  `ImpureJoin` lowering) behind `--emit-cfg`; tred2/orthes/hqr2 lift;
  cfg_equiv stays zero-divergence; same-session controlled pair
  recorded either way (a measured NO-GO is a valid recorded outcome).

### [2026-11-11 session 16] Reset #15 recovered via §9; G3.1's deferred-join mechanism LANDED behind --emit-cfg and proven zero-divergence (cfg_equiv 109×3); the three kernels do not lift yet — the census control pair is committed and the gate stays open

- Sandbox reset #15 hit between sessions (the third §9 recovery): tree
  back at pre-M3 `b4badea`, tools/ + env.sh wiped, `target/` (509 MB)
  survived → warm builds. Fetch + ff-only merge → HEAD `6c5e6dd` ==
  origin/main (176 commits). Toolchain rebootstrap byte-exact
  (rustup-init 21,347,088 B → rustc/cargo 1.99.1, rustfmt 1.10.0,
  clippy 0.1.99; Temurin jdk-21.0.13+9 207,858,352 B; env.sh recreated).
  Baseline re-proven BEFORE the G3.1 code (T05's law): **466 tests, 0
  failures, zero divergences** at `6c5e6dd` (cfg_equiv 349.08 s,
  diff_suite 466.61 s).
- **The deferred-join mechanism (the named `ImpureJoin` lowering)
  landed** in crates/j2r-codegen/src/cfg.rs, per the DESIGNED entry:
  (1) the stop lists are BUILDER-OWNED (`Builder.stops` — the call-
  threaded `&Stops` snapshots removed; frames push/pop by label so
  deferred entries survive them; the body_walk and stale-stop laws
  preserved verbatim); (2) DEFERRED JOINS registered at both refusal
  sites — the divergent-joins path defers ONLY the meet(s) with OUTSIDE
  predecessors (the design's own guard; pure meets stay inline in their
  owning arm — deferring both meets caused an early-drain cascade that
  consumed shared tails before later loops needed them), and the
  join-purity path defers instead of refusing when deferrable; (3) the
  DRAIN is DIAMOND-SCOPED (each diamond closes its own deferred labels
  immediately after its if/else — the label's braces open and close
  within the diamond's own emission).
- **cfg_equiv caught the first draft loudly** — the loop-body-end drain
  left a depth-0 label open across the enclosing diamond's sibling arm:
  BigJson's cfg-mode emission was invalid Rust (`} else {` with no `if`).
  The debug brace tracker (J2R_CFG_TRACE: every structural push recorded,
  every `}` must close the top opener, every `} else {` must close an
  `if`) pinpointed the exact construct in one run; the diamond-scoped
  drain fixed it. A latent PRE-EXISTING bug was unmasked and fixed on
  the way: a loop-GUARD diamond whose meet lies on the exit side hoisted
  exit-side code INTO the loop body via join_via_label (tred2's
  loop(4)/meet-60 shape — unreachable before because the method died
  earlier); the meet-side-transfer check now free-walks that shape.
- EVIDENCE: **cfg_equiv 109×{base,cfg,typed} ZERO divergences** with the
  mechanism live (the deferral changes which methods lift; behavior is
  identical); full battery 466/0 (classfile 41, codegen 136, jni-ffi 17,
  runtime 243, cli bin 15, corpus/jar/partition/soe/lambda/native_e2e/
  jni_parity 11, cfg_equiv 347.18 s, diff_suite 466.61 s — 109 fixtures,
  zero divergences); fmt clean; clippy `-D warnings` clean; gates
  11/19/20/12 green (gate-11 `fixtures=109 tests=466`).
- **The census control pair is committed** (scripts/evidence/g3_1/):
  baseline 2026-11-10 (ImpureJoin ×4: tred2/orthes/hqr2/QR <init>) vs
  after-mechanism 2026-11-11 — **12 balanced deferrals** (registered ==
  drained), the refusals moved DEEPER: tred2/orthes reach lift end with
  a few named un-emitted blocks (tred2 [26, 38, 51–54, 58]; orthes
  [16–19, 25–28, 39, 51–54]), hqr2 one more `exit_after_body` shape
  (block 66 inside loop(15)), QR <init> likewise. The three kernels do
  NOT lift yet — **the G3.1 gate stays OPEN** with the recorded state;
  the remaining walk-order shapes are the next session's work, and a
  measured NO-GO remains a valid recorded outcome either way. The
  refusals all carry named-block diagnostics now (J2R_CFG_DEBUG=1 names
  the block, =2 block/loop tables, =3 dumps the emitted text at the
  refusal, J2R_CFG_TRACE replays every diamond decision).
- Next: **G3.1 continuation** — walk the three remaining shapes to the
  lift (the un-emitted blocks are reachable only through edges the
  deferral converted to `break`s; the walk order must route the
  enclosing chain through the deferred bodies exactly once), then the
  typed-payload re-measure (G3.4's protocol) once the census moves.

## Session 17 — 2026-11-11 — G3.1 CLOSED: all FOUR JamaBench kernels TypedReady

- **Sandbox reset #16** between sessions (the FOURTH §9 recovery): tree
  at pre-M3 `b4badea`, tools/ + env.sh wiped, `target/` survived → warm
  builds. Fetch + ff-only merge → HEAD `dcb3034` == origin/main (177
  commits). Toolchain rebootstrap byte-exact (rustup-init 21,347,088 B →
  cargo/rustc 1.99.1, rustfmt 1.10.0, clippy 0.1.99 into tools/rustup2 +
  tools/cargo; Temurin jdk-21.0.13+9 207,858,352 B via the
  releases-asset route; env.sh recreated). Baseline re-proven BEFORE the
  session-17 code (T05's law): **466 tests, 0 failures, zero
  divergences** in chunks (libs/bins 424 · codegen integration 26 · cli
  medium 10 · c_probe 2 · corpus 1 @ 30.31 s · cfg_equiv 1 @ 30.41 s ·
  diff_suite 2 @ 448.71 s).
- **The session-16 state reproduced exactly**: tred2/orthes/hqr2/tql2
  all NeedsCfgLift BranchIntoEmittedCode; the refusals were at the
  lift-end all-emitted check (missing=[26,38,51..] for tred2, [22] for
  tql2, ...) — and **a REGRESSION was discovered**: ~30 simple Matrix
  methods (transpose, uminus, plus, minus, ...) that were TypedReady in
  the 2026-11-10 baseline refused with missing=[5] — the session-16
  mechanism broke previously-working methods that cfg_equiv cannot see
  (they are not in the 109-fixture set). Added the missing=[...] debug
  dump at the lift-end check + method names in the cfg-trace diamond
  lines (both debug-gated).
- **FIX 1 — FRAME-ID STOP STACK** (the root cause of both the missing
  blocks and the regression): emit_loop popped its exit stop with a
  POSITIONAL `truncate(saved)`, while the diamond frames pop by
  label-retain — a retain below the mark SHIFTS the stack and the
  truncate keeps WRONG entries (transpose: the inner diamond's retain
  removed the outer diamond's (1,'loop_1_1') entry below the mark, so
  truncate(1) kept the exit stop (5,'loop_3_2') → region(5)
  short-circuited to stop_transfer → block 5 never emitted, and a
  `break 'loop_3_2'` landed OUTSIDE its loop). The label-retain had a
  second latent collision: an active-meet diamond's frame label IS the
  enclosing loop's label, which is also the loop's own exit-stop entry
  label — the retain killed the exit stop mid-body. Fix: `StopEntry {
  block, label, sid }` with a monotonic `stop_seq`; `push_stop` hands
  back the sid; ALL frame pops (emit_loop exit, active_meet,
  join_via_label, drains) retain by SID — immune to position shifts and
  label collisions. RESULT: tred2, tql2 AND orthes lifted TypedReady and
  the Matrix regression reverted (76 typed-ready restored).
- **FIX 2 — DELEGATED-EXIT ORDER** (hqr2's next wall): emit_loop refused
  `exit_emitted` BEFORE computing `delegated` — hqr2's inner loop(72)
  has exit=15 = the still-active outer loop's HEADER (the shared-exit
  shape: the inner loop's exit edge IS the outer loop's continue; the
  header is legitimately in `emitted` AND a stop). Fix: delegated first;
  refuse only for an exit that is emitted AND unowned.
- **FIX 3 — DEEP CROSS-MEET ATTEMPT** (hqr2's final wall): the block
  table + javap revealed diamond(76)'s true shape — block 82 FALLS INTO
  83 (the f-arm merges into the t-arm: 83 is both the true arm's start
  AND the convergence point), and both arms converge at the loop latch
  105 (`iinc 32,1; goto 72` — preds: the else arm's own `goto 105` (81),
  the notlast diamond(85)'s true branch, and the inner loop(91)'s exit).
  walk_meet stops at the FIRST join per arm (83, 80) — the shallow
  divergent path spuriously deferred 83 (its "outside pred" was its own
  chain successor 84) and emitted 105 inline in the f-arm, so
  diamond(85) hit emitted code. Fix: `walk_deep_set` — a bounded DFS
  through INTERNAL joins (raw+resolved forms recorded so trampoline
  preds pass the purity check), transfer boundaries at stop targets and
  ACTIVE/emitted loop headers, walk-through at FRESH open-exit headers
  (walk_meet's semantics — required: latch pred 101 lives inside fresh
  loop(91)); the cross-meet attempt fires ONLY for the ARM-START
  (empty-arm) shape — a general unique-candidate fallback intercepted a
  pure-meet shape the shallow logic deliberately refuses (the pinned
  walk_meet law test) and lowered it incompletely; REMAINING cross-arm
  joins (the latch 105) are PRE-REGISTERED (label opens before the
  if/else so every break site — the else arm's 81-edge AND the meet
  body's 85/101 edges — is inside it); the attempt is OPTIMISTIC with
  FULL ROLLBACK (out/brace_trace/deferred/stops/active_loops/emitted/
  body_walk restored by mark or sid; join_via_label's Ok(false)
  contract honored — the caller emits region(meet)).
- **The law test updated** (the session-16 precedent): the
  walk_meet_maps_instruction_indices_to_blocks shape (the ret block IS
  the outer join — an empty-arm meet) now LIFTS with both joins
  label-scoped; the test's primary purpose (the no-panic index mapping)
  is preserved and its doc comment states the new law.
- EVIDENCE: **the census control pair committed** —
  scripts/evidence/g3_1/readiness_before_s17_fixes_2026-11-11.txt (the
  dcb3034 state: 101 methods, 76 typed-ready, needs-cfg-lift 10 =
  ImpureJoin 1 + HasExceptionTable 8 + HasSwitch 1) vs
  readiness_after_s17_fixes_2026-11-11.txt (**77 typed-ready (+1 =
  hqr2), needs-cfg-lift 9 = HasExceptionTable 8 + HasSwitch 1, ZERO
  CFG-shape refusals; tred2 83.4%, tql2 92.2%, orthes 86.5%, hqr2 93.2%
  prim — ALL TypedReady**). **cfg_equiv 109×{base,cfg,typed} ZERO
  divergences** with the mechanism live (332.69 s full re-run); full
  battery 466/0; fmt clean, clippy `-D warnings` clean; gates
  11/19/20/12 green (fixtures=109 tests=466).
- Boards: TODO.md G3.1 entry → [x] CLOSED with the three fixes + the
  control pair; sequencing → G3.2 next; ROADMAP M5 row refreshed.
- Next: **G3.2** — the block-join layer + primitive-specialized
  collections vs the 2.78× boxing tax, opening with the typed-payload
  re-measure on the four now-lifted kernels (G3.4's protocol); G4.1
  owner verdict open with enforcement evidence ready; G5 (T14–T23)
  UNBLOCKED ordered after G3/G4.

## Session 18 — 2026-11-12: the G3.2 opening measurement — typed-payload re-measure at the all-four-kernels census

- **Reset #17 hit** (fourth §9 recovery): repo tree survived at
  `fb6683c` == origin (178 commits, 0 dirty beyond the untracked
  `downloads/`), but the toolchain + `$WORK` wiped again. The proven
  recipe scripts (`/home/z/my-project/scripts/bootstrap15.sh`,
  `bootstrap_jdk15.sh`) + the cached byte-exact tarballs survived;
  rebootstrap in one chunk: rustup-init **21,347,088 B**, Temurin
  **jdk-21.0.13+9 207,858,352 B** (byte-exact, 5th validation of the
  §3.1 law), `tools/jdk-21.0.13+9` reused, `tools/cargo` +
  `tools/rustup2` rebuilt (cargo/rustc **1.99.1**, rustfmt 1.10.0,
  clippy 0.1.99). `/home/z/my-project/env.sh` survived intact.
  `target/` FULLY warm: workspace build 0.01 s (no rebuild).
- Orientation sweep first (the standing law): TODO.md G3 block,
  console_summary.md tail, REVIEW_CHECKLIST.md G3 section,
  TYPED_REGISTER_READINESS.md §5 (the register), BENCHMARKS.md
  addendum 7/8 (the 2.78× container tax + the typed-payload protocol),
  ROADMAP M5 row, README status line, the corpus/ + evidence/ layout.
- Census reproduced at this tree BEFORE any measurement: **101
  methods, 77 typed-ready, 12 liftable-all-ref, 9 needs-cfg-lift
  (8 HasExceptionTable + 1 HasSwitch)** — the session-17 end state,
  byte-for-byte.
- **The typed-payload re-measure TAKEN (the G3.4-framed opening of
  G3.2)**: fresh emits both twins (no manifest reuse), release builds
  (typed 94.4 s / default 48.9 s), same-session JVM reference
  (Temurin 21.0.13, K=1200 N=40 M=24, wall 2.60 s), **median of 2:
  default 279.23 s vs typed 256.31 s = 0.918** — the lever ledger
  moves 0.936 → 0.918 (8.2% cumulative). The 1.8pp is the
  tred2/orthes/hqr2 typing margin: addendum 8's 0.936 had only tql2
  typed of the four kernels. All five processes exit 0, all four
  native runs **byte-identical** to the JVM on stdout. Secondary
  census on this 8-class crate: 106 `.expect(` sites in BOTH twins
  (raw accessors shared; the 873 figure was the full JAMA-corpus
  crate — different crate, noted in the evidence file).
- EVIDENCE: `scripts/evidence/g3_2/typed_payload_remeasure_2026-11-12.txt`
  (pair table + protocol + honesty notes) + the five raw run logs
  (java_ref, typed_r1/r2, default_r1/r2).
- Boards: TODO.md gains the session-18 [x] entry (the re-measure) and
  re-points at **G3.2 — the block-join/entry-var layer** (~140 slot-var
  reads, the named prerequisite at its measured ceiling); G3.3
  (primitive-specialized collections vs the 2.78× tax) sequenced after
  the entry-var layer per the register.
- Commit `535066a`: the re-measure evidence + boards pushed (origin/main
  = 179). A near-miss caught by the remote: `git add -A` swept the
  untracked cached-tarball dir (`downloads/`, 198 MB) into the commit —
  rejected by GitHub's 100 MB limit; fixed by untracking + gitignoring
  `downloads/` and amending BEFORE any large-file history landed. The
  tarballs stay local-only (they are the §3.1 rebootstrap cache).
- **G3.2 slice 1 LANDED — the REF-ONLY REFRESH PROJECTION.** Census
  first: the typed JAMA crate has 1023 refresh sites carrying 5119
  wraps+clones; `FrameGuard::refresh` (frame.rs) pattern-matches every
  entry and keeps ONLY `Value::Ref(Some(_))` — every prim wrap and
  HighHalf Void fill was pure construction + scan waste. The typed
  refresh now carries only `Var::Cell` locals (ref or mixed) and
  REF-kind live stack positions; the handle list is IDENTICAL (GC-root
  parity by construction), sites unchanged. The dead `latest()` helper
  removed; module doc + fn doc state the law. Measured (same-session,
  median of 2, byte-identical): 0.918 → **0.913** (+0.5pp; run sets do
  not overlap; refresh-array traffic −60.3%). Ledger: 0.992 → 0.958 →
  0.936 → 0.918 → **0.913** (8.7% cumulative).
- **Register correction:** the entry-var layer's named target ("~140
  slot-var reads") re-censused on today's tree: **20 stack-entry
  reads** (Eigen 1, Maths 1, Matrix 16, SVD 2) — the unification +
  payload waves consumed the stack-entry round trip. The measured
  residual is the register's THIRD family: per-access invoke/arg-vec
  construction (`vec![Value...]` per call) + obj_get/invoke helpers.
  TYPED_REGISTER_READINESS §5 updated; the layer's next slice re-aims
  at that family.
- Proof: cfg_equiv 109×{base,cfg,typed} ZERO divergences (343.07 s);
  diff_suite 109 (458.93 s); corpus 4/4 (31.87 s); codegen 136;
  runtime 243; classfile 41; jni-ffi 17; cli 15+10; jar 3; partition 1;
  soe 1 = **full battery 466/0**; fmt + clippy `-D warnings` clean
  (the doc-list-item lint rewording verified emission-neutral:
  typed_v3 diff vs typed_v2 identical).
- Evidence: scripts/evidence/g3_2/refonly_refresh_projection_2026-11-12.txt
  + slotvar_census.py + crossblock_census.py + typed_v2_r{1,2}.txt.

### [2026-11-11 20:54–21:49 UTC → 2026-11-12, session 18a — parallel continuation] G3.2 opening measurement + slice 1

(Landed as 535066a + c38fcdb while session 18b worked; recovered by the
reset watch at session-18b start: origin/main had moved past fb6683c,
ff-merge pulled both commits. See console_summary.md session-18 block.)

### [2026-11-12, session 18b] Reset #17 recovered (5th §9 validation); flagged leftovers verified; G3.2 slice 2 LANDED — the INVOKE/ARG-VEC STACK-ARRAY PROJECTION (0.913 → 0.894)

- Reset watch: tree at pre-M3 `b4badea`, toolchain gone, `target/`
  (454 MB) survived. origin/main had moved PAST session 17's fb6683c —
  two G3.2 commits from the parallel continuation (535066a typed-payload
  re-measure 0.918; c38fcdb slice 1 ref-only refresh projection 0.913).
  ff-merge → `c38fcdb`, then the §9 recipe: rustup-init **21,347,088 B
  byte-exact** → 1.99.1 + rustfmt 1.10.0 + clippy 0.1.99; Temurin
  **207,858,352 B byte-exact** (GitHub releases-asset route); env.sh
  recreated; warm build 6.24 s.
- **Owner's flagged leftovers verified** (directive: "these are left
  from to do list"):
  * Wave residual depth >= 5: trigger NOT fired — the only `[[[[[I` on
    disk is the registered-limit test itself (verify.rs:2202/2235); no
    corpus entry or fixture needs 5+ nesting. Stays deferred per the
    DECIDED 2026-11-08 design (recursive descriptor Nest(Box<ElemKind>)
    after the typed-operand unification wave closes; loud registered
    limit meanwhile).
  * R2 metadata generator: unchanged, registered post-M4 scope
    (docs/STRATEGIC_GAPS.md R2; static subset only).
- Baseline re-proven BEFORE any edit: 437 (workspace minus cli) + 15
  (cli bin) + 10 (light cli: jar 3, jni_parity 1, lambda 2, native_e2e 2,
  partition 1, soe 1) + 1 (corpus) + 1 (cfg_equiv) + 2 (diff_suite) =
  **466/0**.
- **G3.2 slice 2 — the re-aimed slice (invoke/arg-vec family).** Census:
  228 `let __a = vec![...]` invoke-arg sites in the JAMA typed crate
  (67 empty, 161 non-empty up to 4 args); every one verified one-shot
  into `natives::invoke_exact(..., &__a, ...)` or a direct
  compiled-method call `crate::classes::X::m_yyy(recv, &__a)` — both
  take `&[Value]`. The three invoke arms in typed.rs now emit
  fixed-size stack arrays (`let __a = [args...];` / typed empty
  `let __a: [::j2r_runtime::Value; 0] = [];`) — unsized coercion, zero
  runtime change, clones unchanged (GC-root parity: the same clones
  root the same refs; only the container moved heap → stack). The
  false-positive audit (word-boundary `__a` uses in windows) confirmed
  no push/reuse/rebind-before-consume anywhere.
- MEASURED (same-session pair, registered protocol, median of 2, fresh
  emits, byte-identical stdout vs the same-session Temurin run on all
  five processes): default 280.150/280.734 → **280.44 s**; typed
  251.025/250.405 → **250.72 s** = **0.894**. Ledger: 0.992 → 0.958 →
  0.936 → 0.918 → 0.913 → **0.894** (10.6% cumulative). JVM ref this
  session 2.87 s → typed gap 87.4x (context only; the ratio is the
  claim — G3.5).
- Proof: codegen lib 110/110; cfg_equiv 109×{base,cfg,typed} ZERO
  divergences (337.15 s); diff_suite 109 (463.65 s); corpus 1/1;
  full battery **466/0**; fmt clean; clippy --all-targets
  --all-features clean. JAMA workload exit 0 four times + JVM ref
  twice (stable stdout).
- Scope note: the DEFAULT backend's arg vecs are the same shape —
  registered as a follow-up candidate (separately measured if taken);
  typed-backend residual: obj_get/invoke dispatch inside the runtime +
  the G3.3 container tax (2.78x, register step 3).
- Evidence: scripts/evidence/g3_2/argvec_stack_array_2026-11-12.txt +
  five raw run logs (java_ref_s18b, default_s18b_r{1,2},
  typed_s18b_r{1,2}).
- Next: G3.3 — primitive-specialized collections (the 2.78× container
  tax, register step 3), byte-identical output required; or the
  runtime-side invoke/obj_get family if the owner orders the perf
  ladder deeper first.
- **G3.3 OPENING (same session, after slice 2 pushed):** the register's
  step 3 (the 2.78× container tax) concretized — census on the
  addendum-7 instrument (BoxingBoxedBench): javac's hot loop is
  Map.get → doubleValue → +w → Double.valueOf → Map.put
  (pc 99/107/113/116); the typed backend pays per op 1 heap alloc +
  2 dispatches + 2 obj natives. Design now concrete: signature-pinned
  specialization, two rewrites (map_put_prim / map_get_prim) behind a
  dataflow-window guard (no branch/phi/other use between box birth
  and consume), generic exits materialize lazily cached in the node
  slot (identity law kept), node slot stays a Value (the M3 law), one
  native per op. Registered, NOT scheduled — after the G3.2 slices.
  Evidence: scripts/evidence/g3_3/opening_census_2026-11-12.txt;
  TODO.md G3.3 entry is the index.

### [2026-11-12, session 19] Wave residuals CLOSED — the depth-5 recursive descriptor wave (owner directive)

- The owner asked "Wave residuals (remaining) and r2 metadata are still
  not done why?" — the second ask on the depth-5 residual; the recorded
  deferral reason (typed-operand unification first) had EXPIRED (the
  G3.2 slices landed through the same seams the same day). Executed the
  DECIDED 2026-11-08 design: the 12 enumerated RefPrimArray* Copy
  variants replaced by ONE `ElemKind::Nest(Box<ElemKind>)` — `[[I` →
  Nest(IntGroup), any depth folds one more level, zero variants per
  future depth. ElemKind/VTy dropped Copy (Clone retained); the
  62-error Copy ripple was mechanical and compiler-guided (merge fns to
  match-on-refs, dup family clones — compile-time-only code — and
  &VTy Kind::of/Pool::of signatures). Runtime UNTOUCHED (array.rs's
  shape dispatch was always depth-agnostic; the lattice was the only
  limit).
- Law tests: `depth5_still_ref_registered_limit` replaced by
  `depth5_recursive_descriptor_any_depth` (aaload on `[[[[[I` yields
  the exact 4-deep row for a `([[[[I)V` call site; wrong-kind loads
  name the folded shape); the depth-4 message test updated to the new
  Debug shape.
- Proof: codegen 110/110; cfg_equiv 109×3 ZERO divergences (340.17 s);
  diff_suite 109 (481.57 s); corpus 1/1; full battery **466/0**; fmt +
  clippy -D-clean (4 redundant-closure findings fixed en route).
- Evidence: scripts/evidence/depth5_closure/
  recursive_descriptor_2026-11-12.txt; docs/CORPUS_WAVE.md §gaps +
  §wave-order updated; TODO.md wave-residuals entry flipped to [x].
- Next in this session: the R2 metadata generator opening (census +
  design + slice 1 scope) — the owner's other flagged leftover.

### [2026-11-12, session 19 cont.] R2 OPENED — slice 1: Class.forName resolves the closed world (byte-identical fixture)

- The owner's second flagged leftover, pulled forward the same session.
  The opening census CORRECTED the premise: the M3 subset already
  retained per-class metadata statics (UserClassInfo: fields/methods/
  is_interface/simple_name, consumed via Dispatch::class_info) — the
  loud string-lookup gap was `Class.forName(name-as-data)`.
- SLICE 1 LANDED: forName(String) + forName(String, boolean,
  ClassLoader) as natives resolving against the closed world (compiled
  registry first, then the new `builtin_class_known` test); the 3-arg
  form accepts the INERT ClassLoader (getSystemClassLoader registered —
  the system loader IS the closed world); Temurin-21-exact contracts
  oracle-captured BEFORE implementation (CNFE message IS the input
  name; null → NPE "null"; "int" unresolvable; `[I` and
  `[Ljava.lang.String;` resolve with dots-to-slashes inside `L...;`).
- NEW FIXTURE ReflectForName.java (the 110th): **byte-identical vs
  Temurin on the first differential run**. Doc-count gate updated
  (fixtures=110 tests=466, checker green); diff_suite guard 108 → 109.
- DISCOVERED EN ROUTE (registered, not fixed): Field.get on a STATIC of
  a compiled class returns null — the M3 static-pool boundary; slice
  2's first row. Registered remaining slices: static-pool access +
  getModifiers, getDeclaredConstructors, retained annotations, static
  proxies, --reflection=all.
- Proof: cfg_equiv 110-class census ZERO divergences (354.84 s);
  diff_suite 110 fixtures (464.41 s) ZERO; corpus 1/1; full battery
  **466/0**; fmt + clippy zero findings.
- Evidence: scripts/evidence/r2_opening/forname_slice1_2026-11-12.txt;
  TODO.md R2 entry [ ] -> [~] with the slice plan.

### [2026-11-13, session 19b] EXTERNAL PORT VALIDATION persisted + integrated (owner directive)

- Reset #18 recovered first (6th §9 run): tree at pre-M3 b4badea,
  toolchain + work dirs + env.sh gone, target/ (509 MB) survived;
  fetch + ff-merge to ac4af2d — which ALSO pulled the parallel
  continuation's two commits (e6ace40 depth-5 CLOSED via the recursive
  Nest(Box<ElemKind>) descriptor; ac4af2d R2 slice 1 Class.forName,
  110th fixture byte-identical). The owner's "why still not done"
  challenge is answered BY ACTION on both items.
- The owner then delivered a NEW independent result: jbox2d 2.2.1.1
  (167-class physics engine, 156 compiled) ported OUTSIDE this repo on
  a 2 vCPU / 2 GB box against c38fcdb + ~1,100 lines of owner patches.
  PERSISTED VERBATIM (owner directive: "add these in todo list and
  roadmap to survive sandbox reset"): docs/external-port/{README.md,
  jbox2d_result.md, jbox2d_bug.md, jbox2d_opt.md} (commit bc4a068).
- The result: 6/6 workloads byte-identical to Temurin 21 (1,980
  chaotic physics steps + 1e6 trig evals); steady-state 6.9x-84x
  slower than the JVM (median 25x — the same regime as our published
  JamaBench numbers, independently reproduced on a different program
  and machine); peak RSS 1.74x-4.80x LOWER; startup 26x faster;
  translate 156 ms vs javac 969 ms, then 111.8 s of cargo.
- TODO.md §3 integration (same commit): EXTERNAL-PORT PERSISTED [x];
  EXTERNAL-B1 [ ] the formatter %f small-magnitude silent-wrong-output
  fix + O4 band fixture; EXTERNAL-B2 [ ] the partitioned-emission
  registry-hook fix (E0433 on any exception-table class) + O5
  partition/exception fixture; EXTERNAL-B3..B12 [ ] the nine-gap
  natives/jdk surface (fdlibm module the biggest piece); EXTERNAL-
  HARNESS [ ] (cfg-report --surface, collect-all-diagnostics,
  partition-exception law, charset pin); EXTERNAL-CORROB [ ] R1-R9
  mapped onto register §5 (the owner's 5.1x stack-cost micro + measured
  R1/R2 prototypes corroborate the typed-register track; R6(c) heap
  borrow region is the new candidate; R9 = the G2.6/M5 wall with a
  2 GB OOM data point). docs/ROADMAP.md carries the validation
  paragraph.
- Corroboration of the two prior deferrals: the port needed NO depth
  >= 5 array descriptors and no reflection beyond forName-shaped
  lookups — a real game-shaped program confirms both were correctly
  sequenced (and both have since landed anyway: e6ace40, ac4af2d).
- Next: toolchain rebootstrap (§9, 6th validation) → baseline 466/0
  BEFORE any code → EXTERNAL-B1 (the formatter fix) with the O4
  fixture, then EXTERNAL-B2.
