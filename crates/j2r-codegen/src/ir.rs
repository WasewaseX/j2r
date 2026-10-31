//! JVM bytecode decoder: turns a `Code` attribute's raw bytes into a typed,
//! pc-annotated instruction list ([`Instr`]).
//!
//! The decoder is intentionally mechanical — no stack simulation: the typed
//! judgement happens in [`crate::verify`], which runs before any Rust is
//! emitted. What the decoder *does* preserve is operand-width information
//! the verifier needs: array loads/stores keep their element kind
//! ([`ElemKind`]), so e.g. `lastore` consuming an `int` is a compile-time
//! verification error, never a runtime type panic.

use j2r_classfile::ConstantPool;
use j2r_runtime::TypeDesc;

#[derive(Clone, Debug, PartialEq)]
pub enum Instr {
    Nop,
    AConstNull,
    IConst(i32),
    LConst(i64),
    FConst(f32),
    DConst(f64),
    Ldc(Ldc),
    Ldc2W(Ldc2),
    Load(u16),
    Store(u16),
    /// `xXaload`; carries the element kind the verifier checks against.
    ALoad(ElemKind),
    /// `xXastore`; carries the element kind the verifier checks against.
    AStore(ElemKind),
    ArrayLength,
    Pop,
    Pop2,
    Dup,
    DupX1,
    DupX2,
    Dup2,
    Dup2X1,
    Dup2X2,
    Swap,
    Arith {
        op: ArithOp,
        kind: NumKind,
    },
    Convert(Conv),
    IInc {
        idx: u16,
        c: i32,
    },
    Cmp(CmpKind),
    IfZero(CmpOp, u32),
    IfICmp(CmpOp, u32),
    IfRefCmp {
        eq: bool,
        target: u32,
    },
    IfNull(u32),
    IfNonNull(u32),
    Goto(u32),
    TableSwitch {
        default: u32,
        low: i32,
        targets: Vec<u32>,
    },
    LookupSwitch {
        default: u32,
        pairs: Vec<(i32, u32)>,
    },
    Return(TypeDesc),
    GetStatic {
        class: String,
        field: String,
        desc: String,
    },
    PutStatic {
        class: String,
        field: String,
        desc: String,
    },
    GetField {
        class: String,
        field: String,
        desc: String,
    },
    PutField {
        class: String,
        field: String,
        desc: String,
    },
    InvokeVirtual {
        class: String,
        name: String,
        desc: String,
    },
    InvokeSpecial {
        class: String,
        name: String,
        desc: String,
    },
    InvokeStatic {
        class: String,
        name: String,
        desc: String,
    },
    InvokeInterface {
        class: String,
        name: String,
        desc: String,
    },
    InvokeDynamic {
        index: u16,
    },
    New {
        class: String,
    },
    NewArray(PrimType),
    ANewArray {
        elem: String,
    },
    /// `multianewarray`: `desc` is the full array-class descriptor from the
    /// constant pool (`[[I`, `[[Ljava/lang/String;`), `dims` how many leading
    /// dimension sizes are consumed from the operand stack (outermost first).
    MultiNewArray {
        desc: String,
        dims: u8,
    },
    AThrow,
    CheckCast {
        class: String,
    },
    InstanceOf {
        class: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ldc {
    Int(i32),
    Float(f32),
    String(String),
    /// Reported as an explicit compile error by the code generator.
    Class(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ldc2 {
    Long(i64),
    Double(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Neg,
    Shl,
    Shr,
    UShr,
    And,
    Or,
    Xor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumKind {
    Int,
    Long,
    Float,
    Double,
    /// Only used as a conversion target (`i2b`, `i2c`, `i2s`).
    Byte,
    Char,
    Short,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Conv {
    pub from: NumKind,
    pub to: NumKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpKind {
    LCmp,
    FCmpL,
    FCmpG,
    DCmpL,
    DCmpG,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Ge,
    Gt,
    Le,
}

impl CmpOp {
    pub fn rust_op(self) -> &'static str {
        match self {
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Lt => "<",
            CmpOp::Ge => ">=",
            CmpOp::Gt => ">",
            CmpOp::Le => "<=",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimType {
    Boolean,
    Char,
    Float,
    Double,
    Byte,
    Short,
    Int,
    Long,
}

/// The element kind of an `xXaload`/`xXastore` opcode, as the verifier sees
/// it (`baload`/`caload`/`saload` operate on ints in this runtime).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElemKind {
    /// `iaload`, `baload`, `caload`, `saload` (and stores)
    IntGroup,
    Long,
    Float,
    Double,
    /// `aaload` / `aastore`
    Ref,
}

impl PrimType {
    pub fn array_desc(self) -> &'static str {
        match self {
            PrimType::Boolean => "[Z",
            PrimType::Char => "[C",
            PrimType::Float => "[F",
            PrimType::Double => "[D",
            PrimType::Byte => "[B",
            PrimType::Short => "[S",
            PrimType::Int => "[I",
            PrimType::Long => "[J",
        }
    }
}

/// A decoding failure with the faulting pc.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodeError {
    pub pc: u32,
    pub message: String,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pc {}: {}", self.pc, self.message)
    }
}

type DecodeResult<T> = std::result::Result<T, DecodeError>;

struct Decoder<'a> {
    code: &'a [u8],
    pos: usize,
    cp: &'a ConstantPool,
}

impl<'a> Decoder<'a> {
    fn u1(&mut self) -> DecodeResult<u8> {
        let b = *self
            .code
            .get(self.pos)
            .ok_or_else(|| self.err("unexpected end of code"))?;
        self.pos += 1;
        Ok(b)
    }

    fn u2(&mut self) -> DecodeResult<u16> {
        let a = self.u1()? as u16;
        let b = self.u1()? as u16;
        Ok((a << 8) | b)
    }

    fn s2(&mut self) -> DecodeResult<i16> {
        Ok(self.u2()? as i16)
    }

    fn s4(&mut self) -> DecodeResult<i32> {
        let b = [self.u1()?, self.u1()?, self.u1()?, self.u1()?];
        Ok(i32::from_be_bytes(b))
    }

    fn err(&self, msg: impl Into<String>) -> DecodeError {
        DecodeError {
            pc: self.pos as u32,
            message: msg.into(),
        }
    }

    fn member_ref(&mut self) -> DecodeResult<(String, String, String)> {
        let idx = self.u2()?;
        self.cp.member_ref(idx).map_err(|e| DecodeError {
            pc: self.pos as u32,
            message: e.message,
        })
    }

    fn class_at(&mut self, at: u32) -> DecodeResult<String> {
        let idx = self.u2()?;
        self.cp
            .class_name(idx)
            .map(|s| s.to_string())
            .map_err(|e| DecodeError {
                pc: at,
                message: e.message,
            })
    }
}

/// Decodes a method body into `(pc, instruction)` pairs, in code order.
pub fn decode(code: &[u8], cp: &ConstantPool) -> DecodeResult<Vec<(u32, Instr)>> {
    let mut d = Decoder { code, pos: 0, cp };
    let mut out = Vec::new();
    while d.pos < code.len() {
        let at = d.pos as u32;
        let op = d.u1()?;
        let instr = match op {
            0x00 => Instr::Nop,
            0x01 => Instr::AConstNull,
            0x02 => Instr::IConst(-1),
            0x03..=0x08 => Instr::IConst((op - 0x03) as i32),
            0x09 => Instr::LConst(0),
            0x0a => Instr::LConst(1),
            0x0b..=0x0d => Instr::FConst((op - 0x0b) as f32),
            0x0e => Instr::DConst(0.0),
            0x0f => Instr::DConst(1.0),
            0x10 => Instr::IConst(d.u1()? as i8 as i32),
            0x11 => Instr::IConst(d.s2()? as i32),
            0x12 => {
                let idx = d.u1()?;
                decode_ldc(cp, idx as u16)?
            }
            0x13 => {
                let idx = d.u2()?;
                decode_ldc(cp, idx)?
            }
            0x14 => {
                let idx = d.u2()?;
                decode_ldc2(cp, idx)?
            }
            0x15..=0x19 => Instr::Load(d.u1()? as u16),
            0x1a..=0x2d => Instr::Load(((op - 0x1a) % 4) as u16),
            0x2e..=0x35 => Instr::ALoad(elem_kind(op - 0x2e)?),
            0x36..=0x3a => Instr::Store(d.u1()? as u16),
            0x3b..=0x4e => Instr::Store(((op - 0x3b) % 4) as u16),
            0x4f..=0x56 => Instr::AStore(elem_kind(op - 0x4f)?),
            0x57 => Instr::Pop,
            0x58 => Instr::Pop2,
            0x59 => Instr::Dup,
            0x5a => Instr::DupX1,
            0x5b => Instr::DupX2,
            0x5c => Instr::Dup2,
            0x5d => Instr::Dup2X1,
            0x5e => Instr::Dup2X2,
            0x5f => Instr::Swap,
            0x60..=0x83 => explicit_arith(op)?,
            0x84 => {
                let idx = d.u1()? as u16;
                let c = d.u1()? as i8 as i32;
                Instr::IInc { idx, c }
            }
            0x85..=0x93 => explicit_convert(op)?,
            0x94 => Instr::Cmp(CmpKind::LCmp),
            0x95 => Instr::Cmp(CmpKind::FCmpL),
            0x96 => Instr::Cmp(CmpKind::FCmpG),
            0x97 => Instr::Cmp(CmpKind::DCmpL),
            0x98 => Instr::Cmp(CmpKind::DCmpG),
            0x99..=0x9e => {
                let t = branch_s2(&mut d, at)?;
                Instr::IfZero(cmp_op(op - 0x99)?, t)
            }
            0x9f..=0xa4 => {
                let t = branch_s2(&mut d, at)?;
                Instr::IfICmp(cmp_op(op - 0x9f)?, t)
            }
            0xa5 => {
                let t = branch_s2(&mut d, at)?;
                Instr::IfRefCmp {
                    eq: true,
                    target: t,
                }
            }
            0xa6 => {
                let t = branch_s2(&mut d, at)?;
                Instr::IfRefCmp {
                    eq: false,
                    target: t,
                }
            }
            0xa7 => {
                let t = branch_s2(&mut d, at)?;
                Instr::Goto(t)
            }
            0xa8 | 0xa9 => {
                return Err(DecodeError {
                    pc: at,
                    message: "jsr/ret are not supported (pre-Java-6 bytecode)".to_string(),
                });
            }
            0xaa => {
                let pad = (4 - (d.pos % 4)) % 4;
                d.pos += pad;
                let default = (at as i64 + d.s4()? as i64) as u32;
                let low = d.s4()?;
                let high = d.s4()?;
                if high < low {
                    return Err(DecodeError {
                        pc: at,
                        message: "tableswitch with high < low".to_string(),
                    });
                }
                let count = (high - low + 1) as usize;
                let mut targets = Vec::with_capacity(count);
                for _ in 0..count {
                    targets.push((at as i64 + d.s4()? as i64) as u32);
                }
                Instr::TableSwitch {
                    default,
                    low,
                    targets,
                }
            }
            0xab => {
                let pad = (4 - (d.pos % 4)) % 4;
                d.pos += pad;
                let default = (at as i64 + d.s4()? as i64) as u32;
                let npairs = d.s4()? as usize;
                let mut pairs = Vec::with_capacity(npairs);
                for _ in 0..npairs {
                    let m = d.s4()?;
                    let t = (at as i64 + d.s4()? as i64) as u32;
                    pairs.push((m, t));
                }
                Instr::LookupSwitch { default, pairs }
            }
            0xac => Instr::Return(TypeDesc::Int),
            0xad => Instr::Return(TypeDesc::Long),
            0xae => Instr::Return(TypeDesc::Float),
            0xaf => Instr::Return(TypeDesc::Double),
            0xb0 => Instr::Return(TypeDesc::Object(String::new())),
            0xb1 => Instr::Return(TypeDesc::Void),
            0xb2..=0xb5 => {
                let (class, field, desc) = d.member_ref()?;
                match op {
                    0xb2 => Instr::GetStatic { class, field, desc },
                    0xb3 => Instr::PutStatic { class, field, desc },
                    0xb4 => Instr::GetField { class, field, desc },
                    _ => Instr::PutField { class, field, desc },
                }
            }
            0xb6..=0xb8 => {
                let (class, name, desc) = d.member_ref()?;
                match op {
                    0xb6 => Instr::InvokeVirtual { class, name, desc },
                    0xb7 => Instr::InvokeSpecial { class, name, desc },
                    _ => Instr::InvokeStatic { class, name, desc },
                }
            }
            0xb9 => {
                let (class, name, desc) = d.member_ref()?;
                let _count = d.u1()?;
                let _zero = d.u1()?;
                Instr::InvokeInterface { class, name, desc }
            }
            0xba => {
                let idx = d.u2()?;
                let _z1 = d.u1()?;
                let _z2 = d.u1()?;
                Instr::InvokeDynamic { index: idx }
            }
            0xbb => Instr::New {
                class: d.class_at(at)?,
            },
            0xbc => {
                let atype = d.u1()?;
                let prim = match atype {
                    4 => PrimType::Boolean,
                    5 => PrimType::Char,
                    6 => PrimType::Float,
                    7 => PrimType::Double,
                    8 => PrimType::Byte,
                    9 => PrimType::Short,
                    10 => PrimType::Int,
                    11 => PrimType::Long,
                    other => {
                        return Err(DecodeError {
                            pc: at,
                            message: format!("bad newarray atype {other}"),
                        })
                    }
                };
                Instr::NewArray(prim)
            }
            0xbd => Instr::ANewArray {
                elem: d.class_at(at)?,
            },
            0xbe => Instr::ArrayLength,
            0xbf => Instr::AThrow,
            0xc0 => Instr::CheckCast {
                class: d.class_at(at)?,
            },
            0xc1 => Instr::InstanceOf {
                class: d.class_at(at)?,
            },
            0xc2 | 0xc3 => {
                return Err(DecodeError {
                    pc: at,
                    message: "monitorenter/monitorexit are not supported yet (synchronized blocks)"
                        .to_string(),
                });
            }
            0xc4 => {
                let wop = d.u1()?;
                match wop {
                    0x15..=0x19 => Instr::Load(d.u2()?),
                    0x36..=0x3a => Instr::Store(d.u2()?),
                    0x84 => {
                        let idx = d.u2()?;
                        let c = d.s2()? as i32;
                        Instr::IInc { idx, c }
                    }
                    0xa9 => {
                        return Err(DecodeError {
                            pc: at,
                            message: "wide ret is not supported".to_string(),
                        })
                    }
                    other => {
                        return Err(DecodeError {
                            pc: at,
                            message: format!("bad wide opcode {other:#x}"),
                        })
                    }
                }
            }
            0xc5 => {
                let desc = d.class_at(at)?;
                let dims = d.u1()?;
                Instr::MultiNewArray { desc, dims }
            }
            0xc6 => {
                let t = branch_s2(&mut d, at)?;
                Instr::IfNull(t)
            }
            0xc7 => {
                let t = branch_s2(&mut d, at)?;
                Instr::IfNonNull(t)
            }
            0xc8 => {
                let off = d.s4()?;
                Instr::Goto((at as i64 + off as i64) as u32)
            }
            0xc9 => {
                return Err(DecodeError {
                    pc: at,
                    message: "jsr_w is not supported".to_string(),
                });
            }
            other => {
                return Err(DecodeError {
                    pc: at,
                    message: format!("unknown opcode {other:#x}"),
                })
            }
        };
        out.push((at, instr));
    }
    Ok(out)
}

fn branch_s2(d: &mut Decoder<'_>, at: u32) -> DecodeResult<u32> {
    let off = d.s2()?;
    Ok((at as i64 + off as i64) as u32)
}

/// Opcode delta (0..=7) → array element kind, for both the load
/// (0x2e `iaload` .. 0x35 `saload`) and store (0x4f `iastore` .. 0x56
/// `sastore`) families, which share their ordering.
fn elem_kind(delta: u8) -> DecodeResult<ElemKind> {
    Ok(match delta {
        0 => ElemKind::IntGroup,     // i
        1 => ElemKind::Long,         // l
        2 => ElemKind::Float,        // f
        3 => ElemKind::Double,       // d
        4 => ElemKind::Ref,          // a
        5..=7 => ElemKind::IntGroup, // b, c, s
        other => {
            return Err(DecodeError {
                pc: 0,
                message: format!("not an array-access opcode delta: {other}"),
            })
        }
    })
}

fn cmp_op(delta: u8) -> DecodeResult<CmpOp> {
    Ok(match delta {
        0 => CmpOp::Eq,
        1 => CmpOp::Ne,
        2 => CmpOp::Lt,
        3 => CmpOp::Ge,
        4 => CmpOp::Gt,
        5 => CmpOp::Le,
        _ => {
            return Err(DecodeError {
                pc: 0,
                message: "bad comparison opcode".to_string(),
            })
        }
    })
}

fn explicit_arith(op: u8) -> DecodeResult<Instr> {
    let (kind, arith) = match op {
        0x60 => (NumKind::Int, ArithOp::Add),
        0x61 => (NumKind::Long, ArithOp::Add),
        0x62 => (NumKind::Float, ArithOp::Add),
        0x63 => (NumKind::Double, ArithOp::Add),
        0x64 => (NumKind::Int, ArithOp::Sub),
        0x65 => (NumKind::Long, ArithOp::Sub),
        0x66 => (NumKind::Float, ArithOp::Sub),
        0x67 => (NumKind::Double, ArithOp::Sub),
        0x68 => (NumKind::Int, ArithOp::Mul),
        0x69 => (NumKind::Long, ArithOp::Mul),
        0x6a => (NumKind::Float, ArithOp::Mul),
        0x6b => (NumKind::Double, ArithOp::Mul),
        0x6c => (NumKind::Int, ArithOp::Div),
        0x6d => (NumKind::Long, ArithOp::Div),
        0x6e => (NumKind::Float, ArithOp::Div),
        0x6f => (NumKind::Double, ArithOp::Div),
        0x70 => (NumKind::Int, ArithOp::Rem),
        0x71 => (NumKind::Long, ArithOp::Rem),
        0x72 => (NumKind::Float, ArithOp::Rem),
        0x73 => (NumKind::Double, ArithOp::Rem),
        0x74 => (NumKind::Int, ArithOp::Neg),
        0x75 => (NumKind::Long, ArithOp::Neg),
        0x76 => (NumKind::Float, ArithOp::Neg),
        0x77 => (NumKind::Double, ArithOp::Neg),
        0x78 => (NumKind::Int, ArithOp::Shl),
        0x79 => (NumKind::Long, ArithOp::Shl),
        0x7a => (NumKind::Int, ArithOp::Shr),
        0x7b => (NumKind::Long, ArithOp::Shr),
        0x7c => (NumKind::Int, ArithOp::UShr),
        0x7d => (NumKind::Long, ArithOp::UShr),
        0x7e => (NumKind::Int, ArithOp::And),
        0x7f => (NumKind::Long, ArithOp::And),
        0x80 => (NumKind::Int, ArithOp::Or),
        0x81 => (NumKind::Long, ArithOp::Or),
        0x82 => (NumKind::Int, ArithOp::Xor),
        0x83 => (NumKind::Long, ArithOp::Xor),
        _ => {
            return Err(DecodeError {
                pc: 0,
                message: format!("not an arithmetic opcode: {op:#x}"),
            })
        }
    };
    Ok(Instr::Arith { op: arith, kind })
}

fn explicit_convert(op: u8) -> DecodeResult<Instr> {
    let conv = match op {
        0x85 => Conv {
            from: NumKind::Int,
            to: NumKind::Long,
        },
        0x86 => Conv {
            from: NumKind::Int,
            to: NumKind::Float,
        },
        0x87 => Conv {
            from: NumKind::Int,
            to: NumKind::Double,
        },
        0x88 => Conv {
            from: NumKind::Long,
            to: NumKind::Int,
        },
        0x89 => Conv {
            from: NumKind::Long,
            to: NumKind::Float,
        },
        0x8a => Conv {
            from: NumKind::Long,
            to: NumKind::Double,
        },
        0x8b => Conv {
            from: NumKind::Float,
            to: NumKind::Int,
        },
        0x8c => Conv {
            from: NumKind::Float,
            to: NumKind::Long,
        },
        0x8d => Conv {
            from: NumKind::Float,
            to: NumKind::Double,
        },
        0x8e => Conv {
            from: NumKind::Double,
            to: NumKind::Int,
        },
        0x8f => Conv {
            from: NumKind::Double,
            to: NumKind::Long,
        },
        0x90 => Conv {
            from: NumKind::Double,
            to: NumKind::Float,
        },
        0x91 => Conv {
            from: NumKind::Int,
            to: NumKind::Byte,
        },
        0x92 => Conv {
            from: NumKind::Int,
            to: NumKind::Char,
        },
        0x93 => Conv {
            from: NumKind::Int,
            to: NumKind::Short,
        },
        _ => {
            return Err(DecodeError {
                pc: 0,
                message: format!("not a conversion opcode: {op:#x}"),
            })
        }
    };
    Ok(Instr::Convert(conv))
}

fn decode_ldc(cp: &ConstantPool, idx: u16) -> DecodeResult<Instr> {
    let entry = cp.entry(idx).map_err(|e| DecodeError {
        pc: 0,
        message: e.message,
    })?;
    Ok(match entry {
        j2r_classfile::CpEntry::Integer(v) => Instr::Ldc(Ldc::Int(*v)),
        j2r_classfile::CpEntry::Float(v) => Instr::Ldc(Ldc::Float(*v)),
        j2r_classfile::CpEntry::String(u) => {
            let s = cp.utf8(*u).map_err(|e| DecodeError {
                pc: 0,
                message: e.message,
            })?;
            Instr::Ldc(Ldc::String(s.to_string()))
        }
        j2r_classfile::CpEntry::Class(name_idx) => {
            let name = cp.utf8(*name_idx).map_err(|e| DecodeError {
                pc: 0,
                message: e.message,
            })?;
            Instr::Ldc(Ldc::Class(name.to_string()))
        }
        other => {
            return Err(DecodeError {
                pc: 0,
                message: format!("unsupported ldc constant ({})", other.tag_name()),
            })
        }
    })
}

fn decode_ldc2(cp: &ConstantPool, idx: u16) -> DecodeResult<Instr> {
    let entry = cp.entry(idx).map_err(|e| DecodeError {
        pc: 0,
        message: e.message,
    })?;
    Ok(match entry {
        j2r_classfile::CpEntry::Long(v) => Instr::Ldc2W(Ldc2::Long(*v)),
        j2r_classfile::CpEntry::Double(v) => Instr::Ldc2W(Ldc2::Double(*v)),
        other => {
            return Err(DecodeError {
                pc: 0,
                message: format!("unsupported ldc2_w constant ({})", other.tag_name()),
            })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_pool() -> ConstantPool {
        ConstantPool::default()
    }

    #[test]
    fn decodes_simple_straight_line() {
        // iconst_1 iconst_3 iadd ireturn
        let code = [0x04, 0x06, 0x60, 0xac];
        let instrs = decode(&code, &empty_pool()).unwrap();
        assert_eq!(
            instrs,
            vec![
                (0, Instr::IConst(1)),
                (1, Instr::IConst(3)),
                (
                    2,
                    Instr::Arith {
                        op: ArithOp::Add,
                        kind: NumKind::Int
                    }
                ),
                (3, Instr::Return(TypeDesc::Int)),
            ]
        );
    }

    #[test]
    fn decodes_branches_and_switches() {
        // goto +5; nop; nop; nop; return  (goto at 0 -> 5)
        let code = [0xa7, 0x00, 0x05, 0x00, 0x00, 0x00, 0xb1];
        let instrs = decode(&code, &empty_pool()).unwrap();
        assert_eq!(instrs[0], (0, Instr::Goto(5)));
        assert_eq!(instrs[1], (3, Instr::Nop));
        assert_eq!(instrs[4], (6, Instr::Return(TypeDesc::Void)));
    }

    #[test]
    fn decodes_tableswitch_with_padding() {
        // pc 0: tableswitch with 1 padding byte (pos 1 -> pad 3), low=1 high=2
        let mut code = vec![0xaa, 0x00, 0x00, 0x00];
        code.extend_from_slice(&8i32.to_be_bytes()); // default -> 8
        code.extend_from_slice(&1i32.to_be_bytes()); // low
        code.extend_from_slice(&2i32.to_be_bytes()); // high
        code.extend_from_slice(&4i32.to_be_bytes()); // case 1 -> 4
        code.extend_from_slice(&6i32.to_be_bytes()); // case 2 -> 6
        code.extend_from_slice(&[0xb1]);
        let instrs = decode(&code, &empty_pool()).unwrap();
        match &instrs[0].1 {
            Instr::TableSwitch {
                default,
                low,
                targets,
            } => {
                assert_eq!((*default, *low, targets.as_slice()), (8, 1, &[4, 6][..]));
            }
            other => panic!("expected tableswitch, got {other:?}"),
        }
    }

    #[test]
    fn rejects_jsr() {
        let code = [0xa8, 0x00, 0x02];
        let err = decode(&code, &empty_pool()).unwrap_err();
        assert!(err.message.contains("jsr"));
    }
}
