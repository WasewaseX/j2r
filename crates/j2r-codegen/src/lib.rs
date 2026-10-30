//! `j2r-codegen` — compiles JVM class files into a self-contained Rust crate.
//!
//! Pipeline:
//! 1. parse every input `.class` ([`j2r_classfile`]),
//! 2. validate all symbolic references against the combined universe of
//!    compiled classes + runtime natives (unsupported features are *compile
//!    errors*, never silent miscompiles),
//! 3. compute flattened field-slot layouts across class hierarchies and
//!    assign static-field pool slots,
//! 4. decode every method body to IR ([`ir`]),
//! 5. emit Rust sources ([`emit`]) plus a vendored copy of `j2r-runtime`,
//!    producing a crate that builds standalone with `cargo`.

pub mod emit;
pub mod ir;

/// Method plans + clinit fn name + concat sites for one class.
pub type MethodPlans = (
    Vec<MethodPlan>,
    Option<String>,
    std::collections::BTreeMap<u16, ConcatPlan>,
);

use j2r_classfile::{ClassFile, ConstantValue};
use j2r_runtime::{builtin, natives, parse_method_desc, TypeDesc};
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

// ---------------------------------------------------------------- errors

#[derive(Debug)]
pub enum CodegenError {
    Io(std::io::Error),
    ClassFile(String),
    Unsupported {
        class: String,
        method: String,
        pc: u32,
        what: String,
    },
    Semantic {
        class: String,
        message: String,
    },
}

impl fmt::Display for CodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodegenError::Io(e) => write!(f, "I/O error: {e}"),
            CodegenError::ClassFile(msg) => write!(f, "{msg}"),
            CodegenError::Unsupported {
                class,
                method,
                pc,
                what,
            } => write!(
                f,
                "unsupported feature in {class}.{method} at pc {pc}: {what}"
            ),
            CodegenError::Semantic { class, message } => {
                write!(f, "error in class {class}: {message}")
            }
        }
    }
}

impl std::error::Error for CodegenError {}

