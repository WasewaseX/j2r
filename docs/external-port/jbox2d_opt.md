# Opt.md — optimization analysis of j2r, driven by the jbox2d port

Date: 2026-11-12 · Input: the 156-class / 140,359-line output j2r produced for jbox2d 2.2.1.1, measured on a 2-core / 2 GB box.
Baseline: median 25× slower than the fully-JIT-warmed JVM (range 6.9–84×) with 1.7–4.8× lower RSS — see jbox2d_result.md.

Everything below is itemised by where in j2r's structure it lives, what it costs as measured here, and what the fix would be. Nothing in this document was applied to the benchmarked binary: the numbers in result.md are j2r output as-is, per the ground rule. Where I did measure a prototype, it lives in analysis/exp/ and is labelled [PROTOTYPE — measured].

How to read the impact column: cost/MS is the estimated share of a 10.6 s pendulum-300 run, computed as (measured op count) × (measured ns/op) from the counters in §1. It is an upper bound on what isolating that one component could win; components interact, so the shares do not add up to 100% usefully.

## 1. The measured cost ledger

This is the part most optimization advice skips, so it is first. I instrumented a copy of the runtime with relaxed atomic counters and ran the real ported workloads (analysis/exp/bench-ctl, counters identical to stock; times from analysis/exp/bench-exp).

### 1.1 Op counts per workload (release build, real bytecode, real data)

| counter | pendulum-60 | pendulum-300 | mixed-120 | trig-1M |
|---|---|---|---|---|
| FrameGuard::refresh calls | 2,040,447 | 13,303,955 | 6,663,565 | 5,000,027 |
| FrameGuard::enter (= Java method invocations) | 1,198,302 | 8,965,618 | 4,684,613 | 3 |
| heap::alloc | 31,303 | 32,791 | 37,510 | 19 |
| GC collections | 2 | 2 | 3 | 0 |
| obj_get (field read) | 6,906,455 | 58,730,349 | 37,898,962 | 0 |
| obj_set (field write) | 1,357,969 | 11,329,582 | 5,820,576 | 2 |
| invoke_virtual | 763,691 | 5,757,735 | 3,153,238 | 12 |
| pop_ref | 9,644,529 | 79,951,667 | 49,230,401 | 19 |
| pop_float | 5,300,560 | 52,694,845 | 30,084,517 | 4,000,000 |
| pop_int | 2,990,241 | 20,536,911 | 13,521,722 | 5,000,016 |
| pop_n (slots consumed) | 2,439,366 | 18,847,684 | 9,684,721 | 11,000,040 |
| stack_pop | 21,714,564 | 184,714,708 | 111,069,042 | 28,000,053 |

Read these against the static census in result.md §5 (18,328 pop_ref call sites, 13,614 obj_get/obj_set sites, 6,216 refresh sites in 140k lines). The static count says the emitter is uniform; the dynamic count says where the uniformity is spent: the same 18,328 static pop_ref sites execute 80 million times in pendulum-300 and 19 times in trig-1M — a 4-million-fold swing in how much the identical emitted code matters. Conversely refresh: 6,216 static sites, 13.3 M dynamic calls in one workload. Code size tells you nothing about the cost distribution; the counters do, and they say to fix the shapes, not the volume of the emitter.

Two facts jump out and they set the entire agenda:

- trig-1M runs 1,000,000 loop iterations in 3 method invocations and 19 allocations. It is one straight-line Java method — yet it spends ~2.1 s. Every nanosecond there is spent on the within-a-method machinery: operand stack, local slots, bounds checks, refresh.
- pendulum-300 does 185 M operand-stack pops, 80 M pop_ref, and 59 M field reads to advance 30 joints by 300 steps. This is a dispatch-and-access problem, not a floating-point problem.

### 1.2 Measured cost of each primitive (real runtime, release, analysis/micro/realrt)

| primitive | measured |
|---|---|
| obj_get (heap mutex + liveness + RefCell borrow + Value clone) | 18.1–20.4 ns |
| invoke_virtual, first-level hit | 37.5 ns |
| stack::pop_n + one push | 16.8 ns |
| pop_float | 7.6 ns |
| pop_ref | 9.6 ns |
| FrameGuard::enter + drop | 109.5 ns |
| FrameGuard::refresh (4 locals + 2 stack) | 73.7 ns |
| plain Rust field read from a &[Value], same enum | 1.6 ns |

### 1.3 What that implies (pendulum-300, 10,585 ms measured at the time of counting)

