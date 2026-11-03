# External audit — verification of every claim (2026-11)

An external review of a **partial source dump** of this repo produced seven
findings. The dump did not include the full `j2r-codegen` / `j2r-runtime`
implementations, and the reviewer said so. Before acting on any of it, every
claim was re-derived from the actual source on `main` — file, line, and where
possible a live experiment. Nothing below is taken on trust, including the
reviewer's severity ratings.

Verdict key: **CONFIRMED** (reproduced exactly as stated), **PARTIAL**
(real observation, wrong weight or wrong mechanism), **REJECTED** (not
reproducible — hallucinated).

## 1. `compare.rs` hides suppressed exceptions — P0 — **CONFIRMED, worse than stated**

Claim: the differential harness deletes meaningful stderr differences and can
report a match for programs that behave differently.

Verified in `crates/j2r-cli/src/compare.rs`:

* Line 47: `if t == "..." || t.starts_with("Suppressed: ")` — every
  `Suppressed: <type>: <message>` line is dropped *before comparison*, on
  both sides. Suppressed lines carry exception types and messages — exactly
  the information the module doc (lines 7–14) promises is never normalized
  away. The doc contradicts its own code: line 10 says frames "contain no
  exception type or message information", line 34 strips lines that do.
* Line 47 also strips bare `...` lines. The JVM prints the "more" tail as
  tab-prefixed (`\t... 3 more`), which line 44 already covers; a program
  printing a literal `...` to stderr has its own output erased.
* Deeper than the reviewer knew: `rg addSuppressed|getSuppressed` over the
  whole crate tree returns **zero hits**. The runtime has no suppressed-
  exception machinery at all. Today a `try`-with-resources program whose
  `close()` throws fails j2r *compilation* (unresolved `addSuppressed`) —
  honest, per the no-silent-miscompile rule. But the moment suppression
  support lands partially, this normalization would let it pass diffs while
  diverging from the JVM. The harness must be made honest *first*.

This is the project's truth spine. A masking bug here poisons every "all
green" claim built on top of it. P0 agreed.

## 2. `main.rs` main-class selection is unreliable — P1 — **CONFIRMED, worse than stated**

Verified in `crates/j2r-cli/src/main.rs`:

* Lines 447–449: the comment says "prefer the class matching the source file
  name", then `let simple = ...; let _ = simple;` — the preference was never
  implemented. Dead code next to a stated intent.
* Line 450: the fallback is "first class with `main` in `collect_class_files`
  order" — i.e. `read_dir` order, which is filesystem-arbitrary.
* Line 352: the native side calls `compile_crate(..., None, ...)` — codegen
  then picks its own main at `crates/j2r-codegen/src/lib.rs:618–625` by
  first-match in `prog.classes` order. **Two independent selection
  implementations** that agree today only because both are fed the same
  arbitrary directory order. A multi-class program with two `main` methods
  can run different mains on the two sides of the differential test — the
  worst possible failure mode for a differential harness.

## 3. `lib.rs` — validate ConstantValue and nested Code attributes — P1 — **PARTIAL** (right family, overstated)

Verified in `crates/j2r-classfile/src/lib.rs`:

* What the reviewer missed: ConstantValue *tag* validation already exists —
  lines 787–792 reject a pool entry of the wrong type with a precise error.
  Nested `Code` bytes are bounds-checked (`r.bytes`), and nested attributes
  cannot corrupt the outer parse (they live in a dropped slice, line 821).
* What is genuinely missing: the attribute **length** is read (line 778) and
  never checked. JVMS §4.7.16 requires `ConstantValue.attribute_length == 2`;
  a malformed length shifts the reader onto the next attribute and can
  misparse silently or fail with a confusing offset instead of a precise
  diagnostic. Same family for `Code`: no `code_length` range check (JVMS
  §4.7.3 requires `0 < code_length < 65536`).
* Impact is confined to **hostile class files** — javac never emits
  malformed attribute lengths. Re-rated P1 → **P2 hardening**, fix is small.

## 4. `inflate.rs` — "follow the DEFLATE distance-code rules" — P1 — **PARTIAL** (the rules are followed; the *validation* diverges from zlib, in both directions)