impl From<std::io::Error> for CodegenError {
    fn from(e: std::io::Error) -> Self {
        CodegenError::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, CodegenError>;

fn cf_err(e: j2r_classfile::Error) -> CodegenError {
    CodegenError::ClassFile(e.to_string())
}

// ---------------------------------------------------------------- options

#[derive(Debug, Clone)]
pub struct Options {
    pub inputs: Vec<PathBuf>,
    /// Class to use as the program entry point (internal or dotted form).
    /// Auto-detected when absent.
    pub main_class: Option<String>,
    /// Name of the generated crate.
    pub crate_name: String,
    /// Directory the generated crate is written to.
    pub out_dir: PathBuf,
}

impl Options {
    pub fn new(inputs: Vec<PathBuf>, out_dir: PathBuf) -> Options {
        Options {
            inputs,
            main_class: None,
            crate_name: "j2r-app".to_string(),
            out_dir,
        }
    }
}

// ---------------------------------------------------------------- model

#[derive(Debug, Clone)]
pub enum CallTarget {
    /// A method compiled from the inputs.
    User { module: String, fn_name: String },
    /// A native implementation inside j2r-runtime, found on builtin class
    /// `class`.
    Native { class: String },
}

#[derive(Debug)]
pub struct FieldPlan {
    pub name: String,
    pub desc: String,
    pub is_static: bool,
    pub static_slot: Option<u16>,
    /// Absolute slot index in the flattened object layout.
    pub instance_slot: Option<usize>,
    pub constant_value: Option<ConstantValue>,
}

#[derive(Debug, Clone)]
pub struct ConcatPlan {
    pub recipe: String,
    pub constants: Vec<j2r_runtime::Value>,
    /// One descriptor char per dynamic argument (I/J/F/D/C/Z/B/S/L).
    pub kinds: String,
}

#[derive(Debug)]
pub struct MethodPlan {
    pub name: String,
    pub desc: String,
    pub is_static: bool,
    pub fn_name: String,
    pub instrs: Vec<(u32, ir::Instr)>,
    pub exception_table: Vec<(u32, u32, u32, Option<String>)>,
    pub max_locals: usize,
    pub max_stack: usize,
}

#[derive(Debug)]
pub struct ClassPlan {
    pub name: String,
    pub super_name: Option<String>,
    pub interfaces: Vec<String>,
    pub is_interface: bool,
    pub is_abstract: bool,
    pub module: String,
    pub id: u16,
    /// Slot index where this class's own instance fields begin.
    pub base_slots: usize,
    /// Total instance slots (including inherited ones).
    pub total_slots: usize,
    pub fields: Vec<FieldPlan>,
    pub methods: Vec<MethodPlan>,
    /// `Some(fn_name)` when the class needs a `<clinit>` (declared or
    /// synthesized from ConstantValue fields).
    pub clinit_fn: Option<String>,
    /// invokedynamic call sites that resolve to string concatenation.
    pub concats: BTreeMap<u16, ConcatPlan>,
    cf: ClassFile,
}

impl ClassPlan {
    pub fn method(&self, name: &str, desc: &str) -> Option<&MethodPlan> {
        self.methods
            .iter()
            .find(|m| m.name == name && m.desc == desc)
    }
}

#[derive(Debug)]
pub struct Program {
    pub classes: Vec<ClassPlan>,
    pub user_index: BTreeMap<String, usize>,
    pub total_statics: usize,
    pub main_class: Option<String>,
}

impl Program {
    pub fn class(&self, name: &str) -> Option<&ClassPlan> {
        self.user_index.get(name).map(|i| &self.classes[*i])
    }

    fn is_user(&self, name: &str) -> bool {
        self.user_index.contains_key(name)
    }
}

// ---------------------------------------------------------------- helpers

pub fn sanitize_module(class: &str) -> String {
    class
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn method_fn_name(name: &str, desc: &str) -> String {
    let mut s = String::from("m_");
    for c in name.chars() {
        match c {
            '<' => s.push_str("init"),
            '>' => {}
            _ => s.push(c),
        }
    }
    s.push_str("__");
    for c in desc.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c);
        } else {
            s.push('_');
        }
    }
    s
}

fn to_internal(name: &str) -> String {
    name.replace('.', "/")
}

fn builtin_is_instantiable(class: &str) -> bool {
    if matches!(
        class,
        "java/lang/Object"
            | "java/lang/String"
            | "java/lang/StringBuilder"
            | "java/lang/StringBuffer"
    ) {
        return true;
    }
    // any builtin Throwable subclass is instantiable (its <init> is native)
    let mut cur = class;
    for _ in 0..16 {
        match builtin::builtin_parent_of(cur) {
            Some(p) => {
                if p == "java/lang/Throwable" {
                    return true;
                }
                cur = p;
            }
            None => return false,
        }
    }
    false
}

// ---------------------------------------------------------------- compilation

