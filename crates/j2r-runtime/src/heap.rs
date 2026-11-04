//! The tracing, cycle-collecting arena heap — Wave 2 of M2
//! (`docs/M2_PLAN.md` §4).
//!
//! Architecture (settled in the M2 plan review):
//!
//! * **Non-moving mark-and-sweep arena.** Objects live in `Vec<Option<Entry>>`
//!   at stable addresses for their whole lifetime; `jclass` strings, slot
//!   vectors and `Value::Str` payloads behave exactly as before.
//! * **Generation-tagged Copy handles.** A [`JRef`] is an 8-byte
//!   `(idx, gen)` pair. When sweep frees a slot it bumps that slot's
//!   generation; a handle whose generation no longer matches is stale, and
//!   *using* it panics loudly in all builds
//!   (`j2r: stale JRef (idx=.., gen=..) — GC rooting bug`). Silent aliasing
//!   onto a recycled object is impossible by construction (M2_PLAN §4.1).
//! * **Zero `unsafe`.** Marking uses an explicit `Vec<u32>` worklist (never
//!   recursion — a 1 M-node list must not overflow the native stack);
//!   sweeping rebuilds the free list; mark bits are `Cell<bool>` beside the
//!   object's `RefCell`, so mark writes never conflict with object borrows.
//! * **One collection point.** Only [`gc_point`] may collect, and generated
//!   code calls it only immediately before the four allocating opcodes
//!   (`new`, `newarray`, `anewarray`, `multianewarray`). `alloc` never
//!   collects, so natives and runtime helpers can allocate freely without
//!   ever observing a collection (M2_PLAN §4.3).
//! * **Roots** (M2_PLAN §4.4, pointer-free): every registered frame's owned
//!   snapshot slot (refreshed by generated code before every call and at
//!   every collection point — `frame::FrameGuard::refresh`), the generated
//!   statics pool (registered via [`set_statics_scanner`]), and the runtime
//!   roots (intern cache, boxed-identity cache, `System.out`). A stale
//!   index found while marking is skipped silently — it is over-
//!   approximation residue, and any *use* of such a handle still panics at
//!   access time, so rooting bugs surface loudly in tests.
//!
//! Identity: `JRef` equality is `(idx, gen)` pair equality; for a live
//! object `gen` never advances (a rooted entry never re-enters the free
//! list), so this is exactly the old reference equality.
//!
//! **M4 Wave 1 — the heap is process-global.** One [`Mutex`] guards the
//! whole arena (`HEAP` below; `heap()` is the accessor). The lock-ordering
//! law that keeps this deadlock-free (M4_PLAN §3 Wave 1, enforced by
//! review + the suite):
//!
//! * **L1 — the heap lock is a leaf.** While a heap guard is alive, no
//!   code may lock another runtime container (caches, statics pool, frame
//!   registry). The collector honors this by construction: the root scan
//!   is pointer-free and runs *before* the lock is taken.
//! * **L2 — caches may nest heap locks, never the reverse.** `intern`,
//!   the boxed-identity cache and the Class-object cache allocate while
//!   holding their own lock (the same nesting shape the `RefCell` era
//!   had). A cache lock must never be acquired while a heap guard lives.
//! * **L3 — no heap re-entry.** A `with_obj`/`with_obj_mut` closure must
//!   not call back into the heap (`alloc`, nested `with_obj`, …): the
//!   RefCell era allowed nested *shared* borrows, a `Mutex` deadlocks on
//!   ANY re-entry. A full census (M4_EVIDENCE §migration) found four
//!   pre-existing nested sites; all were restructured flat, and the
//!   suite (253 tests + 84 diff fixtures + corpus) is the tripwire. Wave
//!   2's scheduler must additionally never hold a guard while parking.

use crate::object::JObj;
use std::cell::{Cell, RefCell};
use std::num::NonZeroU32;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};

/// Copy object handle: 1-based arena index + generation tag.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct JRef {
    idx: NonZeroU32,
    gen: u32,
}

impl JRef {
    /// Reference identity (`if_acmpeq`, `Object.equals`): `(idx, gen)` pair
    /// equality. Kept as an associated function so call sites are unchanged
    /// from the `Rc` implementation.
    pub fn ptr_eq(a: &JRef, b: &JRef) -> bool {
        a == b
    }

    /// The handle's arena index (root-scan and identity-hash plumbing).
    pub fn index(&self) -> u32 {
        self.idx.get()
    }

    pub(crate) fn from_parts(idx: u32, gen: u32) -> JRef {
        JRef {
            idx: NonZeroU32::new(idx).expect("j2r internal: JRef index 0"),
            gen,
        }
    }
}

