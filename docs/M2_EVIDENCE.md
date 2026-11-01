# M2 Evidence Record

Per-wave evidence in the M1.5 discipline (M2_PLAN §8 invariant 5): bug-or-
risk, reproduction, change, tests, JVM-vs-native results, CI run link.

---

## Wave 1 — catchable `StackOverflowError` (T07–T10)

**Bug/risk addressed.** Java call depth == native thread-stack depth (the
approved M1 call ABI), so deep Java recursion aborted the whole process
(native stack overflow) instead of throwing `StackOverflowError`. Any
unbounded recursion — trivial to write, common in real code — killed the
program. HotSpot throws a *catchable* `java/lang/StackOverflowError` (a
`VirtualMachineError`); the thread continues normally after a catch.

**Reproduction (pre-change).** `java -version`-free: compile
`static int down(int n){return down(n+1)+1;} down(1);` with M1.6 →
process aborts (SIGSEGV / Rust stack overflow) at some depth.

**Change.**

* New `crates/j2r-runtime/src/frame.rs`: `FrameGuard::enter()` registers a
  frame in a `thread_local` depth counter and returns
  `Err(StackOverflowError)` past the limit; `Drop` pops on normal return,
  `Err` propagation and panic unwind. **No pointers, no allocation on the
  hot path** — the Wave-1 record is a `usize` (M2_PLAN §3.2 memory-safety
  row).
* `crates/j2r-codegen/src/emit.rs`: every generated method's first
  statement is the guard; a refused registration becomes an immediate
  `return Err(t)` — the SOE surfaces at the *call site* like the JVM
  (the caller's exception table dispatches it; the callee's own handlers
  never see it, matching HotSpot where the frame never started executing).
  The generated `main` validates the env var first.
* Limit policy (M2_PLAN §3.1, §3.1.1): default 8192;
  `J2R_MAX_JAVA_FRAMES` accepted only in `1..=131_072` where
  `131_072 = 512 MiB / (512 B × 8)` from the *measured* worst-case frame
  footprint (plan-time calibration: 208–304 B across three frame shapes);
  compile-time const assertion pins `default ≤ ceiling`; invalid values
  (`0`, over-ceiling, non-numeric) print
  `j2r: invalid J2R_MAX_JAVA_FRAMES=<v>` and exit 2 before any Java code.

**Tests.**

* Runtime unit tests (7 new): const math pins the ceiling; `parse_limit`
  accepts default/1/8192/131072 and rejects `0`/`131073`/`abc`/empty/
  negative/overflow; depth accounting across nested guards; depth restored
  on panic unwind (catch_unwind); SOE object shape (class, no message,
  `VirtualMachineError` subtype); `enter()` refuses at the limit without
  registering.
* Differential fixtures (suite 34 → 36, both against Temurin 21 through
  the unchanged three-channel harness):
  * `StackDepth.java` — unbounded recursion, `catch (StackOverflowError)`
    prints `caught-soe` / `msg-null`, thread continues (`continued`,
    `sum 5`), recursion re-runs and is caught again (`caught-again`,
    `done`). Depths are never asserted (JVM-dependent by design).
  * `UncaughtSOE.java` — uncaught SOE; expected stderr is the
    `Exception in thread "main" java.lang.StackOverflowError` line (frame
    lines normalized), exit status 1 — byte-identical to HotSpot after
    normalization.
* Emitted-shape check (JDK-free, in `j2r-codegen/tests/e2e.rs`): generated
  method text contains `FrameGuard::enter()` first and the early
  `return Err` arm; generated main contains `validate_env_or_exit`.
* `crates/j2r-cli/tests/soe_e2e.rs` (release build):
  * `J2R_MAX_JAVA_FRAMES=64` → process exits **0** with `caught-soe` —
    never aborts (no SIGSEGV/101).
  * Six-case boundary matrix: unset (default) accepted, `1` accepted
    (SOE at first recursive call), `131072` accepted and completes,
    `131073` rejected exit 2, `0` rejected exit 2, `abc` rejected exit 2 —
    every rejection prints the diagnostic and produces no Java output.

