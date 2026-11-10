# M5 wave 2 — T11: the Call*Method surface and the variadic-ABI answer
(design note)

Status: DESIGN OF RECORD for T11 (committed before any code, the
slice-3a pattern). Resolves flagged-open finding **F-J2** ("variadic
slots typed at A-form arity until T11's trade-off note", registered at
T08) and defines the pending-exception machinery Call*Method forces
into existence. Read with M5_PLAN §3, M5_JNI_FFI.md and
M5_JNI_CLASSES.md (the method-ID registry this note builds on).

## 1. The question, stated exactly

JNI exposes every method call in THREE forms per return kind K:

- bare `Call<K>Method(env, obj, mid, ...)` — **C-variadic**: the caller
  appends the Java method's arguments per its descriptor,
- `Call<K>MethodV(env, obj, mid, va_list ap)` — the caller's saved
  argument list,
- `Call<K>MethodA(env, obj, mid, const jvalue *args)` — an ARRAY of
  8-byte `jvalue` words.

K ranges over Object, Boolean, Byte, Char, Short, Int, Long, Float,
Double, Void; the same triple exists for `CallStatic<K>Method` and
`CallNonvirtual<K>Method` — **90 call slots** plus `GetMethodID` (33)
and `GetStaticMethodID` (113), on top of the slice-3a field registry.

F-J2: today the bare slots are typed `(env, obj, mid, _args:
*const c_void)` — "A-form arity". That typing is a documented LIE: it
reads at most one vararg word and silently drops the rest.

## 2. The ABI analysis (why there is no shortcut)

x86-64 SysV classifies EACH vararg: INTEGER args go in
rsi/rdx/rcx/r8/r9 (then stack), SSE args (float/double) in xmm0–7. A
fixed-arity shim with integer-only trailing params reads doubles as
garbage and MISALIGNS every following argument — silent corruption,
the exact failure class this project exists to make impossible.
Concretely: `CallVoidMethod(env, obj, mid, 3.14)` puts 3.14 in xmm0;
an integer-typed 4th param never sees it.

- **libffi**: the general answer, and REJECTED — an external
  dependency in the zero-dep runtime plus a mass of unsafe call-frame
  construction. Both standing laws break.
- **Defining C-variadic fns in Rust**: the definition is stable, but
  the BODY cannot read its own varargs without `va_start` — and
  `core::ffi::VaList` is nightly-only; manual register reads are
  `unsafe`. Under `#![forbid(unsafe_code)]` the body can do nothing
  with the args it received.
- **Max-arity integer shims**: corrupt for SSE-class args (above). A
  "works for ints" shim is worse than an abort — it lies.

## 3. The decision

1. **A-forms are the v1 workhorse — implemented FULLY and faithfully.**
   `jvalue*` is a plain array of 8-byte words; the method-ID registry
   (slice 3a) already knows the method's descriptor, hence arity and
   per-arg kinds. The wrapper copies N words in through the audited
   `copy_from_caller`, marshals per the descriptor into `Value`s, and
   invokes through the host dispatch. No variadics, no unsafe, no
   approximation. ~30 A-slots (10 kinds × 3 families).
2. **V-forms: implemented via the documented SysV `__va_list_tag`
   save-area walk.** On this ABI `va_list` is a pointer to a 24-byte
   record (gp_offset, fp_offset, overflow_arg_area, reg_save_area);
   consuming it is a DETERMINISTIC algorithm over that record. The
   pointer reads live in `j2r-jni-ffi` as new audited primitives
   (`read_word_at`, `read_fp_word_at`, `read_u32_at`) — the same
   scoped-unsafe pattern as write_u8_out, with the gate-10 count
   amended IN THE SAME COMMIT (T09 precedent; pin ≤ 12, currently 6
   used — `ACTUAL_UNSAFE_OCCURRENCES` = 6 asserts the file count). The
   walk is unit-tested against hand-built save areas with known
   layouts.
3. **Bare forms: LOUD aborts, honestly typed.** The slot stays a stub
   but the message stops lying: it names the slot, states that the
   bare C-variadic ABI cannot be served in safe stable Rust (F-J2
   resolution text), and directs the caller to the A-form — which is
   what the T13 C-harness (OUR evidence, M5_PLAN §3.4) exercises.
   Registered for M6 evidence: which forms LWJGL3 actually emits; if
   bare forms dominate, the mixin wave reopens this with libffi as a
   SEPARATE optional crate behind a feature flag — never in the
   runtime.
4. **The pending-exception cell — Call*Method forces it.** A called
   method that throws must NOT abort the process: JNI semantics say
   the throwable is recorded PENDING, the call returns the zero value
   (null/false/0/0.0), and `ExceptionOccurred` (15) / `ExceptionCheck`
   (228) / `ExceptionClear` (17) report and clear it. This slice adds
   the thread-safe pending cell to the bridge and makes those three
   slots real — the FIRST faithful exception transport across the
   boundary, and the upgrade path for the T13-registered NPE gaps in
   earlier slices (each later slice decides its own migration; the
   landed v1 laws stay as documented until then).

## 4. Invocation semantics (the three families)

- `Call<K>Method`: dispatch by the RECEIVER's runtime class (virtual —
  the ordinary override law).
- `CallStatic<K>Method`: resolve the static method on the jclass's
  class; no receiver.
- `CallNonvirtual<K>Method`: dispatch on the DECLARED class (the
  jclass argument), not the runtime class — the `super.m()` shape.
- Null receiver → a pending `java/lang/NullPointerException` + zero
  return (the faithful shape, now that the cell exists).
- The thrown object itself is a heap `Throw` — stored in the cell, so
  a caller may inspect and rethrow it after `ExceptionOccurred`.
- `jvalue` marshalling per descriptor letter: `Z`→bool, `B/C/S/I`→
  Int-group i32 words (narrowed per the ARRAY/param descriptor law),
  `J`/`F`/`D`→i64/f32/f64, `L…;`/`[…]`→reference (null = JNI null).

## 5. Evidence bar (acceptance)

Unit tests: descriptor-driven arity walk (right arg count from a
hand-built methodID); jvalue↔Value marshalling per kind including
Int-group narrowing and null refs; virtual override vs Nonvirtual
declared-class dispatch vs static; pending-exception round-trip
(throw inside the callee → zero return + ExceptionOccurred true →
ExceptionCheck agrees → ExceptionClear clears); V-form save-area walk
against hand-built records for INTEGER-only, FP-only and mixed arg
lists; bare-form abort message names the slot (child-process, the
slice-2/3b pattern). Then fmt, clippy `-D warnings`, gate-10 count
assert (5 → amended number in the same commit as the new primitives),
doc-count sync, full battery re-run (the boards' standing law), boards
+ push.

## 6. Slice order

- **11a** — this note (the F-J2 resolution).
- **11b** — pending-exception cell + ExceptionOccurred/Clear/Check
  slots real + A-forms (all 30) + descriptor-walk invoke core. The
  battery in the same landing.
- **11c** — V-forms via the save-area walk + the gate-10 amendment
  (new audited primitives + count assert), battery again.

T13 keeps the C-harness evidence bar (a compiled `.so` exercising
every v1 function on both runtimes, three-channel) — this note defines
what that harness can rely on.
