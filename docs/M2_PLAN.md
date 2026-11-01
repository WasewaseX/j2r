# J2R — M2 Implementation Plan (generated code quality)

**Status: PLAN ONLY (revision 2.1) — nothing in this document has been
implemented.** Per the M1 architectural review, coding may not begin until
this plan itself passes architectural review. Revision 2 answers the
**REQUEST CHANGES** verdict on the original plan (review of 2026-11-01);
revision 2.1 (docs-only) adds the plan-time stack-footprint calibration
measurement demanded by blocker 3 (§3.1.1); Wave 1 remains blocked until
this revision is re-approved.

### 0. Response to the M2 plan review (REQUEST CHANGES, 2026-11-01)

| # | Blocker | Resolution | Where |
| --- | --- | --- | --- |
| 1 | GC root safety not sufficiently specified; scoped-access vs raw pointers unresolved | **SETTLED — zero raw pointers.** Owned root snapshots for suspended caller frames (refreshed at every emitted call boundary), direct scan of the current frame at each safepoint, `NativeRoots` scopes for native→Java re-entry. The raw-pointer alternative is withdrawn. | §4.4 |
| 2 | Recycled handles could alias a different object | **Generation-tagged handles adopted:** `JRef { idx, gen }`; sweep bumps `gen`; stale dereference is a loud VM-internal panic in all builds — silent aliasing impossible by construction | §4.1, §4.5, §4.6 |
| 3 | `J2R_MAX_JAVA_FRAMES` needs a hard safety limit | **Hard ceiling derived from a measured per-frame native footprint** — measured at plan time: worst shape 304 B/frame, const 512 B, ceiling `131 072` (§3.1.1); override accepted only in `1..=MAX_FRAMES_ABS`; `0`, `>ceiling`, non-numeric rejected with exit 2; boundary-value test matrix (default/min/max/max+1/0/invalid) | §3.1, §3.1.1, §3.2 |
| 4 | Threading architecture preliminary; Copy handles ≠ thread safety | **Boundary stated:** M2 delivers no concurrency and claims none; the M3 migration checklist (shared heap, shared statics, stop-the-world coordination, per-thread root registration, monitor word, shared identity caches) is enumerated for future review | §7 |
| 5 | Typed locals vs operand stacks ambiguous | **Scope settled:** specialization covers operand-stack positions *and* local-variable slots for primitives; references never leave `Vec<Value>` storage (GC scan surface unchanged); per-method all-or-nothing decision; explicit reduced-scope fallback at the 3a checkpoint | §5.1–§5.3 |
| 6 | README contradictions (`multianewarray`, fixture count) | **FIXED** in README.md: `multianewarray` removed from the unsupported list (it was implemented in M1.6), differential suite count corrected to 34 | README.md |

Approved baseline this plan builds on: commit `c07a10f` (M1.6), CI run
`41201763892` SUCCESS on all 8 gates (2026-10-31): 110 unit tests, 12
malformed-classfile e2e tests, 1 wide-ids e2e test, 34-fixture differential
suite against Temurin 21 (three-channel compare: stdout, stderr, exit
status), zero `unsafe` in the workspace, `j2r-runtime` and `j2r-classfile`
have zero external dependencies.

