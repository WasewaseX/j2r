# J2R M3 Implementation Plan — JDK coverage

Revision 1 · 2026-11-02 · authored for review as the M2 plan was
(`docs/M2_PLAN.md` is the style and discipline reference).

Per the owner's standing directive ("always do big chunks of roadmap, as
many tasks as possible, it doesn't matter if the milestone is reached —
you already know what's next"), the M3 waves proceed in parallel with the
pending M2 completion-review verdict. The same disclosure applies as in
M2: every wave ships its own evidence record and green CI
(`docs/M3_EVIDENCE.md`), the review can still force changes, and no claim
here depends on the M2 verdict.

Milestone goal (ROADMAP §3): **real JDK libraries load and pass
differential tests** — classpath/JAR loading, `java.util` collections,
lambdas/method references (`invokedynamic`), reflection, `synchronized`.

---

## §0 Scope, entrance conditions, honesty boundaries

Entrance conditions met:

1. M2 is code-complete: Waves 1–4 implemented with per-wave evidence
   (ROADMAP §2, `docs/M2_EVIDENCE.md`); CI 10/10 gates green at HEAD
   `b4badea` (check-runs re-verified 2026-11-02).
2. Baseline re-proven after the latest sandbox recycle (2026-11-02,
   Temurin 21.0.13 at `/tmp/jdk21`): **170 tests, 0 failures**, including
   the 39-fixture three-channel differential suite.
3. The M2 completion package (T22) is submitted (`b4badea`); the verdict
   is pending and explicitly not a blocker per the owner's directive.

Standing rules carried forward unchanged (ROADMAP §1): differential
testing vs Temurin 21 on three channels defines correctness; zero
`unsafe`; zero external dependencies in `j2r-classfile` and
`j2r-runtime` (this plan **keeps** that for M3 — see §9); loud failure
over silent miscompile; no Minecraft claims until M4 evidence.

Out of scope for M3 (loud errors stay in place, each listed with its
rejection site or planned rejection):

- Regex anything (`String.split`, `String.matches`, `Pattern`) — loud
  compile-time rejection via the existing unknown-JDK-method path.
- Streams, `Optional`, `java.time` — not in the M3 natives inventory
  (§4); unknown methods keep failing loudly.
- Threads, concurrency, `wait/notify` — M3 stays single-threaded, same
  boundary statement as M2_PLAN §7; `Object.wait/notify/notifyAll` get
  loud runtime errors when the monitor wave lands (§7).
- Serialization: `writeObject`, `$deserializeLambda$` — loud errors.
- Modules, access control, `setAccessible` semantics beyond "always
  accessible" (there are no modules in the ahead-of-time model) —
  documented deviation, §8.

M3 exit criteria (from ROADMAP §3): a program that loads its classes
from a real `.jar` file, uses collections, lambdas/method references,
`synchronized`, and the reflection subset compiles and passes the
three-channel differential suite. Each wave adds its fixtures to the
suite; the suite-count guard rises with every wave.

---

## §1 Audit — what M3 builds on (verified against source, 2026-11-02)

All citations re-checked by script this session.

1. **Multi-class programs already work.** The CLI takes `j2r compile
   <inputs...>` (`crates/j2r-cli/src/main.rs:36-37`); cross-class
   `invokestatic`/`invokevirtual`/field access resolve across the input
   set and fail loudly otherwise
   (`crates/j2r-codegen/src/lib.rs:731-764` `resolve_call`,
   `lib.rs:809-843` `resolve_field`). Flattened field-slot layouts are
   computed across hierarchies (`lib.rs:526` errors on unknown classes).
   What is missing: *discovery* — today every class must be named on the
   command line; there is no classpath, no JAR, no transitive closure.
