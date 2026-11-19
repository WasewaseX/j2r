use super::*;

/// The red-black storage's materialized value reader (G3.3 slice 4): the
/// SAME law at the TreeMap$Entry layout. Routed at every semantic value
/// reader — n_tm_get/get_or_default/contains_value, the remove and
/// put-overwrite returns, tm_all_entries (toString/containsValue/the
/// view builders), the value iterators, entry getValue, and the sms
/// view entries — so the bounded views ride by construction.
pub(crate) fn tm_entry_value_materialized(e: &JRef) -> Result<Value, Throw> {
    node_value_materialized_at(e, TME_VALUE, TME_TAG)
}
/// G3.3 slice 4: the fused get law at the RED-BLACK storage — the same
/// failure modes as [`hm_get_prim`], byte-for-byte, with the walk (and
/// the compare/CCE law on the key) being tm_find's own:
/// - a MISSING key is the null box reaching `xValue()`: the bare NPE;
/// - a stored null VALUE is the same bare NPE;
/// - a FOREIGN box fails the faithful checkcast through the shared
///   helper (the exact CCE bytes);
/// - a raw-prim slot whose tag disagrees with `expect` is the same CCE
///   with the tag's class as the actual type;
/// - a matching box or tag unwraps to the prim payload.
pub fn tm_get_prim(
    map: &JRef,
    key: Value,
    expect: &'static str,
    ctx: Dispatch,
) -> Result<Value, Throw> {
    match tm_find(map, &key, ctx)? {
        Some(node) => {
            let v = with_obj(node, |o| o.slots[TME_VALUE].clone());
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
                    let tag = with_obj(node, |o| o.slots[TME_TAG].clone());
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
pub(crate) const TM_COMPARATOR: usize = 0;
pub(crate) const TM_SIZE: usize = 1;
pub(crate) const TM_MODCOUNT: usize = 2;
pub(crate) const TM_ROOT: usize = 3;
pub(crate) const TME_KEY: usize = 0;
pub(crate) const TME_VALUE: usize = 1;
pub(crate) const TME_LEFT: usize = 2;
pub(crate) const TME_RIGHT: usize = 3;
pub(crate) const TME_PARENT: usize = 4;
pub(crate) const TME_COLOR: usize = 5; // Int: 0 = RED, 1 = BLACK
                                       // G3.3 slice 4: the class tag (the slice-3 law, now on the red-black
                                       // storage too): Void = a faithful node; Int(desc char) = slot TME_VALUE
                                       // holds a RAW prim written by tm_put_prim. The tag is an ANNOTATION, not
                                       // a mode switch — a faithful overwrite clears it, a faithful overwrite
                                       // of a tagged slot leaves a box in the slot and the tag passes through
                                       // every reader.
pub(crate) const TME_TAG: usize = 6;
pub(crate) const TM_RED: i32 = 0;
pub(crate) const TM_BLACK: i32 = 1;
// iterator slots
pub(crate) const TMI_MAP: usize = 0;
pub(crate) const TMI_NEXT: usize = 1;
pub(crate) const TMI_CURRENT: usize = 2;
pub(crate) const TMI_EXPECTED: usize = 3;
// the NavigableMap descent modes
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum TmNav {
    Ceiling,
    Higher,
    Floor,
    Lower,
}
pub(crate) fn tm_root(map: &JRef) -> Option<JRef> {
    with_obj(*map, |o| match &o.slots[TM_ROOT] {
        Value::Ref(Some(r)) => Some(*r),
        _ => None,
    })
}
pub(crate) fn tm_entry_key(e: &JRef) -> Value {
    with_obj(*e, |o| o.slots[TME_KEY].clone())
}
fn tm_entry_value(e: &JRef) -> Value {
    with_obj(*e, |o| o.slots[TME_VALUE].clone())
}
pub(crate) fn tm_child(e: &JRef, slot: usize) -> Option<JRef> {
    with_obj(*e, |o| match &o.slots[slot] {
        Value::Ref(Some(c)) => Some(*c),
        _ => None,
    })
}
fn tm_parent(e: &JRef) -> Option<JRef> {
    tm_child(e, TME_PARENT)
}
pub(crate) fn tm_color(e: Option<&JRef>) -> i32 {
    match e {
        None => TM_BLACK, // the nil leaves are black
        Some(r) => with_obj(*r, |o| o.slots[TME_COLOR].expect_int()),
    }
}
fn tm_set_child(e: &JRef, slot: usize, c: Option<JRef>) {
    with_obj_mut(*e, |o| {
        o.slots[slot] = match c {
            Some(x) => Value::Ref(Some(x)),
            None => Value::Ref(None),
        }
    });
}
fn tm_set_parent(e: &JRef, p: Option<JRef>) {
    tm_set_child(e, TME_PARENT, p);
}
fn tm_set_color(e: &JRef, c: i32) {
    with_obj_mut(*e, |o| o.slots[TME_COLOR] = Value::Int(c));
}
pub(crate) fn tm_size_of(map: &JRef) -> i32 {
    with_obj(*map, |o| o.slots[TM_SIZE].expect_int())
}
pub(crate) fn tm_modcount(map: &JRef) -> i32 {
    with_obj(*map, |o| o.slots[TM_MODCOUNT].expect_int())
}
/// The map's ordering: the Comparator walk when the map carries one,
/// otherwise the natural Comparable walk (with the probed NPE / CCE
/// laws on the way in).
pub(crate) fn tm_compare(map: &JRef, a: &Value, b: &Value, ctx: Dispatch) -> Result<i32, Throw> {
    let comp = with_obj(*map, |o| match &o.slots[TM_COMPARATOR] {
        Value::Ref(Some(c)) => Some(*c),
        _ => None,
    });
    if let Some(c) = comp {
        let out = crate::invoke_virtual(
            &c,
            "java/util/Comparator",
            "compare",
            "(Ljava/lang/Object;Ljava/lang/Object;)I",
            &[a.clone(), b.clone()],
            ctx,
        )?;
        return Ok(out.expect_int());
    }
    let ra = match a {
        Value::Ref(Some(r)) => *r,
        Value::Ref(None) => return Err(crate::npe()),
        other => {
            return Err(internal_error(&format!(
                "j2r internal: TreeMap natural key {other:?}"
            )))
        }
    };
    // the CCE law (probed): a non-Comparable key never enters the tree
    let cls = with_obj(ra, |o| o.jclass.to_string());
    if !ctx.is_subtype(&cls, "java/lang/Comparable") {
        return Err(throw_new(
            "java/lang/ClassCastException",
            Some(&format!(
                "class {cls} cannot be cast to class java.lang.Comparable"
            )),
        ));
    }
    let out = crate::invoke_virtual(
        &ra,
        "java/lang/Comparable",
        "compareTo",
        "(Ljava/lang/Object;)I",
        std::slice::from_ref(b),
        ctx,
    )?;
    Ok(out.expect_int())
}
pub(crate) fn tm_leftmost(mut n: JRef) -> JRef {
    while let Some(l) = tm_child(&n, TME_LEFT) {
        n = l;
    }
    n
}
pub(crate) fn tm_rightmost(mut n: JRef) -> JRef {
    while let Some(r) = tm_child(&n, TME_RIGHT) {
        n = r;
    }
    n
}
/// The in-order successor (OpenJDK `successor(t)`): the leftmost of the
/// right subtree, else the first ancestor whose left chain contains t.
pub(crate) fn tm_successor_of(n: JRef) -> Option<JRef> {
    if let Some(r) = tm_child(&n, TME_RIGHT) {
        return Some(tm_leftmost(r));
    }
    let mut cur = n;
    loop {
        let p = tm_parent(&cur)?;
        if tm_child(&p, TME_LEFT) == Some(cur) {
            return Some(p);
        }
        cur = p;
    }
}
/// The in-order predecessor (OpenJDK `predecessor(t)`) — the firstEntry
/// walk of a descending iteration (not probed this slice; kept for the
/// navigable mirror).
#[allow(dead_code)]
fn tm_predecessor_of(n: JRef) -> Option<JRef> {
    if let Some(l) = tm_child(&n, TME_LEFT) {
        return Some(tm_rightmost(l));
    }
    let mut cur = n;
    loop {
        let p = tm_parent(&cur)?;
        if tm_child(&p, TME_RIGHT) == Some(cur) {
            return Some(p);
        }
        cur = p;
    }
}
/// Every (key, value) pair in KEY order — the toString / ctor-from-map /
/// containsValue walk (T1).
pub(crate) fn tm_all_entries(map: &JRef) -> Result<Vec<(Value, Value)>, Throw> {
    let mut out = Vec::new();
    if let Some(root) = tm_root(map) {
        let mut cur = Some(tm_leftmost(root));
        while let Some(n) = cur {
            out.push((tm_entry_key(&n), tm_entry_value_materialized(&n)?));
            cur = tm_successor_of(n);
        }
    }
    Ok(out)
}

// --- the CLRS rotations (parent pointers updated; the root slot fixed) ---
fn tm_rotate_left(map: &JRef, x: JRef) {
    let y = tm_child(&x, TME_RIGHT).expect("j2r internal: rotate_left without right child");
    let yl = tm_child(&y, TME_LEFT);
    tm_set_child(&x, TME_RIGHT, yl);
    if let Some(yl) = yl {
        tm_set_parent(&yl, Some(x));
    }
    let xp = tm_parent(&x);
    tm_set_parent(&y, xp);
    match xp {
        None => tm_set_root(map, Some(y)),
        Some(p) => {
            if tm_child(&p, TME_LEFT) == Some(x) {
                tm_set_child(&p, TME_LEFT, Some(y));
            } else {
                tm_set_child(&p, TME_RIGHT, Some(y));
            }
        }
    }
    tm_set_child(&y, TME_LEFT, Some(x));
    tm_set_parent(&x, Some(y));
}
fn tm_rotate_right(map: &JRef, x: JRef) {
    let y = tm_child(&x, TME_LEFT).expect("j2r internal: rotate_right without left child");
    let yr = tm_child(&y, TME_RIGHT);
    tm_set_child(&x, TME_LEFT, yr);
    if let Some(yr) = yr {
        tm_set_parent(&yr, Some(x));
    }
    let xp = tm_parent(&x);
    tm_set_parent(&y, xp);
    match xp {
        None => tm_set_root(map, Some(y)),
        Some(p) => {
            if tm_child(&p, TME_RIGHT) == Some(x) {
                tm_set_child(&p, TME_RIGHT, Some(y));
            } else {
                tm_set_child(&p, TME_LEFT, Some(y));
            }
        }
    }
    tm_set_child(&y, TME_RIGHT, Some(x));
    tm_set_parent(&x, Some(y));
}
fn tm_set_root(map: &JRef, r: Option<JRef>) {
    with_obj_mut(*map, |o| {
        o.slots[TM_ROOT] = match r {
            Some(x) => Value::Ref(Some(x)),
            None => Value::Ref(None),
        }
    });
}
/// The insert fixup (CLRS §13.3): redden the new node, repair the
/// double-red, recolor the root black.
fn tm_insert_fixup(map: &JRef, mut z: JRef) {
    loop {
        let p = match tm_parent(&z) {
            Some(p) if tm_color(Some(&p)) == TM_RED => p,
            _ => break,
        };
        let gp = match tm_parent(&p) {
            Some(g) => g,
            None => break, // the root is black; unreachable in a valid tree
        };
        let p_is_left = tm_child(&gp, TME_LEFT) == Some(p);
        let uncle = tm_child(&gp, if p_is_left { TME_RIGHT } else { TME_LEFT });
        if tm_color(uncle.as_ref()) == TM_RED {
            tm_set_color(&p, TM_BLACK);
            if let Some(u) = uncle {
                tm_set_color(&u, TM_BLACK);
            }
            tm_set_color(&gp, TM_RED);
            z = gp;
            continue;
        }
        if p_is_left {
            // zig-zag first: after the rotation z is the TOP of the former
            // double-red pair (the CLRS rename z = z.p BEFORE the rotation
            // lands the old parent below it), so `top` = z; a straight
            // line keeps the parent as the pivot
            let top = if tm_child(&p, TME_RIGHT) == Some(z) {
                tm_rotate_left(map, p);
                z
            } else {
                p
            };
            tm_set_color(&top, TM_BLACK);
            let gp2 = tm_parent(&top).unwrap_or_else(|| panic!("j2r internal: fixup gp"));
            tm_set_color(&gp2, TM_RED);
            tm_rotate_right(map, gp2);
        } else {
            let top = if tm_child(&p, TME_LEFT) == Some(z) {
                tm_rotate_right(map, p);
                z
            } else {
                p
            };
            tm_set_color(&top, TM_BLACK);
            let gp2 = tm_parent(&top).unwrap_or_else(|| panic!("j2r internal: fixup gp"));
            tm_set_color(&gp2, TM_RED);
            tm_rotate_left(map, gp2);
        }
    }
    if let Some(root) = tm_root(map) {
        tm_set_color(&root, TM_BLACK);
    }
}
/// Replaces the subtree rooted at u with the subtree rooted at v
/// (CLRS §13.4 transplant).
fn tm_transplant(map: &JRef, u: JRef, v: Option<JRef>) {
    let up = tm_parent(&u);
    match up {
        None => tm_set_root(map, v),
        Some(p) => {
            if tm_child(&p, TME_LEFT) == Some(u) {
                tm_set_child(&p, TME_LEFT, v);
            } else {
                tm_set_child(&p, TME_RIGHT, v);
            }
        }
    }
    if let Some(v) = v {
        tm_set_parent(&v, up);
    }
}
/// The delete fixup (CLRS §13.4 / OpenJDK fixupAfterDeletion): x may be
/// the node just logically removed (the nil-sentinel trick — its pointers
/// still reach the tree until the transplant).
fn tm_delete_fixup(map: &JRef, mut x: JRef, mut xp: JRef) {
    // CLRS: `while x != T.root && x.color == BLACK` — a RED x means the
    // deficit is absorbed by recoloring (the exit recolor below), not by
    // more rotations; the root is re-read LIVE (a case-1 rotation can
    // replace it mid-fixup)
    while Some(x) != tm_root(map) && tm_color(Some(&x)) == TM_BLACK {
        let x_is_left = tm_child(&xp, TME_LEFT) == Some(x);
        let far = if x_is_left { TME_RIGHT } else { TME_LEFT };
        let near = if x_is_left { TME_LEFT } else { TME_RIGHT };
        let w = match tm_child(&xp, far) {
            Some(w) => w,
            None => break, // a valid red-black tree never gets here
        };
        if tm_color(Some(&w)) == TM_RED {
            tm_set_color(&w, TM_BLACK);
            tm_set_color(&xp, TM_RED);
            if x_is_left {
                tm_rotate_left(map, xp);
            } else {
                tm_rotate_right(map, xp);
            }
            // xp moved down; the new sibling
            xp = match tm_parent(&x) {
                Some(p) => p,
                None => break,
            };
            continue;
        }
        let wl = tm_color(tm_child(&w, TME_LEFT).as_ref());
        let wr = tm_color(tm_child(&w, TME_RIGHT).as_ref());
        if wl == TM_BLACK && wr == TM_BLACK {
            tm_set_color(&w, TM_RED);
            let x2 = xp;
            match tm_parent(&x2) {
                Some(p) => {
                    x = x2;
                    xp = p;
                }
                None => break, // x2 is the root; the final recolor fixes it
            }
            continue;
        }
        let far_black = tm_color(tm_child(&w, far).as_ref()) == TM_BLACK;
        if far_black {
            // the near nephew is RED: rotate the sibling into a straight line
            if let Some(nn) = tm_child(&w, near) {
                tm_set_color(&nn, TM_BLACK);
            }
            tm_set_color(&w, TM_RED);
            if x_is_left {
                tm_rotate_right(map, w);
            } else {
                tm_rotate_left(map, w);
            }
        }
        let w2 = tm_child(&xp, far).expect("j2r internal: delete fixup sibling");
        tm_set_color(&w2, tm_color(Some(&xp)));
        tm_set_color(&xp, TM_BLACK);
        if let Some(f) = tm_child(&w2, far) {
            tm_set_color(&f, TM_BLACK);
        }
        if x_is_left {
            tm_rotate_left(map, xp);
        } else {
            tm_rotate_right(map, xp);
        }
        break;
    }
    // THE CLRS EXIT LAW (the red-red-at-16.right corruption): the node the
    // loop ended on recolors BLACK UNCONDITIONALLY — it absorbed the
    // double-black (or was red from case 1's parent recolor and must go
    // black). The root-only recolor left case-1+case-2 climbs red.
    tm_set_color(&x, TM_BLACK);
}
/// Removes one node from the tree (OpenJDK `deleteEntry`): the two-child
/// case splices the successor's payload into the target and deletes the
/// successor (at most one child); the black-loss case runs the fixup.
pub(crate) fn tm_delete(map: &JRef, z: JRef) {
    let mc = tm_modcount(map);
    let sz = tm_size_of(map);
    with_obj_mut(*map, |o| {
        o.slots[TM_MODCOUNT] = Value::Int(mc + 1);
        o.slots[TM_SIZE] = Value::Int(sz - 1);
    });
    let mut target = z;
    if tm_child(&z, TME_LEFT).is_some() && tm_child(&z, TME_RIGHT).is_some() {
        let s = tm_successor_of(z).expect("j2r internal: successor of a two-child node");
        let (sk, sv) = (tm_entry_key(&s), tm_entry_value(&s));
        // G3.3 slice 4: the class tag RIDES the value in the
        // transplant-by-copy — the successor node dies here, so its raw
        // provenance must move with the payload (a cleared tag on a raw
        // slot would be a raw slot no reader can name).
        let stag = with_obj(s, |o| o.slots[TME_TAG].clone());
        with_obj_mut(z, |o| {
            o.slots[TME_KEY] = sk;
            o.slots[TME_VALUE] = sv;
            o.slots[TME_TAG] = stag;
        });
        target = s;
    }
    let rep = match tm_child(&target, TME_LEFT) {
        Some(l) => Some(l),
        None => tm_child(&target, TME_RIGHT),
    };
    let tp = tm_parent(&target);
    match rep {
        Some(r) => {
            tm_transplant(map, target, Some(r));
            if tm_color(Some(&target)) == TM_BLACK {
                match tp {
                    Some(p) => tm_delete_fixup(map, r, p),
                    // the deleted node was the ROOT (the slice-clear path
                    // exposed this run-5 hole): the replacement IS the new
                    // root, so CLRS's `while x != root` exits immediately
                    // and the exit recolor is the whole law
                    None => tm_set_color(&r, TM_BLACK),
                }
            }
        }
        None => match tp {
            None => tm_set_root(map, None),
            Some(p) => {
                if tm_color(Some(&target)) == TM_BLACK {
                    tm_delete_fixup(map, target, p);
                }
                tm_transplant(map, target, None);
                tm_set_parent(&target, None);
            }
        },
    }
}
/// The descent machinery shared by put/find/navigate.
pub(crate) fn tm_find(map: &JRef, key: &Value, ctx: Dispatch) -> Result<Option<JRef>, Throw> {
    let mut cur = tm_root(map);
    while let Some(c) = cur {
        let cmp = tm_compare(map, key, &tm_entry_key(&c), ctx)?;
        cur = if cmp < 0 {
            tm_child(&c, TME_LEFT)
        } else if cmp > 0 {
            tm_child(&c, TME_RIGHT)
        } else {
            return Ok(Some(c));
        };
    }
    Ok(None)
}
/// The NavigableMap descent (T5): Ceiling = least >= key, Higher = least
/// > key, Floor = greatest <= key, Lower = greatest < key.
pub(crate) fn tm_navigate(
    map: &JRef,
    key: &Value,
    mode: TmNav,
    ctx: Dispatch,
) -> Result<Option<JRef>, Throw> {
    let mut best: Option<JRef> = None;
    let mut cur = tm_root(map);
    while let Some(c) = cur {
        let cmp = tm_compare(map, key, &tm_entry_key(&c), ctx)?;
        if cmp == 0 && matches!(mode, TmNav::Ceiling | TmNav::Floor) {
            return Ok(Some(c));
        }
        // candidates: Ceiling/Higher accept the node when key < node.key;
        // Floor/Lower when key > node.key
        let candidate = match mode {
            TmNav::Ceiling | TmNav::Higher => cmp < 0,
            TmNav::Floor | TmNav::Lower => cmp > 0,
        };
        let go = if matches!(mode, TmNav::Ceiling | TmNav::Higher) {
            // keep searching the high side unless the node is a keeper
            if cmp == 0 {
                TME_RIGHT
            } else if candidate {
                TME_LEFT
            } else {
                TME_RIGHT
            }
        } else if cmp == 0 {
            TME_LEFT
        } else if candidate {
            TME_RIGHT
        } else {
            TME_LEFT
        };
        if candidate {
            best = Some(c);
        }
        cur = tm_child(&c, go);
    }
    Ok(best)
}
/// put — the type/null check FIRST (T7/T8: it runs even on an empty
/// map), then the descent; an equal key replaces (T3), a fresh key
/// inserts RED and rebalances.
pub fn n_tm_put(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.put without receiver");
    let key = arg_or_null(args);
    let value = args.get(1).cloned().unwrap_or(Value::Ref(None));
    tm_put_impl(r, key, value, ctx, None).map(|old| old.unwrap_or(Value::Ref(None)))
}
/// G3.3 slice 4: the specialized-map put on the red-black storage. The
/// walk, the self-compare (the addEntryToEmptyMap law — a non-Comparable
/// key throws CCE on the FIRST put), the CLRS insert and the modCount/
/// size are n_tm_put's own machinery — only the box allocation and the
/// valueOf dispatch disappear. The put's return (the OLD value, per the
/// Map contract) is discarded: the rewritten core requires the `Pop`, so
/// no caller can observe it.
pub fn tm_put_prim(map: &JRef, key: Value, value: Value, ctx: Dispatch) -> Result<(), Throw> {
    // The tag rides the stored value's own prim kind — the node becomes
    // self-describing, so a faithful reader materializes the exact box
    // class the compiler's closure law hid.
    let tag = match &value {
        Value::Double(_) => Some('D'),
        Value::Float(_) => Some('F'),
        Value::Int(_) => Some('I'),
        Value::Long(_) => Some('J'),
        _ => None,
    };
    tm_put_impl(map, key, value, ctx, tag).map(|old| old.unwrap_or(Value::Ref(None)))?;
    Ok(())
}
/// The put machinery shared by n_tm_put and tm_put_prim. `tag` is None
/// for the faithful path (byte-identical behavior) and Some(desc char)
/// for the specialized path (the node stores the RAW prim + its tag).
/// The overwrite return is materialized BEFORE the slot write (the
/// hm_put pattern — the Map.put contract; the allocation happens
/// outside any with_obj scope).
fn tm_put_impl(
    r: &JRef,
    key: Value,
    value: Value,
    ctx: Dispatch,
    tag: Option<char>,
) -> Result<Option<Value>, Throw> {
    let _ = tm_compare(r, &key, &key, ctx)?;
    let mut parent: Option<JRef> = None;
    let mut last_cmp: i32 = 0;
    let mut cur = tm_root(r);
    while let Some(c) = cur {
        let cmp = tm_compare(r, &key, &tm_entry_key(&c), ctx)?;
        if cmp == 0 {
            let old = node_value_materialized_at(&c, TME_VALUE, TME_TAG)?;
            with_obj_mut(c, |o| {
                o.slots[TME_VALUE] = value.clone();
                o.slots[TME_TAG] = tag_slot(tag);
            });
            return Ok(Some(old));
        }
        parent = Some(c);
        last_cmp = cmp;
        cur = tm_child(&c, if cmp < 0 { TME_LEFT } else { TME_RIGHT });
    }
    let node = new_obj("java/util/TreeMap$Entry", 7)?;
    with_obj_mut(node, |o| {
        o.slots[TME_KEY] = key;
        o.slots[TME_VALUE] = value;
        o.slots[TME_COLOR] = Value::Int(TM_RED);
        o.slots[TME_TAG] = tag_slot(tag);
    });
    match parent {
        Some(p) => {
            tm_set_parent(&node, Some(p));
            tm_set_child(
                &p,
                if last_cmp < 0 { TME_LEFT } else { TME_RIGHT },
                Some(node),
            );
        }
        None => tm_set_root(r, Some(node)),
    }
    tm_insert_fixup(r, node);
    let (mc, sz) = (tm_modcount(r), tm_size_of(r));
    with_obj_mut(*r, |o| {
        o.slots[TM_MODCOUNT] = Value::Int(mc + 1);
        o.slots[TM_SIZE] = Value::Int(sz + 1);
    });
    Ok(None)
}
pub fn n_tm_get(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.get without receiver");
    let key = arg_or_null(args);
    match tm_find(r, &key, ctx)? {
        Some(n) => tm_entry_value_materialized(&n),
        None => Ok(Value::Ref(None)),
    }
}
pub fn n_tm_contains_key(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.containsKey without receiver");
    let key = arg_or_null(args);
    let found = tm_find(r, &key, ctx)?.is_some();
    Ok(Value::Int(found as i32))
}
pub fn n_tm_contains_value(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.containsValue without receiver");
    let target = arg_or_null(args);
    for (_, v) in tm_all_entries(r)? {
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
pub fn n_tm_get_or_default(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.getOrDefault without receiver");
    let key = arg_or_null(args);
    let dflt = args.get(1).cloned().unwrap_or(Value::Ref(None));
    match tm_find(r, &key, ctx)? {
        Some(n) => tm_entry_value_materialized(&n),
        None => Ok(dflt),
    }
}
pub fn n_tm_remove(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.remove without receiver");
    let key = arg_or_null(args);
    match tm_find(r, &key, ctx)? {
        Some(n) => {
            let old = node_value_materialized_at(&n, TME_VALUE, TME_TAG)?;
            tm_delete(r, n);
            Ok(old)
        }
        None => Ok(Value::Ref(None)),
    }
}
pub fn n_tm_size(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.size without receiver");
    Ok(Value::Int(tm_size_of(r)))
}
pub fn n_tm_is_empty(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.isEmpty without receiver");
    Ok(Value::Int((tm_size_of(r) == 0) as i32))
}
pub fn n_tm_clear(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.clear without receiver");
    let mc = tm_modcount(r);
    with_obj_mut(*r, |o| {
        o.slots[TM_SIZE] = Value::Int(0);
        o.slots[TM_MODCOUNT] = Value::Int(mc + 1);
        o.slots[TM_ROOT] = Value::Ref(None);
    });
    Ok(Value::Void)
}
pub fn n_tm_to_string(recv: Option<&JRef>, _args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.toString without receiver");
    let entries = tm_all_entries(r)?;
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
pub fn n_tm_first_key(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.firstKey without receiver");
    match tm_root(r) {
        Some(root) => Ok(tm_entry_key(&tm_leftmost(root))),
        None => Err(throw_new("java/util/NoSuchElementException", None)),
    }
}
pub fn n_tm_last_key(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.lastKey without receiver");
    match tm_root(r) {
        Some(root) => Ok(tm_entry_key(&tm_rightmost(root))),
        None => Err(throw_new("java/util/NoSuchElementException", None)),
    }
}
/// The NavigableMap key queries (T5): a null result is a null KEY.
fn tm_nav_key(
    recv: Option<&JRef>,
    args: &[Value],
    mode: TmNav,
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.navigable query without receiver");
    let key = arg_or_null(args);
    match tm_navigate(r, &key, mode, ctx)? {
        Some(n) => Ok(tm_entry_key(&n)),
        None => Ok(Value::Ref(None)),
    }
}
pub fn n_tm_ceiling_key(
    recv: Option<&JRef>,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    tm_nav_key(recv, args, TmNav::Ceiling, ctx)
}
pub fn n_tm_floor_key(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    tm_nav_key(recv, args, TmNav::Floor, ctx)
}
pub fn n_tm_higher_key(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    tm_nav_key(recv, args, TmNav::Higher, ctx)
}
pub fn n_tm_lower_key(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    tm_nav_key(recv, args, TmNav::Lower, ctx)
}
// <init>()V / <init>(Ljava/util/Comparator;)V / <init>(Ljava/util/Map;)V —
// three ROWS (a null Comparator and a null Map both arrive as Ref(None);
// the row disambiguates). The SortedMap ctor and the (C,K) navigable ctor
// are NOT registered (the same-surface law; the bounded views are their
// own slice).
pub fn n_tm_init(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.<init> without receiver");
    with_obj_mut(*r, |o| {
        o.slots[TM_COMPARATOR] = Value::Ref(None);
        o.slots[TM_SIZE] = Value::Int(0);
        o.slots[TM_MODCOUNT] = Value::Int(0);
        o.slots[TM_ROOT] = Value::Ref(None);
    });
    Ok(Value::Void)
}
/// the Comparator ctor: the ref rides (a null comparator means natural)
pub fn n_tm_init_cmp(recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.<init> without receiver");
    n_tm_init(Some(r), &[], _ctx)?;
    if let Some(Value::Ref(Some(c))) = args.first() {
        with_obj_mut(*r, |o| o.slots[TM_COMPARATOR] = Value::Ref(Some(*c)));
    }
    // a null comparator = natural ordering (the JDK law)
    Ok(Value::Void)
}
/// the copy ctor (T9): sorted regardless of the source's iteration order
pub fn n_tm_init_map(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.<init> without receiver");
    n_tm_init(Some(r), &[], ctx)?;
    let src = first_arg_ref!(args, Ok(Value::Void));
    let src_class = with_obj(src, |o| o.jclass.to_string());
    let entries = match src_class.as_str() {
        "java/util/LinkedHashMap" => lhm_all_entries(&src)?,
        _ => hm_all_entries(&src)?,
    };
    for (k, v) in entries {
        n_tm_put(Some(r), &[k, v], ctx)?;
    }
    Ok(Value::Void)
}

// --- the views (map-ref objects) and the successor-walk iterators ---
fn tm_new_view(map: &JRef, class: &str) -> Result<Value, Throw> {
    let v = new_obj(class, 1)?;
    with_obj_mut(v, |o| o.slots[0] = Value::Ref(Some(*map)));
    Ok(Value::Ref(Some(v)))
}
pub(crate) fn tm_view_map(recv: Option<&JRef>, who: &str) -> JRef {
    let r = recv.unwrap_or_else(|| panic!("j2r internal: TreeMap view {who} without receiver"));
    with_obj(*r, |o| match &o.slots[0] {
        Value::Ref(Some(m)) => *m,
        _ => panic!("j2r internal: TreeMap view {who} without map"),
    })
}
pub fn n_tm_key_set(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.keySet without receiver");
    tm_new_view(r, "java/util/TreeMap$KeySet")
}
pub fn n_tm_values(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.values without receiver");
    tm_new_view(r, "java/util/TreeMap$Values")
}
pub fn n_tm_entry_set(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = recv.expect("j2r internal: TreeMap.entrySet without receiver");
    tm_new_view(r, "java/util/TreeMap$EntrySet")
}
/// Builds a positioned successor-walk iterator: next = leftmost(root),
/// current = None, expectedModCount snapshotted.
pub(crate) fn tm_new_iterator(map: &JRef, class: &str) -> Result<Value, Throw> {
    let next = tm_root(map).map(tm_leftmost);
    // pre-read under its own lock (the heap mutex is NOT re-entrant — a
    // with_obj inside a with_obj_mut closure deadlocks)
    let mc = tm_modcount(map);
    let it = new_obj(class, 4)?;
    with_obj_mut(it, |o| {
        o.slots[TMI_MAP] = Value::Ref(Some(*map));
        o.slots[TMI_NEXT] = match next {
            Some(n) => Value::Ref(Some(n)),
            None => Value::Ref(None),
        };
        o.slots[TMI_CURRENT] = Value::Ref(None);
        o.slots[TMI_EXPECTED] = Value::Int(mc);
    });
    Ok(Value::Ref(Some(it)))
}
/// Shared hasNext: no comodification check (the JDK law).
pub fn n_tm_i_has_next(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: TreeMap iterator hasNext without receiver");
    let has = with_obj(*it, |o| matches!(&o.slots[TMI_NEXT], Value::Ref(Some(_))));
    Ok(Value::Int(has as i32))
}
/// Shared nextNode: the CME check first, then NoSuchElementException,
/// then current = next, next = successor(next).
pub(crate) fn tm_iter_next_node(recv: Option<&JRef>, who: &str) -> Result<(JRef, JRef), Throw> {
    let it = recv.unwrap_or_else(|| panic!("j2r internal: {who} without receiver"));
    let (map, expected, next) = with_obj(*it, |o| {
        (
            match &o.slots[TMI_MAP] {
                Value::Ref(Some(m)) => *m,
                _ => panic!("j2r internal: {who} without map"),
            },
            o.slots[TMI_EXPECTED].expect_int(),
            match &o.slots[TMI_NEXT] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    if expected != tm_modcount(&map) {
        return Err(crate::cme());
    }
    let Some(node) = next else {
        return Err(throw_new("java/util/NoSuchElementException", None));
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
/// Iterator.remove over the tree (T10 + V8): the PROBED ISE-first law
/// (the run-4 ItrOrderProbe law — ISE on null current BEFORE the CME
/// check), then the map's own delete, then the re-sync (no CME on a
/// continued iteration). View-aware (run 6): the bounded iterators share
/// this SAME native — their slot 0 carries the TreeMap$SubMap view, whose
/// slot 0 carries the backing map; the ISE-first law carries by
/// construction.
pub fn n_tm_i_remove(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let it = recv.expect("j2r internal: TreeMap iterator remove without receiver");
    let (map_slot, expected, current) = with_obj(*it, |o| {
        (
            o.slots[TMI_MAP].clone(),
            o.slots[TMI_EXPECTED].expect_int(),
            match &o.slots[TMI_CURRENT] {
                Value::Ref(Some(n)) => Some(*n),
                _ => None,
            },
        )
    });
    let map = match &map_slot {
        Value::Ref(Some(m)) => {
            let cls = with_obj(*m, |o| o.jclass.to_string());
            if cls == "java/util/TreeMap$SubMap" {
                sms_backing(m)
            } else {
                *m
            }
        }
        _ => panic!("j2r internal: TreeMap iterator without map"),
    };
    let Some(node) = current else {
        return Err(throw_new("java/lang/IllegalStateException", None));
    };
    if expected != tm_modcount(&map) {
        return Err(crate::cme());
    }
    tm_delete(&map, node);
    let mc = tm_modcount(&map);
    with_obj_mut(*it, |o| {
        o.slots[TMI_CURRENT] = Value::Ref(None);
        o.slots[TMI_EXPECTED] = Value::Int(mc);
    });
    Ok(Value::Void)
}
/// TreeMap$Entry.getKey / getValue (the entrySet checkcast + accessor law).
pub fn n_tme_get_key(recv: Option<&JRef>, _args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let e = recv.expect("j2r internal: TreeMap$Entry.getKey without receiver");
    Ok(tm_entry_key(e))
}
pub fn n_tme_get_value(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let e = recv.expect("j2r internal: TreeMap$Entry.getValue without receiver");
    tm_entry_value_materialized(e)
}

// --------------------------------------------------------- TreeMap$SubMap
//
// Run 6 (2026-11-15): the BOUNDED views — subMap/headMap/tailMap as a
// 5-slot view over the backing TreeMap (0 = backing map, 1 = lo key,
// 2 = hi key, 3 = loInclusive Int, 4 = hiInclusive Int; a null bound is
// the unbounded side, exactly the absolute map's fromStart/toEnd flags).
//
// Laws (probed vs Temurin 21.0.13+9, TreeMapViewsProbe, evidence
// scripts/evidence/treemap_views/treemapviewsprobe_temurin_2026-11-15.txt):
//   V1  contents/toString/size/isEmpty are computed over the RANGE, not
//       mirrored (V5 35-key clear: the parent keeps the out-of-range keys)
//   V2  from-inclusive, to-EXCLUSIVE on the 2-arg forms; the 4-arg forms
//       carry the flags; a single-key inclusive view is puttable (c29)
//   V3  first/lastKey over the range; empty view -> NoSuchElementException
//   V4  the range-gated Map surface: get/containsKey/getOrDefault/remove
//       SILENTLY miss out-of-range keys (c08/c09/c11/c34 — even on an
//       empty view), put throws IAE "key out of range" (c12); null keys
//       NPE through the range compare (c18) and non-Comparable keys CCE
//       (c19) — the type check rides the range check
//   V5  live both ways (c03/c13): put/remove through the view hits the
//       backing tree; parent puts in-range appear in the view
//   V6  slice-only clear (c15/c35): the slice empties, out-of-range keys
//       survive
//   V7  THE NARROWING MATRIX (c16, 26 cells) — chained builders validate
//       STRICTLY, never intersecting/clamping, with the probed ASYMMETRY:
//       the fromKey side uses the FULL inRange (tooLow with inclusivity
//       AND tooHigh with inclusivity — fromKey == an exclusive hi throws
//       "fromKey out of range", c16f/s/t), while the toKey side is
//       INCLUSIVITY-BLIND on the hi (strictly-above-hi only: headMap
//       ACCEPTS toKey == an exclusive hi, c16e, and on a hi-INCLUSIVE
//       parent too, c16y) but full tooLow on the lo (c16u "toKey out of
//       range"). Check order: fromKey range, then toKey range, then the
//       crossed check "fromKey > toKey" (c16w/x/q). The ABSOLUTE map's
//       builders never range-check — only the ctor's crossed check (c16l)
//       plus the self-compare type check (null -> NPE, non-Comparable ->
//       CCE). Chained narrowing of a DISJOINT window throws (c16o/p),
//       never an empty view; from == to is a legal empty view (c16m).
//   V8  the bounded iterators share the unbounded 4-slot layout with
//       slot 0 = the VIEW (the backing map rides the view's slot 0);
//       expectedModCount is the BACKING map's, so a foreign put
//       OUT-of-range still CMEs (c27); remove is LITERALLY the same
//       native as the unbounded iterators' (n_tm_i_remove derefs the
//       view) — the ISE-first law carries by construction (c25/c28)
//   V9  keySet/values/entrySet are live view-of-view objects; the bounded
//       iterators walk the range in key order and stop at the hi fence
pub fn n_tm_sub_map(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = sms_view(recv, "subMap");
    let f = arg_or_null(args);
    let t = args.get(1).cloned().unwrap_or(Value::Ref(None));
    if tm_compare(&r, &f, &t, ctx)? > 0 {
        return Err(sms_iae("fromKey > toKey"));
    }
    sms_new(&r, f, true, t, false)
}
pub fn n_tm_sub_map4(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = sms_view(recv, "subMap4");
    let f = arg_or_null(args);
    let t = args.get(2).cloned().unwrap_or(Value::Ref(None));
    if tm_compare(&r, &f, &t, ctx)? > 0 {
        return Err(sms_iae("fromKey > toKey"));
    }
    sms_new(&r, f, arg_bool(args, 1), t, arg_bool(args, 3))
}
pub fn n_tm_head_map(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = sms_view(recv, "headMap");
    let t = arg_or_null(args);
    let _ = tm_compare(&r, &t, &t, ctx)?;
    sms_new(&r, Value::Ref(None), false, t, false)
}
pub fn n_tm_head_map4(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = sms_view(recv, "headMap4");
    let t = arg_or_null(args);
    let _ = tm_compare(&r, &t, &t, ctx)?;
    sms_new(&r, Value::Ref(None), false, t, arg_bool(args, 1))
}
pub fn n_tm_tail_map(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = sms_view(recv, "tailMap");
    let f = arg_or_null(args);
    let _ = tm_compare(&r, &f, &f, ctx)?;
    sms_new(&r, f, true, Value::Ref(None), false)
}
pub fn n_tm_tail_map4(recv: Option<&JRef>, args: &[Value], ctx: Dispatch) -> Result<Value, Throw> {
    let r = sms_view(recv, "tailMap4");
    let f = arg_or_null(args);
    let _ = tm_compare(&r, &f, &f, ctx)?;
    sms_new(&r, f, arg_bool(args, 1), Value::Ref(None), false)
}
pub fn n_tm_comparator(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let r = sms_view(recv, "comparator");
    Ok(with_obj(r, |o| o.slots[TM_COMPARATOR].clone()))
}