The fine-grained working todolist for all M2 work — 23 tasks (T01–T23)
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
  The limit defaults to 8192 and is overridable via `J2R_MAX_JAVA_FRAMES`,
  but only within a **hard safety ceiling derived from a measured native-
  stack budget** (review blocker 3): `MAX_FRAMES_ABS = WORKER_STACK_BYTES
  / (FRAME_FOOTPRINT × 8)`, where `FRAME_FOOTPRINT` is the per-Java-frame
  native-stack usage **measured by a calibration test** — a recursive
  chain on a fixed-size thread, footprint = (stack-address delta across
  the chain) / depth. The measurement has already been performed at plan
  time (§3.1.1): worst measured shape 304 B/frame, `FRAME_FOOTPRINT` const
  set to **512 B** (1.68× headroom), giving `MAX_FRAMES_ABS = 512 MiB /
  (512 × 8) = 131 072` frames; the Wave-1 CI calibration test re-measures
  and fails the build if the live footprint exceeds 1.25× the const
  (640 B). Accepted override range is
  `1..=MAX_FRAMES_ABS`. `0`, values above the ceiling, and non-numeric
  strings are rejected **before `main` runs**: the process prints
  `j2r: invalid J2R_MAX_JAVA_FRAMES=<v>` and exits 2 — an operator error,
  never a Java-observable result (the env var does not exist in HotSpot,
  so rejections are runtime-level e2e tests, not differential fixtures).
  A compile-time `const` assertion pins `DEFAULT_FRAMES ≤ MAX_FRAMES_ABS`,
  so a future footprint regression fails the build instead of silently
  eroding the safety margin.
* Native→user re-entry (`toString` during concatenation, future
  comparators) passes through `invoke_virtual` into a generated method,
  so it is counted by the same mechanism with no extra instrumentation.

Why this is safe against real native-stack exhaustion: the 512 MB worker
stack gives a budget of 64 KB of native stack per Java frame at the
8192 limit. A generated frame's native footprint is small (three `Vec`s
are heap-allocated; the dispatch `match` does not recurse): **measured
at 208–304 B/frame** on real M1.6-generated code (§3.1.1), natives and
native→generated re-entry included in the 304 B shape. The default limit
therefore runs with a 216× native-stack margin, mirrors the order of
magnitude of HotSpot's default (~10–20 k frames on a 1 MB thread stack),
and sits 16× below the measured-derived ceiling. The hard ceiling above
turns this argument from an assumption into a measured, const-checked
invariant.

#### 3.1.1 Plan-time calibration measurement (2026-11-01, evidence for blocker 3)

Method (exactly the §3.1 formula, executed against the current M1.6
compiler — no repository code was modified; the harness instruments only
throwaway generated crates under /tmp): a probe records the address of a
stack local at every generated method entry; the fixture recurses to a
known depth D on the standard 512 MB worker thread; per-frame footprint =
(outermost probe address − innermost probe address) / D. Toolchain:
rustc 1.99.1 release, Temurin 21.0.13 for `javac`.

| Frame shape | Depth | Footprint |
| --- | --- | --- |
| A — pure arithmetic recursion (`down(n-1)+n`) | 100 000 | **208 B** |
| B — one native dispatch per level (`"" + n`, `String.length`) | 50 000 | **304 B** |
| C — fat frame: 8 locals + operand temporaries + try/catch + native dispatch | 50 000 | **304 B** |

All three chains completed normally (the deepest used 20.8 MB of
512 MB — under 4 %). Interpretation: the per-level cost is dominated by
the native-dispatch path (B == C), not by local count; the worst shape
measured is 304 B/frame. Constants derived for Wave 1 (all const-checked
at compile time, re-measured in CI):

* `FRAME_FOOTPRINT = 512` (worst measured 304 B, 1.68× headroom; CI
  assertion measured ≤ 640 B = 1.25× const)
* `MAX_FRAMES_ABS = 536 870 912 / (512 × 8) = 131 072` (= 2¹⁷)
* `DEFAULT_FRAMES = 8192` — 16× below the ceiling, 216× native-stack
  margin at the default limit

Applicability note: Wave 1 adds a `FrameGuard` carrying a depth counter
(≤ 32 B of extra stack), Wave 2 adds the owned snapshot-slot handle
(≤ 16 B) — both well inside the const's headroom, and the CI calibration
test re-measures the real shipped shape at every merge anyway.

### 3.2 Per-change specification

