use super::*;

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
    /// `Signature` attribute (JVMS §4.7.9): the class's generic signature
    /// (type parameters, superclass and superinterface type arguments),
    /// unparsed. `None` for non-generic classes.
    pub signature: Option<String>,
    /// `NestHost` attribute (JVMS §4.7.28): the host class named by the
    /// attribute, `None` when absent (a top-level class IS its own nest
    /// host). JDK 11+ private-member access is NEST-based, not
    /// package-based (JEP 181) — the different-package private-ctor
    /// IllegalAccessException law resolves both sides' hosts and
    /// compares (areNestMates), so this attribute is the authoritative
    /// fact the check keys on.
    pub nest_host: Option<String>,
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
        // Class-level attributes: BootstrapMethods (invokedynamic),
        // InnerClasses (reflection simple names, M3_PLAN §8.1) and
        // Signature (the generic-signature pin, G3.3); the rest are
        // skipped by length per the class-file format.
        let mut bootstrap_methods = Vec::new();
        let mut inner_name: Option<String> = None;
        let mut signature: Option<String> = None;
        let mut nest_host: Option<String> = None;
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
                "Signature" => {
                    // JVMS §4.7.9: exactly one u2, the signature_index.
                    if len != 2 {
                        return Err(Error::new(
                            r.pos,
                            format!("Signature.attribute_length must be 2, got {len}"),
                        ));
                    }
                    let idx = r.u2()?;
                    signature = Some(constant_pool.utf8(idx)?.to_string());
                }
                "NestHost" => {
                    // JVMS §4.7.28: exactly one u2, the host_class_index.
                    if len != 2 {
                        return Err(Error::new(
                            r.pos,
                            format!("NestHost.attribute_length must be 2, got {len}"),
                        ));
                    }
                    let idx = r.u2()?;
                    nest_host = Some(constant_pool.class_name(idx)?.to_string());
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
            signature,
            nest_host,
        })
    }
}
pub(crate) struct Reader<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8]> {
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

    pub(crate) fn u1(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u2(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    pub(crate) fn u4(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.take(n)
    }

    pub(crate) fn skip(&mut self, n: usize) -> Result<()> {
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
