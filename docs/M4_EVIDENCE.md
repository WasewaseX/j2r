# M4_EVIDENCE — Waves, evidence, decisions

Live evidence record for M4 (threads & concurrency). One section per wave;
every checked register task in `M4_PLAN.md` §6 points here. Companion
records: `docs/M2_EVIDENCE.md`, `docs/M3_EVIDENCE.md` (same discipline:
no claim without a runnable artifact).

---

## Wave 1 — shared substrate (T02, T03, T04, T05)

**Goal (M4_PLAN §3 Wave 1):** heap, statics, and identity caches stop being
per-thread; program-visible behavior stays byte-identical single-threaded.

### Migration checklist — every §1 seam and its fate (R3 deliverable)

| §1 | Seam (old location) | Fate in Wave 1 |
| --- | --- | --- |
| 1 | `HEAP: RefCell<Heap>` thread_local (heap.rs) | **Shared**: `static HEAP: OnceLock<Mutex<Heap>>` (`heap()` accessor) |
| 2/3 | `alloc` never collects; `gc_point` sole collection point | Unchanged semantics; `collecting` flag and debug-asserts preserved |
| 4 | `INTERNED` thread_local (object.rs) | **Shared**: `OnceLock<Mutex<HashMap<Arc<str>, JRef>>>` |
| 5 | `OUT_REF` thread_local (object.rs) | **Shared**: `Mutex<Option<JRef>>` |
| 6 | boxing identity cache thread_local (natives.rs) | **Shared**: `OnceLock<Mutex<HashMap<(&'static str, i64), JRef>>>` |
| 7 | `CLASS_OBJECTS` thread_local (reflect.rs) | **Shared**: `OnceLock<Mutex<HashMap<Arc<str>, JRef>>>` |
| 8 | frame registry thread_local (frame.rs) | **Per-thread BY DESIGN** — `M4-SEAM` annotated; Wave 2 (T07) owns it |
| 9 | monitor re-entrancy counter (heap entry) | Unchanged (Wave 3 makes it contended) |
| 10 | generated statics pool thread_local (emit.rs template) | **Shared**: generated `statics.rs` now uses `Mutex<Vec<…>>` statics (POOL/FLAGS/MONITORS) |
| 11 | `wait/notify/notifyAll` loud-unsupported | Unchanged (Wave 3 delivers) |

**Mechanical enforcement (R3):** unit test
`j2r-runtime::m4_wave1::thread_local_seams_are_annotated` — any
`thread_local!` invocation in `j2r-runtime/src` must live in a file carrying
an `M4-SEAM` justification, and Wave 1 leaves exactly one: `frame.rs`.

### The Rc→Arc groundwork (prerequisite, commit `50ba0e6`)

`Value::Str(Rc<String>)` → `Arc<String>`, `jclass Rc<str>` → `Arc<str>`,
intern keys, boxing cache values, reflect metadata, StringBuilder COW
(`Arc::make_mut` — same API), 3 codegen emission sites. 18 runtime files,
zero residue (grepped). Without this, `Mutex<Heap>` cannot even compile:
`Mutex<T>: Sync` requires `T: Send`.

### The container decision (recorded honestly)

1. First attempt: `Mutex<Heap>` directly — **revealed that nested heap
   access is load-bearing**. A `with_obj` closure calling another heap
   accessor (e.g. `string_value` inside a closure) self-deadlocks a plain
   `Mutex`; the RefCell era allowed nested *shared* borrows.
2. Second attempt: std `ReentrantLock<RefCell<Heap>>` — **unstable on our
   toolchain** (`reentrant_lock` feature). Rejected: no nightly, no
   external crates (plan constraint).
3. Custom `ReentrantHeapLock` (owner/depth thread-locals + condvar) —
   **hit a language wall**: a `static` requires `Sync`, and any container
   holding a bare `RefCell` can never be `Sync`. No safe-Rust workaround.
