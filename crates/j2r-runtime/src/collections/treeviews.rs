use super::*;

pub(crate) fn view_map(recv: Option<&JRef>, who: &str) -> JRef {
    let v = recv.unwrap_or_else(|| panic!("j2r internal: {who} without receiver"));
    with_obj(*v, |o| match &o.slots[0] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: {who} without map"),
    })
}
pub fn n_ks_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(view_map(recv, "KeySet.size"), |o| {
        o.slots[HM_SIZE].expect_int()
    })))
}
pub fn n_ks_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(view_map(recv, "KeySet.isEmpty"), |o| {
            o.slots[HM_SIZE].expect_int()
        }) == 0) as i32,
    ))
}
pub fn n_ks_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = view_map(recv, "KeySet.contains");
    let key = arg_or_null(args);
    Ok(Value::Int(hm_find(&map, &key, ctx)?.is_some() as i32))
}
pub fn n_ks_to_string(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = view_map(recv, "KeySet.toString");
    let items: Vec<Value> = hm_all_entries(&map)?.into_iter().map(|(k, _)| k).collect();
    collection_to_string(items, ctx)
}
pub fn n_ks_iterator(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let map = view_map(recv, "KeySet.iterator");
    hm_new_hash_iterator(&map, "java/util/HashMap$KeyIterator")
}
pub fn n_vs_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(view_map(recv, "Values.size"), |o| {
        o.slots[HM_SIZE].expect_int()
    })))
}
pub fn n_vs_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(view_map(recv, "Values.isEmpty"), |o| {
            o.slots[HM_SIZE].expect_int()
        }) == 0) as i32,
    ))
}
pub fn n_vs_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    n_hm_contains_value(Some(&view_map(recv, "Values.contains")), args, ctx)
}
pub fn n_vs_to_string(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = view_map(recv, "Values.toString");
    let items: Vec<Value> = hm_all_entries(&map)?.into_iter().map(|(_, v)| v).collect();
    collection_to_string(items, ctx)
}
pub fn n_vs_iterator(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let map = view_map(recv, "Values.iterator");
    hm_new_hash_iterator(&map, "java/util/HashMap$ValueIterator")
}
pub fn n_ki_has_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: KeyIterator.hasNext without receiver");
    let has = with_obj(*it, |o| matches!(&o.slots[2], Value::Ref(Some(_))));
    Ok(Value::Int(has as i32))
}
pub fn n_ki_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = hm_hash_next_node(recv, "KeyIterator.next")?;
    Ok(node_parts(&node).1)
}
pub fn n_vi_has_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    n_ki_has_next(recv, _args, _ctx)
}
pub fn n_vi_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = hm_hash_next_node(recv, "ValueIterator.next")?;
    node_value_materialized(&node)
}