struct Entry {
    obj: RefCell<JObj>,
    mark: Cell<bool>,
    /// Intrinsic monitor (M3 Wave 4): re-entrancy count, 0 = unlocked.
    /// The runtime is single-threaded, so a nonzero count means the one
    /// thread owns it; `monitorexit` without ownership is
    /// `IllegalMonitorStateException`, exactly HotSpot's behavior.
    monitor: Cell<u32>,
}

struct Heap {
    /// Slot `i` is `Some` while an object occupies it.
    entries: Vec<Option<Entry>>,
    /// Generation per slot; survives frees (sweep bumps it, which is what
    /// invalidates outstanding handles).
    gens: Vec<u32>,
    /// Free slot indices (0-based), LIFO.
    free: Vec<u32>,
    live: usize,
    since_gc: usize,
    live_at_last_gc: usize,
    high_water: usize,
    total_collections: usize,
    collecting: bool,
    /// `J2R_GC_EVERY_N`: forced collection at every Nth collection point.
    forced: Option<u32>,
    forced_counter: u32,
}

impl Heap {
    fn new() -> Heap {
        Heap {
            entries: Vec::new(),
            gens: Vec::new(),
            free: Vec::new(),
            live: 0,
            since_gc: 0,
            live_at_last_gc: 0,
            high_water: 0,
            total_collections: 0,
            collecting: false,
            forced: read_forced_env(),
            forced_counter: 0,
        }
    }

    fn threshold(&self) -> usize {
        (2 * self.live_at_last_gc).max(4096)
    }
}

fn read_forced_env() -> Option<u32> {
    std::env::var("J2R_GC_EVERY_N")
        .ok()
        .and_then(|v| v.parse().ok())
}

/// Root-scanner signature: collects the indices of the roots it owns.
pub type StaticsScanner = fn(&mut Vec<u32>);

/// The process-global heap (M4 Wave 1, T02). `OnceLock` because `Heap::new`
/// reads `J2R_GC_EVERY_N` (not const); cross-thread sharing because Wave 2
/// runs Java threads.
///
/// `Mutex` (the plan's preferred shape): a static requires `Sync`, which
/// rules out a bare `RefCell` container, and the nested-shared-borrow
/// patterns the RefCell era allowed were censused (M4_EVIDENCE §migration)
/// down to four sites, all restructured flat. See the lock law L1–L3 in
/// the module docs.
static HEAP: OnceLock<Mutex<Heap>> = OnceLock::new();

/// Registered by the generated `main_run`: scans the statics pool into
/// the root vector.
static STATICS_SCANNER: Mutex<Option<StaticsScanner>> = Mutex::new(None);

fn heap() -> &'static Mutex<Heap> {
    HEAP.get_or_init(|| Mutex::new(Heap::new()))
}

/// Allocates an object; **never collects** (M2_PLAN §4.3 — natives and
/// runtime helpers may allocate freely).
///
/// M4 Wave 2 (T08): allocations that race a stop-the-world request window
/// are **born survivors** — the entry is inserted with its mark bit set
/// when `GC_REQUESTED` was observed under the heap lock, so the imminent
/// sweep keeps it. This closes the last allocation/GC race: a thread that
/// allocates between its failed safepoint poll and the collector's sweep
/// can never hold a handle to a swept object. The extra kept object is
/// conservative garbage at worst — the *next* collection reclaims it if
/// it stayed unrooted (identical output either way; GC is invisible to
/// program semantics).
pub fn alloc(obj: JObj) -> JRef {
    let heap = &mut *heap().lock().unwrap_or_else(|e| e.into_inner());
    debug_assert!(!heap.collecting, "alloc during collection");
    // Born-survivor marker: a GC is requested (or in its wait phase) — the
    // coming sweep must keep this object (its allocating thread may not
    // have stored the handle into a refreshed frame slot yet).
    let born_survivor = crate::thread::GC_REQUESTED.load(Ordering::Acquire);
    heap.since_gc += 1;
    let (idx, gen) = match heap.free.pop() {
        Some(i) => {
            let i = i as usize;
            heap.entries[i] = Some(Entry {
                obj: RefCell::new(obj),
                mark: Cell::new(born_survivor),
                monitor: Cell::new(0),
            });
            (i, heap.gens[i])
        }
        None => {
            heap.entries.push(Some(Entry {
                obj: RefCell::new(obj),
                mark: Cell::new(born_survivor),
                monitor: Cell::new(0),
            }));
            heap.gens.push(0);
            (heap.entries.len() - 1, 0)
        }
    };
    heap.live += 1;
    heap.high_water = heap.high_water.max(heap.live);
    JRef::from_parts(idx as u32 + 1, gen)
}

