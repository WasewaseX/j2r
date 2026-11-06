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
  store checks for reference arrays. (Correction from the F4/F5 closure wave:
  in the `can not copy` message, reference-component arrays print the JVM kind
  name `object array` REGARDLESS of their element class — wave 1 only ever
  observed `Object[]` and wrongly generalized to dotted names; pinned with the
  ArraycopyMismatch fixture: `String[] → int[]` prints `object array[] into
  int[]`.)
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

* ~~**F4 — hex-float literals**~~ — **FIXED (2026-11-05)**. Temurin probe
  (corpus/HexFloatProbe.java, 97 cases with exact IEEE bits) pinned the
  grammar (`[+-]? 0[xX] HexDigits [. HexDigits?] p[+-]? DecDigits` — the
  binary exponent is REQUIRED, `0x` is REQUIRED: `.8p1` is an NFE, the
  Javadoc's `. HexDigits` significand branch is NOT accepted; the exponent
  sign IS accepted both ways) and the exact rounding: round-half-to-even on
  the exact binary value, overflow at the max-finite + half-ulp boundary →
  Infinity, subnormal grid with tie-to-zero at half MIN_VALUE, signed zero
  (`-0x0p0` keeps the sign). Implementation: crates/j2r-runtime/src/
  hexfloat.rs (exact guard/sticky discipline, no double rounding — f32
  never rounds through f64); 8 unit tests + the HexFloat differential
  fixture (bit-exact via the new floatToIntBits/doubleToLongBits natives)
  byte-identical vs Temurin.
* ~~**F5 — arraycopy partial-copy message**~~ — **FIXED (2026-11-05)**.
  The JDK vector (corpus/ElementMismatchProbe.java + the NestedElemProbe
  follow-up) pinned it: `arraycopy: element type mismatch: can not cast
  one of the elements of {src component}[] to the type of the destination
  array, {dst element}` with JVM external names (dotted for classes,
  descriptor-shaped for arrays: `[Ljava.lang.String;`), the src name being
  the SOURCE ARRAY's component class, not the failing element's runtime
  class. Confirmed alongside: NO upfront component rejection for ref→ref
  (all-String Object[] → String[] succeeds per-element), partial copy in
  destination order, nulls always copy. Pinned by the ArraycopyMismatch
  fixture, byte-identical vs Temurin.
* InputStream.skip(J) default (JZlib wave), UTF-16 charset family,
  `[[[<prim>` verifier depth, Enum.valueOf — unchanged, loud.

## Wave order

1. ~~Fix F2~~ — done (verifier null-for-reference-shape).
2. ~~Land F3 natives; corpus driver diffs green~~ — done (see F3 result above).
3. ~~Add a corpus runner to CI~~ — done (`tests/corpus_suite.rs`).
4. Corpus entry 2: JZlib (pure-Java deflate/inflate) — pairs directly with the M3
   inflate work and stress-tests `java.io` streams further.
5. ~~Close F4 (hex-float parser) and F5 (arraycopy message vector)~~ —
   done (2026-11-05, see the corrected gap catalog above).

## Wave 2 — JZlib (entry 2, landed 2026-11-05)

**Entry**: ymnk/jzlib (BSD-style, `corpus/jzlib/LICENSE.txt`) — 23 files,
~7.2k lines: a real DEFLATE/INFLATE implementation with its own
huffman/block/codes machinery, Adler32/CRC32 checksums, gzip wrappers,
and the stream classes. Vendored + merged into one default-package file
by `scripts/build_corpus_jzlib.sh` (the same shape as wave 1), driver
`JZlibCorpus` exercising the public ZStream API (levels 0/3/6/9 sampled,
Z_FINISH deflate, Z_NO_FLUSH inflate loop, exact round-trip booleans)
plus Adler32/CRC32 over a fixed vector and the library version string.
**Output-identical to Temurin 21.0.13 on all three channels** — locally
and pinned permanently in `corpus_suite.rs` (both entries).

### What wave 2 actually cost (the JDK surface a real library drags in)

Each item below was found by a loud compile error and closed with either
a probe-pinned native (Temurin 21.0.13) or a structural fix:

1. **Byte-stream hierarchy** — `OutputStream`/`FilterOutputStream`/
   `InputStream`/`FilterInputStream` as builtins (parents, interfaces,
   slot layouts), with the protected `out`/`in` fields the user
   subclasses touch BY NAME: new `builtin_field_slot` + the codegen's
   `resolve_field_slot` (inherited-builtin-field resolution; absolute
   slot = owner's ancestor-base + own index). JDK 21 Filter* behavior
   pinned in io.rs (per-byte `out.write(I)` loop, write([B) self-dispatch
   to write([BII), close = flush-then-close).
2. **ByteArrayOutputStream** — JDK field model in slots (buf byte[]
   ref, count Int), write/write/size/reset/toByteArray/toString/writeTo,
   JDK growth contract, `IllegalArgumentException` on negative initial
   size (Inflate.readString/readBytes).
3. **java/lang/Enum** — javac compiles every `enum` to `extends Enum`
   (JZlib's hand-rolled WrapperType): slots (name, ordinal), <init>
   super(name, ordinal), name/ordinal/toString/equals(identity)/
   compareTo(ordinal). `Enum.valueOf(Class,String)` was a loud marker
   until the F6 wave made it REAL: the class's synthesized `$VALUES`
   static is read through the reflection metadata (meta names the field,
   static_get reads the slot), and javac's synthetic `E.valueOf(String)`
   delegates to it — one implementation covers both entry points.
4. **Array `clone()`** — every array type is a subtype of Object with a
   public clone (JVMS 2.15): validate_virtual accepts array receivers
   for clone + the Object methods; invoke_virtual shallow-copies
   (`array::array_shallow_copy`) and dispatches the Object methods
   through the Object natives. Found by javac's enum `$values()`.
5. **Object.clone default** — fresh object of the same runtime class,
   shallow slot copy, no constructor (JZlib Deflate.clone /
   GZIPHeader.clone call `super.clone()`).
6. **Verifier: nested primitive arrays** — `[[I` elements are `int[]`
   values, not generic refs: the lattice gained
   `ElemKind::RefPrimArray{Int,Long,Float,Double}` (one level;
   `[[[<prim>` still models Ref elements — registered limit), `aaload`
   yields the receiver's static element shape, `anewarray "[I"` (javac's
   `new int[n][]`) maps through the operand, checkcast/multianewarray
   refine by descriptor. Found at JZlib InfBlocks/InfCodes — the JVM
   verifier's exact behavior.
7. **String byte/charset natives** — `getBytes(String)`, `getBytes()`,
   `new String(byte[],String)`, `new String(byte[],int,int,String)` with
   probe-pinned JDK contracts (CharsetProbe/RangeProbe): UTF-8,
   ISO-8859-1, US-ASCII (case-insensitive names); unknown name →
   UnsupportedEncodingException naming the input; null name → NPE("null");
   non-representable chars encode `?`; malformed UTF-8 → one U+FFFD per
   maximal bad subpart; range overflow → the JDK's exact
   StringIndexOutOfBoundsException message. Registered divergence: the
   UTF-16 family is JDK-accepted but unsupported (loud) until a corpus
   path needs it.
8. **Exceptions** — CloneNotSupportedException (JZlib's clone() methods
   declare it) and InternalError (javac's unreachable-default arms)
   joined the builtin hierarchy.

### Build-side note (honest)

The JZlib generated crate (~7.2k lines of emitted Rust with giant switch
tables) SIGSEGVs rustc's default thread stack during expansion; the
corpus runner sets `RUST_MIN_STACK=32 MiB` for its child builds. Recorded
because it is part of the reproduction command.

### Registered gaps from wave 2 (loud, not silent)

- `InputStream.skip(J)` default — LANDED (skip/charset wave): the JDK
  loop-read through a 2048-byte skip buffer via `read([BII)I` on SELF;
  corpus/SkipProbe.java pins a read()-only subclass draining to EOF. The
  JDK `read([BII)I` default was ALSO wrongly a loud "abstract" marker —
  it is concrete and inherited by read()-only subclasses; both are real
  now. ByteArrayInputStream landed with it (the wave-2 phase-2
  candidate; close() is a no-op, negative skip = 0).
- `PrintStream.printf` / `String.format` — the Formatter engine (HALF_UP
  decimal rounding, %s/%d/%.Nf/%n) is unmodelled; calling it fails loud
  (F6 wave registered it when corpus/EnumProbe.java pinned enum surface
  — the probe pins doubles via println instead). Its own wave, probe
  first.
- UTF-16 charset family — LANDED (skip/charset wave,
  corpus/Utf16Probe.java): UTF-16/BE/LE encode+decode with the exact BOM
  rules (FE FF→BE, FF FE→LE, none→BE default), surrogate pairs, U+FFFD
  substitution for unpaired surrogates and dangling trailing bytes, exact
  UEE names. FOUND+FIXED while probing: the classfile Utf8 constant
  parser rejected unpaired surrogates (legal modified UTF-8, JVMS 4.4.7)
  and failed the whole load — now lossy-normalized (the same substitution
  the JDK encoders emit). Standard charsets BEYOND the family (UTF-32,
  KOI8_R — Temurin accepts them silently) stay loud.
- BufferedInputStream — LANDED (stream wave 3, corpus/BufferedProbe.java
  pins 26 rows): the buffer state machine (inherited inner slot + buf/pos/
  count/markpos/marklimit), read-through-buffer and read([BII) spanning
  buffer edges (read1 with the >= marklimit direct-bypass that invalidates
  the mark), the skip law discovered by the probe — a skip over a
  PARTIALLY consumed buffer consumes ONLY the buffer (skip(20) over 7
  buffered bytes returns 7, pos advances to count, the next read refills
  from the inner stream) while a skip on an EMPTY buffer drains the inner
  stream clamped to its available() (skip(100) over 26 = 26) — mark/
  reset with readlimit growth (fill() shifts the marked region to the
  front even when the buffer is exhausted, re-anchoring markpos to 0;
  the JDK's first-draft fill reconstruction got this wrong and the probe
  caught it), "Resetting to invalid mark", and the full "Stream closed"
  family (close() nulls in/buf; every getter then throws). Buffered
  fixture byte-identical vs Temurin; suite 100 → 101. One loud stream
  builtin remains: PushbackInputStream (unmodelled).
- `[[[<prim>` verifier depth — models as Ref elements beyond one level.
- Stream wrappers LANDED (phase 2, skip/charset wave): the corpus driver
  now exercises DeflaterOutputStream/InflaterInputStream,
  ZOutputStream/ZInputStream (with getTotalIn/totalOut and JZlib's own
  decompressor-driven skip), and the full GZIP roundtrip (header parse +
  CRC32 trailer) — all over the landed ByteArrayInputStream/
  ByteArrayOutputStream builtins, byte-identical vs Temurin
  (wrap-deflate n=72, zstream n=72/72/79, gzip n=84, zskip=20 → 'j').
  BufferedInputStream stays the one missing stream builtin (loud).