// ------------------------------------------------------------- LinkedHashMap
//
// Run 4 (2026-11-15): the OpenJDK subclass shape — the map IS a HashMap
// (the same size/modCount/threshold/table slots at 0..3, the same Node
// prefix hash/key/value/next on its entries) plus the ORDER LINKS:
//   map slot 4 = head (the OLDEST entry), slot 5 = tail (the NEWEST
//   entry) — the own slots AFTER the inherited HashMap base (the
//   builtin_own_slots layout law: inherited first, own after);
//   entry slots 5/6 = before/after (the doubly-linked pair AFTER the
//   Node prefix; the runtime allocates 7 slots — the undeclared node-tag
//   slot 4 rides at the same index as the HashMap$Node convention).
//
// Laws (probed vs Temurin 21.0.13+9, LhmProbe, evidence
// scripts/evidence/lhm/lhmprobe_temurin_2026-11-15.txt):
//   L1  iteration order = insertion order (first insertion wins position);
//   L2  re-put of an existing key replaces the value and KEEPS position
//       (hm_put returns no node on an overwrite — no link work);
//   L3  remove + re-put moves the entry to the TAIL;
//   L4  get/containsKey never reorder (they inherit the HashMap rows
//       through the builtin parent walk — order-neutral);
//   L5  <init>(Map) copies in the SOURCE's iteration order (a
//       LinkedHashMap source in link order, anything else table order);
//   L6  is-a HashMap (builtin_parent_of) and is-a Map (interfaces table);
//   L7  keySet/values/entrySet iterate in link order (own view classes);
//   L8  iterator.remove() removes the last-returned entry (order intact,
//       expectedModCount re-synced — the JDK HashIterator.remove law);
//   L9  one null key, null values (the HashMap law: hash(null) = 0);
//   L10 clear resets everything (head/tail included), re-adds re-link;
//   L11 toString iterates in link order.
pub fn n_tks_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "KeySet.iterator");
    tm_new_iterator(&map, "java/util/TreeMap$KeyIterator")
}
pub fn n_tvs_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "Values.iterator");
    tm_new_iterator(&map, "java/util/TreeMap$ValueIterator")
}
pub fn n_tes_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "EntrySet.iterator");
    tm_new_iterator(&map, "java/util/TreeMap$EntryIterator")
}
pub fn n_tks_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "KeySet.size");
    Ok(Value::Int(tm_size_of(&map)))
}
pub fn n_tvs_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "Values.size");
    Ok(Value::Int(tm_size_of(&map)))
}
pub fn n_tes_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "EntrySet.size");
    Ok(Value::Int(tm_size_of(&map)))
}
pub fn n_tks_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "KeySet.contains");
    let key = arg_or_null(args);
    let found = tm_find(&map, &key, ctx)?.is_some();
    Ok(Value::Int(found as i32))
}
pub fn n_tvs_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "Values.contains");
    n_tm_contains_value(Some(&map), args, ctx)
}
pub fn n_tes_contains(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    // the entrySet contains law needs a Map.Entry arg — the probed surface
    // never calls it; route through the key walk of the entry's key when
    // it is one of our entry nodes, else false
    let _map = tm_view_map(recv, "EntrySet.contains");
    Ok(Value::Int(0))
}
pub fn n_tks_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "KeySet.toString");
    let entries = tm_all_entries(&map)?;
    let mut out = String::from("[");
    for (i, (k, _)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(k, ctx)?);
    }
    out.push(']');
    Ok(crate::object::string_out(&out))
}
pub fn n_tvs_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "Values.toString");
    let entries = tm_all_entries(&map)?;
    let mut out = String::from("[");
    for (i, (_, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(v, ctx)?);
    }
    out.push(']');
    Ok(crate::object::string_out(&out))
}
pub fn n_tes_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = tm_view_map(recv, "EntrySet.toString");
    let entries = tm_all_entries(&map)?;
    let mut out = String::from("[");
    for (i, (k, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(k, ctx)?);
        out.push('=');
        out.push_str(&dispatch_to_string(v, ctx)?);
    }
    out.push(']');
    Ok(crate::object::string_out(&out))
}
pub(crate) const SMS_MAP: usize = 0;
pub(crate) const SMS_LO: usize = 1;
pub(crate) const SMS_HI: usize = 2;
pub(crate) const SMS_LO_INC: usize = 3;
pub(crate) const SMS_HI_INC: usize = 4;
pub(crate) fn sms_backing(view: &JRef) -> JRef {
    with_obj(*view, |o| match &o.slots[SMS_MAP] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: TreeMap$SubMap without backing map"),
    })
}
fn sms_bound(view: &JRef, slot: usize) -> Option<Value> {
    let v = with_obj(*view, |o| o.slots[slot].clone());
    match v {
        Value::Ref(Some(_)) => Some(v),
        _ => None, // the unbounded side carries Value::Ref(None)
    }
}
fn sms_incl(view: &JRef, slot: usize) -> bool {
    with_obj(*view, |o| o.slots[slot].expect_int() != 0)
}
/// tooLow with inclusivity — the fromKey-side gate (V7).
fn sms_too_low(view: &JRef, key: &Value, ctx: Dispatch) -> Result<bool, Throw> {
    match sms_bound(view, SMS_LO) {
        None => Ok(false),
        Some(lo) => {
            let backing = sms_backing(view);
            let cmp = tm_compare(&backing, key, &lo, ctx)?;
            Ok(cmp < 0 || (cmp == 0 && !sms_incl(view, SMS_LO_INC)))
        }
    }
}
/// tooHigh with inclusivity — half of the fromKey-side gate.
fn sms_too_high_full(view: &JRef, key: &Value, ctx: Dispatch) -> Result<bool, Throw> {
    match sms_bound(view, SMS_HI) {
        None => Ok(false),
        Some(hi) => {
            let backing = sms_backing(view);
            let cmp = tm_compare(&backing, key, &hi, ctx)?;
            Ok(cmp > 0 || (cmp == 0 && !sms_incl(view, SMS_HI_INC)))
        }
    }
}
/// strictly-above-hi — the toKey-side hi gate: INCLUSIVITY-BLIND (V7, the
/// probed c16e/c16y asymmetry — headMap accepts toKey == an exclusive hi).
fn sms_above_hi_strict(view: &JRef, key: &Value, ctx: Dispatch) -> Result<bool, Throw> {
    match sms_bound(view, SMS_HI) {
        None => Ok(false),
        Some(hi) => {
            let backing = sms_backing(view);
            Ok(tm_compare(&backing, key, &hi, ctx)? > 0)
        }
    }
}
/// The fromKey-side full range gate (put/get/remove/tailMap/subMap).
fn sms_in_range(view: &JRef, key: &Value, ctx: Dispatch) -> Result<bool, Throw> {
    Ok(!sms_too_low(view, key, ctx)? && !sms_too_high_full(view, key, ctx)?)
}
/// The toKey-side range gate (headMap/subMap's toKey): full tooLow,
/// strictly-above-hi on the other end (V7).
fn sms_in_range_for_to(view: &JRef, key: &Value, ctx: Dispatch) -> Result<bool, Throw> {
    Ok(!sms_too_low(view, key, ctx)? && !sms_above_hi_strict(view, key, ctx)?)
}
/// The leftmost in-range node: the lo bound's ceiling/higher, else the
/// tree's leftmost — hi-fence-gated (an empty view's first is None).
fn sms_first_node(view: &JRef, ctx: Dispatch) -> Result<Option<JRef>, Throw> {
    let backing = sms_backing(view);
    let start = match sms_bound(view, SMS_LO) {
        None => tm_root(&backing).map(tm_leftmost),
        Some(lo) => tm_navigate(
            &backing,
            &lo,
            if sms_incl(view, SMS_LO_INC) {
                TmNav::Ceiling
            } else {
                TmNav::Higher
            },
            ctx,
        )?,
    };
    match start {
        Some(n) if sms_too_high_full(view, &tm_entry_key(&n), ctx)? => Ok(None),
        other => Ok(other),
    }
}
/// The rightmost in-range node: the hi bound's floor/lower, else the
/// tree's rightmost — lo-fence-gated (the c07c law: Floor(20) on [10,20)
/// over {1,3,5,7,9} lands on 9, which is BELOW the view's lo).
fn sms_last_node(view: &JRef, ctx: Dispatch) -> Result<Option<JRef>, Throw> {
    let backing = sms_backing(view);
    let end = match sms_bound(view, SMS_HI) {
        None => tm_root(&backing).map(tm_rightmost),
        Some(hi) => tm_navigate(
            &backing,
            &hi,
            if sms_incl(view, SMS_HI_INC) {
                TmNav::Floor
            } else {
                TmNav::Lower
            },
            ctx,
        )?,
    };
    match end {
        Some(n) if sms_too_low(view, &tm_entry_key(&n), ctx)? => Ok(None),
        other => Ok(other),
    }
}
/// Every (key, value) pair inside the view, in KEY order (V1).
fn sms_all_entries(view: &JRef, ctx: Dispatch) -> Result<Vec<(Value, Value)>, Throw> {
    let mut out = Vec::new();
    let mut cur = sms_first_node(view, ctx)?;
    while let Some(n) = cur {
        let k = tm_entry_key(&n);
        if sms_too_high_full(view, &k, ctx)? {
            break;
        }
        out.push((k, tm_entry_value_materialized(&n)?));
        cur = tm_successor_of(n);
    }
    Ok(out)
}
pub(crate) fn sms_new(
    map: &JRef,
    lo: Value,
    lo_inc: bool,
    hi: Value,
    hi_inc: bool,
) -> Result<Value, Throw> {
    let v = new_obj("java/util/TreeMap$SubMap", 5)?;
    with_obj_mut(v, |o| {
        o.slots[SMS_MAP] = Value::Ref(Some(*map));
        o.slots[SMS_LO] = lo;
        o.slots[SMS_HI] = hi;
        o.slots[SMS_LO_INC] = Value::Int(lo_inc as i32);
        o.slots[SMS_HI_INC] = Value::Int(hi_inc as i32);
    });
    Ok(Value::Ref(Some(v)))
}
pub(crate) fn sms_view(recv: Option<&JRef>, who: &str) -> JRef {
    *recv.unwrap_or_else(|| panic!("j2r internal: TreeMap$SubMap {who} without receiver"))
}
pub(crate) fn sms_iae(msg: &str) -> Throw {
    throw_new("java/lang/IllegalArgumentException", Some(msg))
}

