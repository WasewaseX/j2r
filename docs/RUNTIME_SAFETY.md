# Runtime safety — ownership, cycles, and panic paths

This document states precisely what the runtime (`j2r-runtime`) guarantees,
what it deliberately does not, and how every panic path is prevented from
being reachable. It is the reference for the claims that may be made about
the current **single-threaded** milestone, and it will be revised — not
silently widened — when threads land. **M2 Wave 2 revision (arena tracing
GC):** the `Rc` heap was replaced by a generation-tagged handle arena
(`docs/M2_PLAN.md` §4); the census in §4 below was re-issued for the new
access model.

## 1. The ownership model

Every Java object lives in exactly one shape:

```text
JRef = { idx: NonZeroU32, gen: u32 }      // Copy handle into the arena
JObj { jclass: Rc<str>,                   // runtime class / array descriptor
       slots:  Vec<Value> }               // flattened field slots
heap:  thread_local Vec<Option<Entry>>    // non-moving arena, mark-sweep GC
```

* **Handles, not pointers.** A `JRef` is an 8-byte `(idx, gen)` pair —
  `Copy`, so `Value` drops its refcount traffic. Java identity (`==`) is
  `(idx, gen)` pair equality; for a live object `gen` never advances.
* **Stale handles cannot alias.** Sweep bumps a freed slot's generation;
  every object access (`heap::with_obj` / `with_obj_mut` and every helper
  built on them) checks the generation first and **panics loudly on
  mismatch in all builds** (`j2r: stale JRef (idx=.., gen=..) — GC rooting
  bug`). A recycled slot can never silently serve an old handle
  (M2_PLAN §4.1).
* **Interior mutability** (`RefCell` per object) models Java's mutable
  fields; every access is dynamically checked, so there is no `unsafe`
  (mechanically enforced by `#![forbid(unsafe_code)]` in every crate root),
  no aliasing UB, and a mis-nested borrow is a controlled panic — never
  memory corruption.
* **Collection** runs only at explicit collection points emitted before the
  four allocating opcodes; roots come from frame snapshots, the statics
  pool and the runtime caches (M2_PLAN §4.3/§4.4). Marking is an iterative
  worklist — never recursion — and marks are `Cell<bool>` beside, not
  inside, the object's `RefCell`.
* **`Value::Str(Rc<String>)`** is the character payload of `String` /
  `StringBuilder` objects. Mutations go through `Rc::make_mut`
  (copy-on-write), so a `String` aliasing a `StringBuilder` buffer is never
  mutated in place — `sb.toString()` then `sb.append(...)` behaves exactly
  like the JDK. Payloads are not in the object graph; a dying String
  object's payload dies with its slot vector, and any `StringBuilder`
  alias keeps it alive by refcount — pinned by the `Cycles`/
  `InternAcrossGc` fixtures and the forced-GC suite (gate-9).
* Interning (`object::intern_string`) maps equal literals to one object,
  matching constant-pool identity; the cache is `thread_local` and is
  scanned as a permanent root set.

## 2. What is safe to claim today (single-threaded milestone)

1. **Memory safety.** No `unsafe` anywhere in generated code or in
   `j2r-runtime`; all interior mutability is `RefCell`-checked. There is no
   path to use-after-free, dangling references, or data races — the latter
   because there is exactly one Java thread.
2. **Reference identity.** `==` / `!=` (`if_acmpeq`), string interning and
   identity hashing behave like the JVM.
3. **Determinism.** With one thread and no external state, a compiled
   program is deterministic for a given input.
4. **Cycles are reclaimed.** (Revised in Wave 2: this was the `Rc` heap's
   documented leak.) The tracing mark-sweep collector reclaims unreachable
   cyclic graphs; the 200 000-cycle stress test asserts the arena stays
   under 64 live slots with 10 live objects, and the `Cycles` differential
   fixture pins the observable semantics (identical output on HotSpot and
   J2R, which is all Java lets a program observe). A configurable heap
   limit with catchable `OutOfMemoryError` remains out of scope (M2_PLAN
   §10).
5. **Single-threaded execution is an architectural invariant, not an
   accident.** The statics pool, init flags, intern cache, boxed-value
   cache (the JLS-mandated `-128..127` identity objects) and `System.out`
   handle are all `thread_local`; the generated `main` runs the whole
   program on one spawned worker thread (512 MB stack for deep recursion).
   Java-level `Thread`/`synchronized` are *compile errors*, so no generated
   program can rely on concurrency that does not exist.

## 3. Panic paths and what guards them

Malformed input cannot reach the runtime's panic paths: every method body
passes the compile-time verifier (`j2r-codegen/src/verify.rs`) before any
Rust is emitted. The verifier proves, per method: operand-stack underflow
cannot happen, category-2 values are never split or mis-typed, local
indices are in range and initialized, branch targets are instruction
boundaries, joins agree on stack shape and primitive kinds, array element
kinds match the operation (`lastore` on an `int` is a *compile error*), and
returns match the descriptor.

