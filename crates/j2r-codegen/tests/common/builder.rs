//! Minimal `.class` file builder used by tests (the test counterpart of
//! javac). Produces valid Java 8 class files (major 52, no StackMapTable).

pub const ACC_PUBLIC: u16 = 0x0001;
pub const ACC_STATIC: u16 = 0x0008;

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
        self.take()
    }

    pub fn class(&mut self, name_idx: u16) -> u16 {
        self.pool.push(7);
        self.pool.extend_from_slice(&name_idx.to_be_bytes());
        self.take()
    }

    pub fn string(&mut self, utf8_idx: u16) -> u16 {
        self.pool.push(8);
        self.pool.extend_from_slice(&utf8_idx.to_be_bytes());
        self.take()
    }

    pub fn name_and_type(&mut self, name_idx: u16, desc_idx: u16) -> u16 {
        self.pool.push(12);
        self.pool.extend_from_slice(&name_idx.to_be_bytes());
        self.pool.extend_from_slice(&desc_idx.to_be_bytes());
        self.take()
    }

    pub fn method_ref(&mut self, class_idx: u16, nat_idx: u16) -> u16 {
        self.pool.push(10);
        self.pool.extend_from_slice(&class_idx.to_be_bytes());
        self.pool.extend_from_slice(&nat_idx.to_be_bytes());
        self.take()
    }

    pub fn field_ref(&mut self, class_idx: u16, nat_idx: u16) -> u16 {
        self.pool.push(9);
        self.pool.extend_from_slice(&class_idx.to_be_bytes());
        self.pool.extend_from_slice(&nat_idx.to_be_bytes());
        self.take()
    }

    fn take(&mut self) -> u16 {
        let i = self.idx;
        self.idx += 1;
        i
    }

    #[allow(clippy::too_many_arguments)]
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
        b.extend_from_slice(&0u16.to_be_bytes());
        b.extend_from_slice(&52u16.to_be_bytes());
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

/// One method with a Code attribute and optional exception table.
#[allow(clippy::too_many_arguments)]
pub fn method(
    access: u16,
    name_idx: u16,
    desc_idx: u16,
    max_stack: u16,
    max_locals: u16,
    code: &[u8],
    exception_table: &[(u16, u16, u16, u16)],
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
    body.extend_from_slice(&0u16.to_be_bytes());

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
