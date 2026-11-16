//! Operand-stack operations used by generated code.
//!
//! The `dup*`/`pop2` families behave differently depending on whether values
//! are category-1 or category-2 (`long`/`double`). J2R represents a category-2
//! value as a single [`Value`], so these helpers branch on [`Value::cat2`] to
//! reproduce every form of JVMS §6.5 exactly (verified against the spec text).
//!
//! Spec reference (JVMS se21 §6.5), stack shown bottom-to-top:
//!
//! * `dup2`: F1 `v2,v1 -> v2,v1,v2,v1` (cat1,cat1); F2 `v1 -> v1,v1` (cat2)
//! * `dup_x1`: `v2,v1 -> v1,v2,v1` (cat1,cat1)
//! * `dup_x2`: F1 `v3,v2,v1 -> v1,v3,v2,v1` (all cat1); F2 `v2,v1 -> v1,v2,v1` (v2 cat2, v1 cat1)
//! * `dup2_x1`: F1 `v3,v2,v1 -> v2,v1,v3,v2,v1` (all cat1); F2 `v2,v1 -> v1,v2,v1` (**v1 cat2**, v2 cat1)
//! * `dup2_x2`: F1 `v4,v3,v2,v1 -> v2,v1,v4,v3,v2,v1` (all cat1); F2 `v3,v2,v1 -> v1,v3,v2,v1` (v1 cat2, v2 v3 cat1); F3 `v3,v2,v1 -> v2,v1,v3,v2,v1` (v3 cat2, v1 v2 cat1); F4 `v2,v1 -> v1,v2,v1` (both cat2)
//! * `pop2`: F1 pop two cat1; F2 pop one cat2
//! * `swap`: `v2,v1 -> v1,v2` (both cat1)

use crate::heap::JRef;
use crate::Throw;
use crate::Value;

/// Pops one value of any shape.
pub fn stack_pop(s: &mut Vec<Value>) -> Value {
    s.pop().expect("j2r internal: pop on empty operand stack")
}

/// Pops `n` values and returns them in bottom-to-top (original) order.
pub fn pop_n(s: &mut Vec<Value>, n: usize) -> Vec<Value> {
    assert!(
        s.len() >= n,
        "j2r internal: pop_n({n}) on stack of {} values",
        s.len()
    );
    s.split_off(s.len() - n)
}

pub fn pop_int(s: &mut Vec<Value>) -> i32 {
    stack_pop(s).expect_int()
}

pub fn pop_long(s: &mut Vec<Value>) -> i64 {
    stack_pop(s).expect_long()
}

pub fn pop_float(s: &mut Vec<Value>) -> f32 {
    stack_pop(s).expect_float()
}

pub fn pop_double(s: &mut Vec<Value>) -> f64 {
    stack_pop(s).expect_double()
}

/// Pops a reference; `null` raises `NullPointerException` at the caller's
/// faulting pc.
pub fn pop_ref(s: &mut Vec<Value>) -> Result<JRef, Throw> {
    pop_ref_msg(s, None)
}

/// The `pop_ref` law with the emitter-computed helpful-NPE message (the
/// JEP 358 residual, standing run 4): `msg` is the STATIC message string
/// the codegen built from the faulting site's own facts (the receiver's
/// static class, the (name, desc), the producing instruction or the local
/// slot — the OpenJDK `bytecodeUtils.cpp` grammar, oracle-pinned by
/// NpeShapeProbe2 and the extended captures in scripts/evidence/helpful_npe/).
/// The message lands on the thrown object's `detailMessage` slot, so
/// `getMessage()` reads it exactly like Temurin's. `msg = None` is the
/// bare NPE (the pre-existing law) — sites without a computable message
/// keep emitting plain `pop_ref`, so their generated code is unchanged.
pub fn pop_ref_msg(s: &mut Vec<Value>, msg: Option<&str>) -> Result<JRef, Throw> {
    match stack_pop(s) {
        Value::Ref(Some(r)) => Ok(r),
        _ => Err(crate::throw_new("java/lang/NullPointerException", msg)),
    }
}