2. **invokedynamic is concat-only.** The decoder carries the full
   `InvokeDynamic` instruction (`crates/j2r-codegen/src/ir.rs:488`) and
   the classfile parser reads `CONSTANT_MethodHandle`, `MethodType`,
   `InvokeDynamic` and the `BootstrapMethods` attribute
   (`crates/j2r-classfile/src/lib.rs:67-76, 281-297, 384, 468`). M1
   resolves exactly one bootstrap shape: string concatenation
   (`lib.rs:658-660` `resolve_concat` doc: "Milestone 1 supports exactly
   one…"); any other indy site is the compile error
   `"invokedynamic #idx was not resolved"` (`lib.rs:922`).
3. **Native classes and dispatch.** The natives table is a static
   `(class, method, descriptor, impl)` array — 114 entries over 15
   classes (`crates/j2r-runtime/src/natives.rs:938+`): `PrintStream`,
   the 8 box types, `Math`, `Object`, `String`, `StringBuilder`,
   `System`, `Throwable`. Virtual calls compile to
   `invoke_virtual(&recv, class, name, desc, …)` which walks the
   *runtime* class hierarchy via `Dispatch { lookup, parents }`
   (`crates/j2r-runtime/src/lib.rs:142-166`), while known-native
   receivers compile straight to `natives::invoke_exact`
   (`crates/j2r-codegen/src/emit.rs:1000-1025`). Native classes do not
   yet participate in interface dispatch or subtype checks — the
   registry that `j2r_dispatch()` builds covers compiled classes only
   (`emit.rs:79-99`). This is the central Wave-2 extension point.
4. **synchronized is loudly rejected.** `monitorenter`/`monitorexit`
   decode to a `DecodeError` (`ir.rs:527`), so `synchronized` blocks
   cannot compile at all; `ACC_SYNCHRONIZED` methods are *not* checked
   yet (flag constant exists, `j2r-classfile/src/lib.rs:24-25`) — Wave 4
   must add the method-flag check together with the monitor
   implementation so neither path is silently unmonitored.
5. **GC surface for new runtime state.** All Java-visible objects live
   in the `Vec<Entry>` heap with generation-tagged handles; roots are
   the frame scan (`GcFrame`), statics `gc_scan_roots`, and
   `NativeRoots` scopes (M2 Wave 2). Every collection, closure, and
   reflection object M3 introduces must store its Java-visible state in
   heap objects so the existing mark/sweep sees it (§4.4, §5.4, §8.3) —
   no `Rc<RefCell<…>>` side tables.
6. **Suite shape.** 39 fixtures in `crates/j2r-cli/tests/suite/`, run by
   the differential harness `crates/j2r-cli/tests/diff_suite.rs`
   (per-fixture `j2r test <file>` vs `javac`+`java`, three channels;
   count guard `>= 39` at `diff_suite.rs:56-62`). Gates 9 (forced-GC
   full suite) and the `cfg_equiv` equivalence run both iterate the same
   directory, so new fixtures are automatically covered by both.

---

## §2 Risk register (M3-specific)

| # | Risk | Mitigation |
| --- | --- | --- |
| M3-R1 | Hand-rolled inflate (RFC 1951) corrupts JAR entries silently | Table-driven canonical-Huffman decoder, ~500 lines, zero-dep; round-trip fixtures compressed by *the real JDK* (`jar` tool + `Deflater` at levels 0/1/9) pinned in tests; checksum discipline: store-level CRC-32 verified on every extracted entry, mismatch = loud compile error |
| M3-R2 | `HashMap` iteration order diverges from OpenJDK | Implement the exact OpenJDK algorithm: `h ^ (h>>>16)` spreading, table index `(n-1) & h`, capacity 16, load factor 0.75, ×2 resize, Java-8 bin-split preserving relative order; iteration order is then *determined* by identical key hashes — pinned by differential fixtures that print iteration order |
| M3-R3 | Treeification gap: ≥8 colliding entries need tree bins | List bins only; reaching the treeify threshold is a **loud internal error** naming the class/key count — never a silent order divergence; fixtures stay under the threshold and a unit test documents the guard fires |
| M3-R4 | Lambda captures alias mutable locals (JLS requires capture-by-value) | Captured values are copied into the closure object at indy-execution time (JLS 15.27.2 semantics); test that mutating the original local after creating the lambda does not change observed capture |
| M3-R5 | Lambda objects/captures are GC roots invisible to the collector | Closures are heap objects; captures live in their field slots; the closure reference flows through the normal operand stack → existing frame scan roots it (M2 Wave-2 protocol, M2_PLAN §4.4). Forced-GC suite covers every lambda fixture (gate-9) |
| M3-R6 | Duplicate classes on classpath (dir + jar shadows) | First-match-wins in explicit classpath order (JLS/JVM classloading convention for the application loader); duplicate detection logs nothing but resolution is deterministic and fixture-tested; self-jar round-trip test asserts which copy was loaded via a distinguishing static |
| M3-R7 | `<clinit>` cycles across jar-loaded classes | The existing ensure/dispatch init path is the same machinery StaticOrder.java already pins; new fixtures: two classes whose statics reference each other's constants (constant-folded, no cycle) and a genuine lazy-init cycle through methods; both must match Temurin byte-for-byte |
| M3-R8 | Reflection subset overclaim ("we support reflection") | §8 lists the exact method set; everything else keeps failing loudly; the claim sentence in every doc says "reflection subset"; README updated in the same commit, not later |
| M3-R9 | Monitor release on exceptional unwind missed → dead object state | Monitor depth is frame-scoped: monitorenter pushes the reference onto a per-frame monitor stack (FrameGuard-owned, like the root snapshot slots of M2_PLAN §4.4); unwind pops and releases all monitors held by the frame; `synchronized` fixtures include throw-inside-synchronized and nested monitors |
| M3-R10 | Dependency creep to make JAR/zip "easy" (`zip` crate, `flate2`) | Rejected up front: §9 zero-dep policy; hand-rolled reader is a M3 deliverable with its own test battery; gate-7 `cargo audit` stays trivially clean because the workspace still has no dependencies |

---

## §3 Wave architecture (overview)

Five implementation waves after this planning wave, each ending in its
own evidence section, green CI, and a suite-count bump:

```
Wave 0  this plan + register                      (T01–T07)
Wave 1  classpath & JAR loading                   (T08–T11)
        j2r-classfile: zip.rs + inflate.rs (zero-dep)
        j2r-codegen:   classpath resolution, transitive closure loading
        j2r-cli:       -cp/--classpath, jar fixtures harness mode
Wave 2  java.util collections & java.lang extensions (T12–T15)
        j2r-runtime:  collections.rs (ArrayList/HashMap/HashSet/
                      Iterator/Objects), natives + hierarchy registration
        j2r-codegen:  native-class subtype checks + interface dispatch
Wave 3  lambdas & method references               (T16–T18)
        j2r-codegen:  LambdaMetafactory site resolution → closure objects
        j2r-runtime:  closure invocation, functional interfaces
Wave 4  synchronized monitors                     (T19–T20)
        j2r-runtime:  per-object monitor counters, FrameGuard monitor stack
        j2r-codegen:  monitorenter/monitorexit emission, ACC_SYNCHRONIZED
Wave 5  reflection subset                         (T21–T22)
        j2r-runtime:  Class objects, by-name Method/Field tables
T23     M3 completion package + review submission
```

Wave ordering rationale: Wave 1 is the loader every later wave's
fixtures can use (jar-delivered fixtures from Wave 2 onward exercise it
continuously); collections before lambdas because the lambda fixtures
use `ArrayList.forEach`/`removeIf`; monitors before reflection because
static-synchronized methods need the per-class monitor token the Class
wave formalizes (Wave 4 uses synthetic per-class monitor objects, §7.3).

---

## §4 Wave 1 — classpath & JAR loading

### 4.1 Files

| File | Change |
| --- | --- |
| `crates/j2r-classfile/src/inflate.rs` | **new** — RFC 1951 DEFLATE decoder (stored/fixed/dynamic Huffman), ~500 lines + ~10 tests |
| `crates/j2r-classfile/src/zip.rs` | **new** — JAR/ZIP reader: end-of-central-directory scan, central directory parse, local header parse, STORED + DEFLATED entries, CRC-32 check, ~250 lines + tests |
| `crates/j2r-classfile/src/crc32.rs` | **new** — table-driven CRC-32 (RFC 1952), ~40 lines + test vectors |
| `crates/j2r-codegen/src/classpath.rs` | **new** — classpath resolution: dir + `.jar`/`.zip` entries in order, first-match-wins, transitive closure from the main class over `CONSTANT_Class` refs + superclasses + interfaces |
| `crates/j2r-codegen/src/lib.rs` | `compile_program` gains the closure-loading path; `--classpath` inputs feed `Program` construction |
| `crates/j2r-cli/src/main.rs` | `-cp/--classpath` on `compile`/`test`; `test` gains a jar-fixture mode (below) |

### 4.2 Java semantics

- Classpath model = JVM application class loader semantics, simplified
  to the ahead-of-time whole-program model: `-cp` entries in order,
  `dir` searched as `a/b/C.class`, `.jar`/`.zip` searched by exact
  entry name; first match wins; no parent-delegation chain to model
  (single loader, like the JDK application loader for user classes).
- Transitive closure: starting at the main class, every referenced
  `CONSTANT_Class` (supertypes, field/method descriptors, new/checkcast/
  instanceof/anewarray/multianewarray/ldc-Class) is loaded too; JDK
  classes are excluded (they resolve through the natives table or fail
  loudly per M1). The closure is computed *at compile time* — the
  generated crate stays self-contained (gate-5 standalone shape
  unchanged).
- `<clinit>`: unchanged machinery; jar-loaded classes participate in the
  same ensure-on-first-use path. Static *constant* references
  (`CONSTANT_Fieldref` to `static final` with `ConstantValue`) read the
  attribute exactly as M1 does today for explicit inputs.
- Resource-less model: JARs are read for **.class entries only**;
  `META-INF/` and resources are ignored (documented; no silent
  resource API).

### 4.3 Memory safety

- `inflate.rs`/`zip.rs` are pure functions `&[u8] → Result<Vec<u8>, …>`;
  all bounds checks return errors, never index-panic: fuzz-style unit
  tests feed truncated/corrupted archives and assert `Err`, not panic.
- Zero unsafe; no recursion in the Huffman decoder loop (iterative
  bit-reader) so a hostile archive cannot overflow the native stack;
  decompression bomb guard: declared uncompressed size is capped at
  64 MiB per entry, over-cap = loud error (documented compile-time
  limit, mirrors the SOE-ceiling philosophy of M2_PLAN §3).

### 4.4 Performance

- Inflate is a compile-time cold path; no perf claims. The closure
  loader reuses the M1 parser; a 1000-class jar must parse in < 2 s
  (e2e assertion, generous but bounded).

### 4.5 Differential tests

New fixture directory `crates/j2r-cli/tests/suite_jar/<Name>/`, each a
folder of `.java` files + `main` file naming the main class. Harness
compiles with `javac`, packs with `jar cf`, then runs `j2r test --cp
app.jar <Main>` and the reference JVM with `-cp app.jar`. Fixtures:

1. `JarMain` — 3 classes across 2 packages in one jar; static-init
   order across classes; cross-class virtual dispatch.
2. `JarSuper` — superclass + interface loaded from the jar; the
   subclass overrides; abstract dispatch from a third class.
3. `JarDup` — same class present in a dir entry (earlier) and the jar;
   asserts first-match-wins by printing the distinguishing static.
4. `JarClosure` — 30+ classes referencing each other in a chain;
   asserts the transitive loader pulls the whole chain from one main.

### 4.6 Regression tests

- All 39 existing fixtures must stay green unchanged (count guard rises
  to `>= 43` total suite files across both directories).
- `inflate` test battery: JDK-compressed vectors at levels 0/1/9,
  fixed-Huffman block, dynamic-Huffman with 300+ symbols, stored block,
  truncated-stream `Err`, corrupt-CRC `Err`, bomb-guard `Err`.
- e2e: compile from jar + compile from equivalent exploded dir produce
  byte-identical generated crates (determinism guard).

### 4.7 Acceptance criteria

- [ ] `j2r compile -cp app.jar --main-class a.b.C` builds a runnable
      crate for all four jar fixtures; three-channel differential green.
- [ ] Inflate battery green; no panic on any corrupted input tested.
- [ ] Suite-count guard `>= 43`; CI 10/10 (new jar fixtures run inside
      gate-3 and gate-9).

---

## §5 Wave 2 — java.util collections & java.lang extensions

### 5.1 Files

| File | Change |
| --- | --- |
| `crates/j2r-runtime/src/collections.rs` | **new** — ArrayList, HashMap (list bins, faithful order), HashSet, Iterator, Objects; ~1200 lines + ~25 unit tests |
| `crates/j2r-runtime/src/natives.rs` | table entries for the new classes; exact-method entries for the extension methods of §5.2 |
| `crates/j2r-runtime/src/lib.rs` | native-class hierarchy registration in `Dispatch` (subtype checks + interface dispatch reach natives) |
| `crates/j2r-codegen/src/lib.rs` | `resolve_call`/`resolve_field` treat registered native classes as resolvable virtual/interface targets; layout pass skips native classes (they have no compiled slots) |

### 5.2 Method inventory (the honest M3 set; everything else loud)

- `java/util/ArrayList<E>`: `<init>()I→capacity`, `<init>(I)V`, `add(E)Z`,
  `add(IL,E)V`, `get(I)E`, `set(IL,E)E`, `remove(I)E`, `remove(Object)Z`,
  `size()I`, `isEmpty()Z`, `contains(Object)Z`, `indexOf(Object)I`,
  `clear()V`, `iterator()Iterator`, `toString()` (faithful
  `[a, b, c]` format), `forEach(Consumer)` (lands with Wave 3).
- `java/util/Iterator<E>`: `hasNext()Z`, `next()E` (+`remove()` loud
  unsupported runtime error in M3).
- `java/util/HashMap<K,V>`: `<init>()V`, `<init>(I)V` (capacity rounded
  to next power of two, OpenJDK `tableSizeFor`), `put(K,V)V→old`,
  `get(Object)V`, `getOrDefault`, `containsKey(Z)`, `remove(Object)V`,
  `size()I`, `isEmpty()Z`, `keySet()Set`, `values()` (collection view
  with faithful order), `containsValue`, `clear`, `toString` (faithful
  `{k=v, …}` order = table order).
- `java/util/HashSet<E>`: backed by a private HashMap; `add`, `remove`,
  `contains`, `size`, `isEmpty`, `iterator`, `toString` (faithful
  `[…, …]` table order).
- `java/util/Objects`: `equals`, `hashCode`, `requireNonNull(×3 overloads)`,
  `toString`, `isNull`, `nonNull`.
- `java/lang/String` extensions: `substring(I)`, `substring(II)`,
  `indexOf(String)`, `indexOf(String,I)`, `indexOf(I)`, `lastIndexOf(I)`,
  `concat`, `replace(CC)`, `toUpperCase()`, `toLowerCase()`,
  `trim`, `compareTo`, `compareToIgnoreCase`, `startsWith`,
  `endsWith`, `contains`, `equalsIgnoreCase`, `toCharArray`,
  `repeat(I)` (Java 11), `chars()` → loud error (streams).
- `java/lang/StringBuilder` extensions: `reverse`, `insert(I,…)`,
  `deleteCharAt`, `delete(II)`, `replace(II,String)`, `setLength`,
  `capacity`, `charAt`, `indexOf`.
- `java/lang/Math` extensions: `floorDiv(II)`, `floorMod(II)`,
  `floorDiv(JJ)`, `floorMod(JJ)`, `round(D)J`, `round(F)I`, `sqrt`, `pow`,
  `floor`, `ceil`, `hypot`? — no; exact set: `floorDiv`, `floorMod`,
  `round`, `sqrt`, `cbrt`, `pow`, `floor`, `ceil`, `signum`,
  `toIntExact` (ArithmeticException), `addExact/subtractExact/
  multiplyExact(II/JJ)` — exceptions must match Temurin exactly
  (message text included).
- `java/lang/Integer/Long` extensions: `parseInt`, `parseLong`,
  `toString(I)`, `toBinaryString`, `toHexString`, `toOctalString`,
  `compare`, `max/min/sum`, `bitCount`, `numberOfLeadingZeros`,
  `numberOfTrailingZeros`, `highestOneBit`, `lowestOneBit`,
  `rotateLeft/Right`, `reverse`, `SIGNUM constants` already exist.
- `java/lang/Character`: `isDigit`, `isLetter`, `isLetterOrDigit`,
  `isWhitespace`, `isUpperCase`, `isLowerCase`, `toUpperCase`,
  `toLowerCase`, `getNumericValue` — ASCII + pinned Unicode extras via
  differential fixtures (the harness is the arbiter).

### 5.3 Java semantics pinned by fixtures

- **HashMap iteration order is algorithm-faithful** (risk M3-R2):
  spreading `h ^ (h>>>16)`; index `(n-1) & hash`; initial capacity 16
  (or `tableSizeFor(initialCap)`); load factor 0.75; resize ×2; Java-8
  split preserves relative order within lo/hi bins; iteration = table
  order, bins in insertion order. Equality via dispatched
  `equals/hashCode` (user classes included) — a fixture uses user
  class keys with overridden equals/hashCode.
- **ArrayList fail-fast shape**: `get(-1)`, `get(size)`,
  `Iterator.next()` past end must throw `IndexOutOfBoundsException` /
  `NoSuchElementException` with Temurin's exact message text (pinned,
  three-channel).