| Requirement | Detail |
| --- | --- |
| Files/modules affected | New `crates/j2r-runtime/src/frame.rs` (registry, depth counter, `StackOverflowError` constructor, hard-ceiling constant + limit validation, footprint calibration); `crates/j2r-runtime/src/lib.rs` (module + `builtin.rs` already maps `StackOverflowError → VirtualMachineError`, no change); `crates/j2r-codegen/src/emit.rs` (`emit_method`: emit guard, depth check, early `return Err`); `crates/j2r-cli/tests/diff_suite.rs` (fixture-count guard 34 → 36) |
| Java semantic requirements | JLS/JVMS: `StackOverflowError` is a `VirtualMachineError`, thrown asynchronously-ish on thread-stack exhaustion, **catchable**; after a caught SOE the thread continues normally; uncaught SOE prints `Exception in thread "main" java.lang.StackOverflowError` (no message) and exits 1. Depth at which it fires is *not specified* (depends on JVM stack size) — differential fixtures must therefore never assert a depth. |
| Memory-safety implications | In Wave 1 the registry needs **no pointers at all**: the record is a depth counter (push/increment, pop/decrement via `Drop`), so there is nothing to dereference and nothing to leak — Wave 1 introduces no new memory-safety surface beyond a `thread_local usize`. Scanning frame *contents* is a Wave 2 requirement (§4.4); its mechanism is now **settled** — owned root snapshots for suspended frames plus direct scan of the current frame (§4.4) — and contains **no raw pointers of any kind**; the previously open "quarantined raw pointers" alternative is withdrawn |
| Performance tradeoffs | One `thread_local` push/pop and one counter check per call. Expected effect on the differential suite: within noise (< 5 %). Measured and recorded in the evidence doc; if a regression shows up, the fallback is checking depth only every K frames (K=8) with the guarantee that the native-stack budget still holds at K granularity. |
| Differential tests vs OpenJDK | New fixtures: (1) `StackDepth.java` — unbounded recursion, `catch (StackOverflowError e)` prints `caught-soe` and returns normally; (2) `UncaughtSOE.java` — unbounded recursion with no handler; expected stderr is the uncaught-SOE line (frame lines normalized by the existing harness), exit status 1. Both run against Temurin 21 through the unchanged three-channel harness. **Override-boundary matrix** (runtime-level e2e, not differential — the env var does not exist in HotSpot): default (8192 accepted), minimum (`1` accepted; any recursion deeper than one frame SOEs), maximum (`MAX_FRAMES_ABS` accepted; a chain of exactly `MAX_FRAMES_ABS − 1` frames completes), `MAX_FRAMES_ABS + 1` rejected, `0` rejected, `abc` rejected — acceptances run the program to normal completion; rejections assert the `j2r: invalid J2R_MAX_JAVA_FRAMES=…` message and exit 2. |
| Regression tests | Unit tests in `frame.rs`: depth accounting across normal return / throw / panic; limit validation (accepted at default/min/max, rejected at `0`/`>MAX`/non-numeric); SOE object shape (class, no message, subtype of `VirtualMachineError`); native re-entry counted (concat → `toString` recursion builds frames); calibration test re-run in CI asserting the measured footprint stays within 1.25× of the hard-coded const (footprint drift fails the build, never the margin). Existing `Recursion.java` fixture must stay green unchanged. |
| Measurable acceptance criteria | (a) All 8 CI gates green with the suite grown to 36 fixtures. (b) A dedicated e2e test compiles and runs a recursive program with a deliberately tiny `J2R_MAX_JAVA_FRAMES=64` and asserts the process exits 0 with `caught-soe` on stdout — i.e. the process **never aborts** (no SIGSEGV/101) at any limit. (c) Frame accounting unit test: after `main` returns, registry depth is 0. (d) The six-case boundary matrix (default/min/max/max+1/0/invalid) is green: no accepted value ever aborts the process, every rejected value exits 2 with the diagnostic. (e) Calibration CI check: measured `FRAME_FOOTPRINT` ≤ 1.25× the const used in the ceiling formula (≤ 640 B against the 512 B const; plan-time measured value 304 B, §3.1.1). |

---

## 4. Design B — Tracing garbage collector (Wave 2)

### 4.1 Chosen architecture

A **precise, non-moving, mark-and-sweep arena GC** with index handles:

```rust
// crates/j2r-runtime/src/heap.rs (new)
pub struct JRef { idx: NonZeroU32, gen: u32 }  // Copy; identity == (idx, gen)
struct Entry { obj: RefCell<JObj>, mark: Cell<bool>, gen: u32 }
pub struct Heap { entries: Vec<Entry>, free: Vec<u32>, live: usize, ... }
```

* **Generation-tagged handles — stale references cannot alias (review
  blocker 2).** When sweep frees an entry it increments that entry's
  `gen`; allocation stamps the new handle with the entry's current `gen`.
  Every heap access (`with_obj`, `obj_get`, `obj_set`, array element
  access, every native helper that touches object state) first checks
  `entry.gen == handle.gen`; a mismatch means the holder roots nothing —
  a rooting bug — and the runtime **panics with a diagnostic
  (`j2r: stale JRef (idx=…, gen=…) — GC rooting bug`) in all builds**.
  Silent aliasing onto a recycled object is therefore impossible by
  construction, not just by testing. Generation wrap-around is argued
  unreachable for *rooted* handles: a rooted handle keeps its entry out
  of the free list, so its `gen` never advances; an unrooted handle is a
  bug the `gen` check turns loud long before 2^32 reuse cycles could
  matter. Cost: one integer compare per object access (not per handle
  copy — handles are `Copy` and circulate freely).

* **Handles, not pointers.** `JRef` becomes `Copy`; `Option<JRef>` is
  8 bytes (`idx` + `gen` — the price of stale-handle detection); `Value`
  drops its refcount traffic (R3). `JRef::ptr_eq` becomes `(idx, gen)`
  pair equality. `MethodFn` keeps its shape with `Option<JRef>` passed
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
   the current frame's scanned storage, in a suspended caller's root
   snapshot, in the statics pool, or in a runtime root** —
   there is nowhere else (verified below).
3. **The window between a caller popping args and the callee registering
   its frame contains no collection point**: `pop_n`/`unmarshal_args`
   allocate no Java objects, and `gc_point()` only precedes allocation
   opcodes inside a method body whose frame is already registered.

### 4.4 Root set (settled — owned root snapshots, zero raw pointers)

**Decision (review blocker 1).** The choice left open in revision 1 —
scoped-lifetime access vs quarantined raw pointers — is resolved: **the
root mechanism contains no pointers and no `unsafe` at all.** The GC
obtains roots through exactly three channels:

1. **The current frame — scanned directly.** `gc_point()` is emitted only
   inside a method body (before the 4 allocating opcodes, §4.3), where
   `locals`, `stack` and `thrown` are ordinary in-scope bindings. The
   emitted call is
   `heap::gc_point(GcFrame::current(&locals, &stack, &thrown))`: the heap
   module receives shared references to the live frame's storage and
   reads ref handles out of them. No registry lookup, no pointer, nothing
   that can dangle.
2. **Suspended caller frames — owned root snapshots.** Each `FrameGuard`
   owns one slot in a `thread_local` LIFO registry
   (`RootStack { slots: Vec<Vec<u32>> }`; each slot is an owned
   `Vec<u32>` of ref handles, capacity reused across calls, so the
   steady state is allocation-free). At **every generated call boundary**
   the emitted code refreshes the frame's slot from its current
   `locals`/`stack`/`thrown` **before** popping arguments, then transfers
   control. A suspended caller's Rust state cannot change while it is
   suspended (execution is sequential), so its snapshot is exact for the
   entire suspension. Over-approximation — the snapshot lists a handle
   that has died by the time some later collection runs — is safe by
   definition: it only delays reclamation, which Java does not allow a
   program to observe.
3. **Statics pool and runtime roots** — `statics.rs::gc_scan_roots`
   (registered by the emitted `main_run` at startup) and the intern
   cache, boxed-identity cache and `System.out` handle, scanned by the
   heap module directly.

**Native→Java re-entry.** A native dispatching into generated code
(concat → `toString`, future comparators) is a call boundary like any
other: the dispatching native must wrap its live refs in a `NativeRoots`
RAII scope — an owned `Vec<u32>` pushed onto the same `RootStack`
registry, popped by `Drop`. The Wave-2 census (§4.7) enumerates **every**
native→generated dispatch site and proves each one scopes its refs.

