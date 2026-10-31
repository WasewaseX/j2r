# J2R — M2 Implementation Plan (generated code quality)

**Status: PLAN ONLY — nothing in this document has been implemented.**
**Per the M1 architectural review, coding may not begin until this plan
itself passes architectural review.**

Approved baseline this plan builds on: commit `c07a10f` (M1.6), CI run
`41201763892` SUCCESS on all 8 gates (2026-10-31): 110 unit tests, 12
malformed-classfile e2e tests, 1 wide-ids e2e test, 34-fixture differential
suite against Temurin 21 (three-channel compare: stdout, stderr, exit
status), zero `unsafe` in the workspace, `j2r-runtime` and `j2r-classfile`
have zero external dependencies.

The fine-grained working todolist for all M2 work — 22 tasks (T01–T22)
with statuses and the dependency chain — lives in `docs/ROADMAP.md` §4.
This document carries the design depth and the per-change specifications
the review demands; the roadmap carries the ordering and the status.

M2 scope (per the review and ROADMAP.md):

1. `StackOverflowError` instead of process abort on deep recursion
2. A tracing, cycle-collecting garbage collector replacing the `Rc` heap
3. Typed operand-stack specialization
4. CFG reconstruction (investigation; no default-emission change in M2)

The review requires, for every proposed change: exact files and modules
affected, Java semantic requirements, memory-safety implications,
performance tradeoffs, differential tests against OpenJDK, regression
tests, and measurable acceptance criteria. Sections 3–6 give each design
in that structure. Section 8 orders the work into four independent waves
and states the global invariants.

---

## 1. Audit of the current architecture (verified against the source, 2026-10-31)

### 1.1 Operand stack and locals (generated code)

Every Java method compiles to one Rust function (emit.rs `emit_method`,
lines 468–590):

```rust
pub fn m_<name>(recv: Option<&JRef>, args: &[Value]) -> Result<Value, Throw> {
    let mut locals: Vec<Value> = vec![Value::Void; max_locals];
    j2r_runtime::unmarshal_args(&mut locals, args, desc, start)...;
    let mut stack: Vec<Value> = Vec::with_capacity(max_stack);
    let mut pc: u32 = 0;
    let mut thrown: Option<Throw> = None;      // pending throw object
    let mut fault_pc: u32 = 0;                 // pc of the faulting instruction
    'run: loop {
        if let Some(t) = thrown.take() { /* exception-table dispatch */ }
        match pc { 0u32 => { /* basic block */ } ... }
    }
}
```

* The uniform value representation is `Value` (runtime/lib.rs 49–59):
  `Void | Int(i32) | Long(i64) | Float(f32) | Double(f64) | Str(Rc<String>)
  | Ref(Option<JRef>)`. Category-2 values are a single `Value`; all
  `dup*/pop2/swap` forms branch on `Value::cat2` (runtime/stack.rs, spec
  tables in its header, unit-tested).
* Every stack/local operation goes through runtime helpers
  (`pop_int`, `stack_dup2_x2`, ...). Cloning a `Value` clones an `Rc`
  (refcount increment/decrement) on nearly every `Load`, dup and
  argument passing.
* Handler entry clears the stack and pushes the throwable (emit.rs
  544–551) — exactly the one-reference handler-entry shape the verifier
  enforces.

### 1.2 Typed IR

* `ir::Instr` (codegen/ir.rs 15–133) is a pc-annotated, fully decoded
  instruction list. The decoder is deliberately mechanical — all typing
  judgement lives in the verifier. `jsr`/`ret` are rejected at decode
  time (ir.rs 413–418): the IR never sees pre-Java-6 subroutine calls.
* The verifier (codegen/verify.rs) computes, per method, the abstract
  state at every instruction: `State { stack: Vec<VTy>, locals: Vec<VTy> }`
  over the lattice `VTy = Unset | Poison | Top | Int | Long | Float |
  Double | Ref | Array(ElemKind) | Null | HighHalf` (verify.rs 76–101).
  Joins require equal stack height and compatible kinds; reference
  subtypes merge to opaque `Ref` (no class names in the lattice).
  36 unit tests pin the semantics. **Key M2 fact: the per-pc stack/local
  shape this pass already computes is exactly the input a typed-stack
  specialization pass needs; it is currently discarded after validation.**

### 1.3 CFG representation

There is no CFG data structure in the compiler. emit.rs `compute_leaders`
(608–660) derives a `BTreeSet<u32>` of basic-block leaders (branch targets,
fall-through after conditionals, return/athrow, handler pcs) and emission
is `match pc { leader => block }` inside `'run: loop`. Control flow is
data (the `pc` variable), not structure. This is the audited and approved
M1 architecture: correct for arbitrary, even irreducible, bytecode.

