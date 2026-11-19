use super::*;

pub(crate) const HM_SIZE: usize = 0;
pub(crate) const HM_MODCOUNT: usize = 1;
pub(crate) const HM_THRESHOLD: usize = 2;
pub(crate) const HM_TABLE: usize = 3;
/// `HashMap$Node` slot 4 — the prim tag (G3.3 slice 3). `Value::Void` on
/// a faithful node; `Value::Int(desc-char)` when the value slot holds a
/// RAW prim payload written by `hm_put_prim`.
pub(crate) const HM_NODE_TAG: usize = 4;
pub(crate) fn tag_slot(tag: Option<char>) -> Value {
    match tag {
        Some(c) => Value::Int(c as i32),
        None => Value::Void,
    }
}
pub(crate) fn hm_state(map: &JRef) -> (i32, i32, i32, Option<JRef>) {
    with_obj(*map, |o| {
        (
            o.slots[HM_SIZE].expect_int(),
            o.slots[HM_MODCOUNT].expect_int(),
            o.slots[HM_THRESHOLD].expect_int(),
            match &o.slots[HM_TABLE] {
                Value::Ref(Some(t)) => Some(*t),
                _ => None,
            },
        )
    })
}
pub(crate) fn hm_table_len(table: &JRef) -> i32 {
    with_obj(*table, |o| o.slots[0].expect_int())
}
fn hm_bin(table: &JRef, idx: i32) -> Option<JRef> {
    with_obj(*table, |o| match &o.slots[(idx + 1) as usize] {
        Value::Ref(Some(n)) => Some(*n),
        _ => None,
    })
}
fn hm_set_bin(table: &JRef, idx: i32, node: Option<JRef>) {
    with_obj_mut(*table, |o| {
        o.slots[(idx + 1) as usize] = match node {
            Some(n) => Value::Ref(Some(n)),
            None => Value::Ref(None),
        }
    });
}
pub(crate) fn node_parts(node: &JRef) -> (i32, Value, Value, Option<JRef>) {
    with_obj(*node, |o| {
        (
            o.slots[0].expect_int(),
            o.slots[1].clone(),
            o.slots[2].clone(),
            match &o.slots[3] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    })
}
fn alloc_node_table(len: i32) -> JRef {
    let mut slots = Vec::with_capacity(len as usize + 1);
    slots.push(Value::Int(len));
    for _ in 0..len {
        slots.push(Value::Ref(None));
    }
    alloc(crate::object::JObj {
        jclass: Arc::from("[Ljava/util/HashMap$Node;"),
        slots,
        prim: None,
    })
}
/// Key equality per OpenJDK `putVal`: `key == k || (k != null &&
/// key.equals(k))`, guarded by equal stored hashes.
fn key_matches(
    stored_hash: i32,
    stored_key: &Value,
    hash: i32,
    key: &Value,
    ctx: Dispatch,
) -> Result<bool, Throw> {
    if stored_hash != hash {
        return Ok(false);
    }
    match (stored_key, key) {
        (Value::Ref(Some(_)), Value::Ref(None)) => {
            // stored key non-null, looked-up key null: OpenJDK guards with
            // `k != null`, so null never matches a non-null stored key.
            Ok(false)
        }
        (Value::Ref(None), Value::Ref(None)) => Ok(true),
        (Value::Ref(Some(a_ref)), b) => {
            if let Value::Ref(Some(b_ref)) = b {
                if JRef::ptr_eq(a_ref, b_ref) {
                    return Ok(true);
                }
            }
            dispatch_equals(a_ref, b, ctx)
        }
        _ => Err(internal_error("collection key is not a reference")),
    }
}
/// OpenJDK `resize` (Java-8 split): doubles the table, splitting each bin
/// into lo (`(hash & oldCap) == 0`) and hi halves **preserving relative
/// order within each half** — this is what makes iteration order match
/// Temurin byte-for-byte.
fn hm_resize(map: &JRef) -> Result<(), Throw> {
    let (size, mc, threshold, table) = hm_state(map);
    let _ = mc;
    let (old_cap, old_thr) = match table {
        None => (0, threshold),
        Some(t) => (hm_table_len(&t), threshold),
    };
    let (new_cap, mut new_thr) = if old_cap > 0 {
        // grow path. Java doubles the threshold only when
        // `oldCap >= DEFAULT_INITIAL_CAPACITY (16)`; below that it falls
        // through to the ft recompute — the timing of this branch is
        // observable (it decides when the next resize fires, which changes
        // table capacities and therefore iteration order), so it is
        // reproduced exactly.
        let nc = old_cap << 1;
        let nt = if old_cap >= 16 { threshold << 1 } else { 0 };
        (nc, nt)
    } else if old_thr > 0 {
        // first allocation from `new HashMap<>(initialCapacity)`
        (old_thr, 0)
    } else {
        (16, 12)
    };
    if new_thr == 0 {
        // Java: float ft = (float)newCap * loadFactor; (float arithmetic)
        let ft = (new_cap as f32 * 0.75f32) as i32;
        new_thr = if new_cap < (1 << 30) && (ft as f32) < (1 << 30) as f32 {
            ft
        } else {
            1 << 30
        };
    }
    let fresh = alloc_node_table(new_cap);
    if let Some(old) = table {
        let old_len = hm_table_len(&old);
        for i in 0..old_len {
            let mut lo_head: Option<JRef> = None;
            let mut lo_tail: Option<JRef> = None;
            let mut hi_head: Option<JRef> = None;
            let mut hi_tail: Option<JRef> = None;
            let mut cur = hm_bin(&old, i);
            while let Some(node) = cur {
                let (hash, key, value, next) = node_parts(&node);
                if (hash & old_cap) == 0 {
                    match lo_tail {
                        None => lo_head = Some(node),
                        Some(t) => with_obj_mut(t, |o| o.slots[3] = Value::Ref(Some(node))),
                    }
                    lo_tail = Some(node);
                } else {
                    match hi_tail {
                        None => hi_head = Some(node),
                        Some(t) => with_obj_mut(t, |o| o.slots[3] = Value::Ref(Some(node))),
                    }
                    hi_tail = Some(node);
                }
                cur = next;
                let _ = (key, value);
            }
            // OpenJDK terminates both split lists explicitly
            // (`loTail.next = null; hiTail.next = null`) — without this the
            // old cross-bin chain pointer survives and one node is iterated
            // from two bins.
            if let Some(t) = lo_tail {
                with_obj_mut(t, |o| o.slots[3] = Value::Ref(None));
            }
            if let Some(t) = hi_tail {
                with_obj_mut(t, |o| o.slots[3] = Value::Ref(None));
            }
            if let Some(h) = lo_head {
                hm_set_bin(&fresh, i, Some(h));
            }
            if let Some(h) = hi_head {
                hm_set_bin(&fresh, i + old_cap, Some(h));
            }
        }
    }
    with_obj_mut(*map, |o| {
        o.slots[HM_TABLE] = Value::Ref(Some(fresh));
        o.slots[HM_THRESHOLD] = Value::Int(new_thr);
    });
    let _ = size;
    Ok(())
}
/// The node layout hm_put allocates: the plain HashMap$Node (5 slots —
/// hash/key/value/next + the undeclared tag) or the LinkedHashMap$Entry
/// (7 slots — before/after after the tag, the order-link pair).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum NodeKind {
    HashNode,
    LinkedEntry,
}
/// OpenJDK `putVal` with tail insertion. Returns the previous value
/// (`Ref(None)` when absent) plus the freshly-inserted NODE (None on an
/// overwrite — LinkedHashMap's order law needs the new node to link it
/// at the tail; an overwrite keeps position, so no link work).
pub(crate) fn hm_put(
    map: &JRef,
    key: Value,
    value: Value,
    ctx: Dispatch,
    tag: Option<char>,
    kind: NodeKind,
) -> Result<(Value, Option<JRef>), Throw> {
    let hash = map_hash(&key, ctx)?;
    let (_, _, _, table) = hm_state(map);
    if table.is_none() || hm_table_len(table.as_ref().unwrap()) == 0 {
        hm_resize(map)?;
    }
    let table = match hm_state(map).3 {
        Some(t) => t,
        None => return Err(internal_error("HashMap table missing after resize")),
    };
    let len = hm_table_len(&table);
    let idx = (len - 1) & hash;
    // walk the bin
    let mut cur = hm_bin(&table, idx);
    let mut prev: Option<JRef> = None;
    while let Some(node) = cur {
        let (h, k, _old_value, next) = node_parts(&node);
        if key_matches(h, &k, hash, &key, ctx)? {
            // The Map.put contract returns the OLD value — materialize a
            // raw-prim slot first (slice 3), so the faithful caller sees
            // the same box Java would have held. The allocation happens
            // before any with_obj scope (the heap-lock law).
            let old = node_value_materialized(&node)?;
            with_obj_mut(node, |o| {
                o.slots[2] = value;
                o.slots[HM_NODE_TAG] = tag_slot(tag);
            });
            return Ok((old, None));
        }
        prev = Some(node);
        cur = next;
    }
    // tail-insert a fresh node (the kind decides the layout — LinkedHashMap
    // entries carry the before/after pair the order law walks)
    let node = match kind {
        NodeKind::HashNode => new_obj("java/util/HashMap$Node", 5)?,
        NodeKind::LinkedEntry => new_obj("java/util/LinkedHashMap$Entry", 7)?,
    };
    with_obj_mut(node, |o| {
        o.slots[0] = Value::Int(hash);
        o.slots[1] = key;
        o.slots[2] = value;
        o.slots[3] = Value::Ref(None);
        o.slots[HM_NODE_TAG] = tag_slot(tag);
    });
    match prev {
        None => hm_set_bin(&table, idx, Some(node)),
        Some(p) => with_obj_mut(p, |o| o.slots[3] = Value::Ref(Some(node))),
    }
    let (size, mc, _, _) = hm_state(map);
    with_obj_mut(*map, |o| {
        o.slots[HM_SIZE] = Value::Int(size + 1);
        o.slots[HM_MODCOUNT] = Value::Int(mc + 1);
    });
    if size + 1 > with_obj(*map, |o| o.slots[HM_THRESHOLD].expect_int()) {
        hm_resize(map)?;
    }
    Ok((Value::Ref(None), Some(node)))
}
pub(crate) fn hm_find(map: &JRef, key: &Value, ctx: Dispatch) -> Result<Option<JRef>, Throw> {
    let (_, _, _, table) = hm_state(map);
    let Some(table) = table else { return Ok(None) };
    let hash = map_hash(key, ctx)?;
    let len = hm_table_len(&table);
    let idx = (len - 1) & hash;
    let mut cur = hm_bin(&table, idx);
    while let Some(node) = cur {
        let (h, k, _, next) = node_parts(&node);
        if key_matches(h, &k, hash, key, ctx)? {
            return Ok(Some(node));
        }
        cur = next;
    }
    Ok(None)
}
pub(crate) fn hm_remove(map: &JRef, key: &Value, ctx: Dispatch) -> Result<Value, Throw> {
    let (_, _, _, table) = hm_state(map);
    let Some(table) = table else {
        return Ok(Value::Ref(None));
    };
    let hash = map_hash(key, ctx)?;
    let len = hm_table_len(&table);
    let idx = (len - 1) & hash;
    let mut prev: Option<JRef> = None;
    let mut cur = hm_bin(&table, idx);
    while let Some(node) = cur {
        let (h, k, _value, next) = node_parts(&node);
        if key_matches(h, &k, hash, key, ctx)? {
            // The removed value escapes to faithful code — materialize
            // before unlinking (slice 3; the alloc precedes any with_obj
            // scope).
            let out = node_value_materialized(&node)?;
            match prev {
                None => hm_set_bin(&table, idx, next),
                Some(p) => with_obj_mut(p, |o| {
                    o.slots[3] = match next {
                        Some(n) => Value::Ref(Some(n)),
                        None => Value::Ref(None),
                    }
                }),
            }
            let (size, mc, _, _) = hm_state(map);
            with_obj_mut(*map, |o| {
                o.slots[HM_SIZE] = Value::Int(size - 1);
                o.slots[HM_MODCOUNT] = Value::Int(mc + 1);
            });
            return Ok(out);
        }
        prev = Some(node);
        cur = next;
    }
    Ok(Value::Ref(None))
}
/// Table-order iteration (bins in order, within-bin list order). Values
/// materialize lazily (slice 3): every consumer of this Vec is faithful
/// code (toString/containsValue/view toString), which must observe real
/// boxes.
pub(crate) fn hm_all_entries(map: &JRef) -> Result<Vec<(Value, Value)>, Throw> {
    let (_, _, _, table) = hm_state(map);
    let Some(table) = table else {
        return Ok(Vec::new());
    };
    let len = hm_table_len(&table);
    let mut out = Vec::new();
    for i in 0..len {
        let mut cur = hm_bin(&table, i);
        while let Some(node) = cur {
            let (_, k, _, next) = node_parts(&node);
            let v = node_value_materialized(&node)?;
            out.push((k, v));
            cur = next;
        }
    }
    Ok(out)
}
// <init>()V / <init>(I)V / <init>(Ljava/util/Map;)V — the (I,F) ctor is
// NOT registered (the HashMap (I,F) ctor is unsupported too; the
// access-order (I,F,Z) ctor is its own loud gap). The Map arm is
// OpenJDK's putMapEntries on a fresh table: the pre-size law
// (threshold = tableSizeFor((int)((float)s / 0.75f + 1.0f)), the table
// stays lazy until the first put) and the source's ITERATION ORDER
// drives insertion — the probed HashMapCopyProbe laws L1-L9 (the
// Temurin 21.0.13+9 oracle, scripts/evidence/hm_copy_ctor/).
pub fn n_hm_init(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.<init> without receiver");
    match args.first() {
        None | Some(Value::Int(_)) => {
            // HashMap(): the threshold stays 0 — the first put takes the
            // default cap 16 (table_size_for(0) would be 1, which is the
            // (I)-ctor law, NOT the no-arg law; the distinction is
            // observable through the first allocation's capacity).
            let thr = match args.first() {
                None => 0,
                Some(v) => {
                    let c = v.expect_int();
                    if c < 0 {
                        return Err(throw_new(
                            "java/lang/IllegalArgumentException",
                            Some(&format!("Illegal initial capacity: {c}")),
                        ));
                    }
                    table_size_for(c)
                }
            };
            with_obj_mut(*r, |o| {
                o.slots[HM_SIZE] = Value::Int(0);
                o.slots[HM_MODCOUNT] = Value::Int(0);
                o.slots[HM_THRESHOLD] = Value::Int(thr);
                o.slots[HM_TABLE] = Value::Ref(None);
            });
            Ok(Value::Void)
        }
        Some(Value::Ref(None)) => {
            // putMapEntries reads m.size() on the null map FIRST.
            Err(throw_new("java/lang/NullPointerException", None))
        }
        Some(Value::Ref(Some(src))) => {
            let entries = map_source_entries(src, ctx.parents)?;
            hm_ctor_presize(r, entries.len())?;
            for (k, v) in entries {
                hm_put(r, k, v, ctx, None, NodeKind::HashNode)?;
            }
            Ok(Value::Void)
        }
        Some(other) => Err(internal_error(&format!(
            "j2r internal: HashMap.<init> unexpected arg {other:?}"
        ))),
    }
}
/// The copy-ctor SOURCE-storage law shared by the HashMap and
/// LinkedHashMap copy ctors: iterate the source in ITS order — the
/// LinkedHashMap link chain FIRST (LHM extends HashMap, so the LHM test
/// must win the dispatch), then the TreeMap key order, then the plain
/// HashMap table order. User subclasses inherit their builtin
/// ancestor's storage (the generated parents table classifies them);
/// a user class implementing Map directly has no builtin storage to
/// walk and stays loud (the same-surface law; the user-Map-source
/// residual — Temurin would iterate its entrySet()).
pub(crate) fn map_source_entries(
    src: &JRef,
    parents: Option<UserParents>,
) -> Result<Vec<(Value, Value)>, Throw> {
    let src_class = with_obj(*src, |o| o.jclass.to_string());
    if is_subtype(&src_class, "java/util/LinkedHashMap", parents) {
        lhm_all_entries(src)
    } else if is_subtype(&src_class, "java/util/TreeMap", parents) {
        tm_all_entries(src)
    } else if is_subtype(&src_class, "java/util/HashMap", parents) {
        hm_all_entries(src)
    } else {
        Err(internal_error(&format!(
            "j2r internal: map copy from unsupported source class {src_class} (the user-Map-source residual)"
        )))
    }
}
/// The putMapEntries pre-size law shared by both copy ctors: threshold
/// = tableSizeFor((int)((float)s / 0.75f + 1.0f)) (or 0 for an empty
/// source — putMapEntries skips everything and the first put takes the
/// default cap 16), and the table stays lazy: hm_resize's old_thr>0
/// arm allocates it at the threshold on the first put.
pub(crate) fn hm_ctor_presize(r: &JRef, s: usize) -> Result<(), Throw> {
    let thr = if s > 0 {
        // Java: float ft = ((float)s / 0.75f) + 1.0f;
        //       int t = (ft < (float)MAXIMUM_CAPACITY) ? (int)ft : Integer.MAX_VALUE;
        let ft = (s as f32 / 0.75f32) + 1.0f32;
        let t = if ft < (1u32 << 30) as f32 {
            ft as i32
        } else {
            i32::MAX
        };
        table_size_for(t)
    } else {
        0
    };
    with_obj_mut(*r, |o| {
        o.slots[HM_SIZE] = Value::Int(0);
        o.slots[HM_MODCOUNT] = Value::Int(0);
        o.slots[HM_THRESHOLD] = Value::Int(thr);
        o.slots[HM_TABLE] = Value::Ref(None);
    });
    Ok(())
}
pub fn n_hm_put(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.put without receiver");
    let key = arg_or_null(args);
    let value = args.get(1).cloned().unwrap_or(Value::Ref(None));
    let (old, _) = hm_put(r, key, value, ctx, None, NodeKind::HashNode)?;
    Ok(old)
}
pub fn n_hm_get(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.get without receiver");
    let key = arg_or_null(args);
    match hm_find(r, &key, ctx)? {
        Some(node) => node_value_materialized(&node),
        None => Ok(Value::Ref(None)),
    }
}

