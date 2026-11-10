//! T09 — the JNI handle-table bridge (the SAFE half of the jobject ABI).
//!
//! Design of record: M5_PLAN §3.2 ("object model bridge") and
//! M5_JNI_FFI.md §4 ("the safe half of the bridge stays in
//! `j2r-runtime::jni` exactly as built in T08"). This module gives the
//! fabricated `jobject` values their meaning:
//!
//! * **A `jobject` is an integer tag, not a pointer.** Tag `0` is JNI
//!   null. Any other tag = `(entry_gen: u32) << 32 | (slot: u32) + 1`
//!   into ONE process-global handle table; the int→pointer cast at the
//!   ABI boundary is safe (`ptr as usize` / `int as ptr` — only a
//!   *dereference* would be unsafe, and no foreign code ever dereferences
//!   our tags: they are table keys, exactly the handle-tag pattern the
//!   design note pinned).
//! * **Generation-pinned at the boundary.** Each table slot carries its
//!   own generation, bumped when the slot is freed; a stale tag (foreign
//!   bug: use-after-Delete*) resolves to a generation mismatch and PANICS
//!   loudly — across the `extern "system"` boundary that is a process
//!   abort naming the failure (T08's loudness law). Silent aliasing to a
//!   recycled object is structurally impossible.
//! * **Every kind of reference is a GC root.** Local refs (per-thread
//!   frame stacks), global refs and the direct-critical registry are all
//!   scanned by [`scan_roots`] from the collector's root set
//!   (heap::collect root source 5) — pointer-free protocol, index push
//!   only. A ref held only by native code therefore survives collection
//!   exactly as long as the JNI contract says it must.
//! * **The heap is never touched under a bridge lock.** Lock laws:
//!   L-J1 order is `frames` → `table` (nobody takes them the other way
//!   round; `scan_roots` takes `table` alone); L-J2 no `safepoint_poll`,
//!   no collection, no Java dispatch and no heap access while either
//!   bridge lock is held — critical sections are short pure-Rust
//!   container ops, so a stop-the-world collection can never wait on a
//!   bridge lock chain (the M4 lesson, restated for the new locks).
//!
//! v1 simplifications (documented, none silent): `PushLocalFrame`'s
//! capacity is a hint — the table grows unbounded instead of failing at
//! 16 (C code relying on the capacity failure is out of scope until a
//! differential fixture forces the behavior); weak global refs stay loud
//! stubs (phantom semantics are their own task); thread detach is
//! explicit ([`detach_thread`]) — the attach lifecycle wires it in T13.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::thread::ThreadId;

use super::tables::{jobject, JObjectOpaque};
use super::{JNI_ERR, JNI_OK};
use crate::heap::JRef;

// ---------------------------------------------------------------- kinds

#[derive(Clone, Copy, Debug)]
enum Kind {
    /// A local reference: owned by one thread, freed when its frame pops
    /// (or by `DeleteLocalRef`).
    Local { owner: ThreadId },
    /// A global reference: valid from any thread until
    /// `DeleteGlobalRef`.
    Global,
}

struct Entry {
    jref: JRef,
    kind: Kind,
}

// ---------------------------------------------------------------- table

#[derive(Default)]
struct Table {
    slots: Vec<Option<Entry>>,
    /// Per-slot generation, bumped on every free — the stale-tag pin.
    gens: Vec<u32>,
    free: Vec<usize>,
}

impl Table {
    fn insert(&mut self, entry: Entry) -> (u32, u32) {
        let slot: usize = self.free.pop().unwrap_or_else(|| {
            self.slots.push(None);
            self.gens.push(0);
            self.slots.len() - 1
        });
        let gen = self.gens[slot];
        self.slots[slot] = Some(entry);
        (slot as u32, gen)
    }

    fn free_slot(&mut self, slot: u32) {
        let i = slot as usize;
        self.slots[i] = None;
        self.gens[i] = self.gens[i].wrapping_add(1);
        self.free.push(i);
    }
}