pub fn compile(opts: &Options) -> Result<Program> {
    if opts.inputs.is_empty() {
        return Err(CodegenError::Semantic {
            class: "-".to_string(),
            message: "no input .class files".to_string(),
        });
    }

    // 1. parse ------------------------------------------------------------
    let mut classes: Vec<ClassPlan> = Vec::new();
    let mut user_index: BTreeMap<String, usize> = BTreeMap::new();
    let mut static_slots = 0u16;

    for path in &opts.inputs {
        let bytes = std::fs::read(path).map_err(CodegenError::Io)?;
        let cf = ClassFile::parse(&bytes).map_err(cf_err)?;
        let name = cf.this_class.clone();
        if name.starts_with("java/")
            || name.starts_with("javax/")
            || name.starts_with("jdk/")
            || name.starts_with("sun/")
            || name.starts_with("com/sun/")
        {
            return Err(CodegenError::Semantic {
                class: name.clone(),
                message: "refusing to compile a JDK builtin class; it is provided by j2r-runtime"
                    .to_string(),
            });
        }
        if user_index.contains_key(&name) {
            return Err(CodegenError::Semantic {
                class: name.clone(),
                message: format!("duplicate class {} (check your inputs)", name),
            });
        }
        let module = sanitize_module(&name);
        if classes.iter().any(|c| c.module == module) {
            return Err(CodegenError::Semantic {
                class: name.clone(),
                message: format!(
                    "module name collision for {name} (two classes sanitize to {module})"
                ),
            });
        }
        let is_interface = cf.is_interface();
        let is_abstract = cf.is_abstract() || is_interface;
        let id = classes.len() as u16;

        // fields: layout + static slots
        let mut fields = Vec::new();
        let mut instance_count = 0usize;
        for f in &cf.fields {
            let is_static = f.is_static();
            let (static_slot, instance_slot) = if is_static {
                let s = static_slots;
                static_slots += 1;
                (Some(s), None)
            } else {
                let s = instance_count;
                instance_count += 1;
                (None, Some(s))
            };
            fields.push(FieldPlan {
                name: f.name.clone(),
                desc: f.descriptor.clone(),
                is_static,
                static_slot,
                instance_slot,
                constant_value: f.constant_value.clone(),
            });
        }

        user_index.insert(name.clone(), classes.len());
        classes.push(ClassPlan {
            name: name.clone(),
            super_name: cf.super_class.clone(),
            interfaces: cf.interfaces.clone(),
            is_interface,
            is_abstract,
            module,
            id,
            base_slots: 0,
            total_slots: 0,
            fields,
            methods: Vec::new(),
            clinit_fn: None,
            concats: BTreeMap::new(),
            cf,
        });
    }

    let mut prog = Program {
        classes,
        user_index,
        total_statics: static_slots as usize,
        main_class: None,
    };

    // 2. superclass validation -------------------------------------------
    for c in &prog.classes {
        if let Some(sup) = &c.super_name {
            if !prog.is_user(sup)
                && sup != "java/lang/Object"
                && builtin::builtin_parent_of(sup).is_none()
            {
                return Err(CodegenError::Semantic {
                    class: c.name.clone(),
                    message: format!("unknown superclass {sup}: add its .class file to the inputs"),
                });
            }
            let sup_plan = prog.class(sup);
            let sup_is_interface = sup_plan.map(|s| s.is_interface).unwrap_or(false);
            if !c.is_interface && sup_is_interface {
                return Err(CodegenError::Semantic {
                    class: c.name.clone(),
                    message: format!("class cannot extend interface {sup}"),
                });
            }
        }
        for itf in &c.interfaces {
            if !prog.is_user(itf) {
                // builtin interfaces are tolerated (Object/Comparable/...)
                continue;
            }
            if !prog.class(itf).map(|x| x.is_interface).unwrap_or(false) {
                return Err(CodegenError::Semantic {
                    class: c.name.clone(),
                    message: format!("interface {itf} is not an interface"),
                });
            }
        }
    }

    // 3. slot layouts ------------------------------------------------------
    compute_layouts(&mut prog)?;

    // 4. decode + synthesize clinit ----------------------------------------
    for ci in 0..prog.classes.len() {
        let (methods, clinit_fn, concats) = plan_methods(&prog, ci)?;
        let plan = &mut prog.classes[ci];
        plan.methods = methods;
        plan.clinit_fn = clinit_fn;
        plan.concats = concats;
    }

    // 5. validate every instruction ----------------------------------------
    for c in &prog.classes {
        validate_class(&prog, c)?;
    }

    // 6. main class ---------------------------------------------------------
    let main_class = match &opts.main_class {
        Some(m) => {
            let internal = to_internal(m);
            if !prog.is_user(&internal) {
                return Err(CodegenError::Semantic {
                    class: internal.clone(),
                    message: "--main-class not found among the inputs".to_string(),
                });
            }
            let plan = prog.class(&internal).unwrap();
            if plan.method("main", "([Ljava/lang/String;)V").is_none() {
                return Err(CodegenError::Semantic {
                    class: internal.clone(),
                    message: "--main-class has no public static void main(String[])".to_string(),
                });
            }
            Some(internal)
        }
        None => {
            let found = prog.classes.iter().find(|c| {
                c.method("main", "([Ljava/lang/String;)V")
                    .map(|m| m.is_static)
                    .unwrap_or(false)
            });
            found.map(|c| c.name.clone())
        }
    };
    prog.main_class = main_class;

    Ok(prog)
}

