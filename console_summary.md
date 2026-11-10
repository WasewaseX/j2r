# j2r console summary — the state board

Updated 2026-11-10 (session 5 cont. 2 — **T11 slice 11b part 1 landed:
pending-exception cell + method IDs + ten virtual Call*MethodA, 418
green**; T10 COMPLETE at 414. See console.md session-5 entries and §9).
Raw log: `console.md`.
**Read this file before acting**; if a memory disagrees with this board,
the board wins.

## 1. What the project is

j2r: a Java → Rust compiler written in safe Rust (zero `unsafe`,
compiler-enforced). Input is compiled Java `.class` bytecode (not Java
source); output is Rust source that is compiled by rustc into a native
binary. Correctness law: three-channel differential testing (stdout /
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
| **M5** | **Minecraft readiness** | **STARTED 2026-11-08** — wave 1 nearly closed |
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
zero divergences. Remaining wave 2: T11 (Call*Method + arity note) ·
T12 (RegisterNatives) · T13 (C harness + attach lifecycle). Then T14–T16
mappings pipeline · T17–T21 mixin applier · T22–T23 inline caches. The
M4 completion package still awaits owner review in parallel.

### 3.1 T08 preconditions — the wave-2 entry checklist (all must be green before T08 code)

1. **Toolchain re-proven after every reset** (environments rot — T05's
   law): rustup → rustc/cargo 1.99.1, Temurin 21.0.13 →
   `/home/z/my-project/tools/jdk-21.0.13+9` (session 3 moved the JDK to
   the persistent layer; /tmp is not trusted), `source
   /home/z/my-project/env.sh`. Status: **DONE (session 5; was session
   4)** — exact pinned versions restored again (reset #6 reverted the
   persistent layer; tarball 207,858,352 B ≈ the recorded 199 MB);
   rebootstrap download measured: rustup-init 21 MB + Temurin tarball
   199 MB.
2. **Full local baseline green before the first T08 commit**: unit tests
   across all four crates + `diff_suite` (three-channel vs Temurin) +
   corpus suite; test counts re-synced per gate-11. Status: **DONE
   (session 4, re-proven again at 84734d6 before T10 slice 2)** —
   **388 tests, 0 failures** (classfile 38, runtime 192, codegen 125,
   jni-ffi 7, cli 26 incl. cfg_equiv 324 s + diff_suite 426 s, zero
   divergences; fmt clean, clippy `-D warnings` clean). Re-proven at
   **390** after T10 slice 2 (runtime 193, jni-ffi 8, cli 26;
   cfg_equiv 347 s + diff_suite 430 s, zero divergences). Re-proven at
   **398** after T10 slice 3a (runtime 201; partition 24.5 s,
   cfg_equiv 348 s + diff_suite 422 s, zero divergences). Re-proven at
   **414** after T10 slice 3b (runtime 217; cfg_equiv 339.36 s +
   diff_suite 438.24 s, zero divergences; cli per-suite labels
   corrected: soe_e2e 1, jar_suite 3).
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
| no-op rebuild | 357.5 s → 0.535 s (668×), 1k release | T06/T07 |
| one-body-edit rebuild | 71.0 s (5×), only its partition + app recompile | T06/T07 |
| closure-cache effect | compile stage 7.98 → 2.02 s @ 5k (−75 %) | T03 |
| **20k end-to-end (this session)** | cold ≈ **17.1 min**; no-op **38.8 s** (~26×); one-edit **55.3 s**; parity PASS | farm20k/*.time |
| 20k peak rustc | 3.49 GB — the app/Dispatch composite (F3 follow-up) | farm20k |
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
- [F1] emit-stage super-linearity at 20k — needs a profile (34.12 s vs
  T03's near-linear 2.02 s @ 5k).
- [F2] no-op emit SHA-256-rehash-bound (~38 s @ 20k) — (mtime,size) fast
  path registered.
- [F3] app/Dispatch composite = rustc near-wall at 20k (3.49 GB) —
  split-table emission registered (same follow-up family as the JZlib
  mega-functions).
- [F4] release-profile 20k trio — registered (disk-guarded).
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
- [~] **T11** — design note docs/M5_JNI_CALLS.md LANDED (11a, the F-J2
  resolution: A-forms workhorse / V-forms via the SysV save-area walk /
  bare forms loud, honestly typed; pending-exception cell scoped). Part
  1 LANDED (11b-1): the per-thread pending cell (ExceptionOccurred/
  Clear/Check slots 15/17/228 real — the first faithful exception
  transport); the bit-62 method-ID registry (GetMethodID/GetStatic—
  MethodID 33/113, ancestor walk, NoSuchMethodError pended on miss);
  the A-form invoke core (descriptor-driven jvalue decode, virtual
  dispatch by the receiver's runtime class, zero-return-on-throw); 15
  slots wired (91 → 106); 4 unit tests; 418 green. NEXT (11b-2): the
  mock-dispatch invoke tests (override vs declared-class), the
  static/nonvirtual families, the FULL battery; then 11c: V-forms via
  the save-area walk + the gate-10 amendment (6 used of pin 12).
- [ops] Session 5 (2026-11-10): reset #6 survived via §9 (tree back at
  pre-M3 `b4badea`, toolchain wiped); baseline re-proven 398 green at
  6e9805a, then slice 3b landed at 414. Correction of record: the
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
