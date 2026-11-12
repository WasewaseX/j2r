# bug.md — every bug and blocker hit while porting jbox2d with j2r

Date: 2026-11-12 · Scope: the whole port attempt, in the order it happened.

Verdict summary: 2 real bugs in j2r (wrong behaviour, one silent), 1 silent-divergence trap in the runtime's design, 9 loud coverage gaps (compile errors — by j2r's own law these are correct behaviour, not bugs), 2 scale/robustness blockers, and 3 process mistakes of mine that cost real time and are listed so they are not repeated.

Legend: [BUG] = j2r was wrong and is fixed here · [GAP] = j2r correctly refused; runtime surface extended · [BLOCK] = j2r/rustc could not build; worked around, not fixed · [MINE] = my error.

Patch footprint: 9 files — 7 existing ones carrying Port wave 1 markers (j2r-runtime/src/{natives,builtin,langext,jdkutil,formatter}.rs, j2r-codegen/src/{lib,partition}.rs) plus the new j2r-runtime/src/fdlibm.rs (533 lines) wired in through lib.rs::pub mod fdlibm — about 1,100 added lines in total. j2r's own suites re-run after the fixes: runtime 243/243 green (test_runtime.log) and differential suite green — every fixture in tests/suite, plus the gate (test_diff_jdk21.log, 597 s).

On the first differential run: 5 fixtures failed and the cause was not j2r. BigJson, CharClassify, StringExt, StringIoSemantics, Utf16 all differed only in non-ASCII characters — the expected side had `?` where the actual side had `é中ß`. That is the fingerprint of a reference JVM writing with an ASCII default charset (any JDK before 18 with a C/POSIX locale; file.encoding became UTF-8 by default in JEP 400 / JDK 18). Re-run with LANG=C.UTF-8 and J2R_JAVAC pointed at Temurin 21: all five pass, and the whole suite passes. Worth knowing for anyone reproducing: j2r's differential harness compares raw bytes, so the reference channel's locale is part of the contract and is not asserted by the harness.

## B1 [BUG — silent, wrong output] String.format("%.f") destroys magnitudes < 1e-3

**Symptom.** The ported physics engine printed joint angles as 6.542073097 where the JVM printed -0.000654208. The simulation itself was byte-identical; only printing diverged. It made the whole pendulum workload look like a physics bug and cost hours of bisecting the solver (__j2r_find_handler, Sweep, field slots, GC rooting — all innocent).

**Root cause.** j2r-runtime/src/formatter.rs::render_f derived its digits from javafmt::double_to_string, the shortest round-trip repr. Java switches that repr to scientific notation for magnitudes < 1e-3 and >= 1e7, but render_f assumed fixed-point text and split on '.'. For 6.149717374e-4 the repr is 6.14972E-4, so it took "6" as the integer part, "14972E-4" as the fraction, and dropped the exponent. Every %f, %.Nf and %,f of such a value was wrong — and the radix point was gone, so the output also failed the "obvious nonsense" check.

**Why the existing fixtures missed it.** corpus/FmtResidualProbe.java and the Fmt fixtures pin %f on values like 3.5, PI, 1/3 — all inside [1e-3, 1e7), the only interval where repr and fixed-point agree. Nothing in the corpus formatted a value from the JVM's scientific-repr band.

**Fix (≈40 lines).** Normalise the shortest repr to pure digit strings before the precision rounding: split off the exponent, shift the decimal point, then run the existing HALF_UP rounding. Verified against Temurin on a 15-value probe covering 1e-4 … 1.2345678e7, %.9f/%.6f/%.3f/%,f/%e/%g/%s → byte-identical (probe/F01_FmtSmall).

Probe that now pins it:

```
input 6.149717374e-4   java "0.000614972"   j2r-before "6.149717374"   j2r-after "0.000614972"
input 1.0e-10          java "0.000000000"   j2r-before "1.0E-100000"   j2r-after "0.000000000"
input 9.99e-5          java "0.000099900"   j2r-before "9.99E-50000"   j2r-after "0.000099900"
```

## B2 [BUG — hard build failure] Partitioned emission wires exception handlers to a module that does not exist

**Symptom.** --partition-classes N on jbox2d emitted 213 files and then every partition crate failed to compile:

```
error[E0433]: cannot find `registry` in the crate root
  --> p2/src/classes/org_jbox2d_dynamics_World_1.rs:16:152
   | ... Option::Some(crate::registry::parents_of)) ...
```