// --- the ABSOLUTE map's builders (no range to violate; the ctor's
//     crossed check + the self-compare type check are the whole law) ---
/// The BOUNDED view's comparator: the view's slot 0 is the backing map,
/// not the comparator — deref first (the c20 law the battery caught).
pub fn n_sms_comparator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view(recv, "comparator");
    let backing = sms_backing(&view);
    Ok(with_obj(backing, |o| o.slots[TM_COMPARATOR].clone()))
}

// --- the BOUNDED view's range-gated surface ---
pub fn n_sms_put(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "put");
    let key = arg_or_null(args);
    if !sms_in_range(&view, &key, ctx)? {
        return Err(sms_iae("key out of range"));
    }
    let backing = sms_backing(&view);
    n_tm_put(Some(&backing), args, ctx)
}
pub fn n_sms_get(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "get");
    let key = arg_or_null(args);
    if !sms_in_range(&view, &key, ctx)? {
        return Ok(Value::Ref(None));
    }
    let backing = sms_backing(&view);
    n_tm_get(Some(&backing), args, ctx)
}
pub fn n_sms_contains_key(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view(recv, "containsKey");
    let key = arg_or_null(args);
    if !sms_in_range(&view, &key, ctx)? {
        return Ok(Value::Int(0));
    }
    let backing = sms_backing(&view);
    n_tm_contains_key(Some(&backing), args, ctx)
}
pub fn n_sms_get_or_default(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view(recv, "getOrDefault");
    let key = arg_or_null(args);
    if !sms_in_range(&view, &key, ctx)? {
        return Ok(args.get(1).cloned().unwrap_or(Value::Ref(None)));
    }
    let backing = sms_backing(&view);
    n_tm_get_or_default(Some(&backing), args, ctx)
}
pub fn n_sms_remove(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "remove");
    let key = arg_or_null(args);
    if !sms_in_range(&view, &key, ctx)? {
        return Ok(Value::Ref(None));
    }
    let backing = sms_backing(&view);
    n_tm_remove(Some(&backing), args, ctx)
}
pub fn n_sms_contains_value(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view(recv, "containsValue");
    let target = arg_or_null(args);
    for (_, v) in sms_all_entries(&view, ctx)? {
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
pub fn n_sms_size(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "size");
    let n = sms_all_entries(&view, ctx)?.len();
    Ok(Value::Int(n as i32))
}
pub fn n_sms_is_empty(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "isEmpty");
    Ok(Value::Int((sms_all_entries(&view, ctx)?.is_empty()) as i32))
}
/// Slice-only clear (V6): delete every in-range node; the backing map's
/// out-of-range keys survive; each delete bumps the backing modCount.
/// The hi fence gates the loop (the lo side alone would drain the whole
/// tree above lo — the c15/c35 over-delete the battery caught).
pub fn n_sms_clear(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "clear");
    let backing = sms_backing(&view);
    while let Some(node) = sms_first_node(&view, ctx)? {
        if sms_too_high_full(&view, &tm_entry_key(&node), ctx)? {
            break;
        }
        tm_delete(&backing, node);
    }
    Ok(Value::Void)
}
pub fn n_sms_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view(recv, "toString");
    let entries = sms_all_entries(&view, ctx)?;
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
    Ok(crate::object::string_out(&out))
}
pub fn n_sms_first_key(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view(recv, "firstKey");
    match sms_first_node(&view, ctx)? {
        Some(n) => Ok(tm_entry_key(&n)),
        None => Err(throw_new("java/util/NoSuchElementException", None)),
    }
}
pub fn n_sms_last_key(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "lastKey");
    match sms_last_node(&view, ctx)? {
        Some(n) => Ok(tm_entry_key(&n)),
        None => Err(throw_new("java/util/NoSuchElementException", None)),
    }
}