### 1.4 Heap ownership (runtime)

* `JRef = Rc<RefCell<JObj>>`; `JObj { jclass: Rc<str>, slots: Vec<Value> }`
  (runtime/object.rs 15–25). Java identity is `Rc::ptr_eq`; Java mutation
  is `RefCell` interior mutability; there is no `unsafe` and no aliasing UB.
* Arrays are `JObj`s whose class is the descriptor (`[I`, `[[I`, ...);
  slot 0 holds the length, slots `1..=len` the elements (array.rs).
  Multi-dimensional allocation is recursive with per-level descriptors.
* `java/lang/String` objects hold their character payload as
  `Value::Str(Rc<String>)`; `StringBuilder` shares that payload via
  `Rc::make_mut` copy-on-write (object.rs 57–72), so the JDK aliasing
  behaviour (`sb.toString()` then `sb.append()`) is exact.
* Process-wide single-threaded state, all `thread_local`:
  string-intern cache (object.rs 74–90), `System.out` handle
  (object.rs 132–157), the JLS-mandated boxed-value identity cache
  (natives.rs 521–534), and the generated crate's static field pool +
  init flags (emitted `statics.rs`, emit.rs 140–237).
* **`Object.hashCode` returns the raw `Rc` address truncated to `i32`**
  (natives.rs 107–113). This is *unstable under any heap that can move
  or reuse storage* — a direct M2 constraint (§4).
* The generated `main` runs the program on one spawned thread with a
  512 MB stack (emit.rs 253–300) as the current deep-recursion mitigation.

### 1.5 Generated-call ABI

`MethodFn = fn(Option<&JRef>, &[Value]) -> Result<Value, Throw>`
(runtime/lib.rs 124). Calls compile to plain native Rust calls
(user target: direct `crate::classes::…::m_…`; virtual/interface: runtime
`invoke_virtual` walking the receiver's class chain; natives: `invoke_exact`).
Consequence: Java call depth == native thread-stack depth. There is no
Java-visible frame, so there is nothing to depth-check today — the root
cause of the missing `StackOverflowError` (§3).

---

## 2. Semantic risks in the `Rc`-based object model

Numbered for reference from the designs below.

* **R1 — Cycles leak (documented, RUNTIME_SAFETY.md §2.4).** A cyclic
  graph unreachable from roots is never reclaimed. Semantics stay correct
  (the objects are still reachable from Java's point of view; no
  double-free, no resurrection), but a long-running program that churns
  cyclic graphs eventually exhausts memory and the process aborts like any
  OOM native program. Java instead keeps the program running (GC reclaims
  the cycle) and, at a true heap limit, throws a *catchable*
  `OutOfMemoryError`.
* **R2 — No heap ceiling.** Related to R1: there is no allocation
  accounting, hence no possibility of `OutOfMemoryError` semantics at all
  (which the JVM is allowed to throw). M2 fixes the cycle-reclamation
  half; a *configurable heap limit with catchable OOME* is explicitly out
  of M2 scope (see §10) because no deterministic differential test can
  pin OOME timing against HotSpot.
* **R3 — Refcount overhead on the hot path.** Every `Value` clone/drop is
  an `Rc` refcount pair; the operand stack, locals, dup families and
  argument marshalling clone constantly. `Option<JRef>` cannot be `Copy`.
* **R4 — `Rc` is `!Send`, so the current heap can never host Java
  threads.** The reviewer's requirement that the GC architecture
  eventually support real threads (§7) is unreachable with `Rc`; this is
  the structural reason M2 replaces the heap instead of patching it.
* **R5 — Identity hash instability under storage reuse.** The current
  `Object.hashCode` (raw pointer bits) is only correct because objects
  never move and are never freed while reachable. Under any collector
  with a free list, identity hashes must become lifetime-stable by
  construction (§4.6), not address-derived.
* **R6 — `RefCell` reentrancy.** Audited site-by-site in
  RUNTIME_SAFETY.md §4 (30 borrow sites in natives.rs, 6 in object.rs,
  4 in array.rs, 5 in lib.rs; every borrow is statement-local or dropped
  before dispatching into user code). Any heap redesign must preserve
  this discipline and the census must be re-issued (§4.7).
* **R7 — `Value::Str(Rc<String>)` payloads are outside the object
  graph.** A tracing GC that only traces `JRef`s must still keep
  payload sharing (`string_from_rc`) correct: when a `String` object dies
  its payload `Rc` drops; a `StringBuilder` aliasing it keeps the buffer
  alive by refcount. This is sound under a non-moving collector and must
  be pinned by tests (§4.8).
* **R8 — No rooting discipline exists.** With `Rc`, every live handle
  roots itself; a tracing GC makes that false. The design must introduce
  an explicit, mechanically enforced root set before any collection can
  run (§4.3). This is the single largest correctness risk of M2 and it
  gets its own wave-ordering justification (§8).

---

## 3. Design A — `StackOverflowError` (Wave 1)

### 3.1 Approach

Keep native recursion (the approved architecture — no rewrite of the call
ABI). Add an explicit Java-frame registry with a depth counter:

* Every generated method body wraps its `locals`, `stack` and `thrown` in
  a `FrameGuard` (new runtime module `frame.rs`) created at method entry.
  Construction pushes a frame record (raw pointers to the three slots, no
  allocation) onto a `thread_local` frame stack; `Drop` pops it — correct
  on normal return, on `Err` propagation, and on panic unwind alike.
* Construction first checks the depth: if `depth + 1 > limit`, it returns
  the `Throw` for `java/lang/StackOverflowError` instead of registering —
  emitted code turns that into `return Err(...)` before doing any work.
  The limit defaults to 8192 and is overridable via
  `J2R_MAX_JAVA_FRAMES` (used by tests; 0 is rejected as unsafe).
* Native→user re-entry (`toString` during concatenation, future
  comparators) passes through `invoke_virtual` into a generated method,
  so it is counted by the same mechanism with no extra instrumentation.

Why this is safe against real native-stack exhaustion: the 512 MB worker
stack gives a budget of 64 KB of native stack per Java frame at the
8192 limit. A generated frame's native footprint is small (three `Vec`s
are heap-allocated; the dispatch `match` does not recurse), measured at
well under 1 KB today; natives add comparable, bounded frames. The limit
is therefore two orders of magnitude away from danger, and the default
mirrors the order of magnitude of HotSpot's default (~10–20 k frames on
a 1 MB thread stack).

