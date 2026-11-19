use super::*;

pub(crate) const HS_MAP: usize = 0;
pub fn n_hs_init(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: HashSet.<init> without receiver");
    let map = new_obj("java/util/HashMap", 4)?;
    n_hm_init(Some(&map), args, ctx)?;
    with_obj_mut(*r, |o| o.slots[HS_MAP] = Value::Ref(Some(map)));
    Ok(Value::Void)
}
fn hs_map(recv: Option<&JRef>, who: &str) -> JRef {
    let r = recv.unwrap_or_else(|| panic!("j2r internal: HashSet.{who} without receiver"));
    with_obj(*r, |o| match &o.slots[HS_MAP] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: HashSet.{who} without backing map"),
    })
}
pub fn n_hs_add(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = hs_map(recv, "add");
    let e = arg_or_null(args);
    // Java: map.put(e, PRESENT) == null — PRESENT is a shared dummy whose
    // only requirement is being a live heap object (roots scan it through
    // the map's node slots).
    let present = new_obj("java/lang/Object", 0)?;
    let (old, _) = hm_put(
        &map,
        e,
        Value::Ref(Some(present)),
        ctx,
        None,
        NodeKind::HashNode,
    )?;
    Ok(Value::Int(old.is_null() as i32))
}
pub fn n_hs_remove(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = hs_map(recv, "remove");
    let e = arg_or_null(args);
    let old = hm_remove(&map, &e, ctx)?;
    Ok(Value::Int((!old.is_null()) as i32))
}
/// `Set.addAll(Collection)` — the stringsim corpus wave (Jaccard's union).
/// Iterates through Collection dispatch so any modelled collection works.
pub fn n_hs_add_all(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let set = recv.expect("j2r internal: Set.addAll without receiver");
    let coll = match args.first() {
        Some(Value::Ref(Some(c))) => *c,
        Some(Value::Ref(None)) | None => return Err(crate::npe()),
        _ => return Err(internal_error("Set.addAll needs a Collection")),
    };
    // Lock law L3: no heap access nests inside a heap guard — read the
    // backing map ref first, then its size, as two flat accesses.
    let map = with_obj(*set, |o| match &o.slots[HS_MAP] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: HashSet backing map"),
    });
    let before = with_obj(map, |m| m.slots[HM_SIZE].expect_int());
    let it = invoke_virtual(
        &coll,
        "java/util/Collection",
        "iterator",
        "()Ljava/util/Iterator;",
        &[],
        ctx,
    )?;
    let it_ref = match it {
        Value::Ref(Some(r)) => r,
        _ => return Err(internal_error("Set.addAll: iterator() not an object")),
    };
    loop {
        let has = invoke_virtual(&it_ref, "java/util/Iterator", "hasNext", "()Z", &[], ctx)?;
        if !matches!(has, Value::Int(1)) {
            break;
        }
        let v = invoke_virtual(
            &it_ref,
            "java/util/Iterator",
            "next",
            "()Ljava/lang/Object;",
            &[],
            ctx,
        )?;
        invoke_virtual(
            set,
            "java/util/Set",
            "add",
            "(Ljava/lang/Object;)Z",
            std::slice::from_ref(&v),
            ctx,
        )?;
    }
    let after = with_obj(map, |m| m.slots[HM_SIZE].expect_int());
    Ok(Value::Int((after != before) as i32))
}
pub fn n_hs_contains(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = hs_map(recv, "contains");
    let e = arg_or_null(args);
    Ok(Value::Int(hm_find(&map, &e, ctx)?.is_some() as i32))
}
pub fn n_hs_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(hs_map(recv, "size"), |o| {
        o.slots[HM_SIZE].expect_int()
    })))
}
pub fn n_hs_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(hs_map(recv, "isEmpty"), |o| o.slots[HM_SIZE].expect_int()) == 0) as i32,
    ))
}
pub fn n_hs_iterator(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let map = hs_map(recv, "iterator");
    hm_new_hash_iterator(&map, "java/util/HashMap$KeyIterator")
}
pub fn n_hs_to_string(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = hs_map(recv, "toString");
    let items: Vec<Value> = hm_all_entries(&map)?.into_iter().map(|(k, _)| k).collect();
    collection_to_string(items, ctx)
}

// ---------------------------------------------------------------- Objects
// ---------------------------------------------------------------- EntrySet
//
// The stringsim corpus wave (2026-11-07): profile iteration through
// Map.entrySet(). The JDK returns the INTERNAL Node objects as the
// Map.Entry values (HashMap$Node implements Map.Entry), so the entry
// iterator hands out the node itself and getKey/getValue are registered
// as virtual rows on HashMap$Node. The EntrySet view is slot-0-delegate
// like KeySet/Values; iteration order is the map's table order, already
// byte-identical to Temurin by the HashMapOrder law.