fn table() -> &'static Mutex<Table> {
    static TABLE: OnceLock<Mutex<Table>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(Table::default()))
}

// ---------------------------------------------------------------- frames

#[derive(Default)]
struct Frame {
    /// Tags created in this frame, creation order (the frame's own
    /// entries only — stale tags from earlier deletes may linger; they
    /// resolve to a freed slot and are skipped).
    tags: Vec<u64>,
}

fn frames() -> &'static Mutex<HashMap<ThreadId, Vec<Frame>>> {
    static FRAMES: OnceLock<Mutex<HashMap<ThreadId, Vec<Frame>>>> = OnceLock::new();
    FRAMES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn current_thread() -> ThreadId {
    std::thread::current().id()
}

/// Ensures the calling thread has a frame stack with a bottom frame
/// (the native entry frame). Returns the frames lock guard.
fn with_own_stack<R>(f: impl FnOnce(&mut Vec<Frame>) -> R) -> R {
    let mut map = frames().lock().unwrap_or_else(|e| e.into_inner());
    let stack = map.entry(current_thread()).or_default();
    if stack.is_empty() {
        stack.push(Frame::default());
    }
    f(stack)
}

// ------------------------------------------------------------- tag codec

fn encode(slot: u32, gen: u32) -> u64 {
    ((gen as u64) << 32) | (slot as u64 + 1)
}

fn decode(tag: u64) -> (u32, u32) {
    ((tag & 0xFFFF_FFFF) as u32 - 1, (tag >> 32) as u32)
}

/// The ABI boundary: a tag dresses up as a `jobject` (safe int→ptr cast;
/// foreign code never dereferences it — it is a table key).
pub fn tag_to_jobject(tag: u64) -> jobject {
    tag as *mut JObjectOpaque
}

/// The ABI boundary, inbound. JNI null (`0`) round-trips as `0`.
pub fn jobject_to_tag(p: jobject) -> u64 {
    p as usize as u64
}

// ------------------------------------------------------------ handle ops

/// `NewLocalRef`: roots `jref` for the calling thread's current frame.
pub fn new_local(jref: JRef) -> u64 {
    let owner = current_thread();
    with_own_stack(|stack| {
        let mut t = table().lock().unwrap_or_else(|e| e.into_inner());
        let (slot, gen) = t.insert(Entry {
            jref,
            kind: Kind::Local { owner },
        });
        let tag = encode(slot, gen);
        stack
            .last_mut()
            .expect("with_own_stack made a frame")
            .tags
            .push(tag);
        tag
    })
}

/// `NewGlobalRef`: roots `jref` until [`delete_global`].
pub fn new_global(jref: JRef) -> u64 {
    let mut t = table().lock().unwrap_or_else(|e| e.into_inner());
    let (slot, gen) = t.insert(Entry {
        jref,
        kind: Kind::Global,
    });
    encode(slot, gen)
}

/// `DeleteLocalRef`. JNI null is a legal no-op; anything else must be a
/// live local ref of the CALLING thread — a foreign cross-thread or
/// wrong-kind delete is a loud contract violation.
pub fn delete_local(tag: u64) {
    if tag == 0 {
        return;
    }
    let me = current_thread();
    let mut t = table().lock().unwrap_or_else(|e| e.into_inner());
    let (slot, gen) = decode(tag);
    let i = slot as usize;
    let ours = match t.slots.get(i) {
        Some(Some(Entry {
            kind: Kind::Local { owner },
            ..
        })) => t.gens[i] == gen && *owner == me,
        _ => false,
    };
    if ours {
        t.free_slot(slot);
    } else {
        stale_tag_panic("DeleteLocalRef", tag);
    }
}

