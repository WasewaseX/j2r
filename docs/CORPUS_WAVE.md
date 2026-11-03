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

### F1 — JDK superclass coverage in the builtin table — FIXED (this wave)

`WritingBuffer extends java.io.Writer` was rejected at load: "unknown superclass
java/io/Writer: add its .class file to the inputs". JDK types are not inputs — they are
the runtime's job. `crates/j2r-runtime/src/builtin.rs` now knows the java.io
writer/reader hierarchy: `Writer`/`Reader` (with the `lock` slot),
`StringWriter`/`StringReader` (payload slot on top), and the interfaces
`Appendable`/`Closeable`/`Flushable`/`Readable`. Pinned by the
`corpus_wave1_io_hierarchy` unit test.

### F2 — verifier vs. reference-typed statics initialized in `<clinit>` — OPEN

`PrettyPrint.singleLine()` — the plainest static singleton factory in Java — fails
j2r's verifier: `expected Array(IntGroup) on the stack, found Null`. Reference-typed
statics whose value is assigned in `<clinit>` are mistyped in the static-slot type
model. Singleton/static-factory patterns are ubiquitous in real libraries, so this
blocks the corpus behind one codegen fix. Fix location: the static-typing pass in
j2r-codegen (shapes/specialization from M2 wave 3). **Priority: high.**

### F3 — java.io behavior natives — OPEN (deliberately after F2)

Once the corpus loads, the driver's write paths hit `StringWriter.write/toString`,
`StringReader.read`, and the `Writer` lock-slot constructor. These must land as runtime
natives (collections.rs pattern) — until then the generated crate fails loud with a
named missing native, which is the designed behavior, not a silent wrong answer.

## Wave order

1. Fix F2 (verifier/static-typing) — unblocks the entire corpus.
2. Land F3 natives; corpus driver diffs green vs. Temurin 21.0.13 on all three
   channels.
3. Add a corpus runner to CI (separate from the 83-fixture suite guard).
4. Corpus entry 2: JZlib (pure-Java deflate/inflate) — pairs directly with the M3
   inflate work and stress-tests `java.io` streams further.