### 3.2 Per-change specification

| Requirement | Detail |
| --- | --- |
| Files/modules affected | New `crates/j2r-runtime/src/frame.rs` (registry, depth counter, `StackOverflowError` constructor); `crates/j2r-runtime/src/lib.rs` (module + `builtin.rs` already maps `StackOverflowError → VirtualMachineError`, no change); `crates/j2r-codegen/src/emit.rs` (`emit_method`: emit guard, depth check, early `return Err`); `crates/j2r-cli/tests/diff_suite.rs` (fixture-count guard 34 → 36) |
| Java semantic requirements | JLS/JVMS: `StackOverflowError` is a `VirtualMachineError`, thrown asynchronously-ish on thread-stack exhaustion, **catchable**; after a caught SOE the thread continues normally; uncaught SOE prints `Exception in thread "main" java.lang.StackOverflowError` (no message) and exits 1. Depth at which it fires is *not specified* (depends on JVM stack size) — differential fixtures must therefore never assert a depth. |
| Memory-safety implications | In Wave 1 the registry needs **no pointers at all**: the record is a depth counter (push/increment, pop/decrement via `Drop`), so there is nothing to dereference and nothing to leak — Wave 1 introduces no new memory-safety surface beyond a `thread_local usize`. Scanning frame *contents* is a Wave 2 requirement (§4.4); that extension's safety design — preference order (1) scoped-lifetime access during marking, (2) quarantined raw pointers with the LIFO + collection-point argument — is specified in §4.8 and reviewed as part of Wave 2, not smuggled in here |
| Performance tradeoffs | One `thread_local` push/pop and one counter check per call. Expected effect on the differential suite: within noise (< 5 %). Measured and recorded in the evidence doc; if a regression shows up, the fallback is checking depth only every K frames (K=8) with the guarantee that the native-stack budget still holds at K granularity. |
| Differential tests vs OpenJDK | New fixtures: (1) `StackDepth.java` — unbounded recursion, `catch (StackOverflowError e)` prints `caught-soe` and returns normally; (2) `UncaughtSOE.java` — unbounded recursion with no handler; expected stderr is the uncaught-SOE line (frame lines normalized by the existing harness), exit status 1. Both run against Temurin 21 through the unchanged three-channel harness. |
| Regression tests | Unit tests in `frame.rs`: depth accounting across normal return / throw / panic; limit override via env; SOE object shape (class, no message, subtype of `VirtualMachineError`); native re-entry counted (concat → `toString` recursion builds frames). Existing `Recursion.java` fixture must stay green unchanged. |
| Measurable acceptance criteria | (a) All 8 CI gates green with the suite grown to 36 fixtures. (b) A dedicated e2e test compiles and runs a recursive program with a deliberately tiny `J2R_MAX_JAVA_FRAMES=64` and asserts the process exits 0 with `caught-soe` on stdout — i.e. the process **never aborts** (no SIGSEGV/101) at any limit. (c) Frame accounting unit test: after `main` returns, registry depth is 0. |