// --- G3.3 slice 2: the prim-slot map natives (the specialized-map path) ---
//
// The peephole (`map_prim_rewrites`) rewrites javac's `valueOf -> put ->
// pop` and `get -> checkcast -> doubleValue` cores on receiver locals its
// closure law proved safe (every load of the local is a rewritten core's
// receiver, so the map never aliases and every slot was written by a
// rewritten put) and — slice 3 — on static fields whose class-level law
// holds (every PutStatic a HashMap creation run, every GetStatic a
// guarded core's receiver pinned by the field signature). There the box
// is unobservable as an `Object`, and the node's value slot holds the RAW
// prim `Value` — one fewer heap child per node, no valueOf alloc, no
// unbox dispatch (the register's 2.78x container tax, evidence
// g3_3/opening_census).
/// G3.3 slice 3: the lazy box materialization at the faithful exits.
///
/// A node whose value slot holds a RAW prim (stamped by `hm_put_prim`
/// with the payload's desc char in slot 4) is materialized into its
/// boxed object on the first faithful read and the box is CACHED back
/// into the slot — get-twice-returns-the-same-ref (probe law L1: three
/// generic exits of one key compare identical) — and the boxing routes
/// through the same `valueOf` natives user code uses, so the JLS
/// identity caches apply (probe law L2: `CACHE.get("n") ==
/// Integer.valueOf(5)` is TRUE for cached values). A tag-less node is a
/// faithful node: its slot is already a box or null, pass it through.
/// The allocation happens OUTSIDE any `with_obj` scope (the heap-lock
/// law: alloc never nests under an object borrow).
pub(crate) fn node_value_materialized(node: &JRef) -> Result<Value, Throw> {
    node_value_materialized_at(node, 2, HM_NODE_TAG)
}
/// The layout-aware form of [`node_value_materialized`] (G3.3 slice 4):
/// the same law at an arbitrary (value slot, tag slot) pair — the
/// red-black storage's `TreeMap$Entry` reads (TME_VALUE, TME_TAG). All
/// other laws are the helper's own (shared above).
pub(crate) fn node_value_materialized_at(
    node: &JRef,
    value_idx: usize,
    tag_idx: usize,
) -> Result<Value, Throw> {
    let (v, tag) = with_obj(*node, |o| {
        (o.slots[value_idx].clone(), o.slots[tag_idx].clone())
    });
    let Value::Int(c) = tag else { return Ok(v) };
    let is_prim = matches!(
        v,
        Value::Double(_) | Value::Float(_) | Value::Int(_) | Value::Long(_)
    );
    if !is_prim {
        // A tagged slot whose value is already a box: the box was
        // materialized earlier and the tag kept as provenance — pass it
        // through (the tag is an annotation, not a mode switch).
        return Ok(v);
    }
    let desc = match char::from_u32(c as u32) {
        Some(d @ ('D' | 'F' | 'I' | 'J')) => d,
        _ => {
            return Err(internal_error(
                "j2r internal: map node carries a prim slot with an invalid tag",
            ))
        }
    };
    let boxed = crate::reflect::box_by_desc(v, &desc.to_string())?;
    with_obj_mut(*node, |o| o.slots[value_idx] = boxed.clone());
    Ok(boxed)
}
/// The specialized-map put: stores `value` (a raw `Value::Double/Float/
/// Int/Long`) in the node's value slot. The key's hash/equals dispatch,
/// the resize law and the modCount are `hm_put`'s own machinery — only
/// the box allocation and the `valueOf` dispatch disappear. The `Map.put`
/// return (the OLD value, per the Map contract) is discarded: the
/// rewritten core requires the `Pop`, so no caller can observe it.
pub fn hm_put_prim(map: &JRef, key: Value, value: Value, ctx: Dispatch) -> Result<(), Throw> {
    // The tag rides the stored value's own prim kind — the node becomes
    // self-describing, so a faithful reader (slice 3's materialization)
    // constructs the exact box class the compiler's closure law hid.
    let tag = match &value {
        Value::Double(_) => Some('D'),
        Value::Float(_) => Some('F'),
        Value::Int(_) => Some('I'),
        Value::Long(_) => Some('J'),
        _ => None,
    };
    // G3.3 slice 4: the node kind follows the MAP's runtime class — a
    // LinkedHashMap rides the same rewrite (the link-chain storage, the
    // tail-link law included; an overwrite keeps position, so no link
    // work there either). The linked entry layout already carries the
    // tag (the post-slice-3 node layout), so the materialization layer
    // serves it unchanged.
    let cls = with_obj(*map, |o| o.jclass.to_string());
    let kind = if cls == "java/util/LinkedHashMap" {
        NodeKind::LinkedEntry
    } else {
        NodeKind::HashNode
    };
    let (_, node) = hm_put(map, key, value, ctx, tag, kind)?;
    if kind == NodeKind::LinkedEntry {
        if let Some(node) = node {
            lhm_link_at_tail(map, &node);
        }
    }
    Ok(())
}
/// The specialized-map get: the FUSED `checkcast + xValue()` law. The
/// synthetic replaces `get; checkcast Box; Box.xValue()p`, so its
/// failure modes are the fused law's failure modes, byte-for-byte:
/// - a MISSING key is the null box reaching `xValue()`: the bare
///   `NullPointerException` (no message) — the same throw the boxing
///   path produces at that program point (uncaught traces render no
///   frame lines, and the peephole refuses spans straddling a handler
///   boundary, so the fault-pc shift is unobservable);
/// - a stored `null` VALUE (a non-core put wrote it) is the same null
///   box reaching `xValue()`: the same bare NPE;
/// - a FOREIGN box (a non-core put wrote a wrong-type box — raw types,
///   unchecked generics) fails the faithful `checkcast` through the
///   shared helper: the exact CCE bytes;
/// - a raw-prim slot whose tag disagrees with `expect` is the same CCE
///   with the tag's class as the actual type (the faithful slot held
///   that box);
/// - a matching box or tag unwraps to the prim payload.
pub fn hm_get_prim(
    map: &JRef,
    key: Value,
    expect: &'static str,
    ctx: Dispatch,
) -> Result<Value, Throw> {
    match hm_find(map, &key, ctx)? {
        Some(node) => {
            let v = node_parts(&node).2;
            match v {
                Value::Ref(None) => Err(throw_new("java/lang/NullPointerException", None)),
                Value::Ref(Some(boxed)) => {
                    crate::checkcast(Value::Ref(Some(boxed)), expect, ctx)?;
                    let payload = with_obj(boxed, |o| o.slots[0].clone());
                    match payload {
                        p @ (Value::Double(_)
                        | Value::Float(_)
                        | Value::Int(_)
                        | Value::Long(_)) => Ok(p),
                        _ => Err(internal_error(
                            "j2r internal: map prim slot holds a non-prim box",
                        )),
                    }
                }
                other @ (Value::Double(_) | Value::Float(_) | Value::Int(_) | Value::Long(_)) => {
                    // The RAW prim slot: the tag is the node's own class
                    // stamp — the provenance check the faithful checkcast
                    // would have run on the box.
                    let tag = with_obj(node, |o| o.slots[HM_NODE_TAG].clone());
                    let Value::Int(c) = tag else {
                        return Err(internal_error(
                            "j2r internal: map prim slot without a class tag",
                        ));
                    };
                    let actual = match char::from_u32(c as u32) {
                        Some('D') => "java/lang/Double",
                        Some('F') => "java/lang/Float",
                        Some('I') => "java/lang/Integer",
                        Some('J') => "java/lang/Long",
                        _ => {
                            return Err(internal_error(
                                "j2r internal: map node carries an invalid class tag",
                            ))
                        }
                    };
                    if actual == expect {
                        Ok(other)
                    } else {
                        Err(crate::cce_throw(actual, expect))
                    }
                }
                other => Err(internal_error(&format!(
                    "j2r internal: map prim slot holds {other:?}"
                ))),
            }
        }
        None => Err(throw_new("java/lang/NullPointerException", None)),
    }
}
pub fn n_hm_get_or_default(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.getOrDefault without receiver");
    let key = arg_or_null(args);
    let default = args.get(1).cloned().unwrap_or(Value::Ref(None));
    match hm_find(r, &key, ctx)? {
        Some(node) => node_value_materialized(&node),
        None => Ok(default),
    }
}
pub fn n_hm_contains_key(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.containsKey without receiver");
    let key = arg_or_null(args);
    Ok(Value::Int(hm_find(r, &key, ctx)?.is_some() as i32))
}
pub fn n_hm_contains_value(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.containsValue without receiver");
    let target = arg_or_null(args);
    for (_, v) in hm_all_entries(r)? {
        let eq = match (&v, &target) {
            (Value::Ref(Some(a)), Value::Ref(Some(_))) => dispatch_equals(a, &target, ctx)?,
            (Value::Ref(None), Value::Ref(None)) => true,
            _ => false,
        };
        if eq {
            return Ok(Value::Int(1));
        }
    }
    Ok(Value::Int(0))
}
pub fn n_hm_remove(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.remove without receiver");
    let key = arg_or_null(args);
    hm_remove(r, &key, ctx)
}
pub fn n_hm_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.size without receiver");
    Ok(Value::Int(with_obj(*r, |o| o.slots[HM_SIZE].expect_int())))
}
pub fn n_hm_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.isEmpty without receiver");
    Ok(Value::Int(
        (with_obj(*r, |o| o.slots[HM_SIZE].expect_int()) == 0) as i32,
    ))
}
pub fn n_hm_clear(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.clear without receiver");
    let mc = with_obj(*r, |o| o.slots[HM_MODCOUNT].expect_int());
    with_obj_mut(*r, |o| {
        o.slots[HM_SIZE] = Value::Int(0);
        o.slots[HM_MODCOUNT] = Value::Int(mc + 1);
        o.slots[HM_TABLE] = Value::Ref(None);
    });
    Ok(Value::Void)
}
pub fn n_hm_to_string(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.toString without receiver");
    let entries = hm_all_entries(r)?;
    let mut out = String::from("{");
    for (i, (k, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(k, ctx)?);
        out.push('=');
        out.push_str(&dispatch_to_string(v, ctx)?);
    }
    out.push('}');
    Ok(Value::Ref(Some(crate::object::new_string(&out))))
}
pub fn n_hm_key_set(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.keySet without receiver");
    let ks = new_obj("java/util/HashMap$KeySet", 1)?;
    with_obj_mut(ks, |o| o.slots[0] = Value::Ref(Some(*r)));
    Ok(Value::Ref(Some(ks)))
}
pub fn n_hm_values(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashMap.values without receiver");
    let vs = new_obj("java/util/HashMap$Values", 1)?;
    with_obj_mut(vs, |o| o.slots[0] = Value::Ref(Some(*r)));
    Ok(Value::Ref(Some(vs)))
}