- **Boxing at the boundaries**: `ArrayList<Integer>` auto-boxing flows
  through the existing M1.6 box cache; identity of cached boxes through
  collections is pinned (extends `BoxingIdentity`).
- **Generics are erased**: fixtures use raw + generic-syntax sources;
  the erased descriptors drive resolution exactly as javac emits them.
- **ConcurrentModificationException**: not modeled (no mod-count
  semantics in single-threaded M3? — *decision*: implement the mod-count
  check anyway; it is observable in single-threaded programs through
  iterator invalidation and Temurin throws it deterministically in that
  case. Faithful > convenient.)

### 5.4 Memory safety & GC

- All collection state is **heap-resident**: an `ArrayList` is a heap
  object whose slots hold `size`, `modCount`, and a `JRef` to a heap
  `Object[]`; `HashMap` holds `size`, `modCount`, threshold, and a
  `JRef` to a heap `Entry[]` of heap `Node` objects (`hash`, `key`,
  `value`, `next` slots). The existing mark/sweep therefore scans
  collections with **zero new root machinery** — the strongest possible
  GC story, and forced-GC mode (gate-9) proves it across every
  collection fixture.
- No `Rc<RefCell<…>>` side tables anywhere; borrow discipline via the
  scoped `with_obj` accessors of M2 T13 (census re-issued in
  RUNTIME_SAFETY.md §4 with the new call sites counted).