---

## 4. Design B — Tracing garbage collector (Wave 2)

### 4.1 Chosen architecture

A **precise, non-moving, mark-and-sweep arena GC** with index handles:

```rust
// crates/j2r-runtime/src/heap.rs (new)
pub struct JRef(NonZeroU32);            // Copy; Java identity == handle equality
struct Entry { obj: RefCell<JObj>, mark: Cell<bool> }
pub struct Heap { entries: Vec<Entry>, free: Vec<u32>, live: usize, ... }
```

* **Handles, not pointers.** `JRef` becomes `Copy`; `Option<JRef>` is
  4 bytes; `Value` drops its refcount traffic (R3). `JRef::ptr_eq` becomes
  integer equality. `MethodFn` keeps its shape with `Option<JRef>` passed
  by value (a mechanical, compiler-enforced change across generated code
  and the ~30 native signatures).
* **Non-moving** is chosen deliberately: object storage never relocates,
  so (a) raw pointer use stays at zero, (b) `jclass`/slot semantics are
  trivially stable, (c) `System.identityHashCode`/`Object.hashCode` can be
  the handle value — stable for the object's lifetime (R5 fixed by
  construction). Compaction is a possible later optimization and would
  first require a per-object hash header; it is out of scope (§10).
* **Zero `unsafe` preserved**: marking iterates an explicit `Vec<u32>`
  worklist (never recursive — a 1 M-node list must not overflow the
  native stack while marking); sweeping rebuilds the free list; `Cell`
  mark bits sit beside, not inside, the `RefCell`, so mark writes never
  conflict with object borrows.
* **Rejected alternatives** (recorded because the review asked for a
  design, not a leap): (i) keeping `Rc` and adding a Bacon–Rajan-style
  cycle collector — not a tracing GC, does not unlock threads (R4), and
  its trial-deletion bookkeeping is harder to audit than mark/sweep;
  (ii) conservative scanning of the Rust stack — unsound and requires
  `unsafe`; (iii) a moving/compacting collector — forces `unsafe` or
  double indirection for marginal M2 benefit.
* **`Value::Str` payloads stay as `Rc<String>`** (R7): they are immutable,
  COW-shared, and die with their `JObj`'s slot vector on sweep. No
  correctness change; verified by existing `StringBuilder` tests plus new
  GC fixtures (§4.8).

### 4.2 Collection policy

* First collection when the arena reaches 4096 live-or-free entries;
  afterwards when `allocated_since_last_gc > max(2 × live_at_last_gc,
  4096)`. Java does not specify GC timing, so this is a correctness-free
  tuning knob; the differential harness never observes it directly.
* `J2R_GC_EVERY_N` forces a collection at every Nth collection point
  (`N=1` = collect at every allocation) — the *forced-GC mode* used as
  the rooting proof (§4.8, §9).

### 4.3 The GC safety invariant (the heart of Wave 2)

**Collection may only be triggered from generated code, immediately
before an allocating opcode.** The API split enforces it:

* `heap::alloc(JObj) -> JRef` — pure allocation, never collects. Used by
  every runtime/natives path (`throw_new`, `new_string`, array
  allocation, boxed cache fill, interning).
* `heap::gc_point()` — may collect. Emitted **only** before the four
  allocating opcodes: `new`, `newarray`, `anewarray`, `multianewarray`
  (emit.rs emission sites at lines 979–1019). These are the only
  collection points in the entire system.

Why this is sound without a write barrier or read barrier:

1. **Native code never collects.** A native holding `JRef` temporaries
   across its own allocations (`throw_new` inside `pop_ref` failure, etc.)
   can never observe a collection; handles stay valid by definition. A
   debug assertion in `Heap` counts `gc_point()` vs `alloc()` re-entrancy
   so a future native that calls `gc_point` fails tests (§4.8).
2. **Between generated instructions, every reachable handle is either in
   a registered frame, in the statics pool, or in a runtime root** —
   there is nowhere else (verified below).
3. **The window between a caller popping args and the callee registering
   its frame contains no collection point**: `pop_n`/`unmarshal_args`
   allocate no Java objects, and `gc_point()` only precedes allocation
   opcodes inside a method body whose frame is already registered.

### 4.4 Root set

1. **Java frames** — the Wave 1 registry. `FrameGuard` extends to expose
   `scan(&mut dyn FnMut(u32))` over `locals`, `stack` and `thrown`
   (a pending throw holds a live `JRef` across handler dispatch).