| Runtime path | Would panic on | Guard | Classification |
| --- | --- | --- | --- |
| `stack::stack_pop`, `pop_n`, `pop2`, `dup*`, `swap` (`expect`/`assert`) | empty/mis-shaped operand stack | verifier stack simulation | internal invariant (codegen bug) |
| `Value::expect_int/long/float/double/ref` | wrong kind on stack/local | verifier type lattice (`VTy`) | internal invariant |
| `object::obj_get/obj_set` slot indexing | out-of-range field slot | compile-time flattened layout (`compute_layouts`) | internal invariant |
| `natives::this()`, `arg()` | missing receiver/argument | verifier pops exactly the descriptor's operands | internal invariant |
| `object::string_value`, `sb_text` | non-String payload in slot 0 | `is_string` checks before every call; layout owned by codegen | internal invariant |
| `array::check_array` | non-array object in array op | verifier (`pop_array`) — a *runtime* residual surfaces as `java/lang/Error` (a `Throw`), never a Rust panic | defensive |
| `javafmt` `expect("LowerExp always has 'e'")` | Rust std formatting without exponent | true by construction of Rust's `LowerExp` | internal invariant |
| Generated `pc out of range` (`match pc`) | branch outside the body | verifier boundary check on every branch/switch target | internal invariant |

The common classification, "internal invariant", means: reachable only by a
code-generator bug, never by any bytecode — valid or malformed — that the
compiler accepts. Malformed bytecode is rejected earlier as a structured
`CodegenError::Verify` diagnostic with the faulting `pc`.