/// The `pop_ref` law applied to a value that is already off the stack —
/// the typed-operand emitter reads stack positions in place and needs the
/// exact same null handling (`Value::Ref(None)` and non-ref kinds raise
/// `NullPointerException` exactly as `pop_ref` does). Pure: borrows the
/// value, touches no stack.
pub fn ref_of(v: &Value) -> Result<JRef, Throw> {
    match v {
        Value::Ref(Some(r)) => Ok(*r),
        _ => Err(crate::throw_new("java/lang/NullPointerException", None)),
    }
}

fn top(s: &[Value]) -> &Value {
    s.last().expect("j2r internal: peek on empty stack")
}

pub fn stack_pop2(s: &mut Vec<Value>) {
    if top(s).cat2() {
        s.pop();
    } else {
        assert!(s.len() >= 2, "j2r internal: pop2 underflow");
        s.pop();
        s.pop();
    }
}

pub fn stack_dup(s: &mut Vec<Value>) {
    let v = top(s).clone();
    s.push(v);
}

pub fn stack_dup_x1(s: &mut Vec<Value>) {
    let v1 = s.pop().expect("j2r internal: dup_x1 underflow");
    let v2 = s.pop().expect("j2r internal: dup_x1 underflow");
    s.push(v1.clone());
    s.push(v2);
    s.push(v1);
}

/// `dup_x2`, both spec forms.
pub fn stack_dup_x2(s: &mut Vec<Value>) {
    let v1 = s.pop().expect("j2r internal: dup_x2 underflow");
    // values sitting below v1: one category-2 value, or two category-1 values
    let depth = if top(s).cat2() { 1 } else { 2 };
    assert!(s.len() >= depth, "j2r internal: dup_x2 underflow");
    let at = s.len() - depth;
    s.insert(at, v1.clone());
    s.push(v1);
}

/// `dup2`, both spec forms.
pub fn stack_dup2(s: &mut Vec<Value>) {
    if top(s).cat2() {
        let v = top(s).clone();
        s.push(v);
    } else {
        assert!(s.len() >= 2, "j2r internal: dup2 underflow");
        let v2 = s[s.len() - 2].clone();
        let v1 = s[s.len() - 1].clone();
        s.push(v2);
        s.push(v1);
    }
}

/// `dup2_x1`, both spec forms.
pub fn stack_dup2_x1(s: &mut Vec<Value>) {
    let v1 = s.pop().expect("j2r internal: dup2_x1 underflow");
    let v2 = s.pop().expect("j2r internal: dup2_x1 underflow");
    if v1.cat2() {
        // Form 2: v2(cat1), v1(cat2) -> v1, v2, v1
        s.push(v1.clone());
        s.push(v2);
        s.push(v1);
    } else {
        // Form 1: v3, v2, v1 (all cat1) -> v2, v1, v3, v2, v1
        let v3 = s.pop().expect("j2r internal: dup2_x1 underflow");
        s.push(v2.clone());
        s.push(v1.clone());
        s.push(v3);
        s.push(v2);
        s.push(v1);
    }
}