fn compute_layouts(prog: &mut Program) -> Result<()> {
    // memoized total_slots over user classes + builtin parents
    fn total_slots(prog: &Program, name: &str, depth: usize) -> Result<usize> {
        if depth > 64 {
            return Err(CodegenError::Semantic {
                class: name.to_string(),
                message: "class hierarchy too deep (cycle?)".to_string(),
            });
        }
        if let Some(idx) = prog.user_index.get(name) {
            let sup = prog.classes[*idx].super_name.clone();
            let base = match &sup {
                Some(s) => {
                    if prog.is_user(s) {
                        total_slots(prog, s, depth + 1)?
                    } else {
                        builtin::builtin_total_slots(s).ok_or_else(|| CodegenError::Semantic {
                            class: name.to_string(),
                            message: format!("unknown superclass {s}"),
                        })?
                    }
                }
                None => 0,
            };
            let own = prog.classes[*idx]
                .fields
                .iter()
                .filter(|f| !f.is_static)
                .count();
            Ok(base + own)
        } else {
            builtin::builtin_total_slots(name).ok_or_else(|| CodegenError::Semantic {
                class: name.to_string(),
                message: format!("unknown class {name} in layout computation"),
            })
        }
    }

    let names: Vec<String> = prog.classes.iter().map(|c| c.name.clone()).collect();
    for name in &names {
        let idx = prog.user_index[name];
        let sup = prog.classes[idx].super_name.clone();
        let base = match &sup {
            Some(s) => {
                if prog.is_user(s) {
                    total_slots(prog, s, 0)?
                } else {
                    builtin::builtin_total_slots(s).ok_or_else(|| CodegenError::Semantic {
                        class: name.clone(),
                        message: format!("unknown superclass {s}"),
                    })?
                }
            }
            None => 0,
        };
        let total = total_slots(prog, name, 0)?;
        let plan = &mut prog.classes[idx];
        plan.base_slots = base;
        plan.total_slots = total;
        // assign absolute instance slots
        let mut slot = base;
        for f in plan.fields.iter_mut() {
            if !f.is_static {
                f.instance_slot = Some(slot);
                slot += 1;
            }
        }
    }
    Ok(())
}

