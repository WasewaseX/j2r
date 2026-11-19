use super::*;

pub(crate) const LHM_HEAD: usize = 4;
pub(crate) const LHM_TAIL: usize = 5;
// LinkedHashMap$Entry link slots (after the Node prefix + tag).
pub(crate) const LHE_BEFORE: usize = 5;
pub(crate) const LHE_AFTER: usize = 6;
/// Reads the order links under one heap-lock scope.
fn lhm_links(map: &JRef) -> (Option<JRef>, Option<JRef>) {
    with_obj(*map, |o| {
        let head = match &o.slots[LHM_HEAD] {
            Value::Ref(Some(h)) => Some(*h),
            _ => None,
        };
        let tail = match &o.slots[LHM_TAIL] {
            Value::Ref(Some(t)) => Some(*t),
            _ => None,
        };
        (head, tail)
    })
}
/// Links `node` at the tail (the L3 law: every NEW insertion goes last).
/// Pure slot writes — no allocation inside any scope (the heap-lock law).
pub(crate) fn lhm_link_at_tail(map: &JRef, node: &JRef) {
    let (head, tail) = lhm_links(map);
    with_obj_mut(*node, |o| {
        o.slots[LHE_BEFORE] = match tail {
            Some(t) => Value::Ref(Some(t)),
            None => Value::Ref(None),
        };
        o.slots[LHE_AFTER] = Value::Ref(None);
    });
    if let Some(t) = tail {
        with_obj_mut(t, |o| o.slots[LHE_AFTER] = Value::Ref(Some(*node)));
    }
    with_obj_mut(*map, |o| {
        if head.is_none() {
            o.slots[LHM_HEAD] = Value::Ref(Some(*node));
        }
        o.slots[LHM_TAIL] = Value::Ref(Some(*node));
    });
}
/// Unlinks `node` from the chain (the remove law). Pure slot writes.
fn lhm_unlink(map: &JRef, node: &JRef) {
    let (before, after) = with_obj(*node, |o| {
        let b = match &o.slots[LHE_BEFORE] {
            Value::Ref(Some(x)) => Some(*x),
            _ => None,
        };
        let a = match &o.slots[LHE_AFTER] {
            Value::Ref(Some(x)) => Some(*x),
            _ => None,
        };
        (b, a)
    });
    match before {
        Some(b) => with_obj_mut(b, |o| {
            o.slots[LHE_AFTER] = match after {
                Some(a) => Value::Ref(Some(a)),
                None => Value::Ref(None),
            }
        }),
        None => with_obj_mut(*map, |o| {
            o.slots[LHM_HEAD] = match after {
                Some(a) => Value::Ref(Some(a)),
                None => Value::Ref(None),
            }
        }),
    }
    match after {
        Some(a) => with_obj_mut(a, |o| {
            o.slots[LHE_BEFORE] = match before {
                Some(b) => Value::Ref(Some(b)),
                None => Value::Ref(None),
            }
        }),
        None => with_obj_mut(*map, |o| {
            o.slots[LHM_TAIL] = match before {
                Some(b) => Value::Ref(Some(b)),
                None => Value::Ref(None),
            }
        }),
    }
    with_obj_mut(*node, |o| {
        o.slots[LHE_BEFORE] = Value::Ref(None);
        o.slots[LHE_AFTER] = Value::Ref(None);
    });
}
/// Link-order entry walk (the L1/L7 law). Values materialize lazily
/// (slice 3) — every consumer is faithful code.
pub(crate) fn lhm_all_entries(map: &JRef) -> Result<Vec<(Value, Value)>, Throw> {
    let (mut cur, _) = lhm_links(map);
    let mut out = Vec::new();
    while let Some(node) = cur {
        let (_, k, _, next) = node_parts(&node);
        let v = node_value_materialized(&node)?;
        out.push((k, v));
        cur = with_obj(node, |o| match &o.slots[LHE_AFTER] {
            Value::Ref(Some(x)) => Some(*x),
            _ => None,
        });
        let _ = next;
    }
    Ok(out)
}
// <init>()V / <init>(I)V / <init>(Ljava/util/Map;)V — the (I,F) ctor is
// NOT registered (the HashMap (I,F) ctor is unsupported too; the
// access-order (I,F,Z) ctor is its own loud gap).
pub fn n_lhm_init(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.<init> without receiver");
    match args.first() {
        None | Some(Value::Int(_)) => {
            // The HashMap init law for the empty shapes (the negative-cap
            // IAE included), then the order links start empty.
            n_hm_init(Some(r), args, ctx)?;
            with_obj_mut(*r, |o| {
                o.slots[LHM_HEAD] = Value::Ref(None);
                o.slots[LHM_TAIL] = Value::Ref(None);
            });
            Ok(Value::Void)
        }
        Some(Value::Ref(Some(src))) => {
            let entries = map_source_entries(src, ctx.parents)?;
            hm_ctor_presize(r, entries.len())?;
            with_obj_mut(*r, |o| {
                o.slots[LHM_HEAD] = Value::Ref(None);
                o.slots[LHM_TAIL] = Value::Ref(None);
            });
            for (k, v) in entries {
                lhm_put_entry(r, k, v, ctx)?;
            }
            Ok(Value::Void)
        }
        Some(Value::Ref(None)) => Err(throw_new("java/lang/NullPointerException", None)),
        Some(other) => Err(internal_error(&format!(
            "j2r internal: LinkedHashMap.<init> unexpected arg {other:?}"
        ))),
    }
}
/// The put machinery shared by n_lhm_put and the copy ctor: hm_put for
/// the table (the HashMap law untouched), the new node linked at the
/// tail (L1/L3). Overwrites return the old value and touch no links (L2).
fn lhm_put_entry(map: &JRef, key: Value, value: Value, ctx: Dispatch) -> Result<Value, Throw> {
    let (old, node) = hm_put(map, key, value, ctx, None, NodeKind::LinkedEntry)?;
    if let Some(node) = node {
        lhm_link_at_tail(map, &node);
    }
    Ok(old)
}
pub fn n_lhm_put(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.put without receiver");
    let key = arg_or_null(args);
    let value = args.get(1).cloned().unwrap_or(Value::Ref(None));
    lhm_put_entry(r, key, value, ctx)
}
pub fn n_lhm_remove(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.remove without receiver");
    let key = arg_or_null(args);
    let node = hm_find(r, &key, ctx)?;
    match node {
        Some(n) => {
            // The table walk first (materializes the old value), then the
            // chain unlink (pure slot writes, no allocation between).
            let old = hm_remove(r, &key, ctx)?;
            lhm_unlink(r, &n);
            Ok(old)
        }
        None => Ok(Value::Ref(None)),
    }
}
pub fn n_lhm_clear(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.clear without receiver");
    let mc = with_obj(*r, |o| o.slots[HM_MODCOUNT].expect_int());
    with_obj_mut(*r, |o| {
        o.slots[HM_SIZE] = Value::Int(0);
        o.slots[HM_MODCOUNT] = Value::Int(mc + 1);
        o.slots[HM_TABLE] = Value::Ref(None);
        o.slots[LHM_HEAD] = Value::Ref(None);
        o.slots[LHM_TAIL] = Value::Ref(None);
    });
    Ok(Value::Void)
}
pub fn n_lhm_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.toString without receiver");
    let entries = lhm_all_entries(r)?;
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
fn lhm_new_view(map: &JRef, class: &str) -> Result<Value, Throw> {
    let v = new_obj(class, 1)?;
    with_obj_mut(v, |o| o.slots[0] = Value::Ref(Some(*map)));
    Ok(Value::Ref(Some(v)))
}
pub fn n_lhm_key_set(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.keySet without receiver");
    lhm_new_view(r, "java/util/LinkedHashMap$KeySet")
}
pub fn n_lhm_values(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.values without receiver");
    lhm_new_view(r, "java/util/LinkedHashMap$Values")
}
pub fn n_lhm_entry_set(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: LinkedHashMap.entrySet without receiver");
    lhm_new_view(r, "java/util/LinkedHashMap$EntrySet")
}