/// `dup2_x2`, all four spec forms. The form is decided by the categories of
/// the top two values (v1 is topmost):
///
/// * (cat2, cat2) F4: `v2,v1 -> v1,v2,v1`
/// * (cat2, cat1) F2: `v3,v2,v1 -> v1,v3,v2,v1`
/// * (cat1, cat2) F3: `v3,v2,v1 -> v2,v1,v3,v2,v1`
/// * (cat1, cat1) F1: `v4,v3,v2,v1 -> v2,v1,v4,v3,v2,v1`
pub fn stack_dup2_x2(s: &mut Vec<Value>) {
    let v1 = s.pop().expect("j2r internal: dup2_x2 underflow");
    let v2 = s.pop().expect("j2r internal: dup2_x2 underflow");
    match (v1.cat2(), v2.cat2()) {
        (true, true) => {
            // Form 4
            s.push(v1.clone());
            s.push(v2);
            s.push(v1);
        }
        (true, false) => {
            // Form 2
            let v3 = s.pop().expect("j2r internal: dup2_x2 underflow");
            s.push(v1.clone());
            s.push(v3);
            s.push(v2);
            s.push(v1);
        }
        (false, true) => {
            // Form 3
            let v3 = s.pop().expect("j2r internal: dup2_x2 underflow");
            s.push(v2.clone());
            s.push(v1.clone());
            s.push(v3);
            s.push(v2);
            s.push(v1);
        }
        (false, false) => {
            // v1 and v2 are cat-1; what sits below decides F1 vs F3
            if top(s).cat2() {
                // Form 3: v3 (cat2), v2, v1 -> v2, v1, v3, v2, v1
                let v3 = s.pop().expect("j2r internal: dup2_x2 underflow");
                s.push(v2.clone());
                s.push(v1.clone());
                s.push(v3);
                s.push(v2);
                s.push(v1);
            } else {
                // Form 1: v4, v3, v2, v1 (all cat1) -> v2, v1, v4, v3, v2, v1
                let v3 = s.pop().expect("j2r internal: dup2_x2 underflow");
                let v4 = s.pop().expect("j2r internal: dup2_x2 underflow");
                s.push(v2.clone());
                s.push(v1.clone());
                s.push(v4);
                s.push(v3);
                s.push(v2);
                s.push(v1);
            }
        }
    }
}

