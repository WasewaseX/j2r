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
