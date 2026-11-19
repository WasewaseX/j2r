use super::*;

/// `Collections.unmodifiableMap(m)` — static factory for the wrapper.
pub fn n_collections_unmodifiable_map(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let _ = recv;
    let wrapper = new_obj("java/util/Collections$UnmodifiableMap", 1)?;
    n_um_init(Some(&wrapper), args, ctx)?;
    Ok(Value::Ref(Some(wrapper)))
}
pub fn n_um_init(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: UnmodifiableMap.<init> without receiver");
    let delegate = match args.first() {
        Some(Value::Ref(Some(m))) => *m,
        _ => return Err(crate::npe()),
    };
    with_obj_mut(*r, |o| o.slots[0] = Value::Ref(Some(delegate)));
    Ok(Value::Void)
}
fn um_delegate(recv: Option<&JRef>, who: &str) -> JRef {
    let r = recv.unwrap_or_else(|| panic!("j2r internal: UnmodifiableMap.{who} without receiver"));
    with_obj(*r, |o| match &o.slots[0] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: UnmodifiableMap.{who} without delegate"),
    })
}
pub fn n_um_get(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = um_delegate(recv, "get");
    let key = arg_or_null(args);
    match hm_find(&map, &key, ctx)? {
        Some(node) => Ok(with_obj(node, |o| o.slots[2].clone())),
        None => Ok(Value::Ref(None)),
    }
}
pub fn n_um_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(with_obj(um_delegate(recv, "size"), |o| {
        o.slots[HM_SIZE].expect_int()
    })))
}
pub fn n_um_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    Ok(Value::Int(
        (with_obj(um_delegate(recv, "isEmpty"), |o| {
            o.slots[HM_SIZE].expect_int()
        }) == 0) as i32,
    ))
}
pub fn n_um_entry_set(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = um_delegate(recv, "entrySet");
    n_hm_entry_set(Some(&map), _args, _ctx)
}
/// `UnmodifiableMap.keySet()` — forwards to the delegate. The JDK wraps
/// in an UnmodifiableSet view; for read-only use (the corpus rule) the
/// delegate's keySet is observationally identical.
pub fn n_um_key_set(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let map = um_delegate(recv, "keySet");
    invoke_virtual(
        &map,
        "java/util/Map",
        "keySet",
        "()Ljava/util/Set;",
        args,
        ctx,
    )
}
/// `UnmodifiableMap.containsKey(k)` — forwards to the delegate.
pub fn n_um_contains_key(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let map = um_delegate(recv, "containsKey");
    invoke_virtual(
        &map,
        "java/util/Map",
        "containsKey",
        "(Ljava/lang/Object;)Z",
        args,
        ctx,
    )
}
pub fn n_um_put(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let _ = um_delegate(recv, "put");
    Err(throw_new("java/lang/UnsupportedOperationException", None))
}

// ---------------------------------------------------------------- LinkedList
//
// The stringsim corpus wave (Sift4's index-oriented Offset list). Same
// storage model as ArrayList (slot 0 size, slot 1 modCount, slot 2 the
// Object[] payload); the JDK's deque methods stay unregistered (loud)
// until a caller needs them — the honest surface rule.