/// `DeleteGlobalRef`. JNI null is a legal no-op; a local tag here is a
/// loud contract violation.
pub fn delete_global(tag: u64) {
    if tag == 0 {
        return;
    }
    let mut t = table().lock().unwrap_or_else(|e| e.into_inner());
    let (slot, gen) = decode(tag);
    let i = slot as usize;
    let ours = match t.slots.get(i) {
        Some(Some(Entry {
            kind: Kind::Global, ..
        })) => t.gens[i] == gen,
        _ => false,
    };
    if ours {
        t.free_slot(slot);
    } else {
        stale_tag_panic("DeleteGlobalRef", tag);
    }
}

/// Resolves any live tag (local or global) to its heap handle. JNI null
/// is the CALLER's business (every slot null-checks first per the JNI
/// spec) — resolving null here is a bridge bug and panics.
///
/// Stale/foreign/never-issued tags panic loudly (generation mismatch or
/// slot miss) — the boundary aliasing law.
pub fn resolve(tag: u64) -> JRef {
    if tag == 0 {
        panic!("j2r JNI bridge: resolve(null jobject) — slots must null-check first");
    }
    let (slot, gen) = decode(tag);
    let t = table().lock().unwrap_or_else(|e| e.into_inner());
    match t.slots.get(slot as usize) {
        Some(Some(entry)) if t.gens[slot as usize] == gen => entry.jref,
        _ => stale_tag_panic("resolve", tag),
    }
}

#[track_caller]
fn stale_tag_panic(op: &str, tag: u64) -> ! {
    panic!(
        "j2r JNI bridge: {op} on a stale/invalid handle (tag=0x{tag:016x}) — \
         never aliasing a recycled object; use-after-delete is a foreign-side bug"
    )
}

// --------------------------------------------------------- frame protocol

/// `PushLocalFrame`. `capacity < 0` is the spec's `JNI_ERR`; the capacity
/// itself is a v1 hint (module doc).
pub fn push_local_frame(capacity: i32) -> i32 {
    if capacity < 0 {
        return JNI_ERR;
    }
    with_own_stack(|stack| {
        stack.push(Frame::default());
        JNI_OK
    })
}

/// `PopLocalFrame`: frees every local ref created in the current frame;
/// `result` (a tag, `0` = JNI null) is returned as a local ref of the
/// parent frame — re-issued if it lived in the popped frame, unchanged
/// if it is an older local or a global. Popping past the bottom frame is
/// a fatal bridge misuse and panics (the spec gives it no return code).
pub fn pop_local_frame(result: u64) -> u64 {
    let popped_tags: Vec<u64> = with_own_stack(|stack| {
        if stack.len() < 2 {
            panic!(
                "j2r JNI bridge: PopLocalFrame with no pushed frame — \
                 fatal per the JNI spec (no return code exists)"
            );
        }
        stack.pop().expect("len >= 2").tags
    });
    let me = current_thread();
    let promoted: Option<JRef> = if result == 0 {
        None
    } else {
        let (slot, gen) = decode(result);
        let mut t = table().lock().unwrap_or_else(|e| e.into_inner());
        let i = slot as usize;
        let ours = match t.slots.get(i) {
            Some(Some(Entry {
                kind: Kind::Local { owner },
                ..
            })) => t.gens[i] == gen && *owner == me && popped_tags.contains(&result),
            _ => false,
        };
        if ours {
            let jref = t.slots[i].as_ref().expect("matched entry").jref;
            t.free_slot(slot);
            Some(jref)
        } else {
            None
        }
    };
    // Free the rest of the popped frame's locals (JNI: everything created
    // in the frame dies with it). The result's slot — if it was ours — is
    // already freed above; its generation moved, so the sweep skips it.
    if !popped_tags.is_empty() {
        let mut t = table().lock().unwrap_or_else(|e| e.into_inner());
        for &tag in &popped_tags {
            let (slot, gen) = decode(tag);
            let i = slot as usize;
            let ours = match t.slots.get(i) {
                Some(Some(Entry {
                    kind: Kind::Local { owner },
                    ..
                })) => t.gens[i] == gen && *owner == me,
                _ => false,
            };
            if ours {
                t.free_slot(slot);
            }
        }
    }
    match promoted {
        Some(jref) => new_local(jref), // re-issued in the parent frame
        None => result,                // older local or global: already valid
    }
}

