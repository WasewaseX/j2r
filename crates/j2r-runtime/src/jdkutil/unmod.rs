use super::*;

const UNMOD_LIST_BACKING: usize = 0;
fn unmod_backing(recv: Option<&JRef>, op: &str) -> Result<JRef, Throw> {
    let r = match recv {
        Some(r) => r,
        None => return Err(null_arg(op)),
    };
    Ok(with_obj(*r, |o| match &o.slots[UNMOD_LIST_BACKING] {
        Value::Ref(Some(b)) => *b,
        other => panic!("j2r internal: UnmodifiableList without backing list: {other:?}"),
    }))
}
/// `Collections.unmodifiableList(list)` — wraps the list in the builtin
/// `java/util/Collections$UnmodifiableList` view object.
pub fn n_unmod_list(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let list = match args.first() {
        Some(Value::Ref(Some(l))) => *l,
        _ => return Err(null_arg("Collections.unmodifiableList")),
    };
    let view = crate::heap::alloc(crate::object::JObj {
        jclass: Arc::from("java/util/Collections$UnmodifiableList"),
        slots: vec![Value::Ref(Some(list))],
        prim: None,
    });
    Ok(Value::Ref(Some(view)))
}
pub fn n_unmod_get(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let backing = unmod_backing(recv, "UnmodifiableList.get")?;
    collections::n_al_get(Some(&backing), args, ctx)
}
pub fn n_unmod_size(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let backing = unmod_backing(recv, "UnmodifiableList.size")?;
    collections::n_al_size(Some(&backing), args, ctx)
}
pub fn n_unmod_is_empty(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let backing = unmod_backing(recv, "UnmodifiableList.isEmpty")?;
    collections::n_al_is_empty(Some(&backing), args, ctx)
}
pub fn n_unmod_contains(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let backing = unmod_backing(recv, "UnmodifiableList.contains")?;
    collections::n_al_contains(Some(&backing), args, ctx)
}
pub fn n_unmod_index_of(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let backing = unmod_backing(recv, "UnmodifiableList.indexOf")?;
    collections::n_al_index_of(Some(&backing), args, ctx)
}
/// Live iteration over the backing list — the JDK wrapper's iterator
/// delegates the same way, so backing mutations made after `iterator()`
/// are visible, exactly like Temurin.
pub fn n_unmod_iterator(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let backing = unmod_backing(recv, "UnmodifiableList.iterator")?;
    collections::n_al_iterator(Some(&backing), args, ctx)
}
pub fn n_unmod_to_string(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let backing = unmod_backing(recv, "UnmodifiableList.toString")?;
    collections::n_al_to_string(Some(&backing), args, ctx)
}
fn unmod_readonly(op: &str) -> Result<Value, Throw> {
    Err(throw_new(
        "java/lang/UnsupportedOperationException",
        Some(&format!(
            "J2R: unmodifiable list rejects {op} (JDK view semantics)"
        )),
    ))
}
pub fn n_unmod_set(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    unmod_readonly("set")
}
pub fn n_unmod_add(_recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    unmod_readonly("add")
}
pub fn n_unmod_add_at(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    unmod_readonly("add(index, element)")
}
pub fn n_unmod_remove_at(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    unmod_readonly("remove(index)")
}
pub fn n_unmod_remove_obj(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    unmod_readonly("remove(object)")
}
pub fn n_unmod_clear(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    unmod_readonly("clear")
}
pub fn n_unmod_hash_code(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let backing = unmod_backing(recv, "UnmodifiableList.hashCode")?;
    collections::n_al_hash_code(Some(&backing), args, ctx)
}
pub fn n_unmod_equals(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let backing = unmod_backing(recv, "UnmodifiableList.equals")?;
    collections::n_al_equals(Some(&backing), args, ctx)
}
pub fn n_unmod_last_index_of(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let backing = unmod_backing(recv, "UnmodifiableList.lastIndexOf")?;
    collections::n_al_last_index_of(Some(&backing), args, ctx)
}

// ---------------------------------------------------------------------------
// EXTERNAL-B11 (jbox2d port, 2026-11-13): ArrayList.toArray() and
// toArray(T[]) — the JDK's two-branch contract, byte-exact.
//
// OpenJDK ArrayList.java:
//   toArray()        -> Arrays.copyOf(elementData, size)  (Object[])
//   toArray(T[] a)   -> a.length < size : a fresh array of A'S RUNTIME
//                                        component type, size elements
//                      a.length >= size : copy into a; a.length > size
//                                        writes the null terminator;
//                                        returns a itself.
// The runtime component type comes from the passed array's descriptor,
// so `Integer[]` lists land in `Number[]` or `Object[]` receivers with
// the JDK's exact ArrayStoreException law when the store is illegal
// (array_store implements the JVM's aastore check).