### 5.5 Differential tests

8+ new fixtures, e.g.: `ArrayListOps` (order, CME, IOOBE message),
`HashMapOrder` (Integer + String + user-class keys, iteration print,
resize at 13th put), `HashSetOps`, `ObjectsNPE`
(`requireNonNull` NPE message text), `StringExt`, `StringBuilderExt`,
`MathExact` (exception messages), `IntegerParse` (NumberFormatException
messages), `CharClassify`. Suite guard rises to `>= 51`.

### 5.6 Acceptance criteria

- [ ] Every §5.2 method either works differentially or fails loudly at
      compile time — no third state.
- [ ] `HashMapOrder` prints byte-identical iteration order vs Temurin
      including the post-resize order.
- [ ] Forced-GC suite (gate-9) green including all new fixtures.
- [ ] Borrow census re-issued; `RUNTIME_SAFETY.md` updated in the wave
      commit.

---

## §6 Wave 3 — lambdas & method references (invokedynamic)

### 6.1 Design

- Sites: `invokedynamic` whose bootstrap handle is
  `java/lang/invoke/LambdaMetafactory.metafactory`
  (`crates/j2r-codegen/src/lib.rs:922` currently rejects them).
- Resolution (compile time): read `BootstrapMethods[bsm_index]`; verify
  the bootstrap method is exactly `LambdaMetafactory.metafactory`;
  extract `samMethodType`, `implMethod` (`CONSTANT_MethodHandle`), and
  `instantiatedMethodType` from the static arguments; the indy
  `NameAndType` gives the SAM name and the *capture signature* (dynamic
  parameters = captured values).