**JVM-vs-native results.** Both SOE fixtures PASS three-channel against
Temurin 21.0.13. Full local run: 133 tests, 0 failures (117 unit, 12
malformed, wide-ids, JDK-free e2e, SOE e2e, 36-fixture differential in
~62 s wall-clock).

**Acceptance criteria scorecard (M2_PLAN §3.2).**

| Criterion | Result |
| --- | --- |
| (a) all 8 gates green, suite = 36 | met (CI run recorded in ROADMAP §2) |
| (b) tiny-limit e2e never aborts, prints `caught-soe`, exit 0 | met |
| (c) registry depth 0 after main returns | met (unit: nested + panic tests) |
| (d) six-case boundary matrix green | met |
| (e) calibration CI check ≤ 1.25× const | plan-time measurement recorded (§3.1.1: 304 B ≤ 640 B); the release-profile e2e proves the max accepted value completes without native overflow |

**Known deviation from plan text.** None in Wave 1. (The depth-counter
registry matches M2_PLAN §3.1 exactly; Wave 2 will extend `FrameGuard`
with the owned snapshot slot per §4.4.)

---

## Wave 2 — tracing, cycle-collecting arena GC (T11–T18)

**Bug/risk addressed.** The `Rc<RefCell<JObj>>` heap (a) leaked every
unreachable cyclic graph until the process aborted at memory exhaustion
(R1), (b) paid a refcount pair on nearly every `Value` clone (R3), (c) made
Java threads structurally impossible (`Rc` is `!Send`, R4), and (d) derived
`Object.hashCode` from the raw `Rc` address — unstable under any heap that
reuses storage (R5).

**Change.** The heap is now a non-moving mark-sweep arena with
generation-tagged `Copy` handles, exactly per M2_PLAN §4 (settle points of
plan revision 2, blockers 1/2):

* `crates/j2r-runtime/src/heap.rs` (new): `Vec<Option<Entry>>` arena,
  free list, `JRef { idx: NonZeroU32, gen: u32 }` (Copy, 8 bytes),
  iterative-worklist mark, generation-bumping sweep, threshold policy
  (`> max(2×live_at_last_gc, 4096)`), `J2R_GC_EVERY_N` forced mode,
  `gc_point()` as the single collection primitive, `with_obj`/
  `with_obj_mut` generation-checked scoped accessors (stale dereference
  panics loudly in all builds), `set_statics_scanner`, high_water/live/
  collections evidence metrics, debug re-entrancy assertions.
* `frame.rs`: `FrameGuard` gains its owned root-snapshot slot; generated
  code refreshes it before **every call of any kind** (user, native,
  virtual, static, concat, `<clinit>` ensure) and at every collection
  point; `NativeRoots` scopes root native-held refs across user dispatch
  (`throwable_display` uses it). Slots live in a LIFO registry with
  capacity reuse — allocation-free in steady state.
* Ownership conversion (T13): all object access now goes through
  `heap::with_obj`/`with_obj_mut` or the unchanged-signature helpers
  (`obj_get`, `obj_set`, `string_value`, ...). **Direct JObj borrows
  outside the heap kernel: 0** (was 45 across natives/object/array/lib).
  Census re-issued in `RUNTIME_SAFETY.md` §4.
* Identity (T14): `Object.hashCode` / `System.identityHashCode` =
  `idx` (stable across collections; distinct for distinct live objects);
  `==` = `(idx, gen)` pair equality; interned literals and the boxed cache
  are permanent roots; `System.out` is a root.
* Emission (T15): `__frame.refresh(..); heap::gc_point();` immediately
  before `new`, `newarray`, `anewarray`, `multianewarray` — the only
  collection points; refresh before every call boundary. Statics pool
  template gains `gc_scan_roots`; generated `main_run` registers it.
