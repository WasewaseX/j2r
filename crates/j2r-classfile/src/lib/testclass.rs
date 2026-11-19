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

/// Like `method`, but with an explicit `code_length` field and no
/// exception table — for the negative bounds tests (0 and >= 65536 must
/// be rejected, audit A4).
pub fn method_with_code_len(
    access: u16,
    name_idx: u16,
    desc_idx: u16,
    max_stack: u16,
    max_locals: u16,
    code_len: u32,
    code: &[u8],
    code_attr_name_idx: u16,
) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&max_stack.to_be_bytes());
    body.extend_from_slice(&max_locals.to_be_bytes());
    body.extend_from_slice(&code_len.to_be_bytes());
    body.extend_from_slice(code);
    body.extend_from_slice(&0u16.to_be_bytes()); // exception_table_length
    body.extend_from_slice(&0u16.to_be_bytes()); // attributes_count

    let mut m = Vec::new();
    m.extend_from_slice(&access.to_be_bytes());
    m.extend_from_slice(&name_idx.to_be_bytes());
    m.extend_from_slice(&desc_idx.to_be_bytes());
    m.extend_from_slice(&1u16.to_be_bytes());
    m.extend_from_slice(&code_attr_name_idx.to_be_bytes());
    m.extend_from_slice(&(body.len() as u32).to_be_bytes());
    m.extend_from_slice(&body);
    m
}

/// Like `method`, but with explicit fully-formed Code sub-attribute
/// blobs (`attribute_info`: name_idx u2, length u4, payload) — for the
/// sub-attribute walk tests (audit A4).
pub fn method_with_sub_attrs(
    access: u16,
    name_idx: u16,
    desc_idx: u16,
    max_stack: u16,
    max_locals: u16,
    code: &[u8],
    sub_attrs: &[&[u8]],
    code_attr_name_idx: u16,
) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&max_stack.to_be_bytes());
    body.extend_from_slice(&max_locals.to_be_bytes());
    body.extend_from_slice(&(code.len() as u32).to_be_bytes());
    body.extend_from_slice(code);
    body.extend_from_slice(&0u16.to_be_bytes()); // exception_table_length
    body.extend_from_slice(&(sub_attrs.len() as u16).to_be_bytes());
    for a in sub_attrs {
        body.extend_from_slice(a);
    }

    let mut m = Vec::new();
    m.extend_from_slice(&access.to_be_bytes());
    m.extend_from_slice(&name_idx.to_be_bytes());
    m.extend_from_slice(&desc_idx.to_be_bytes());
    m.extend_from_slice(&1u16.to_be_bytes());
    m.extend_from_slice(&code_attr_name_idx.to_be_bytes());
    m.extend_from_slice(&(body.len() as u32).to_be_bytes());
    m.extend_from_slice(&body);
    m
}

/// Like `field`, but with an explicit ConstantValue attribute length —
/// negative tests: anything but 2 must be rejected (audit A4).
pub fn field_with_constant_value_len(
    access: u16,
    name_idx: u16,
    desc_idx: u16,
    cv_len: u32,
    constant_value: u16,
    constvalue_name_idx: u16,
) -> Vec<u8> {
    let mut f = Vec::new();
    f.extend_from_slice(&access.to_be_bytes());
    f.extend_from_slice(&name_idx.to_be_bytes());
    f.extend_from_slice(&desc_idx.to_be_bytes());
    f.extend_from_slice(&1u16.to_be_bytes());
    f.extend_from_slice(&constvalue_name_idx.to_be_bytes());
    f.extend_from_slice(&cv_len.to_be_bytes());
    f.extend_from_slice(&constant_value.to_be_bytes());
    f
}

pub fn access_public_static() -> u16 {
    flags::ACC_PUBLIC | flags::ACC_STATIC
}