/// Decodes all methods of class `ci`, synthesizing `<clinit>` bodies for
/// `ConstantValue` fields and resolving `invokedynamic` concat sites.
fn plan_methods(prog: &Program, ci: usize) -> Result<MethodPlans> {
    let c = &prog.classes[ci];
    let mut methods = Vec::new();
    let mut clinit_fn = None;
    let mut concats = BTreeMap::new();

    for m in &c.cf.methods {
        if m.code.is_none() {
            if m.is_abstract() {
                continue; // abstract: no body to compile
            }
            return Err(CodegenError::Unsupported {
                class: c.name.clone(),
                method: format!("{}{}", m.name, m.descriptor),
                pc: 0,
                what: "native methods are not supported".to_string(),
            });
        }
        let code = m.code().map_err(cf_err)?;
        let instrs =
            ir::decode(&code.code, &c.cf.constant_pool).map_err(|e| CodegenError::Unsupported {
                class: c.name.clone(),
                method: format!("{}{}", m.name, m.descriptor),
                pc: e.pc,
                what: e.message,
            })?;

        // invokedynamic sites
        for (pc, instr) in &instrs {
            if let ir::Instr::InvokeDynamic { index } = instr {
                let plan = resolve_concat(&prog.classes[ci], *index).map_err(|what| {
                    CodegenError::Unsupported {
                        class: c.name.clone(),
                        method: format!("{}{}", m.name, m.descriptor),
                        pc: *pc,
                        what,
                    }
                })?;
                concats.insert(*index, plan);
            }
        }

        let is_clinit = m.name == "<clinit>";
        if is_clinit {
            clinit_fn = Some(method_fn_name(&m.name, &m.descriptor));
        }
        methods.push(MethodPlan {
            name: m.name.clone(),
            desc: m.descriptor.clone(),
            is_static: m.is_static(),
            fn_name: method_fn_name(&m.name, &m.descriptor),
            instrs,
            exception_table: code
                .exception_table
                .iter()
                .map(|e| {
                    (
                        e.start_pc as u32,
                        e.end_pc as u32,
                        e.handler_pc as u32,
                        e.catch_type.clone(),
                    )
                })
                .collect(),
            max_locals: code.max_locals as usize,
            max_stack: code.max_stack as usize,
        });
    }

    // Synthesize <clinit> work for ConstantValue static finals: append
    // Ldc + PutStatic to the existing clinit, or create a new one.
    let constant_fields: Vec<&FieldPlan> = c
        .fields
        .iter()
        .filter(|f| f.is_static && f.constant_value.is_some())
        .collect();
    if !constant_fields.is_empty() {
        let mut synth: Vec<(u32, ir::Instr)> = Vec::new();
        let mut pc = 0u32;
        for f in constant_fields {
            let value = match f.constant_value.as_ref().unwrap() {
                ConstantValue::Int(v) => ir::Ldc::Int(*v),
                ConstantValue::Float(v) => ir::Ldc::Float(*v),
                ConstantValue::Long(v) => {
                    synth.push((pc, ir::Instr::Ldc2W(ir::Ldc2::Long(*v))));
                    pc += 1;
                    synth.push((
                        pc,
                        ir::Instr::PutStatic {
                            class: c.name.clone(),
                            field: f.name.clone(),
                            desc: f.desc.clone(),
                        },
                    ));
                    pc += 1;
                    continue;
                }
                ConstantValue::Double(v) => {
                    synth.push((pc, ir::Instr::Ldc2W(ir::Ldc2::Double(*v))));
                    pc += 1;
                    synth.push((
                        pc,
                        ir::Instr::PutStatic {
                            class: c.name.clone(),
                            field: f.name.clone(),
                            desc: f.desc.clone(),
                        },
                    ));
                    pc += 1;
                    continue;
                }
                ConstantValue::String(s) => ir::Ldc::String(s.clone()),
            };
            synth.push((pc, ir::Instr::Ldc(value)));
            pc += 1;
            synth.push((
                pc,
                ir::Instr::PutStatic {
                    class: c.name.clone(),
                    field: f.name.clone(),
                    desc: f.desc.clone(),
                },
            ));
            pc += 1;
        }
        match methods.iter_mut().find(|m| m.name == "<clinit>") {
            Some(clinit) => {
                clinit.instrs.extend(synth);
            }
            None => {
                clinit_fn = Some(method_fn_name("<clinit>", "()V"));
                methods.push(MethodPlan {
                    name: "<clinit>".to_string(),
                    desc: "()V".to_string(),
                    is_static: true,
                    fn_name: method_fn_name("<clinit>", "()V"),
                    instrs: synth,
                    exception_table: Vec::new(),
                    max_locals: 0,
                    max_stack: 2,
                });
            }
        }
    }

    Ok((methods, clinit_fn, concats))
}