**In-flight arguments.** The caller's snapshot is refreshed before args
are popped, so argument handles stay rooted from the pop through the
callee's `FrameGuard` registration — a window in which no collection
point exists anyway (§4.3.3), and one where the snapshot still names
them even if one existed.

**Reviewer-required failure scenarios, and how each is caught:**

* (a) *A call boundary emitted without a snapshot refresh* (a codegen
  bug) → under-rooting → with `J2R_GC_EVERY_N=1` a collection runs at
  **every allocation**, so the missed root dies within one allocation and
  the differential suite fails loudly; a codegen unit test additionally
  asserts that every emitted `invoke*` is immediately preceded by a
  snapshot refresh.
* (b) *A frame that never registers its slot* → same forced-GC proof.
* (c) *A native dispatching into user code without a `NativeRoots`
  scope* → census (§4.7) + forced-GC proof.
* (d) *LIFO violation under panic unwind* → guards drop in
  reverse-construction order; a debug assertion checks
  `pop depth == push depth` on every frame exit.
* (e) *A stale handle that survives an incomplete root scan* → the
  generation check (§4.1) converts what was silent corruption in
  revision 1 into a loud VM-internal panic with the culprit `(idx, gen)`;
  the stale-handle unit test (§4.8) pins this.

### 4.5 What sweep does

An unmarked entry is removed, its generation counter is incremented
(invalidating every outstanding handle to it — §4.1), its index goes to
the free list, and
dropping its `JObj` releases the `jclass` string and the slots vector
(which recursively drops `Value`s — pure field drops, no recursive
object destruction, since handles are plain integers). Free-list reuse
gives handle reuse after death — permitted by Java identity-hash rules
(hash stability is required only for live objects; §4.6).

### 4.6 Identity and hashing

* `==` (`if_acmpeq/ne`, `Object.equals`): handle equality — `(idx, gen)`
  pair equality; for a live object `gen` is constant (a rooted entry
  never re-enters the free list), so equality semantics are exactly the
  old reference equality.
