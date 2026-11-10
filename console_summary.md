# j2r console summary — the state board

Updated 2026-11-10 (session 10 — the stale-tree discipline fired again:
tree at pre-M3 `b4badea`, ff-only → `0552cba` (161 commits, 0 ahead,
nothing lost). The updated external review's five doc-drift candidates
all verified against the tree and fixed docs-only: the M5 milestone row
"nearly closed" → wave 1 + wave 2 CLOSED; §8 open items rebuilt as the
full G2.1–G2.7 ladder (F1–F4 mapped into G2.1–G2.4, the G2.6 either-arm
bar and G2.5/G2.7 spelled out); the zero-`unsafe` one-liners here, in
console.md and TODO.md now carry the audited-FFI exception (11
occurrences, pin ≤ 12, machine-counted this session, none outside
ffi.rs); README's stale 241 era-scoped to the build-manifest wave
(6f79a10) with the measured 452 aggregate — gate-11 never checked 241
(correcting session 9's verification line). Plus a SIXTH drift the
review missed: M5_JNI_FFI §3's growth sentence ("→ 12 in T13 13c")
contradicted its own §4 register and the gate count — fixed to
11 of pin 12. Gates 11/19 green post-fix. Open: G2 = the 70k path
(G2.1–G2.7, §8) and the pending M4 owner review. Session-9 detail
(reset #10, the all-.md read, its 6 fixes, the rustup-home recovery)
lives in console.md. Raw log: `console.md`.
**Read this file before acting**; if a memory disagrees with this board,
the board wins.

## 1. What the project is

j2r: a Java → Rust compiler written in safe Rust — zero `unsafe` in
every crate EXCEPT the audited `j2r-jni-ffi` FFI core (11 occurrences,
gate-10-pinned ≤ 12, docs/M5_JNI_FFI.md §3; the count is asserted by
CI and mirrored by an in-crate test). Input is compiled Java `.class`
bytecode (not Java source); output is Rust source that is compiled by
rustc into a native binary. Correctness law: three-channel differential testing (stdout /
stderr / exit) against Temurin 21 — never "similar behavior", byte-identical
output. North star: compile Minecraft Java Edition (~70,000 classes) and
its mod ecosystem.

## 2. Milestone ladder (correct names)

| M | Name | Status |
| --- | --- | --- |
| M1 | Core compiler | ✅ done, externally reviewed |
| M1.5 | Engineering hardening | ✅ done |
| M2 | Generated-code quality (GC, catchable SOE, CFG) | ✅ done |
| M3 | JDK coverage | ✅ done (suite 106 → 109 fixtures) |
| M3.5 | Audit response A1–A8 | ✅ done |
| M3.6 | Strategic gap register | ✅ registered |
| M3.7 | Real-world corpus wave | in progress (minimal-json, JZlib, java-string-similarity, Jama waves) |
| M4 | Threads & concurrency | all 4 waves landed, CI-verified — **owner review pending** |
| **M5** | **Minecraft readiness** | **STARTED 2026-11-08** — wave 1 (20k scale) + wave 2 (JNI v1 + C-harness parity) CLOSED; remaining P0: G2 (the 70k path) |
| M6 | Mod ecosystem | future |

## 3. M5 register — where it stands

Wave 1 (scale & incremental compilation) — **all tasks landed**:
- T01 census: rustc cliff measured — **~4.2 MB peak RSS per emitted class,
  one process, fresh build**; ~900-class ceiling on this 4 GiB box; 70k in
  ONE process ≈ ~300 GB. javac 0.5 MB/class; j2r translation flat
  (0.4 ms + 48 KB/class). Isolations: the mega-function is NOT the driver;
  metadata-only rustc OOMs pre-LLVM → the wall is the pre-codegen
  metadata/MIR phase.
- T02 design note (docs/M5_SCALE.md): **link-table crate partitioning**
  chosen. SCC/DAG layering rejected with a measured reason (the class web
  is one strongly connected component — exactly the Minecraft shape).
  Options (a) tuned-down single crate (measured: still dies) and
  (d) thinner emission (constant-factor only, kept as follow-up lever)
  also rejected with reasons. This is the bottleneck analysis + plan the
  owner required before coding — T05 implemented exactly it.
- T03 closure cache: quadratic worklist fixed; compile stage −75 % at 5k.
- T04 partition differential: 6 fixtures × forced k=2, CI-pinned.
- T05 partitioned emission (`--partition-classes K`): support crate
  (statics pool + global link table + dispatch slot) + p0..pN crates +
  app crate; cross-partition calls via deterministic global method ids;
  1k @ 1.91 GB / 5k @ 1.89 GB peak (single-crate SIGKILLed at 3.66 GB);
  byte-identical vs Temurin; default path re-proven unchanged.
- T06/T07 build manifest (owner: **APPROVED**, 2026-11-09): SHA-256
  input→emitted tracking, global gate, write-if-changed, tamper/corruption
  fallback; 1k farm release: cold 357.5 s → no-op 0.535 s (668×) →
  one-body-edit 71.0 s (5×); byte-identical.
- **20k census-jar confirmation LANDED (2026-11-09, this session) — wave 1
  CLOSED.** N=20000, k=250 (80 part crates + support + app), cargo -j2,
  dev profile with debug=0 (disk-forced, recorded). Cold end-to-end
  ≈ 17.1 min (javac 26.15 s / 1.55 GB; emit 34.12 s / 858 MB; cargo
  ≈ 967 s, peak rustc **3.49 GB** in the app-bin compile); no-op rebuild
  **38.8 s** (20,003 reused / 0 written + cargo 0.28 s); one-body-edit
  **55.3 s** (20,002 reused / 1 re-emitted + only its partition + app
  rebuilt); **parity PASS** (stdout/stderr/exit identical, 0 == 0);
  binary ≈ 1.01 GiB. Findings F1–F5 registered in console.md — the two
  that matter: no-op emit is SHA-256-rehash-bound (F2, mtime/size fast
  path is the fix) and the app/Dispatch composite is the new rustc
  near-wall at 20k (F3 — fix is the already-registered split-table
  emission).

Waves 2–5 (next): T08 ✅ · **T09 ✅ LANDED (session 3)** — the audited
FFI crate `j2r-jni-ffi` (6 primitives, **5 actual `unsafe` occurrences,
pin ≤ 12, gate-10 amended in the same commit**) + the safe handle-table
bridge (`jobject` = generation-pinned table tag, local/global refs,
Push/PopLocalFrame, critical registry, GC root source 5); 379 tests
green, diff_suite + cfg_equiv zero divergences after landing. Wave 2
now: **T10 slice 2 ✅ LANDED (session 4 cont.)** — the five string slots
(NewString/NewStringUTF/GetStringUTFLength/GetStringUTFChars/
ReleaseStringUTFChars) wired through the generator `REAL_SLOTS`
registry; audited FFI surface 6 → 7 (`write_u8_out`, the 1-byte
`isCopy` out-slot); the release-miss abort shape pinned by a
child-process test; 390 green, diff_suite + cfg_equiv re-run anyway —
zero divergences. **T10 slice 3a ✅ LANDED (session 4 cont. 2)** — the
global-dispatch decision implemented: `jni::host_dispatch` installed
by the emitted main_run (both layouts; partition differential = the
proof); jclass = the interned Class mirror; jfieldID registry with the
inherited-field walk + 36 programmatic accessor wrappers; the T08
bare-`c_void` jfieldID stand-in amended to the ABI-faithful pointer;
398 green, full battery (partition + cfg_equiv 348 s + diff_suite
422 s) zero divergences. **T10 slice 3b ✅ LANDED (session 5) —
T10 COMPLETE** — the 46 array slots (91 real slots total): 171 length
(null → 0 conceded), 172–174 object arrays (element class FROM the
jclass mirror; ASE check rides the host dispatch), 175–182
New<Prim>Array (one primitive per slot, loud NASE), 183–190 elements
(registry-pinned caller copy, isCopy=TRUE via primitive 7), 191–198
release mode law (COMMIT/ABORT/0) with validate-before-remove registry
discipline, 199–214 regions (up-front bounds, loud AIOOBE), 222/223
criticals via the existing registry (isCopy=FALSE, the address IS the
payload); 15 arrays tests + the child-process release-miss abort pin;
no new FFI primitives (gate-10 unchanged); design-note correction of
record: the critical pair is 222/223 (217/218 are MonitorEnter/Exit);
414 green, full battery re-run (cfg_equiv 339 s + diff_suite 438 s)
zero divergences. **T11 (Call*Method) ✅ COMPLETE — 11a design note +
11b (the pending cell, the method-ID registry, all thirty
Call*MethodA) + 11c (the SysV save-area walk, all thirty Call*MethodV,
FFI 7 → 10 primitives, 6 → 9 unsafe occurrences of pin 12)**; **T12 ✅ COMPLETE — 12a design note + 12b (the
binding registry, slots 215/216, the walk consult) + 12c (the
ACC_NATIVE call-through emission, the verifier skip, the acceptance
three-channel e2e with byte-identical UnsatisfiedLinkError headers)
LANDED at 440 green**; **T13 13a+13b LANDED (session 6)** — docs/M5_JNI_HARNESS.md
(design of record) + primitive 11 `call_foreign_native` (the fixed SysV
register-window call; return read as BOTH channels; beyond the window =
LOUD) + gate-10 amended in the same commit (occurrences 9 → 10, pin ≤
12) + `invoke_binding`/`foreign_call` (descriptor-driven marshal, the
jclass-as-second-slot static convention) + the register-window loudness
pinned through call_native → invoke_binding → foreign_call → primitive
11 + the REAL C probe (build.rs compiles it with the platform compiler,
links into TEST binaries only — vendored generated projects never see
it; the `cc` crate rejected because a build-dep would break their
`--offline` builds); 444 green (jni-ffi 13 incl. 2 real-C integration
tests), diff_suite 454.98 s + cfg_equiv 353.99 s zero divergences.
Remaining wave 2: 13c (the harness C source + the Temurin driver + the
j2r loading story + the three-channel parity runner) and 13d (the
attach-lifecycle battery + the CI gate). Then T14–T16
mappings pipeline · T17–T21 mixin applier · T22–T23 inline caches. The
M4 completion package still awaits owner review in parallel.
**13d LANDED (session 8) — WAVE 2 (JNI) CLOSED**: the MEASURED attach
contract (the C probe in scripts/evidence/attach_lifecycle_probe/ —
fresh-thread GetEnv = JNI_EDETACHED pre-attach AND post-detach;
attach/re-attach/AsDaemon = JNI_OK idempotent; detach = JNI_OK ALWAYS,
three consecutive detaches measured JNI_OK — the oracle tolerates, it
does not punish) → VM AttachCurrentThread (4) / DetachCurrentThread
(5) / AttachCurrentThreadAsDaemon (7) real (**162 real slots**;
DestroyJavaVM stays loud), GetEnv attach-aware (ERR → EVERSION →
EDETACHED check order), main_run marks the main thread attached at
birth, the harness pthread battery (verdict 2047: every measured arm +
the self-checking battery on the attached env in BOTH attach cycles)
byte-identical three-channel vs Temurin (Temurin measured standalone
FIRST), the loud-gap child-process pins (NewGlobalRef/MonitorEnter/
MonitorExit/GetStringUTFRegion abort naming the slot; InvokeInterface
corrected of record — NOT in the Temurin 21 jni.h at all), and the
dedicated CI gate **gate-22-jni-c-harness-parity** wired into the
all-gates aggregate (17 upstream gates). 452 green, full battery
(cfg_equiv 370.05 s + diff_suite 491.11 s) zero divergences.
Next: G2 (the 70k path) is the remaining P0; G5 mappings (T14–T16)
starts only after G1 AND G2 are checked.

### 3.1 T08 preconditions — the wave-2 entry checklist (all must be green before T08 code)

1. **Toolchain re-proven after every reset** (environments rot — T05's
   law): rustup → rustc/cargo 1.99.1, Temurin 21.0.13 →
   `/home/z/my-project/tools/jdk-21.0.13+9` (session 3 moved the JDK to
   the persistent layer; /tmp is not trusted), `source
   /home/z/my-project/env.sh`. Status: **DONE (session 12; reset #12 —
   full-download route, both tarballs byte-exact vs the recorded
   sizes; rustup 21,347,088 B; JDK 207,858,352 B). Prior: session 8
   (reset #9 — recovered with NO download: the persistent layer held
   the full rustup install; `tools/cargo` = CARGO_HOME + `tools/rustup`
   = RUSTUP_HOME; env.sh now exports both, so a tools/-surviving reset
   never re-downloads). Exact pinned versions verified again this
   session — rustc/cargo 1.99.1, rustfmt 1.10.0, clippy 0.1.99,
   Temurin javac/java 21.0.13; target/ survived → warm builds.
2. **Full local baseline green before the first T08 commit**: unit tests
   across all four crates + `diff_suite` (three-channel vs Temurin) +
   corpus suite; test counts re-synced per gate-11. Status: **DONE
   (session 12, re-proven at `c4c45cf` before G2.4)** — **462 tests, 0
   failures** (classfile 38, codegen 135, jni-ffi 15, runtime 243,
   cli 29 incl. cfg_equiv 342.90 s + diff_suite 461.36 s, zero
   divergences; fmt clean, clippy `-D warnings` clean; gate-11
   fixtures=109 tests=462). Prior: **452** after T13 13d (session 8;
   classfile 38, runtime 239, jni-ffi 17 = 15 unit + 2 real-C, codegen
   125, cli 29; cfg_equiv 347 s + diff_suite 464 s, zero divergences;
   the C-harness parity battery green both runtimes).
3. **Design of record read**: M5_PLAN §3 (JNIEnv/InvokeInterface static
   tables, object-model bridge, v1 surface, C-harness evidence) +
   STRATEGIC_GAPS §R3. No new design debate unless a measured fact
   forces one.
4. **Standing constraints carried into T08**: `j2r-runtime` keeps zero
   external dependencies; zero `unsafe` (`#![forbid(unsafe_code)]` +
   gate-10); the `extern "system"` fn pointers point at safe Rust fns —
   callers' FFI is theirs, our side stays safe; index-for-index layout
   vs `jni.h` pinned by a const-evaluated assertion table (drift =
   compile error, not a segfault).
5. **Evidence bar known up front** (M5_PLAN §3 acceptance): a
   C-compiled harness `.so` exercising every v1 function, run against
   Temurin and the j2r binary with identical observable results,
   three-channel; `RegisterNatives` round-trip demonstrated; JNI suite
   pinned in CI (T13).

## 4. Cross-module correctness — the six constraints, mapped to landed mechanisms

The owner required the cross-module design before the next wave. The T05
architecture answers each constraint; evidence = partition differential
suite (T04) + byte-identical farm runs:

| Constraint (Java behavior) | Landed mechanism |
| --- | --- |
| static fields globally consistent | shared `support` crate: one id-keyed statics pool every partition links against |
| object identity across batches | heap handles + identity caches live in the runtime/support, never per-crate |
| virtual dispatch across crate boundaries | composite Dispatch in `app` routed by global method ids; link table holds the same fn pointers — semantics identical to direct calls (proven by byte-identical runs) |
| GC sees references from every batch | collector scans the runtime-owned registries (M2/M4 design), which partitions never duplicate |
| exceptions propagate across modules | throw/catch machinery is runtime code; partition boundaries are plain Rust calls — no new boundary semantics (pinned by partition fixtures incl. Cycles) |
| reflection sees global class metadata | registry/statics live in `support`; the static reflection subset reads the same tables (M3 surface) |

The cycle problem (Java A→B→C→A across crate edges): **solved by the link
table** — nothing references across crates by path, so Cargo never sees a
cycle; SCC grouping was evaluated and rejected as the primary mechanism
because one giant SCC (the real Minecraft shape) collapses layering to a
single crate (M5_SCALE.md §2b, with the measured reason). Inline caches
(T23) may later cache hot cross-partition targets.

## 5. The numbers that matter (all measured on this box)

| Claim | Number | Where |
| --- | --- | --- |
| rustc single-process cost | ~4.2 MB peak RSS / emitted class | T01 census CSV |
| single-process ceiling (4 GiB) | ~900 classes | T01 |
| 70k in one process | ~300 GB (transient build memory) | T01 derivation |
| emitted source size | ~48 KB / class → 70k ≈ **3.2 GiB source**, binary smaller | T01 + manifest |
| partitioned peak RSS | 1.91 GB @ 1k k=500; 1.89 GB @ 5k k=300 | T05 |
| no-op rebuild | 357.5 s → 0.535 s (668×), 1k release; 20k emit 41.74 → 29.93 (G2.2 fast path) → ≈5 s (G2.3 profile fix) | T06/T07 + g2_2 + g2_3 |
| one-body-edit rebuild | 71.0 s (5×), only its partition + app recompile | T06/T07 |
| closure-cache effect | compile stage 7.98 → 2.02 s @ 5k (−75 %) | T03 |
| **20k end-to-end (this session)** | cold ≈ **17.1 min**; no-op **38.8 s** (~26×); one-edit **55.3 s**; parity PASS | farm20k/*.time |
| 20k peak (post-G2.1 re-measure) | worst single rustc **≈0.99 GB** (partition, k=250; was 3.49 GB composite); build peak 3.09 GB = **rust-lld** on the 1.08 GiB binary → G2.1b | farm20k + proc_peak_sampler |
| 20k artifacts | emitted source 854 MB; binary ≈ 1.01 GiB (debug=0) | farm20k |
| JNI table (T08) | 235 slots (4 reserved + 231 fns), const-offset_of!-pinned vs Temurin jni.h; 231 non-null slots | jni/tables.rs + scripts/evidence/jni_slot_list.txt |

**Download-size answer (owner question, 2026-11-09):** No — nobody
downloads hundreds of GB. The 105–273 GiB / ~300 GB figures are the peak
resident memory (VmHWM of the largest single process, measured via
getrusage of reaped children) of ONE rustc process during a cold build.
The deliverable is ~3.2 GiB of generated Rust source (or a smaller
compiled binary). With partitioning, even the build-time peak is bounded
by the per-crate budget K.

## 6. External-tool v1.2 Minicraft test — digest (full: docs/EXTERNAL_V12_MINICRAFT_ANALYSIS.md)

- The owner tested a **source-level** converter (cguz/java2rust) on
  Minicraft: 76 files → 1,438 raw rustc errors, zero files compile; the
  deepest defects are silent (static-init blocks dropped → empty crafting;
  java.util.Random / fdlibm trig unmapped → wrong-but-compiling worlds;
  octal literals whose obvious fix changes colors).
- **Why most of the catalog cannot reproduce in OUR j2r:** we compile
  verified bytecode, not source — syntax-level bugs (instanceof/null
  emission, octal tokens, struct-field syntax, overload collisions,
  anonymous-class syntax) are structurally impossible; anonymous classes
  are real .class files; try/catch is exception tables + CFG; overloads
  are distinct methods by descriptor. The semantic classes are already
  covered and pinned: static blocks (JLS 12.4.2 clinit state machine +
  CyclicInit/SingletonFactory fixtures), exact Random + fdlibm (runtime
  natives + MathNatives fixtures), inheritance resolution (whole-Program
  resolution + partition differential), statics (statics pool).
- **What we genuinely adopt:** (1) Minicraft is an excellent corpus-wave
  candidate — a real 76-class web with deep Entity→Mob→… hierarchies,
  instanceof-heavy code, statics, and a bit-exact checksum harness design
  we can reuse for differential proof; (2) the GUI-boundary lesson (their
  M17) is our wave-2 JNI discipline: name the boundary, stub it loudly,
  list it in the port README; (3) their Rust-vs-Java performance table
  (2.8–8.7× faster, 8.8× less peak RSS) is the expectation-setter for our
  steady-state lever and M6.

## 7. Never-repeat mistake list (the "check every time" list)

1. **Milestone names:** M4 = threads & concurrency; **M5 = Minecraft
   readiness**; M6 = mod ecosystem. The old "M4 = Minecraft readiness"
   wording from a stale ROADMAP draft caused a naming mistake — the
   GitHub ROADMAP/TODO are authoritative.
2. **Measurement semantics:** every "peak RSS" number in this project =
   getrusage(RUSAGE_CHILDREN).ru_maxrss after a reaped `-j 1` build — the
   max single-process resident peak, NOT a parallel sum, NOT virtual
   memory. Say so wherever the number appears.
3. **Compile memory ≠ artifact size** — never let "~300 GB" imply a
   download size; source is ~48 KB/class, binary smaller.
4. **/tmp does not survive** sandbox recycles; persistent state belongs
   under /home/z/my-project (and is mirrored from /tmp at session start
   when a previous session left it there).
5. **env.sh paths** must point at /home/z/my-project/tools (a stale /tmp
   pointer silently breaks every cargo/javac call).
6. **Stale trees / reset recovery:** after any sandbox reset, no local
   tree is trusted — pin it: `git fetch origin && git reset --hard
   origin/main`, verify `git rev-parse HEAD` == `origin/main`. Since
   reset #2 the working tree is `work/j2r-fresh` (the old "work only in
   work/j2r-gh" rule died with that tree; the invariant is HEAD ==
   origin/main, not the directory name).
7. **Never claim done without runnable evidence; never claim Minecraft
   compatibility without milestone evidence** (standing law).
8. **Background processes are reaped when a tool call ends** (nohup and
   setsid both die) — long builds run as foreground calls ≤10 min,
   resuming via cargo fingerprints; sum the chunk walls honestly.
9. **Farm-classpath trap:** jar the classes dir (farm_src/classes), not
   the source tree — a source-jar makes j2r fail with "main class not
   found". The classes dir works directly as `--classpath`.
10. **Debuginfo fills small disks:** a 20k-class debug build hit 100 %
   disk (ld SIGBUS); use `CARGO_PROFILE_DEV_DEBUG=0` on this box and
   record the deviation.
11. **Commit raw measurement files** (`scripts/evidence/`, same commit
   as the claim) — the farm20k `.time` files died with a sandbox; only
   transcribed values survived.
12. **The boards are singular:** `console.md` + `console_summary.md`
   (lowercase, repo root) are the ONLY state files. A fresh session
   that finds a stale local tree must §9-recover, never create
   parallel/duplicate boards (the reset-#3 uppercase-CONSOLE near-miss;
   details in console.md session-3 entry).

## 8. Open items (live)

- [x] 20k census-jar confirmation — LANDED (session 1), wave 1 closed;
  state survived sandbox reset #2 via these boards (session 2, §9).
- [x] **G2.1 = F3** split the app/Dispatch composite — **CLOSED session
  11** (split landed session 10; 20k re-measure session 11): per-partition
  register_defaults/register_constants fns (bounded by k) + app-side Vec
  composition + static per-partition route tables. 5k A/B: peak 1.89 →
  1.26 GB (−33 %), wall −14 %. **20k re-measure**: every rustc ≤ 0.99 GB
  (partition crates, uniform — was the 3.49 GB composite), build peak
  3.09 GB attributed to **rust-lld** (the link stage, registered G2.1b);
  no-op 42.1 s; one-edit 66.0 s; parity PASS three-channel; partition
  differential k=2 byte-identical; battery green (452, zero divergences).
  Evidence: scripts/evidence/g2_1_split/ (raw .time + per-process VmHWM
  table + remeasure log). Honest caveat: the baseline "3.49 GB app-bin
  rustc" label conflated rustc with its lld child — the new
  scripts/proc_peak_sampler.py separates them.
- [x] **G2.1b — the link-stage wall — CLOSED session 13 via arm (b)**:
  the boundary is accepted in writing with the measured number + the
  sampler wired into the standing protocol. G2.4 release trio: 20k
  RELEASE link peak **3.29/4.0 GB VmHWM** (1.01 GiB binary, final
  chunk sampler-wrapped); one-edit relink 3.30 GB; debug=0 cold stands
  at rust-lld 3.09 GB. Written boundary: link memory scales with
  BINARY size (still total-class-scaled, different process); no
  mitigation attempted; a ~70k release link is EXPECTED to exceed the
  4 GB budget and the G2.5 ladder records that wall if it fires —
  measured, not extrapolated. Evidence: scripts/evidence/g2_4/.
- [x] **G2.2 = F2** no-op rebuild fast path — **CLOSED session 11**:
  manifest v2 records the emitted file's (mtime_ns,size);
  verify_reuse fast-paths on a stat match (content-hash fallback on any
  mismatch; Cargo-fingerprint trust model documented + behaviorally
  tested: stat-match reuse with a corrupted hash, then content-change
  detection). 20k no-op: emit **41.74 → 29.93 s (−28 %)**, cargo 0.36 s,
  20,003 reused / 0 written; partition k=2 byte-identical; corpus+jar
  suites green. Evidence: scripts/evidence/g2_2/. Honest residual
  (measured): the remaining ~30 s = parse/plan + input hashing + glue
  IO → G2.3's profile names it.
- [x] **G2.3 = F1** emission-stage profile at 20k — **CLOSED session
  11, fix landed**: new J2R_PROFILE_STAGES instrumentation names
  stage1_load_parse_hash = 26.09 s of the 29.93 s no-op emit (87 %);
  inside it two named hot functions — the closure worklist's linear
  referer attribution and the O(N²) module-collision scan — replaced
  with parse-time maps (same first-hit semantics, diagnostics-only).
  Per-class stage1: 20k 1.30 → 0.111 ms (5k 0.080 ms — FLAT across 4×
  N, the super-linearity is gone); 20k no-op emit ≈ 5 s (from 41.74 s
  pre-G2.2); full render 34.30 → 18.4 s. Battery green (diff 109 zero
  divergences, cfg_equiv, partition, corpus, jar). Evidence:
  scripts/evidence/g2_3/. Honest residual: cold emit_render_write
  (14.58 s, 0.73 ms/class) is now the largest stage — linear, accepted
  with its measured constant.
- [x] **G2.4 = F4** release-profile 20k trio — **CLOSED session 13**:
  cold/no-op/one-edit under `--release` on the disk-guarded staged
  protocol (scripts/g2_4_release_trio.sh, sampler on every cargo
  chunk). Cold release cargo **7537.36 s over 14 chunks, peak VmHWM
  3.29/4.0 GB** (final link, no OOM/SIGKILL); release binary 1.01 GiB;
  emit cold 11.63 / no-op 5.83 / one-edit 5.81 s; no-op release cargo
  **0.36 s** (G2.2 stat fast path carries into release); one-edit
  release cargo **100.88 s** (relink-dominated, peak 3.30 GB); parity
  vs Temurin PASS (stdout+stderr+exit identical). Raw evidence:
  scripts/evidence/g2_4/ (chunk tables + times + parity stdouts + log).
- [G2.5] the honest ladder — 35k → 50k → 70k on this 4 GB box class,
  end-to-end, three-channel parity, no OOM/SIGKILL, per-stage peak
  memory, raw files committed. Extrapolation does NOT count.
- [x] **G2.6 = F6** the JZlib fat-method regression — **CLOSED session
  11 via arm (a)**: the array-literal peephole (peephole.rs) collapses
  javac's clinit fill runs into ONE nested-static literal constructor;
  InfTree.rs 1.2 MB → 124 KB, StaticTree 387 KB → 16 KB; the JZlib
  crate now completes `--release -O3` on this box: SIGKILL at rustc
  3.25 GB → rc=0, 50.34 s, max process 678 MB (−79 %); release binary
  byte-identical vs Temurin (stdout+stderr+exit); battery green
  (codegen 110 incl. 8 peephole tests; partition k=2; corpus_suite;
  diff_suite 109 zero divergences; cfg_equiv; runtime 243). Evidence:
  scripts/evidence/g2_6/. Honest scope: the switch-table shape remains
  (InfBlocks::proc 217 KB — compiles fine, no budget threat measured);
  sampler + census = the standing regression probe.
- [G2.7] the 70k acceptance names the 4 GB/process budget up front and
  is measured against it.
- Implementation order (2026-11-10 external review): ~~G2.1~~ DONE;
  ~~G2.6~~ DONE; ~~G2.2~~ DONE; ~~G2.3~~ DONE; ~~G2.4~~ DONE; ~~G2.1b~~
  DONE (all sessions 11–13) → **G2.5** (the honest 35k → 50k → 70k
  ladder) with G2.7's budget framing; no mappings/mod-loader work until
  G2 passes.
- [x] **T08** (JNI, wave 2) — LANDED session 2 with full CI evidence
  (run 42616529578, all 21 gates + CodeQL/OSV/Scorecard success):
  generated static tables, const offset_of! assertions, loud-abort
  stubs, 7 unit tests; findings F-J1/F-J2 registered; suite-caught
  vendoring fix (recursive runtime copy).
- [x] **F-J1 design note** — docs/M5_JNI_FFI.md is T09's design of
  record: scoped FFI crate `j2r-jni-ffi`, exact law-amendment wording,
  6 audited primitives, 35-slot inventory (mechanically verified).
- [x] **T09** LANDED (session 3): audited FFI crate + handle-table
  bridge per M5_JNI_FFI.md — jobject tags generation-pinned (stale use
  PANICS, never aliases), per-thread local frames + Push/PopLocalFrame,
  global refs, critical registry, GC root source 5; 20 new tests
  (13 bridge + 7 ffi); gate-10 amended in the same commit (5 unsafe
  occurrences, pin ≤ 12); vendoring ships both crates (e2e-proven);
  379 green. Evidence: console.md session-3 T09 entry; M5_PLAN §6.
- [x] **T10** v1 surface — COMPLETE (sessions 3–5). Slice 1 LANDED
  (session 3): `jni::strings` modified-UTF-8 marshal + string-family
  slot bodies + 9 tests (workspace 388 green). Slice 2 LANDED (session
  4): generator `REAL_SLOTS` registry wires the five string slots;
  generator default path pinned to `$JAVA_HOME` (lesson #5);
  regenerate+fmt idempotent byte-for-byte; audited FFI 6 → 7
  primitives; abort shape pinned by a child-process test; 390 green,
  zero divergences. Slice 3a LANDED (session 4 cont. 2): host dispatch
  + codegen install (both layouts); jclass = interned Class mirror;
  jfieldID registry + walk + 36 accessor wrappers; the jfieldID
  c_void→pointer amendment; 398 green with the FULL battery. Slice 3b
  LANDED (session 5): the 46 array slots per M5_JNI_CLASSES.md §5
  (see §3 above for the full list), 15 arrays tests + the child-process
  array-release-miss abort pin (J2R_JNI_ARRAY_RELEASE_ABORT_CHILD);
  validate-before-remove registry discipline (a caught loud miss never
  breaks a later valid release); critical-pair slot correction
  217/218 → 222/223 in the design note + mod.rs doc; 414 green, full
  battery re-run — zero divergences. **T10 DONE.**
- [x] **T11** — design note docs/M5_JNI_CALLS.md LANDED (11a, the F-J2
  resolution: A-forms workhorse / V-forms via the SysV save-area walk /
  bare forms loud, honestly typed; pending-exception cell scoped). Part
  1 LANDED (11b-1): the per-thread pending cell (ExceptionOccurred/
  Clear/Check slots 15/17/228 real — the first faithful exception
  transport); the bit-62 method-ID registry (GetMethodID/GetStatic—
  MethodID 33/113, ancestor walk, NoSuchMethodError pended on miss);
  the A-form invoke core (descriptor-driven jvalue decode, virtual
  dispatch by the receiver's runtime class, zero-return-on-throw); 15
  slots wired (91 → 106); 4 unit tests; 418 green. Part 2 LANDED
  (11b-2): ONE invoke core now serves THREE families — the ten
  CallStatic*MethodA (no receiver; the jclass rides the receiver
  position) and the ten CallNonvirtual*MethodA (dispatch by the
  DECLARED class, the super.m() shape) — with the SAME ancestor walk
  for all three (fixes the 11b-1 single-shot gap: an inherited method
  resolves through the runtime-class walk); family/staticness
  mismatch = LOUD panic; null jclass pends NPE; the six mock-dispatch
  tests (override vs declared-class vs static, staticness loudness,
  NPE/zero laws, thrown-callee round-trip); 20 slots wired (106 → 126)
  with slot numbers asserted mechanically against the parsed header;
  regenerate+fmt byte-idempotent; 424 green, full battery re-run —
  zero divergences. Part 3 LANDED (11c) — **T11 ✅ COMPLETE**: the
  three audited read primitives (read_u32_at / read_word_at /
  read_fp_word_at; FFI 7 → 10, `ACTUAL_UNSAFE_OCCURRENCES` 6 → 9 in
  the same commit, pin ≤ 12 unchanged); the SysV __va_list_tag
  save-area walk in safe code (GP < 48 / FP < 176 bounds, overflow
  fallback, LOCAL-cursor law — the caller's record is never written
  back); all thirty Call*MethodV slots (35..62 / 65..92 / 115..142,
  asserted mechanically); 5 hand-built-save-area tests + 3 primitive
  tests; 432 green, full battery re-run — zero divergences. The bare
  C-variadic forms stay LOUD aborts (design-note decision 3).
- [x] **T12** — design note docs/M5_JNI_NATIVES.md LANDED (12a, the
  12a-before-code pattern: the v1 binding shape = the runtime's
  MethodFn; the registry laws; the dispatch-walk consult; the
  UserMethod.is_native metadata decision; foreign C fnPtrs wait for
  T13's audited call thunk, primitive 11). Part 1 LANDED (12b): the
  jni::natives registry (declared-native law class-SCOPED, rebinding
  overrides, unbind clears, the unbound UnsatisfiedLinkError, the
  Rust-shape round-trip via call_native); slots 215/216 real (156 →
  158, slot numbers asserted mechanically); the SHAPE LAW (the C-array
  slot path stores Foreign only; a Foreign call LOUDLY aborts naming
  primitive 11); the walk consult (bind-on-Base resolves through a
  Derived receiver — pinned); UserMethod.is_native landed with the
  generated rows emitting it (MethodPlan carries it, 12b = false
  everywhere). 5 tests; 437 green, full battery — zero divergences.
  Part 2 LANDED (12c) — **T12 ✅ COMPLETE**: plan_methods accepts
  ACC_NATIVE (the loud error is gone) with the metadata row + the
  emitted call-through body (FrameGuard + call_native); the verifier
  skips native plans; both lookup layouts map natives to the
  call-through fn (uniform dispatch); the acceptance e2e is a REAL
  three-channel differential (stdout/exit/stderr-header byte-identical
  vs Temurin — our message reproduces the oracle's quoted Java
  declaration `'int NativeProbe.add(int, int)'`), which surfaced and
  fixed two root defects (the message shape + the missing
  linkage-error builtin parent chain). 2 e2e tests + 1 builtin unit
  test; 440 green, full battery — zero divergences. **T12 ✅ COMPLETE.**
- [x] **T13** — 13a design note docs/M5_JNI_HARNESS.md LANDED (724e839).
  13b LANDED (session 6): primitive 11 `call_foreign_native` (the fixed
  SysV register-window call, both return channels, LOUD beyond the
  window); gate-10 amended 9 → 10 occurrences of pin 12 in the same
  commit; the `Binding::Foreign` path switch (`invoke_binding` /
  `foreign_call`: descriptor-driven marshal, rooted receiver, jclass for
  statics); the window loudness pinned through the full runtime path
  (fixed the in-flight test's dead-code tail); the REAL C probe via
  build.rs + `tests/c_probe.rs` (the first C in the repo — platform
  compiler, TEST binaries only, the `cc` crate rejected to protect the
  generated projects' `--offline` builds); 444 green, full battery zero
  divergences. **13c LANDED (session 7)**: the ONE harness `.so`
  (tests/jni_harness/harness.c) + the ONE self-checking driver fixture
  + the loading story (VM GetEnv supported path {1.1..1.8, 9, 10 — the
  MEASURED HotSpot 21 set} + GetJavaVM wired through audited primitive
  3, 159 real slots; System.loadLibrary as a runtime native over
  primitive 12 load_foreign_jni_library = dlopen + dlsym + the
  ABI-fixed JNI_OnLoad call, gate-10 10 → 11 same commit; GetVersion
  corrected to the MEASURED Temurin answer 0x0015_0000 — T08's 1.8
  assumption falsified by the harness) + the three-channel parity
  runner (tests/jni_parity.rs) — **Temurin vs j2r BYTE-IDENTICAL on
  stdout/stderr/exit, the full v1 battery, both directions;
  REVIEW_CHECKLIST G1.1/G1.2/G1.3 checked**; 448 green, full battery
  (diff_suite 464.16 s + cfg_equiv 347.11 s) zero divergences.
  REMAINING: 13d the attach-lifecycle battery (pthread → Attach →
  battery → Detach → re-attach) + the loud-gap probes + the CI gate —
  wave 2 closes at 13d. ALSO:
  docs/REVIEW_CHECKLIST.md (G0–G6) now governs milestone-completion
  claims — the external review's P0–P2 as pass/fail gates.
  **13d LANDED (session 8) — T13 COMPLETE, WAVE 2 (JNI) CLOSED, G1.1–
  G1.6 ALL checked**: the measured attach contract → VM slots 4/5/7
  real (162 slots), GetEnv attach-aware, main_run attach-mark, the
  pthread battery verdict 2047 byte-identical both runtimes, the
  loud-gap pins, gate-22 wired into the aggregate (17 gates); 452
  green. The checklist's G1 section is fully discharged.
- [ops] Session 13 (2026-11-10 ~22:30 UTC): **G2.4 CLOSED + G2.1b
  CLOSED (arm b)** — release-profile 20k trio measured end-to-end:
  cold release cargo 7537.36 s across 14 chunked stages (550 s cap,
  fingerprints resume), peak VmHWM 3.29/4.0 GB (final rust-lld link,
  no OOM/SIGKILL), release binary 1.01 GiB, parity vs Temurin PASS
  three-channel; no-op release cargo 0.36 s, one-edit 100.88 s; the
  G2.2 stat fast path carries into release. Disk guard ≥1.2 GB held
  (840 MB stale target/debug freed first; release target 3.1 GB).
  Raw evidence scripts/evidence/g2_4/ committed. Next: **G2.5** (35k →
  50k → 70k ladder) with G2.7 budget framing.
- [ops] Session 12 (2026-11-10 ~20:13 UTC): reset #12 survived via §9
  (tree back at pre-M3 `b4badea`; ff-only merge → `c4c45cf` ==
  origin/main, 167 commits — all four G2 closures verified ON origin);
  toolchain FULL re-download rebootstrap (rustup 1.99.1 + rustfmt 1.10.0
  + clippy 0.1.99 byte-pinned; Temurin jdk-21.0.13+9 207,858,352 B
  byte-exact; env.sh recreated); baseline re-proven **462 green** (the
  battery grew +10 tests with the G2.x closures — peephole 8, manifest
  fast-path 1, stage instrumentation 1); README 452→462 drift fixed,
  gate-11/19/20/12 green. Next: **G2.4** (release 20k trio).
- [ops] Session 8 (2026-11-10): reset #9 survived via §9 (tree back at
  pre-M3 `b4badea`; ff-only merge → `2722bf9` == origin/main, 0 ahead /
  138 behind, nothing lost); toolchain recovered from the persistent
  layer with NO re-download (CARGO_HOME/RUSTUP_HOME now in env.sh —
  the §3.1 item-1 protocol improved); both boards re-read in full
  before acting; next: baseline re-proof, then 13d (wave 2 closes).
- [ops] Session 7 (2026-11-10 ~08:25 UTC): reset #8 survived via §9
  (tree back at pre-M3 `b4badea`, toolchain + tools/ + env.sh wiped);
  ff-only merge → `754ae40`; rebootstrap byte-exact (the GitHub
  releases route recorded); the owner's 8-hour cron schedule verified
  COMPLETE — 0 jobs remain. Next: baseline re-proof, then 13c.
- [ops] Session 5 (2026-11-10): reset #6 survived via §9 (tree back at
  pre-M3 `b4badea`, toolchain wiped); baseline re-proven 398 green at
  6e9805a, then slice 3b landed at 414. Reset #7 also survived via §9
  (session 5 cont. 3, same recipe); baseline re-proven 418 green at
  cf8394e, then 11b-2 landed at 424. Correction of record: the
  session-4 cli per-suite labels were transposed — soe_e2e = 1,
  jar_suite = 3 (totals unchanged).
- [hygiene] Two open dependabot PRs: #1 `actions/checkout` 4.4.0→7.0.1,
  #2 `actions/setup-java` 4.9.1→6.1.0 (2026-11-05) — review after the
  baseline is re-proven; 21 open code-scanning alerts (zizmor
  ref-version-mismatch ×3, dependabot-cooldown ×2, j2r/stdout-print
  ×14+) — tracked, not wave-2 blockers.
- [pending] M4 completion-package owner review (M4 stays open until then).
- [candidate] Minicraft → corpus wave 2 (see §6).

## 9. Sandbox-reset survival protocol (proven 2026-11-09, reset #2)

Why these boards exist: the sandbox recycles and the persistent layer
can come back reverted to an old snapshot. Everything of record lives
in the authoritative repo, so recovery is always possible from GitHub.

What survived reset #2 (pushed before the reset): all decisions,
measurements (transcribed), findings F1–F5, the M5 plan/evidence, CI
state, the two open PRs, the mistake list. What died with the sandbox:
the installed toolchain (rustup, Temurin), `env.sh`, `tools/`, raw
measurement files under `work/m5invest/`, and the `work/j2r-gh` tree.

The recovery recipe (one command + verification):

```bash
cd /home/z/my-project/work/j2r-fresh   # or clone fresh if missing
git fetch origin && git reset --hard origin/main
git rev-parse HEAD   # must equal origin/main — then the board is the truth
```

Then: rebootstrap the toolchain (console_summary.md §3.1 item 1),
re-prove the baseline green (item 2), and only then continue the task
named in §8 [next]. Session rule: **append to console.md and push
BEFORE ending any session, and again after every landed commit** — the
boards only protect what has been pushed.

Raw-evidence lesson (#11, added 2026-11-09): the farm20k raw `.time`
files died with the sandbox; only their transcribed values survived in
console.md. From now on, raw measurement files are committed under
`scripts/evidence/` in the same commit as the claim they support.
