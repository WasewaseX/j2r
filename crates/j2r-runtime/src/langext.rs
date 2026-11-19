//! `java.lang` method extensions — Wave 2 of M3 (`docs/M3_PLAN.md` §5.2).
//!
//! Everything here is registered in the natives table (`natives.rs`) and
//! behaves exactly like the corresponding JDK method on Temurin 21 for the
//! supported set; anything outside the set keeps failing loudly at compile
//! time (not registered = resolve error).
//!
//! Java-exact notes implemented here:
//! * `String.trim` removes chars `<= U+0020` (NOT Unicode whitespace),
//! * `String.compareTo` compares UTF-16 code units (not Rust `str` order),
//! * `Math.floorDiv/floorMod` have Java's `/ by zero` and `MIN/-1` edges,
//! * `Math.addExact/subtractExact/multiplyExact` throw the JDK messages
//!   (`integer overflow` / `long overflow`),
//! * `Integer.parseInt` throws `NumberFormatException: For input string:
//!   "..."` (already registered in M1; extended here by the radix form).
//!
//! Character classification is implemented for ASCII plus the pinned
//! Unicode ranges exercised by the `CharClassify` differential fixture
//! (the three-channel harness is the arbiter — M3_PLAN §5.2).

use crate::heap::{with_obj, JRef};
use crate::Value;
use crate::{throw_new, Dispatch, Throw};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
fn str_of(v: &Value, who: &str) -> Arc<String> {
    match v {
        Value::Ref(Some(r)) => crate::object::string_value(r),
        _ => panic!("j2r internal: {who} without string"),
    }
}
/// UTF-16 code units of a string value (Java's unit of indexing).
fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn from_utf16(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}
fn this(recv: Option<&JRef>, who: &str) -> JRef {
    *recv.unwrap_or_else(|| panic!("j2r internal: {who} called without receiver"))
}
fn arg<'a>(args: &'a [Value], i: usize, who: &str) -> &'a Value {
    args.get(i)
        .unwrap_or_else(|| panic!("j2r internal: {who} missing argument {i}"))
}
fn i32_at(args: &[Value], i: usize, who: &str) -> i32 {
    arg(args, i, who).expect_int()
}
fn string_index_oob(msg: String) -> Throw {
    throw_new("java/lang/StringIndexOutOfBoundsException", Some(&msg))
}

// ---------------------------------------------------------------- String
static MATH_RANDOM_UNIQUIFIER: AtomicI64 = AtomicI64::new(8682522807148012199);

// --- scanaislop split: the items below live verbatim in the child modules ---
mod char;
mod math;
mod numbers;
mod string;
mod stringbuilder;
#[cfg(test)]
mod tests;
pub(crate) use char::*;
pub(crate) use math::*;
pub(crate) use numbers::*;
pub(crate) use string::*;
pub(crate) use stringbuilder::*;
