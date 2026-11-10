# M5 wave 2 — T12: RegisterNatives and the native-method round-trip
(design note)

Status: DESIGN OF RECORD for T12 (committed before any code — the
slice-3a/11a pattern). Read with M5_PLAN §3 (the acceptance bar:
"`RegisterNatives` round-trip demonstrated — a native fn bound from
Rust called from Java code"), M5_JNI_FFI.md, M5_JNI_CALLS.md and
M5_JNI_CLASSES.md (the registries this note builds beside).

## 1. The question, stated exactly

JNI's `RegisterNatives` (slot 215) lets native code bind
implementations to a class's `native` methods; `UnregisterNatives`
(216) clears them. A `native` method is one javac compiles with
`ACC_NATIVE` and NO `Code` attribute — there is no bytecode body to
translate, so j2r's generated lookup table cannot contain one today.
Two gaps must close together:

- the **JNI side**: the registry the two slots drive (bind / unbind /
  the JNI laws on mismatch and rebinding);
- the **call side**: how a Java-callable path (bytecode `invoke*` in
  compiled code, and the `Call*MethodA/V` families) reaches a bound
  native. Without the call side the registry is dead storage — the
  acceptance round-trip is the PROOF the two halves meet.

## 2. The binding shape (the v1 decision)

A binding is a **Rust function in the runtime's own method shape**:

```text
fn(Option<&JRef>, &[Value]) -> Result<Value, Throw>
```

— exactly the `MethodFn` shape the generated lookup and the
`Call*MethodA/V` dispatch already speak. Consequences, each honest:

- the round-trip ("bound from Rust, called from Java") is the v1
  acceptance and needs NO new FFI primitive: the registry stores the
  fn, the dispatch tail calls it like any other method fn. gate-10 is
  untouched by T12.
- a **foreign C fn pointer** (the T13 C-harness shape) is stored as
  the raw `fnPtr` the caller handed us, but CALLING it from our safe
  runtime cannot be done under `forbid(unsafe_code)` — the C native
  ABI is the bare/variadic form (M5_JNI_CALLS.md §2: no safe stable-
  Rust way to build that call). The note registers primitive 11
  (`call_foreign_native`: an audited, descriptor-driven call thunk in
  `j2r-jni-ffi`, the gate-10 count amended in the same commit — the
  11c precedent) as **T13's deliverable**, where the harness evidence
  forces its exact shape. Until then a foreign-shaped fnPtr is stored
  faithfully and a call through it LOUDLY aborts naming the missing
  primitive (never a silent wrong answer, never a fake success).
- `RegisterNatives` accepts BOTH shapes because JNI hands us an
  opaque `void*`; the Rust-shape adapter is installed by whoever binds
  (our tests, and later the emitted glue). The registry records which
  shape it was given and the call path refuses the foreign shape with
  the registered loud abort until primitive 11 lands.

## 3. The registry and the JNI laws (slot 215/216 semantics)

Per the JNI spec, for each `{name, sig, fnPtr}` triple of a
`RegisterNatives` call:

1. **The class must declare the method as `native`** with exactly that
   name+descriptor. A mismatch is `JNI_ERR` + a pending
   `java/lang/NoSuchMethodError` naming the method (the spec's own
   shape; today's registry facts come from `Dispatch.class_info`,
   whose `UserMethod` rows carry `is_static` — an `is_static == false`
   row cannot be a declared native unless we extend the facts; see §5
   for the metadata decision).
2. **Rebinding overrides** — a later `RegisterNatives` for the same
   triple replaces the earlier binding (the spec: "the native
   functions are linked... the JNI function table entry is replaced").
   No dedup-by-identity complaints; last writer wins.
3. **Class-scoped, key = (declaring class, name, descriptor)** —
   binding on a subclass does NOT shadow an inherited native (JNI
   binds per class; the method-ID ancestor walk is a Get*MethodID
   law, not a RegisterNatives law).
4. **`UnregisterNatives` clears ALL bindings of the class** and
   returns 0; unregistering a class with no bindings is still 0 (the
   spec gives no error for it). After unregister, a call to the
   native method reverts to the unbound law (below).
5. **Staticness**: the binding receives `None` as the receiver for a
   static native and `Some(&JRef)` otherwise — the same convention as
   the `Call*MethodA/V` families.
6. **The unbound law**: calling a `native` method with no binding
   pends `java/lang/UnsatisfiedLinkError` naming the method and
   answers the zero value (the oracle's shape; NEVER a silent zero
   without the throwable).

Storage lives in `jni::natives` beside the field/method registries:
one `Mutex<HashMap<(String, String, String), Binding>>` keyed by the
declaring class + name + descriptor. `Binding` = the fn payload in
one of the two shapes + the staticness recorded at bind time.

## 4. The call side (where bindings are consulted)

- **`Call*MethodA/V` and the compiled `invoke*` path**: a native
  method has no entry in the generated lookup, so the dispatch walk's
  MISS arm gains one consult BEFORE the loud miss pend: if the
  `(start_class_or_owner?, name, desc)` triple is bound, invoke the
  binding. The faithful consult key is the **declaring owner the
  walk is standing on when the lookup misses** — i.e. the walk checks
  the registry at EVERY class it visits (bind-on-Base resolves for a
  Derived receiver through the same walk, exactly like inherited
  compiled methods).
- **Codegen (the round-trip half)**: the emitter, for an
  `ACC_NATIVE` method, emits a body that calls
  `j2r_runtime::jni::natives::call_native(owner, name, desc, recv,
  args)` — the runtime fn applies §3's laws (registry consult →
  invoke, unbound → `UnsatisfiedLinkError`). No new dispatch
  machinery: the emitted body is one call into the runtime, the same
  shape as the catchable-SOE boundary bodies.
- **Argument marshalling**: identical to the A-form decode law
  (descriptor-driven, sub-word kinds in the low bytes) — the call
  sites already build `Vec<Value>`; the binding receives exactly
  that.

## 5. The metadata decision (how codegen knows a method is native)

`UserClassInfo.methods` (`UserMethod{name, desc, is_static}`) has no
native flag, and the generated `lookup` skips bodyless methods today.
Decision: **extend `UserMethod` with `is_native: bool`** (a
metadata-only amendment, defaulted `false` at every existing
construction site — the M3 reflection surface reads the same rows and
`getModifiers` may later expose it). The emitter writes `is_native`
from the classfile's `ACC_NATIVE` and (a) omits the method from the
lookup table (no body exists), (b) emits the §4 call-through body,
(c) `class_info` consumers (Get*MethodID's walk) still see the
declared method — which is what makes RegisterNatives' §3.1 check and
GetMethodID on a native method both work. This is the note's only
cross-crate (codegen) edit.

## 6. Evidence bar (acceptance)

Unit tests (runtime): bind/unbind/rebind laws; the
NoSuchMethodError-on-unknown-method law (name, descriptor, and
not-actually-native mismatches, each loud-pended + `JNI_ERR`); the
unbound-call `UnsatisfiedLinkError`; static-vs-instance receiver
shape; the dispatch-walk consult (bind on Base, call through a
Derived receiver). Codegen round-trip (the M5_PLAN acceptance): a
Java fixture class with a `native int` method + a caller, the binding
installed from Rust before `main_run`'s Java section, byte-identical
three-channel output vs Temurin with a Temurin-side C shim playing
the binding role — the harness parity point T13 generalizes. Then
fmt, clippy `-D warnings`, doc-count sync, full battery, boards + push.

## 7. Slice order

- **12a** — this note.
- **12b** — `jni::natives` registry + slots 215/216 + the unit-test
  battery + the dispatch-walk consult. LANDED. 12b precision of
  record (§2 shape law, refined): the C-array slot path stores
  FOREIGN fn pointers ONLY — the C ABI gives no way to distinguish a
  Rust `MethodFn` from a C fn, and reconstructing a Rust fn from a
  word would take an unsafe transmute the runtime forbids. Rust-shape
  bindings ride the Rust API (`bind`, used by the emitted glue and
  the tests); a Foreign binding call LOUDLY aborts naming T13's
  primitive 11.
- **12c** — the codegen half + the acceptance e2e. LANDED:
  `plan_methods` accepts `ACC_NATIVE` (no body) with the metadata row
  (`is_native: true`) and the emitted call-through body (FrameGuard +
  `call_native`); the verifier skips native plans (nothing to verify —
  the body is correct by construction); BOTH lookup layouts map the
  native triple to the call-through fn (a real `MethodFn` — the
  dispatch stays uniform; the 12b walk consult remains as the
  no-metadata fallback). The acceptance e2e (`tests/native_e2e.rs`)
  is a REAL three-channel differential: javac compiles the fixture,
  Temurin runs it (unbound native), j2r runs it, and stdout + exit +
  the stderr HEADER are byte-identical — our `UnsatisfiedLinkError`
  message reproduces the oracle's quoted Java declaration
  (`'int NativeProbe.add(int, int)'`, probed live 2026-11-10), which
  needed the linkage-error family in the builtin parent table (the
  builtin toString dispatch). The frame-level parity pin and the
  bound-C-native direction (primitive 11) belong to T13's C harness.

T13 keeps the C-harness bar and owns primitive 11 (the foreign-call
thunk) per §2.
