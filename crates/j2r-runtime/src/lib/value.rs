use super::*;

/// The uniform value representation of the compiled world.
///
/// Java primitives map as follows: `boolean/byte/short/char/int` -> `Int`,
/// `long` -> `Long`, `float` -> `Float`, `double` -> `Double`. References use
/// `Ref(None)` for Java `null`.
///
/// `Str` never appears on an operand stack or in locals; it is only stored in
/// the field slot of `java/lang/String` and `java/lang/StringBuilder` objects.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Value {
    #[default]
    Void,
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    Str(Arc<String>),
    Ref(Option<JRef>),
}
impl Value {
    /// True when the value occupies two JVM operand-stack slots
    /// (`long`/`double` category-2 values).
    pub fn cat2(&self) -> bool {
        matches!(self, Value::Long(_) | Value::Double(_))
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Ref(None))
    }

    pub fn expect_int(&self) -> i32 {
        match self {
            Value::Int(v) => *v,
            other => panic!("j2r internal: expected int value, got {other:?}"),
        }
    }

    pub fn expect_long(&self) -> i64 {
        match self {
            Value::Long(v) => *v,
            other => panic!("j2r internal: expected long value, got {other:?}"),
        }
    }

    pub fn expect_float(&self) -> f32 {
        match self {
            Value::Float(v) => *v,
            other => panic!("j2r internal: expected float value, got {other:?}"),
        }
    }

    pub fn expect_double(&self) -> f64 {
        match self {
            Value::Double(v) => *v,
            other => panic!("j2r internal: expected double value, got {other:?}"),
        }
    }

    pub fn expect_ref(&self) -> Option<JRef> {
        match self {
            Value::Ref(r) => *r,
            other => panic!("j2r internal: expected reference value, got {other:?}"),
        }
    }
}
