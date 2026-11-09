# Analysis: the v1.2 Minicraft test of the external source-level converter

Date: 2026-11-09. Input: the owner's pasted `results_v1.2` / `bugs_v1.2`
(M01–M20) / `opt_v1.2` — a test of the **source-level** converter
`cguz/java2rust` (CLI packaging of aschoerk's converter) on Minicraft
(Notch's Ludum Dare 22 entry, `com.mojang.ld22`, 76 files / 5,535 lines),
with a hand-repaired working port used for the Java-vs-Rust benchmark.

This document answers: what does this result mean for **our** j2r (the
bytecode-level compiler at github.com/WasewaseX/j2r)? It is written from
zero, per the standing report law: every term defined, every claim either
cited from the owner's paste or mapped to our recorded evidence.

## 1. What was tested, precisely

- **The converter under test translates Java *source*** (`java -jar
  java2rust.jar -d <dir>`): it parses `.java` text and prints Rust text.
- **Our j2r consumes Java *bytecode***: `javac` first, then j2r reads
  verified `.class` files, runs its own abstract-interpreter verifier, and
  emits Rust from the lowered IR. Anything javac accepted is already in a
  normal form (explicit casts, desugared constructs, typed constant pool).
- The v1.2 run: 76 files → 76 emitted Rust files / 7,133 lines →
  **1,438 raw rustc errors, zero files compile** (E0425 ×797, E0433 ×320,
  E0423 ×42, E0308 ×37, E0592 ×12, E0061 ×8, E0618 ×4 + parse damage).
  Error classes M01–M20 in the owner's catalog; four defect classes are
  *silent* (compile-then-wrong or compile-then-empty), which the catalog
  itself ranks as the worst: M13 (static initializer blocks dropped),
  M09 (java.util.Random unmapped), M10 (transcendental math unmapped with
  an ulp trap), M02 (inheritance erased).
- The repaired working port then proved **bit-exact parity** with the JVM
  (64-bit FNV-style state checksums, intermediate milestones, tick-by-tick
  per-entity hashes) and measured **2.8–8.7× faster, 8.8× less peak RSS**
  (73.5 MB → 8.4 MB) — on the same box class as ours (2 vCPU, ~3.9 GB).

## 2. Which catalog bugs cannot reproduce in our j2r — by construction

Our pipeline starts *after* the Java frontend. That makes the entire
"Java syntax emitted verbatim into Rust" family structurally impossible:

| Owner's bug | Why our pipeline cannot emit it |
| --- | --- |
| M01 `let mut x: i32,` struct fields | we never see field *declarations as text*; fields come from the classfile's field table with descriptors |
| M04 `let .lvl = lvl;` assignments | assignments are lowered from bytecode store instructions into typed IR, then rendered as real Rust statements |
| M07 `instanceof` verbatim | `instanceof`/`checkcast` are instructions we lower to runtime subtype checks (`subtype.rs`); pinned by the `Instanceof` fixture |
| M08 `null` verbatim | null is a typed IR value; reference locals are verifier-typed (the M1.5 verifier) and null checks lower to runtime calls |
| M15 overloads as duplicate `fn` names | overloads are distinct methods in bytecode (name + descriptor); each gets its own mangled symbol |
| M16 labeled-loop/match try/catch hybrid | exceptions are exception tables + our CFG lifter (M2 wave 4); pinned by Exceptions/FinallyOrder/LabeledLoop fixtures |
| M18 octal literals `050` | javac already decoded them into integer constants in the constant pool; we never see source tokens |
| M19 wildcards/unchecked casts in signatures | generics are erased by javac before we run; we see erased descriptors |
| M12 anonymous inner classes as impossible constructors | anonymous classes are separate `.class` files; we compile them like any class (the corpus programs and fixtures contain them) |
| M06 `#[derive(Runnable)]` for interfaces | interfaces arrive as classfile constructs; our emitter produces real traits + impls (Interfaces fixture) |
| M03 bare `super(...)` calls | constructors are `<init>` methods; the verifier enforces super-chain initialization (verified bytecode, ZeroInit fixture) |
| M20 `-o` silently ignored | our CLI is clap-based, all flags effective; flag flips invalidate the manifest gate (T06/T07 test: option change ⇒ no reuse) |
| M11 JDK API surface bare + E0433 ×320 | same-package/JDK types are resolved from the classpath closure (T03 cache); JDK natives route through `j2r-runtime` (M1/M3) |

Summary: **~14 of the 20 catalog entries are source-parser defects our
architecture cannot produce.** This is not luck; it is the reason the
project consumes bytecode.

## 3. Which catalog classes are universal lessons — and where we already cover them

These are semantic, not syntactic; ANY translator must answer them:

| Universal defect | Our answer (recorded evidence) |
| --- | --- |
| M13 static initializer blocks silently dropped | `<clinit>` is a method we compile; JLS 12.4.2 initialization state machine in `j2r-runtime::clinit` (M4 wave-3 fix); SingletonFactory (incl. initialization-on-demand-holder laziness proof) and CyclicInit fixtures are byte-identical vs Temurin |
| M09 `java.util.Random` semantics | 48-bit LCG + exact `nextInt(bound)` rejection lives in `j2r-runtime` natives; `MathNatives`/`Runtimes` differential fixtures pin the streams |
| M10 fdlibm trig/log (the ulp trap) | StrictMath surface pinned by `MathNatives` against Temurin on all three channels; the corpus runs real math-heavy programs (Jama/linear algebra waves) byte-identically |
| M02 inheritance erased | member resolution runs over the whole `Program` (verifier-checked bytecode), so inherited members resolve globally; partitioned emission keeps resolution global while compilation splits (T04 partition differential incl. `Cycles` — the cyclic-web fixture) |
| M14 static mutable state | statics live in the id-keyed statics pool (the `support` crate under partitioning) — one pool, every partition links it |
| M17 GUI/AWT coupling passes through | our standing law: **unsupported fails loudly at compile time, never silently miscompiles**; the JNI/wave-2 boundary work is exactly the "name the boundary, stub it loudly, list it" discipline done at the native layer |
| Silent-bug class as the top risk | the differential three-channel battery (109 fixtures) + corpus + forced-GC + partition suites exist precisely to convert "compiles" into "provably the same program" |

## 4. What we genuinely adopt from this test

1. **Minicraft joins the corpus queue (realistic-shape benchmark).** The
   owner has demanded realistic workloads for scale claims; Minicraft is a
   real 76-class web with deep hierarchies (Entity→Mob→Player/Zombie/
   Slime/AirWizard, Item→…, Tile→22 subclasses, Menu→10 screens),
   instanceof-heavy dispatch, statics, string/format use, and a large
   static-init registry — plus the owner's checksum-harness methodology
   (FNV-style state mixing, milestone hashing) that we can reuse as the
   differential driver. Plan: strip/patch the AWT surface through the
   loud-unsupported boundary list, drive it headless (the owner's
   `Bench_v12.java` pattern), compare vs Temurin, and run it through
   `--partition-classes` at more than one K. This exercises exactly the
   shapes the synthetic farm only approximates.
2. **The GUI-boundary lesson (their M17) is our wave-2 JNI discipline.**
   Their repair needed an H-series shim list; ours is registered as the
   JNI bridge (T08–T13) with the loud-unsupported law. Minicraft is the
   small rehearsal; Minecraft/LWJGL is the real show.
3. **Performance expectations for M6:** their measured multipliers (2.8–8.7×
   CPU, 8.8× RSS on an allocation-heavy game loop) are consistent with our
   BENCHMARKS steady-state work (container tax 2.78× vs JVM 1.77×; lever at
   0.936 and moving). Their "smallest wins are JIT-friendly dense integer
   loops" note is the same reason our inline-cache wave (T22–T23) targets
   megamorphic dispatch, not arithmetic.
4. **opt_v1.2 §1/§6 confirm our order of operations.** Their top
   suggestion (two-pass symbol resolution before codegen) is our T03
   closure cache + whole-Program resolution — already landed; their "ship a
   java-semantics runtime crate" is our `j2r-runtime` (zero-dep, since M1).

## 5. Honest boundaries

- We have **zero AWT/GUI surface**; a Minicraft corpus run needs the
  boundary patch list before any differential claim. That work is scoped,
  not yet done — no compatibility claim is made here (standing law).
- The v1.2 performance table is the external tool's hand-repaired port,
  not our output; we may not quote its multipliers as ours.
- The silent-defect risk (their M09/M10/M13 class) is never "fixed" for
  us by architecture alone — only by the differential battery staying
  green on real programs. That is why Minicraft should actually run.

## 6. Decisions taken this session

1. Minicraft registered as corpus-wave candidate (§4.1) — next corpus wave
   after the current queue, or interleaved when a wave closes.
2. Console mechanism (console.md + console_summary.md) created per owner
   directive; the milestone-name correction (M5 = Minecraft readiness)
   recorded as mistake #1 on the never-repeat list.
3. The 20k census-jar confirmation (last open wave-1 item) launched in the
   background; results will be appended to console.md when they land.