It only fires for classes that carry an exception table — World$1.<clinit> (the JBox2DSettings NoSuchFieldError bridge) was the first one hit. A class without try/catch partitions fine, which is why j2r's own partition fixtures and the farm census never saw it.

**Root cause.** emit.rs emits the exception-matcher helper as `is_subtype(thrown_class, "<catch>", Some(crate::registry::parents_of))`. partition.rs::rewrite_class_source rewrites crate::statics:: → support::statics::, crate::j2r_dispatch() → support::dispatch_slot::dispatch(), and cross-partition crate::classes:: → link-table calls — but has no arm for crate::registry::. So the path survived verbatim into a partition crate, which has no registry module.

**Why the obvious fix is wrong.** Pointing it at the app crate (which owns the whole-program route table) is a cargo dependency cycle: app depends on p0..pN, so pN cannot depend on app. E0433: unresolved module or unlinked crate 'app' — observed.

**Fix (3 parts).**

1. rewrite_class_source gains the `crate::registry::` arm and rewrites it to `support::registry_hook::`.
2. A new generated `support/src/registry_hook.rs`: a OnceLock holding `fn(&str) -> Option<&'static [&'static str]>` for parents and the meta_of equivalent — the same process-global-slot pattern the crate already uses for dispatch_slot and the link table. A miss panics loudly ("registry hook not installed before use"), never returns a wrong subtype answer.
3. app/src/main.rs installs it in the existing startup sequence (`support::registry_hook::install(<lib>::registry::parents_of, <lib>::registry::meta_of)`), and the app crate now exposes its composite route table as `pub mod registry { … }` (three forwarding fns over part-by-part route tables; the module needs super::part_of/super::ROUTE_* paths, which was two more compile errors on the way).

**Consequence if it had not been fixed:** no multi-class program with a try/catch could be emitted in partitioned mode, i.e. exactly the 70k-class Minecraft path (M5) that partitioning exists for.

## B3 [BUG class — silent divergence trap] Math.sin/cos/tan/atan are not the platform libm

**Symptom.** Nothing fails. That is the problem.

**Measurement.** fdlibm (what Java specifies and StrictMath implements) vs the platform libm Rust's f64::sin calls, on 200,000 doubles from a deterministic LCG:

| function | platform-libm vs JVM | fdlibm-C vs JVM | j2r fdlibm-Rust vs JVM |
|---|---|---|---|
| sin | 27 mismatches | 4 | 4 |
| cos | 15 | 9 | 9 |
| tan | 29 | 0 | 0 |
| atan | 48 | 0 | 0 |

So a port that maps Math.sin → f64::sin() silently differs from the JVM on ~1 input in 10,000. For jbox2d that is fatal in principle: any divergence in Rot.mul/Vec2 math changes contact geometry, and a solver amplifies it within a few hundred steps — which is exactly why the port's byte-identical result matters.

**Why j2r had no sin at all.** Its Math surface was abs/max/min/round/sqrt/cbrt/pow/floor/ceil/signum/random/floorDiv/floorMod/…Exact — deliberately the set its corpus needed. sin/cos/tan/atan/atan2 were absent, so the call was a loud compile error ("cannot resolve method sin(D)D"), which is j2r's stated law working as designed. Good law; the surface was just too small for a game.

**Fix.** New module j2r-runtime/src/fdlibm.rs (533 lines): a faithful transliteration of the fdlibm kernels as musl carries them (__rem_pio2, __sin, __cos, __tan, atan, atan2), registered for both Math and StrictMath. Arithmetic is plain f64 (no FMA contraction), matching the C compiled without -ffp-contract=fast.

**Honest limitation, stated in the module doc:** the Rust kernels are bit-identical to the C fdlibm reference (0 mismatches on 200k inputs, all four functions), but the JVM itself differs from fdlibm on 4 sin and 9 cos inputs out of 200k. Temurin's JVM is not pure fdlibm on every input — it agrees with the platform libm more often than with fdlibm on sin. Closing that last 6.5-input-per-100k gap needs whatever Temurin's StrictMath actually links; it is not closed here and is listed as the top item in Opt.md §5. Consequence: an exhaustive ulp-parity fixture for Math.sin would still find ~1 in 20,000 inputs differing. The port's own workloads did not hit one (verified byte-identical on 1e6 evaluations).

__rem_pio2_large (Payne–Hanek, needed only for |x| >= 2^20·π/2) is not implemented: the code panics with a named message rather than returning a plausible wrong value.