/// Resolves an `invokedynamic` call site. Milestone 1 supports exactly one
/// bootstrap: `StringConcatFactory.makeConcatWithConstants`.
fn resolve_concat(c: &ClassPlan, indy_index: u16) -> std::result::Result<ConcatPlan, String> {
    let cp = &c.cf.constant_pool;
    let (bsm_index, _name, desc) = cp.dynamic(indy_index).map_err(|e| e.message)?;
    let bsm =
        c.cf.bootstrap_methods
            .get(bsm_index as usize)
            .ok_or_else(|| format!("bootstrap method #{bsm_index} missing"))?;
    // method handle -> methodref
    let handle = cp.entry(bsm.method_handle).map_err(|e| e.message)?;
    let mh_index = match handle {
        j2r_classfile::CpEntry::MethodHandle { kind: _, index } => *index,
        other => return Err(format!("bootstrap handle is {}", other.tag_name())),
    };
    let (cls, meth, _mdesc) = cp.member_ref(mh_index).map_err(|e| e.message)?;
    if cls != "java/lang/invoke/StringConcatFactory" || meth != "makeConcatWithConstants" {
        return Err(format!(
            "unsupported invokedynamic bootstrap {cls}.{meth} (only StringConcatFactory.makeConcatWithConstants is supported)"
        ));
    }
    let recipe = match bsm.arguments.first() {
        Some(j2r_classfile::BootstrapArgument::String(s)) => s.clone(),
        Some(other) => {
            return Err(format!(
                "makeConcatWithConstants recipe must be a String, found {other:?}"
            ))
        }
        None => return Err("makeConcatWithConstants without recipe".to_string()),
    };
    let mut constants = Vec::new();
    for arg in &bsm.arguments[1..] {
        constants.push(match arg {
            j2r_classfile::BootstrapArgument::Int(v) => j2r_runtime::Value::Int(*v),
            j2r_classfile::BootstrapArgument::Long(v) => j2r_runtime::Value::Long(*v),
            j2r_classfile::BootstrapArgument::Float(v) => j2r_runtime::Value::Float(*v),
            j2r_classfile::BootstrapArgument::Double(v) => j2r_runtime::Value::Double(*v),
            j2r_classfile::BootstrapArgument::String(s) => {
                j2r_runtime::Value::Str(std::rc::Rc::new(s.clone()))
            }
            j2r_classfile::BootstrapArgument::Opaque { tag } => {
                return Err(format!("unsupported concat bootstrap constant ({tag})"))
            }
        });
    }
    let (params, ret) =
        parse_method_desc(&desc).ok_or_else(|| format!("bad indy descriptor {desc:?}"))?;
    if !matches!(ret, TypeDesc::Object(ref o) if o == "java/lang/String") {
        return Err(format!("concat must return String, got {desc:?}"));
    }
    let kinds: String = params
        .iter()
        .map(|t| match t {
            TypeDesc::Int => 'I',
            TypeDesc::Long => 'J',
            TypeDesc::Float => 'F',
            TypeDesc::Double => 'D',
            TypeDesc::Char => 'C',
            TypeDesc::Boolean => 'Z',
            TypeDesc::Byte => 'B',
            TypeDesc::Short => 'S',
            TypeDesc::Object(_) | TypeDesc::Array(_) => 'L',
            TypeDesc::Void => 'V',
        })
        .collect();
    Ok(ConcatPlan {
        recipe,
        constants,
        kinds,
    })
}

/// Resolves a static or special (non-virtual) method reference.
fn resolve_call(prog: &Program, start: &str, name: &str, desc: &str) -> Result<CallTarget> {
    let mut cur = start.to_string();
    for _ in 0..64 {
        if prog.is_user(&cur) {
            let c = prog.class(&cur).unwrap();
            if let Some(m) = c.method(name, desc) {
                return Ok(CallTarget::User {
                    module: c.module.clone(),
                    fn_name: m.fn_name.clone(),
                });
            }
            match &c.super_name {
                Some(s) => cur = s.clone(),
                None => break,
            }
        } else {
            break;
        }
    }
    // builtin chain, continuing from the first builtin ancestor
    for _ in 0..64 {
        if let Some(n) = natives::lookup_exact(&cur, name, desc) {
            let _ = n;
            return Ok(CallTarget::Native { class: cur });
        }
        match builtin::builtin_parent_of(&cur) {
            Some(p) => cur = p.to_string(),
            None => break,
        }
    }
    Err(CodegenError::Semantic {
        class: start.to_string(),
        message: format!(
            "cannot resolve method {name}{desc} starting at {start} (is its class in the inputs, or is it an unsupported JDK method?)"
        ),
    })
}

