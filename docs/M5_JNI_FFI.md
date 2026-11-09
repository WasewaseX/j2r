# M5 wave 2 — F-J1 resolution: the scoped FFI crate (design note)

Status: DESIGN OF RECORD for T09 (registered 2026-11-09, session 2).
Author-context: T08's implementation surfaced F-J1 and this note answers
it before any handle-bridge code lands, per the wave rule "design note
first, evidence always".

## 1. The finding, restated precisely (T08)

T08 proved the tables can be generated, pinned and handed to foreign
callers with ZERO `unsafe` in our code: raw pointers are created (safe),
never dereferenced; statics are `Sync` via `&'static` table pointers; the
handle bridge can be safe because `jobject` values we fabricate are
integer tags dressed as pointers (`ptr as usize` casts are safe; the tag
indexes OUR table, bounds-checked, generation-checked — no deref of
foreign memory needed on the hot path).

But 35 of the 231 slots cannot work that way. JNI is a bidirectional
memory ABI: for these slots, foreign code hands us a buffer pointer to
READ (`Set*ArrayRegion`, `NewString`/`NewStringUTF`, `DefineClass`), or a
buffer pointer to WRITE (`Get*ArrayRegion`, `GetStringRegion`/
`GetStringUTFRegion`, `GetJavaVM`/`GetEnv`/`AttachCurrentThread` out
params), or a pointer we previously returned whose CURRENT CONTENTS we
must re-read (`Release<Prim>ArrayElements` copy-back). Rust has no safe
way to read or write memory behind a raw pointer — every one of these
slots requires at least one `unsafe` operation SOMEWHERE in our code.
`#![forbid(unsafe_code)]` (crate-wide) cannot be locally relaxed — an
inner `allow` under `forbid` is a compile error — so these slots are
unimplementable in `j2r-runtime` as it stands.

## 2. Options considered

**(a) Dedicated FFI crate with an audited unsafe core — CHOSEN.**
A new workspace crate `j2r-jni-ffi`: zero dependencies, no
`forbid(unsafe_code)`, containing ONLY the memory primitives the bridge
needs (est. 6 small functions, est. ≤ 12 `unsafe` occurrences, each with
a `// SAFETY:` justification naming its caller and its contract). The
safe half of the bridge stays in `j2r-runtime::jni` exactly as built in
T08: handle fabrication, table management, JNI-exception state, GC
rooting — all safe. `j2r-jni-ffi` is reachable only from
`j2r-runtime::jni` (not from generated code, not from other crates).

**(b) Stay 100 % safe forever: leave all 35 slots loud aborts.**
Honest but self-defeating: those slots are precisely LWJGL's memory
lifecycle (buffer regions, string regions, element copies). Choosing (b)
means the JNI bridge cannot serve the registered M5 blocker. Rejected as
the primary path — but (b) remains the FALLBACK if the owner rejects the
law amendment; the aborts are already in place from T08, so the fallback
costs nothing now.

**(c) Implement the memory slots via a registry keyed on
addresses the foreign side returned to us.**
Works for `Release*`-style calls (match the pointer as a `usize` against
our registry — no deref), and T09 will use that trick wherever possible.
It does NOT work for the region/`New*`/out-param families: the caller's
buffer is theirs, allocated outside our knowledge — there is nothing to
look up. Rejected as a complete answer; adopted as a partial one.

## 3. The law amendment (what changes, exactly)

The standing law "zero `unsafe` in the workspace" becomes:

> Zero `unsafe` in every crate EXCEPT `j2r-jni-ffi`, where every
> `unsafe` occurrence (a) carries a `// SAFETY:` comment naming the JNI
> contract that justifies it, (b) is inside one of the ≤ 7 audited
> primitive functions listed in §4 (T09: six; T10 slice 2 amended to
> seven with `write_u8_out` — see §4 entry 7 — the audited-surface
> growth is registered for the M5 completion review alongside the
> original amendment), and (c) is counted and pinned by
> gate-10, which is amended from "grep finds no unsafe in crates/" to:
> grep finds no unsafe in crates/ EXCEPT `crates/j2r-jni-ffi/src/ffi.rs`,
> whose `unsafe` occurrence count is asserted ≤ 12 (the count lives in
> the gate script, drift fails CI loudly).

The amendment is registered for the M5 completion review alongside the
wave evidence, same as every process deviation before it. Gate-10 keeps
its all-gates wiring; the amended gate FAILS if `j2r-jni-ffi` disappears
(the allowlist is an explicit exception list, not a blind spot), and
fails if any OTHER crate gains so much as one `unsafe`.

## 4. The audited primitive surface (complete list, v1)

All in `j2r-jni-ffi/src/ffi.rs`; each is a total function with explicit
null handling in a safe wrapper in `j2r-runtime::jni`:

1. `copy_from_caller(src: *const u8, len: usize) -> Vec<u8>` — read a
   caller-owned byte region (Set*ArrayRegion, DefineClass, NewString).
   SAFETY: caller guarantees `len` readable bytes per the JNI spec.
2. `copy_to_caller(dst: *mut u8, src: &[u8])` — write a caller-owned
   region (Get*ArrayRegion, GetStringRegion families). SAFETY: caller
   guarantees `dst` holds `len` writable bytes per the JNI spec.