// --- the LinkedHashMap views (slot 0 = the backing map, the HashMap
// view shape; every order-visible operation walks the LINK CHAIN).
pub fn n_lks_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(
        view_map(recv, "LinkedKeySet.size"),
        |o| o.slots[HM_SIZE].expect_int(),
    )))
}
pub fn n_lks_is_empty(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(view_map(recv, "LinkedKeySet.isEmpty"), |o| {
            o.slots[HM_SIZE].expect_int()
        }) == 0) as i32,
    ))
}
pub fn n_lks_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = view_map(recv, "LinkedKeySet.contains");
    let key = arg_or_null(args);
    Ok(Value::Int(hm_find(&map, &key, ctx)?.is_some() as i32))
}
pub fn n_lks_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = view_map(recv, "LinkedKeySet.toString");
    let items: Vec<Value> = lhm_all_entries(&map)?.into_iter().map(|(k, _)| k).collect();
    collection_to_string(items, ctx)
}
pub fn n_lks_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = view_map(recv, "LinkedKeySet.iterator");
    lhm_new_link_iterator(&map, "java/util/LinkedHashMap$KeyIterator")
}
pub fn n_lvs_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(
        view_map(recv, "LinkedValues.size"),
        |o| o.slots[HM_SIZE].expect_int(),
    )))
}
pub fn n_lvs_is_empty(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(view_map(recv, "LinkedValues.isEmpty"), |o| {
            o.slots[HM_SIZE].expect_int()
        }) == 0) as i32,
    ))
}
pub fn n_lvs_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    n_hm_contains_value(Some(&view_map(recv, "LinkedValues.contains")), args, ctx)
}
pub fn n_lvs_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = view_map(recv, "LinkedValues.toString");
    let items: Vec<Value> = lhm_all_entries(&map)?.into_iter().map(|(_, v)| v).collect();
    collection_to_string(items, ctx)
}
pub fn n_lvs_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = view_map(recv, "LinkedValues.iterator");
    lhm_new_link_iterator(&map, "java/util/LinkedHashMap$ValueIterator")
}
pub fn n_les_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(
        view_map(recv, "LinkedEntrySet.size"),
        |o| o.slots[HM_SIZE].expect_int(),
    )))
}
pub fn n_les_is_empty(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(view_map(recv, "LinkedEntrySet.isEmpty"), |o| {
            o.slots[HM_SIZE].expect_int()
        }) == 0) as i32,
    ))
}
pub fn n_les_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = view_map(recv, "LinkedEntrySet.iterator");
    lhm_new_link_iterator(&map, "java/util/LinkedHashMap$EntryIterator")
}
/// The link-chain iterator (the JDK LinkedHashMap LinkedHashIterator
/// shape): 0 = map, 1 = pending node (the constructor advances to head),
/// 2 = lastRet, 3 = expectedModCount.
fn lhm_new_link_iterator(map: &JRef, class: &str) -> Result<Value, Throw> {
    let mc = with_obj(*map, |o| o.slots[HM_MODCOUNT].expect_int());
    let (head, _) = lhm_links(map);
    let it = new_obj(class, 4)?;
    with_obj_mut(it, |o| {
        o.slots[0] = Value::Ref(Some(*map));
        o.slots[1] = match head {
            Some(h) => Value::Ref(Some(h)),
            None => Value::Ref(None),
        };
        o.slots[2] = Value::Ref(None);
        o.slots[3] = Value::Int(mc);
    });
    Ok(Value::Ref(Some(it)))
}
/// Shared `nextNode()` for the link iterators: the CME check FIRST
/// (exact OpenJDK order), then NoSuchElementException when exhausted;
/// advance = `pending.after`.
fn lhm_link_next_node(recv: Option<&JRef>, who: &str) -> Result<(JRef, JRef), Throw> {
    let it = recv.unwrap_or_else(|| panic!("j2r internal: {who} without receiver"));
    let (map, expected, pending) = with_obj(*it, |o| {
        (
            match &o.slots[0] {
                Value::Ref(Some(m)) => *m,
                _ => panic!("j2r internal: {who} without map"),
            },
            o.slots[3].expect_int(),
            match &o.slots[1] {
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
    let after = with_obj(node, |o| match &o.slots[LHE_AFTER] {
        Value::Ref(Some(x)) => Value::Ref(Some(*x)),
        _ => Value::Ref(None),
    });
    with_obj_mut(*it, |o| {
        o.slots[1] = after;
        o.slots[2] = Value::Ref(Some(node));
    });
    Ok((map, node))
}
fn lhm_link_has_next(recv: Option<&JRef>, who: &str) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: link iterator hasNext without receiver");
    let _ = who;
    let has = with_obj(*it, |o| matches!(&o.slots[1], Value::Ref(Some(_))));
    Ok(Value::Int(has as i32))
}
pub fn n_lki_has_next(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    lhm_link_has_next(recv, "LinkedKeyIterator")
}
pub fn n_lki_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = lhm_link_next_node(recv, "LinkedKeyIterator.next")?;
    Ok(node_parts(&node).1)
}
pub fn n_lvi_has_next(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    lhm_link_has_next(recv, "LinkedValueIterator")
}
pub fn n_lvi_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = lhm_link_next_node(recv, "LinkedValueIterator.next")?;
    node_value_materialized(&node)
}
pub fn n_lei_has_next(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    lhm_link_has_next(recv, "LinkedEntryIterator")
}
/// The entry iterator's next() IS the internal Entry node (the JDK law:
/// LinkedHashMap.Entry implements Map.Entry), exposed as Map.Entry.
pub fn n_lei_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = lhm_link_next_node(recv, "LinkedEntryIterator.next")?;
    Ok(Value::Ref(Some(node)))
}
/// Iterator.remove over the link chain (the L8 law): **ISE FIRST** when
/// lastRet is null (the 2026-11-15 ItrOrderProbe re-probe corrected the
/// original check order — Temurin case 5: both conditions hold → ISE,
/// NOT CME; the original CME-first order was never probed with both
/// conditions combined), then the CME check; the removal goes through
/// the map's own remove law (table + chain), lastRet clears,
/// expectedModCount re-syncs (no CME on a continued iteration).
pub fn n_lhm_itr_remove(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: LinkedHashMap iterator remove without receiver");
    let (map, expected, last_ret) = with_obj(*it, |o| {
        (
            match &o.slots[0] {
                Value::Ref(Some(m)) => *m,
                _ => panic!("j2r internal: link iterator without map"),
            },
            o.slots[3].expect_int(),
            match &o.slots[2] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    // probed law: ISE on null lastRet BEFORE the CME check
    // (Temurin 21.0.13+9, ItrOrderProbe case 5)
    let Some(node) = last_ret else {
        return Err(throw_new("java/lang/IllegalStateException", None));
    };
    let mc = with_obj(map, |o| o.slots[HM_MODCOUNT].expect_int());
    if expected != mc {
        return Err(crate::cme());
    }
    let key = node_parts(&node).1;
    n_lhm_remove(Some(&map), std::slice::from_ref(&key), ctx)?;
    let mc = with_obj(map, |o| o.slots[HM_MODCOUNT].expect_int());
    with_obj_mut(*it, |o| {
        o.slots[2] = Value::Ref(None);
        o.slots[3] = Value::Int(mc);
    });
    Ok(Value::Void)
}

// ----------------------------------------------------------------- TreeMap
//
// Run 5 (2026-11-15): the SORTED map — red-black storage (the CLRS shape
// OpenJDK uses), natural ordering through the ordinary Comparable dispatch
// walk, Comparator ordering through the Comparator walk (lambda + builtin
// comparators resolve the same way Arrays.sort(Object[], Comparator) has
// resolved since the stringsim wave).
//
// Laws (probed vs Temurin 21.0.13+9, TreeMapProbe, evidence
// scripts/evidence/treemap/treemapprobe_temurin_2026-11-15.txt):
//   T1  iteration in KEY order (the whole point) — keys ascending under
//       the map's ordering; values/entries follow KEY order; toString
//       `{k=v, k=v}` sorted
//   T2  firstKey/lastKey = min/max under the ordering
//   T3  duplicate put replaces the value, keeps the position, returns old
//   T4  remove returns the old value (absent → null), order preserved
//   T5  ceiling/floor accept exact; higher/lower are strict;
//       out-of-range → null
//   T6  the Comparator surface reorders everything (iteration, first/
//       last, navigation)
//   T7  put(null) under natural ordering → NPE (the type check runs even
//       on an empty map — OpenJDK's compare(key, key) pre-step)
//   T8  put(non-Comparable) → ClassCastException
//   T9  the copy ctor sorts regardless of the source's iteration order
//   T10 iterator remove mid-walk: order intact, no CME (expectedModCount
//       re-synced); ISE-first on null current (the run-4 probed law);
//       foreign modification → CME
//   T11 size/isEmpty/clear/getOrDefault/containsValue
//
// Slots: 0 = comparator ref (null = natural), 1 = size, 2 = modCount,
// 3 = root (TreeMap$Entry or null). Entry: 0 key, 1 value, 2 left,
// 3 right, 4 parent, 5 color (Int 0=RED/1=BLACK). Iterators: 0 map,
// 1 next (successor-walk position), 2 current (last-returned entry —
// the field remove() consumes), 3 expectedModCount. Views: map ref.
// subMap/headMap/tailMap are NOT registered this slice (the bounded
// views are their own slice; loud unsupported by the same-surface law).