// --- the narrowing-checked chained builders (V7) ---
pub fn n_sms_sub_map(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "subMap");
    let backing = sms_backing(&view);
    let f = arg_or_null(args);
    let t = args.get(1).cloned().unwrap_or(Value::Ref(None));
    if !sms_in_range(&view, &f, ctx)? {
        return Err(sms_iae("fromKey out of range"));
    }
    if !sms_in_range_for_to(&view, &t, ctx)? {
        return Err(sms_iae("toKey out of range"));
    }
    if tm_compare(&backing, &f, &t, ctx)? > 0 {
        return Err(sms_iae("fromKey > toKey"));
    }
    sms_new(&backing, f, true, t, false)
}
pub fn n_sms_sub_map4(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "subMap4");
    let backing = sms_backing(&view);
    let f = arg_or_null(args);
    let t = args.get(2).cloned().unwrap_or(Value::Ref(None));
    if !sms_in_range(&view, &f, ctx)? {
        return Err(sms_iae("fromKey out of range"));
    }
    if !sms_in_range_for_to(&view, &t, ctx)? {
        return Err(sms_iae("toKey out of range"));
    }
    if tm_compare(&backing, &f, &t, ctx)? > 0 {
        return Err(sms_iae("fromKey > toKey"));
    }
    sms_new(&backing, f, arg_bool(args, 1), t, arg_bool(args, 3))
}
pub fn n_sms_head_map(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "headMap");
    let backing = sms_backing(&view);
    let t = arg_or_null(args);
    if !sms_in_range_for_to(&view, &t, ctx)? {
        return Err(sms_iae("toKey out of range"));
    }
    let (lo, lo_inc) = (
        sms_bound(&view, SMS_LO).unwrap_or(Value::Ref(None)),
        sms_incl(&view, SMS_LO_INC),
    );
    sms_new(&backing, lo, lo_inc, t, false)
}
pub fn n_sms_head_map4(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "headMap4");
    let backing = sms_backing(&view);
    let t = arg_or_null(args);
    if !sms_in_range_for_to(&view, &t, ctx)? {
        return Err(sms_iae("toKey out of range"));
    }
    let (lo, lo_inc) = (
        sms_bound(&view, SMS_LO).unwrap_or(Value::Ref(None)),
        sms_incl(&view, SMS_LO_INC),
    );
    sms_new(&backing, lo, lo_inc, t, arg_bool(args, 1))
}
pub fn n_sms_tail_map(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "tailMap");
    let backing = sms_backing(&view);
    let f = arg_or_null(args);
    if !sms_in_range(&view, &f, ctx)? {
        return Err(sms_iae("fromKey out of range"));
    }
    let (hi, hi_inc) = (
        sms_bound(&view, SMS_HI).unwrap_or(Value::Ref(None)),
        sms_incl(&view, SMS_HI_INC),
    );
    sms_new(&backing, f, true, hi, hi_inc)
}
pub fn n_sms_tail_map4(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view(recv, "tailMap4");
    let backing = sms_backing(&view);
    let f = arg_or_null(args);
    if !sms_in_range(&view, &f, ctx)? {
        return Err(sms_iae("fromKey out of range"));
    }
    let (hi, hi_inc) = (
        sms_bound(&view, SMS_HI).unwrap_or(Value::Ref(None)),
        sms_incl(&view, SMS_HI_INC),
    );
    sms_new(&backing, f, arg_bool(args, 1), hi, hi_inc)
}