* Zero-unsafe enforcement (T18): `#![forbid(unsafe_code)]` in all four
  crate roots (compiler-enforced) + gate-10 grep as evidence.

**One deliberate design deviation from the plan text (documented per the
evidence discipline).** M2_PLAN §4.4 sketched a `GcFrame::current(...)`
parameter for a direct scan of the current frame at collection points. The
implementation instead **refreshes the current frame's snapshot slot at the
collection point itself** and the collector scans all occupied slots
uniformly. Consequences: (a) one scan path instead of two — simpler to
audit; (b) the exactness argument is unchanged (the current frame's
snapshot is written from its live `locals`/`stack`/`thrown` immediately
before the collection, suspended callers' snapshots are frozen at their
call boundaries); (c) over-approximation residue in slots is skipped at
mark time and any *use* of a stale handle still panics — rooting bugs
surface loudly either way. The `ensure`→`<clinit>` suspension — which the
GcFrame sketch did not cover — is handled by the same rule (refresh precedes
every implicit call too).

**Tests.**

* `heap.rs` unit tests (17): alloc/access roundtrip; unreachable object
  collected; rooted object survives 100 forced collections with generation
  stable and payload intact; simple cycle collected; rooted cycle kept;
  array children marked; interned strings are roots (identity across GCs);
  boxed-cache objects are roots; **stale handle use panics with the
  diagnostic**; stale handle never aliases the replacement; suspended-frame
  snapshot keeps a handle alive; **1 000 000-node list marked iteratively
  without native-stack overflow and reclaimed in one sweep**;
  re-entrancy flag; threshold policy math; identity-hash stability across
  collections; **200 000 build-and-drop cycle allocations with 10 live
  objects keep `high_water < 64`** (T17 acceptance, numerically).
* Differential fixtures (suite 36 → 39, three-channel vs Temurin 21):
  `Cycles.java` (rings + mutual cycles dropped in a loop — identical
  progress output proves reachability semantics), `GcIdentity.java`
  (aliasing through array + static, mutation via one path visible via the
  other across churn), `InternAcrossGc.java` (literal identity before and
  after 20 000 concat allocations).
* **Forced-GC full-suite proof (gate-9)**: all 39 fixtures byte-identical
  with `J2R_GC_EVERY_N=1` — a collection at every allocation. This is the
  rooting proof demanded by the review: any missing root dies within one
  allocation and fails the suite loudly. Verified locally (75.9 s) and wired
  as CI gate-9.
* All 133 pre-existing tests stay green (Wave-1 criterion, held).

**Wall-clock (±20 % acceptance band, M2_PLAN §4.8c).** Wave-1 baseline:
58.77 s for the 34-fixture suite (1.73 s/fixture). Wave-2: 72 s for
36 fixtures (2.0 s/fixture, +16 %) and the 39-fixture suite at ~76 s in
the same session (1.95 s/fixture, +13 % per fixture). Within the band;
the honest total-run increase also reflects the three added fixtures.

**Acceptance criteria scorecard (M2_PLAN §4.8).**

| Criterion | Result |
| --- | --- |
| (a) existing gates green + gate-9 forced-GC suite | met (CI run recorded in ROADMAP §2) |
| (b) 200 000-cycle stress, 10 live, `high_water < 64` | met (unit test) |
| (c) suite wall-clock within ±20 % | met per-fixture (+13–16 %), recorded |
| (d) `rg "unsafe"` empty; root mechanism pointer-free | met + `forbid(unsafe_code)` makes it compile-enforced |
| (e) `Object.hashCode` stable across forced collections | met (unit test + `InternAcrossGc` differential) |
| (f) stale-handle test: reuse with bumped gen panics; new object intact | met |
| (g) every emitted `invoke*` preceded by a root-snapshot refresh | met by construction (single refresh emission at every call/alloc boundary); asserted on generated text in the codegen e2e |
| (h) `rg "as_ptr\|from_raw"` over `crates/` empty | met (the `Object.hashCode` `Rc::as_ptr` is gone) |