3. `read_ptr_out(penv: *mut *mut c_void, value: usize)` — write one
   pointer-sized out-param (GetEnv, AttachCurrentThread(AsDaemon),
   GetJavaVM). SAFETY: JNI contract — the caller owns a valid out-slot.
4. `read_caller_str(ptr: *const c_char) -> Option<String>` — C-string
   read with the modified-UTF-8 decode done SAFE-side (ThrowNew,
   NewStringUTF, DefineClass name). SAFETY: caller guarantees NUL
   termination per the JNI spec.
5. `copy_back_elements(elems: *mut c_void, dst: &mut [u8], mode)` —
   Release<Prim>ArrayElements copy-back over a pointer WE handed out
   (registry-verified before this runs). SAFETY: the pointer is ours,
   registry-matched, and the caller may have mutated its contents —
   re-reading it is the operation's purpose.
6. `direct_buffer_ok(ptr: *mut c_void)` — one-beyond contract check for
   NewDirectByteBuffer/GetDirectBufferAddress bookkeeping (the pointers
   themselves are created safe-side; this only normalizes the stored
   form). SAFETY: no dereference — address arithmetic only.
7. `write_u8_out(slot: *mut u8, value: u8)` — **added by the T10
   slice-2 amendment**: write one `jboolean` out-param
   (`GetStringUTFChars`'s `isCopy`; later the `Get<Prim>ArrayElements`
   family's `isCopy`). The amendment exists because primitive 3 writes
   a full pointer word and would overrun the 1-byte `jboolean` slot —
   the honest options were this one-byte primitive or silently skipping
   a required write, and the latter is a wrong answer by definition.
   SAFETY: caller guarantees one writable `jboolean` out-slot per the
   JNI spec; total (null slot = no-op, the spec's "caller doesn't ask").

Everything else in the bridge stays in `j2r-runtime::jni` under
`#![forbid(unsafe_code)]`: handle tags, local/global/pinned-ref tables,
frame push/pop, exception state, registry lookups (address-as-usize
keys), and all 196 slots that never touch foreign memory.

## 5. Evidence bar (how this note stops being prose)

- T09 lands the crate + bridge with unit tests per primitive (null
  rejection, registry-miss loudness, round-trips). — **LANDED (T09,
  2026-11-09, session 3): `j2r-jni-ffi` 7 unit tests; the safe bridge
  `j2r-runtime::jni::bridge` 13 unit tests (handle round-trips, stale-
  tag loudness, wrong-kind delete loudness, frame push/pop + promote +
  sweep, detach, GC survival, critical registry round-trip + loud miss,
  root-scan coverage); 379 tests workspace-wide, 0 failures; fmt/
  clippy(`-D warnings`)/doc-counts green; cfg_equiv + diff_suite zero
  divergences; the codegen e2e proves the two-crate vendoring (the
  runtime's internal path dep is vendored alongside it).**
- The gate-10 amendment lands in the SAME commit as the first unsafe
  occurrence (the gate and its exception ship together — the
  gate-upgrade discipline from the 3b202f8/7bb7a8d lessons). — **DONE,
  same commit.**
- The T13 C harness remains the end-to-end judge: every v1 function
  exercised from real C against Temurin vs j2r, three channels.
- The `unsafe` occurrence count appears in the T09 evidence record with
  its final number; the gate pins it from then on. — **Final count: 5
  (one per primitive 1–5; primitive 6 needs no unsafe — address
  arithmetic is safe). Pin ≤ 12; the count is also asserted by an
  in-crate test that mirrors the gate's grep, so drift fails locally
  before CI.**
- Signature refinement registered (vs §4's first draft):
  `read_caller_str` returns the raw bytes up to NUL (`Option<Vec<u8>>`)
  rather than a `String` — modified UTF-8 is not always valid UTF-8, and
  the mUTF-8 decode belongs safe-side in `j2r-runtime` (which owns the
  classfile decoder). The note already required the decode be safe-side.

## 6. Slot inventory (which of the 231 need the primitives)

Mechanically derived from `scripts/evidence/jni_slot_list.txt` (T08 —
indices below are read off the committed evidence file, not recalled):
`GetJavaVM` (219, out-param), `GetEnv`/`AttachCurrentThread`/
`AttachCurrentThreadAsDaemon` (VM table slots 4/7/6, out-params),
`DefineClass` (5), `NewString` (163), `NewStringUTF` (167),
`GetStringRegion` (220), `GetStringUTFRegion` (221),
`Set<Prim>ArrayRegion` × 8 (207–214, read caller buffer),
`Get<Prim>ArrayRegion` × 8 (199–206, write caller buffer),
`Get<Prim>ArrayElements` × 8 + `Release<Prim>ArrayElements` × 8
(183–190, 191–198 — release copy-back re-reads), plus
`ReleaseStringChars` (166)/`ReleaseStringUTFChars` (170), which are
safe-side (our buffer, registry-freed). Counted: 35 slots need the FFI
crate; 196 never do. The v1 surface (M5_PLAN §3.3) uses a subset; the
primitives above cover all 35, so no later wave revisits the crate.