4. **Final: `Mutex<Heap>` (the plan's preferred shape) + a census of every
   nested site + flat restructuring + poison-recovery locking.**

**Census method and result.** Static scan of every `with_obj`/`with_obj_mut`
closure for calls that transitively touch the heap (direct accessors,
`string_value`, `is_string`, `class_name_of`, `method_slots`, `invoke_sam`,
module re-exports), each hit manually verified against the guard's real
scope (many first-pass hits were flat code — the guard ends when
`with_obj` returns). **Four genuinely nested sites, all restructured:**

| Site | Fix |
| --- | --- |
| `array.rs::check_array` — `internal_error` inside the closure | closure carries out the class name; error built after the guard (`map_err`) |
| `natives.rs::n_method_equals` — class check + `method_slots` inside one closure | split into flat sequential accesses |
| `reflect.rs::get_declared_method` — `class_name_of` per param inside the array walk | two-phase: refs out first, names after the guard |
| `heap.rs` IMSE test — `string_value` inside the closure | extract the message ref, then read it after the guard |

`collections.rs`'s old "with_obj may nest (shared borrows)" doc replaced by
the new law. The three CME `throw_new` sites flagged by the scan were
verified flat (the guard ends before the `if`).

### Lock law (heap.rs module docs, binding for Wave 2)

* **L1 — the heap lock is a leaf.** Nothing locks another runtime container
  while a heap guard is alive; the collector's root scan is pointer-free
  and runs before the lock is taken.
* **L2 — caches may nest heap locks, never the reverse.** Intern, boxing,
  Class-object caches allocate under their own locks (the same nesting
  shape the RefCell era had).
* **L3 — no heap re-entry.** Any `with_obj` closure calling back into the
  heap deadlocks a `Mutex`; the census found and fixed all four pre-existing
  sites; the suite is the tripwire for new ones.

### Poison-recovery policy (R4 as implemented)

Tests deliberately panic under the heap lock (stale-handle tests assert the
loud panic). std mutex poisoning then cascades to every later test — the
RefCell era had no poison. Policy: every runtime lock uses
`.lock().unwrap_or_else(|e| e.into_inner())` — the panicking operation
still fails loudly, but one panic no longer poisons the whole process.
Memory-safe by construction; torn state cannot escape a generated program
because a panic unwinds to process exit. 40 sites converted (20 heap, 4
object, 3 reflect, 2 natives, 1 test-support, 10 in the generated template).

### Test-suite changes forced by the shared substrate

* **Serialization:** tests used to be isolated by thread_local state (one
  heap per test thread); the shared heap makes the runtime process-global,
  so all 113 runtime unit tests take a test-only `test_support::runtime()`
  lock. No new dependencies (`serial_test` forbidden by the plan).
* **Assertion form:** "live returns to `before`" is not a stable invariant
  on a shared heap (a collection also sweeps unrooted residue from other
  tests). `unreachable_object_is_collected`, `simple_cycle_is_collected`
  and `cycle_churn_*` now assert the intent directly on handle staleness
  (`!is_live(h)` / ring handles all live), and `cycle_churn` snapshots and
  restores the monotonic process-wide `high_water`.
* **New proofs (T05):** `shared_containers_are_send_sync` (Value/JObj/JRef),
  `shared_heap_container_is_send_sync` (`Mutex<Heap>` + scanner lock),
  `thread_local_seams_are_annotated` (R3 checker).

### Evidence (local, HEAD + wave-1 tree; CI re-runs all of it on push)

* gate-1 fmt clean; gate-2 clippy `-D warnings` clean; gate-6 doc clean;
  gate-10 zero-unsafe grep clean; gate-11 `fixtures=84 tests=256` in sync.
* gate-3: 218 unit tests green (38 classfile/inflate/zip + 67 cli/verify +
  113 runtime); malformed 12; e2e 1; wide_ids 1; jar 3; lambda 2; SOE 1.
* **Differential suite: 84 fixtures × 3 channels byte-identical vs Temurin
  21.0.13 — 288.1 s.** Corpus (real minimal-json): 5.6 s. CFG-equivalence
  84/84: 566.8 s.
* gate-4 release build green (`RUSTFLAGS -D warnings`); **gate-5 standalone:
  fixture compiled to an isolated crate with the new Mutex statics
  template, built `--offline`, ran with exact expected output.**
* gate-9 (gc-stress, forced collection at every allocation): authoritative
  run is CI's on this push (local ReflectGC wall-time cap documented in
  TODO.md §2).

