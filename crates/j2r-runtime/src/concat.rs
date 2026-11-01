//! Java string concatenation — the runtime side of `invokedynamic
//! makeConcatWithConstants` (how `javac` compiles `+` since Java 9).
//!
//! The recipe embeds dynamic argument positions as `\u0001` and
//! bootstrap constants as `\u0002`; everything else is literal text.

use crate::object::{self, is_string};
use crate::Value;
use crate::{Dispatch, Throw};

/// Formats one value the way Java would convert it for concatenation.
/// `kind` comes from the invokedynamic descriptor: one of `I J F D C Z B S L`.
pub fn format_value(v: &Value, kind: char, ctx: Dispatch) -> Result<String, Throw> {
    let s = match (kind, v) {
        // int-like primitives all render as integers
        ('I' | 'B' | 'S' | 'C', Value::Int(i)) => {
            if kind == 'C' {
                char::from_u32(*i as u32)
                    .unwrap_or(char::REPLACEMENT_CHARACTER)
                    .to_string()
            } else {
                i.to_string()
            }
        }
        ('J', Value::Long(l)) => l.to_string(),
        ('F', Value::Float(f)) => crate::javafmt::float_to_string(*f),
        ('D', Value::Double(d)) => crate::javafmt::double_to_string(*d),
        ('Z', Value::Int(i)) => bool_string(*i),
        ('L' | 'I' | 'J' | 'F' | 'D' | 'C' | 'Z' | 'B' | 'S', Value::Ref(r)) => {
            // defensive: some code paths pass references with primitive kinds
            format_ref(r.as_ref(), ctx)?
        }
        (k, other) => {
            return Err(crate::internal_error(&format!(
                "concat formatting {other:?} as {k:?}"
            )))
        }
    };
    Ok(s)
}

fn bool_string(i: i32) -> String {
    if i != 0 {
        "true".to_string()
    } else {
        "false".to_string()
    }
}

/// Renders a reference: `null`, string contents, or `toString()` dispatch.
pub fn format_ref(r: Option<&crate::heap::JRef>, ctx: Dispatch) -> Result<String, Throw> {
    match r {
        None => Ok("null".to_string()),
        Some(obj) => {
            if is_string(obj) {
                Ok(object::string_value(obj).to_string())
            } else {
                match crate::invoke_virtual(
                    obj,
                    "java/lang/Object",
                    "toString",
                    "()Ljava/lang/String;",
                    &[],
                    ctx,
                ) {
                    Ok(Value::Ref(Some(s))) => Ok(object::string_value(&s).to_string()),
                    Ok(Value::Ref(None)) => Ok("null".to_string()),
                    Ok(other) => Err(crate::internal_error(&format!(
                        "toString returned {other:?}"
                    ))),
                    Err(t) => Err(t),
                }
            }
        }
    }
}

/// Renders a bootstrap-method constant slot (primitives and strings only in
/// milestone 1).
fn format_constant(v: &Value, ctx: Dispatch) -> Result<String, Throw> {
    match v {
        Value::Int(i) => Ok(i.to_string()),
        Value::Long(l) => Ok(l.to_string()),
        Value::Float(f) => Ok(crate::javafmt::float_to_string(*f)),
        Value::Double(d) => Ok(crate::javafmt::double_to_string(*d)),
        Value::Str(s) => Ok(s.to_string()),
        Value::Ref(r) => format_ref(r.as_ref(), ctx),
        Value::Void => Err(crate::internal_error("void constant in concat recipe")),
    }
}

/// Executes `makeConcatWithConstants`.
///
/// * `recipe` — the bootstrap constant recipe string,
/// * `constants` — the bootstrap constants consumed at each `\u0002`,
/// * `args` — the dynamic arguments consumed at each `\u0001`,
/// * `arg_kinds` — one descriptor char per dynamic argument.
pub fn string_concat(
    recipe: &str,
    constants: &[Value],
    args: &[Value],
    arg_kinds: &str,
    ctx: Dispatch,
) -> Result<Value, Throw> {
    let mut out = String::new();
    let mut arg_i = 0usize;
    let mut const_i = 0usize;
    for ch in recipe.chars() {
        match ch {
            '\u{1}' => {
                let v = args
                    .get(arg_i)
                    .ok_or_else(|| crate::internal_error("concat recipe has more slots than args"))?
                    .clone();
                let kind = arg_kinds.chars().nth(arg_i).unwrap_or('L');
                out.push_str(&format_value(&v, kind, ctx)?);
                arg_i += 1;
            }
            '\u{2}' => {
                let v = constants
                    .get(const_i)
                    .ok_or_else(|| {
                        crate::internal_error("concat recipe has more slots than constants")
                    })?
                    .clone();
                out.push_str(&format_constant(&v, ctx)?);
                const_i += 1;
            }
            c => out.push(c),
        }
    }
    Ok(Value::Ref(Some(object::new_string(&out))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_dispatch() -> Dispatch {
        Dispatch::default()
    }

    #[test]
    fn plain_recipe() {
        let r = string_concat("hello world", &[], &[], "", no_dispatch()).unwrap();
        assert_eq!(
            &*object::string_value(&r.expect_ref().unwrap()),
            "hello world"
        );
    }

    #[test]
    fn mixed_args_and_constants() {
        // javac for "x=" + 5 + 1.0d + 'c' + true + s, roughly:
        // recipe "x=\u0001=\u0002 \u0001..." — here a synthetic mix
        let r = string_concat(
            "n=\u{1} c=\u{2} s=\u{1}!",
            &[Value::Double(2.5)],
            &[Value::Int(42), Value::Ref(Some(object::new_string("hi")))],
            "IL",
            no_dispatch(),
        )
        .unwrap();
        assert_eq!(
            &*object::string_value(&r.expect_ref().unwrap()),
            "n=42 c=2.5 s=hi!"
        );
    }

    #[test]
    fn null_and_boolean() {
        let r = string_concat(
            "\u{1}/\u{1}",
            &[],
            &[Value::Ref(None), Value::Int(1)],
            "LZ",
            no_dispatch(),
        )
        .unwrap();
        assert_eq!(
            &*object::string_value(&r.expect_ref().unwrap()),
            "null/true"
        );
    }

    #[test]
    fn char_kind() {
        let r = string_concat("[\u{1}]", &[], &[Value::Int(65)], "C", no_dispatch()).unwrap();
        assert_eq!(&*object::string_value(&r.expect_ref().unwrap()), "[A]");
    }
}