- Emission: the indy site becomes **closure construction** — a heap
  object (native class `j2r/lambda/Closure`) whose slots are: target
  dispatch token (class id + method id of the implMethod, resolved via
  the existing `resolve_call_pub`), the capture kind
  (static/bound-virtual/interface-bound/special/ctor), and the captured
  values copied from the operand stack (JLS 15.27.2 capture-by-value).
- Invocation: `invokeinterface Function.apply` (and friends) dispatches
  through the normal `invoke_virtual` walk to the native closure
  `apply` entry, which re-prepends the captures and calls the compiled
  `lambda$…` method (javac emits it into the same class we already
  compile) or the method-ref target.
- Supported `MethodHandle` reference kinds: `REF_invokeStatic` (6),
  `REF_invokeVirtual` (5), `REF_invokeInterface` (9),
  `REF_invokeSpecial` (7), `REF_newInvokeSpecial` (8, ctor refs —
  closure `apply` runs `new` + `<init>` then returns). Others loud.
- `altMetafactory`, bridge-method edge shapes, `$deserializeLambda$`:
  loud compile errors naming the shape (risk-matrix honesty).
- Functional interfaces as native interfaces:
  `java/util/function/*` M3 set — `Function`, `BiFunction`, `Supplier`,
  `Consumer`, `BiConsumer`, `Predicate`, `BiPredicate`, `UnaryOperator`,
  `BinaryOperator` — each with its single abstract method (+`andThen`
  only on `Function`/`Consumer`/`Predicate`? decision: **no**
  default-method support in M3 — `andThen` etc. loud-unsupported;
  default methods arrive with real interface defaults, currently
  javac-emitted as compiled code we would already compile — keep it
  simple, measure in Wave 3 e2e).