/// Validates a virtual/interface call target exists somewhere in the
/// receiver's hierarchy (compiled or native). Abstract declarations count.
fn validate_virtual(prog: &Program, declared: &str, name: &str, desc: &str) -> Result<()> {
    let mut cur = declared.to_string();
    for _ in 0..64 {
        if prog.is_user(&cur) {
            let c = prog.class(&cur).unwrap();
            let declared = c.cf.method(name, desc).is_some();
            let compiled = c.method(name, desc).is_some();
            if declared || compiled {
                return Ok(()); // abstract declarations dispatch from the runtime class
            }
            match &c.super_name {
                Some(s) => cur = s.clone(),
                None => break,
            }
        } else {
            break;
        }
    }
    // continue the walk from the first builtin ancestor
    for _ in 0..64 {
        if natives::lookup_exact(&cur, name, desc).is_some() {
            return Ok(());
        }
        match builtin::builtin_parent_of(&cur) {
            Some(p) => cur = p.to_string(),
            None => break,
        }
    }
    Err(CodegenError::Semantic {
        class: declared.to_string(),
        message: format!(
            "virtual call target {name}{desc} not found on {declared} or its hierarchy"
        ),
    })
}

/// Resolves an instance or static field reference to its owning user class
/// and its slot plan. `want_static` selects the field kind.
fn resolve_field<'a>(
    prog: &'a Program,
    start: &str,
    field: &str,
    want_static: bool,
) -> Result<&'a FieldPlan> {
    let mut cur = start.to_string();
    for _ in 0..64 {
        if prog.is_user(&cur) {
            let c = prog.class(&cur).unwrap();
            if let Some(f) = c.fields.iter().find(|f| f.name == field) {
                if f.is_static == want_static {
                    return Ok(f);
                }
                return Err(CodegenError::Semantic {
                    class: start.to_string(),
                    message: format!(
                        "field {field} of {cur} is {} but accessed as {}",
                        if f.is_static { "static" } else { "instance" },
                        if want_static { "static" } else { "instance" }
                    ),
                });
            }
            match &c.super_name {
                Some(s) => cur = s.clone(),
                None => break,
            }
        } else {
            break;
        }
    }
    Err(CodegenError::Semantic {
        class: start.to_string(),
        message: format!(
            "cannot resolve {} field {field} starting at {start} (unsupported JDK fields must be modelled as natives)",
            if want_static { "static" } else { "instance" }
        ),
    })
}

/// Validates every instruction of every method of `c` against `prog`.
fn validate_class(prog: &Program, c: &ClassPlan) -> Result<()> {
    for m in &c.methods {
        let ctx = format!("{}{}", m.name, m.desc);
        for (pc, instr) in &m.instrs {
            validate_instr(prog, c, &ctx, *pc, instr)?;
        }
    }
    Ok(())
}

fn unsupported(c: &ClassPlan, method: &str, pc: u32, what: String) -> CodegenError {
    CodegenError::Unsupported {
        class: c.name.clone(),
        method: method.to_string(),
        pc,
        what,
    }
}