2. **Statics pool** — the generated crate's `statics.rs` gains
   `gc_scan_roots(f: &mut dyn FnMut(u32))`; the emitted `main_run`
   registers it with the runtime at startup. (The same hook later serves
   shared statics when threads arrive, §7.)
3. **Runtime roots** — intern cache, boxed-identity cache, `System.out`
   handle (all already `thread_local` structures in the runtime; scanned
   by the heap module directly).
4. **In-flight arguments**: none — per §4.3.3, no collection point
   exists outside registered frames.

### 4.5 What sweep does

An unmarked entry is removed, its index goes to the free list, and
dropping its `JObj` releases the `jclass` string and the slots vector
(which recursively drops `Value`s — pure field drops, no recursive
object destruction, since handles are plain integers). Free-list reuse
gives handle reuse after death — permitted by Java identity-hash rules
(hash stability is required only for live objects; §4.6).

### 4.6 Identity and hashing

* `==` (`if_acmpeq/ne`, `Object.equals`): handle equality — unchanged
  semantics.
* `Object.hashCode` / `System.identityHashCode`: **the handle value**
  (natives.rs 107–113 changes from `Rc::as_ptr`). Stable for a live
  object regardless of collection; distinct live objects have distinct
  handles by construction. (HotSpot's identity hash is likewise
  unspecified numerically; differential tests may only assert
  self-consistency, which the fixture does — §4.8.)
* String interning: interned literals are strong roots; `ldc` twice
  yields the same handle forever — matches the constant-pool guarantee
  and the existing `intern_returns_same_object` test.

### 4.7 Ownership conversion and the re-issued borrow census

`JRef::borrow()` cannot return a `Ref` that escapes the `thread_local`
heap, so the ~45 direct `.borrow()`/`.borrow_mut()` call sites convert to
scoped access — `heap::with_obj(r, |obj| ...)` — or to the existing
helper functions (`obj_get`, `obj_set`, `string_value`, `is_string`,
`checkcast`, ...) whose *signatures stay identical* (generated code is
largely untouched; the churn is inside the runtime). Deliverable: a
re-issued, per-file census in RUNTIME_SAFETY.md §4 proving the same
statement-local borrow discipline, plus the same "no reentrancy path"
argument for the new accessors.

### 4.8 Per-change specification