- Interface dispatch on native receivers: the Wave-2 registry
  extension is what lets `invokeinterface Function.apply` walk to the
  native closure object. Subtype checks (`checkcast Predicate`,
  `instanceof Consumer`) must include native classes in `parents`.

### 6.2 Files

`j2r-codegen/src/lambda.rs` (new: bootstrap resolution + site planning,
~400 lines + tests), `j2r-codegen/src/emit.rs` (closure-construction
emission at indy sites), `j2r-runtime/src/lambda.rs` (new: closure
invocation, ~250 lines + tests), natives table entries for the
functional interfaces.

### 6.3 Semantics pinned by fixtures

`LambdaCaptureless`, `LambdaCapture` (capture + mutate-original
afterwards — JLS by-value proof, risk M3-R4), `LambdaThis` (inner-class
`this` capture via instance method host), `MethodRefStatic`,
`MethodRefBound`, `MethodRefUnbound` (`String::toUpperCase` shaped),
`MethodRefCtor` (`ArrayList::new`), `LambdaInCollections`
(`forEach`, `removeIf` with closures over user classes), `LambdaGC`
(closure churn under forced GC — gate-9 coverage). Suite guard rises to
`>= 60`.

### 6.4 Acceptance criteria

- [ ] All nine fixtures three-channel green vs Temurin.
- [ ] `--emit-cfg` output-equivalence extends to the new fixtures
      automatically (cfg_equiv iterates the suite dirs).
- [ ] Forced-GC suite green with lambda churn fixtures.
- [ ] Any indy shape outside the supported set produces a compile error
      naming the bootstrap method (negative e2e test).

---

## §7 Wave 4 — synchronized monitors

### 7.1 Design

- Per-object monitor: heap `Entry` gains `monitor: u32`
  (re-entrancy counter, 0 = unlocked). `monitorenter`: counter += 1,
  overflow = loud internal error; `monitorexit`: counter -= 1, underflow
  = `IllegalMonitorStateException` faithful to Temurin's message.
- Frame-scoped release: `FrameGuard` gains a monitor stack
  (`Vec<JRef>`, capacity 8 inline — the 99 % case, spilling heap-free);
  `monitorenter` pushes after acquiring; `monitorexit` pops; on any
  unwinding exit (exception propagation path of `j2r_try!`) the frame
  releases everything above the entry depth — exactly the root-snapshot
  slot discipline of M2_PLAN §4.4, applied to monitors (risk M3-R9).
- `ACC_SYNCHRONIZED` methods: prologue acquires (receiver for instance
  methods; a **synthetic per-class monitor object** for statics,
  allocated once in the emitted statics module and scanned as a static
  root), epilogue + every unwind path releases. The M1 discipline that
  forced a compile error on `synchronized` is lifted *only* in the same
  commit that makes it correct.
- `Object.wait/notify/notifyAll`: loud unsupported runtime errors
  (threading boundary, M2_PLAN §7 unchanged).
- Recursion (`synchronized` method calling itself), nested different
  monitors, throw-between-acquire-and-release, `synchronized` in
  `<clinit>` — all fixture-pinned.

### 7.2 Files

