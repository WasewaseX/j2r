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

/// One entry of a method's exception table.
#[derive(Clone, Debug)]
pub struct ExceptionEntry {
    pub start_pc: u16,
    pub end_pc: u16,
    pub handler_pc: u16,
    /// `None` = catch-all (a `finally` block).
    pub catch_type: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CodeAttribute {
    pub max_stack: u16,
    pub max_locals: u16,
    pub code: Vec<u8>,
    pub exception_table: Vec<ExceptionEntry>,
}

#[derive(Clone, Debug)]
pub struct FieldInfo {
    pub access: u16,
    pub name: String,
    pub descriptor: String,
    /// `ConstantValue` for static finals, if present.
    pub constant_value: Option<ConstantValue>,
}

impl FieldInfo {
    pub fn is_static(&self) -> bool {
        self.access & flags::ACC_STATIC != 0
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

#[derive(Clone, Debug)]
pub struct ClassFile {
    pub minor_version: u16,
    pub major_version: u16,
    pub constant_pool: ConstantPool,
    pub access: u16,
    /// Internal form `a/b/C`.
    pub this_class: String,
    /// Internal form, `None` for `java/lang/Object`.
    pub super_class: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
    pub bootstrap_methods: Vec<BootstrapMethod>,
    /// `InnerClasses` attribute, entry whose `inner_class_info` equals
    /// `this_class`: the raw `inner_name` (M3_PLAN §8.1). `None` when the
    /// attribute is absent (top-level classes compiled without nesting
    /// metadata) or this class has no entry of its own; `Some("")` marks an
    /// anonymous class. `Class.getSimpleName` uses this verbatim.
    pub inner_name: Option<String>,
}

impl ClassFile {
    /// Every class named by a `CONSTANT_Class` entry in the pool — the
    /// supertypes, interfaces, descriptor types, `new`/`checkcast`/
    /// `instanceof`/array targets. This is the over-approximate set the
    /// M3 classpath closure loader walks (M3_PLAN §4.2); array entries
    /// (`[…`) are returned verbatim and decoded by the caller.
    pub fn referenced_classes(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for e in self.constant_pool.entries.iter().flatten() {
            let CpEntry::Class(name_idx) = e else {
                continue;
            };
            // `name_idx` is the Utf8 entry holding the internal name
            // (`a/b/C`, or an array descriptor like `[La/b/C;`).
            let name = self.constant_pool.utf8(*name_idx)?.to_string();
            out.push(name);
        }
        Ok(out)
    }

    pub fn method(&self, name: &str, descriptor: &str) -> Option<&MethodInfo> {
        self.methods
            .iter()
            .find(|m| m.name == name && m.descriptor == descriptor)
    }

    pub fn field(&self, name: &str) -> Option<&FieldInfo> {
        self.fields.iter().find(|f| f.name == name)
    }

    pub fn is_interface(&self) -> bool {
        self.access & flags::ACC_INTERFACE != 0
    }

    pub fn is_abstract(&self) -> bool {
        self.access & flags::ACC_ABSTRACT != 0
    }

    /// Parses a class file from raw bytes.
    pub fn parse(bytes: &[u8]) -> Result<ClassFile> {
        let mut r = Reader { bytes, pos: 0 };
        r.expect_magic()?;
        let minor_version = r.u2()?;
        let major_version = r.u2()?;
        let constant_pool = parse_constant_pool(&mut r)?;
        let access = r.u2()?;
        let this_class = {
            let idx = r.u2()?;
            constant_pool.class_name(idx)?.to_string()
        };
        let super_class = {
            let idx = r.u2()?;
            if idx == 0 {
                None
            } else {
                Some(constant_pool.class_name(idx)?.to_string())
            }
        };
        let interface_count = r.u2()?;
        let mut interfaces = Vec::with_capacity(interface_count as usize);
        for _ in 0..interface_count {
            let idx = r.u2()?;
            interfaces.push(constant_pool.class_name(idx)?.to_string());
        }
        let fields = parse_fields(&mut r, &constant_pool)?;
        let methods = parse_methods(&mut r, &constant_pool)?;
        // Class-level attributes: BootstrapMethods (invokedynamic) and
        // InnerClasses (reflection simple names, M3_PLAN §8.1); the rest are
        // skipped by length per the class-file format.
        let mut bootstrap_methods = Vec::new();
        let mut inner_name: Option<String> = None;
        let attr_count = r.u2()?;
        for _ in 0..attr_count {
            let name_idx = r.u2()?;
            let len = r.u4()?;
            match constant_pool.utf8(name_idx)? {
                "BootstrapMethods" => {
                    let body = r.bytes(len as usize)?;
                    bootstrap_methods = parse_bootstrap_methods(body, &constant_pool)?;
                }
                "InnerClasses" => {
                    let body = r.bytes(len as usize)?;
                    inner_name = parse_inner_classes(body, &constant_pool, &this_class)?;
                }
                _ => {
                    r.skip(len as usize)?;
                }
            }
        }
        r.expect_end()?;
        Ok(ClassFile {
            minor_version,
            major_version,
            constant_pool,
            access,
            this_class,
            super_class,
            interfaces,
            fields,
            methods,
            bootstrap_methods,
            inner_name,
        })
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.pos + n > self.bytes.len() {
            return Err(Error::new(
                self.pos,
                format!("unexpected end of file (needed {n} bytes)"),
            ));
        }
        let out = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn u1(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u2(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn u4(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.take(n)
    }

    fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    fn expect_magic(&mut self) -> Result<()> {
        let magic = self.u4()?;
        if magic != 0xCAFEBABE {
            return Err(Error::new(
                0,
                format!("bad magic 0x{magic:08X}, expected 0xCAFEBABE — is this a .class file?"),
            ));
        }
        Ok(())
    }

    fn expect_end(&mut self) -> Result<()> {
        if self.pos != self.bytes.len() {
            return Err(Error::new(
                self.pos,
                format!(
                    "{} trailing bytes after class file",
                    self.bytes.len() - self.pos
                ),
            ));
        }
        Ok(())
    }
}

fn parse_constant_pool(r: &mut Reader<'_>) -> Result<ConstantPool> {
    let count = r.u2()? as usize;
    let mut entries: Vec<Option<CpEntry>> = vec![None; count.max(1)];
    let mut i = 1usize;
    while i < count {
        let at = r.pos;
        let tag = r.u1()?;
        // Long/Double occupy two constant-pool slots (JVMS §4.4.5).
        let two_slots = matches!(tag, 5 | 6);
        if two_slots && i + 1 >= count {
            return Err(Error::new(
                at,
                if tag == 5 {
                    "Long constant ends the constant pool"
                } else {
                    "Double constant ends the constant pool"
                },
            ));
        }
        let entry = match tag {
            1 => {
                let len = r.u2()? as usize;
                let bytes = r.bytes(len)?;
                match decode_modified_utf8(bytes) {
                    Some(s) => CpEntry::Utf8(s),
                    None => return Err(Error::new(at, "invalid modified UTF-8 in Utf8 constant")),
                }
            }
            3 => CpEntry::Integer(i32::from_be_bytes(r.bytes(4)?.try_into().unwrap())),
            4 => CpEntry::Float(f32::from_bits(u32::from_be_bytes(
                r.bytes(4)?.try_into().unwrap(),
            ))),
            5 => CpEntry::Long(i64::from_be_bytes(r.bytes(8)?.try_into().unwrap())),
            6 => CpEntry::Double(f64::from_bits(u64::from_be_bytes(
                r.bytes(8)?.try_into().unwrap(),
            ))),
            7 => CpEntry::Class(r.u2()?),
            8 => CpEntry::String(r.u2()?),
            9 => {
                let class = r.u2()?;
                let nat = r.u2()?;
                CpEntry::FieldRef {
                    class,
                    name_and_type: nat,
                }
            }
            10 => {
                let class = r.u2()?;
                let nat = r.u2()?;
                CpEntry::MethodRef {
                    class,
                    name_and_type: nat,
                }
            }
            11 => {
                let class = r.u2()?;
                let nat = r.u2()?;
                CpEntry::InterfaceMethodRef {
                    class,
                    name_and_type: nat,
                }
            }
            12 => {
                let name = r.u2()?;
                let desc = r.u2()?;
                CpEntry::NameAndType {
                    name,
                    descriptor: desc,
                }
            }
            15 => {
                let kind = r.u1()?;
                let index = r.u2()?;
                match kind {
                    1..=9 => {}
                    _ => return Err(Error::new(at, format!("bad MethodHandle kind {kind}"))),
                }
                CpEntry::MethodHandle { kind, index }
            }
            16 => CpEntry::MethodType(r.u2()?),
            17 => {
                let bsm = r.u2()?;
                let nat = r.u2()?;
                CpEntry::Dynamic {
                    bootstrap: bsm,
                    name_and_type: nat,
                }
            }
            18 => {
                let bsm = r.u2()?;
                let nat = r.u2()?;
                CpEntry::InvokeDynamic {
                    bootstrap: bsm,
                    name_and_type: nat,
                }
            }
            19 => CpEntry::Module(r.u2()?),
            20 => CpEntry::Package(r.u2()?),
            other => return Err(Error::new(at, format!("unknown constant pool tag {other}"))),
        };
        entries[i] = Some(entry);
        if two_slots {
            entries[i + 1] = Some(CpEntry::Phantom);
            i += 2;
        } else {
            i += 1;
        }
    }
    Ok(ConstantPool { entries })
}

/// Decodes JVM *modified UTF-8* (JVMS §4.4.7): NUL as `C0 80` and
/// supplementary characters as surrogate pairs of 3-byte sequences.
fn decode_modified_utf8(bytes: &[u8]) -> Option<String> {
    if bytes.iter().all(|b| *b != 0 && *b < 0x80) {
        return Some(String::from_utf8_lossy(bytes).into_owned());
    }
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        let unit = if b & 0x80 == 0 {
            i += 1;
            b as u16
        } else if b & 0xE0 == 0xC0 {
            if i + 1 >= bytes.len() {
                return None;
            }
            let b2 = bytes[i + 1];
            if b2 & 0xC0 != 0x80 {
                return None;
            }
            i += 2;
            (((b & 0x1F) as u16) << 6) | (b2 & 0x3F) as u16
        } else if b & 0xF0 == 0xE0 {
            if i + 2 >= bytes.len() {
                return None;
            }
            let (b2, b3) = (bytes[i + 1], bytes[i + 2]);
            if b2 & 0xC0 != 0x80 || b3 & 0xC0 != 0x80 {
                return None;
            }
            i += 3;
            (((b & 0x0F) as u16) << 12) | (((b2 & 0x3F) as u16) << 6) | (b3 & 0x3F) as u16
        } else {
            return None;
        };
        units.push(unit);
    }
    String::from_utf16(&units).ok()
}

fn parse_fields(r: &mut Reader<'_>, cp: &ConstantPool) -> Result<Vec<FieldInfo>> {
    let count = r.u2()?;
    let mut fields = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let access = r.u2()?;
        let name = cp.utf8(r.u2()?)?.to_string();
        let descriptor = cp.utf8(r.u2()?)?.to_string();
        let mut constant_value = None;
        let attr_count = r.u2()?;
        for _ in 0..attr_count {
            let attr_name = cp.utf8(r.u2()?)?.to_string();
            let len = r.u4()?;
            if attr_name == "ConstantValue" {
                let idx = r.u2()?;
                constant_value = Some(match cp.entry(idx)? {
                    CpEntry::Integer(v) => ConstantValue::Int(*v),
                    CpEntry::Float(v) => ConstantValue::Float(*v),
                    CpEntry::Long(v) => ConstantValue::Long(*v),
                    CpEntry::Double(v) => ConstantValue::Double(*v),
                    CpEntry::String(u) => ConstantValue::String(cp.utf8(*u)?.to_string()),
                    other => {
                        return Err(Error::new(
                            r.pos,
                            format!("ConstantValue is {}, unexpected", other.tag_name()),
                        ))
                    }
                });
            } else {
                r.skip(len as usize)?;
            }
        }
        fields.push(FieldInfo {
            access,
            name,
            descriptor,
            constant_value,
        });
    }
    Ok(fields)
}

fn parse_methods(r: &mut Reader<'_>, cp: &ConstantPool) -> Result<Vec<MethodInfo>> {
    let count = r.u2()?;
    let mut methods = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let access = r.u2()?;
        let name = cp.utf8(r.u2()?)?.to_string();
        let descriptor = cp.utf8(r.u2()?)?.to_string();
        let mut code = None;
        let attr_count = r.u2()?;
        for _ in 0..attr_count {
            let attr_name = cp.utf8(r.u2()?)?.to_string();
            let len = r.u4()?;
            if attr_name == "Code" {
                let body = r.bytes(len as usize)?;
                code = Some(parse_code(body, cp)?);
            } else {
                r.skip(len as usize)?;
            }
        }
        methods.push(MethodInfo {
            access,
            name,
            descriptor,
            code,
        });
    }
    Ok(methods)
}

fn parse_code(body: &[u8], cp: &ConstantPool) -> Result<CodeAttribute> {
    let mut r = Reader {
        bytes: body,
        pos: 0,
    };
    let max_stack = r.u2()?;
    let max_locals = r.u2()?;
    let code_len = r.u4()? as usize;
    let code = r.bytes(code_len)?.to_vec();
    let ex_count = r.u2()?;
    let mut exception_table = Vec::with_capacity(ex_count as usize);
    for _ in 0..ex_count {
        let start_pc = r.u2()?;
        let end_pc = r.u2()?;
        let handler_pc = r.u2()?;
        let catch_idx = r.u2()?;
        let catch_type = if catch_idx == 0 {
            None
        } else {
            Some(cp.class_name(catch_idx)?.to_string())
        };
        exception_table.push(ExceptionEntry {
            start_pc,
            end_pc,
            handler_pc,
            catch_type,
        });
    }
    // Remaining attributes of Code (LineNumberTable, StackMapTable, ...) are
    // skipped by length — the class-file format allows this.
    Ok(CodeAttribute {
        max_stack,
        max_locals,
        code,
        exception_table,
    })
}

/// Parses an `InnerClasses` attribute and returns the `inner_name` of the
/// entry matching `this_class` (JVMS §4.7.6). `None` when no entry matches
/// or the matching entry has `inner_name_index == 0` (anonymous class —
/// reported as `Some("")` by callers is NOT done here: index 0 yields
/// `Some(String::new())` so anonymous and absent stay distinguishable).
fn parse_inner_classes(body: &[u8], cp: &ConstantPool, this_class: &str) -> Result<Option<String>> {
    let mut r = Reader {
        bytes: body,
        pos: 0,
    };
    let count = r.u2()?;
    let mut found = None;
    for _ in 0..count {
        let inner_idx = r.u2()?;
        let _outer_idx = r.u2()?;
        let name_idx = r.u2()?;
        let _access = r.u2()?;
        let inner = cp.class_name(inner_idx)?;
        if inner == this_class && found.is_none() {
            found = Some(if name_idx == 0 {
                String::new()
            } else {
                cp.utf8(name_idx)?.to_string()
            });
        }
    }
    Ok(found)
}

fn parse_bootstrap_methods(body: &[u8], cp: &ConstantPool) -> Result<Vec<BootstrapMethod>> {
    let mut r = Reader {
        bytes: body,
        pos: 0,
    };
    let count = r.u2()?;
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let method_handle = r.u2()?;
        let arg_count = r.u2()?;
        let mut arguments = Vec::with_capacity(arg_count as usize);
        for _ in 0..arg_count {
            let idx = r.u2()?;
            arguments.push(match cp.entry(idx)? {
                CpEntry::Integer(v) => BootstrapArgument::Int(*v),
                CpEntry::Long(v) => BootstrapArgument::Long(*v),
                CpEntry::Float(v) => BootstrapArgument::Float(*v),
                CpEntry::Double(v) => BootstrapArgument::Double(*v),
                CpEntry::String(u) => BootstrapArgument::String(cp.utf8(*u)?.to_string()),
                CpEntry::MethodType(u) => BootstrapArgument::MethodType(cp.utf8(*u)?.to_string()),
                CpEntry::Class(_) => BootstrapArgument::Class(cp.class_name(idx)?.to_string()),
                CpEntry::MethodHandle { kind, index } => {
                    let (cls, name, desc) = cp.member_ref(*index)?;
                    BootstrapArgument::MethodHandle {
                        kind: *kind,
                        class: cls,
                        name,
                        desc,
                    }
                }
                other => BootstrapArgument::Opaque {
                    tag: other.tag_name(),
                },
            });
        }
        out.push(BootstrapMethod {
            method_handle,
            arguments,
        });
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod testclass {
    //! Builder for hand-crafted class files used by tests (no javac needed).
    #![allow(dead_code, clippy::too_many_arguments)]

    use super::flags;

    pub struct Builder {
        pub pool: Vec<u8>,
        pub idx: u16,
    }

    impl Builder {
        pub fn new() -> Builder {
            Builder {
                pool: Vec::new(),
                idx: 1,
            }
        }

        pub fn utf8(&mut self, s: &str) -> u16 {
            self.pool.push(1);
            self.pool.extend_from_slice(&(s.len() as u16).to_be_bytes());
            self.pool.extend_from_slice(s.as_bytes());
            let i = self.idx;
            self.idx += 1;
            i
        }

        pub fn class(&mut self, name_idx: u16) -> u16 {
            self.pool.push(7);
            self.pool.extend_from_slice(&name_idx.to_be_bytes());
            let i = self.idx;
            self.idx += 1;
            i
        }

        pub fn string(&mut self, utf8_idx: u16) -> u16 {
            self.pool.push(8);
            self.pool.extend_from_slice(&utf8_idx.to_be_bytes());
            let i = self.idx;
            self.idx += 1;
            i
        }

        pub fn integer(&mut self, v: i32) -> u16 {
            self.pool.push(3);
            self.pool.extend_from_slice(&v.to_be_bytes());
            let i = self.idx;
            self.idx += 1;
            i
        }

        pub fn long(&mut self, v: i64) -> u16 {
            self.pool.push(5);
            self.pool.extend_from_slice(&v.to_be_bytes());
            let i = self.idx;
            self.idx += 2; // occupies two slots
            i
        }

        pub fn name_and_type(&mut self, name_idx: u16, desc_idx: u16) -> u16 {
            self.pool.push(12);
            self.pool.extend_from_slice(&name_idx.to_be_bytes());
            self.pool.extend_from_slice(&desc_idx.to_be_bytes());
            let i = self.idx;
            self.idx += 1;
            i
        }

        pub fn method_ref(&mut self, class_idx: u16, nat_idx: u16) -> u16 {
            self.pool.push(10);
            self.pool.extend_from_slice(&class_idx.to_be_bytes());
            self.pool.extend_from_slice(&nat_idx.to_be_bytes());
            let i = self.idx;
            self.idx += 1;
            i
        }

        pub fn field_ref(&mut self, class_idx: u16, nat_idx: u16) -> u16 {
            self.pool.push(9);
            self.pool.extend_from_slice(&class_idx.to_be_bytes());
            self.pool.extend_from_slice(&nat_idx.to_be_bytes());
            let i = self.idx;
            self.idx += 1;
            i
        }

        pub fn finish(
            self,
            access: u16,
            this_class: u16,
            super_class: u16,
            interfaces: &[u16],
            fields: &[&[u8]],
            methods: &[&[u8]],
            class_attrs: &[&[u8]],
        ) -> Vec<u8> {
            let count = self.idx;
            let mut b = Vec::new();
            b.extend_from_slice(&0xCAFEBABEu32.to_be_bytes());
            b.extend_from_slice(&0u16.to_be_bytes()); // minor
            b.extend_from_slice(&52u16.to_be_bytes()); // major (Java 8, no StackMapTable required)
            b.extend_from_slice(&count.to_be_bytes());
            b.extend_from_slice(&self.pool);
            b.extend_from_slice(&access.to_be_bytes());
            b.extend_from_slice(&this_class.to_be_bytes());
            b.extend_from_slice(&super_class.to_be_bytes());
            b.extend_from_slice(&(interfaces.len() as u16).to_be_bytes());
            for i in interfaces {
                b.extend_from_slice(&i.to_be_bytes());
            }
            b.extend_from_slice(&(fields.len() as u16).to_be_bytes());
            for f in fields {
                b.extend_from_slice(f);
            }
            b.extend_from_slice(&(methods.len() as u16).to_be_bytes());
            for m in methods {
                b.extend_from_slice(m);
            }
            b.extend_from_slice(&(class_attrs.len() as u16).to_be_bytes());
            for a in class_attrs {
                b.extend_from_slice(a);
            }
            b
        }
    }

    /// Encodes one field with an optional ConstantValue attribute.
    pub fn field(
        access: u16,
        name_idx: u16,
        desc_idx: u16,
        constant_value: Option<u16>,
        constvalue_name_idx: u16,
    ) -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(&access.to_be_bytes());
        f.extend_from_slice(&name_idx.to_be_bytes());
        f.extend_from_slice(&desc_idx.to_be_bytes());
        match constant_value {
            Some(cv) => {
                f.extend_from_slice(&1u16.to_be_bytes());
                f.extend_from_slice(&constvalue_name_idx.to_be_bytes());
                f.extend_from_slice(&2u32.to_be_bytes()); // u4 attribute length
                f.extend_from_slice(&cv.to_be_bytes());
            }
            None => f.extend_from_slice(&0u16.to_be_bytes()),
        }
        f
    }

    /// Encodes one method with a Code attribute.
    pub fn method(
        access: u16,
        name_idx: u16,
        desc_idx: u16,
        max_stack: u16,
        max_locals: u16,
        code: &[u8],
        exception_table: &[(u16, u16, u16, u16)], // start, end, handler, catch_class_idx (0 = any)
        code_attr_name_idx: u16,
    ) -> Vec<u8> {
        let mut ex = Vec::new();
        for (s, e, h, c) in exception_table {
            ex.extend_from_slice(&s.to_be_bytes());
            ex.extend_from_slice(&e.to_be_bytes());
            ex.extend_from_slice(&h.to_be_bytes());
            ex.extend_from_slice(&c.to_be_bytes());
        }
        let mut body = Vec::new();
        body.extend_from_slice(&max_stack.to_be_bytes());
        body.extend_from_slice(&max_locals.to_be_bytes());
        body.extend_from_slice(&(code.len() as u32).to_be_bytes());
        body.extend_from_slice(code);
        body.extend_from_slice(&(exception_table.len() as u16).to_be_bytes());
        body.extend_from_slice(&ex);
        body.extend_from_slice(&0u16.to_be_bytes()); // no Code sub-attributes

        let mut m = Vec::new();
        m.extend_from_slice(&access.to_be_bytes());
        m.extend_from_slice(&name_idx.to_be_bytes());
        m.extend_from_slice(&desc_idx.to_be_bytes());
        m.extend_from_slice(&1u16.to_be_bytes()); // one attribute
        m.extend_from_slice(&code_attr_name_idx.to_be_bytes());
        m.extend_from_slice(&(body.len() as u32).to_be_bytes());
        m.extend_from_slice(&body);
        m
    }

    pub fn access_public_static() -> u16 {
        flags::ACC_PUBLIC | flags::ACC_STATIC
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testclass::Builder;

    #[test]
    fn rejects_bad_magic() {
        let err = ClassFile::parse(&[1, 2, 3, 4]).unwrap_err();
        assert!(err.message.contains("magic"));
    }

    #[test]
    fn rejects_trailing_garbage() {
        let mut b = Builder::new();
        let name = b.utf8("T");
        let cls = b.class(name);
        let obj_name = b.utf8("java/lang/Object");
        let obj_cls = b.class(obj_name);
        let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[], &[], &[]);
        let mut bad = bytes.clone();
        bad.push(0xFF);
        let err = ClassFile::parse(&bad).unwrap_err();
        assert!(err.message.contains("trailing"));
        // and the clean version parses
        let cf = ClassFile::parse(&bytes).unwrap();
        assert_eq!(cf.this_class, "T");
        assert_eq!(cf.super_class.as_deref(), Some("java/lang/Object"));
    }

    #[test]
    fn long_takes_two_pool_slots() {
        let mut b = Builder::new();
        let name = b.utf8("T");
        let cls = b.class(name);
        let obj_name = b.utf8("java/lang/Object");
        let obj_cls = b.class(obj_name);
        let field_name = b.utf8("BIG");
        let field_desc = b.utf8("J");
        let long_idx = b.long(0x7FFF_FFFF_FFFF_FFFF);
        let cv_name = b.utf8("ConstantValue");
        let field = testclass::field(
            flags::ACC_PUBLIC | flags::ACC_STATIC | flags::ACC_FINAL,
            field_name,
            field_desc,
            Some(long_idx),
            cv_name,
        );
        let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[&field], &[], &[]);
        let cf = ClassFile::parse(&bytes).unwrap();
        let f = cf.field("BIG").unwrap();
        assert_eq!(f.descriptor, "J");
        assert!(f.is_static());
        assert_eq!(
            f.constant_value,
            Some(ConstantValue::Long(0x7FFF_FFFF_FFFF_FFFF))
        );
        assert_eq!(
            cf.constant_pool.long(long_idx).unwrap(),
            0x7FFF_FFFF_FFFF_FFFF
        );
    }

    #[test]
    fn methods_with_code_and_exceptions_parse() {
        let mut b = Builder::new();
        let name = b.utf8("T");
        let cls = b.class(name);
        let obj_name = b.utf8("java/lang/Object");
        let obj_cls = b.class(obj_name);
        let m_name = b.utf8("run");
        let m_desc = b.utf8("()V");
        let code_name = b.utf8("Code");
        let exc_name = b.utf8("java/lang/RuntimeException");
        let exc_cls = b.class(exc_name);
        let m = testclass::method(
            flags::ACC_PUBLIC,
            m_name,
            m_desc,
            2,
            1,
            &[0xB1], // return
            &[(0, 1, 1, exc_cls)],
            code_name,
        );
        let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[], &[&m], &[]);
        let cf = ClassFile::parse(&bytes).unwrap();
        let m = cf.method("run", "()V").unwrap();
        let code = m.code().unwrap();
        assert_eq!(code.code, vec![0xB1]);
        assert_eq!(code.max_stack, 2);
        assert_eq!(code.exception_table.len(), 1);
        assert_eq!(
            code.exception_table[0].catch_type.as_deref(),
            Some("java/lang/RuntimeException")
        );
    }

    #[test]
    fn modified_utf8_nul_decodes() {
        let mut b = Builder::new();
        // raw Utf8 entry with modified-UTF-8 NUL (0xC0 0x80): "a\u{0}" is 3 bytes
        b.pool.push(1);
        b.pool.extend_from_slice(&3u16.to_be_bytes());
        b.pool.extend_from_slice(&[b'a', 0xC0, 0x80]);
        let idx = b.idx;
        b.idx += 1;
        let cls = b.class(idx);
        let obj_name = b.utf8("java/lang/Object");
        let obj_cls = b.class(obj_name);
        let bytes = b.finish(flags::ACC_PUBLIC, cls, obj_cls, &[], &[], &[], &[]);
        let cf = ClassFile::parse(&bytes).unwrap();
        assert_eq!(cf.this_class, "a\u{0}");
    }
}