| Requirement | Detail |
| --- | --- |
| Files/modules affected | New `crates/j2r-runtime/src/heap.rs` (heap, mark/sweep, roots, thresholds, `JRef`); rewrite of `crates/j2r-runtime/src/{lib.rs, object.rs, array.rs, natives.rs, concat.rs}` (handle mechanics, closure accessors); `crates/j2r-codegen/src/emit.rs` (`gc_point()` emission before the 4 allocating opcodes; `statics.rs` template gains `gc_scan_roots`; `main.rs` template registers it); `crates/j2r-cli/tests/diff_suite.rs` (guard bump 36 → 39); `.github/workflows/ci.yml` (new gate-9, §9); docs RUNTIME_SAFETY.md §1/§2/§4 rewritten for the new ownership model |
| Java semantic requirements | JLS: object reachability is the only observable; GC timing is not. Must preserve: reference identity and mutation visibility through any alias at any time (including across collections); interned-literal identity (JLS 3.10.5 / `String.intern`); JLS 5.1.7 boxed-cache identity; `Throwable` cause/suppressed graphs (cycles are ordinary objects — currently unimplementable via the two-arg constructor, but the object model must not preclude M3); array element aliasing; `StringBuilder`/`String` COW sharing |
| Memory-safety implications | All safe Rust: `Vec<Entry>` (stable addresses, non-moving), `NonZeroU32` handles, `Cell<bool>` mark bits, explicit worklist. The rooting discipline is the safety kernel: §4.3's three-point argument + forced-GC mode. Dangling handles are impossible in safe code (lookup by index always yields a valid entry or the object is unreachable and the handle provably dead); stale-handle *reuse* is ordinary Java memory-model behaviour. The Wave 2 frame-scan extension follows the safe-Rust preference order specified in §3.2/§4.4 (scoped-lifetime access during marking first); if quarantined raw pointers survive review they live only in `frame.rs` with the LIFO + collection-point argument written above their definition |
| Performance tradeoffs | Wins: no refcount traffic on any `Value` clone (operand stack, locals, args); `Option<JRef>` becomes `Copy` (4 B); allocation is a free-list pop instead of `malloc`. Costs: mark pauses proportional to live objects; non-moving arena retains free-list fragmentation; ~1 word header per object. Expected net: neutral to faster on the fixture suite; the evidence doc records wall-clock before/after with a ±20 % acceptance band (§8) |
| Differential tests vs OpenJDK | New fixtures: (1) `Cycles.java` — builds and drops cyclic graphs (linked ring + `Throwable`-shaped mutual graph via fields) in a deterministic loop, prints progress values; identical output on both JVMs proves cycle *semantics* (reachability) are unaffected; (2) `GcIdentity.java` — aliases one object through an array and a field, mutates via one path, reads via the other after printing checkpoints (collections may or may not have fired — output identical either way, which is the point); (3) `InternAcrossGc.java` — `ldc` identity asserted before/after churn-heavy code (`x == y` stays true). Plus the **same 39-fixture suite re-run in forced-GC mode** (`J2R_GC_EVERY_N=1`) as a CI gate: every fixture must produce byte-identical output with a collection at every allocation — the rooting proof |
| Regression tests | Unit tests in `heap.rs`: unreachable cycle collected / reachable cycle kept; array children marked; intern + boxed + `System.out` roots kept; statics-scan hook keeps a static-referenced object; free-list reuse does not resurrect (a stale handle's index is reused, and old handles held by *dead* frames can never be dereferenced because frames pop LIFO before sweep); worklist marking on a 1 M-node list does not overflow; `gc_point` re-entrancy debug assertion fires; threshold policy math. Census re-issue (§4.7). All 125 existing tests stay green |
| Measurable acceptance criteria | (a) All existing gates green + new gate-9 (forced-GC differential suite) green. (b) A native stress test loops 200 000 build-and-drop cycle allocations with 10 live objects and asserts `Heap::high_water() < 64` entries — the R1 leak is gone, numerically. (c) Differential suite wall-clock within ±20 % of the Wave 1 baseline, recorded in the evidence doc. (d) `rg "unsafe"` over the workspace returns nothing (or only the reviewed, documented exception if the Wave 2 frame scan required one — stated, not hidden). (e) native unit test: `Object.hashCode` returns the same value for a live object before and after forced collections (the differential side is covered by `InternAcrossGc.java`) |

---

## 5. Design C — Typed operand-stack specialization (Wave 3)

### 5.1 Approach

The verifier already computes exactly what is needed: for every pc, the
`VTy` of every stack slot and local (§1.2). Wave 3 *retains* that shape
instead of discarding it and emits specialized storage:

* For a method where every stack position's primitive kind is stable
  across all pcs where that position is live, emit typed slot arrays
  (`stack_i: [i32; k]`, `stack_l: [i64; m]`, `stack_f`, `stack_d`; refs
  stay in the existing `Vec<Value>` shape) and direct typed operations —
  `pop_int` becomes an indexed read, `Value::Int` boxing disappears from
  the hot path entirely.
* Where a position's kind varies by pc (the verifier's kind-stable-join
  rule allows different positions to change kind between program
  points), or anything else is unclear, **the method falls back to the
  current uniform emission**. The decision is per-method, made by a new
  `specialize.rs` pass whose inputs and invariants are unit-tested;
  correctness never depends on specialization succeeding.
* This is a codegen-only change: the runtime, ABI and heap are untouched,
  so it cannot regress GC or semantics beyond codegen bugs — which the
  unchanged 39-fixture differential suite catches.

### 5.2 Scope control and the go/no-go checkpoint

Sub-step 3a delivers the shape-extraction and representation
infrastructure plus the decision logic **and a written report** of how
many fixture methods specialize (expected: the arithmetic-heavy
majority). Sub-step 3b delivers the emission. If 3a's report shows the
coloring complexity is out of proportion (e.g. half the methods fall
back), the wave ends after 3a with the recommendation recorded and
unspecialized emission unchanged — M2 remains complete without it, since
§3 and §4 carry the semantic weight of the milestone.

### 5.3 Per-change specification