### Honest deviations from the plan text

* The plan's preferred `Mutex<HeapState>` needed the four restructures
  above to be deadlock-free — the plan did not anticipate that
  RefCell-era nested *shared* borrows existed in runtime code (its L3
  narrative said the RefCell era "panicked loudly on re-entry", which is
  only true for mutable re-entry). Recorded here rather than silently
  widened.
* `OnceLock` wrappers where `Mutex::new` needs a non-const initializer
  (`Heap::new` reads `J2R_GC_EVERY_N`; `HashMap::new` is not const).
* The per-thread allocation-staging design (§3.1 alternative) remains
  documented-but-not-built, as the plan pre-approved.

---

## Wave 2 — real threads (T06, T07, T08, T09)

**Goal (M4_PLAN §3 Wave 2):** `Thread` exists; N Java threads run on N
native threads; GC stays correct under the stop-the-world protocol.

### What landed

* **T06 — `j2r-runtime/src/thread.rs` (new).** `java/lang/Thread` as a
  builtin class (5 slots: runnable, name, daemon, started, tid), the full
  native row set (start/run/join×3/sleep×2/yield/currentThread/setDaemon/
  isDaemon/getName/setName/isAlive/activeCount, four `<init>` shapes), and
  JDK-exact failure semantics (Temurin messages: IAE "timeout value is
  negative" / "nanosecond timeout value out of range", NPE "name cannot be
  null" from setName, no-message ITSE from double-start and setDaemon-
  after-start). Child threads run `Runnable` bodies — lambda closures
  (the M3 SAM machinery; `java/lang/Runnable` joined the codegen supported
  set with its 0-arity `()V` shape) or compiled classes (virtual
  `run()V` walk). Uncaught child exceptions print the JDK line and kill
  the thread only; Rust panics inside a thread kill the process loudly.