// ------------------------------------------------------------------ monitors

/// `monitorenter`: re-entrant acquisition (M3 Wave 4). The runtime is
/// single-threaded, so there is no contention path; the only failures are
/// the null receiver (NPE at the caller) and counter overflow (internal
/// error — a u32 recursion depth in one object is not reachable by any
/// real program given the frame ceiling).
pub fn monitor_enter(r: &JRef) -> Result<(), crate::Throw> {
    let heap = &mut *heap().lock().unwrap_or_else(|e| e.into_inner());
    let i = live_slot_index(heap, *r);
    let e = heap.entries[i].as_ref().expect("live entry");
    let n = e.monitor.get();
    e.monitor.set(
        n.checked_add(1)
            .expect("j2r internal: monitor count overflow"),
    );
    Ok(())
}

/// `monitorexit`: releases one level of ownership. Exiting a monitor the
/// thread does not hold throws `IllegalMonitorStateException` with
/// HotSpot's message.
pub fn monitor_exit(r: &JRef) -> Result<(), crate::Throw> {
    // The IMSE object is allocated AFTER the borrow ends: `throw_new`
    // itself allocates, and the heap RefCell must not be re-entered.
    let unowned = {
        let heap = &mut *heap().lock().unwrap_or_else(|e| e.into_inner());
        let i = live_slot_index(heap, *r);
        let e = heap.entries[i].as_ref().expect("live entry");
        let n = e.monitor.get();
        if n == 0 {
            true
        } else {
            e.monitor.set(n - 1);
            false
        }
    };
    if unowned {
        return Err(crate::throw_new(
            "java/lang/IllegalMonitorStateException",
            Some("current thread not owner"),
        ));
    }
    Ok(())
}

/// Unwind-release (FrameGuard `Drop` safety net): decrements without the
/// ownership check, because the frame recorded the acquisition. Debug-
/// asserted to never underflow.
pub(crate) fn monitor_release(r: &JRef) {
    let heap = &mut *heap().lock().unwrap_or_else(|e| e.into_inner());
    let i = live_slot_index(heap, *r);
    let e = heap.entries[i].as_ref().expect("live entry");
    let n = e.monitor.get();
    debug_assert!(n >= 1, "j2r internal: monitor release underflow");
    e.monitor.set(n.saturating_sub(1));
}

/// Current monitor count (test/observation helper).
#[cfg(test)]
pub(crate) fn monitor_count(r: &JRef) -> u32 {
    let heap = heap().lock().unwrap_or_else(|e| e.into_inner());
    let i = live_slot_index(&heap, *r);
    heap.entries[i].as_ref().expect("live entry").monitor.get()
}

fn live_slot_index(heap: &Heap, r: JRef) -> usize {
    let i = (r.idx.get() - 1) as usize;
    let live = matches!(heap.entries.get(i), Some(Some(_))) && heap.gens[i] == r.gen;
    if !live {
        panic!(
            "j2r: stale JRef (idx={}, gen={}) — GC rooting bug",
            r.idx.get(),
            r.gen
        );
    }
    i
}

/// Scoped shared access to an object (replaces the old `r.borrow()` —
/// a `Ref` must never escape the heap lock guard, M2_PLAN §4.7 + M4 L3:
/// the closure must not re-enter the heap).
pub fn with_obj<T>(r: JRef, f: impl FnOnce(&JObj) -> T) -> T {
    let heap = &mut *heap().lock().unwrap_or_else(|e| e.into_inner());
    let i = live_slot_index(heap, r);
    let entry = heap.entries[i].as_ref().expect("checked live");
    let result = f(&entry.obj.borrow());
    result
}

/// Scoped exclusive access to an object (replaces `r.borrow_mut()`).
pub fn with_obj_mut<T>(r: JRef, f: impl FnOnce(&mut JObj) -> T) -> T {
    let heap = &mut *heap().lock().unwrap_or_else(|e| e.into_inner());
    let i = live_slot_index(heap, r);
    let entry = heap.entries[i].as_mut().expect("checked live");
    let result = f(&mut entry.obj.borrow_mut());
    result
}

/// True when the handle names a live object (test/observation helper).
pub fn is_live(r: JRef) -> bool {
    let heap = heap().lock().unwrap_or_else(|e| e.into_inner());
    let i = (r.idx.get() - 1) as usize;
    matches!(heap.entries.get(i), Some(Some(_))) && heap.gens[i] == r.gen
}

