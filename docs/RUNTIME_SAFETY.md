# Runtime safety — ownership, cycles, and panic paths

This document states precisely what the milestone-1 runtime (`j2r-runtime`)
guarantees, what it deliberately does not, and how every panic path is
prevented from being reachable. It is the reference for the claims that may
be made about the current **single-threaded** milestone, and it will be
revised — not silently widened — when threads and the tracing GC land.

## 1. The ownership model

Every Java object lives in exactly one shape:

```text
JRef = Rc<RefCell<JObj>>          // shared, mutable, single-threaded
JObj { jclass: Rc<str>,           // runtime class / array descriptor
       slots:  Vec<Value> }       // flattened field slots
```

* **Sharing** models Java reference semantics: two variables can point at
  the same object, and `Rc::ptr_eq` is Java's `==`.
* **Interior mutability** (`RefCell`) models Java's mutable fields; every
  access is dynamically checked, so there is no `unsafe`, no aliasing UB,
  and a mis-nested borrow is a controlled panic — never memory corruption.
* **`Value::Str(Rc<String>)`** is the character payload of `String` /
  `StringBuilder` objects. Mutations go through `Rc::make_mut`
  (copy-on-write), so a `String` aliasing a `StringBuilder` buffer is never
  mutated in place — `sb.toString()` then `sb.append(...)` behaves exactly
  like the JDK.
* Interning (`object::intern_string`) maps equal literals to one object,
  matching constant-pool identity; the cache is `thread_local`.

## 2. What is safe to claim today (single-threaded milestone)

1. **Memory safety.** No `unsafe` anywhere in generated code or in
   `j2r-runtime`; all interior mutability is `RefCell`-checked. There is no
   path to use-after-free, dangling references, or data races — the latter
   because there is exactly one Java thread.
2. **Reference identity.** `==` / `!=` (`if_acmpeq`), string interning and
   identity hashing behave like the JVM.
3. **Determinism.** With one thread and no external state, a compiled
   program is deterministic for a given input.
4. **Cycles leak memory — they do not corrupt behavior.** An object graph
   with a reference cycle is never collected by `Rc`. Precisely: the
   objects stay reachable (correct semantics — the program can still read
   them), the memory is freed only at process exit, and *nothing else*
   changes: no double-free, no resurrection, no wrong values. The honest
   boundary: a long-running program that allocates unbounded cyclic graphs
   will eventually exhaust memory — at that point the process aborts like
   any out-of-memory native program. For the milestone's target
   (correctness of bounded, batch-style Java programs) this is acceptable
   and is exactly why the roadmap puts a tracing, cycle-collecting GC
   before any long-running or threaded workload (M2).
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

## 4. Reentrancy (RefCell double-borrow) audit

A `RefCell` panic would require a borrow held *across* a call that can
re-enter the same object. The runtime was audited site by site; the census
below counts non-test `.borrow()` / `.borrow_mut()` sites per file
(natives.rs 30, object.rs 6, array.rs 4, lib.rs 5; concat.rs has no direct
borrow — its 4 string reads borrow through `object::string_value`, counted
under object.rs; the `intern_string`/`BOXED_CACHE` RefCells cache
`Rc<str>`/`JRef` keys and cannot re-enter user code):

* every borrow scope is statement-local or ends before the function
  dispatches into user code (`format_ref` releases the string borrow before
  calling `toString`; `PrintStream` natives read the sink id, drop the
  borrow, then format; `checkcast`/`instanceof`/`invoke_virtual` clone the
  `jclass` string and drop the borrow before dispatching; boxed `equals`
  clones both payloads and drops both borrows before comparing);
* user code runs only through `invoke_virtual` / generated calls, none of
  which hold a runtime borrow;
* `StringBuilder`/`String` sharing uses `Rc::make_mut` (ownership transfer,
  no borrow held).

No reentrancy path exists in the current instruction set. This audit must
be repeated whenever a native gains a callback into user code (e.g. future
`Comparator` support).

## 5. Known, documented gaps (not silent)

* **No tracing GC** — cyclic graphs leak (see §2.4). Tracked as M2.
* **No `StackOverflowError`** — deep recursion exhausts the (large)
  pre-allocated thread stack and aborts the process. Tracked as M2.
* **No threads / monitors** — `Thread`, `synchronized` (blocks and
  methods), `wait`/`notify` are compile errors with precise messages.
* **Uncaught-exception reporting** prints the type and message exactly, but
  not JVM stack frames (no `LineNumberTable` support yet); the
  differential harness normalizes frame lines and nothing else.
* **`Caused by:` chains** are not yet supported end-to-end
  (`Throwable(String, Throwable)` constructors are compile errors today),
  so the differential harness deliberately does *not* normalize them away.
