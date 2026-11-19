use super::*;

/// One entry of a method's exception table.
#[derive(Clone, Debug)]
pub struct ExceptionEntry {
    pub start_pc: u16,
    pub end_pc: u16,
    pub handler_pc: u16,
    /// `None` = catch-all (a `finally` block).
    pub catch_type: Option<String>,
}
/// One `LocalVariableTypeTable` entry (JVMS §4.7.14): the GENERIC type of
/// one local-variable slot over one pc range. This is where javac records
/// `Map<String, Double>` for a local declared with that type — the erased
/// descriptor says `Ljava/util/Map;`, only the LVTT carries the type
/// arguments. The G3.3 signature-pin layer reads exactly this.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalVarType {
    pub start_pc: u16,
    pub length: u16,
    pub name: String,
    /// The raw generic signature (JVMS §4.7.9.1), unparsed.
    pub signature: String,
    /// The local-variable slot index (category-2 types still use one index).
    pub index: u16,
}
#[derive(Clone, Debug)]
pub struct CodeAttribute {
    pub max_stack: u16,
    pub max_locals: u16,
    pub code: Vec<u8>,
    pub exception_table: Vec<ExceptionEntry>,
    /// `LocalVariableTypeTable` entries, in classfile order (JVMS says no
    /// canonical order; javac emits them in slot order). Empty when absent.
    pub lvtt: Vec<LocalVarType>,
}
#[derive(Clone, Debug)]
pub struct FieldInfo {
    pub access: u16,
    pub name: String,
    pub descriptor: String,
    /// `ConstantValue` for static finals, if present.
    pub constant_value: Option<ConstantValue>,
    /// `Signature` attribute (JVMS §4.7.9): the field's generic signature
    /// (e.g. `Ljava/util/Map<Ljava/lang/String;Ljava/lang/Double;>;`),
    /// unparsed. `None` for non-generic fields.
    pub signature: Option<String>,
}
impl FieldInfo {
    pub fn is_static(&self) -> bool {
        self.access & flags::ACC_STATIC != 0
    }

    /// `ACC_VOLATILE` (G4.2 arm (b)): the input bytecode promises Java
    /// volatile semantics; j2r compiles the field as a plain slot access,
    /// so the boundary must be marked at every generated use site.
    pub fn is_volatile(&self) -> bool {
        self.access & flags::ACC_VOLATILE != 0
    }

    /// `ACC_FINAL`. A `static final` WITH a `ConstantValue` is folded at
    /// compile time and carries no boundary (the JMM explicitly allows
    /// constant folding); a final field without one is a plain slot and
    /// the freeze semantics are absent.
    pub fn is_final(&self) -> bool {
        self.access & flags::ACC_FINAL != 0
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum ConstantValue {
    Int(i32),
    Float(f32),
    Long(i64),
    Double(f64),
    String(String),
}
#[derive(Clone, Debug)]
pub struct MethodInfo {
    pub access: u16,
    pub name: String,
    pub descriptor: String,
    pub code: Option<CodeAttribute>,
    /// `Signature` attribute (JVMS §4.7.9): the method's generic signature
    /// (type parameters, parameter type arguments, throws), unparsed.
    pub signature: Option<String>,
}
impl MethodInfo {
    pub fn is_static(&self) -> bool {
        self.access & flags::ACC_STATIC != 0
    }

    pub fn is_abstract(&self) -> bool {
        self.access & flags::ACC_ABSTRACT != 0
    }

    /// Returns the method body or an error when the method is abstract/native
    /// and has no `Code` attribute.
    pub fn code(&self) -> Result<&CodeAttribute> {
        self.code.as_ref().ok_or_else(|| {
            crate::Error::new(
                0,
                format!(
                    "method {}{} has no Code attribute (abstract or native)",
                    self.name, self.descriptor
                ),
            )
        })
    }
}
#[derive(Clone, Debug)]
pub struct BootstrapMethod {
    /// Index into the constant pool of a `MethodHandle`.
    pub method_handle: u16,
    /// Bootstrap arguments, pre-resolved to what J2R supports.
    pub arguments: Vec<BootstrapArgument>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum BootstrapArgument {
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    String(String),
    /// A `CONSTANT_MethodType` argument, resolved to its descriptor
    /// (M3 Wave 3: `LambdaMetafactory`'s `samMethodType` and
    /// `instantiatedMethodType`).
    MethodType(String),
    /// A `CONSTANT_MethodHandle` argument, resolved to its kind and the
    /// member it references (M3 Wave 3: the `implMethod` argument).
    MethodHandle {
        kind: u8,
        class: String,
        name: String,
        desc: String,
    },
    /// A `CONSTANT_Class` argument, resolved to its internal name.
    Class(String),
    /// Other exotic constants remain opaque so the code generator can
    /// report a precise error if it ever needs them.
    Opaque {
        tag: &'static str,
    },
}