// ------------------------------------------- HashMap views and iterators
/// Builds a positioned hash iterator (`HashMap$HashIterator` semantics:
/// the constructor advances to the first entry; `hasNext` is `next !=
/// null` with no comodification check; `nextNode` checks modCount first).
/// Layout: 0 = map, 1 = next table index to scan, 2 = pending node or
/// null, 3 = expectedModCount, 4 = current (last-returned node or null).
pub(crate) fn hm_new_hash_iterator(map: &JRef, class: &str) -> Result<Value, Throw> {
    let (_, _, _, table) = hm_state(map);
    let mc = with_obj(*map, |o| o.slots[HM_MODCOUNT].expect_int());
    let (mut idx, mut pending) = (0i32, None);
    if let Some(t) = table {
        let len = hm_table_len(&t);
        while idx < len {
            if let Some(n) = hm_bin(&t, idx) {
                pending = Some(n);
                idx += 1;
                break;
            }
            idx += 1;
        }
    }
    let it = new_obj(class, 5)?;
    with_obj_mut(it, |o| {
        o.slots[0] = Value::Ref(Some(*map));
        o.slots[1] = Value::Int(idx);
        o.slots[2] = match pending {
            Some(n) => Value::Ref(Some(n)),
            None => Value::Ref(None),
        };
        o.slots[3] = Value::Int(mc);
        o.slots[4] = Value::Ref(None);
    });
    Ok(Value::Ref(Some(it)))
}
/// Advances an iterator after consuming `pending`: next node in the same
/// bin, else the next non-null bin from slot 1.
fn hm_advance(it: &JRef, pending: JRef) {
    let (_, _, _, next) = node_parts(&pending);
    if let Some(n) = next {
        with_obj_mut(*it, |o| o.slots[2] = Value::Ref(Some(n)));
        return;
    }
    let map = with_obj(*it, |o| match &o.slots[0] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: hash iterator without map"),
    });
    let mut idx = with_obj(*it, |o| o.slots[1].expect_int());
    let table = match hm_state(&map).3 {
        Some(t) => t,
        None => {
            with_obj_mut(*it, |o| {
                o.slots[2] = Value::Ref(None);
                o.slots[1] = Value::Int(idx);
            });
            return;
        }
    };
    let len = hm_table_len(&table);
    let mut found = None;
    while idx < len {
        if let Some(n) = hm_bin(&table, idx) {
            found = Some(n);
            idx += 1;
            break;
        }
        idx += 1;
    }
    with_obj_mut(*it, |o| {
        o.slots[2] = match found {
            Some(n) => Value::Ref(Some(n)),
            None => Value::Ref(None),
        };
        o.slots[1] = Value::Int(idx);
    });
}
/// Shared `nextNode()` for both hash iterators: CME check first (exact
/// OpenJDK order), then `NoSuchElementException` when exhausted.
pub(crate) fn hm_hash_next_node(recv: Option<&JRef>, who: &str) -> Result<(JRef, JRef), Throw> {
    let it = recv.unwrap_or_else(|| panic!("j2r internal: {who} without receiver"));
    let (map, expected, pending) = with_obj(*it, |o| {
        (
            match &o.slots[0] {
                Value::Ref(Some(m)) => *m,
                _ => panic!("j2r internal: {who} without map"),
            },
            o.slots[3].expect_int(),
            match &o.slots[2] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    let mc = with_obj(map, |o| o.slots[HM_MODCOUNT].expect_int());
    if expected != mc {
        return Err(crate::cme());
    }
    let Some(node) = pending else {
        return Err(throw_new("java/util/NoSuchElementException", None));
    };
    hm_advance(it, node);
    // record `current` (the last-returned node) — the field the JDK
    // HashIterator.remove() consumes
    with_obj_mut(*it, |o| o.slots[4] = Value::Ref(Some(node)));
    Ok((*it, node))
}
/// The hash iterators' `remove()` (`HashMap$KeyIterator`,
/// `HashMap$ValueIterator`, `HashMap$EntryIterator`) — the OpenJDK
/// `HashIterator.remove` law, PROBED vs Temurin 21.0.13+9 (evidence
/// scripts/evidence/itr_remove/itrorder_temurin_2026-11-15.txt, cases
/// 1–9): **ISE FIRST** when `current` is null (case 1: both conditions
/// hold → ISE, not CME), THEN the CME check (case 4rm: lastRet set +
/// foreign mod → CME); the removal goes through the map's own remove
/// law (the same path as `HashMap.remove`); `current` clears (case 3:
/// double remove → ISE); `expectedModCount` re-syncs (case 9: continued
/// iteration after remove throws no CME). This REPLACES the accidental
/// `n_itr_remove` wiring (the ArrayList-shaped rows — the slot-2
/// type confusion registered as a residual in the stringsim wave).
pub fn n_hi_remove(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: hash iterator remove without receiver");
    let (map, expected, current) = with_obj(*it, |o| {
        (
            match &o.slots[0] {
                Value::Ref(Some(m)) => *m,
                _ => panic!("j2r internal: hash iterator without map"),
            },
            o.slots[3].expect_int(),
            match &o.slots[4] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    // probed law: ISE on null current BEFORE the CME check
    let Some(node) = current else {
        return Err(throw_new("java/lang/IllegalStateException", None));
    };
    let mc = with_obj(map, |o| o.slots[HM_MODCOUNT].expect_int());
    if expected != mc {
        return Err(crate::cme());
    }
    let key = node_parts(&node).1;
    n_hm_remove(Some(&map), std::slice::from_ref(&key), ctx)?;
    let mc = with_obj(map, |o| o.slots[HM_MODCOUNT].expect_int());
    with_obj_mut(*it, |o| {
        o.slots[4] = Value::Ref(None);
        o.slots[3] = Value::Int(mc);
    });
    Ok(Value::Void)
}
pub fn n_hm_entry_set(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = recv.expect("j2r internal: HashMap.entrySet without receiver");
    let view = new_obj("java/util/HashMap$EntrySet", 1)?;
    with_obj_mut(view, |o| o.slots[0] = Value::Ref(Some(*map)));
    Ok(Value::Ref(Some(view)))
}
fn hes_map(recv: Option<&JRef>, who: &str) -> JRef {
    let r = recv.unwrap_or_else(|| panic!("j2r internal: EntrySet.{who} without receiver"));
    with_obj(*r, |o| match &o.slots[0] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: EntrySet.{who} without backing map"),
    })
}
pub fn n_hes_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = hes_map(recv, "iterator");
    hm_new_hash_iterator(&map, "java/util/HashMap$EntryIterator")
}
pub fn n_hes_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(hes_map(recv, "size"), |o| {
        o.slots[HM_SIZE].expect_int()
    })))
}
pub fn n_hes_is_empty(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(hes_map(recv, "isEmpty"), |o| o.slots[HM_SIZE].expect_int()) == 0) as i32,
    ))
}
pub fn n_ei_has_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    n_ki_has_next(recv, _args, _ctx)
}
/// JDK semantics: the entry iterator's next() IS the internal Node, exposed
/// as Map.Entry (HashMap$Node implements Map.Entry).
pub fn n_ei_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = hm_hash_next_node(recv, "EntryIterator.next")?;
    Ok(Value::Ref(Some(node)))
}
pub fn n_node_get_key(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let n = recv.expect("j2r internal: Node.getKey without receiver");
    Ok(node_parts(n).1)
}
pub fn n_node_get_value(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let n = recv.expect("j2r internal: Node.getValue without receiver");
    node_value_materialized(n)
}

// ------------------------------------------------- Collections$UnmodifiableMap
//
// The stringsim corpus wave: ShingleBased.getProfile returns
// Collections.unmodifiableMap(shingles). Slot 0 = delegate map. Reads
// forward; mutations throw UnsupportedOperationException (JDK: no
// message). entrySet forwards to the delegate — the JDK wrapper also
// iterates the delegate's entrySet, so order semantics are unchanged.