**Java-program errors are not panics.** NPE, `ArrayIndexOutOfBoundsException`,
`ArrayStoreException`, `NegativeArraySizeException`, `ArithmeticException`,
`ClassCastException`, divide-by-zero — all are `Throw`s (real heap objects
on the runtime's exception tables), catchable by compiled code exactly like
the JVM.

## 4. Reentrancy (RefCell double-borrow) audit — re-issued for M3 Wave 2

A `RefCell` panic would require a borrow held *across* a call that can
re-enter the same object. The census below counts non-test
`.borrow()` / `.borrow_mut()` sites per file (re-issued after the M3
Wave-2 collections/langext additions; object accesses go through
`heap::with_obj` / `with_obj_mut`):

* **JObj borrows: 3, all inside the heap kernel** (`heap.rs`):
  `with_obj` (shared), `with_obj_mut` (exclusive) and the mark loop's child
  scan during collection. Every runtime and native access to object state
  goes through those two scoped accessors — **zero direct object borrows
  remain in `natives.rs`, `object.rs`, `array.rs`, `concat.rs`, `lib.rs`,
  `collections.rs` or `langext.rs`** (verified by grep).
* The remaining borrow sites are plain non-object containers: the frame
  slot registry (frame.rs 16), the heap/stats containers (heap.rs), the
  intern cache and `System.out` handle (object.rs 4) and the boxed-identity
  cache (natives.rs 2). None of them can re-enter user code: their values
  are indices, handles and cached map entries.
* Every object-access closure is statement-local by construction (the
  accessor takes `FnOnce(&JObj) -> T` and returns before the next
  instruction), so the old discipline — no borrow held across a dispatch
  into user code — is now enforced by the type system rather than by
  audit.
* **M3 Wave-2 additions dispatch into user code** (collection keys/elements
  dispatch `hashCode`/`equals`/`toString` through `invoke_virtual`). The
  audit re-check is mechanical and currently clean: **0 of the 68
  `with_obj`/`with_obj_mut` closures in `collections.rs` (34 shared + 34
  exclusive) and 4 in `langext.rs` contain an `invoke_virtual` call**
  (script-verified over brace-balanced closure bodies). Dispatch helpers
  (`collections::dispatch_hash_code/equals/to_string`) read values with
  scoped accessors first and dispatch with no heap borrow held; bulk
  readers (`hm_all_entries`) clone entries out of the bin before any
  dispatch. `HashMap` put/remove walks interleave `with_obj` reads and
  `with_obj_mut` writes strictly sequentially — no accessor is nested
  inside another accessor's closure.
* User code runs only through `invoke_virtual` / generated calls, which
  hold no heap borrow.
* `StringBuilder`/`String` sharing uses fresh `Rc` replacement on full
  writes (`langext::sb_set`) and `Rc::make_mut` on appends (ownership
  transfer, no borrow held).
* Nested `with_obj` on the *same* handle inside a closure panics exactly
  like the old `RefCell` discipline — and no such site exists (the
  accessors' closures never call accessors).

No reentrancy path exists in the current instruction set. This audit must
be repeated whenever a native gains a callback into user code (M3 Wave 3
lambdas and Wave 4 monitors are the next scheduled re-issues).

## 5. Known, documented gaps (not silent)

* ~~**No tracing GC** — cyclic graphs leak~~ — **resolved in M2 Wave 2**
  (arena mark-sweep collector; see §2.4 and `docs/M2_EVIDENCE.md`).
* ~~**No `StackOverflowError`**~~ — **resolved in M2 Wave 1**: deep
  recursion throws a catchable `StackOverflowError`; the frame registry's
  hard ceiling is derived from the measured per-frame native-stack
  footprint (`frame.rs`, M2_PLAN §3.1.1).
* **No threads / monitors** — `Thread`, `synchronized` (blocks and
  methods), `wait`/`notify` are compile errors with precise messages. M2
  delivers no concurrency and claims none (M2_PLAN §7); the M3 migration
  checklist (shared heap, shared statics, stop-the-world coordination,
  per-thread root registration) is a plan for future review.
* **Uncaught-exception reporting** prints the type and message exactly, but
  not JVM stack frames (no `LineNumberTable` support yet); the
  differential harness normalizes frame lines and nothing else.
* **`Caused by:` chains** are not yet supported end-to-end
  (`Throwable(String, Throwable)` constructors are compile errors today),
  so the differential harness deliberately does *not* normalize them away.

## 6. JMM-lite — what the concurrency surface guarantees (T16, M4 Wave 4)

This is the documented memory-model statement for the M4 concurrency
surface (real threads, monitors, wait/notify, interrupt). It mirrors the
JVM *as observed on Temurin 21.0.13* and pins where j2r is intentionally
weaker or stronger. Racy programs are not the differential suite's
contract — three-channel comparison covers deterministic programs — so
this section is the review artifact for everything the suite cannot see.

### Guaranteed

* **Monitor handover is the happens-before edge, exactly as in Java**
  (JLS §17.4.4): every write a thread performs before releasing a
  monitor is visible to whichever thread next acquires it; the same edge
  carries `wait` → `notify` handoffs (the waiter's writes before
  `wait` are visible to the notifier after it acquires; the notifier's
  writes before `notify` are visible to the waiter after re-acquisition).
  Implementation: all Java-visible object state mutates under the heap
  lock; the monitor state machine (T10) serializes owner changes on the
  same lock discipline, so release→acquire orders all prior heap writes.
* **`Thread.start` happens-before the child's every action; the target's
  death happens-before `join` returns** (the JDK start/join edges).
  Implementation: registration publishes the record before `start`
  returns; the death sequence (T15) stores `Exited` under the SCHED lock
  and notifies the same condvar the joiner waits on.
* **Stale *handles* are impossible, not merely discouraged**: a `JRef`
  naming a swept object fails the `(idx, gen)` check and turns into a
  loud panic (documented `.unwrap()` policy, §3) — torn access can never
  become silent corruption.
* **The runtime's own state (entry table, monitor table, coop ring,
  registries) is protected internally**; no Java-level action can
  observe a torn runtime invariant (§5 of M4_PLAN).
* **No long/double tearing** — *stronger than the JVM guarantees*: the
  JVM permits tearing for non-volatile `long`/`double` (JLS §17.7);
  every j2r field write is a single heap-lock-guarded slot store, so
  64-bit values are never observed half-written.
* **Default initialization races are closed**: the class-initialization
  state machine (JLS §12.4.2, landed in Wave 3 after the
  MonitorContention find) blocks concurrent first-touchers until
  `<clinit>` completes; a loser cannot read raw default statics.
* **Interrupt visibility** (T14): `interrupt()` publishes the status flag
  with release ordering and the blocking paths read it with acquire
  ordering at every wake; delivery clears it before the exception
  escapes.

### NOT guaranteed (use monitors; the JDK's weaker edges are not faked)

* **`volatile` does not exist**: compiled `volatile` fields are plain
  fields — no memory barriers, no ordering. A program relying on
  volatile for visibility or atomicity is out of contract.
* **`final`-field freeze semantics do not exist**: a `final` field is a
  plain field; a data race may observe its default value across threads
  (the JDK guarantees the initialized value for properly constructed
  objects — j2r does not, because there is no freeze barrier).
* **Data-race freedom for racy programs is not claimed**: two threads
  doing plain-field writes/reads race as in Java — stale reads are
  allowed. j2r's heap-lock-per-access gives each *individual* access
  atomicity (no tearing), which is slightly stronger than the JVM
  minimum, but ordering and coherence across a racy sequence follow the
  same "whatever the lock discipline did last" model; programs that need
  more must synchronize.
* **`Thread.sleep`/`Thread.yield` guarantee nothing about memory** (JDK
  behaviour, pinned in Wave 2 and unchanged): neither publishes nor
  acquires anything; coop-mode turns are scheduling, not memory edges.
* **Free-running mode is non-deterministic by design**: output
  equivalence under the differential harness is claimed only for
  fixtures whose ordering is enforced by join/monitor edges; the M4
  stress fixtures (T17) assert invariants, not output.

### The honest gap above the JVM

The JVM's JIT inserts acquire/release fences on monitorexit/monitorenter
and publishes `final` fields; j2r's correctness rests on the single heap
lock, which is *coarser* (every access serializes) but also *simpler* —
there is no instruction-level reordering window for Java-visible state.
What j2r does NOT have is any lock-free fast path that could subdivide
that guarantee; if one is ever added (inline caches on monitorenter,
per-object lock strips), this section must be re-issued before the wave
lands.