* `Object.hashCode` / `System.identityHashCode`: **the index component
  `idx as i32`** (natives.rs 107–113 changes from `Rc::as_ptr`). Distinct
  live objects have distinct indices by construction, and the value is
  stable for a live object regardless of collection; `gen` is
  deliberately excluded because hash stability is required only for
  *live* objects, and a live object never shares its index. (HotSpot's
  identity hash is likewise unspecified numerically; differential tests
  may only assert self-consistency, which the fixture does — §4.8.)
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
| Files/modules affected | New `crates/j2r-runtime/src/heap.rs` (heap, mark/sweep, generation tags, roots, thresholds, `JRef`, `GcFrame`); new `frame.rs` additions: `RootStack` registry + `NativeRoots` scopes; rewrite of `crates/j2r-runtime/src/{lib.rs, object.rs, array.rs, natives.rs, concat.rs}` (handle mechanics, gen-checked closure accessors); `crates/j2r-codegen/src/emit.rs` (`gc_point(GcFrame::current(...))` emission before the 4 allocating opcodes **and** root-snapshot refresh at every emitted call boundary; `statics.rs` template gains `gc_scan_roots`; `main.rs` template registers it); `crates/j2r-cli/tests/diff_suite.rs` (guard bump 36 → 39); `.github/workflows/ci.yml` (new gate-9, §9); docs RUNTIME_SAFETY.md §1/§2/§4 rewritten for the new ownership model |
| Java semantic requirements | JLS: object reachability is the only observable; GC timing is not. Must preserve: reference identity and mutation visibility through any alias at any time (including across collections); interned-literal identity (JLS 3.10.5 / `String.intern`); JLS 5.1.7 boxed-cache identity; `Throwable` cause/suppressed graphs (cycles are ordinary objects — currently unimplementable via the two-arg constructor, but the object model must not preclude M3); array element aliasing; `StringBuilder`/`String` COW sharing |
| Memory-safety implications | All safe Rust: `Vec<Entry>` (stable addresses, non-moving), generation-tagged handles (`idx` + `gen`), `Cell<bool>` mark bits, explicit worklist. The rooting discipline is the safety kernel and is now **settled and pointer-free** (§4.4): direct current-frame scan, owned per-frame snapshot slots, `NativeRoots` scopes — zero raw pointers, zero `unsafe` (mechanically enforced by gate-10). A stale handle can never silently alias: the `gen` check in every heap accessor panics with a diagnostic in all builds (§4.1) |
| Performance tradeoffs | Wins: no refcount traffic on any `Value` clone (operand stack, locals, args); `Option<JRef>` becomes `Copy` (8 B with generation tag); allocation is a free-list pop instead of `malloc`. Costs: one `gen` compare per object access; mark pauses proportional to live objects; non-moving arena retains free-list fragmentation; ~1 word header per object; root-snapshot refresh (copy the frame's ref handles into the owned slot) at every call boundary — linear in the frame's live ref count, expected single-digit ns on typical frames. Expected net: neutral to faster on the fixture suite; the evidence doc records wall-clock before/after with a ±20 % acceptance band (§8) |
| Differential tests vs OpenJDK | New fixtures: (1) `Cycles.java` — builds and drops cyclic graphs (linked ring + `Throwable`-shaped mutual graph via fields) in a deterministic loop, prints progress values; identical output on both JVMs proves cycle *semantics* (reachability) are unaffected; (2) `GcIdentity.java` — aliases one object through an array and a field, mutates via one path, reads via the other after printing checkpoints (collections may or may not have fired — output identical either way, which is the point); (3) `InternAcrossGc.java` — `ldc` identity asserted before/after churn-heavy code (`x == y` stays true). Plus the **same 39-fixture suite re-run in forced-GC mode** (`J2R_GC_EVERY_N=1`) as a CI gate: every fixture must produce byte-identical output with a collection at every allocation — the rooting proof |
| Regression tests | Unit tests in `heap.rs`: unreachable cycle collected / reachable cycle kept; array children marked; intern + boxed + `System.out` roots kept; statics-scan hook keeps a static-referenced object; **stale handles cannot alias**: an object dies, its index is reused by a new object, the old handle's `(idx, gen)` mismatches — dereferencing it panics with the stale-handle diagnostic (pinned via `catch_unwind`), and the new object's fields are untouched; a *rooted* object's handle survives any number of forced collections with `gen` unchanged; a suspended-caller snapshot keeps a handle alive across a collection fired inside the callee; worklist marking on a 1 M-node list does not overflow; `gc_point` re-entrancy debug assertion fires; threshold policy math. Census re-issue (§4.7) including the `NativeRoots` dispatch-site inventory. All 125 existing tests stay green |
| Measurable acceptance criteria | (a) All existing gates green + new gate-9 (forced-GC differential suite) green. (b) A native stress test loops 200 000 build-and-drop cycle allocations with 10 live objects and asserts `Heap::high_water() < 64` entries — the R1 leak is gone, numerically. (c) Differential suite wall-clock within ±20 % of the Wave 1 baseline, recorded in the evidence doc. (d) `rg "unsafe"` over the workspace returns nothing — there is no documented exception: the root mechanism is pointer-free (§4.4). (e) Native unit test: `Object.hashCode` returns the same value for a live object before and after forced collections (the differential side is covered by `InternAcrossGc.java`). (f) Stale-handle unit test green: reuse with bumped `gen` panics on dereference; new object intact. (g) Codegen unit test: every emitted `invoke*` site is immediately preceded by a root-snapshot refresh. (h) `rg "as_ptr|from_raw"` over `crates/` returns nothing (the last `Rc::as_ptr`, in `Object.hashCode`, is removed by §4.6). |

---

## 5. Design C — Typed operand-stack specialization (Wave 3)

### 5.1 Approach

The verifier already computes exactly what is needed: for every pc, the
`VTy` of every stack slot and local (§1.2). Wave 3 *retains* that shape
instead of discarding it and emits specialized storage:

* **Scope (review blocker 5): specialization covers operand-stack
  positions *and* local-variable slots.** For a method where every stack
  position's and every live local slot's primitive kind is stable across
  all pcs where it is live, emit typed primitive storage
  (`p_i: [i32; k]`, `p_l: [i64; m]`, `p_f`, `p_d` — stack positions and
  locals share the per-kind pools, indexed by a mapping precomputed by
  `specialize.rs`) and direct typed operations — `pop_int` becomes an
  indexed read, `Value::Int` boxing disappears from the hot path
  entirely. **References never leave `Vec<Value>` storage** (ref stack
  positions, ref locals and `thrown` stay exactly as they are), so the
  Wave-2 GC scan surface `GcFrame::current(&locals, &stack, &thrown)`
  is unchanged by construction.
* Where any stack position's or local slot's kind varies by pc (the
  verifier's kind-stable-join rule allows different slots to change kind
  between program points), or anything else is unclear, **the whole
  method falls back to the current uniform emission** — the decision is
  per-method all-or-nothing, never a partial mix, so there is exactly one
  frame shape per method and one scan surface. The decision is made by a
  new `specialize.rs` pass whose inputs and invariants are unit-tested;
  correctness never depends on specialization succeeding.
* This is a codegen-only change: the runtime, ABI and heap are untouched,
  so it cannot regress GC or semantics beyond codegen bugs — which the
  unchanged 39-fixture differential suite catches.

### 5.2 Scope control and the go/no-go checkpoint

Sub-step 3a delivers the shape-extraction and representation
infrastructure plus the decision logic **and a written report** of how
many fixture methods specialize (expected: the arithmetic-heavy
majority). The report tabulates, per method: stack positions
specialized, local slots specialized, and the fallback reason code —
so the delivered scope (stacks *and* locals, §5.1) is always visible
and never overstated. Sub-step 3b delivers the emission. If 3a's report
shows the combined complexity is out of proportion (e.g. half the
methods fall back), the documented **reduced scope** is operand-stack-
only specialization with locals left uniform — decided and recorded at
the 3a checkpoint, never silently. The wave may also end after 3a with
the recommendation recorded and unspecialized emission unchanged — M2
remains complete without it, since §3 and §4 carry the semantic weight
of the milestone.

### 5.3 Per-change specification

| Requirement | Detail |
| --- | --- |
| Files/modules affected | `crates/j2r-codegen/src/verify.rs` (export the per-pc `State` shapes — stack *and* locals — behind a `pub struct StackShape`); new `crates/j2r-codegen/src/specialize.rs` (stack+locals slot analysis, pool mapping, per-method decision); `crates/j2r-codegen/src/emit.rs` (hybrid emission, per-method dispatch to specialized or uniform body) |
| Java semantic requirements | None new — this wave changes *representation only*. Every observable (overflow wrapping, NaN behaviour, int/long shifts, category-2 slot semantics) must be bit-identical, enforced by the unchanged differential suite |
| Memory-safety implications | Typed primitive pools are plain Rust values in the frame; **references never leave `Vec<Value>` storage**, so `GcFrame::current(&locals, &stack, &thrown)` scans identical shapes with or without specialization — the GC invariant of §4.4 is preserved by construction, not by a second scan implementation. Unit tests: a specialized method holding the only reference to an object across a forced collection keeps it alive; a specialized method's local slot round-trips values across a loop back-edge |
| Performance tradeoffs | Removes `Value` boxing/tag checks from arithmetic, local traffic and dup families on specialized methods. Measured via a deterministic benchmark (see acceptance); target ≥ 1.3× on arithmetic-recursion fixtures (e.g. fib(28)-shaped), *with the explicit understanding that if the gain does not materialize the wave's value is still the shape infrastructure* (recorded honestly) |
| Differential tests vs OpenJDK | No new fixtures required — the 39-fixture suite (including `IntOverflow`, `FloatEdge`, `Shifts`, `LongOps`, `Casts`, `Arith`, `Arrays2D`, `JaggedNew`) is the test: byte-identical output with specialization on, which is the default path |
| Regression tests | Unit tests in `specialize.rs`: kind-stability detection for stack positions *and* local slots (a method whose stack position or local slot is `Int` at one pc and `Ref` at another falls back); category-2 values occupy exactly one pool entry (our `Value` model) and never alias the wrong pool; local→pool index mapping matches the verifier's slot numbering; fallback methods compile and run identically; GcFrame scans specialized frames (see above) |
| Measurable acceptance criteria | (a) All gates green; suite output byte-identical. (b) ≥ 60 % of fixture methods take the specialized path (recorded per fixture in the evidence doc, with per-method stack-position and local-slot counts). (c) Benchmark: repeated-run median ratio recorded; ≥ 1.3× on arithmetic fixtures or an honest no-gain finding with analysis. (d) A unit check asserts the generated Rust for a known arithmetic method contains no `Value::` construction on its hot path. (e) The 3a report states the delivered scope explicitly: "stacks + locals" or the documented reduced scope |

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
remain loud compile errors). **Boundary statement (review blocker 4):
M2 delivers no concurrency and claims none.** A `Copy` integer handle is
necessary for a shared heap but nowhere near sufficient: it does not make
the *heap* thread-safe. What M2 does is (a) keep the single-threaded heap
correct and (b) leave architecture-shaped hooks so M3's migration is an
extension rather than a rewrite. The M3 checklist below is a **plan for
future review, not delivered capability**:

* **Handles are the thread interface.** A `JRef` is a plain 8-byte
  `(idx, gen)` value with no thread affinity, so a second thread can hold
  references to shared objects the same way the first one does — the
  opposite of the `Rc`
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
* **Shared statics (M3).** Java `static` fields are process-global; M2's
  generated statics pool is `thread_local` — correct only because M2 is
  single-threaded, an acknowledged deviation that becomes a defect the
  moment a second thread exists. M3 moves the pool behind the shared heap
  container; the `gc_scan_roots` hook (§4.4) is the seam it plugs into.
* **Stop-the-world coordination (M3).** `gc_point` becomes a safepoint
  poll: a global "GC requested" flag; arriving threads park; the collector
  scans **all registered threads'** `RootStack` registries plus the shared
  roots, then releases. M2's single-threaded poll shape (§4.3) is exactly
  this minus the parking.
* **Shared identity structures (M3).** The intern cache and the boxed-
  value identity cache must become process-global with internal locking,
  because JLS 5.1.7 boxed identity and `String.intern` identity must hold
  **across** threads. Per-thread caches would be a semantic bug; M2's
  thread-local versions are correct only single-threaded.
* **Per-thread root registration (M3).** Each Java thread registers its
  `RootStack` in a runtime-side thread registry at birth and unregisters
  at death; the collector iterates it. Nothing in Wave 1/2 hard-codes
  "exactly one registry" (§4.4).

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

**Revision history.** Revision 1 (commit `2dcd031`): original plan,
verdict REQUEST CHANGES with six blockers. Revision 2 (commit `942648e`):
blockers 1–6 resolved — settled pointer-free root protocol (§4.4),
generation-tagged handles (§4.1/§4.5/§4.6), SOE hard ceiling + boundary
tests (§3), threading boundary + M3 checklist (§7), stacks-and-locals
specialization scope (§5), README contradictions fixed. Revision 2.1
(docs-only): plan-time calibration measurement recorded (§3.1.1) —
per-frame native footprint measured 208–304 B across three frame shapes
on M1.6-generated code; `FRAME_FOOTPRINT` const 512 B, `MAX_FRAMES_ABS`
131 072 — turning blocker 3's "measured" requirement from a promise
into recorded numbers. No design decision changed. Wave 1 remains
blocked until this revision passes architectural re-review; M1/M1.5/
M1.6 code and tests are untouched and stay green.
