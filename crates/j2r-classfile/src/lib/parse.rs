use super::*;

pub(crate) fn parse_constant_pool(r: &mut Reader<'_>) -> Result<ConstantPool> {
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
pub(crate) fn decode_modified_utf8(bytes: &[u8]) -> Option<String> {
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
    // Lossy, not `String::from_utf16` (which REJECTED the whole constant):
    // modified UTF-8 legally encodes unpaired surrogates (JVMS 4.4.7 — a
    // javac string literal like "\uD834" is a 3-byte CESU-8 sequence), and
    // rejecting them failed the entire classfile load (found by
    // corpus/Utf16Probe.java). The j2r string model is Rust String, which
    // cannot hold a lone surrogate, so each unpaired surrogate normalizes
    // to U+FFFD — the same substitution the JDK's encoders emit for one,
    // so every observable path (getBytes/print) still matches Temurin.
    Some(String::from_utf16_lossy(&units))
}
pub(crate) fn parse_fields(r: &mut Reader<'_>, cp: &ConstantPool) -> Result<Vec<FieldInfo>> {
    let count = r.u2()?;
    let mut fields = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let access = r.u2()?;
        let name = cp.utf8(r.u2()?)?.to_string();
        let descriptor = cp.utf8(r.u2()?)?.to_string();
        let mut constant_value = None;
        let mut signature = None;
        let attr_count = r.u2()?;
        for _ in 0..attr_count {
            let attr_name = cp.utf8(r.u2()?)?.to_string();
            let len = r.u4()?;
            if attr_name == "ConstantValue" {
                // JVMS §4.7.16: the attribute length must be exactly 2.
                // Anything else shifts the reader onto the next attribute
                // and misparses silently — reject with a precise offset
                // (audit A4).
                if len != 2 {
                    return Err(Error::new(
                        r.pos,
                        format!("ConstantValue.attribute_length must be 2, got {len}"),
                    ));
                }
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
            } else if attr_name == "Signature" {
                // JVMS §4.7.9: exactly one u2, the signature_index (same
                // strict-length law as ConstantValue, audit A4's lesson).
                if len != 2 {
                    return Err(Error::new(
                        r.pos,
                        format!("Signature.attribute_length must be 2, got {len}"),
                    ));
                }
                let idx = r.u2()?;
                signature = Some(cp.utf8(idx)?.to_string());
            } else {
                r.skip(len as usize)?;
            }
        }
        fields.push(FieldInfo {
            access,
            name,
            descriptor,
            constant_value,
            signature,
        });
    }
    Ok(fields)
}
pub(crate) fn parse_methods(r: &mut Reader<'_>, cp: &ConstantPool) -> Result<Vec<MethodInfo>> {
    let count = r.u2()?;
    let mut methods = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let access = r.u2()?;
        let name = cp.utf8(r.u2()?)?.to_string();
        let descriptor = cp.utf8(r.u2()?)?.to_string();
        let mut code = None;
        let mut signature = None;
        let attr_count = r.u2()?;
        for _ in 0..attr_count {
            let attr_name = cp.utf8(r.u2()?)?.to_string();
            let len = r.u4()?;
            if attr_name == "Code" {
                let body = r.bytes(len as usize)?;
                code = Some(parse_code(body, cp)?);
            } else if attr_name == "Signature" {
                if len != 2 {
                    return Err(Error::new(
                        r.pos,
                        format!("Signature.attribute_length must be 2, got {len}"),
                    ));
                }
                let idx = r.u2()?;
                signature = Some(cp.utf8(idx)?.to_string());
            } else {
                r.skip(len as usize)?;
            }
        }
        methods.push(MethodInfo {
            access,
            name,
            descriptor,
            code,
            signature,
        });
    }
    Ok(methods)
}
pub(crate) fn parse_code(body: &[u8], cp: &ConstantPool) -> Result<CodeAttribute> {
    let mut r = Reader {
        bytes: body,
        pos: 0,
    };
    let max_stack = r.u2()?;
    let max_locals = r.u2()?;
    let code_len = r.u4()? as usize;
    // JVMS §4.7.3: 0 < code_length < 65536. Enforce the bound here so a
    // malformed length is a precise error instead of an allocation-size
    // surprise in the byte read below (audit A4).
    if code_len == 0 || code_len >= 65536 {
        return Err(Error::new(
            r.pos,
            format!("code_length must be in 1..=65535, got {code_len}"),
        ));
    }
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
    // Walk the Code sub-attributes (LineNumberTable, StackMapTable, ...)
    // explicitly (JVMS §4.7.3: attributes_count + attribute_info entries),
    // validating each name index and length instead of trusting the
    // enclosing slice — a truncated or padded Code body is now a precise
    // error, and the pool walk catches garbage name indices (audit A4).
    // LocalVariableTypeTable (JVMS §4.7.14) is kept: the G3.3
    // signature-pin layer reads the generic types of locals from it.
    let sub_count = r.u2()?;
    let mut lvtt = Vec::new();
    for _ in 0..sub_count {
        let sub_name_idx = r.u2()?;
        let sub_len = r.u4()? as usize;
        let sub_name = cp.utf8(sub_name_idx)?;
        if sub_name == "LocalVariableTypeTable" {
            let body = r.bytes(sub_len)?;
            lvtt = parse_lvtt(body, cp)?;
        } else {
            r.skip(sub_len)?;
        }
    }
    if r.pos != body.len() {
        return Err(Error::new(
            r.pos,
            format!(
                "Code attribute has {} unread trailing byte(s) after its sub-attributes",
                body.len() - r.pos
            ),
        ));
    }
    Ok(CodeAttribute {
        max_stack,
        max_locals,
        code,
        exception_table,
        lvtt,
    })
}
/// Parses a `LocalVariableTypeTable` body (JVMS §4.7.14): u2 count, then
/// per entry start_pc u2, length u2, name_index u2, signature_index u2,
/// index u2. A truncated body is a precise error, same law as the
/// enclosing Code walk (audit A4).
pub(crate) fn parse_lvtt(body: &[u8], cp: &ConstantPool) -> Result<Vec<LocalVarType>> {
    let mut r = Reader {
        bytes: body,
        pos: 0,
    };
    let count = r.u2()?;
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let start_pc = r.u2()?;
        let length = r.u2()?;
        let name_idx = r.u2()?;
        let sig_idx = r.u2()?;
        let index = r.u2()?;
        out.push(LocalVarType {
            start_pc,
            length,
            name: cp.utf8(name_idx)?.to_string(),
            signature: cp.utf8(sig_idx)?.to_string(),
            index,
        });
    }
    if r.pos != body.len() {
        return Err(Error::new(
            r.pos,
            format!(
                "LocalVariableTypeTable has {} unread trailing byte(s)",
                body.len() - r.pos
            ),
        ));
    }
    Ok(out)
}
/// Parses an `InnerClasses` attribute and returns the `inner_name` of the
/// entry matching `this_class` (JVMS §4.7.6). `None` when no entry matches
/// or the matching entry has `inner_name_index == 0` (anonymous class —
/// reported as `Some("")` by callers is NOT done here: index 0 yields
/// `Some(String::new())` so anonymous and absent stay distinguishable).
pub(crate) fn parse_inner_classes(
    body: &[u8],
    cp: &ConstantPool,
    this_class: &str,
) -> Result<Option<String>> {
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
pub(crate) fn parse_bootstrap_methods(
    body: &[u8],
    cp: &ConstantPool,
) -> Result<Vec<BootstrapMethod>> {
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