/// Detach hook (wired by the T13 attach lifecycle): drops the calling
/// thread's frame stack, freeing every local ref it still holds. Global
/// refs are NOT affected.
pub fn detach_thread() {
    let popped_tags: Vec<u64> = {
        let mut map = frames().lock().unwrap_or_else(|e| e.into_inner());
        map.remove(&current_thread())
            .map(|stack| stack.into_iter().flat_map(|f| f.tags).collect())
            .unwrap_or_default()
    };
    if popped_tags.is_empty() {
        return;
    }
    let owner = current_thread();
    let mut t = table().lock().unwrap_or_else(|e| e.into_inner());
    for tag in popped_tags {
        let (slot, gen) = decode(tag);
        if let Some(Some(Entry {
            kind: Kind::Local { owner: o },
            ..
        })) = t.slots.get(slot as usize)
        {
            if *o == owner && t.gens[slot as usize] == gen {
                t.free_slot(slot);
            }
        }
    }
}

// ------------------------------------------------------ critical registry

fn critical() -> &'static Mutex<HashMap<usize, JRef>> {
    static CRITICAL: OnceLock<Mutex<HashMap<usize, JRef>>> = OnceLock::new();
    CRITICAL.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers a direct-critical region: the address we handed out maps to
/// the array's handle, which is a GC root until release. The arena never
/// moves objects and array payloads never reallocate (fixed length at
/// allocation), so the address stays valid for the critical region's
/// lifetime — the pinning is FREE in this runtime (M5_PLAN §3.2); the
/// registry exists so release verifies the pointer and the root is
/// explicit rather than an accident of scan coverage.
pub fn critical_register(addr: usize, jref: JRef) {
    let mut c = critical().lock().unwrap_or_else(|e| e.into_inner());
    if c.insert(addr, jref).is_some() {
        panic!("j2r JNI bridge: critical_register on an already-registered address 0x{addr:x}");
    }
}

/// `Release<Prim>ArrayElements`/`ReleasePrimitiveArrayCritical` plumbing:
/// unregisters the address. A miss is a foreign-side bug (double release
/// or a pointer we never handed out) — loud, never silent.
pub fn critical_release(addr: usize) {
    let mut c = critical().lock().unwrap_or_else(|e| e.into_inner());
    if c.remove(&addr).is_none() {
        panic!(
            "j2r JNI bridge: critical_release on an unregistered address 0x{addr:x} \
             — registry miss is a foreign-side bug"
        );
    }
}

// -------------------------------------------------- pending exceptions

/// The per-thread pending throwable (JNI §The Exception Process: a
/// pending exception is per-thread state, not a return value). T11
/// slice 11b — Call*Method is the first slot family that throws AND
/// continues: the call returns the zero value, the throwable waits in
/// this cell, and `ExceptionOccurred`/`ExceptionCheck`/`ExceptionClear`
/// (slots 15/228/17) report and clear it.
fn pending() -> &'static Mutex<HashMap<ThreadId, Option<crate::Throw>>> {
    static PENDING: OnceLock<Mutex<HashMap<ThreadId, Option<crate::Throw>>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Records the throwable as pending on the CURRENT thread (replacing
/// any previous one — the JNI law for nested throws).
pub fn pend_exception(t: crate::Throw) {
    let mut map = pending().lock().unwrap_or_else(|e| e.into_inner());
    map.insert(current_thread(), Some(t));
}

/// Takes the pending throwable off the current thread (the
/// `ExceptionClear` shape: report-and-clear can't share one call, so
/// `ExceptionOccurred` CLONES the handle out and the caller clears).
pub fn take_pending_exception() -> Option<crate::Throw> {
    let mut map = pending().lock().unwrap_or_else(|e| e.into_inner());
    map.remove(&current_thread()).flatten()
}

/// Is a throwable pending on the current thread? (`ExceptionCheck`.)
pub fn has_pending_exception() -> bool {
    pending()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&current_thread())
        .is_some_and(|t| t.is_some())
}

