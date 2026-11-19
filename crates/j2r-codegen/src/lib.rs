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
pub mod dispatch_table;
pub mod emit;
pub mod ir;
pub mod lambda;
pub mod manifest;
pub mod partition;
pub mod peephole;
pub mod readiness;
pub mod sha256;
pub mod signature;
pub mod specialize;
pub mod surface;
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
use j2r_runtime::TypeDesc;
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

// G2.3 stage profiler (F1): J2R_PROFILE_STAGES=1 turns on coarse
// wall-clock stage accumulation; `stages_report()` renders the table
// after `compile_to_crate_stats`. Off by default (one bool check per
// stage); the machine-checked answer to "where does the 20k emit
// residual go" — names the hot stage, never adjectives.
static STAGES: std::sync::Mutex<Option<Vec<(String, f64)>>> = std::sync::Mutex::new(None);

pub fn stages_enabled() -> bool {
    std::env::var_os("J2R_PROFILE_STAGES").is_some()
}

/// Accumulates wall time for one named stage while alive. `None` when
/// profiling is disabled (the guard costs nothing).
pub struct StageGuard {
    name: &'static str,
    t0: std::time::Instant,
}

impl StageGuard {
    pub fn enter(name: &'static str) -> Option<StageGuard> {
        if !stages_enabled() {
            return None;
        }
        Some(StageGuard {
            name,
            t0: std::time::Instant::now(),
        })
    }
}

impl Drop for StageGuard {
    fn drop(&mut self) {
        let w = self.t0.elapsed().as_secs_f64();
        if let Ok(mut g) = STAGES.lock() {
            let v = g.get_or_insert_with(Vec::new);
            match v.last_mut() {
                Some(last) if last.0 == self.name => last.1 += w,
                _ => v.push((self.name.to_string(), w)),
            }
        }
    }
}

/// The accumulated stage table, or `None` when profiling never engaged.
pub fn stages_report() -> Option<String> {
    let g = STAGES.lock().ok()?;
    let v = g.as_ref()?;
    if v.is_empty() {
        return None;
    }
    let mut s = String::from("stage-profile (J2R_PROFILE_STAGES=1):\n");
    for (name, w) in v {
        s.push_str(&format!("  stage={name} wall={w:.2}s\n"));
    }
    Some(s)
}

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
    /// R7 (Opt.md §3 R7) — the measured **NO-GO** lever, kept runnable
    /// behind this default-**off** flag (the M2-CFG precedent): emit the
    /// registry lookup as a FNV-1a 64-bit selector match with the full
    /// key verified inside every arm, instead of the string-tuple match.
    /// The same-session A/B at N=2,169 arms falsified the premise: rustc
    /// 1.99 lowers the string-tuple match to a balanced decision tree
    /// (~27 ns/call) while the selector shapes pay the serial hash walk
    /// first (~216 ns/call, both the tuple-led and the selector-led
    /// emission); evidence scripts/evidence/r7/. Default off keeps the
    /// shipped emission byte-identical to the pre-R7 shape; a future
    /// rustc whose string-match lowering regresses to a linear chain can
    /// flip the flag and re-measure with the same instruments.
    pub emit_hash_dispatch: bool,
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
            emit_hash_dispatch: false,
        }
    }
}

// ---------------------------------------------------------------- model

