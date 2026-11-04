# M4_PLAN — Threads & Concurrency (rev 1 — ACCEPTED)

**Status: accepted (rev 1, 2026-11-04).** The owner reviewed rev 0 against
the roadmap and accepted the plan: wave order, risk register, determinism
strategy, and the out-of-scope fence all stand as drafted. Every task in
§6 is still unchecked and stays unchecked until it has recorded evidence,
same standing discipline as M2/M3. This plan
inherits two iron rules from the M2 review: no concurrency claims without
differential evidence, and zero `unsafe` (no exceptions, no "just this
once" — `#![forbid(unsafe_code)]` stays in every crate root).

---

## §0 Scope and the milestone renumbering

M2_PLAN §7 promised the thread-migration checklist "for future review".
This is that review. M4 = **threads & concurrency**, because:

1. The migration checklist was designed and deferred in M2 (§7); M3 then
   carried the single-threaded boundary forward explicitly (M3_PLAN §0).
   Every later milestone pays interest on that deferral.
2. Minecraft readiness (ROADMAP's current "M4") is thread-heavy the moment
   it is real: the client main loop, the server tick loop, the chunk
   workers, the audio mixer. Compiling a thread-heavy program on a
   single-threaded runtime is a demo, not a compiler. Threads must exist
   and be *proven* before Minecraft work starts.

**Consequence:** ROADMAP's "M4 — Minecraft readiness" becomes **M5**, and
"M5 — Mod ecosystem" becomes **M6**. The ROADMAP renumbering lands in the
same commit as this draft so the register never carries two "M4" labels;
the *task register* (§6) stays unchecked until the plan itself is
accepted. The quality-milestone candidates M3
deferred (regex, `assert`, `LineNumberTable` stack traces) stay deferred —
they are not thread work and M4 stays focused.

Out of M4 scope entirely (loud-unsupported, documented per class):

* `java.util.concurrent` (executors, atomics classes, concurrent
  collections) — M5+ candidate; the primitives it needs land in M4.
* `volatile` field semantics and `sun.misc.Unsafe`-style fences — the M4
  memory model (§5) covers monitor-based happens-before only. Field-level
  volatility needs access-mode tracking in codegen, a separate design.
* Virtual threads / continuations — stackful platform threads only.
* `Thread.stop/suspend/resume` — deprecated before most contributors were
  born; dead API, loud error.
* `wait(timeout)` with nanosecond precision guarantees — millisecond
  resolution like the JDK's actual behaviour, documented.

---

## §1 Architecture audit (verified against source, 2026-11-02)

The single-threadedness is structural today — the following are the exact
seams, all verified this session:

| # | Fact | Location |
| --- | --- | --- |
| 1 | Heap container: `thread_local! { HEAP: RefCell<Heap> }` + `STATICS_SCANNER` | heap.rs:129–133 |
| 2 | `alloc()` never collects; collection happens only in `gc_point()` | heap.rs:138, heap.rs:327 |
| 3 | `gc_point` checks forced/threshold counters, then `collect()` — no global GC-requested flag exists yet | heap.rs:327–345 |
| 4 | Interned strings: `thread_local` `INTERNED: RefCell<HashMap<Rc<str>, JRef>>` | object.rs:79–81 |
| 5 | `System.out`/`err` refs: `thread_local OUT_REF` | object.rs:136–138 |
| 6 | Boxing identity cache: `thread_local` | natives.rs:573 |
| 7 | Interned `Class` objects (M3 Wave 5): `thread_local CLASS_OBJECTS` | reflect.rs:37–39 |
| 8 | Frame registry: `thread_local`, and frames are *deliberately* `!Send` (`PhantomData<*const ()>`) | frame.rs:82, frame.rs:195 |
| 9 | Monitors: reentrancy **counts** on the heap entry; "ownership" is a counter; no contention path (documented in M3_EVIDENCE wave 4 honesty notes) | heap.rs:176–233 |
| 10 | Generated statics pool: `thread_local` in generated crates (deviation acknowledged in M2_PLAN §7) | emit.rs statics template |
| 11 | `wait/notify/notifyAll`: loud-unsupported at runtime by design | M3_EVIDENCE wave 4 |

What M2 §7 got right, and this plan builds on: handles (`JRef { idx, gen }`)
have no thread affinity; root registries are already per-thread stacks that
the collector iterates; `gc_point` is already the only collection site and
has exactly the safepoint-poll shape; the monitor slot exists in the entry
layout. The migration is an extension, not a rewrite — M2 §7 called this
correctly.

What M2 §7 did not settle (this plan must):

* **The container choice.** `Mutex<Heap>` is the obvious move but puts a
  global lock on every allocation. §3.1 adopts a two-level design instead
  (per-thread allocation staging + shared `Mutex`-guarded mark data), with
  an explicit single-lock fallback if the staged design shows any
  correctness wobble. Correctness beats cleverness; the fallback is
  pre-approved, not a failure state.
* **The scheduling model for testing.** Differential three-channel
  comparison is meaningless under OS-level nondeterminism. §4 is the
  answer: a runtime-controlled cooperative mode that makes thread
  interleavings *reproducible* on demand.
* **`Thread` object identity.** `Thread.currentThread()` must return the
  same heap object across calls in one thread; two threads' objects must
  differ. Wave 2 owns this.

---

## §2 Risk register

| # | Risk | Severity | Mitigation |
| --- | --- | --- | --- |
| R1 | **Differential testing breaks under real parallelism** — three-channel compare assumes deterministic output | top | §4: cooperative mode (`J2R_SCHED=coop`) — thread switches only at runtime-controlled yield points (safepoints, monitor ops, `Thread.yield`, `sleep(0)`); round-robin with a fixed seed. Differential fixtures run in coop mode; stress fixtures run free |
| R2 | Deadlock/livelock in the STW protocol (thread parks while holding the heap lock) | high | lock-order discipline: never park while holding any runtime lock; the GC-requested flag is checked *before* acquiring; unit tests for the two-thread park/resume handshake |
| R3 | Data race on heap internals through a missed `thread_local` seam (11 listed in §1; one missed = corrupt heap) | high | the migration is one wave with a mechanical checklist: every `thread_local!` in the runtime is either migrated or annotated with its justification; CI greps for unannotated `thread_local!` in j2r-runtime |
| R4 | `Condvar`-based parking drags in `std::sync` APIs whose poisoned-lock semantics surface as panics mid-Java-program | medium | `.lock().unwrap()` policy documented (poisoning = runtime invariant violation, loud crash is correct); monitor ops that can fail do so as Java exceptions, never as lock errors |
| R5 | Generational stale-handle detection races (two threads, one collecting) | high | sweep runs exclusively under the collector's ownership of the heap; `gen` bumps are atomic stores observed by `JRef` validation under the same lock discipline as the entry table |
| R6 | Generated-code stack (512 MB worker) × N threads exhausts memory | medium | per-thread worker stacks shrink to a measured multiple of the frame footprint ceiling (M2 §3.1.1 calibration reused); `J2R_THREAD_STACK` override; boundary matrix tested |
| R7 | Scope creep: "while we're here" executors/atomics | medium | §0 list is the fence; anything outside = new plan revision |

---

## §3 Wave design

Every wave: all prior tests stay green, zero `unsafe`, zero new external
dependencies (`std::sync` only), one reviewed commit with CI evidence
before the next wave starts.

### Wave 1 — shared substrate (no threads yet)

Goal: the heap, statics, and identity caches stop being per-thread, while
the program-visible behaviour stays byte-identical single-threaded.

* `HEAP` moves out of `thread_local` into a shared container. Preferred
  shape: `Mutex<HeapState>` where `HeapState` = entry table + mark state +
  counters; allocation takes the lock, does the push, drops it. If
  profiling shows single-threaded suite runs degrade beyond noise, the
  per-thread staging buffer design is *documented but not built* — a
  single `Mutex` is correct and M4 is not a performance milestone.
* Generated statics pool becomes process-global (`static` + `Mutex<Pool>`
  or `OnceLock`-initialized interior mutability, per emit.rs template
  changes); the `STATICS_SCANNER` hook moves with it.
* `INTERNED`, boxing cache, `OUT_REF`, `CLASS_OBJECTS` become process-global
  with internal locking (JLS identity must hold across threads — M2 §7).
  `Rc<str>` keys inside these caches become `Arc<str>` or slot-indexed
  strings — `Rc` is `!Send`, which matters the moment the container is
  shared even if the program is still single-threaded.
* `frame.rs:195`'s `!Send` marker stays for Wave 1 (frames are still
  per-thread); Wave 2 owns the registry part.
* Evidence: full suite (84) green + gc-stress gate green + new unit test
  asserting the shared caches are actually shared-visible (compile-level
  `Send`/`Sync` assertions on the container types).

### Wave 2 — real threads

Goal: `Thread` exists; N Java threads run on N native threads; GC stays
correct.

* `j2r-runtime/src/thread.rs` (**new**): `Thread` natives —
  `currentThread`, `start`, `join`, `sleep`, `yield`, `setDaemon`/`isDaemon`,
  `getName`/`setName`, `activeCount`. `Thread.currentThread()` returns a
  per-thread interned heap object (a `java/lang/Thread` builtin class whose
  slots carry the native thread id, name, daemon flag).
* Each Java thread spawns a native thread with its own: frame registry
  (the existing per-thread design, one instance per thread), worker stack
  (`J2R_THREAD_STACK`, default derived from the M2 calibration; boundary
  matrix re-tested), and a root-registration record in a global thread
  registry (`Mutex<Vec<ThreadRecord>>`); death unregisters.
* `main` becomes a Java thread too (the "main thread" record is registered
  like any other — no special collector case).
* **STW safepoints:** a global `GC_REQUESTED: AtomicBool` (or state
  machine). `gc_point()` becomes: check flag → if set, register self as
  parked-at-safepoint and wait on the GC condvar → collector scans all
  registered threads' frame registries + shared roots → sweep under heap
  ownership → release all. A thread parked *inside* a native that allocates
  (never collects, per heap.rs:138's contract) resumes after release.
  Natives never block on the flag — only `gc_point` pollers cooperate.
* `Thread.join` = wait on the target thread's completion condvar (not
  spin-sleep); `Thread.sleep` = the JDK contract (releases no monitors,
  wakes on interrupt — interrupt is Wave 4, `sleep` throws nothing in
  Wave 2 since `interrupt` doesn't exist yet — documented gap).
* Evidence: new fixtures `ThreadHello` (start/join ordering, cooperative),
  `ThreadSafepoint` (allocator thread + forcing collector handshake under
  coop mode), unit tests for the park/resume handshake (R2).

### Wave 3 — monitor contention + wait/notify

Goal: `synchronized` becomes *actually* blocking; `Object.wait/notify`
exist with javadoc-exact semantics.

* Monitor state machine per entry: `unowned | owned{owner, count}` where
  `owner` is a thread id. Contended enter parks on a per-monitor
  `Condvar` (the monitor word M2 §7 reserved becomes real).
* `wait()` requires ownership (else `IllegalMonitorStateException`,
  existing message pinned), atomically releases (fully — count to zero)
  and parks; `notify()`/`notifyAll()` wake one/all waiters; re-acquisition
  on wake restores the full count; spurious wakeups are handled by
  re-checking the wait condition — but the *Java-level* API has no
  condition, so the runtime parks until notified or interrupted; spurious
  Condvar wakeups are absorbed internally and are invisible to Java code.
* `wait(timeout)`/`wait(timeout, nanos)`: millisecond-effective timing,
  JDK-faithful rounding (`nanos >= 500000` rounds up), `IllegalArgumentException`
  on negative inputs with exact messages.
* Evidence: `WaitNotify` fixture (producer/consumer handoff, deterministic
  in coop mode), `MonitorContention` (two threads, 10k uncontended +
  contended cycles, coop-mode deterministic interleaving), `WaitTimeout`
  (timing-window assertions kept loose enough for CI runners), unit tests
  for the state machine including re-entrancy across wait.

### Wave 4 — interrupt, daemon shutdown, JMM-lite statement, stress

* `Thread.interrupt`/`isInterrupted`/`interrupted`: interrupt status flag
  per thread; `sleep`/`join`/`wait` wake with `InterruptedException`
  (exact messages); monitor-enter is NOT interruptible (JDK behaviour).
* Daemon threads: `main` exits when only daemons remain; daemon death
  during GC cooperation is handled by the registry (unregister-before-park
  discipline, unit-tested).
* **JMM-lite documented statement** (docs/RUNTIME_SAFETY.md §6): what is
  guaranteed (monitor release→acquire happens-before; `Thread.start`/
  `join` edges; `Thread.sleep` guarantees nothing) and what is NOT
  (volatile, final-field freeze, data-race freedom for racy programs).
  Racy programs may observe stale values — same as the JVM — but stale
  *handle* reads are impossible (the gen check turns torn access into a
  loud panic, and §5's discipline keeps handles off shared mutation).
* Concurrency stress: the deadlock-free assertion suite (N producer/consumer
  pairs, randomized coop seeds × fixed iterations), plus free-running
  (non-coop) stress with only invariant assertions (no output comparison).
* New CI gate **gate-13-concurrency**: coop-mode differential fixtures +
  stress, with a per-seed timeout that fails loudly. (The plan draft said
  "gate-11"; gate-11 is doc-counts and gate-12 was taken 2026-11-04 by
  the runtime-seam guard — the R3 CI grep, landed early as a named gate
  with a printed census, `scripts/check_seams.py`.)

---

## §4 The determinism strategy (the part that makes this testable)

Three-channel differential testing — the project's spine since M1 —
needs determinism. Real parallel scheduling has none. The design:

* **Cooperative mode** (`J2R_SCHED=coop`, default `free`): thread switches
  happen only at yield points — `gc_point`, monitor enter/exit/wait,
  `Thread.yield`, `Thread.sleep(0)`, `start`/`join` handoffs. A
  runtime-side scheduler (round-robin over a fixed seed from
  `J2R_SCHED_SEED`) picks who runs next. In coop mode a program's output
  is a pure function of (program, seed) — so differential fixtures pin
  `(program, seed)` pairs and compare all three channels exactly like M1
  did.
* **Free mode** is the real deployment shape: no scheduler, OS schedules.
  Nothing in free mode may depend on coop-only timing (checked by running
  the stress suite in free mode).
* **Yield-point density is the fidelity cost**: coop mode cannot expose a
  race that needs interleaving *inside* a single non-yielding instruction
  run. That's fine — such races cannot exist in Java-level semantics
  either (the JVM's JMM gives the same granularity through its own
  synchronizations). The plan's claim, stated precisely: coop mode
  explores all interleavings reachable at Java-level action boundaries;
  it does not claim to explore instruction-level races inside natives.
  Natives are single-owned (heap lock) so their internals are not racy.

This is the same honest-boundary discipline as M2 §7: claim exactly what
is proven, document exactly what is not.

---

## §5 Memory-model statement (summary; full text in RUNTIME_SAFETY.md §6 at Wave 4)

1. Handles are plain data; sharing a `JRef` across threads is legal and
   its `(idx, gen)` validation is authoritative under the collector's
   lock discipline.
2. Object *contents* follow the JDK model: racy plain-field access is a
   data race with JVM-equivalent observable behaviour (stale reads
   possible); monitors are the happens-before tool, as in Java.
3. The runtime's own state (entry table, caches) is protected internally;
   no Java-level action can observe a torn runtime invariant — a violation
   of that invariant is a loud panic (documented `.unwrap()` policy),
   never silent corruption.

---

## §6 Task register (evidence before check — T01 accepted)

| Task | Wave | Delivers |
| --- | --- | --- |
| T01 | 0 | Plan reviewed/accepted (rev 1, owner directive 2026-11-04); ROADMAP renumbering verified in repo: `M4 — threads & concurrency`, `M5 — Minecraft readiness (was M4)`, `M6 — Mod ecosystem (was M5)` |
| T02 | 1 | **DONE** — shared heap container (`Mutex<Heap>`) + full migration checklist: docs/M4_EVIDENCE.md §Wave 1 |
| T03 | 1 | **DONE** — shared statics pool; generated `statics.rs` template on `Mutex` statics; gate-5 standalone re-proven |
| T04 | 1 | **DONE** — shared identity caches (intern, boxing, out/err, Class objects), `Arc` keys/values |
| T05 | 1 | **local evidence green** — suite 84×3ch byte-identical (288.1s), corpus 5.6s, cfg-equiv 84/84 (566.8s), standalone run, tests=256 in sync, sharing/seam unit tests; gate-9 authoritative run = CI on this push |
| T06 | 2 | **DONE** — `j2r-runtime/src/thread.rs`: Thread natives (start/join/sleep/yield/currentThread/daemon flag/name/isAlive/activeCount/run, four `<init>` shapes), JDK-exact failure strings; `java/lang/Runnable` joined the codegen SAM set — docs/M4_EVIDENCE.md §Wave 2 |
| T07 | 2 | **DONE** — per-thread frame registries in the global thread registry (`ThreadRecord`), main thread registered first thing, `await_non_daemons` shutdown rule, worker-stack sizing (`J2R_THREAD_STACK`, M2-calibration default) |
| T08 | 2 | **DONE** — STW safepoint protocol (`GC_REQUESTED`, `StwGuard`, park/resume, collector scans all registries) + born-survivor allocation marking; lock law L4–L6 |
| T09 | 2 | **local evidence green** — runtime 123 tests (handshake, cross-thread rooting, identity, join HB, coop determinism/ring/recovery, registry stability); ThreadHello + ThreadSafepoint byte-identical vs Temurin (all 3 channels); gate-9 authoritative run = CI on this push |
| T10 | 3 | **DONE** — `monitor.rs` per-entry state machine (owner + count + FIFO wait set), lazy `Arc<Monitor>` in the heap entry, contended enter with park protocol; 9 monitor unit tests |
| T11 | 3 | **DONE** — Mesa wait/notify/notifyAll with full-count release/restore; IMSE wording pinned to Temurin 21 ("current thread is not owner") |
| T12 | 3 | **DONE** — JDK-exact wait(long[,int]): ranges BEFORE ownership (Temurin OrderProbe falsified the old pin), rounding, fixed deadline; monitorexit IMSE message-less (classfile probe) |
| T13 | 3 | **DONE** — WaitNotify/MonitorContention/WaitTimeout differential vs Temurin 21.0.13+9; caught the clinit first-touch race → JLS 12.4.2 gate (`clinit.rs`); MonitorContention 0/30 flaky; tests 277; docs/M4_EVIDENCE.md §Wave 3 addendum |
| T14 | 4 | Interrupt semantics (status flag, wake-with-exception paths) |
| T15 | 4 | Daemon shutdown rules + registry edge cases |
| T16 | 4 | JMM-lite statement in RUNTIME_SAFETY.md §6 |
| T17 | 4 | Stress suites (coop seeded + free-running) + gate-13-concurrency |
| T18 | 4 | M4 completion package + review submission |

Dependency chain: T01 → T02 → T03 → T04 → T05 → T06 → T07 → T08 → T09 →
T10 → T11 → T12 → T13 → T14 → T15 → T16 → T17 → T18. No wave starts
before the previous wave's CI evidence exists. Wave 2 evidence:
docs/M4_EVIDENCE.md §Wave 2 (four deadlock classes found and closed are
recorded there too).

---

## Revision history

- Rev 1.1 (2026-11-04): gate numbering fixed — the Wave-4 concurrency gate
  is **gate-13** (11 = doc-counts, 12 = runtime-seam guard, landed early
  with the gate-upgrade package); no plan semantics changed.
- Rev 1 (2026-11-04): accepted. Owner directive "continue the roadmap"
  recorded as plan acceptance; renumbering re-verified in ROADMAP.md
  (M4 threads / M5 Minecraft readiness / M6 mod ecosystem); §1 audit line
  numbers to be re-verified against source at Wave-1 start (T02 evidence,
  docs/M4_EVIDENCE.md) since the corpus wave touched neighboring files.
- Rev 0 (2026-11-02): draft. Architecture audit verified against source
  (11 seams, §1); risks R1–R7; four waves; determinism strategy §4;
  18-task register, all unchecked. Pending review before any task starts.