The reviewer's headline is wrong for valid streams, and we can prove it:

* Length/distance tables match RFC 1951 §3.2.5 value-for-value
  (`LENGTH_BASE`, `LENGTH_EXTRA`, `DIST_BASE`, `DIST_EXTRA`, lines 230–245).
* The decode walk is the puff algorithm (bit-at-a-time canonical decode,
  lines 202–226), and the LZ77 copy is overlap-safe byte-at-a-time
  (lines 451–458). The three JDK-produced vectors (`deflate_l0/l1/l9.bin`)
  pin real zlib output. **No valid stream decodes wrong.**

Where the file genuinely diverges from zlib is acceptance of *malformed*
tables. zlib's `inftrees.c` (fetched and quoted from upstream,
madler/zlib develop branch, lines 114–134):

```c
if (left > 0 && (type == CODES || max != 1))
    return -1;                      /* incomplete set */
```

i.e. an incomplete Huffman code is legal **only** when exactly one code of
length 1 is defined; an empty table (max == 0) builds a lazy error marker.
j2r today:

* accepts **any** incomplete distance table (`allow_incomplete = true`
  unconditionally, line 264) — more lenient than zlib; a malformed archive
  zlib's `Inflater` rejects ("invalid distances set") can extract here (it
  still errors loudly if an invalid code is actually demanded, so output is
  never corrupted — acceptance is the only divergence);
* rejects *all* incomplete literal tables (`from_lengths(..., false)`,
  lines 321, 393) — **stricter** than zlib, which accepts the single-
  1-bit-code case for length tables too.

`java.util.zip.Inflater` wraps zlib, so "zlib-exact" *is* the Java-exact
behavior this project promises. Re-rated P1 → **P2**, fix precisely
specified in the plan below.

## 5. `main.rs` — Windows PATH and classpath parsing — P1 — **CONFIRMED** (portability-scoped; re-rated P2)

* Line 493: `path.split(':')` — Windows `PATH` separator is `;`.
* Lines 44/67: `value_delimiter = ':'` — splits Windows classpaths on the
  colon inside `C:\...` drive letters.
* Line 500: `/usr/lib/jvm` fallback scan is unconditional.

Real defects, zero impact on Linux CI/dev. The project doesn't claim Windows
today, but M5 (Minecraft readiness) implies Windows launchers eventually.
Registered as **P2, portability**, with a `#[cfg(windows)]` fix.

## 6. `crc32.rs` — cache the CRC32 table — P2 — **CONFIRMED** (trivial)

Line 38: `pub fn update(...) { let table = build_table(); ... }` — the
256-entry table is rebuilt on **every** `update()` call. O(1) overhead per
call, zero correctness impact, insulting to call P1 anywhere. Fix is a
`LazyLock`. P2 confirmed as stated.

## 7. README — fixture and test information is wrong — P2 — **CONFIRMED**, three stale counts plus a duplicate

Measured on `main` (`dc347e2`):

* README line 16 claims "**70 fixture programs**"; `tests/suite/` holds
  **78** `.java` files (the real-program wave of `909b459`/`de4dc32`
  happened after the README was last touched; the suite guard at
  `diff_suite.rs:63` is `>= 77`).
* README line 260 claims "**196 tests**" (M2-era number).
* README line 266 claims the differential suite is "**39 programs**" —
  stale since M3 Wave 1.
* "Classpath & JAR loading (M3 Wave 1)" appears **twice** in *What works
  today* (lines 155 and 172) with near-duplicate wording.

And the meta-lesson the reviewer's last line implies: counts were hand-edited
into prose and drifted. Standing fix (user directive): a count-sync check so
this class of rot fails CI instead of shipping.

## Hallucination audit — summary

| # | Claim | Verdict | Reviewer severity | Re-rated |
|---|-------|---------|-------------------|----------|
| 1 | compare.rs hides suppressed diffs | CONFIRMED (worse) | P0 | P0 |
| 2 | main-class selection unreliable | CONFIRMED (worse) | P1 | P1 |
| 3 | ConstantValue / nested Code validation | PARTIAL — tag validation exists; length validation missing | P1 | P2 |
| 4 | inflate distance-code rules | PARTIAL — valid streams provably correct; zlib-validation diverges both ways | P1 | P2 |
| 5 | Windows PATH/classpath parsing | CONFIRMED (portability) | P1 | P2 |
| 6 | crc32 table rebuilt per call | CONFIRMED (trivial) | P2 | P2 |
| 7 | README stale counts | CONFIRMED (3 counts + dup) | P2 | P2 |