/// Peak number of simultaneously occupied slots (evidence metric, T17).
pub fn high_water() -> usize {
    heap().lock().unwrap_or_else(|e| e.into_inner()).high_water
}

/// Currently occupied slots (evidence metric).
pub fn live_count() -> usize {
    heap().lock().unwrap_or_else(|e| e.into_inner()).live
}

/// Collections run so far (evidence metric).
pub fn total_collections() -> usize {
    heap()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .total_collections
}

/// Registers the generated crate's statics-pool root scanner. Called by the
/// generated `main_run` before any Java code runs.
pub fn set_statics_scanner(f: fn(&mut Vec<u32>)) {
    *STATICS_SCANNER.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
}

/// `System.gc()`: a full collection at an explicit native boundary. Safe
/// because natives hold no live heap borrows across the call and generated
/// code refreshes its frame snapshot at every call boundary (M2_PLAN §4.4).
pub fn gc_now() {
    collect();
}

/// Test/diagnostic hook: force a collection at every Nth collection point
/// (`N = 1` = collect at every collection point). Overrides the
/// `J2R_GC_EVERY_N` env var.
pub fn set_forced_gc_every(n: Option<u32>) {
    let mut heap = heap().lock().unwrap_or_else(|e| e.into_inner());
    heap.forced = n;
    heap.forced_counter = 0;
}

/// The one collection point in the entire system (M2_PLAN §4.3). Generated
/// code emits `__frame.refresh(..); heap::gc_point();` immediately before
/// each of the four allocating opcodes; nothing else ever calls this.
///
/// M4 Wave 2 (T08) — the point is now a full safepoint poll:
///
/// 1. `safepoint_poll()` — park here if a stop-the-world collection is in
///    flight (before ANY runtime lock, law L6; the frame snapshot was
///    refreshed immediately before this call by generated code).
/// 2. the threshold check (unchanged; brief heap lock).
/// 3. if a collection is due: win the collect token, publish
///    `GC_REQUESTED`, wait for every other thread to park or exit, run
///    the mark/sweep, release everyone (the `StwGuard` drop).
pub fn gc_point() {
    crate::thread::safepoint_poll();
    let should = {
        let heap = &mut *heap().lock().unwrap_or_else(|e| e.into_inner());
        debug_assert!(!heap.collecting, "gc_point re-entered during collection");
        match heap.forced {
            Some(n) => {
                heap.forced_counter += 1;
                if heap.forced_counter >= n {
                    heap.forced_counter = 0;
                    true
                } else {
                    false
                }
            }
            None => heap.since_gc > heap.threshold(),
        }
    };
    if should {
        if let Some(_stw) = crate::thread::StwGuard::begin() {
            collect();
        }
        // Whether we collected or lost the token race, a collection just
        // ran or is running: poll again so this thread parks through any
        // in-flight one before its allocating opcode proceeds.
        crate::thread::safepoint_poll();
    }
}

/// Marks from the root set with an explicit worklist, then sweeps.
/// Runs ONLY inside a `StwGuard` (every other thread parked or exited).
fn collect() {
    // ---- root set (pointer-free; M2_PLAN §4.4) ------------------------
    let mut roots: Vec<u32> = Vec::new();
    // 1. every registered thread's frame snapshot slots (T08: the STW
    //    protocol guarantees no owner mutates its slots during this walk;
    //    the collector's own frames included)
    crate::frame::for_each_live_slot(|slot| roots.extend_from_slice(slot));
    // 2. the generated statics pool — the fn pointer is copied out under
    //    the scanner lock and called with NO lock held (L1: the scan is
    //    pointer-free and lock-free by design)
    let scanner = *STATICS_SCANNER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(f) = scanner {
        f(&mut roots);
    }
    // 3. runtime roots (intern cache, System.out, boxed-identity cache,
    //    interned Class objects — M3_PLAN §8.1)
    crate::object::scan_runtime_roots(&mut roots);
    crate::natives::scan_runtime_roots(&mut roots);
    crate::reflect::scan_roots(&mut roots);
    // 4. the thread registry (T07/T08): every record's Java Thread object
    crate::thread::scan_registry_roots(&mut roots);

    {
        let heap = &mut *heap().lock().unwrap_or_else(|e| e.into_inner());
        heap.collecting = true;
        heap.total_collections += 1;

        // ---- mark (iterative; stale root indices are tolerated here:
        // they are over-approximation residue and may name a freed slot
        // (skipped) or even a recycled slot (kept alive — over-approximation
        // is always safe). Any *use* of a stale handle still panics at
        // access time, so rooting bugs surface loudly in tests.) --------
        let mut worklist: Vec<u32> = roots;
        while let Some(idx0p1) = worklist.pop() {
            let i = (idx0p1 - 1) as usize;
            let Some(entry) = heap.entries.get(i).and_then(|s| s.as_ref()) else {
                continue; // freed slot: stale root residue
            };
            if entry.mark.get() {
                continue;
            }
            entry.mark.set(true);
            // push children: every Ref(Some(h)) in the object's slots
            for v in entry.obj.borrow().slots.iter() {
                if let crate::Value::Ref(Some(h)) = v {
                    worklist.push(h.index());
                }
            }
        }

        // ---- sweep -----------------------------------------------------
        let mut freed = 0usize;
        for i in 0..heap.entries.len() {
            match &mut heap.entries[i] {
                Some(e) if e.mark.get() => e.mark.set(false),
                Some(_) => {
                    heap.entries[i] = None;
                    heap.gens[i] = heap.gens[i].wrapping_add(1);
                    heap.free.push(i as u32);
                    freed += 1;
                }
                None => {}
            }
        }
        heap.live -= freed;
        heap.collecting = false;
        heap.since_gc = 0;
        heap.live_at_last_gc = heap.live;
    }
}