| Requirement | Detail |
| --- | --- |
| Files/modules affected | `crates/j2r-codegen/src/verify.rs` (export the per-pc `State` shapes behind a `pub struct StackShape`); new `crates/j2r-codegen/src/specialize.rs` (slot analysis + decision); `crates/j2r-codegen/src/emit.rs` (hybrid emission, per-method dispatch to specialized or uniform body) |
| Java semantic requirements | None new — this wave changes *representation only*. Every observable (overflow wrapping, NaN behaviour, int/long shifts, category-2 slot semantics) must be bit-identical, enforced by the unchanged differential suite |
| Memory-safety implications | Typed slot arrays are plain Rust values in the frame; the FrameGuard scan (GC roots) must scan whichever representation a method uses — the specialized frame exposes the same `scan` surface, so the GC invariant of §4.3 is preserved by construction. Unit test: a specialized method holding the only reference to an object across a forced collection keeps it alive |
| Performance tradeoffs | Removes `Value` boxing/tag checks from arithmetic, local traffic and dup families on specialized methods. Measured via a deterministic benchmark (see acceptance); target ≥ 1.3× on arithmetic-recursion fixtures (e.g. fib(28)-shaped), *with the explicit understanding that if the gain does not materialize the wave's value is still the shape infrastructure* (recorded honestly) |
| Differential tests vs OpenJDK | No new fixtures required — the 39-fixture suite (including `IntOverflow`, `FloatEdge`, `Shifts`, `LongOps`, `Casts`, `Arith`, `Arrays2D`, `JaggedNew`) is the test: byte-identical output with specialization on, which is the default path |
| Regression tests | Unit tests in `specialize.rs`: kind-stability detection (a method whose slot 0 is `Int` at one pc and `Ref` at another falls back); category-2 slots occupy exactly one typed slot (our `Value` model) and never alias the wrong pool; fallback methods compile and run identically; FrameGuard scans specialized frames (see above) |
| Measurable acceptance criteria | (a) All gates green; suite output byte-identical. (b) ≥ 60 % of fixture methods take the specialized path (recorded per fixture in the evidence doc). (c) Benchmark: repeated-run median ratio recorded; ≥ 1.3× on arithmetic fixtures or an honest no-gain finding with analysis. (d) A unit check asserts the generated Rust for a known arithmetic method contains no `Value::` construction on its hot path |

---

## 6. Design D — CFG reconstruction (Wave 4: investigation, not a rewrite)

### 6.1 Approach

The reviewer asked for *safe CFG reconstruction* to be investigated. The
audited architecture (§1.3) is approved and stays the default. Wave 4
produces:

1. **A written design report** (`docs/M2_CFG_REPORT.md`): condition-based
   lifting over the leader/branch structure (dominator-tree region
   formation → `if`/`while`/`loop` emission), with explicit handling of
   the hard cases the JVM allows: irreducible loops (fall back to the
   pc-dispatch body for that method), exception-handler edges as
   function-level try regions, switch lowering, and stack-shape
   consistency across reconstructed edges (reused from the verifier's
   per-pc shapes).
2. **A prototype** behind `--emit-cfg` (default **off**), landing only if
   the report's risk assessment supports it. The prototype's gate is
   output-equivalence: for every fixture, `--emit-cfg` and default
   emission produce byte-identical stdout/stderr/exit status.
3. **A go/no-go recommendation** for making it the default in a future
   milestone — which is a separate architectural review, not a decision
   taken inside M2.

### 6.2 Per-change specification

| Requirement | Detail |
| --- | --- |
| Files/modules affected | New `crates/j2r-codegen/src/cfg.rs` (prototype analysis + emission); `crates/j2r-cli/src/main.rs` (`--emit-cfg` flag, default off); `docs/M2_CFG_REPORT.md` |
| Java semantic requirements | None — emission strategy only. Irreducible control flow, `jsr`-free bytecode (already guaranteed), handler-entry shapes and switch semantics must all be either lifted exactly or delegated to the existing per-method fallback |
| Memory-safety implications | None at runtime; compile-time only. The prototype reuses the verifier, so unsound lifting is impossible without first failing verification-derived invariants (unit-tested) |
| Performance tradeoffs | Only for `--emit-cfg` users: removes the dispatch `match` per basic block. No effect on default emission |
| Differential tests vs OpenJDK | The full suite run twice: default and `--emit-cfg`; outputs must be identical — automated as a test, not a manual step |
| Regression tests | Unit tests: dominator/region construction on synthetic CFGs; irreducible-graph detection falls back; handler-edge lowering |
| Measurable acceptance criteria | (a) Report exists with the go/no-go recommendation and per-fixture lift statistics. (b) If the prototype lands: all gates green with the equivalence test passing; default emission byte-for-byte unchanged versus Wave 3 |

---

## 7. How the GC architecture supports real Java threads (evaluation required by the review)

No thread code is written in M2 (`Thread`, `synchronized`, `wait`/`notify`
remain loud compile errors). What M2 *must* do is shape the architecture
so M3's threading is an extension, not a rewrite:

* **Handles are the thread interface.** A `JRef` is a plain `u32` with no
  thread affinity, so a second thread can hold references to shared
  objects the same way the first one does — the opposite of the `Rc`
  world, where `!Send` forecloses sharing (risk R4).
* **Roots are per-thread by design.** The Wave 1 frame registry is a
  `thread_local` stack of frame records; with N threads it becomes N such
  stacks, and the GC scans all of them. Nothing in Wave 1/2 hard-codes
  "exactly one frame stack".
* **Collection points are safepoints.** §4.3's only-collection-in-
  generated-code rule is, in a threaded world, the standard
  safepoint-poll placement: a thread reaching a `gc_point` checks a
  global "GC requested" flag and cooperates. M2 implements exactly this
  poll shape (single-threaded no-op fast path), so M3 adds a stop signal,
  not new call sites.