**Zero findings were pure hallucination.** Two of the seven were directionally
right but mechanistically off, and both P1 downgrades rest on evidence the
reviewer could not have had from the dump (the existing tag validation; the
JDK vector tests). The reviewer's own caveat about the partial dump was
honest and is hereby confirmed.

## Fix plan (registered in ROADMAP.md as M3.5, tasks A1–A8)

* **A1 (P0)** `compare.rs`: keep `Suppressed: <type>: <message>` lines; strip
  only tab-prefixed frame lines (`\tat `, `\t... N more`); drop the bare-`...`
  rule. Flip the two unit tests that pinned the old behavior; add: different
  suppressed types fail, different suppressed messages fail, a program's own
  `...` output survives. The harness must be honest *before* the runtime
  feature lands.
* **A2 (P0)** Runtime suppressed-exception machinery: `Throwable.addSuppressed`
  / `getSuppressed` natives, suppressed list on throwable heap objects,
  uncaught-handler printing of `Suppressed: ...` sections, and a
  try-with-resources differential fixture with a throwing `close()`.
* **A3 (P1)** One main-class selection, used by both sides: prefer the class
  whose name matches the source-file stem; else require exactly one
  main-bearing class; multiple candidates without a preferred match = loud
  error listing them. Pass the selection to `compile_crate` instead of
  `None`.
* **A4 (P2)** `j2r-classfile`: enforce `ConstantValue.attribute_length == 2`,
  `0 < code_length < 65536`; walk/skip Code sub-attributes explicitly so the
  misleading "skipped by length" comment becomes true. Malformed → precise
  byte-offset errors.
* **A5 (P2)** `inflate.rs`: zlib-exact table validation — incomplete rejected
  unless empty (lazy error) or a single 1-bit code; fixed distance table
  becomes the complete 32×5-bit RFC form with the existing `dsym >= 30`
  use-time check. Unit tests pin the rule at `Huffman::from_lengths` level;
  JDK vectors prove no valid-stream regression.
* **A6 (P2)** `crc32.rs`: build the table once (`LazyLock<[u32; 256]>`).
* **A7 (P2)** Windows portability: `PATH`/classpath splitting via
  `std::path::env::join_paths`-compatible separator handling
  (`;` on Windows, drive-letter-safe), `/usr/lib/jvm` scan behind
  `#[cfg(unix)]`.
* **A8 (P2)** README counts corrected **and kept synchronized**: a
  `scripts/check_doc_counts.py` that measures suite size and test totals and
  fails on drift from the numbers written in README/ROADMAP, wired into CI.
  Hand-edited prose counts are banned going forward.

Acceptance for the whole block: all ten CI gates green, the differential
suite (now 78) green, `cargo doc -D warnings` clean, and the two P0 items
demonstrated with a before/after fixture run in the evidence doc.

## Verification log — 2026-11-03

**A1 executed.** The masking rule (`t == "..." || t.starts_with("Suppressed: ")`)
is deleted from `normalize_stderr`. Kept: `Suppressed: <type>: <message>`
header lines and a program's own bare `...` output, compared byte-for-byte.
Normalized still: tab-prefixed JVM frame lines (`\tat `, `\t... N more`).
Module doc rewritten to match, with an audit-A1 note. Five tests pin the
semantics: `suppressed_header_is_kept_but_its_frames_are_normalized`,
`different_suppressed_types_fail`, `different_suppressed_messages_fail`,
`missing_suppressed_section_fails`, `a_programs_own_ellipsis_output_survives`.

**Evidence for the first post-A1 green run:**

* `cargo build --workspace` — exit 0.
* `cargo test --workspace` — 232 passed, 0 failed (includes the five new
  A1 tests; the differential entry skips without a JDK on PATH).