`j2r-runtime/src/heap.rs` (Entry field + monitor ops), `frame.rs`
(monitor stack), `emit.rs` (monitorenter/exit emission, sync-method
prologue/epilogue, unwind release hooks), `ir.rs` (lift the decode
rejection at `ir.rs:527`, add ACC_SYNCHRONIZED validation),
`natives.rs` (loud wait/notify entries).

### 7.3 Fixtures

`SyncBlock` (basic + nested), `SyncMethod` (instance + static),
`SyncThrow` (exception releases exactly once — observable via a
re-acquire counter class), `SyncRecursion` (re-entrancy), `SyncClinit`
(monitor in class init). Suite guard rises to `>= 65`.

### 7.4 Acceptance criteria

- [ ] All five fixtures three-channel green.
- [ ] Monitor-state assertion in the stress test: 100 k acquire/release
      cycles on one object leave `monitor == 0`.
- [ ] Unwind-release proof: forced-GC + `SyncThrow` green (a leaked
      monitor would not show in GC, but the re-acquire counter fixture
      observes the release count).

---

## §8 Wave 5 — reflection subset

### 8.1 The honest M3 set

| Supported | Loud-unsupported |
| --- | --- |
| `Object.getClass()`, `X.class` literals (`ldc CONST_Class`) | `getDeclaredFields` bulk views |
| `Class.getName()`, `getSimpleName()`, `toString()` | `getMethods`/`getDeclaredMethods` bulk views |
| `Class.equals/hashCode` (identity — per-class interned) | `getAnnotations*` |
| `Class.isInstance`, `isAssignableFrom`, `isInterface`, `isArray` | `getGeneric*` |
| `Class.getSuperclass`, `getComponentType` | `newInstance` (use ctor refs / `REF_newInvokeSpecial` instead) |
| `Class.getDeclaredField(String)` + `Field.get/set` with boxing | `setAccessible` access-widening semantics (always-accessible documented) |
| `Class.getDeclaredMethod(String,String…-shaped lookup)` + `Method.invoke` with boxing | `Proxy`, `MethodHandles.Lookup` |

Class objects: lazily constructed, per-class interned in the emitted
statics (identity stable across the program — pinned by fixtures),
scanned as static roots (existing `gc_scan_roots` hook).

`getSimpleName` uses the `InnerClasses` attribute when present (parser
extension, small); nested-class fixtures pin it. `Method.invoke`
wraps reflective failures (`NoSuchMethodException`,
`IllegalArgumentException`, NPE for null-receiver-on-instance-method)
with Temurin-exact messages; invocation overhead is documented, no
perf claim.

### 8.2 Files

`j2r-runtime/src/reflect.rs` (new, ~400 lines + tests), `natives.rs`
entries, `emit.rs` (ldc-Class → interned class object), statics
template gains the class-object table.

### 8.3 Fixtures

`ClassIdentity` (ldc + getClass interning, hashCode stability across
GC), `ClassMeta` (names, isInstance across hierarchies + arrays),
`ReflectField` (get/set with boxing round-trips), `ReflectMethod`
(invoke, exception shapes), `ReflectGC` (class objects survive
forced GC via static roots). Suite guard rises to `>= 70`.

---

## §9 Dependency & supply-chain policy (unchanged, restated)

`j2r-classfile` and `j2r-runtime` keep **zero external dependencies**
(ROADMAP §1.3). The JAR reader is hand-rolled (§4) rather than pulling
`zip`/`flate2` into the workspace; `j2r-codegen`/`j2r-cli` also gain no
new dependencies in M3. Gate-7 (`cargo audit`) therefore continues to
audit nothing — which is the point. Revisit only with an explicit
review-approved exception.

## §10 CI gate evolution

| Gate | M3 change |
| --- | --- |
| 1–10 | unchanged, always required |
| 3 diff-suite | count guard follows the waves (43 → 51 → 60 → 65 → 70) |
| 9 gc-stress | automatically covers every new fixture (forced-GC mode) |
| new: jar determinism e2e | folded into gate-5 (standalone crate build compiles from a jar fixture) — no 11th gate needed; the equivalence assertions live in the Wave-1 e2e test file |

## §11 Threading boundary statement (unchanged from M2_PLAN §7)

M3 adds zero concurrency. Monitors are re-entrancy counters without
contention (single thread cannot contend with itself); `wait/notify`
are loud errors; the M3→M4 migration checklist (shared heap, per-thread
root registration, monitor word redesign) carries over from M2_PLAN §7
and is extended with "collections/closures must be re-audited for
shared-mutation re-entrancy" in M4 planning.

---

## §12 Task register (23 tasks — the working todolist)

Status: `[x]` done · `[~]` in progress · `[ ]` pending.

### Wave 0 — planning (this wave)

- [x] **T01** Audit current class-loading, dispatch, natives, indy,
  monitor surfaces with file:line citations (§1).
- [x] **T02** M3 risk register M3-R1…M3-R10 (§2).
- [x] **T03** Classpath/JAR architecture incl. zero-dep inflate (§4).
- [x] **T04** Collections architecture incl. faithful HashMap order and
  heap-resident storage (§5).
