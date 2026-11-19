//! `j2r-classfile` — a complete Java `.class` file parser for the J2R
//! compiler.
//!
//! Parses the full constant pool (all 17 tags), fields, methods, the `Code`
//! attribute (including exception tables) and `BootstrapMethods` (needed for
//! `invokedynamic` string concatenation). Unknown attributes are skipped by
//! length, exactly as the class-file format requires, so newer JDK class
//! files parse fine.
//!
//! The parser validates structure (magic, pool layout, two-slot
//! `long`/`double` entries) and reports precise errors with the byte offset
//! where parsing failed.

#![forbid(unsafe_code)]

use std::fmt;
pub mod crc32;
pub mod inflate;
pub mod remap;
pub mod remap_attrs;
pub mod remap_pass;
pub mod zip;
pub mod flags {
    pub const ACC_PUBLIC: u16 = 0x0001;
    pub const ACC_PRIVATE: u16 = 0x0002;
    pub const ACC_PROTECTED: u16 = 0x0004;
    pub const ACC_STATIC: u16 = 0x0008;
    pub const ACC_FINAL: u16 = 0x0010;
    pub const ACC_SUPER: u16 = 0x0020;
    pub const ACC_SYNCHRONIZED: u16 = 0x0020;
    pub const ACC_VOLATILE: u16 = 0x0040;
    pub const ACC_BRIDGE: u16 = 0x0040;
    pub const ACC_TRANSIENT: u16 = 0x0080;
    pub const ACC_VARARGS: u16 = 0x0080;
    pub const ACC_NATIVE: u16 = 0x0100;
    pub const ACC_INTERFACE: u16 = 0x0200;
    pub const ACC_ABSTRACT: u16 = 0x0400;
    pub const ACC_STRICT: u16 = 0x0800;
    pub const ACC_SYNTHETIC: u16 = 0x1000;
    pub const ACC_ANNOTATION: u16 = 0x2000;
    pub const ACC_ENUM: u16 = 0x4000;
}
/// One resolved constant-pool entry. Indices refer to other entries.
#[derive(Clone, Debug, PartialEq)]
pub enum CpEntry {
    Utf8(String),
    Integer(i32),
    Float(f32),
    /// `long` constants also occupy the following (unused) pool slot.
    Long(i64),
    /// `double` constants also occupy the following (unused) pool slot.
    Double(f64),
    Class(u16),
    String(u16),
    FieldRef {
        class: u16,
        name_and_type: u16,
    },
    MethodRef {
        class: u16,
        name_and_type: u16,
    },
    InterfaceMethodRef {
        class: u16,
        name_and_type: u16,
    },
    NameAndType {
        name: u16,
        descriptor: u16,
    },
    MethodHandle {
        kind: u8,
        index: u16,
    },
    MethodType(u16),
    Dynamic {
        bootstrap: u16,
        name_and_type: u16,
    },
    InvokeDynamic {
        bootstrap: u16,
        name_and_type: u16,
    },
    Module(u16),
    Package(u16),
    /// The unused second slot of a `long`/`double` entry.
    Phantom,
}
impl CpEntry {
    pub fn tag_name(&self) -> &'static str {
        match self {
            CpEntry::Utf8(_) => "Utf8",
            CpEntry::Integer(_) => "Integer",
            CpEntry::Float(_) => "Float",
            CpEntry::Long(_) => "Long",
            CpEntry::Double(_) => "Double",
            CpEntry::Class(_) => "Class",
            CpEntry::String(_) => "String",
            CpEntry::FieldRef { .. } => "Fieldref",
            CpEntry::MethodRef { .. } => "Methodref",
            CpEntry::InterfaceMethodRef { .. } => "InterfaceMethodref",
            CpEntry::NameAndType { .. } => "NameAndType",
            CpEntry::MethodHandle { .. } => "MethodHandle",
            CpEntry::MethodType(_) => "MethodType",
            CpEntry::Dynamic { .. } => "Dynamic",
            CpEntry::InvokeDynamic { .. } => "InvokeDynamic",
            CpEntry::Module(_) => "Module",
            CpEntry::Package(_) => "Package",
            CpEntry::Phantom => "(phantom)",
        }
    }
}
/// A resolved constant pool. Entry indices are 1-based as in the class file;
/// slot 0 is always `None`.
#[derive(Clone, Debug, Default)]
pub struct ConstantPool {
    entries: Vec<Option<CpEntry>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub message: String,
    pub offset: usize,
}
impl Error {
    fn new(offset: usize, message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            offset,
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "class file error at byte {}: {}",
            self.offset, self.message
        )
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;
impl ConstantPool {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entry(&self, idx: u16) -> Result<&CpEntry> {
        self.entries
            .get(idx as usize)
            .and_then(|e| e.as_ref())
            .ok_or_else(|| Error::new(0, format!("constant pool index {idx} out of range")))
    }