## B4–B12 [GAP] Nine runtime surfaces jbox2d needs that j2r did not have

All were loud compile errors, found one per compile in a fixed order. j2r's behaviour here is correct by its own contract; extending the surface is the work.

| # | error j2r raised | what was missing | fix |
|---|---|---|---|
| B4 | verification error in org/jbox2d/dynamics/World$1.<clinit> at pc 9: exception catch type java/lang/NoSuchFieldError does not name a known class | the NoSuchFieldError class itself is not in the builtin hierarchy, so the verifier rejects any class that catches it (javac emits this catch for JLS 13.4.9 field-resolution bridges) | builtin.rs: NoSuchFieldError → IncompatibleClassChangeError (JDK parentage). One row, unblocked the whole load |
| B5 | error in class java/lang/StrictMath: cannot resolve method sin(D)D | StrictMath is a separate class in the JDK; j2r had no hierarchy row and no natives for it | hierarchy row + 35 native rows sharing the Math implementations |
| B6 | unsupported feature in World.<clinit> at pc 17: instantiation of JDK class java/lang/Integer is not supported | `new Integer(int)` — the Java-9-deprecated-but-alive boxed constructors. j2r had valueOf only, and boxed classes were not on the instantiation allow-list | builtin_is_instantiable accepts the 8 boxed classes; `<init>` rows are aliases of valueOf, so the JDK identity cache applies (new Integer(127) == new Integer(127) is true on HotSpot too) |
| B7 | error in class java/lang/AssertionError: cannot resolve method `<init>`(Ljava/lang/Object;)V | AssertionError(Object) | `<init>` rows mapped to the existing Throwable message-slot machinery |
| B8 | error in class java/lang/Float: cannot resolve method intBitsToFloat… | the four IEEE-754 bit-reinterpretation statics (Float.intBitsToFloat, floatToIntBits, floatToRawIntBits, Double.longBitsToDouble). The functions existed in j2r; the table rows were missing | four rows; floatToIntBits canonicalises NaN (0x7fc00000) while floatToRawIntBits does not — that distinction is why Float.equals treats NaN as equal |
| B9 | cannot resolve method hypot(DD)D / error in class java/lang/Math: sin | transcendental surface (see B3) | fdlibm.rs + 40 rows for Math and StrictMath |
| B10 | unsupported feature …: instantiation of JDK class java/util/Random is not supported | java.util.Random — a real object with per-instance state, not a static facade | new builtin class (seed in own slot 0, gaussian cache in slots 1–2) + 12 natives reproducing the JDK's 48-bit LCG, power-of-two/rejection nextInt(bound), polar nextGaussian, nextBytes, and the exact IllegalArgumentException("bound must be positive") |
| B11 | virtual call target toArray()[Ljava/lang/Object; not found on java/util/ArrayList | ArrayList.toArray() and toArray(T[]) — including the JDK's two-branch contract (a.length >= size → fill and return a, write the null terminator; else allocate the component-typed array) | two natives in jdkutil.rs |
| B12 | cannot resolve method sort([Ljava/lang/Object;II)V | Arrays.sort(T[], from, to[, Comparator]) — the range forms, with the JDK's exact IllegalArgumentException("fromIndex(5) > toIndex(2)") / ArrayIndexOutOfBoundsException messages; plus Class.desiredAssertionStatus() (assertions are not a j2r feature, so the JDK default false is the honest answer) | three natives reusing the existing stable-merge comparator path |

**A note on how these were found.** Each compile reports only the first unresolved reference, so mapping the surface took one compile per gap (~10 s each because translation is 150 ms but the closure validation walks all 156 classes). The systematic way was the API probe battery (probe/, 25 probes) — see Opt.md §6.

## B13 [BLOCK] The generated crate cannot be compiled as a single crate on a 2 GB box

**Symptom.** cargo build --release of the un-partitioned jbox2d crate:

```
error: could not compile `j2r_app` (lib)
Caused by: … rustc … --crate-type lib … (signal: 9, SIGKILL: kill)
```

after 2 m 03 s.

**Root cause.** j2r's own measured wall (docs/BENCHMARKS.md, G2.6/M5_SCALE): rustc peak RSS is ~4.2 MB per emitted class and the failure is in the pre-codegen MIR/borrow-check phase, not LLVM. 156 classes × the per-class cost plus the 500 MB process baseline exceeds this box's 2 GB. This is not new information — j2r documents a 1,000-class SIGKILL at 3.66 GB on a 4 GB box — but it means a 280 KB Java library is already past the single-crate limit on a small machine.

**Workaround used.** --partition-classes 20 (8 crates) plus -j1: with -j2 two rustc processes race and one is still SIGKILLed; -j1 builds in 4 m 14 s, -j2 in 1 m 52 s once the page cache is warm. This is the feature (M5 T05 link-table partitioning) doing exactly what it was designed to do — it is listed as a blocker because the default emission path is unusable here, and because the failure mode is an OOM kill with no diagnostic pointing at class count.

## B14 [BLOCK] --partition-classes had never been exercised on a class with an exception table

B2 is the bug; this is the process finding behind it. j2r's partition tests, the synthetic farm census (100–20,000 classes) and its own corpus all pass. jbox2d is the first partitioned input with a catch in a `<clinit>` — and it failed instantly. A single synthetic fixture (one class, one exception table, --partition-classes 2) would have caught it. Recorded in Opt.md §6.

## B15 [MINE] Five debugging mistakes that each cost a build cycle

Listed because they are predictable and each one wasted ~4 minutes of cargo build on this box:

1. **Blamed the physics before checking the printer.** The pendulum divergence looked like solver/frames/Sweep corruption. The actual fault was String.format (B1). Lesson: when a divergence is confined to printed values and the program's own control flow is unchanged, diff the formatting layer first — it is cheap to probe and has a small, enumerable input space.
2. **Assumed partitioned-only.** I "confirmed" the bug in the --partition-classes 20 build and then had to re-run it single-crate to find out it reproduced there too. Lesson: reproduce on the default emission path once, before theorising.
3. **Passed only `<Main>`.class to j2r.** Nested/inner classes (P07_Class$P, R implements AutoCloseable) were not on the command line, so j2r reported "instantiation of JDK class P07_Class$P is not supported" — a misleading message produced by my invocation, not by j2r. The classpath was fine; the inputs list was not. Fixed by passing cls/*.class with --main-class.
4. **Used the host java (JDK 11) as the reference channel while j2r targets 21**, and briefly recorded a UnsupportedClassVersionError as a differential mismatch. Temurin 21 must be the channel (j2r's CI pins exactly that).
5. **Built with the default -j on a 2 GB box and /tmp on a 992 MB tmpfs.** Two concurrent rustc runs filled /tmp and one link died with `ld: Bus error`, which I first read as a toolchain bug. Lesson: on this box, -j1, and put CARGO_TARGET_DIR on the big filesystem.

## B16 [MINE/ENV] target/ and ~/.cargo do not survive the workspace snapshot

The sandbox restores /home/user from a snapshot that excludes target/, .cargo, .local, build, dist, out. After the first session the Rust toolchain, all cargo build trees and the JDK (tools/jdk-21… lost its lib/) were gone, and three later tool calls failed with "No such file or directory" rather than a clear cause. Fixed by writing setup_env.sh (idempotent: installs rustup, re-downloads the JDK, rebuilds j2r) and sourcing it at the top of every command. The j2r source patches are safe (plain files under crates/), the built artefacts are not — artifacts/ holds the binaries that must survive.

## B17 · Not fixed / open (carried into Opt.md §5)

| # | item | why it stays open |
|---|---|---|
| O1 | Math.sin/cos vs Temurin disagree on ~6 inputs per 100k (fdlibm-vs-HotSpot, B3) | needs the JVM's actual reduction/kernel tables, not fdlibm's; no fixture pins it yet |
| O2 | __rem_pio2_large (Payne–Hanek) absent; |x| >= 2^20·π/2 panics | rare (needs |x| ≥ ~3.3M); loud, named panic, never a wrong value |
| O3 | single-crate rustc OOM (B13) is a runtime-scale problem with no diagnostic | fix is codegen- or toolchain-side, out of a port's reach |
| O4 | no regression fixture exists for B1's [1e-9, 1e-3) magnitude band | should be added to corpus/FmtResidualProbe.java |
| O5 | no partition fixture with an exception table (B2/B14) | one 20-line fixture would cover the whole class of bug |
| O6 | the differential harness assumes the reference JVM's default charset is UTF-8; on a C locale it reports 5 phantom failures | should set -Dfile.encoding=UTF-8 (or pass -J-Dfile.encoding) itself instead of inheriting the ambient locale |
