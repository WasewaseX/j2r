# Real-World Corpus Wave

Standing directive: differential-test j2r against **real-world Java programs, not
synthetic fixtures**. This document registers the corpus, the mechanical assembly
pipeline, the golden outputs, and the gap catalog that falls out of each round. The
synthetic suite (83 fixtures, guard ≥ 82) stays the regression spine; the corpus is the
reality check.

---

## Corpus entry 1 — minimal-json (ralfstx/minimal-json, Apache-2.0)

A real production JSON library: 15 source files, ~120 KB of Java, heavy in exactly the
shapes synthetic fixtures underuse — inheritance hierarchies, interfaces with default
methods, generic abstract handlers (`JsonHandler<A, O>`), static singleton factories,
string escaping, char-level parsing.

* **Sources**: fetched from `ralfstx/minimal-json` master, stored verbatim (with their
  license headers) in `corpus/minimal-json/`.
* **Assembly**: `scripts/build_corpus_minimaljson.sh` builds
  `corpus/MinimalJsonCorpus.java` — one multi-class file with a deterministic public
  driver (`MinimalJsonCorpus`) plus all library classes demoted to package-private.
  Mechanical transforms, all documented here:
  1. `package com.eclipsesource.json;` stripped (default package, matching the suite's
     fixture model).
  2. Top-level `public` demoted; the driver stays public as the A3 source-stem main.
  3. `import` statements hoisted and de-duplicated ahead of all type declarations
     (Java requires imports before the first type).
  4. `implements Iterable<Member>` qualified to `Iterable<JsonObject.Member>` — the
     original file resolved the nested type through a self-import that cannot survive
     the default-package transform.
* **Golden output**: captured from Temurin 21.0.13, exit 0, fully deterministic
  (including the library's own pretty-print quirk on empty objects — real-world
  behavior is exactly what must survive differential testing).

## Gap catalog

### F1 — JDK superclass coverage in the builtin table — FIXED (wave 1)

`WritingBuffer extends java.io.Writer` was rejected at load: "unknown superclass
java/io/Writer: add its .class file to the inputs". JDK types are not inputs — they are
the runtime's job. `crates/j2r-runtime/src/builtin.rs` now knows the java.io
writer/reader hierarchy: `Writer`/`Reader` (with the `lock` slot),
`StringWriter`/`StringReader` (payload slot on top), and the interfaces
`Appendable`/`Closeable`/`Flushable`/`Readable`. Pinned by the
`corpus_wave1_io_hierarchy` unit test.

### F2 — verifier rejects null for array-typed slots — FIXED (wave 1)

`PrettyPrint.singleLine()` — the plainest static singleton factory in Java — failed
j2r's verifier: `expected Array(IntGroup) on the stack, found Null`. Root cause
(corrected from the original "static-slot type model" hypothesis): the verifier's
`pop_exact` accepted `Null` only for `want == Ref`, not for typed arrays — javac
emits `aconst_null; putstatic F:[C` for `static char[] NONE = null` in `<clinit>`,
and the same polymorphism applies at call sites (array-typed parameters, e.g.
`foo((int[]) null)`). Fix: `pop_exact` now accepts `Null` for every reference
shape (`Ref` and typed arrays) while keeping exactness everywhere else (an int
still fails a `[C` slot). Pinned by four verifier unit tests plus the
`StaticNullArray` differential fixture.

### F3 — java.io + collections behavior surface — FIXED (wave 1)

The driver's paths surfaced the full named list; every entry landed with JDK-verified
semantics (probes checked in as evidence: `corpus/AcpyProbe.java`,
`corpus/ParseProbe.java`, `corpus/FPProbe.java` — messages captured from Temurin
21.0.13, not invented):

* `io.rs` (new module): `StringWriter` (payload-string model, write/toString/close/
  flush), `StringReader` (remaining-string model, read/read-bulk/close).
* `Writer` template methods: `write(char[])` and `write(String)` are CONCRETE in the
  JDK (they forward to the abstract `write([CII)V` / `write(String,II)V` on the
  receiver) — modeled as real forwarders, not abstract markers, because
  `WritingBuffer` inherits them.
* `Collections.unmodifiableList` — a real view class
  (`java/util/Collections$UnmodifiableList`, one backing-list slot): reads delegate,
  writes throw `UnsupportedOperationException` (JDK wrapper semantics, unit-tested).
* `ArrayList(Collection)` — the copy constructor, iterating the source through its
  OWN iterator dispatch (works for any collection implementation); copy-then-diverge
  unit-tested.
* `List.hashCode/equals/lastIndexOf` — real `AbstractList` semantics
  (31-based hash, pairwise equals with dispatch, reverse scan), rows on both
  `ArrayList` (runtime dispatch) and `List` (compile-time declared walk).
* `System.arraycopy` — JDK messages verified verbatim (negative length, source/
  destination bounds, type mismatch), temp-buffer overlap semantics, per-element
  store checks for reference arrays.
* `Arrays.fill([CC)V`; `Integer/Long.toString(x, radix)` and `Long.parseLong(s, radix)`
  with the exact MIN_RADIX/MAX_RADIX and "under radix N" messages; negative
  accumulation so MIN parses exactly.
* `Float.parseFloat` / `Double.parseDouble` (trim, `empty String`, exact
  Infinity/NaN spellings, f/F/d/D suffixes, correctly-rounded f32 direct parse);
  `Float/Double.isInfinite/isNaN`.
* `String.getChars`, `new String(char[])` / `(char[], off, len)`,
  `StringBuilder.append(char[])` / `(char[], off, len)`.
* Serialization HIERARCHY registered (`Serializable`, `ObjectInputStream`,
  `ClassNotFoundException`) while `defaultReadObject` is a loud-unsupported marker —
  the machinery does not exist and says so by name.

Harness fix found by wiring the JDK in: `cfg_equiv` and `lambda_neg` checked
`J2R_JAVAC` for availability but spawned bare `javac` — a split-brain that never
fired while the differential suite silently skipped. Both now resolve javac through
the same variable.

## Wave 1 result

`corpus/MinimalJsonCorpus.java` (3,985 lines: the minimal-json library + driver)
compiles natively and is output-identical to Temurin 21.0.13 on stdout, stderr
(exit 0), including the library's real pretty-print quirk on empty objects and the
exact ParseException location format. Pinned permanently by
`crates/j2r-cli/tests/corpus_suite.rs` (skips without a JDK, like the other
differential harnesses).

## Open gaps (discovered this wave, loud, unfixed)

* **F4 — hex-float literals**: `Float.parseFloat("0x1.8p1")` is valid Java (JDK
  prints 3.0) and currently raises `NumberFormatException`. Loud divergence,
  registered until a dedicated hex-float parser lands.
* **F5 — arraycopy partial-copy message**: a mid-copy `ArrayStoreException` in a
  ref→ref copy uses the aastore-style message instead of arraycopy's own wording;
  behavior (partial copy, destination order) matches the JDK, the string does not
  — needs a JDK vector to pin.

## Wave order

1. ~~Fix F2~~ — done (verifier null-for-reference-shape).
2. ~~Land F3 natives; corpus driver diffs green~~ — done (see F3 result above).
3. ~~Add a corpus runner to CI~~ — done (`tests/corpus_suite.rs`).
4. Corpus entry 2: JZlib (pure-Java deflate/inflate) — pairs directly with the M3
   inflate work and stress-tests `java.io` streams further.
5. Close F4 (hex-float parser) and F5 (arraycopy message vector).
