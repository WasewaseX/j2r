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
  Enum.valueOf — unchanged, loud. (The `[[[<prim>` verifier-depth item is
  FULLY CLOSED (2026-11-12, depth-5 closure wave): the enumerated
  RefPrimArray variant families were REPLACED by the recursive element
  descriptor (`Nest(Box<ElemKind>)` through array_elem_kind/elem_vty —
  the runtime's shape dispatch was always depth-agnostic) — one wave,
  ANY depth, zero new variants per future depth; the old depth >= 5
  loud collapse is retired. See the wave-order entry below.)

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
- UTF-16 charset family — LANDED (skip/charset wave,
  corpus/Utf16Probe.java): UTF-16/BE/LE encode+decode with the exact BOM
  rules (FE FF→BE, FF FE→LE, none→BE default), surrogate pairs, U+FFFD
  substitution for unpaired surrogates and dangling trailing bytes, exact
  UEE names. FOUND+FIXED while probing: the classfile Utf8 constant
  parser rejected unpaired surrogates (legal modified UTF-8, JVMS 4.4.7)
  and failed the whole load — now lossy-normalized (the same substitution
  the JDK encoders emit). Standard charsets BEYOND the family (UTF-32,
  KOI8_R — Temurin accepts them silently) stay loud.
- `PrintStream.printf` / `String.format` — LANDED (Formatter wave,
  corpus/FmtProbe.java, 43 rows): the common subset with JDK-exact
  semantics; the probe's most important catch is the %f rounding LAW —
  HALF_UP on the SHORTEST decimal repr (2.675 -> 2.68; BigDecimal(double)
  would give 2.67), implemented by rounding the double_to_string digit
  string as a decimal. Implementation: crates/j2r-runtime/src/formatter.rs.
  RESIDUALS LANDED (2026-11-06, corpus/FmtResidualProbe.java, 116 rows —
  the six-round probe falsified three implementation guesses): the
  uppercase-variant family S B H C X E G A uppercases the WHOLE piece
  (%E of Infinity -> `INFINITY`); ',' is legal ONLY on d/f/g (the old
  %,x grouping was wrong — parse-time FormatFlagsConversionMismatchException
  before argument resolution); null prints `null`/`NULL` through every
  conversion except %b (the old %h-of-null PANICKED); %a/%A hex float
  (subnormal 0.x forms, precision HALF_UP on >= 8 with renormalizing
  carries); %g rounds sci-first then branches on the ROUNDED exponent
  with NO trailing-zero stripping; the float family does NOT widen
  integral types; '-' without width -> MissingFormatWidthException; AND
  a real render_e bug exposed: %e of values in [1e-3, 1) had the wrong
  decimal exponent (0.5 printed `0.50000e+00`). Fmt fixture extended in
  place; tests 304 -> 307.
- PushbackInputStream — LANDED (stream wave 4, corpus/PushbackProbe.java,
  11 rows, byte-identical on the first run): the last unmodelled stream
  builtin — the stream builtins are now COMPLETE (BAIS/BAOS/Buffered/
  Pushback/Filter{In,Out}/StringWriter/StringReader).
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
- `[[[<prim>` verifier depth — LANDED (deep-arrays wave) for depth 3:
  corpus/DeepArraysProbe.java pinned a real 3D program failing to verify
  (the one-level lattice modelled the rows as plain Refs); the lattice now
  carries RefPrimArray + RefPrimArray2 variants (depth-aware
  array_elem_kind, elem_vty recursion). **DEPTH 4 LANDED (depth-4 wave,
  2026-11-06)**: the lattice gained RefPrimArray3 variants — `[[[[<prim>`
  elements are `[[[<prim>`-shaped values; the extracted rows now satisfy
  `[[[<prim>`-typed call sites with exact typing, wrong-kind primitive
  ops on a `[[[[I` fail verification naming the exact shape
  (`array is RefPrimArray3Int`), and the DeepArrays fixture grew ten
  in-place depth-4 rows (full/partial/one-dim allocation, typed call
  site, shallow clone, arraycopy, null-row NPE, instanceof + getName
  shapes, the `[[[F`-into-`[[[I` ASE message) — byte-identical vs
  Temurin. **ANY DEPTH LANDED (depth-5 closure wave, 2026-11-12, owner
  directive "why still not done" pulled it forward)**: the enumerated
  RefPrimArray{,2,3} × {Int,Long,Float,Double} families (12 Copy
  variants, four more per future depth) were REPLACED by ONE recursive
  variant `ElemKind::Nest(Box<ElemKind>)` — `[[I` folds to
  `Nest(IntGroup)`, `[[[I` to `Nest(Nest(IntGroup))`, every future
  depth folds one more level with zero new variants; `VTy`/`ElemKind`
  dropped `Copy` (Clone retained; the ripple was mechanical,
  compiler-guided). Depth 5 verifies typed end-to-end (the new law
  test `depth5_recursive_descriptor_any_depth`: aaload on a `[[[[[I`
  yields the exact 4-deep row shape satisfying a `([[[[I)V` call
  site; a wrong-kind prim load names the exact folded shape). The old
  depth >= 5 loud Ref collapse is RETIRED; the runtime's shape
  dispatch was always depth-agnostic (array.rs counts `[`s), so no
  runtime change was needed. cfg_equiv 109×3 + diff_suite 109 zero
  divergences; battery 466/0.
  AND the aastore ArrayStoreException message was fixed by
  corpus/AseProbe.java (5 shapes): Temurin prints the STORED object's
  external name ALONE (`java.lang.Object`, `AseProbe$Inner` binary with
  `$`, `[[I` descriptor-verbatim) — the old "java.lang.X stored into Y"
  shape was never probe-pinned and wrong.
- Stream wrappers LANDED (phase 2, skip/charset wave): the corpus driver
  now exercises DeflaterOutputStream/InflaterInputStream,
  ZOutputStream/ZInputStream (with getTotalIn/totalOut and JZlib's own
  decompressor-driven skip), and the full GZIP roundtrip (header parse +
  CRC32 trailer) — all over the landed ByteArrayInputStream/
  ByteArrayOutputStream builtins, byte-identical vs Temurin
  (wrap-deflate n=72, zstream n=72/72/79, gzip n=84, zskip=20 → 'j').
  BufferedInputStream stays the one missing stream builtin (loud).
- `java.util.regex` (Pattern/Matcher) — LANDED NARROW (stringsim wave,
  2026-11-07): `Pattern.compile("\\s+")` + `matcher(...).replaceAll(...)`
  with the law the registered gap demanded — Java's default `\s` is the
  ASCII class [ \t\n\x0B\f\r] (six chars), hardcoded in
  crates/j2r-runtime/src/regex.rs, NOT Rust's Unicode-aware `\s`; the
  NBSP row in StringsSimCorpus pins it differentially. Any other pattern
  fails LOUDLY naming the gap (the honest surface rule). `\w`/`\d`/
  `(?U)` stay registered for when a caller needs them. Evidence trail:
  the owner's java-string-similarity bugs.md §B21 + opt.md §2.4.

## Wave 3 — JAMA (entry 3, landed 2026-11-06)

**Entry**: NIST JAMA 1.0.3 (public domain, `corpus/jama/LICENSE.txt`) —
7 files, ~3.3k lines: the classic Java matrix package (Matrix +
Cholesky/LU/QR/SingularValue/Eigenvalue decompositions + util/Maths).
A completely different real-world shape from waves 1-2: dense 2D double
arrays, IEEE-heavy numeric loops, `Math` natives, and javac's compound
double-assignment bytecode. Vendored verbatim (including the
"temporary code" sections JAMA disables by breaking its own comment
markers — `\** ... *\` inside a real block comment; `getR()` and the
`(Matrix,int)` constructors are NOT part of the public API), merged into
one default-package file by `scripts/build_corpus_jama.sh`, driver
`JamaCorpus` exercising construction/copy/slice semantics, arithmetic,
norms, all five decompositions, the Hilbert 5x5 SVD, symmetric +
nonsymmetric eigen with residuals, and three error paths
(out-of-bounds get, singular LU solve, non-SPD Cholesky solve, packed
ctor, dimension mismatch — five, all message-exact). Output printed via
Double.toString only — no NumberFormat paths in the driver.
**Output-identical to Temurin 21.0.13 on all three channels** — locally
and pinned permanently in `corpus_suite.rs` (now three entries).

### What wave 3 actually cost (each item found by a loud failure)

1. **Verifier: array covariance (JLS 4.10.3)** — `Vector.copyInto(
   [Ljava/lang/Object;)V` receives a `double[][]`: `[[D <: [Object`
   because `[D <: Object` and `S <: T ⟹ S[] <: T[]`. `pop_exact` now
   accepts `Array(k)` with reference-elements for `Array(Ref)`
   (`ElemKind::elements_are_refs()` in ir.rs); `[I`-style primitive
   groups stay exact. Pinned differentially by the new
   `ArrayCovarianceProbe` fixture (10 rows: parameter/assignment
   covariance, aastore through a covariant view with the JDK's exact
   ArrayStoreException messages `[I` / `java.lang.String`, null stores,
   `[[D` clone independence) — byte-identical vs Temurin on the first
   run. Suite 106 → 107, guard 105 → 106.
2. **Verifier: dup2_x2 forms were scrambled** — the JAMA pivot swap
   `LUrowi[j] = LUcolj[i] -= s;` emits javac's compound cat-2
   assignment: `dup2_x2` with a cat-2 value on TOP of two cat-1s —
   JVMS form 2 — and the old verifier rejected it ("form 4 requires a
   category-2 value below"; a unit test even pinned the wrong
   classification). Rewritten to the runtime stack.rs table (which was
   right all along and differential-proven): F1 all-cat1, F2 cat2-top
   (v3,v2,v1 → v1,v3,v2,v1), F3 cat2-at-bottom (→ v2,v1,v3,v2,v1), F4
   two-cat2; the no-form cat1-on-top-of-cat2 shape now errors with a
   named message. Three unit tests updated to the corrected forms.
3. **`Math.random()` — REAL** (natives.rs/langext.rs): the JDK's own
   48-bit LCG (Random.nextDouble algorithm, seed-uniquifier XOR
   nanoTime lazy seed, CAS per next(bits)). Values are deliberately
   unpredictable — no fixture may print them; the native exists so
   programs referencing `Matrix.random` compile (JLS contract: uniform
   [0,1) doubles).
4. **`PrintWriter` — REAL subset** (natives.rs): JDK-source semantics
   for print(String)/print(char)/println()/println(String)/write(String)
   (null → "null", println = "\n" on Linux) over the two sink families
   the corpus and console output exercise — native stdout/stderr
   PrintStream objects and StringWriter buffers. Slots: inherited
   Writer lock + sink ref + autoFlush. Exotic sinks throw a loud
   UnsupportedOperationException naming the subset.
5. **Registered loud (the serialization precedent)**: `java.util.Vector`
   (5 rows), `java.io.StreamTokenizer` (6 rows + ttype/sval/nval field
   slots), `java.text.DecimalFormat`/`NumberFormat.format(D)`/
   `DecimalFormatSymbols` (9 rows) — resolve and instantiate so real
   programs compile end to end; every behavior native throws
   UnsupportedOperationException naming the gap. The corpus driver
   never calls them (Matrix.read / Matrix.print are uncalled; the
   print path would need the HALF_EVEN-over-exact-decimal DecimalFormat
   engine — a future probe-first wave). `Double.valueOf(String)` is
   REAL (parse + box); `Locale.US` is a real interned constant object.
6. **Bench rows** (the R5 gap "one row per corpus entry"):
   MinimalJsonCorpus 0.04x, JamaCorpus 0.09x native/java (details in
   docs/BENCHMARKS.md §Corpus bench rows 2026-11-06). JZlibCorpus stays
   rowless: the release build is the registered memory limit
   (re-confirmed 2026-11-06 with measured rustc peak RSS 3.03-3.56 GB
   across five attempts and three profiles).

### Wave 3 result

`corpus/JamaCorpus.java` (3,518 lines) compiles natively and is
output-identical to Temurin 21.0.13 on stdout, stderr and exit (180
deterministic lines, including eigenvalue residuals at 1e-13 through
thousands of interpreted double ops — the LU pivot swaps run javac's
dup2_x2 form 2 at full speed). Local gates at this tree: differential
107/107 sharded, cfg_equiv 107/107 both modes (452.7s + 449.5s), corpus
3/3, lib tests 272 green, jar/lambda/soe suites green, fmt/clippy clean,
gates 11 (fixtures=107 tests=311) / 12 / 19 / 20 green, path-filter
17/17, forced-GC on the new fixture green.

## Wave 4 — java-string-similarity (entry 4, landed 2026-11-07)

**Entry**: tdebatty/java-string-similarity v0.24 (Apache-2.0,
`corpus/stringsim/LICENSE.txt`) — 15 algorithm classes + 5 interfaces +
experimental/Sift4, ~2k LOC, vendored verbatim. This is the SAME library
the owner's external report converted with cguz/java-to-rust into 27
broken .rs files and 309 rustc errors; through j2r it compiles clean and
runs **output-identical to Temurin 21.0.13** (after the surface gaps
below closed), merging by scripts/build_corpus_stringsim.sh, driver
StringsSimCorpus, corpus_suite 3 -> 4 entries (70.9s local).

What it cost (each found by a loud failure or the differential):

1. **Map.entrySet / Map.Entry** — the JDK's entry iterator hands out the
   INTERNAL `HashMap$Node` as the `Map.Entry` (Node implements
   Map.Entry), so: HashMap$EntrySet/EntryIterator views (KeySet/KeyIterator
   layout precedents), getKey/getValue rows at BOTH levels (concrete
   `HashMap$Node` AND interface `java/util/Map$Entry` — the static-type
   dispatch law), and the subtype row `HashMap$Node =>
   ["java/util/Map$Entry"]` (the enhanced-for's checkcast depends on it).
2. **Collections.unmodifiableMap** — ShingleBased.getProfile returns it:
   the wrapper forwards get/size/isEmpty/entrySet/keySet/containsKey to
   the delegate and throws the JDK's message-less
   UnsupportedOperationException on put.
3. **Set/HashSet.addAll** — Jaccard's union. Iterates through
   Collection dispatch so any modelled collection works. FOUND+FIXED
   during the wave: the first draft NESTED heap guards (with_obj inside
   with_obj while reading the size counters) and deadlocked at 0% CPU —
   exactly what the module header's lock law L3 forbids; flattened, and
   the AddAllProbe repro is the regression story.
4. **java/util/Map interface-level entrySet row** — static-type dispatch
   needs the row at the interface level (the keySet precedent), not only
   on the concrete classes.
5. **java.util.regex LANDED NARROW** (the registered gap's landing
   condition, see the updated gap line above): the ASCII-`\s` law
   hardcoded, NBSP pinned differentially.

The driver pins the external report's silent-semantic rows against
Temurin: `Levenshtein("\uD83D\uDC4D", "x") = 2.0` (the UTF-16 law the
broken converter's B16 called unfixable), JaroWinkler's f32-fidelity
bits (`jw=0.996666669845581` — the report's B17 row, bit-exact through
j2r's typed float model), the NBSP profile verbatim (B21/ASCII-\s), the
3-shingle HashMap-iteration-order profile prints, Sift4's method-local
class + LinkedList (the converter's B11), interface dispatch through
user implementations, overload resolution, and the message-exact error
paths (nulls, k<=0). Bench rows: NOT taken this wave (correctness
first; the JamaBench steady-state protocol applies when they are).
