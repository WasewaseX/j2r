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
