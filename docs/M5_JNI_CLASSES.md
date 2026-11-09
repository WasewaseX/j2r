# M5 wave 2 — T10 slice 3: the class/field/array surface (design note)

Same discipline as `M5_JNI_FFI.md`: the decision is written BEFORE the
code lands, and the note is the design of record. Scope is the
class-identity half of the v1 surface (M5_PLAN §3.3): `FindClass` (6),
`GetObjectClass` (31), `GetFieldID` (94), the instance accessors
(95–112), `GetStaticFieldID` (144), the static accessors (145–162), and
the array families (171–174, 175–178 `New*Array`, 183–198
`Get/Release*ArrayElements`, 199–206 `Get*ArrayRegion`, 207–214
`Set*ArrayRegion`, plus the critical pair 217/218). Explicitly OUT of
slice 3: `DefineClass`, method IDs / `Call*Method` (T11),
`RegisterNatives` (T12), the exception machinery and the UTF-16 string
family (GetStringLength/GetStringChars/ReleaseStringChars — registered
for the T13 harness), monitors, `ToReflectedField`/`FromReflectedField`.

## 1. The global-dispatch decision (the one slice 3 was gated on)

**Decision: one process-global `Dispatch` slot inside `j2r-runtime`,
installed by the generated program at startup.**

The fact that forces it: the 231 env slots are
`extern "system" fn(JNIEnv, ...)` — the jni.h ABI fixes the signatures,
so no context parameter can be threaded to a slot body, and `NativeFn`'s
`ctx: Dispatch` never reaches FFI entries. The runtime therefore gains
`jni::host_dispatch` — a `Mutex<Option<Dispatch>>`-backed accessor plus
`install_host_dispatch(d: Dispatch)`. The shape mirrors the generated
support crate's `dispatch_slot.rs` (`install`/`dispatch`, partition.rs
emission) — but it lives in the runtime because the RUNTIME needs to
read it, not the generated crate.

The install point is ONE emission in codegen's `main_run`, next to
`statics::init_pool`: `j2r_runtime::jni::install_host_dispatch(...)`.
Because the app crate's `j2r_dispatch()` is the composed dispatch in
partitioned builds (the support-crate slot is installed before Java code
runs), a single install covers both layouts. **Verification obligation
at implementation time**: the partition differential suite (forced k=2)
must stay green — that is the proof the composed dispatch reached the
JNI surface in the multi-crate layout.

Lock law unchanged (L-J1/L-J2/L-J3): the slot is read per call, never
held across heap work; install happens once at startup, before any
thread attaches.

## 2. jclass — a real jobject, not a new registry

**Decision: `jclass` is the tag of the interned `java/lang/Class`
mirror object** (`reflect::class_ref`). A Class mirror is a heap `JObj`
with `jclass = "java/lang/Class"` and `slots[0] = Value::Str(internal)`;
`reflect::class_name_of` recovers the internal name and PANICS on a
non-Class object (the T09 loud law — a foreign-side bug, never silent).

This is Temurin-faithful: a `jclass` IS a `jobject` there — it can be
compared, passed to `GetObjectClass`, stored in a global ref. Interning
gives identity semantics for free (`class_ref` is memoized). And the
whole class identity story reuses what M3 already shipped for
reflection; slice 3 adds no second class registry to keep coherent.

- `GetObjectClass(obj)`: null → JNI null (v1 gap: the spec's NPE shape
  needs the exception machinery — registered for T13). Otherwise read
  `o.jclass` (arrays included — an array's `jclass` IS its descriptor
  `[I`), `class_ref`, `new_local`, return the tag.
- `FindClass(name)`: `name` arrives in JNI form — internal `a/b/C`, or
  an array descriptor `[I`, `[Ljava/lang/String;`. Validation law:
  the class must EXIST — compiled (`ctx.class_info(name).is_some()`,
  `ctx.is_interface`, parents tables) or builtin (the builtin tables
  reflection already consults). A miss returns JNI null (the spec's
  `NoClassDefFoundError` shape — v1 gap, T13-registered); the lookup
  must not intern garbage names as Class mirrors, so the existence
  check PRECEDES `class_ref`. Primitives by bare name (`"int"`) are
  NOT classes in JNI — reject (null).

## 3. jfieldID — one bridge table, superclass walk, permanent IDs

**Decision: field IDs are tags into a dedicated `bridge`-style table**
(the slice-1 `utf_chars` registry pattern). `GetFieldID` /
`GetStaticFieldID` resolve `(owner, name, sig)` and mint the ID; the
same triple returns the SAME ID (Temurin behavior); IDs are permanent
for the process lifetime (JNI's own law — IDs do not move, and v1 never
frees them; registered, matches the oracle's semantics).

Resolution law: JNI `GetFieldID` searches INHERITED fields (unlike
`Class.getDeclaredField`, which is the M3 reflection precedent) — walk
the owner, then `ctx.parent_of` chain, first match wins, `is_static`
comes from the field's own declaration (the static/instance accessor
families must agree with it). Miss → JNI null (the spec's
`NoSuchFieldError` shape — v1 gap, T13-registered). The signature must
match exactly (JNI field IDs are name+descriptor pairs).