    pub fn utf8(&self, idx: u16) -> Result<&str> {
        match self.entry(idx)? {
            CpEntry::Utf8(s) => Ok(s),
            other => Err(Error::new(
                0,
                format!("constant #{idx} is {}, expected Utf8", other.tag_name()),
            )),
        }
    }

    pub fn class_name(&self, idx: u16) -> Result<&str> {
        match self.entry(idx)? {
            CpEntry::Class(name_idx) => self.utf8(*name_idx),
            other => Err(Error::new(
                0,
                format!("constant #{idx} is {}, expected Class", other.tag_name()),
            )),
        }
    }

    pub fn name_and_type(&self, idx: u16) -> Result<(String, String)> {
        match self.entry(idx)? {
            CpEntry::NameAndType { name, descriptor } => Ok((
                self.utf8(*name)?.to_string(),
                self.utf8(*descriptor)?.to_string(),
            )),
            other => Err(Error::new(
                0,
                format!(
                    "constant #{idx} is {}, expected NameAndType",
                    other.tag_name()
                ),
            )),
        }
    }

    /// Resolves a `FieldRef`/`MethodRef`/`InterfaceMethodRef` to
    /// `(class, name, descriptor)`.
    pub fn member_ref(&self, idx: u16) -> Result<(String, String, String)> {
        let (class_idx, nat_idx) = match self.entry(idx)? {
            CpEntry::FieldRef {
                class,
                name_and_type,
            }
            | CpEntry::MethodRef {
                class,
                name_and_type,
            }
            | CpEntry::InterfaceMethodRef {
                class,
                name_and_type,
            } => (*class, *name_and_type),
            other => {
                return Err(Error::new(
                    0,
                    format!(
                        "constant #{idx} is {}, expected a member reference",
                        other.tag_name()
                    ),
                ))
            }
        };
        let class = self.class_name(class_idx)?.to_string();
        let (name, desc) = self.name_and_type(nat_idx)?;
        Ok((class, name, desc))
    }

    /// Resolves a `String` constant to its text.
    pub fn string(&self, idx: u16) -> Result<String> {
        match self.entry(idx)? {
            CpEntry::String(utf8_idx) => Ok(self.utf8(*utf8_idx)?.to_string()),
            other => Err(Error::new(
                0,
                format!("constant #{idx} is {}, expected String", other.tag_name()),
            )),
        }
    }

    pub fn integer(&self, idx: u16) -> Result<i32> {
        match self.entry(idx)? {
            CpEntry::Integer(v) => Ok(*v),
            other => Err(Error::new(
                0,
                format!("constant #{idx} is {}, expected Integer", other.tag_name()),
            )),
        }
    }

    pub fn float(&self, idx: u16) -> Result<f32> {
        match self.entry(idx)? {
            CpEntry::Float(v) => Ok(*v),
            other => Err(Error::new(
                0,
                format!("constant #{idx} is {}, expected Float", other.tag_name()),
            )),
        }
    }

    pub fn long(&self, idx: u16) -> Result<i64> {
        match self.entry(idx)? {
            CpEntry::Long(v) => Ok(*v),
            other => Err(Error::new(
                0,
                format!("constant #{idx} is {}, expected Long", other.tag_name()),
            )),
        }
    }

    pub fn double(&self, idx: u16) -> Result<f64> {
        match self.entry(idx)? {
            CpEntry::Double(v) => Ok(*v),
            other => Err(Error::new(
                0,
                format!("constant #{idx} is {}, expected Double", other.tag_name()),
            )),
        }
    }

    /// Resolves an `InvokeDynamic`/`Dynamic` entry to
    /// `(bootstrap_index, name, descriptor)`.
    pub fn dynamic(&self, idx: u16) -> Result<(u16, String, String)> {
        let (bsm, nat) = match self.entry(idx)? {
            CpEntry::InvokeDynamic {
                bootstrap,
                name_and_type,
            }
            | CpEntry::Dynamic {
                bootstrap,
                name_and_type,
            } => (*bootstrap, *name_and_type),
            other => {
                return Err(Error::new(
                    0,
                    format!(
                        "constant #{idx} is {}, expected InvokeDynamic",
                        other.tag_name()
                    ),
                ))
            }
        };
        let (name, desc) = self.name_and_type(nat)?;
        Ok((bsm, name, desc))
    }
}

// --- scanaislop split: the items below live verbatim in the child modules ---
#[path = "lib/classfile.rs"]
mod classfile;
#[path = "lib/parse.rs"]
mod parse;
#[path = "lib/structs.rs"]
mod structs;
#[cfg(test)]
#[path = "lib/testclass.rs"]
mod testclass;
#[cfg(test)]
#[path = "lib/tests.rs"]
mod tests;
pub use classfile::ClassFile;
pub(crate) use classfile::*;
pub(crate) use parse::*;
pub use structs::BootstrapArgument;
pub use structs::BootstrapMethod;
pub use structs::CodeAttribute;
pub use structs::ConstantValue;
pub use structs::ExceptionEntry;
pub use structs::FieldInfo;
pub use structs::LocalVarType;
pub use structs::MethodInfo;