* **T07 — the thread registry.** Every thread that touches Java state
  auto-registers a `ThreadRecord` (per-thread frame slot storage moved out
  of frame.rs's thread_locals into the record — the Wave-1 seam note is
  retired; depth/limit scalars stay thread-local by final design, and
  thread.rs's `CURRENT` handle joins the gate-12 census). `start` pre-
  registers the child and stamps the record id into the Java object's tid
  slot, so `join`/`isAlive` resolve records that outlive death (records
  are never removed). `main` registers itself as the Java thread "main"
  first thing in the generated `main_run`; after the Java main returns,
  `await_non_daemons` joins non-daemon threads (the basic JVM shutdown
  rule; T15 owns the edge cases).
* **T08 — cooperative stop-the-world.** `gc_point` is now a full
  safepoint: poll (park if a collection is in flight — before any runtime
  lock, law L6) → threshold check → `StwGuard::begin` (publish
  `GC_REQUESTED`, wait until every other record is `Parked`/`Exited`) →
  mark/sweep → drop (clear flag, release everyone). The collector's root
  set gained the thread registry (each record's Java Thread object) and
  now scans EVERY thread's frame slots through the registry. The
  allocate/GC race is closed by **born-survivor marking**: an allocation
  that observes `GC_REQUESTED` under the heap lock is inserted pre-marked,
  so a handle can never name a swept object; the conservative extra keeps
  are reclaimed by the next sweep (output-invisible). Lock law extended:
  L4 (slot mutexes are leaves), L5 (registry→slots order only), L6 (never
  park holding a runtime lock).
* **T09 — the cooperative scheduler.** `J2R_SCHED=coop` (seed:
  `J2R_SCHED_SEED`) rotates a round-robin turn over *ring-participating*
  records at the yield points (`gc_point`, start/join/sleep/yield). The
  yield shape is pass-if-held / acquire-otherwise — a thread that does
  not hold the turn cannot act at a yield point, so coop output is a pure
  function of (program, seed), not of who reached the yield first. Turn
  recovery handles a holder that died or left the ring; death grants the
  turn onward. Free mode (default) is the deployment shape: every coop
  call is a fast-path no-op for the 84 pre-existing fixtures and all unit
  tests.

### The four deadlock classes found and closed during the build

(All reproduced by the first test runs, all covered by unit tests now.)

1. **Token-blocked collectors are invisible.** `StwGuard::begin` under a
   blocking token lock would wait for a thread that waits for the token —
   a cycle. Fixed: the token is taken with `try_lock`; losers park at a
   poll where the owner's release reaches them.
2. **Join-waiters looked Running.** A thread waiting in `join` mutates no
   Java state; marked `Parked` for the wait (same for `await_non_daemons`)
   so the collector never waits on a joiner→target chain.
3. **Test threads died as Running zombies.** libtest runs each test on
   its own thread; a raw exit left the record `Running`, stalling every
   later STW begin. Fixed by the `RuntimeGuard` drop hook (the test lock
   guard marks the record `Parked` on release) plus `TestDeath` in the
   thread tests.
4. **Coop turn starvation on exit.** A dying thread that owns the turn
   must grant it onward (run_body and TestDeath both do), and a
   turn-holder gone from the ring is recovered by the next yielder
   (claim-on-see). Ring participation is a property of *started Java
   threads* — bystanders can never own or starve the turn.

### Honest deviations from the plan text

* The plan's fixture evidence named coop mode for ThreadHello/
  ThreadSafepoint. Strengthened instead: both fixtures are **scheduler-
  deterministic by construction** (children never print; all output rides
  main after joins), so the differential harness compares them in FREE
  mode — the deployment shape — and the JDK join happens-before edge is
  exercised against real scheduling. The coop machinery itself is
  unit-proven (rotation order, ring visibility, recovery).
* `activeCount()` is the exact live registry count (no ThreadGroups in
  Wave 2 — documented in the native).
* `join(self)` unbounded parks (JVM-faithful); `interrupt` is Wave 4, so
  `sleep`/`join` are not yet interruptible (documented gap).

### Evidence (local, wave-2 tree)

* gate-1 fmt clean; gate-2 clippy `-D warnings` clean; gate-4 release
  build clean; gate-10 zero-unsafe grep clean; gate-12 seams PASS
  (census: frame.rs depth/limit + thread.rs CURRENT — all owner-only).
* Runtime unit tests: **123 green (was 113)** — including the STW
  handshake (allocator × collector, 20 rounds), cross-thread frame
  rooting (an object rooted ONLY by another thread's registry survives
  this thread's collection), currentThread identity across threads,
  join happens-before, coop round-robin determinism and ring-visibility,
  registry stability, JDK-exact sleep/setDaemon failures.
* **Differential: ThreadHello + ThreadSafepoint byte-identical to
  Temurin 21.0.13 on all three channels** (stdout/stderr/exit) — the
  first real-threads evidence in the project. Suite on disk: 86.
* gate-9 (a collection at every allocation) exercises the full STW
  protocol across the whole suite; authoritative run = CI on this push.

## Wave 3 — monitor contention + wait/notify (T10, T11, T12; T13 unit half)

**Commit: this push.** `crates/j2r-runtime/src/monitor.rs` (new) replaces
the M3 single-threaded `Cell<u32>` counter with a real per-entry state
machine: `unowned | owned{owner: reg_index, count: u32}` plus a FIFO wait
set, one `Arc<Monitor>` lazily stored in each heap `Entry`
(`Entry.mon: Mutex<Option<Arc<Monitor>>>` — no side table, nothing for
the sweep to clean: a contender's receiver is frame-rooted, so an entry
cannot be swept while a thread parks on it).

### Design decisions (with the alternatives rejected)

* **Mesa semantics** (HotSpot's shape): `notify` moves the FIFO-head
  waiter out of the wait set; the woken thread then competes for
  ownership like any enter-contender and reacquires with its FULL saved
  count. No handoff guarantees — matching the JDK.
* **Lock law L7** (monitor.rs module docs): heap → monitor-inner →
  sched, one-directional; parks happen only through `Condvar::wait` on
  `inner` (which releases it) or with no lock at all; the exception
  object is always allocated after the guard drops.
* **STW integration**: a thread about to park on a monitor marks itself
  `Parked` (the collector stops waiting for it) and, in coop mode,
  rotates the turn away (`thread::monitor_park_prep` — a blocked thread
  holding the turn would deadlock the ring: owner waits for the turn,
  holder waits for the owner's release). On wake:
  `safepoint_poll → coop turn re-acquisition → Running`
  (`thread::monitor_wake_prep`), called with no runtime lock held.
* **StwGuard quiescence is now ring-only**: auto-registered bystanders
  (test harness threads, embedder helpers) never poll, so waiting for
  them would hang the collector forever. They remain safe: all their
  heap mutation goes through the heap lock (held by the collector across
  mark+sweep), allocations are born-survivor-marked, and their slots are
  still scanned.
* **wait(long,int) argument order**: ownership check BEFORE the argument
  ranges (HotSpot `ObjectMonitor::wait` CHECK_OWNER order) — a non-owner
  with a negative timeout gets the IMSE. Messages pinned:
  `timeout value is negative`, `nanosecond timeout value out of range`
  (same strings as `Thread.sleep`), rounding nanos ≥ 500000 → +1ms.
* **Spurious wakeups** absorbed by the wait loops (wait-set membership +
  deadline re-checked under the reacquired guard), invisible to Java.
* `Object.wait()/wait(ms)/wait(ms,nanos)/notify/notifyAll` natives are
  real now; the M3 loud-unsupported stubs are gone.

### Test-suite hardening forced by the wave (recorded honestly)

* The Wave-2 coop tests carried two latent flakes the monitor wave's
  timing perturbation exposed: (1) the initial-turn pin raced the
  child's first yield — the pin now happens BEFORE the barrier; (2) the
  parent's loop can win the final turn exchange and, never yielding
  again, starve the child's last iteration — both coop tests now drain
  (`while !done { yield_point(); }`) and assert the ROTATION CONTRACT
  (strict holder alternation, per-thread program order, exact counts)
  instead of one OS-lucky global interleaving. Both flakes were
  reproduced flakily (2-8 runs), diagnosed via registry snapshots
  (frozen turn on a leaked/racing ring record), and closed.
* `test_support::runtime()` serialization unchanged; the four M3 monitor
  tests moved to `monitor::tests` and grew to nine, now covering:
  re-entrancy, pinned IMSE (exit/wait/notify, ownership-before-args),
  JDK-exact IAE messages, full-count release/restore across wait,
  empty-wait-set notify no-op, real contended-enter park + handover,
  timed-wait deadline (nanos rounding), and notify-wakes-waiter Mesa
  reacquisition.

### Evidence (local, wave-3 tree)

* gate-1 fmt clean; gate-2 clippy `-D warnings` clean; gate-6 doc clean;
  gate-10 zero-unsafe clean; workspace tests: all 16 targets green;
  runtime unit tests: **132 green (was 123)**. Gate-3's differential
  sweep and gate-9 run on CI for this push.
* **T13 fixtures (WaitNotify / MonitorContention / WaitTimeout
  differential vs Temurin) are the remaining half of T13** — next
  increment; the board tracks them unchecked.