| component | op count | × ns/op | ≈ ms | ≈ share |
|---|---|---|---|---|
| refresh snapshots | 13.3 M | 73.7 | ~980 | ~9% |
| FrameGuard::enter + drop | 9.0 M | 109.5 | ~980 | ~9% |
| field reads obj_get | 58.7 M | 18.1 | ~1,065 | ~10% |
| field writes obj_set | 11.3 M | 18.1 | ~205 | ~2% |
| invoke_virtual | 5.8 M | 37.5 | ~215 | ~2% |
| operand-stack ops (pop_* + stack_pop) | ~337 M | ~4–16 | ~1,900 | ~18% |
| subtotal attributable | | | ~5.3 s | ~50% |
| FP/interpreter-arm residue | | | ~5.3 s | ~50% |

So roughly half of the port's runtime is in six identifiable runtime primitives, and the JVM's equivalent of every one of them is a register move. That is the whole story of the 25×.

## 2. Where the structure causes this

### 2.1 One loop { match pc } per Java method, with every value in a Vec<Value> (codegen: emit.rs)

Generated shape, verbatim from p4/src/classes/...:

```rust
let mut locals: Vec<Value> = vec![Value::Void; 3];
let mut stack:  Vec<Value> = Vec::with_capacity(4);
let mut pc: u32 = 0;
'run: loop {
    if let Some(t) = thrown.take() { return Err(t); }
    match pc {
        0u32 => { stack.push(locals[0].clone());
                  let __r = j2r_try!('run, thrown, fault_pc, 1u32, pop_ref(&mut stack));
                  stack.push(obj_get(&__r, 0)); ... }
        ...
    }
}
```

Cost measured in isolation vs. a typed slot machine for the same instruction sequence: Vec<Value> = 3.05 ns, typed u64 slots = 0.60 ns per add-chain → 5.1× (analysis/micro/stackcost.rs). The Value enum is 16 bytes (largest arm Ref(Option<JRef>) + tag), every iload is a clone of it, and pop_ref on a Value::Ref(None) must construct a Throw — so null-checks are on the hot path even when nothing is null.

The pc variable is the deeper issue: it is mut, assigned at the end of every arm, and read at the top of the loop. rustc cannot see that the CFG is acyclic-per-arm, so no arm is ever merged, no value is ever kept in a register across an arm boundary, and no Java local can live in a machine register at all — every Java local is a Vec slot load/store, and every Java expression result makes a round trip through Vec::push/Vec::pop. This is the single largest structural cost and it is not a runtime-library issue.

### 2.2 refresh() — 13 M exact snapshots where zero would do (runtime: frame.rs + emitted call sites)

