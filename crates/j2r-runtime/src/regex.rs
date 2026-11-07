//! java.util.regex — the narrow builtin surface the stringsim corpus wave
//! (2026-11-07) actually drives: `Pattern.compile("\\s+")` +
//! `matcher(input).replaceAll(" ")`, the whitespace-run collapse inside
//! ShingleBased.getProfile.
//!
//! The registered gap (docs/CORPUS_WAVE.md, external-report B21) demanded
//! ONE law before anything else landed: **Java's default `\s` is the ASCII
//! class [ \t\n\x0B\f\r]** — six characters — while Rust's regex `\s` is
//! Unicode-aware (it matches U+00A0 NBSP, U+3000 ideographic space, ...).
//! A naive mapping silently changes every shingle profile built from
//! Unicode-whitespace input, so this engine hardcodes the six-character
//! class and the probe pins NBSP survival differentially.
//!
//! The supported grammar is exactly what callers use today: a single
//! `\s` shorthand with a `+` quantifier. Any other pattern fails LOUDLY
//! with an exception naming the registered gap (the honest surface rule:
//! register what is called; the corpus runner names the next shape).

use crate::heap::{with_obj, with_obj_mut, JRef};
use crate::object::new_obj;
use crate::{internal_error, throw_new, Dispatch, Throw, Value};

/// Java Pattern's default `\s` — the ASCII class, pinned by the
/// StringsSimCorpus differential (NBSP must survive `\s+` collapse).
fn is_ascii_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r')
}

const PATTERN_TEXT: usize = 0;
const M_PATTERN: usize = 0;
const M_INPUT: usize = 1;

/// `Pattern.compile(String)` — validates the supported grammar loudly.
pub fn n_pattern_compile(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let pat = match args.first() {
        Some(Value::Ref(Some(o))) => crate::object::string_value(o).to_string(),
        Some(Value::Ref(None)) => {
            return Err(throw_new(
                "java/lang/IllegalArgumentException",
                Some("Cannot compile null pattern"),
            ))
        }
        _ => return Err(internal_error("Pattern.compile needs a pattern")),
    };
    // The supported grammar today: one `\s` shorthand with a `+` quantifier.
    if pat == "\\s+" {
        let p = new_obj("java/util/regex/Pattern", 1)?;
        with_obj_mut(p, |o| {
            o.slots[PATTERN_TEXT] = Value::Str(std::sync::Arc::new(pat))
        });
        Ok(Value::Ref(Some(p)))
    } else {
        Err(throw_new(
            "java/lang/IllegalArgumentException",
            Some(&format!(
                "j2r: unsupported regex (registered gap, docs/CORPUS_WAVE.md): {pat} — only \\s+ is modelled"
            )),
        ))
    }
}

/// `Pattern.matcher(CharSequence)` — binds the input; stateless beyond
/// that because replaceAll is the only modelled operation.
pub fn n_pattern_matcher(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let p = recv.expect("j2r internal: Pattern.matcher without receiver");
    let input = match args.first() {
        Some(Value::Ref(Some(o))) => crate::object::string_value(o).to_string(),
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(internal_error("Pattern.matcher needs a CharSequence")),
    };
    let m = new_obj("java/util/regex/Matcher", 2)?;
    let pat_text = with_obj(*p, |o| match &o.slots[PATTERN_TEXT] {
        Value::Str(s) => s.clone(),
        other => panic!("j2r internal: Pattern slot {other:?}"),
    });
    with_obj_mut(m, |o| {
        o.slots[M_PATTERN] = Value::Str(pat_text);
        o.slots[M_INPUT] = Value::Str(std::sync::Arc::new(input));
    });
    Ok(Value::Ref(Some(m)))
}

/// `Matcher.replaceAll(String)` — the ASCII-\s+-run collapse. The JDK
/// replaces each maximal run of [ \t\n\x0B\f\r] with the replacement.
pub fn n_matcher_replace_all(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let m = recv.expect("j2r internal: Matcher.replaceAll without receiver");
    let replacement = match args.first() {
        Some(Value::Ref(Some(o))) => crate::object::string_value(o).to_string(),
        Some(Value::Ref(None)) => return Err(throw_new("java/lang/NullPointerException", None)),
        _ => return Err(internal_error("Matcher.replaceAll needs a replacement")),
    };
    let (input, pat) = with_obj(*m, |o| {
        let input = match &o.slots[M_INPUT] {
            Value::Str(s) => s.to_string(),
            other => panic!("j2r internal: Matcher input {other:?}"),
        };
        let pat = match &o.slots[M_PATTERN] {
            Value::Str(s) => s.to_string(),
            other => panic!("j2r internal: Matcher pattern {other:?}"),
        };
        (input, pat)
    });
    debug_assert_eq!(pat, "\\s+");
    let mut out = String::with_capacity(input.len());
    let mut run = String::new();
    for c in input.chars() {
        if is_ascii_space(c) {
            run.push(c);
        } else {
            if !run.is_empty() {
                out.push_str(&replacement);
                run.clear();
            }
            out.push(c);
        }
    }
    if !run.is_empty() {
        out.push_str(&replacement);
    }
    Ok(Value::Ref(Some(crate::object::new_string(&out))))
}