/// Test-only hook: runs one full mark/sweep through the real STW protocol
/// (the same path `gc_point` takes when the threshold fires). Unit tests
/// use this to drive collections directly. Callers must NOT already hold
/// a `StwGuard` (the begin inside would park on its own request — use
/// [`collect_under_held_stw_for_test`] there).
#[cfg(test)]
pub(crate) fn force_collect_for_test() {
    if let Some(_stw) = crate::thread::StwGuard::begin() {
        collect();
    }
    crate::thread::safepoint_poll();
}

/// Test-only hook: the raw mark/sweep, for callers already inside their
/// own `StwGuard` (the STW handshake test drives protocol + collection in
/// one held round).
#[cfg(test)]
pub(crate) fn collect_under_held_stw_for_test() {
    collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T05: the shared heap container is compile-proven Send+Sync — the
    /// property Wave 2's real threads depend on (M4_PLAN §3 Wave 1).
    #[test]
    fn shared_heap_container_is_send_sync() {
        let _rt = crate::test_support::runtime();
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Mutex<Heap>>();
        assert_send_sync::<std::sync::Mutex<Option<StaticsScanner>>>();
    }

    use crate::object::{new_obj, new_string, obj_get, obj_set, string_value, JObj};
    use crate::Value;
    use std::sync::Arc;

    fn obj(class: &str, slots: Vec<Value>) -> JRef {
        alloc(JObj {
            jclass: Arc::from(class),
            slots,
        })
    }

    /// Keeps handles alive across collections exactly like native code
    /// holding refs across a user dispatch: a `NativeRoots` scope.
    fn rooted<F: FnOnce()>(handles: &[JRef], f: F) {
        let _nr = crate::frame::NativeRoots::push(handles);
        f();
    }

    #[test]
    fn alloc_and_access_roundtrip() {
        let _rt = crate::test_support::runtime();
        let r = new_obj("test/A", 3).unwrap();
        obj_set(&r, 1, Value::Int(42));
        assert_eq!(obj_get(&r, 1).expect_int(), 42);
        assert!(is_live(r));
    }

    #[test]
    fn unreachable_object_is_collected() {
        let _rt = crate::test_support::runtime();
        set_forced_gc_every(Some(1));
        // M4 Wave 1: the heap is shared across the test binary, so a
        // collection also sweeps unrooted residue from other tests —
        // `live` deltas are not a stable invariant. The intent lives on
        // the handle: an unreachable object must be STALE after gc_point.
        let dead = new_string("short-lived-gc-test");
        assert!(is_live(dead), "fresh object is live");
        gc_point();
        assert!(
            !is_live(dead),
            "unreachable object must be freed (stale handle)"
        );
        set_forced_gc_every(None);
    }

    #[test]
    fn rooted_object_survives_forced_collections() {
        let _rt = crate::test_support::runtime();
        set_forced_gc_every(Some(1));
        let keep = new_string("kept-alive");
        let gen_before = keep.gen;
        rooted(&[keep], || {
            for _ in 0..100 {
                let _churn = new_string("churn");
                gc_point();
            }
        });
        assert!(is_live(keep), "rooted object must survive collections");
        assert_eq!(keep.gen, gen_before, "live object's generation is stable");
        assert_eq!(
            &*string_value(&keep),
            "kept-alive",
            "surviving object's payload must be intact"
        );
        set_forced_gc_every(None);
    }

    #[test]
    fn simple_cycle_is_collected() {
        let _rt = crate::test_support::runtime();
        set_forced_gc_every(Some(1));
        // M4 Wave 1: asserted on handle staleness (see
        // unreachable_object_is_collected) — shared-heap residue makes
        // live-count deltas unstable across tests.
        let a = obj("test/Node", vec![Value::Ref(None)]);
        let b = obj("test/Node", vec![Value::Ref(None)]);
        obj_set(&a, 0, Value::Ref(Some(b)));
        obj_set(&b, 0, Value::Ref(Some(a)));
        gc_point();
        assert!(
            !is_live(a) && !is_live(b),
            "unreachable cycle must be reclaimed (stale handles)"
        );
        set_forced_gc_every(None);
    }

    #[test]
    fn reachable_cycle_is_kept() {
        let _rt = crate::test_support::runtime();
        set_forced_gc_every(Some(1));
        let a = obj("test/Node", vec![Value::Ref(None)]);
        let b = obj("test/Node", vec![Value::Ref(None)]);
        obj_set(&a, 0, Value::Ref(Some(b)));
        obj_set(&b, 0, Value::Ref(Some(a)));
        rooted(&[a], || {
            for _ in 0..10 {
                gc_point();
            }
        });
        assert!(is_live(a) && is_live(b), "rooted cycle must survive");
        set_forced_gc_every(None);
    }

    #[test]
    fn array_children_are_marked() {
        let _rt = crate::test_support::runtime();
        set_forced_gc_every(Some(1));
        let child = new_string("array-child");
        let arr = obj("test/Arr", vec![Value::Int(2), Value::Ref(Some(child))]);
        rooted(&[arr], || {
            for _ in 0..10 {
                gc_point();
            }
        });
        assert!(
            is_live(arr) && is_live(child),
            "children of a rooted object survive"
        );
        set_forced_gc_every(None);
    }

    #[test]
    fn interned_strings_are_roots() {
        let _rt = crate::test_support::runtime();
        set_forced_gc_every(Some(1));
        let s = crate::object::intern_string("gc-intern-test-literal");
        for _ in 0..50 {
            let _churn = new_string("churn");
            gc_point();
        }
        assert!(is_live(s), "interned literals are permanent roots");
        let again = crate::object::intern_string("gc-intern-test-literal");
        assert!(
            s == again,
            "interning must return the same handle across GCs"
        );
        set_forced_gc_every(None);
    }

    #[test]
    fn boxed_cache_objects_are_roots() {
        let _rt = crate::test_support::runtime();
        set_forced_gc_every(Some(1));
        let b = crate::natives::boxed_cached_for_test(127i64).expect("boxed cache");
        for _ in 0..50 {
            let _churn = new_string("churn");
            gc_point();
        }
        assert!(
            is_live(b),
            "JLS 5.1.7 boxed cache entries are permanent roots"
        );
        set_forced_gc_every(None);
    }

    #[test]
    fn stale_handle_use_panics_loudly() {
        let _rt = crate::test_support::runtime();
        set_forced_gc_every(Some(1));
        let victim = new_string("dies-soon");
        let idx = victim.index();
        let gen = victim.gen;
        // Kill it: no roots name it.
        gc_point();
        assert!(!is_live(victim));
        // Its slot is recycled by a new object (forced-GC mode reuses the
        // free list immediately).
        let _replacement = new_string("takes-the-slot");
        // The old handle must not silently resolve to the new object.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_obj(victim, |o| o.jclass.clone())
        }));
        assert!(result.is_err(), "stale handle dereference must panic");
        let err = result.expect_err("must have panicked");
        let msg = err
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&'static str>().map(|s| s.to_string()))
            .unwrap_or_default();
        assert!(
            msg.contains("stale JRef"),
            "panic must carry the stale-handle diagnostic: {msg}"
        );
        let _ = (idx, gen);
        set_forced_gc_every(None);
    }

    #[test]
    fn stale_handle_never_aliases_the_replacement() {
        let _rt = crate::test_support::runtime();
        set_forced_gc_every(Some(1));
        let victim = new_string("victim");
        obj_set(
            &victim,
            0,
            Value::Str(Arc::new("victim-payload".to_string())),
        );
        gc_point(); // victim dies
        let replacement = new_string("replacement");
        // Force the replacement onto the same slot if the free list has it.
        let same_slot = replacement.index() == victim.index();
        if same_slot {
            // The victim's handle must NOT read the replacement's payload.
            let res =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| string_value(&victim)));
            assert!(res.is_err(), "stale handle must not alias the new object");
        }
        assert_eq!(&*string_value(&replacement), "replacement");
        set_forced_gc_every(None);
    }

    #[test]
    fn suspended_frame_snapshot_keeps_handle_alive() {
        let _rt = crate::test_support::runtime();
        set_forced_gc_every(Some(1));
        let payload = new_string("suspended-frame-payload");
        // Simulate a suspended caller: a frame slot that names the handle
        // while a callee collects.
        crate::frame::with_test_frame(|| {
            // refresh semantics: the "caller" recorded its refs
            crate::frame::test_refresh_slot(&[payload]);
            let _callee_churn = new_string("churn");
            gc_point();
        });
        assert!(is_live(payload), "snapshot in a suspended frame must root");
        set_forced_gc_every(None);
    }

    #[test]
    fn one_million_node_worklist_does_not_overflow() {
        let _rt = crate::test_support::runtime();
        set_forced_gc_every(Some(1));
        // A singly linked list 1_000_000 nodes long, rooted at the head.
        let mut head: Option<JRef> = None;
        for i in 0..1_000_000u32 {
            let n = obj("test/Link", vec![Value::Int(i as i32), Value::Ref(head)]);
            head = Some(n);
        }
        let head = head.unwrap();
        rooted(&[head], || {
            gc_point();
        });
        assert!(is_live(head), "the rooted list head survives");
        assert!(
            live_count() >= 1_000_000,
            "the whole 1M chain must be marked live"
        );
        set_forced_gc_every(None);
        // Release the root and collect it all (iteratively — recursion
        // during marking would be the bug this test exists to prevent).
        set_forced_gc_every(Some(1));
        let before = live_count();
        gc_point();
        assert!(
            live_count() < before && live_count() < 1_000,
            "the 1M list must be reclaimable in one iterative sweep"
        );
        set_forced_gc_every(None);
    }

    #[test]
    fn gc_point_reentrancy_is_a_bug() {
        let _rt = crate::test_support::runtime();
        // The debug assertion in gc_point/alloc fires when collection
        // re-enters; verify the flag mechanism exists by checking that
        // ordinary interleaving works and total_collections advances.
        set_forced_gc_every(Some(1));
        let before = total_collections();
        gc_point();
        assert_eq!(total_collections(), before + 1);
        set_forced_gc_every(None);
    }

    #[test]
    fn threshold_policy_collects_when_grown() {
        let _rt = crate::test_support::runtime();
        set_forced_gc_every(None);
        {
            let mut heap = heap().lock().unwrap_or_else(|e| e.into_inner());
            heap.live_at_last_gc = 0;
            heap.since_gc = 0;
            // threshold with live_at_last_gc == 0 is 4096
            assert_eq!(heap.threshold(), 4096);
            heap.live_at_last_gc = 10_000;
            assert_eq!(heap.threshold(), 20_000, "2x live when above the floor");
            heap.live_at_last_gc = 100;
            assert_eq!(heap.threshold(), 4096, "floor dominates");
        }
        // Restore a clean policy state.
        {
            let mut heap = heap().lock().unwrap_or_else(|e| e.into_inner());
            heap.live_at_last_gc = heap.live;
            heap.since_gc = 0;
        }
    }

    #[test]
    fn identity_hash_is_the_index_and_stable_across_gc() {
        let _rt = crate::test_support::runtime();
        set_forced_gc_every(Some(1));
        let a = new_obj("test/H", 1).unwrap();
        let b = new_obj("test/H", 1).unwrap();
        assert_ne!(
            a.index(),
            b.index(),
            "distinct live objects have distinct indices"
        );
        let before = a.index();
        rooted(&[a], || {
            for _ in 0..10 {
                gc_point();
            }
        });
        assert_eq!(a.index(), before, "identity hash stable across collections");
        set_forced_gc_every(None);
    }

    #[test]
    fn cycle_churn_two_hundred_k_high_water_under_64() {
        let _rt = crate::test_support::runtime();
        // T17 acceptance (M2_PLAN §4.8b): 200_000 build-and-drop cycle
        // allocations with 10 live objects — the R1 leak is gone,
        // numerically.
        //
        // M4 Wave 1: the heap (and therefore `high_water`, a monotonic
        // process-wide maximum, and `live`) is shared across the whole test
        // binary now, so the test measures its OWN footprint: high_water is
        // snapshotted/reset and restored, and `live` is checked as a delta
        // against the pre-ring baseline.
        set_forced_gc_every(Some(1));
        let high_water_before = high_water();
        {
            let mut heap = heap().lock().unwrap_or_else(|e| e.into_inner());
            heap.high_water = heap.live;
        }
        // 10 live objects in a ring, rooted via NativeRoots.
        let mut ring: Vec<JRef> = Vec::new();
        for i in 0..10 {
            ring.push(obj("test/Ring", vec![Value::Int(i), Value::Ref(None)]));
        }
        for i in 0..10 {
            let next = ring[(i + 1) % 10];
            obj_set(&ring[i], 1, Value::Ref(Some(next)));
        }
        rooted(&ring, || {
            for _ in 0..200_000u32 {
                // build a 2-node cycle, drop both handles
                // Build a 2-node cycle; the handles go out of scope at the
                // end of this block, making the cycle unreachable.
                {
                    let x = obj("test/Pair", vec![Value::Ref(None)]);
                    let y = obj("test/Pair", vec![Value::Ref(Some(x))]);
                    obj_set(&x, 0, Value::Ref(Some(y)));
                }
                gc_point(); // the collection point the next allocation
                            // opcode would hit
            }
        });
        assert!(
            high_water() < 64,
            "arena must stay tiny under cycle churn (high_water={})",
            high_water()
        );
        assert!(
            ring.iter().all(|h| is_live(*h)),
            "only the ring survives (every ring handle still live)"
        );
        // Restore the shared-metric history this test consumed.
        {
            let mut heap = heap().lock().unwrap_or_else(|e| e.into_inner());
            heap.high_water = high_water_before;
        }
        set_forced_gc_every(None);
    }

    #[test]
    fn monitor_enter_exit_counts() {
        let _rt = crate::test_support::runtime();
        let r = crate::object::new_obj("java/lang/Object", 0).unwrap();
        assert_eq!(monitor_count(&r), 0);
        monitor_enter(&r).unwrap();
        monitor_enter(&r).unwrap(); // re-entrant
        assert_eq!(monitor_count(&r), 2);
        monitor_exit(&r).unwrap();
        assert_eq!(monitor_count(&r), 1);
        monitor_exit(&r).unwrap();
        assert_eq!(monitor_count(&r), 0);
    }

    #[test]
    fn monitor_exit_without_ownership_is_imse() {
        let _rt = crate::test_support::runtime();
        let r = crate::object::new_obj("java/lang/Object", 0).unwrap();
        let err = monitor_exit(&r).unwrap_err();
        let cls = with_obj(err.obj, |o| o.jclass.to_string());
        assert_eq!(cls, "java/lang/IllegalMonitorStateException");
        let msg_ref = with_obj(err.obj, |o| match &o.slots[0] {
            Value::Ref(Some(m)) => Some(*m),
            other => panic!("unexpected message slot {other:?}"),
        });
        // L3: string_value's own heap access runs AFTER the guard ends.
        let msg = crate::object::string_value(&msg_ref.expect("message ref")).to_string();
        assert_eq!(msg, "current thread not owner");
    }

    #[test]
    fn frame_guard_releases_monitors_on_drop() {
        let _rt = crate::test_support::runtime();
        let r = crate::object::new_obj("java/lang/Object", 0).unwrap();
        {
            let g = crate::frame::FrameGuard::enter().unwrap();
            monitor_enter(&r).unwrap();
            g.record_monitor(r);
            assert_eq!(monitor_count(&r), 1);
        } // Drop releases the recorded acquisition
        assert_eq!(monitor_count(&r), 0);
    }

    #[test]
    fn frame_monitor_unrecord_pops_last_matching() {
        let _rt = crate::test_support::runtime();
        let a = crate::object::new_obj("java/lang/Object", 0).unwrap();
        let b = crate::object::new_obj("java/lang/Object", 0).unwrap();
        let g = crate::frame::FrameGuard::enter().unwrap();
        monitor_enter(&a).unwrap();
        g.record_monitor(a);
        monitor_enter(&b).unwrap();
        g.record_monitor(b);
        monitor_enter(&a).unwrap();
        g.record_monitor(a);
        // exit of a pops only the most recent a record; b stays recorded
        monitor_exit(&a).unwrap();
        g.unrecord_monitor(&a);
        assert_eq!(monitor_count(&a), 1);
        assert_eq!(monitor_count(&b), 1);
        // the Drop releases the remaining two records
        drop(g);
        assert_eq!(monitor_count(&a), 0);
        assert_eq!(monitor_count(&b), 0);
    }
}