refresh allocates nothing steady-state (it pops a buffer from a pool) but it does: a thread-local slot lookup behind a Mutex (with_slots), a walk of every local and every operand-stack entry filtering for Value::Ref, a Vec refill, and a write back under the same mutex. 73.7 ns, called before every call of any kind — 13.3 M times in one workload, to maintain a snapshot that is identical to the previous one almost every time (in jbox2d's solver, dozens of consecutive instructions touch only float locals).

The evidence this is worth attacking: in pendulum-300 there are 13.3 M refreshes but only 2 GC collections, and 32,791 allocations. The snapshot is consumed only by the collector and by suspension of the current thread. 99.98% of refreshes exist to preserve the option of a collection that does not happen.

### 2.3 invoke_virtual walks the class chain by name (runtime: lib.rs:376)

```rust
let mut cur: Arc<str> = heap::with_obj(*recv, |o| o.jclass.clone()); // heap lock + Arc clone
for _ in 0..64 {
    if let Some(f) = ctx.find_method(&cur, name, desc) { return f(Some(recv), args); }
    if let Some(n) = natives::lookup_exact(&cur, name, desc) { ... }
    cur = Arc::from(ctx.parent_of(&cur)?);       // string compare again
}
```

Each step is a (&str,&str,&str) triple lookup into generated match arms (see §2.4), and the first step already costs a heap-lock acquisition plus an Arc<str> clone just to read the class name. 37.5 ns per call, and the call target was already known at codegen time in the overwhelming majority of cases (javac emits invokevirtual on a receiver whose declared type fixes the implementation; genuinely polymorphic sites are a small minority).

### 2.4 The dispatch table is a 2,169-arm match on string tuples (codegen: partition.rs / registry)

p3/src/lib.rs alone is 1,365 lines: 675 arms of `("org/jbox2d/dynamics/World", "step", "(FII)V") => Some(crate::classes::...::m_step___FII_V as MethodFn)`, summed over the 8 partition crates to 2,169 arms. rustc compiles this to a chain of length/hash comparisons; the multi-level or-patterns for parents are emitted per-class. It is correct and it is compact source — but it is a linear string search executed 5.8 M times per workload.

### 2.5 %f/String.format — the bug, and the performance shape behind it (runtime: formatter.rs)

Not a speed item (the bench formats 12 lines total) but structurally relevant to §5: the formatter derived digits from the shortest round-trip representation and re-parsed it, which is why an exponent could be dropped. A digit-string formatter is both faster and exactly-correct-by-construction.

### 2.6 Prologue epilogue in every method (codegen + runtime)

FrameGuard::enter is 109.5 ns — bigger than obj_get and refresh combined — because it takes a thread-local Mutex, pushes a frame record, and allocates the monitor vector (Vec::with_capacity(8), 8 × JRef = 96 bytes, for the 99% of methods that never touch synchronized). jbox2d's solver calls 9 M methods; that is ~1 s of pure frame bookkeeping.

## 3. Ranked recommendations

### R1 — [PROTOTYPE — measured] Make FrameGuard allocation-free and only pay for monitors when used

- **Where:** j2r-runtime/src/frame.rs (FrameGuard::enter, Drop, record_monitor).
- **Change:** monitors: RefCell<Vec<JRef>> → RefCell<Option<Vec<JRef>>>, initialised None, get_or_insert_with on first monitorenter. Same for the drop path.
- **Measured:** −4.7% on pendulum-300, −4.9% on mixed-120 (300 s of A/B on identical source, counters identical, trig −10.3% mostly in refresh's buffer handling). Byte-identical output on every case checked (pendulum joints=30 steps=300 angSum=-0.000020, mixed bodies=63 steps=120 sx=-26.545284 maxY=20.000000).
- **Cost:** ~20 lines, no semantic change, no API change. This is a straight bug-level win and should be first.

### R2 — [PROTOTYPE — measured] Stop round-tripping the root snapshot through the global slot pool

- **Where:** frame.rs::refresh.
- **Change:** a thread_local! RefCell<Vec<u32>> scratch buffer instead of with_slots(|s| s.pool.pop()) + lock + write-back; and write into the slot in place (s.slots[self.slot].as_mut().unwrap()) instead of Some(handles) (which drops and frees the old Vec every call).
- **Measured:** included in the R1 figure above; isolated, the mutex round-trip was the majority of the 73.7 ns.
- **Caveat found while prototyping:** the sound version of this needs the slot to be re-entrant (refresh is called by the frame while the same thread may already hold the slot lock) — the prototype is correct because with_slots is only taken on the write, but a lock-scope audit is required before landing (docs/M4_PLAN L1/L2/L3 laws).

### R3 — Only refresh when a reference value actually changed — the big one

- **Where:** emit.rs (change when __frame.refresh is emitted, not the runtime).
- **Change:** the emitter already tracks, per instruction, the JVM category of everything it pushes and pops. Emit refresh only on the subset of instructions that can change the set of live references reachable from this frame: astore/aload-mutating stores into a local holding a reference slot, pop/pop2 of a reference-typed operand, and at collection points. A float/int/double-only straight-line stretch emits nothing. The runtime side needs one bit: a per-frame dirty flag set by those stores, and refresh early-returns if !dirty && generation == snapshotted_generation.
- **Why it is safe, stated precisely:** a snapshot only needs to be exact when it is observed — i.e. at gc_point and at thread suspension. Between two observations, if no reference-carrying slot changed and no collection happened, the previous snapshot is still a superset of the live set. The generation check catches collections.
- **Upper bound:** 13.3 M refreshes × 73.7 ns ≈ 0.98 s of 10.6 s in pendulum-300; in jbox2d's solver, where the hot inner loops are pure float math, my sampling suggests >90% of refreshes are avoidable. Realistic expectation 10–15% overall, and it compounds with R2.
- **Risk:** this is the one item where a mistake is a silent GC rooting bug. It must land with the existing rooting tripwires (stale JRef panics) plus a forced-GC-every-N stress mode (heap::set_forced_gc_every(1) already exists — run the corpus with J2R_GC_EVERY_N=1).

### R4 — Give the interpreter registers: type the operand stack, kill the pc loop

- **Where:** codegen/emit.rs + runtime/stack.rs — the largest change here by an order of magnitude.
- **Change, staged:**
  1. **Cheap, big:** overlap stack with a fixed-size array [Value; 16] + sp: usize (JVM operand stack depth is bounded by the verifier and is ≤ 16 in essentially all javac output; spill to a Vec only on overflow). Removes Vec capacity checks and the len mutation from every op. Measured proxy: typed slot machine 0.60 ns vs 3.05 ns per add-chain = 5.1× on stack work.
  2. **Real:** emit straight-line Rust for basic blocks with a single successor — which is most of every solver loop body — and keep the match pc only at goto targets, switch, and exception-landing boundaries. Then Java locals become let mut bindings and rustc's SSA construction does the rest.
  3. **Ceiling:** full structured recovery (Java CFG → if/while/loop with labelled break), which is what makes javac → Rust competitive with javac → C in the literature. j2r's opcode surface is already verified; this is an emitter rewrite, not a verifier one.
- **Upper bound:** operand-stack ops are ~18% of pendulum-300 and ~35% of trig-1M (where there is nothing but stack work: 28 M stack_pop + 5 M pop_float for 1 M iterations). Combined with R5, trig-1M plausibly lands within 3× of the JVM rather than 11×.
- **Note:** this is the optimization that changes j2r from "faithful interpreter emitter" to "compiler", and it is the reason the current design is a defensible first milestone.

### R5 — Resolve invokevirtual at codegen time; make the dynamic case a vtable lookup, not a string walk

- **Where:** emit.rs (site analysis) + runtime/lib.rs::invoke_virtual.
- **Change:**
  1. When the verifier proves the receiver's static type and the class is final or the method is not overridden in any compiled class, emit a direct call to the partition function. This needs a whole-program "is overridden" query, which the partitioner already computes for meta_of.
  2. For genuinely polymorphic sites: number the classes, store a u32 class id in JObj alongside jclass, and give each class a per-(name,desc)-hashed slot table (fn pointer per interned selector). invoke_virtual then becomes hash(selector) → slot → call, with no Arc<str> clone and no heap lock.
  3. Keep the string path as the diagnostic fallback so error messages stay as good as they are today (they are excellent — no implementation found for {name}{desc} when dispatched on {cls}).
- **Measured share:** 5.8 M calls × 37.5 ns ≈ 215 ms in pendulum-300 (2%) — the win is not the call itself, it is deleting the Arc<str> + heap-lock from the call path, which also feeds R6.

### R6 — Give the collector's hot accessors a borrow that does not take a global mutex

- **Where:** heap.rs (with_obj, alloc) + object.rs.
- **Change:** obj_get is 18–20 ns for what is a Vec index and a 16-byte copy. Measured breakdown: plain slice read is 1.6 ns, so ~17 ns is mutex + live_slot_index + RefCell + Value clone. Options, in increasing invasiveness: (a) replace Mutex<Option<Heap>> with a RefCell for the single-threaded case plus a fast Cell<u8> "multithreaded" flag that swaps in the mutex (jbox2d's World is single-threaded; j2r already knows the thread count); (b) UnsafeCell + a documented single-writer invariant (violates j2r's zero-unsafe law, so: no); (c) keep the mutex but make obj_get/obj_set take the guard once for a whole basic block — an emitted `let mut h = heap::borrow(); ... h.get(r,0) ... h.set(r,1,v)` region, which removes 58.7 M lock/unlock pairs from pendulum-300 and is the honest fix under the current architecture.
- **Upper bound:** 58.7 M × ~16 ns ≈ 0.94 s of 10.6 s (~9%) for reads alone; ~11% with writes. (c) is the recommended route — it needs no memory-model risk and no unsafe.

### R7 — Make the dispatch table a perfect hash, not 2,169 match arms

- **Where:** partition.rs (table emission).
- **Change:** emit static SELECTORS: &[(&str,&str)] sorted at build time and a match on a computed 64-bit hash of (class,name,desc) (rustc then emits a jump table / binary search on integers instead of a chain of string compares), with a debug_assert full-equality check on hit. Pure codegen change; the error paths stay identical.
- **Impact:** second-order today (it is inside invoke_virtual, R5), but it is what makes R5.2 cheap, and it reduces the compile-time cost of the 2,169-arm match, which at 156 classes is already part of the 111.8 s release build.

### R8 — Turn on the machine that is actually hot: -C target-cpu=native, PGO, and codegen-units=1

- **Where:** j2r's emitted Cargo.toml / build defaults.
- **Change:** jbox2d's solver is branchy f32 math. FP throughput on this box would gain from target-cpu=native (FMA contraction must stay off for JVM parity — see bug.md B3 — so only the scheduling/vectorisation part applies, which is why this is R8 and not R3). PGO needs a training run, which for a library port means shipping the training harness; worth it only for a game with a stable frame loop.
- **Expected:** single-digit percent; listed because it is free to try and j2r currently sets none of it.

### R9 — Fix the compile side: 111.8 s of cargo for 156 classes

- **Where:** partition.rs / emitted crate layout.
- **Facts:** j2r translates in 156 ms; the Rust build takes 111.8 s (clean, -j2, warm cache) and is the entire compile story. Cause is the same shape as §2.1: 140k lines of untyped-Value code with 24,921 j2r_try! expansions and 1,117 Vec<Value> locals means enormous MIR, and j2r's own docs/BENCHMARKS.md measures ~4.2 MB peak rustc RSS per class.
- **Change:** emit the crate-level partition as a function of module count not class count, and enable codegen-units + incremental in the generated workspace by default; the OOM in bug.md B13 is the same problem seen from the memory side. A typed-stack emitter (R4) shrinks MIR substantially and would cut this too.
- **This matters more than it looks:** for the 70k-class target (M5), 111.8 s/156 classes scales to ~14 hours even before the memory wall.

## 4. What I would not do

- Do not switch the GC to a copying/compacting collector to speed up alloc. alloc is 32k times per workload — 0.03% of the time. The heap is not the problem; the accessors are (R6).
- Do not make Value smaller (e.g. NaN-boxing into u64). The 16-byte enum is not what costs 3 ns/op; the Vec push/pop and the clone are. NaN-boxing would complicate Value's Hash/PartialEq (used by String.format and Arrays.sort) for a rounding-error win. Revisit only after R4.
- Do not replace match pc with computed goto / threaded-code table. Rust has no goto; a match on a dense u32 already compiles to a jump table. The problem is not dispatch, it is that values cannot stay in registers across arms (R4.2).
- Do not vectorise the solver by hand. The JVM's C2 does not either, and doing so would break the byte-identical result that is the port's actual achievement.
- Do not "fix" Math.sin by switching to the platform libm. It is faster and wrong (27/200,000 mismatches vs the JVM, bug.md B3).

## 5. Correctness items that also pay for themselves

| item | why it belongs in an optimization document |
|---|---|
| Math.sin/cos still differ from Temurin on ~6 inputs per 100k (bug.md B3/O1) | the fdlibm transliteration is 0-mismatch vs C fdlibm; HotSpot is not pure fdlibm. Closing this needs HotSpot's actual tables. Until then an exhaustive parity fixture would find them, and any game that seeds physics from Math.sin can diverge. This is the highest-value correctness item in the whole port. |
| __rem_pio2_large (Payne–Hanek) missing (O2) | panics loudly at \|x\| ≥ 2^20·π/2 instead of returning a plausible wrong value — honest, but a real gap |
| formatter %f band [1e-9,1e-3) has no fixture (O4) | the bug that cost hours (bug.md B1) had zero test coverage; add 15 values to corpus/FmtResidualProbe.java |
| no partition fixture with an exception table (O5) | one 20-line fixture covers the whole B2 class |
| differential harness inherits the ambient charset (O6) | 5 phantom failures on a C locale; should pass -J-Dfile.encoding=UTF-8 itself |

## 6. Process suggestions for j2r itself

1. **The API surface should be probed, not discovered.** I mapped jbox2d's JDK surface by writing 25 probes over javap output of all 167 classes (probe/, probe2/) before compiling anything. That took an hour and replaced ~10 sequential compile-fix cycles. j2r could ship exactly this as a mode: `j2r cfg-report --surface` → the sorted, deduplicated JDK member set the input needs, diffed against the NATIVES table. cfg-report already exists; the surface diff is a day's work and would have told me B4–B12 in one page.
2. **The first unresolved reference is only the first.** Ten compiles, ten gaps. A "collect all diagnostics" mode (walk every class, report every unresolved member, then fail) would collapse that to one.
3. **Partition-mode tests are structurally blind to exception tables (B2/B14)** — the fixtures partition classes with no try/catch. Add a --partition-classes 2 variant to the existing corpus run: it costs one extra build per fixture and would have caught a bug that makes multi-class ports impossible.
4. **The formatter deserves a property test against java.lang.String.format**, over magnitudes spanning all three repr regimes and both signs. This is a small, enumerable input space — the opposite of the transcendental functions — and it is where a silent wrong-answer bug lived for the whole first phase of this port.

## 7. If I had one week

R1 + R2 (measured −5% already, mechanical, zero risk). R3 (refresh only on reference writes) — highest ratio of win to risk, needs the forced-GC stress harness green. R6(c) (per-basic-block heap guard) — removes the most-numerous expensive primitive from the call path. R4.1 (bounded operand-stack array) — 5× on the stack work, touches only stack.rs + the emitter's stack declaration.

Those four do not change j2r's design; they take the measured parts of the 25× and make them smaller. R4.2/R4.3 (register allocation for Java locals) is the item that decides whether a j2r port is a curiosity or a shipping option — and it is a compiler project, not a patch.