* **Heap sharing.** M2 keeps the `Heap` in a `thread_local` (zero-cost
  today). The M3 migration — `Mutex<Heap>` or per-thread allocation
  buffers over a shared arena with epoch-based reclamation — changes the
  container around `heap::alloc`, not the handle/roots/safepoint design.
  The plan for that migration (including making the *statics pool* shared,
  since `static` fields are process-global in Java, unlike today's
  `thread_local` pool) is written into the §4 design notes so the review
  board can judge it, but it is explicitly not M2 work.
* **Monitors.** `Entry` is laid out so a monitor/hasher word can be added
  in M3 (synchronized over the GC heap; happens-before via
  acquire/release on monitor enter/exit). Reserving *layout thought*
  now costs nothing; reserving *code* now would be speculative sloppiness.

---

## 8. Ordered implementation plan

**Ordering rationale.** Wave 1 first because it is the smallest change
with the largest semantic payoff and because its frame registry is the
GC's root infrastructure — building GC first would require a temporary
rooting story that Wave 1 then replaces. Wave 2 second because it is the
critical, most invasive wave and must land while the tree is otherwise
semantically quiet (typed stacks would multiply the diff). Wave 3 third:
codegen-only, zero runtime risk, and it rides on the verifier. Wave 4
last: investigation, default-off.

Every wave is a separate reviewed-and-green commit; no wave begins
before the previous one's CI evidence exists. **Every existing M1/M1.5/
M1.6 test stays green at every wave** — that sentence is an acceptance
criterion of each wave, not a hope.

| Wave | Tasks (ROADMAP.md §4) | Delivers | Depends on | New CI coverage |
| --- | --- | --- | --- | --- |
| 1 | T07–T10 | Catchable `StackOverflowError`, frame registry | — | +2 fixtures (suite = 36) |
| 2 | T11–T18 | Arena tracing GC (mark-sweep, cycle-collecting) | Wave 1 frames (roots) | +3 fixtures (suite = 39), gate-9 forced-GC suite |
| 3 | T19–T20 | Typed stack specialization with per-method fallback | Wave 2 (FrameGuard scan of typed frames) | benchmark evidence, unit checks |
| 4 | T21–T22 | CFG report + optional default-off prototype; completion review package | Waves 1–3 stable | output-equivalence test |

Global invariants at every wave:

1. Zero `unsafe` (any quarantined exception must be explicitly reviewed
   and documented in RUNTIME_SAFETY.md, never discovered later).
2. `j2r-runtime`/`j2r-classfile` keep zero external dependencies.
3. The differential harness keeps comparing all three channels and never
   normalizes exception types, messages or `Caused by:` chains.
4. Unsupported features keep failing loudly at compile time.
5. Each wave updates RUNTIME_SAFETY.md, ROADMAP.md and a
   `docs/M2_EVIDENCE.md` section (bug-or-risk, reproduction, change,
   tests, JVM-vs-native results, CI run link) — the M1.5 evidence
   discipline, continued.
6. New CI gates are added only with the all-gates job extended, so a
   silently-skipped gate is impossible.

---

## 9. CI evolution

| Gate | Today | M2 change |
| --- | --- | --- |
| 1 fmt / 2 clippy / 3 test / 4 release / 5 standalone / 6 doc / 7 audit / 8 all-gates | green | unchanged, always required |
| 9 gc-stress | — | added in Wave 2: differential suite with `J2R_GC_EVERY_N=1` (forced collection at every allocation) |
| 10 no-unsafe | — | Wave 2: trivial grep gate (`rg -c "unsafe" crates/` must be empty), making invariant §8.1 mechanically enforced rather than asserted |

---

## 10. Explicit non-goals for M2

* Java threads, monitors, `wait`/`notify` (compile errors stay; §7 is
  architecture-shaping only).
* Configurable heap limit with catchable `OutOfMemoryError` (R2) — no
  deterministic differential test exists against HotSpot's OOME timing;
  deferred until a defensible test protocol exists.
* Moving/compacting collection, generational barriers, finalizers,
  references (`SoftReference` et al.), `Cleaner`.
* Changing the default emission strategy to CFG-based (§6 is
  investigation and an opt-in prototype).
* Any Minecraft, Mixin, JNI, JAR or classpath work (M3+ scope).

---

## 11. Review gate

This document is the deliverable the M1 review asked for. Per that
review's instruction — *"Propose an ordered implementation plan and wait
for architectural review before coding"* — implementation of Wave 1 does
not begin until this plan is approved. After approval, each wave ships
with its own evidence record and green CI, and the completed milestone
receives the same external review M1 received.
