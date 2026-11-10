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
//! 5. **verify every method body** ([`verify`]): abstract stack shape,
//!    category-1/2 correctness, local-variable bounds, branch-target and
//!    join consistency — malformed bytecode is a structured compile error,
//!    never a runtime panic,
//! 6. emit Rust sources ([`emit`]) plus a vendored copy of `j2r-runtime`,
//!    producing a crate that builds standalone with `cargo`.

#![forbid(unsafe_code)]

pub mod cfg;
pub mod classpath;
pub mod emit;
pub mod ir;
pub mod lambda;
pub mod manifest;
pub mod partition;
pub mod peephole;
pub mod readiness;
pub mod sha256;
pub mod specialize;
pub mod typed;
pub mod verify;

/// Method plans + clinit fn name + indy sites (concat + lambda) for one class.
pub type MethodPlans = (
    Vec<MethodPlan>,
    Option<String>,
    std::collections::BTreeMap<u16, ConcatPlan>,
    std::collections::BTreeMap<u16, lambda::LambdaPlan>,
);

use j2r_classfile::{ClassFile, ConstantValue};
use j2r_runtime::{builtin, natives, parse_method_desc, TypeDesc};
use std::collections::{BTreeMap, BTreeSet};
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
    /// Bytecode verification failure: malformed input rejected at compile
    /// time with the faulting `pc` (see [`verify`]).
    Verify {
        class: String,
        method: String,
        pc: u32,
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
            CodegenError::Verify {
                class,
                method,
                pc,
                message,
            } => write!(
                f,
                "verification error in {class}.{method} at pc {pc}: {message}"
            ),
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
    /// Classpath entries (M3 Wave 1, M3_PLAN §4): directories and
    /// `.jar`/`.zip` archives searched in order for classes not named on
    /// the command line. Empty = the M1 explicit-inputs-only model.
    pub classpath: Vec<PathBuf>,
    /// Class to use as the program entry point (internal or dotted form).
    /// Auto-detected when absent.
    pub main_class: Option<String>,
    /// Name of the generated crate.
    pub crate_name: String,
    /// Directory the generated crate is written to.
    pub out_dir: PathBuf,
    /// Wave-4 investigation flag (M2_PLAN §6): attempt structured CFG-based
    /// method bodies where a conservative analysis permits it, falling back
    /// to the approved pc-dispatch body per method otherwise. Default
    /// **off** — the dispatch emission is the approved default.
    pub emit_cfg: bool,
    /// The registered typed-operand lever (docs/TYPED_REGISTER_READINESS.md
    /// §5, step 2): emit per-epoch typed operand storage for methods the
    /// readiness analysis plans, falling back per method otherwise. Default
    /// **off**. Mutually exclusive with `emit_cfg`.
    pub emit_typed: bool,
    /// M5 T05 (docs/M5_SCALE.md §3): crate-partitioned emission. `Some(k)`
    /// with more than `k` classes emits a cargo **workspace**: `k` classes
    /// per `part-i` crate, a shared `support` crate (statics pool + global
    /// link table + dispatch slot), and an `app` crate stitching the
    /// composite `Dispatch`. Cross-partition direct calls go through the
    /// link table so the class reference web may stay strongly connected
    /// across crates. Default **None** — the single-crate emission is
    /// byte-identical to the pre-M5 shape.
    pub partition: Option<usize>,
}

