//! Descriptor parsing (`(II)I`, `Ljava/lang/String;`, `[I`, ...).
//!
//! Shared by the code generator (which needs full type info) and the runtime
//! (which needs argument slot widths for unmarshalling).

use crate::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeDesc {
    Boolean,
    Byte,
    Short,
    Char,
    Int,
    Long,
    Float,
    Double,
    /// Class or interface in internal form (`java/lang/String`).
    Object(String),
    /// Array type descriptor (`[I`, `[Ljava/lang/String;`, `[[J`).
    Array(String),
    /// `void` (only valid as a method return type).
    Void,
}

impl TypeDesc {
    /// Default value a field of this type receives (`Value::default_fields`).
    pub fn default_value(&self) -> Value {
        match self {
            TypeDesc::Long => Value::Long(0),
            TypeDesc::Float => Value::Float(0.0),
            TypeDesc::Double => Value::Double(0.0),
            TypeDesc::Void => Value::Void,
            _ => Value::Int(0),
        }
    }

    /// Field descriptor string for this type (internal form, arrays kept as-is).
    pub fn field_desc(&self) -> String {
        match self {
            TypeDesc::Boolean => "Z".into(),
            TypeDesc::Byte => "B".into(),
            TypeDesc::Short => "S".into(),
            TypeDesc::Char => "C".into(),
            TypeDesc::Int => "I".into(),
            TypeDesc::Long => "J".into(),
            TypeDesc::Float => "F".into(),
            TypeDesc::Double => "D".into(),
            TypeDesc::Object(n) => format!("L{n};"),
            TypeDesc::Array(d) => d.clone(),
            TypeDesc::Void => "V".into(),
        }
    }
}

/// Parses a single field-type descriptor starting at `s[0]`.
/// Returns the type and how many bytes were consumed.
pub fn parse_field_desc(s: &str) -> Option<(TypeDesc, usize)> {
    let b = s.as_bytes();
    if b.is_empty() {
        return None;
    }
    match b[0] {
        b'Z' => Some((TypeDesc::Boolean, 1)),
        b'B' => Some((TypeDesc::Byte, 1)),
        b'S' => Some((TypeDesc::Short, 1)),
        b'C' => Some((TypeDesc::Char, 1)),
        b'I' => Some((TypeDesc::Int, 1)),
        b'J' => Some((TypeDesc::Long, 1)),
        b'F' => Some((TypeDesc::Float, 1)),
        b'D' => Some((TypeDesc::Double, 1)),
        b'V' => Some((TypeDesc::Void, 1)),
        b'L' => {
            let end = s.find(';')?;
            Some((TypeDesc::Object(s[1..end].to_string()), end + 1))
        }
        b'[' => {
            let (_, consumed) = parse_field_desc(&s[1..])?;
            Some((TypeDesc::Array(s[..consumed + 1].to_string()), consumed + 1))
        }
        _ => None,
    }
}

/// Parses a full method descriptor like `(IDLjava/lang/String;[I)V`.
pub fn parse_method_desc(s: &str) -> Option<(Vec<TypeDesc>, TypeDesc)> {
    let rest = s.strip_prefix('(')?;
    let mut params = Vec::new();
    let mut rest = rest;
    loop {
        if let Some(tail) = rest.strip_prefix(')') {
            let (ret, _) = parse_field_desc(tail)?;
            return Some((params, ret));
        }
        let (t, consumed) = parse_field_desc(rest)?;
        params.push(t);
        rest = &rest[consumed..];
    }
}

/// Number of JVM local slots an argument of this type occupies (2 for
/// `long`/`double`, otherwise 1).
pub fn type_slots(t: &TypeDesc) -> usize {
    match t {
        TypeDesc::Long | TypeDesc::Double => 2,
        _ => 1,
    }
}

/// Fills `locals` with `args` according to `desc`, starting at local slot
/// `start`. Instance methods pass `start = 1` because slot 0 holds `this`.
/// `long`/`double` arguments occupy two slots (the second is left `Void`).
pub fn unmarshal_args(
    locals: &mut [Value],
    args: &[Value],
    desc: &str,
    start: usize,
) -> Result<(), crate::Throw> {
    let (params, _) = parse_method_desc(desc)
        .ok_or_else(|| crate::internal_error(&format!("bad descriptor {desc:?}")))?;
    if params.len() != args.len() {
        return Err(crate::internal_error(&format!(
            "argument count mismatch for {desc:?}: {} args, {} params",
            args.len(),
            params.len()
        )));
    }
    let mut slot = start;
    for (t, a) in params.iter().zip(args.iter()) {
        locals[slot] = a.clone();
        if type_slots(t) == 2 {
            locals[slot + 1] = Value::Void;
        }
        slot += type_slots(t);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_primitives() {
        let _rt = crate::test_support::runtime();
        let (params, ret) = parse_method_desc("(IJFD)V").unwrap();
        assert_eq!(
            params,
            vec![
                TypeDesc::Int,
                TypeDesc::Long,
                TypeDesc::Float,
                TypeDesc::Double
            ]
        );
        assert_eq!(ret, TypeDesc::Void);
    }

    #[test]
    fn parses_objects_and_arrays() {
        let _rt = crate::test_support::runtime();
        let (params, ret) = parse_method_desc("([Ljava/lang/String;[[ID)J").unwrap();
        assert_eq!(
            params,
            vec![
                TypeDesc::Array("[Ljava/lang/String;".into()),
                TypeDesc::Array("[[I".into()),
                TypeDesc::Double
            ]
        );
        assert_eq!(ret, TypeDesc::Long);
    }

    #[test]
    fn parses_object_containing_semicolon_class_names() {
        let _rt = crate::test_support::runtime();
        let (params, _) = parse_method_desc("(Ljava/lang/String;)Ljava/lang/String;").unwrap();
        assert_eq!(params, vec![TypeDesc::Object("java/lang/String".into())]);
    }

    #[test]
    fn rejects_garbage() {
        let _rt = crate::test_support::runtime();
        assert!(parse_method_desc("(X)V").is_none());
        assert!(parse_method_desc("(I").is_none());
    }

    #[test]
    fn unmarshal_two_slot_args() {
        let _rt = crate::test_support::runtime();
        let mut locals = vec![Value::Void; 6];
        unmarshal_args(&mut locals, &[Value::Int(7), Value::Long(9)], "(IJ)V", 0).unwrap();
        assert_eq!(locals[0].expect_int(), 7);
        assert_eq!(locals[1].expect_long(), 9);
        assert!(matches!(locals[2], Value::Void));
    }
}