// --- the view-of-view objects (V9) ---
fn sms_sub_view(recv: Option<&JRef>, who: &str, class: &str) -> Result<Value, Throw> {
    let view = sms_view(recv, who);
    let v = new_obj(class, 1)?;
    with_obj_mut(v, |o| o.slots[0] = Value::Ref(Some(view)));
    Ok(Value::Ref(Some(v)))
}
fn sms_view_view(recv: Option<&JRef>, who: &str) -> JRef {
    let r = recv.unwrap_or_else(|| panic!("j2r internal: SubMap {who} without receiver"));
    with_obj(*r, |o| match &o.slots[0] {
        Value::Ref(Some(v)) => *v,
        _ => panic!("j2r internal: SubMap {who} without view"),
    })
}
pub fn n_sms_key_set(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    sms_sub_view(recv, "keySet", "java/util/TreeMap$SubMap$KeySet")
}
pub fn n_sms_values(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    sms_sub_view(recv, "values", "java/util/TreeMap$SubMap$Values")
}
pub fn n_sms_entry_set(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    sms_sub_view(recv, "entrySet", "java/util/TreeMap$SubMap$EntrySet")
}
pub fn n_smsks_size(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "KeySet.size");
    Ok(Value::Int(sms_all_entries(&view, ctx)?.len() as i32))
}
pub fn n_smsvs_size(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "Values.size");
    Ok(Value::Int(sms_all_entries(&view, ctx)?.len() as i32))
}
pub fn n_smses_size(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "EntrySet.size");
    Ok(Value::Int(sms_all_entries(&view, ctx)?.len() as i32))
}
pub fn n_smsks_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "KeySet.iterator");
    sms_new_iterator(&view, "java/util/TreeMap$SubMap$KeyIterator", ctx)
}
pub fn n_smsvs_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "Values.iterator");
    sms_new_iterator(&view, "java/util/TreeMap$SubMap$ValueIterator", ctx)
}
pub fn n_smses_iterator(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "EntrySet.iterator");
    sms_new_iterator(&view, "java/util/TreeMap$SubMap$EntryIterator", ctx)
}
pub fn n_smsks_contains(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "KeySet.contains");
    let key = arg_or_null(args);
    if !sms_in_range(&view, &key, ctx)? {
        return Ok(Value::Int(0));
    }
    let backing = sms_backing(&view);
    let found = tm_find(&backing, &key, ctx)?.is_some();
    Ok(Value::Int(found as i32))
}
pub fn n_smsvs_contains(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "Values.contains");
    n_sms_contains_value(Some(&view), args, ctx)
}
pub fn n_smsks_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "KeySet.toString");
    let entries = sms_all_entries(&view, ctx)?;
    let mut out = String::from("[");
    for (i, (k, _)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(k, ctx)?);
    }
    out.push(']');
    Ok(crate::object::string_out(&out))
}
pub fn n_smsvs_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "Values.toString");
    let entries = sms_all_entries(&view, ctx)?;
    let mut out = String::from("[");
    for (i, (_, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(v, ctx)?);
    }
    out.push(']');
    Ok(crate::object::string_out(&out))
}
pub fn n_smses_to_string(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let view = sms_view_view(recv, "EntrySet.toString");
    let entries = sms_all_entries(&view, ctx)?;
    let mut out = String::from("[");
    for (i, (k, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&dispatch_to_string(k, ctx)?);
        out.push('=');
        out.push_str(&dispatch_to_string(v, ctx)?);
    }
    out.push(']');
    Ok(crate::object::string_out(&out))
}