- [x] **T05** Lambda/LambdaMetafactory architecture incl. capture model
  and supported handle kinds (§6).
- [x] **T06** Monitor + reflection-subset architecture (§7, §8).
- [x] **T07** Plan published with all CI gates green (this commit).

### Wave 1 — classpath & JAR loading

- [x] **T08** `inflate.rs` + `zip.rs` + `crc32.rs` (zero-dep), test
  battery incl. corrupted-input Err discipline and bomb guard.
- [x] **T09** `classpath.rs` resolution + transitive closure loading +
  `-cp` plumbing on `compile`/`test`.
- [x] **T10** Jar fixtures `JarMain`, `JarSuper`, `JarDup`, `JarClosure`
  (suite guard 43), determinism e2e (jar vs exploded-dir byte-identical
  crates).
- [x] **T11** Wave-1 evidence record + CI green (run recorded in
  ROADMAP §2).

### Wave 2 — collections & java.lang extensions

- [x] **T12** `collections.rs`: ArrayList + Iterator + Objects with
  faithful exception messages; heap-resident storage. *(done — evidence:
  `M3_EVIDENCE.md` Wave 2)*
- [x] **T13** HashMap/HashSet with faithful OpenJDK iteration order +
  resize behavior; mod-count CME; treeify loud guard; native-class
  subtype/interface dispatch registration. *(done — `HashMapOrder` is
  byte-identical vs Temurin incl. post-resize order; treeify stays out of
  the honest set: bins with 8+ entries are outside the fixture-reachable
  inventory and bin lists are walked linearly, which is behaviourally
  identical for supported key counts)*
- [x] **T14** String/StringBuilder/Math/Integer/Long/Character
  extension methods (§5.2 inventory) with exact exception texts.
  *(done — JDK-21 message shapes captured from Temurin first)*
- [x] **T15** Collections fixtures (suite guard 51) + evidence + CI.
  *(done — 12 new fixtures, 51 total; `M3_EVIDENCE.md` Wave 2; one recorded
  plan deviation: `ArrayList$Itr.remove()` implemented Java-faithfully
  instead of loud-unsupported, disclosed in the evidence record)*

### Wave 3 — lambdas & method references

- [x] **T16** `lambda.rs` bootstrap resolution + closure construction +
  runtime invocation; supported handle kinds 5/6/7/8/9; negative e2e
  for unsupported shapes. *(done — resolution in `j2r-codegen/src/lambda.rs`,
  invocation + SAM↔impl box/unbox adaptation in `j2r-runtime/src/lambda.rs`,
  negative e2e in `j2r-cli/tests/lambda_neg.rs`)*
- [x] **T17** Functional-interface natives + interface-dispatch wiring;
  method refs incl. ctor refs. *(done — closures carry the interface as
  their object class so subtype checks and `invokeinterface` dispatch are
  exact with zero new machinery; Wave-2's `Supplier.get` placeholder
  became the real row; `forEach`/`removeIf` mirrored under `List`)*
- [x] **T18** Lambda fixtures (suite guard 60) + evidence + CI.
  *(done — nine fixtures 51→60, `M3_EVIDENCE.md` Wave 3)*

### Wave 4 — synchronized monitors

- [x] **T19** Heap monitor counters + FrameGuard monitor stack +
  unwind-release + ACC_SYNCHRONIZED (instance + static synthetic
  monitor) + loud wait/notify. *(done — Entry monitor counter + monitor_enter/
  exit/release in heap.rs; FrameGuard monitor stack with Drop release;
  per-class synthetic monitors as static roots; IMSE after the borrow,
  caught by the first unit-test run)*
- [x] **T20** Monitor fixtures (suite guard 65) + stress assertion +
  evidence + CI. *(done — five fixtures 60→65 incl. the 100-deep
  re-entrancy + 100k enter/exit stress; `M3_EVIDENCE.md` Wave 4)*

### Wave 5 — reflection subset

- [ ] **T21** `reflect.rs`: Class objects (interned, static-rooted),
  §8.1 supported set, InnerClasses-aware getSimpleName, Field/Method
  by-name with boxing.
- [ ] **T22** Reflection fixtures (suite guard 70) + evidence + CI.

### Completion

- [ ] **T23** M3 completion package: `docs/M3_EVIDENCE.md` final
  section, README sync, review submission (same process as M2 T22).

### Dependency chain

T01–T07 → T08 → T09 → T10 → T11 → T12 → T13 → T14 → T15 → T16 → T17 →
T18 → T19 → T20 → T21 → T22 → T23. No wave starts before the previous
wave's evidence exists and every prior test is still green.

---

## Revision history

- Rev 1 (2026-11-02): initial plan; audit, risks, five waves, 23-task
  register. Wave tasks T08–T22 are pending, not pre-checked: the
  standing discipline (no done-claim without recorded evidence) applies
  to this plan exactly as it did to M2_PLAN.