/// The pending throwable as a FRESH local ref (`ExceptionOccurred`
/// returns a new local ref to the same object each call — JNI-faithful),
/// leaving the cell untouched.
pub fn pending_exception_local() -> Option<u64> {
    // Clone the Throw out (the cell is untouched — ExceptionOccurred is
    // repeatable), then mint a FRESH local ref from its heap object.
    let t = pending()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&current_thread())
        .and_then(|opt| opt.clone())?;
    Some(new_local(t.obj))
}

// ------------------------------------------------------------- GC roots

/// Collector hook (heap::collect root source 5): every live entry —
/// locals of every thread (including detached-but-unfreed over-
/// approximation residue), globals, critical regions — pushes its arena
/// index. Pointer-free protocol: indices only, no heap access (L-J2).
pub(crate) fn scan_roots(out: &mut Vec<u32>) {
    {
        let t = table().lock().unwrap_or_else(|e| e.into_inner());
        for entry in t.slots.iter().flatten() {
            out.push(entry.jref.index());
        }
    }
    let c = critical().lock().unwrap_or_else(|e| e.into_inner());
    for jref in c.values() {
        out.push(jref.index());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{new_obj, obj_get, obj_set};
    use crate::Value;

    fn obj(n: i32) -> JRef {
        let r = new_obj("test/JniBridge", 1).expect("alloc test object");
        obj_set(&r, 0, Value::Int(n));
        r
    }

    fn payload(tag: u64) -> i32 {
        let r = resolve(tag);
        obj_get(&r, 0).expect_int()
    }

    #[test]
    fn tag_roundtrip_and_abi_boundary() {
        let _rt = crate::test_support::runtime();
        let r = obj(7);
        let tag = new_local(r);
        assert_ne!(tag, 0, "a non-null tag is never 0 — JNI null is reserved");
        assert_eq!(payload(tag), 7);
        // the ABI casts are lossless both ways; null stays null
        let j = tag_to_jobject(tag);
        assert_eq!(jobject_to_tag(j), tag);
        assert_eq!(jobject_to_tag(tag_to_jobject(0)), 0);
        delete_local(tag);
    }

    #[test]
    fn global_ref_is_resolvable_and_deletable() {
        let _rt = crate::test_support::runtime();
        let tag = new_global(obj(9));
        assert_eq!(payload(tag), 9);
        delete_global(tag);
    }

    #[test]
    fn deleted_tag_is_loud_never_aliased() {
        let _rt = crate::test_support::runtime();
        let a = new_local(obj(1));
        delete_local(a);
        // slot reuse must hand out a DIFFERENT tag (generation bumped)
        let b = new_local(obj(2));
        assert_ne!(a, b, "recycled slot must not reissue the old tag value");
        assert_eq!(payload(b), 2);
        // the stale tag resolves to a generation mismatch: loud panic
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = payload(a);
        }))
        .is_err();
        assert!(panicked, "stale tag must panic, never silently alias");
        delete_local(b);
    }

    #[test]
    fn wrong_kind_delete_is_loud() {
        let _rt = crate::test_support::runtime();
        let local = new_local(obj(3));
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            delete_global(local);
        }))
        .is_err();
        assert!(panicked, "DeleteGlobalRef on a local tag must panic");
        delete_local(local);
    }

    #[test]
    fn push_pop_frame_promotes_the_result() {
        let _rt = crate::test_support::runtime();
        let outer = new_local(obj(11));
        assert_eq!(push_local_frame(16), JNI_OK);
        let inner = new_local(obj(12));
        let promoted = pop_local_frame(inner);
        assert_ne!(
            promoted, inner,
            "promoted result is re-issued in the parent frame"
        );
        assert_eq!(payload(promoted), 12, "promoted result stays live");
        assert_eq!(payload(outer), 11, "outer frame untouched");
        // a global result passes through unchanged
        let g = new_global(obj(13));
        assert_eq!(push_local_frame(0), JNI_OK);
        assert_eq!(pop_local_frame(g), g, "global result is returned unchanged");
        delete_global(g);
        // popping the bottom frame is fatal
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = pop_local_frame(0);
        }))
        .is_err();
        assert!(panicked, "PopLocalFrame past the bottom frame must panic");
        delete_local(outer);
        delete_local(promoted);
    }

    #[test]
    fn pop_frees_non_result_locals_of_the_popped_frame() {
        let _rt = crate::test_support::runtime();
        assert_eq!(push_local_frame(0), JNI_OK);
        let discarded = new_local(obj(14));
        let kept = pop_local_frame(0);
        assert_eq!(kept, 0, "null result pops to null");
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = payload(discarded);
        }))
        .is_err();
        assert!(
            panicked,
            "locals of the popped frame are freed — stale use is loud"
        );
    }

    #[test]
    fn negative_capacity_is_jni_err() {
        assert_eq!(push_local_frame(-1), JNI_ERR);
    }

    #[test]
    fn local_refs_survive_gc_then_die_with_the_frame() {
        let _rt = crate::test_support::runtime();
        let tag = new_local(obj(15));
        crate::heap::force_collect_for_test();
        assert_eq!(payload(tag), 15, "a live local ref is a GC root");
        // detach frees it: after detach the tag is stale
        detach_thread();
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = payload(tag);
        }))
        .is_err();
        assert!(
            panicked,
            "detached thread's locals are freed — stale use is loud"
        );
    }

    #[test]
    fn global_refs_survive_gc() {
        let _rt = crate::test_support::runtime();
        let tag = new_global(obj(16));
        crate::heap::force_collect_for_test();
        assert_eq!(payload(tag), 16, "a global ref is a GC root");
        delete_global(tag);
    }

    #[test]
    fn only_rooted_objects_survive_when_the_ref_dies() {
        let _rt = crate::test_support::runtime();
        // The object exists ONLY behind the local ref: collect with the
        // ref live, then delete and collect again.
        let tag = new_local(obj(17));
        crate::heap::force_collect_for_test();
        assert_eq!(payload(tag), 17);
        delete_local(tag);
        crate::heap::force_collect_for_test(); // must not trip or alias
    }

    #[test]
    fn critical_registry_roundtrip_and_loud_miss() {
        let _rt = crate::test_support::runtime();
        let r = obj(18);
        let addr = 0x1234_5600usize; // bookkeeping key; no memory behind it in this test
        critical_register(addr, r);
        let panicked = std::panic::catch_unwind(|| critical_release(0xDEAD)).is_err();
        assert!(panicked, "registry miss must be loud");
        critical_release(addr); // second release would panic — verified shape above
        let panicked = std::panic::catch_unwind(|| critical_release(addr)).is_err();
        assert!(panicked, "double release must be loud");
    }

    #[test]
    fn scan_roots_covers_locals_globals_and_criticals() {
        let _rt = crate::test_support::runtime();
        let r = obj(19);
        let l = new_local(r);
        let g = new_global(r);
        critical_register(0x42_00usize, r);
        let mut roots: Vec<u32> = Vec::new();
        scan_roots(&mut roots);
        let idx = r.index();
        let hits = roots.iter().filter(|&&i| i == idx).count();
        assert!(
            hits >= 3,
            "local + global + critical must all be scanned, got {hits}"
        );
        delete_local(l);
        delete_global(g);
        critical_release(0x42_00);
    }

    #[test]
    fn same_thread_delete_is_the_v1_contract() {
        // v1 honesty: cross-thread LOCAL use is a JNI contract violation we
        // do not police at resolve() time (any thread may resolve any live
        // tag — HotSpot's own debug builds only assert). What IS pinned:
        // delete_local refuses a foreign owner. Registered as the T13
        // attach-lifecycle check item.
        let _rt = crate::test_support::runtime();
        let tag = new_local(obj(20));
        delete_local(tag); // same thread: fine
    }
}