impl Options {
    pub fn new(inputs: Vec<PathBuf>, out_dir: PathBuf) -> Options {
        Options {
            inputs,
            classpath: Vec::new(),
            main_class: None,
            crate_name: "j2r-app".to_string(),
            out_dir,
            emit_cfg: false,
            emit_typed: false,
            partition: None,
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
    /// Index into the generated crate's static-field pool. `u32` so that
    /// codebases far beyond 65,535 static fields compile correctly (the
    /// `u16`s that remain are constant-pool indexes, a JVM-spec limit).
    pub static_slot: Option<u32>,
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
    /// `ACC_NATIVE` (T12, M5_JNI_NATIVES.md §5): the method has no
    /// bytecode body. 12b always records `false` here (native methods
    /// do not reach plan_methods yet); 12c turns the plan loop's native
    /// error into the call-through emission and sets the flag.
    pub is_native: bool,
    /// `ACC_SYNCHRONIZED` (M3 Wave 4): the emitter wraps the body with a
    /// monitor acquisition on the receiver (instance) or the class's
    /// synthetic monitor object (static).
    pub is_sync: bool,
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
    /// `InnerClasses` `inner_name` (M3_PLAN §8.1): `None` when absent,
    /// `Some("")` for anonymous classes; exported into the generated
    /// registry for `Class.getSimpleName`.
    pub simple_name: Option<String>,
    pub module: String,
    /// Internal class id used by the generated `statics` registry (`u32`,
    /// same rationale as [`FieldPlan::static_slot`]).
    pub id: u32,
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
    /// invokedynamic call sites that resolve to LambdaMetafactory closures.
    pub lambdas: BTreeMap<u16, lambda::LambdaPlan>,
    /// Index into the generated statics' class-monitor table when any
    /// static method of this class is `ACC_SYNCHRONIZED` (M3_PLAN §7.1:
    /// a synthetic per-class monitor object, allocated lazily and scanned
    /// as a static root).
    pub static_monitor: Option<u32>,
    /// T03 closure cache (M5_SCALE.md §3.1): the user-namespace classes
    /// this class references, extracted ONCE at parse time. The classpath
    /// closure walks these cached delta sets instead of re-walking every
    /// class's constant pool on every worklist round (the measured
    /// quadratic term: codegen lib.rs closure loop, 0.40 s at 1k farm
    /// classes and growing with classes²).
    pub refs: std::collections::BTreeSet<String>,
    /// SHA-256 of the `.class` bytes this plan was parsed from (M5 T06/T07
    /// build manifest): the per-class input hash for the reuse gate.
    /// Computed once here, so explicit-input and classpath-closure loads
    /// are treated identically.
    pub input_sha: String,
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
            // `$` is legal in JVM method names (javac's synthetic
            // `lambda$…` members) but not in Rust identifiers.
            '$' => s.push_str("_d_"),
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
            // M3 Wave 2: collections are heap-resident runtime classes
            // (M3_PLAN §5.1/§5.4 — the layout pass skips them; their slots
            // are owned by j2r-runtime/src/collections.rs)
            | "java/util/ArrayList"
            | "java/util/HashMap"
            | "java/util/HashSet"
            // stringsim corpus wave (2026-11-07): entrySet views, the
            // unmodifiableMap wrapper, and Sift4's LinkedList — slots
            // owned by j2r-runtime/src/collections.rs
            | "java/util/HashMap$EntrySet"
            | "java/util/HashMap$EntryIterator"
            | "java/util/Collections$UnmodifiableMap"
            | "java/util/LinkedList"
            // corpus wave 1 (F3): java.io StringWriter/StringReader —
            // payloads managed by j2r-runtime/src/io.rs; the layout pass
            // skips builtins
            | "java/io/StringWriter"
            | "java/io/StringReader"
            // M4 Wave 2 (T06): java.lang.Thread — slots owned by
            // j2r-runtime/src/thread.rs (runnable/name/daemon/started/tid);
            // the layout pass skips builtins.
            | "java/lang/Thread"
            // Corpus wave 2 (JZlib): ByteArrayOutputStream — JDK field model
            // in slots (buf byte[] ref, count Int), behavior in io.rs.
            | "java/io/ByteArrayOutputStream"
            // Skip/charset wave: ByteArrayInputStream — slots
            // (buf ref, pos, mark, count), behavior in io.rs; semantics
            // pinned by corpus/SkipProbe.java (close() no-op, negative skip
            // = 0, skip past EOF = actual count).
            | "java/io/ByteArrayInputStream"
            // Stream wave 3: BufferedInputStream — slots (inherited in,
            // buf ref, pos, count, markpos, marklimit), behavior in io.rs;
            // pinned by corpus/BufferedProbe.java ("Stream closed" family,
            // mark/readlimit invalidation, the skip buffer-vs-inner law).
            | "java/io/BufferedInputStream"
            // Stream wave 4: PushbackInputStream — slots (inherited in,
            // pushback buf ref, pos), behavior in io.rs; pinned by
            // corpus/PushbackProbe.java.
            | "java/io/PushbackInputStream"
            // Corpus wave 3 (JAMA): PrintWriter (real subset, natives.rs),
            // plus the registered-loud trio (Vector / StreamTokenizer /
            // DecimalFormat family — constructors resolve, behavior natives
            // throw naming the gap).
            | "java/io/PrintWriter"
            | "java/util/Vector"
            | "java/io/StreamTokenizer"
            | "java/text/DecimalFormat"
            | "java/text/DecimalFormatSymbols"
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

/// Parse one `.class` file and append it to the program universe.
/// Shared by explicit inputs (M1) and classpath closure loading (M3
/// Wave 1) — identical validation for both sources.
fn parse_and_add(
    bytes: &[u8],
    classes: &mut Vec<ClassPlan>,
    user_index: &mut BTreeMap<String, usize>,
    static_slots: &mut u32,
    source: &str,
) -> Result<()> {
    use crate::sha256::sha256_hex;
    let cf = ClassFile::parse(bytes).map_err(cf_err)?;
    let name = cf.this_class.clone();
    if classpath::is_jdk_namespace(&name) {
        return Err(CodegenError::Semantic {
            class: name.clone(),
            message: "refusing to compile a JDK builtin class; it is provided by j2r-runtime"
                .to_string(),
        });
    }
    if user_index.contains_key(&name) {
        return Err(CodegenError::Semantic {
            class: name.clone(),
            message: format!("duplicate class {name} (loaded twice — from {source})"),
        });
    }
    let module = sanitize_module(&name);
    // T03 closure cache: extract the referenced-user-class set once, here,
    // so the closure worklist never re-walks a constant pool.
    let refs = classpath::referenced_user_classes(&cf);
    if classes.iter().any(|c| c.module == module) {
        return Err(CodegenError::Semantic {
            class: name.clone(),
            message: format!("module name collision for {name} (two classes sanitize to {module})"),
        });
    }
    let is_interface = cf.is_interface();
    let is_abstract = cf.is_abstract() || is_interface;
    let id = classes.len() as u32;

    // fields: layout + static slots
    let mut fields = Vec::new();
    let mut instance_count = 0usize;
    for f in &cf.fields {
        let is_static = f.is_static();
        let (static_slot, instance_slot) = if is_static {
            let s = *static_slots;
            *static_slots += 1;
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
        simple_name: cf.inner_name.clone(),
        module,
        id,
        base_slots: 0,
        total_slots: 0,
        fields,
        methods: Vec::new(),
        clinit_fn: None,
        concats: BTreeMap::new(),
        lambdas: BTreeMap::new(),
        static_monitor: None,
        refs,
        input_sha: sha256_hex(bytes),
        cf,
    });
    Ok(())
}

pub fn compile(opts: &Options) -> Result<Program> {
    if opts.inputs.is_empty() && opts.classpath.is_empty() {
        return Err(CodegenError::Semantic {
            class: "-".to_string(),
            message: "no input .class files and no classpath".to_string(),
        });
    }

    // 1. parse ---------------------------------------------------------
    let mut classes: Vec<ClassPlan> = Vec::new();
    let mut user_index: BTreeMap<String, usize> = BTreeMap::new();
    let mut static_slots = 0u32;

    for path in &opts.inputs {
        let bytes = std::fs::read(path).map_err(CodegenError::Io)?;
        parse_and_add(
            &bytes,
            &mut classes,
            &mut user_index,
            &mut static_slots,
            &format!("input file {}", path.display()),
        )?;
    }

    // 1b. classpath closure (M3 Wave 1, M3_PLAN §4.2) ------------------
    if !opts.classpath.is_empty() {
        let cp =
            classpath::Classpath::new(&opts.classpath).map_err(|e| CodegenError::Semantic {
                class: "-".to_string(),
                message: e.message,
            })?;

        // Seed: with no explicit inputs the main class starts the walk.
        if opts.inputs.is_empty() {
            let main = opts
                .main_class
                .as_deref()
                .ok_or_else(|| CodegenError::Semantic {
                    class: "-".to_string(),
                    message: "compiling from a classpath requires --main-class (nothing else seeds the closure)"
                        .to_string(),
                })?;
            let main = to_internal(main);
            let src = cp.find(&main).ok_or_else(|| CodegenError::Semantic {
                class: main.clone(),
                message: format!("main class {main} not found on the classpath"),
            })?;
            let bytes = cp.read(&src).map_err(|e| CodegenError::Semantic {
                class: main.clone(),
                message: e.message,
            })?;
            parse_and_add(
                &bytes,
                &mut classes,
                &mut user_index,
                &mut static_slots,
                &format!("classpath source {src}"),
            )?;
        }

        // Transitive closure over CONSTANT_Class references.
        let mut processed = 0usize;
        loop {
            let mut wanted: BTreeSet<String> = BTreeSet::new();
            for c in &classes {
                // T03: cached at parse time — no constant-pool re-walk here.
                for name in &c.refs {
                    if !user_index.contains_key(name) {
                        wanted.insert(name.clone());
                    }
                }
            }
            if wanted.is_empty() {
                break;
            }
            for name in wanted {
                let referer = classes
                    .iter()
                    .find(|c| c.refs.contains(&name))
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| "<unknown>".to_string());
                let src = cp.find(&name).ok_or_else(|| CodegenError::Semantic {
                    class: referer.clone(),
                    message: format!(
                        "class {name} is referenced but not found on the classpath (referenced by {referer})"
                    ),
                })?;
                let bytes = cp.read(&src).map_err(|e| CodegenError::Semantic {
                    class: referer.clone(),
                    message: e.message,
                })?;
                parse_and_add(
                    &bytes,
                    &mut classes,
                    &mut user_index,
                    &mut static_slots,
                    &format!("classpath source {src}"),
                )?;
            }
            processed += 1;
            if processed > 100_000 {
                return Err(CodegenError::Semantic {
                    class: "-".to_string(),
                    message: "classpath closure did not converge (internal bug)".to_string(),
                });
            }
        }
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

    let mut static_monitor_count: u32 = 0;

    // 4. decode + synthesize clinit ----------------------------------------
    for ci in 0..prog.classes.len() {
        let (methods, clinit_fn, concats, lambdas) = plan_methods(&prog, ci)?;
        let plan = &mut prog.classes[ci];
        plan.methods = methods;
        plan.clinit_fn = clinit_fn;
        plan.concats = concats;
        plan.lambdas = lambdas;
        // G2.6 (console.md F6, REVIEW_CHECKLIST §G2.6 arm a): collapse
        // javac's array-literal clinit fill runs into ONE literal-array
        // instruction BEFORE verify/emit, so the emitted clinit is one
        // constructor call over a const-evaluated static instead of 4N
        // interpreter statements (the measured MIR/borrow-check wall:
        // InfTree <clinit> was 12,354 emitted lines). Byte-identical by
        // construction; every refused shape keeps the old form.
        for m in &mut plan.methods {
            crate::peephole::collapse_array_fill_runs(&mut m.instrs);
        }
        if plan.methods.iter().any(|m| m.is_sync && m.is_static) {
            plan.static_monitor = Some(static_monitor_count);
            static_monitor_count += 1;
        }
    }

    // 5. verify every method body (before emitting any code) ---------------
    // Malformed bytecode must fail here with a structured diagnostic, not
    // reach the panic paths of the generated code or runtime.
    for c in &prog.classes {
        verify::verify_class(&prog, c)?;
    }

    // 6. validate every instruction ----------------------------------------
    for c in &prog.classes {
        validate_class(&prog, c)?;
    }

    // 7. main class ---------------------------------------------------------
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
            // Programmatic default only (first declared main in input
            // order). The `j2r test` differential path (audit A3) now
            // always passes an explicit --main-class selected once for
            // BOTH sides, so the JVM run and the native run cannot pick
            // different mains.
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
    let mut lambdas = BTreeMap::new();

    for m in &c.cf.methods {
        // ACC_SYNCHRONIZED on a method (0x0020). Since the monitor wave the
        // emitter wraps such methods with a monitor acquisition — instance
        // methods on the receiver, static methods on the class's synthetic
        // monitor object (M3_PLAN §7.1). Abstract/interface methods ignore
        // the flag per JVMS.
        let is_sync = !c.is_interface && m.access & j2r_classfile::flags::ACC_SYNCHRONIZED != 0;
        if m.code.is_none() {
            if m.is_abstract() {
                continue; // abstract: no body to compile
            }
            if m.access & j2r_classfile::flags::ACC_NATIVE != 0 {
                // T12 12c (docs/M5_JNI_NATIVES.md §4/§5): a native method
                // has no bytecode body; its implementation arrives via
                // the binding registry. Plan it WITHOUT instrs (the
                // emitter's native branch emits the call-through body;
                // the lookup table omits it) and record the metadata row.
                methods.push(MethodPlan {
                    name: m.name.clone(),
                    desc: m.descriptor.clone(),
                    is_static: m.is_static(),
                    is_native: true,
                    is_sync: false,
                    fn_name: method_fn_name(&m.name, &m.descriptor),
                    instrs: Vec::new(),
                    exception_table: Vec::new(),
                    max_locals: 0,
                    max_stack: 0,
                });
                continue;
            }
            return Err(CodegenError::Unsupported {
                class: c.name.clone(),
                method: format!("{}{}", m.name, m.descriptor),
                pc: 0,
                what: "method has no Code attribute and is neither abstract nor native".to_string(),
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

        // invokedynamic sites, dispatched on the bootstrap class
        for (pc, instr) in &instrs {
            if let ir::Instr::InvokeDynamic { index } = instr {
                let what = |what: String| CodegenError::Unsupported {
                    class: c.name.clone(),
                    method: format!("{}{}", m.name, m.descriptor),
                    pc: *pc,
                    what,
                };
                let bcls = crate::lambda::bootstrap_class_of(c, *index).map_err(what)?;
                match bcls.as_str() {
                    crate::lambda::LMF_CLASS => {
                        let plan = crate::lambda::resolve_lambda(prog, ci, *index).map_err(what)?;
                        lambdas.insert(*index, plan);
                    }
                    "java/lang/invoke/StringConcatFactory" => {
                        let plan = resolve_concat(&prog.classes[ci], *index).map_err(what)?;
                        concats.insert(*index, plan);
                    }
                    other => {
                        return Err(what(format!("unsupported invokedynamic bootstrap {other}")));
                    }
                }
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
            is_native: false,
            is_sync,
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

    // ConstantValue static finals are NOT compiled into <clinit> code.
    // javac records their value in the ConstantValue attribute and inlines
    // reads; the JVM sets them during class preparation, before any
    // <clinit> code runs. J2R mirrors that by placing the values in the
    // generated crate's static pool at startup (see `statics::set_constants`
    // in the emitter). Synthesizing clinit *code* here was incorrect: the
    // appended stores landed after the method's `return` (dead code), and
    // ConstantValue-only classes produced a clinit without a terminator.

    Ok((methods, clinit_fn, concats, lambdas))
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
                j2r_runtime::Value::Str(std::sync::Arc::new(s.clone()))
            }
            // MethodType/MethodHandle/Class constants never appear as concat
            // recipe constants in javac output; they belong to the
            // LambdaMetafactory path, which is dispatched earlier.
            j2r_classfile::BootstrapArgument::MethodType(_)
            | j2r_classfile::BootstrapArgument::MethodHandle { .. }
            | j2r_classfile::BootstrapArgument::Class(_) => {
                return Err("unsupported concat bootstrap constant".to_string())
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
///
/// The walk covers the superclass chain **and the interface hierarchy**:
/// JVMS 5.4.3.3 resolves `invokevirtual` through superinterfaces too —
/// javac emits the receiver's compile-time type as the methodref owner
/// even when the method is declared in an interface the class implements
/// (caught by the JarSuper differential fixture, M3 Wave 1).
fn validate_virtual(prog: &Program, declared: &str, name: &str, desc: &str) -> Result<()> {
    // Array receivers (corpus wave 2, JZlib `$values()`): every array type
    // is a subtype of Object with a public `clone()` (JVMS 2.15) plus the
    // Object methods — javac emits ordinary invokevirtual sites against the
    // array class.
    if declared.starts_with('[') {
        const ARRAY_OK: &[(&str, &str)] = &[
            ("clone", "()Ljava/lang/Object;"),
            ("getClass", "()Ljava/lang/Class;"),
            ("hashCode", "()I"),
            ("equals", "(Ljava/lang/Object;)Z"),
            ("toString", "()Ljava/lang/String;"),
        ];
        if ARRAY_OK.iter().any(|(n, d)| *n == name && *d == desc) {
            return Ok(());
        }
    }
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut builtin_frontier: Vec<String> = Vec::new();
    queue.push_back(declared.to_string());
    while let Some(cur) = queue.pop_front() {
        if !seen.insert(cur.clone()) {
            continue;
        }
        if prog.is_user(&cur) {
            let c = prog.class(&cur).unwrap();
            let declared = c.cf.method(name, desc).is_some();
            let compiled = c.method(name, desc).is_some();
            if declared || compiled {
                return Ok(()); // abstract declarations dispatch from the runtime class
            }
            if let Some(s) = &c.super_name {
                queue.push_back(s.clone());
            }
            for itf in &c.interfaces {
                queue.push_back(itf.clone());
            }
        } else {
            builtin_frontier.push(cur);
        }
    }
    // Continue the walk from every builtin ancestor reached (its own
    // superclass chain may still declare a native implementation).
    for start in builtin_frontier {
        let mut cur = start;
        for _ in 0..64 {
            if natives::lookup_exact(&cur, name, desc).is_some() {
                return Ok(());
            }
            match builtin::builtin_parent_of(&cur) {
                Some(p) => cur = p.to_string(),
                None => break,
            }
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

/// Resolves an INSTANCE field access to its absolute object-slot index,
/// walking user classes and then builtin ancestors. The builtin case
/// (corpus wave 2, JZlib): user stream subclasses touch the protected
/// `FilterOutputStream.out` / `FilterInputStream.in` fields by name —
/// `builtin_field_slot` names them, and the absolute slot is the owner's
/// inherited base (its parent chain's total, exactly how the layout pass
/// computes `base_slots`) plus the field's own index.
pub fn resolve_field_slot(
    prog: &Program,
    start: &str,
    field: &str,
    want_static: bool,
) -> Result<usize> {
    let mut cur = start.to_string();
    for _ in 0..64 {
        if prog.is_user(&cur) {
            let c = prog.class(&cur).unwrap();
            if let Some(f) = c.fields.iter().find(|f| f.name == field) {
                if f.is_static != want_static {
                    return Err(CodegenError::Semantic {
                        class: start.to_string(),
                        message: format!(
                            "field {field} of {cur} is {} but accessed as {}",
                            if f.is_static { "static" } else { "instance" },
                            if want_static { "static" } else { "instance" }
                        ),
                    });
                }
                return Ok(if want_static {
                    f.static_slot.expect("static slot assigned") as usize
                } else {
                    f.instance_slot.expect("instance slot assigned")
                });
            }
            match &c.super_name {
                Some(s) => cur = s.clone(),
                None => break,
            }
        } else {
            if !want_static {
                if let Some(own) = builtin::builtin_field_slot(&cur, field) {
                    let base = match builtin::builtin_parent_of(&cur) {
                        Some(p) => builtin::builtin_total_slots(p).ok_or_else(|| {
                            CodegenError::Semantic {
                                class: start.to_string(),
                                message: format!("unknown builtin ancestor {p} of {cur}"),
                            }
                        })?,
                        None => 0,
                    };
                    return Ok(base + own);
                }
            }
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

pub fn resolve_field_slot_pub(
    prog: &Program,
    start: &str,
    field: &str,
    want_static: bool,
) -> Result<usize> {
    resolve_field_slot(prog, start, field, want_static)
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
        ir::Instr::Ldc(ir::Ldc::Class(_)) => {
            // M3 Wave 5: emitted as an interned `Class` object (M3_PLAN §8.1).
            Ok(())
        }
        ir::Instr::GetStatic { class, field, .. } => {
            if class == "java/lang/System" && (field == "out" || field == "err") {
                return Ok(());
            }
            // Corpus wave 3 (JAMA): the Locale constant objects — builtin
            // materialized at emit time (only the referenced subset).
            if class == "java/util/Locale" && field == "US" {
                return Ok(());
            }
            // The nine `X.class` pseudo-fields (M3_PLAN §8.1): javac
            // compiles primitive class literals to `getstatic X.TYPE`, so
            // they are modelled as statics returning the interned Class
            // object of the primitive descriptor.
            if field == "TYPE" && primitive_type_field(class).is_some() {
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
            resolve_field_slot(prog, class, field, false).map(|_| ())
        }
        ir::Instr::PutField { class, field, .. } => {
            resolve_field_slot(prog, class, field, false).map(|_| ())
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
            if !c.concats.contains_key(index) && !c.lambdas.contains_key(index) {
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
            // Interface-typed arrays are ordinary ref arrays: the F4/F5
            // wave registered Comparable by hand in builtin_parent_of, but
            // the general law is "the element type must be a user class,
            // Object, an array descriptor, or a known builtin CLASS OR
            // INTERFACE" (aastore's ASE check already walks the interface
            // subtype graph, so a stored value is checked against the
            // component type at the full strength of the type law).
            if prog.is_user(elem)
                || elem == "java/lang/Object"
                || builtin::builtin_parent_of(elem).is_some()
                || builtin::builtin_is_interface(elem)
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
    compile_to_crate_stats(opts).map(|(dir, _stats)| dir)
}

/// Full pipeline with build-manifest statistics (M5 T06/T07): compiles,
/// emits with per-class reuse + write-if-changed (single-crate or
/// partitioned workspace — both paths maintain the manifest), and returns
/// both the crate directory and what the emission actually did.
pub fn compile_to_crate_stats(opts: &Options) -> Result<(PathBuf, crate::manifest::EmitStats)> {
    let prog = compile(opts)?;
    let stats = emit::emit_crate(&prog, opts)?;
    Ok((opts.out_dir.clone(), stats))
}

/// Public wrappers used by the emitter (it needs the same resolution logic
/// the validator used).
pub fn resolve_call_pub(prog: &Program, start: &str, name: &str, desc: &str) -> Result<CallTarget> {
    resolve_call(prog, start, name, desc)
}

/// The boxed-class `TYPE` pseudo-fields: `getstatic java/lang/Integer.TYPE`
/// is how javac compiles `int.class` at literal positions (oracle javap,
/// M3_EVIDENCE.md wave-5). Returns the primitive descriptor the interned
/// `Class` object carries.
pub fn primitive_type_field(class: &str) -> Option<&'static str> {
    match class {
        "java/lang/Integer" => Some("I"),
        "java/lang/Long" => Some("J"),
        "java/lang/Float" => Some("F"),
        "java/lang/Double" => Some("D"),
        "java/lang/Byte" => Some("B"),
        "java/lang/Short" => Some("S"),
        "java/lang/Character" => Some("C"),
        "java/lang/Boolean" => Some("Z"),
        "java/lang/Void" => Some("V"),
        _ => None,
    }
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