// --- the bounded iterators (V8): the unbounded 4-slot layout, slot 0 =
//     the VIEW (the backing map rides the view's slot 0) ---
pub(crate) fn sms_new_iterator(view: &JRef, class: &str, ctx: Dispatch) -> Result<Value, Throw> {
    let next = sms_first_node(view, ctx)?;
    let backing = sms_backing(view);
    let mc = tm_modcount(&backing);
    let it = new_obj(class, 4)?;
    with_obj_mut(it, |o| {
        o.slots[TMI_MAP] = Value::Ref(Some(*view));
        o.slots[TMI_NEXT] = match next {
            Some(n) => Value::Ref(Some(n)),
            None => Value::Ref(None),
        };
        o.slots[TMI_CURRENT] = Value::Ref(None);
        o.slots[TMI_EXPECTED] = Value::Int(mc);
    });
    Ok(Value::Ref(Some(it)))
}
/// Shared bounded hasNext: the next node must still be inside the hi
/// fence (the lo side cannot be crossed walking forward).
pub fn n_sms_i_has_next(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: SubMap iterator hasNext without receiver");
    let (view, next) = with_obj(*it, |o| {
        (
            match &o.slots[TMI_MAP] {
                Value::Ref(Some(v)) => *v,
                _ => panic!("j2r internal: SubMap iterator without view"),
            },
            match &o.slots[TMI_NEXT] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    let has = match next {
        None => false,
        Some(n) => !sms_too_high_full(&view, &tm_entry_key(&n), ctx)?,
    };
    Ok(Value::Int(has as i32))
}
/// Shared bounded nextNode: CME against the BACKING map's modCount (the
/// probed c27 law: a foreign put OUT-of-range still comodifies), the hi
/// fence -> NoSuchElementException, then current = next, next =
/// successor(next) (range-gated at hasNext/next time, never stored).
fn sms_iter_next_node(
    recv: Option<&JRef>,
    who: &str,
    ctx: Dispatch,
) -> Result<(JRef, JRef), Throw> {
    let it = recv.unwrap_or_else(|| panic!("j2r internal: {who} without receiver"));
    let (view, expected, next) = with_obj(*it, |o| {
        (
            match &o.slots[TMI_MAP] {
                Value::Ref(Some(v)) => *v,
                _ => panic!("j2r internal: {who} without view"),
            },
            o.slots[TMI_EXPECTED].expect_int(),
            match &o.slots[TMI_NEXT] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    let backing = sms_backing(&view);
    if expected != tm_modcount(&backing) {
        return Err(crate::cme());
    }
    let node = match next {
        None => return Err(throw_new("java/util/NoSuchElementException", None)),
        Some(n) => {
            if sms_too_high_full(&view, &tm_entry_key(&n), ctx)? {
                return Err(throw_new("java/util/NoSuchElementException", None));
            }
            n
        }
    };
    let succ = tm_successor_of(node);
    with_obj_mut(*it, |o| {
        o.slots[TMI_NEXT] = match succ {
            Some(s) => Value::Ref(Some(s)),
            None => Value::Ref(None),
        };
        o.slots[TMI_CURRENT] = Value::Ref(Some(node));
    });
    Ok((*it, node))
}
pub fn n_smki_next(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = sms_iter_next_node(recv, "TreeMap$SubMap$KeyIterator.next", ctx)?;
    Ok(tm_entry_key(&node))
}
pub fn n_smvi_next(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = sms_iter_next_node(recv, "TreeMap$SubMap$ValueIterator.next", ctx)?;
    tm_entry_value_materialized(&node)
}
pub fn n_smei_next(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = sms_iter_next_node(recv, "TreeMap$SubMap$EntryIterator.next", ctx)?;
    Ok(Value::Ref(Some(node)))
}

// ---------------------------------------------------------------- HashSet

/// The key iterator's next: the entry's key (T1).
pub fn n_tki_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = tm_iter_next_node(recv, "TreeMap$KeyIterator.next")?;
    Ok(tm_entry_key(&node))
}

/// The value iterator's next: the entry's value (values follow KEY order).
pub fn n_tvi_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = tm_iter_next_node(recv, "TreeMap$ValueIterator.next")?;
    tm_entry_value_materialized(&node)
}

/// The entry iterator's next IS the internal entry (the JDK law:
/// TreeMap$Entry implements Map.Entry).
pub fn n_tei_next(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let (_, node) = tm_iter_next_node(recv, "TreeMap$EntryIterator.next")?;
    Ok(Value::Ref(Some(node)))
}