fn validate_instr(
    prog: &Program,
    c: &ClassPlan,
    method_ctx: &str,
    pc: u32,
    instr: &ir::Instr,
) -> Result<()> {
    match instr {
        ir::Instr::Ldc(ir::Ldc::Class(name)) => Err(unsupported(
            c,
            method_ctx,
            pc,
            format!("ldc of a class constant ({name}) is not supported yet"),
        )),
        ir::Instr::GetStatic { class, field, .. } => {
            if class == "java/lang/System" && (field == "out" || field == "err") {
                return Ok(());
            }
            resolve_field(prog, class, field, true).map(|_| ())
        }
        ir::Instr::PutStatic { class, field, .. } => {
            if class == "java/lang/System" {
                return Err(unsupported(
                    c,
                    method_ctx,
                    pc,
                    "assigning to System.out/System.err is not supported".to_string(),
                ));
            }
            resolve_field(prog, class, field, true).map(|_| ())
        }
        ir::Instr::GetField { class, field, .. } => {
            resolve_field(prog, class, field, false).map(|_| ())
        }
        ir::Instr::PutField { class, field, .. } => {
            resolve_field(prog, class, field, false).map(|_| ())
        }
        ir::Instr::InvokeStatic { class, name, desc } => {
            resolve_call(prog, class, name, desc).map(|_| ())
        }
        ir::Instr::InvokeSpecial { class, name, desc } => {
            resolve_call(prog, class, name, desc).map(|_| ())
        }
        ir::Instr::InvokeVirtual { class, name, desc }
        | ir::Instr::InvokeInterface { class, name, desc } => {
            validate_virtual(prog, class, name, desc)
        }
        ir::Instr::InvokeDynamic { index } => {
            if !c.concats.contains_key(index) {
                return Err(unsupported(
                    c,
                    method_ctx,
                    pc,
                    format!("invokedynamic #{index} was not resolved"),
                ));
            }
            Ok(())
        }
        ir::Instr::New { class } => {
            if prog.is_user(class) {
                let target = prog.class(class).unwrap();
                if target.is_abstract {
                    return Err(unsupported(
                        c,
                        method_ctx,
                        pc,
                        format!("cannot instantiate abstract class {class}"),
                    ));
                }
                return Ok(());
            }
            if builtin_is_instantiable(class) {
                Ok(())
            } else {
                Err(unsupported(
                    c,
                    method_ctx,
                    pc,
                    format!("instantiation of JDK class {class} is not supported"),
                ))
            }
        }
        ir::Instr::ANewArray { elem } => {
            if prog.is_user(elem)
                || elem == "java/lang/Object"
                || builtin::builtin_parent_of(elem).is_some()
                || elem.starts_with('[')
            {
                Ok(())
            } else {
                Err(unsupported(
                    c,
                    method_ctx,
                    pc,
                    format!("unknown array element type {elem}"),
                ))
            }
        }
        _ => Ok(()),
    }
}

/// Full pipeline: compile + write the crate to `opts.out_dir`.
pub fn compile_to_crate(opts: &Options) -> Result<PathBuf> {
    let prog = compile(opts)?;
    emit::emit_crate(&prog, opts)?;
    Ok(opts.out_dir.clone())
}

/// Public wrappers used by the emitter (it needs the same resolution logic
/// the validator used).
pub fn resolve_call_pub(prog: &Program, start: &str, name: &str, desc: &str) -> Result<CallTarget> {
    resolve_call(prog, start, name, desc)
}

pub fn resolve_field_pub<'a>(
    prog: &'a Program,
    start: &str,
    field: &str,
    want_static: bool,
) -> Result<&'a FieldPlan> {
    resolve_field(prog, start, field, want_static)
}

/// Entry point metadata used by the CLI.
pub struct MainInfo {
    pub module: String,
    pub fn_name: String,
}

pub fn main_entry(prog: &Program) -> Option<MainInfo> {
    prog.main_class.as_ref().and_then(|name| {
        prog.class(name).and_then(|c| {
            c.method("main", "([Ljava/lang/String;)V")
                .map(|m| MainInfo {
                    module: c.module.clone(),
                    fn_name: m.fn_name.clone(),
                })
        })
    })
}
