use super::*;

/// Dynamic dispatch of an `invokevirtual`/`invokeinterface`: walks the
/// receiver's class chain from the concrete class upwards and returns the
/// first compiled or native implementation found.
pub fn invoke_virtual(
    recv: &JRef,
    _declared: &str,
    name: &str,
    desc: &str,
    args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    // The receiver's class chain is walked by name; the receiver itself is
    // rooted by the caller's refreshed frame snapshot (generated code
    // refreshes before every call), so no NativeRoots scope is needed here.
    if let Some(res) = dispatch_chain(recv, name, desc, args, ctx) {
        return res;
    }
    // The interface-default resolution (the registered Probe5 residual,
    // standing run 3, 2026-11-16): the chain above walks parents[0] only,
    // so a user interface's DEFAULT method is off it — Temurin runs the
    // default body, j2r died the loud internal Error. JVMS 5.4.4.4/5.4.6:
    // when no concrete class implementation exists, the MAXIMALLY-SPECIFIC
    // default among the receiver's transitive interface set runs. Law
    // details pinned by the probe (scripts/evidence/default_dispatch/):
    // - the closure is BFS over the FULL parents arrays (class parents =
    //   [super, interfaces...]; interface parents = [Object, super-ifaces],
    //   superclass interfaces included by expansion);
    // - a candidate is a lookup hit on an INTERFACE-typed node: abstract
    //   interface methods have no lookup row (codegen skips them at plan
    //   time), so a user-lookup hit IS a concrete default body; builtin
    //   interfaces answer from the native rows;
    // - dominance: candidate I loses to another candidate J when J is a
    //   strict subtype of I (is_ref_subtype walks the same parents edges);
    // - the diamond conflict (two unrelated survivors) is UNREACHABLE from
    //   javac input — javac refuses it at compile time ("inherits
    //   unrelated defaults"), so it can only arrive in a hand-crafted
    //   classfile; the loud IncompatibleClassChangeError is the JVM's
    //   resolution law, never a silent pick;
    // - the same closed-world argument bounds the static-via-instance and
    //   re-abstract shapes: javac refuses both, concrete classes always
    //   implement re-abstracted members, so no extra filter is taken here
    //   and the loud Error remains the honest fallthrough.
    let survivors = default_survivors(recv, name, desc, ctx);
    if survivors.len() == 1 {
        let (iface, is_user) = &survivors[0];
        if *is_user {
            if let Some(f) = ctx.find_method(iface, name, desc) {
                return f(Some(recv), args);
            }
        } else if let Some(n) = natives::lookup_exact(iface, name, desc) {
            return n(Some(recv), args, ctx);
        }
    }
    if survivors.len() > 1 {
        let names: Vec<String> = survivors.iter().map(|(i, _)| i.to_string()).collect();
        return Err(throw_new(
            "java/lang/IncompatibleClassChangeError",
            Some(&format!(
                "unrelated default methods found for {name}{desc} in {}",
                names.join(", ")
            )),
        ));
    }
    let cls = crate::heap::with_obj(*recv, |o| o.jclass.clone());
    Err(throw_new(
        "java/lang/Error",
        Some(&format!(
            "j2r internal: no implementation found for {name}{desc} when dispatched on {cls}"
        )),
    ))
}
/// The concrete-class chain walk (verbatim from `invoke_virtual`,
/// round-6 reshaping): the array-receiver specials, then up to 64
/// parent hops over compiled fns and native rows. `Some(result)` when
/// a target dispatched, `None` to fall through to the default walk.
fn dispatch_chain(
    recv: &JRef,
    name: &str,
    desc: &str,
    args: &[Value],
    ctx: Dispatch,
) -> Option<Result<Value, Throw>> {
    let mut cur: Arc<str> = crate::heap::with_obj(*recv, |o| o.jclass.clone());
    // Array receivers (corpus wave 2, JZlib): every array type is a
    // subtype of Object with a public `clone()` (JVMS 2.15/6.5 invokevirtual)
    // plus the Object methods. `clone` shallow-copies element-wise here;
    // the Object methods dispatch through the Object natives rows.
    if cur.starts_with('[') {
        if name == "clone" && desc == "()Ljava/lang/Object;" {
            return Some(match crate::array::array_shallow_copy(recv, ctx) {
                Ok(v) => Ok(Value::Ref(Some(v))),
                Err(e) => return Some(Err(e)),
            });
        }
        if matches!(
            (name, desc),
            ("getClass", "()Ljava/lang/Class;")
                | ("hashCode", "()I")
                | ("equals", "(Ljava/lang/Object;)Z")
                | ("toString", "()Ljava/lang/String;")
        ) {
            return Some(natives::invoke_exact(
                "java/lang/Object",
                name,
                desc,
                Some(recv),
                args,
                ctx,
            ));
        }
    }
    for _ in 0..64 {
        if let Some(f) = ctx.find_method(&cur, name, desc) {
            return Some(f(Some(recv), args));
        }
        if let Some(n) = natives::lookup_exact(&cur, name, desc) {
            return Some(n(Some(recv), args, ctx));
        }
        match ctx.parent_of(&cur) {
            Some(p) => cur = Arc::from(p),
            None => break,
        }
    }
    None
}
/// The maximally-specific default-method scan (verbatim continuation):
/// BFS over the FULL parents arrays, interface-typed lookup hits as
/// candidates, dominance by strict subtype.
fn default_survivors(recv: &JRef, name: &str, desc: &str, ctx: Dispatch) -> Vec<(Arc<str>, bool)> {
    let start = crate::heap::with_obj(*recv, |o| o.jclass.clone());
    let mut queue: std::collections::VecDeque<Arc<str>> = std::collections::VecDeque::new();
    let mut seen: std::collections::HashSet<Arc<str>> = std::collections::HashSet::new();
    if let Some(ps) = ctx.parents_of_full(&start) {
        for p in ps {
            queue.push_back(Arc::from(*p));
        }
    }
    // (iface class, user-lookup hit) — natives ride as (class, false).
    let mut candidates: Vec<(Arc<str>, bool)> = Vec::new();
    while let Some(node) = queue.pop_front() {
        if seen.len() >= 1024 || !seen.insert(node.clone()) {
            continue;
        }
        if ctx.is_interface(&node) {
            let user_hit = ctx.find_method(&node, name, desc);
            let native_hit = natives::lookup_exact(&node, name, desc);
            if user_hit.is_some() || native_hit.is_some() {
                candidates.push((node.clone(), user_hit.is_some()));
            }
        }
        if let Some(ps) = ctx.parents_of_full(&node) {
            for p in ps {
                queue.push_back(Arc::from(*p));
            }
        } else if let Some(bp) = crate::builtin::builtin_parent_of(&node) {
            queue.push_back(Arc::from(bp));
        }
    }
    // Maximally specific: drop every candidate dominated by a strict
    // subtype candidate (the concrete-chain override already won above —
    // this loop only runs when it found nothing).
    let mut survivors: Vec<(Arc<str>, bool)> = Vec::new();
    for (iface, is_user) in &candidates {
        let dominated = candidates
            .iter()
            .any(|(other, _)| other != iface && ctx.is_subtype(other, iface));
        if !dominated {
            survivors.push((iface.clone(), *is_user));
        }
    }
    survivors
}