ID encoding: the tag cast to the `jfieldID` pointer shape — the same
int→ptr law as `jobject` (a foreign library never dereferences our
tags). A wrong/foreign ID is a LOUD registry miss (panics → abort
across the boundary), never silent misinterpretation.

## 4. The accessors — reuse the reflection path exactly

- Instance `Get*/Set*` (95–112): resolve ID → `(owner, slot, desc)`;
  receiver null → v1 null/no-op gap (T13); receiver class must be a
  subtype of the ID's owner (the `reflect::field_set` precedent —
  across the JNI boundary a wrong receiver is a foreign-side bug, so
  the violation panics loudly instead of corrupting; Temurin's own
  behavior here is UB). Read/write via `obj_get`/`obj_set` by
  `instance_slot`, boxed/unboxed by the FIELD descriptor with
  `box_by_desc`/`unbox_by_desc`/`prim_cook`.
- Static `Get*/Set*` (145–162): same ID resolution; values via
  `ctx.static_get`/`ctx.static_set` by `static_slot` (the one global
  pool per generated crate — absolute indices, already reflection-
  proven). The int-group law applies identically: boolean/byte/char/
  short ride `Value::Int` and narrow by the FIELD descriptor.
- Object-valued results come back as fresh local refs (`new_local`);
  object-typed writes take a jobject tag → resolve → the stored value
  is the arena `JRef` (Copy — storing copies is correct; the local ref
  dies at frame pop, the object does not).

## 5. Arrays — they are already heap objects

Array objects are `JObj`s (`slots[0]` = length, reference elements in
`slots[1..]`, primitive payloads in `PrimData`; `jclass` is the
descriptor). Mapping:

- `GetArrayLength` (171) → `array_length`; null array → 0 (T13 gap).
- `NewObjectArray` (172): element class FROM THE jclass argument via
  `class_name_of` (so a foreign lib can hand us any Class mirror,
  including builtin ones — `class_ref` interns those too); fill with
  `init` when non-null (resolve ONCE, store the `JRef` copies).
- `Get/SetObjectArrayElement` (173/174) → `array_load`/`array_store`
  (the store's `ArrayStoreException` check runs with the host
  dispatch). Java-observable failures (`ArrayIndexOutOfBoundsException`,
  `ArrayStoreException`) have no transport across the boundary until
  T13 — v1 aborts LOUDLY with the exception name in the message
  (registered gap; strictly better than Temurin's UB).
- `New<Prim>Array` (175–178, 179–182): `array_new_prim` with the
  per-slot descriptor (each slot IS one primitive — no descriptor
  parsing needed).
- `Get<Prim>ArrayRegion`/`Set<Prim>ArrayRegion` (199–206, 207–214):
  bulk copy through the raw accessors (`array_load_i/j/f/d`,
  `array_store_*_raw`), narrowing per the ARRAY descriptor; bounds
  checked up front — violation = the loud AIOOBE abort above.
- `Get<Prim>ArrayElements` (183–190): allocate a caller-owned copy
  (the arena never moves, but the COPY keeps release semantics
  uniform); `isCopy` written JNI_TRUE through audited primitive 7
  (`write_u8_out`). The registration records `(array JRef, len)` keyed
  by the handed-out address — a release for a pointer we did not hand
  out is a LOUD registry miss (the T09/slice-2 law, pinned again by a
  child-process test).
- `Release<Prim>ArrayElements` (191–198): mode law — `JNI_COMMIT` (1)
  copies back without freeing; `0` copies back and frees;
  `JNI_ABORT` (2) frees without copying back. Copy-back goes through
  the raw store accessors.
- `GetPrimitiveArrayCritical` (217) / `ReleasePrimitiveArrayCritical`
  (218): the existing critical registry (`bridge::critical_register`)
  — pinning is free because the arena never moves and payloads never
  reallocate (the documented M2 advantage).

## 6. What the codegen change is (and why the battery is mandatory)

Slice 3 is the first JNI slice that TOUCHES CODEGEN (the `main_run`
install emission). Therefore, unlike slice 2, the full differential
battery (diff_suite + cfg_equiv) is MANDATORY in the same landing —
no deferral argument exists. The partition differential (forced k=2)
is the multi-crate proof of the install path.

## 7. Evidence bar for slice 3

Unit tests: jclass round-trips (object → class → GetFieldID → access);
inherited-field resolution through the parent chain; ID identity (same
triple → same ID) and loud foreign-ID abort (child-process, the
slice-2 pattern); int-group narrowing per descriptor (Z/B/C/S ride
i32); region bounds; elements/release modes (COMMIT/ABORT/0) and the
loud foreign release; array class descriptors for GetObjectClass on
arrays; FindClass miss → null. Then: fmt, clippy `-D warnings`,
doc-count sync, full battery green (§6), boards + push.