* `cargo test -p j2r-cli --test diff_suite` with Temurin
  21.0.13+9 (`/home/z/tools/jdk21`, persistent across sandbox resets) —
  **78/78 fixtures, stdout + stderr + exit all matching, 247.57 s, exit 0.**
  This is the first green differential run that does not have the masking
  rule underneath it, so it is the first one that means what it says.

**Internal finding #8 (`#acro_export]` at `j2r-runtime/src/lib.rs:421`) —
DISPROVEN, closed as phantom.** During A1 verification an internal session
reported committed syntax corruption (`#acro_export]` instead of
`#[macro_export]`), bisected to `de4dc32`. Re-verified today with three
independent methods, all agreeing the file is clean:

1. Blob hashes of the working tree, `HEAD` (`dc347e2`) and `de4dc32` are
   identical: `8c0c2090…` (sha256 over 15 598 bytes).
2. A single-process byte read finds **zero** occurrences of the corrupt
   byte sequence and exactly one `#[macro_export]`.
3. `cargo build --workspace` succeeds — rustc reads real bytes from disk
   and cannot be fooled; a stray `#acro_export]` before `macro_rules!` is
   a hard syntax error.

Root cause of the phantom: after a sandbox reset, tool output served
stale/garbled fragments (observed the same day: `od`, `grep` and `Read`
views of the same file contradicting each other within one batch).
Standing rule recorded here: **tool output is not evidence for syntax
claims in this repo — a compiler run plus a byte hash is.** The earlier
bisect that "found" the corruption trusted a garbled view over a clean
python-heredoc read; this entry exists so nobody repeats that.

Method note for the whole log: the same standard applied to the external
reviewer's claims was applied to our own internal claim, and it is the
only internal claim so far that failed. Zero hallucinations came from the
reviewer; one came from us.

## Completion log — A2 through A8 (2026-11-03, same day)

All eight audit-response tasks landed, one commit each, gates re-run
green after every landing. Evidence per task:

* **A2** (`9d3a043`) — suppressed-exception machinery. Temurin probe
  established the exact bytes before implementation (`\tSuppressed: `
  headers — the probe corrected the implementer's memory of OpenJDK
  formatting; "Self-suppression not permitted"; "Cannot suppress a null
  exception."; `getSuppressed()` starts empty). Four TWR fixtures
  differentially green. The **cfg-equivalence gate caught a real bug
  the wave introduced**: `throw_new` still allocated the old 1-slot
  layout, so enclosed-trace rendering panicked on any runtime-raised
  exception (and the panic text embedded the OS thread id, which made
  the two emit modes differ — the gate did precisely its job). Fixed in
  the same wave; gate re-run green at 82 fixtures.
* **A3** (`5ebbeed`) — `select_main_class`: stem preference, sorted
  candidates, loud ambiguity error; the selection feeds BOTH the JVM run
  and the native compile. `MultiMain` fixture pins the preference; a
  stem-less two-main source errors loudly listing both candidates.
* **A4** (`d9713b2`) — ConstantValue length==2, `0 < code_length <
  65536`, explicit Code sub-attribute walk. Six new negative/positive
  tests on hand-crafted class files; the walk accepts real javac output
  (full codegen suite green).
* **A5+A6** (`6062d5b`) — `TableKind`-parameterized zlib-exact table
  validation; fixed distance table in the complete 32×5-bit form; CRC
  table in a `LazyLock`. Six rule-pinning tests; JDK deflate vectors
  green (no valid-stream regression); JAR suite green.
* **A7** (`d209f84`) — `split_paths` PATH handling ×3 sites, cfg(unix)
  jvm scan, cfg_attr classpath delimiter.
* **A8** (this commit) — `scripts/check_doc_counts.py` + CI gate-11.
  The checker measured fixtures=83, tests=245 — the implementer's
  mental estimate (244) was wrong by one, which is the entire argument
  for measuring instead of claiming. README corrected (70→83, 196→245,
  39→83 ×2, duplicate JAR bullet removed, new suppressed-exceptions
  bullet), suite guard checked by the same gate.

Final state: 83 fixtures (guard ≥ 82), 245 tests, doc counts
machine-checked, fmt + clippy clean, workspace green with the JDK
wired.