pub fn stack_swap(s: &mut Vec<Value>) {
    let v1 = s.pop().expect("j2r internal: swap underflow");
    let v2 = s.pop().expect("j2r internal: swap underflow");
    s.push(v1);
    s.push(v2);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i(v: i32) -> Value {
        Value::Int(v)
    }
    fn l(v: i64) -> Value {
        Value::Long(v)
    }

    #[test]
    fn pop2_handles_both_categories() {
        let _rt = crate::test_support::runtime();
        let mut s = vec![i(1), l(7)];
        stack_pop2(&mut s);
        assert_eq!(s.len(), 1);
        let mut s = vec![i(1), i(2), i(3)];
        stack_pop2(&mut s);
        assert_eq!(s, vec![i(1)]);
    }

    #[test]
    fn dup2_both_forms() {
        let _rt = crate::test_support::runtime();
        // F2: single cat2 value
        let mut s = vec![i(1), l(7)];
        stack_dup2(&mut s);
        assert_eq!(s, vec![i(1), l(7), l(7)]);
        // F1: two cat1 values
        let mut s = vec![i(1), i(2), i(3)];
        stack_dup2(&mut s);
        assert_eq!(s, vec![i(1), i(2), i(3), i(2), i(3)]);
    }

    #[test]
    fn dup_x1_works() {
        let _rt = crate::test_support::runtime();
        let mut s = vec![i(2), i(1)];
        stack_dup_x1(&mut s);
        assert_eq!(s, vec![i(1), i(2), i(1)]);
    }

    #[test]
    fn dup_x2_both_forms() {
        let _rt = crate::test_support::runtime();
        // F1: three cat-1 values
        let mut s = vec![i(3), i(2), i(1)];
        stack_dup_x2(&mut s);
        assert_eq!(s, vec![i(1), i(3), i(2), i(1)]);
        // F2: cat-2 below cat-1
        let mut s = vec![l(9), i(1)];
        stack_dup_x2(&mut s);
        assert_eq!(s, vec![i(1), l(9), i(1)]);
    }

    #[test]
    fn dup2_x1_both_forms() {
        let _rt = crate::test_support::runtime();
        // F1: all cat1
        let mut s = vec![i(3), i(2), i(1)];
        stack_dup2_x1(&mut s);
        assert_eq!(s, vec![i(2), i(1), i(3), i(2), i(1)]);
        // F2: v1 is cat2, v2 is cat1
        let mut s = vec![i(5), l(9)];
        stack_dup2_x1(&mut s);
        assert_eq!(s, vec![l(9), i(5), l(9)]);
    }

    #[test]
    fn dup2_x2_all_four_forms() {
        let _rt = crate::test_support::runtime();
        // F1: all cat1
        let mut s = vec![i(4), i(3), i(2), i(1)];
        stack_dup2_x2(&mut s);
        assert_eq!(s, vec![i(2), i(1), i(4), i(3), i(2), i(1)]);
        // F2: v1 cat2, v2,v3 cat1
        let mut s = vec![i(7), i(8), l(9)];
        stack_dup2_x2(&mut s);
        assert_eq!(s, vec![l(9), i(7), i(8), l(9)]);
        // F3: v3 cat2, v2,v1 cat1
        let mut s = vec![l(5), i(6), i(7)];
        stack_dup2_x2(&mut s);
        assert_eq!(s, vec![i(6), i(7), l(5), i(6), i(7)]);
        // F4: both cat2
        let mut s = vec![l(5), l(9)];
        stack_dup2_x2(&mut s);
        assert_eq!(s, vec![l(9), l(5), l(9)]);
    }

    #[test]
    fn swap_works() {
        let _rt = crate::test_support::runtime();
        let mut s = vec![i(2), i(1)];
        stack_swap(&mut s);
        assert_eq!(s, vec![i(1), i(2)]);
    }

    #[test]
    fn pop_n_returns_original_order() {
        let _rt = crate::test_support::runtime();
        let mut s = vec![i(1), i(2), i(3)];
        assert_eq!(pop_n(&mut s, 2), vec![i(2), i(3)]);
        assert_eq!(s, vec![i(1)]);
    }

    // ------------------------------------------------ helpful-NPE laws
    // (JEP 358 residual, standing run 4): `pop_ref_msg` lands the
    // emitter-computed message on the thrown object's `detailMessage`
    // slot (slot 0) so `Throwable.getMessage()` reads it — Temurin's
    // oracle behavior, probed by NpeShapeProbe2 and the extended
    // captures in scripts/evidence/helpful_npe/.

    #[test]
    fn pop_ref_msg_carries_the_message_on_null() {
        let _rt = crate::test_support::runtime();
        let mut s = vec![Value::Ref(None)];
        let t = pop_ref_msg(
            &mut s,
            Some("Cannot invoke \"String.length()\" because \"<local1>\" is null"),
        )
        .expect_err("null receiver must throw");
        let msg = crate::heap::with_obj(t.obj, |o| o.slots[0].clone());
        let Value::Ref(Some(r)) = msg else {
            panic!("detailMessage slot must hold the string");
        };
        assert_eq!(
            crate::object::string_value(&r).as_str(),
            "Cannot invoke \"String.length()\" because \"<local1>\" is null"
        );
    }

    #[test]
    fn pop_ref_msg_none_is_the_bare_npe_law() {
        let _rt = crate::test_support::runtime();
        // None == the pre-existing pop_ref law: the slot holds a null
        // ref and getMessage() reads it back as null.
        let mut s = vec![Value::Ref(None)];
        let t = pop_ref_msg(&mut s, None).expect_err("null receiver must throw");
        let msg = crate::heap::with_obj(t.obj, |o| o.slots[0].clone());
        assert_eq!(msg, Value::Ref(None));
        // pop_ref delegates: the same law.
        let mut s = vec![Value::Ref(None)];
        let t2 = pop_ref(&mut s).expect_err("null receiver must throw");
        let msg2 = crate::heap::with_obj(t2.obj, |o| o.slots[0].clone());
        assert_eq!(msg2, Value::Ref(None));
    }

    #[test]
    fn pop_ref_msg_success_law_unchanged() {
        let _rt = crate::test_support::runtime();
        // A non-null receiver pops identically with and without a
        // message — the message never touches the success path.
        let obj = crate::object::new_string("s");
        let mut s = vec![Value::Ref(Some(obj))];
        let r = pop_ref_msg(&mut s, Some("unused")).expect("non-null pops");
        assert_eq!(crate::object::string_value(&r).as_str(), "s");
        assert!(s.is_empty());
    }
}