#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// `ACC_VOLATILE` in the input bytecode (G4.2 arm (b)): the runtime
    /// provides no Java volatile ordering — every generated access site
    /// carries the boundary marker and the sidecar lists the field.
    pub is_volatile: bool,
    /// `ACC_FINAL` in the input bytecode WITHOUT a `ConstantValue` (the
    /// folded constants are JMM-safe): no final-field freeze semantics —
    /// marked like volatile.
    pub is_final: bool,
    /// The raw access flags from the classfile (R2 slice 2:
    /// `Field.getModifiers` — the reflection subset's modifier bits).
    pub modifiers: u16,
    /// The `Signature` attribute (JVMS §4.7.9), raw and unparsed — the
    /// G3.3 signature-pin layer parses it at use sites
    /// ([`signature::parse_field_signature`]). `None` for non-generic
    /// fields.
    pub signature: Option<String>,
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
    /// The raw access flags from the classfile (R2 slice 2:
    /// `Method.getModifiers` — the reflection subset's modifier bits).
    pub modifiers: u16,
    /// The `Signature` attribute (JVMS §4.7.9), raw and unparsed — the
    /// G3.3 signature-pin layer parses it at use sites
    /// ([`signature::parse_method_signature`]). `None` for non-generic
    /// methods.
    pub signature: Option<String>,
    /// The method body's `LocalVariableTypeTable` (JVMS §4.7.14), in
    /// classfile order — where javac records the GENERIC type of a local
    /// (`Map<String, Double>` for the erased `Ljava/util/Map;` slot). The
    /// receiver-pin resolution of the G3.3 rewrite slices looks the
    /// receiver's slot up here.
    pub lvtt: Vec<j2r_classfile::LocalVarType>,
    /// The raw opcodes of the method's array-access instructions, keyed
    /// by pc (the `ir::decode` side-map): the helpful-NPE message layer
    /// derives the array kind from the OPCODE — the JVM's law — and
    /// iaload/baload/caload/saload share `ElemKind::IntGroup` in the IR,
    /// so the opcode distinction rides alongside (standing run 4).
    pub array_ops: std::collections::BTreeMap<u32, u8>,
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
    /// `NestHost` attribute (JVMS §4.7.28): the declared host, `None`
    /// when absent (a top-level class IS its own nest host). The
    /// different-package private-ctor law resolves both sides' hosts
    /// and compares (JEP 181 areNestMates) — the reflection
    /// `Constructor.newInstance` access check consumes it via the
    /// runtime META.
    pub nest_host: Option<String>,
    pub module: String,
    /// Internal class id used by the generated `statics` registry (`u32`,
    /// same rationale as [`FieldPlan::static_slot`]).
    pub id: u32,
    /// Slot index where this class's own instance fields begin.
    pub base_slots: usize,
    /// Total instance slots (including inherited ones).
    pub total_slots: usize,
    /// The JLS §4.12.5 default-slot spec over `[base_slots, total_slots)`
    /// (NEW-INSTANCE): one char per slot — `i`/`j`/`f`/`d`/`r` — the SAME
    /// spec the emitted `new` sites apply, computed once here so the
    /// single-crate META, the partitioned META and `new_obj_lines` share
    /// one source of truth.
    pub default_spec: String,
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
/// Entry point metadata used by the CLI.
pub struct MainInfo {
    pub module: String,
    pub fn_name: String,
    /// The main class's statics id + `<clinit>` fn name: the JVM launcher
    /// initializes the main class BEFORE invoking main (JLS 12.4.2); the
    /// generated app main must ensure it (a main() that touches none of
    /// its own class's statics otherwise runs WITHOUT the `<clinit>` —
    /// found by the EXTERNAL-B wave's Externals fixture, 2026-11-13).
    pub class_id: u32,
    pub clinit_fn: Option<String>,
}

pub fn main_entry(prog: &Program) -> Option<MainInfo> {
    prog.main_class.as_ref().and_then(|name| {
        prog.class(name).and_then(|c| {
            c.method("main", "([Ljava/lang/String;)V")
                .map(|m| MainInfo {
                    module: c.module.clone(),
                    fn_name: m.fn_name.clone(),
                    class_id: c.id,
                    clinit_fn: c.clinit_fn.clone(),
                })
        })
    })
}

// ------------------------------------------------- R5 slice 1: the law tests

// --- round-7 file-split pass: verbatim children below ---
#[path = "lib/plan.rs"]
mod plan;
pub use plan::compile;
pub use plan::compile_unchecked;
pub(crate) use plan::*;
#[path = "lib/layout.rs"]
mod layout;
pub(crate) use layout::*;
#[path = "lib/resolve.rs"]
mod resolve;
pub use resolve::compile_to_crate;
pub use resolve::compile_to_crate_stats;
pub use resolve::count_virtual_sites;
pub use resolve::count_vtable_sites;
pub use resolve::primitive_type_field;
pub use resolve::resolve_call_pub;
pub use resolve::resolve_field_pub;
pub use resolve::resolve_field_slot;
pub use resolve::resolve_field_slot_pub;
pub use resolve::resolve_virtual_target;
pub use resolve::resolve_virtual_target_pub;
pub use resolve::resolve_vtable;
pub use resolve::resolve_vtable_pub;
pub use resolve::virtual_receiver_set;
pub use resolve::VtableArm;
pub(crate) use resolve::*;
#[path = "lib/virtual_resolution_tests.rs"]
mod r5_law_tests;
