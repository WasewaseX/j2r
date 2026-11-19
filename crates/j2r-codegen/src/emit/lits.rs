//! `emit`::lits — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use crate::ir::{ArithOp, CmpKind, Conv, NumKind};
use crate::{FieldPlan, MethodPlan, Program};

/// Rust float literal that is valid for every value (NaN/inf included).
pub(crate) fn f64_lit(v: f64) -> String {
    if v.is_nan() {
        "f64::NAN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 {
            "f64::INFINITY".to_string()
        } else {
            "f64::NEG_INFINITY".to_string()
        }
    } else {
        format!("{v:?}")
    }
}

pub(crate) fn f32_lit(v: f32) -> String {
    if v.is_nan() {
        "f32::NAN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 {
            "f32::INFINITY".to_string()
        } else {
            "f32::NEG_INFINITY".to_string()
        }
    } else {
        format!("{v:?}")
    }
}

pub(crate) fn next_pc_of(m: &MethodPlan, pc: u32) -> u32 {
    for (p, _) in &m.instrs {
        if *p > pc {
            return *p;
        }
    }
    pc + 1
}

pub(crate) fn returns_void(desc: &str) -> bool {
    matches!(
        j2r_runtime::parse_method_desc(desc),
        Some((_, j2r_runtime::TypeDesc::Void))
    )
}

pub(crate) fn arg_count(desc: &str) -> usize {
    let (params, _) = j2r_runtime::parse_method_desc(desc).expect("valid descriptor");
    params.len()
}

pub(crate) fn constant_literal(v: &j2r_runtime::Value) -> String {
    match v {
        j2r_runtime::Value::Int(i) => format!("{RT}::Value::Int({i})"),
        j2r_runtime::Value::Long(l) => format!("{RT}::Value::Long({l}i64)"),
        j2r_runtime::Value::Float(f) => format!("{RT}::Value::Float({})", f32_lit(*f)),
        j2r_runtime::Value::Double(d) => format!("{RT}::Value::Double({})", f64_lit(*d)),
        j2r_runtime::Value::Str(s) => {
            format!("{RT}::Value::Str(::std::sync::Arc::new({s:?}.to_string()))")
        }
        other => unreachable!("concat constant {other:?}"),
    }
}

/// G4.2 arm (b) — the JMM boundary marker. Returns the comment text for
/// every generated access to a field whose input bytecode flags promise
/// semantics the runtime does not provide: `ACC_VOLATILE` (no Java
/// volatile ordering) and `ACC_FINAL` without a `ConstantValue` (no
/// final-field freeze; folded constants are JMM-safe). The marker makes
/// the incompatibility visible AT THE USE SITE (docs/RUNTIME_SAFETY.md
/// §6), and CI gate-23 (scripts/check_jmm_boundary.py) fails loudly when
/// a boundary field lacks its markers or the `src/jmm_boundary.rs`
/// sidecar diverges from the input facts.
pub(crate) fn jmm_boundary_marker(f: &FieldPlan, class: &str) -> Option<String> {
    let kind = if f.is_volatile {
        "volatile"
    } else if f.is_final {
        "final"
    } else {
        return None;
    };
    Some(format!(
        "// J2R-JMM-BOUNDARY: {class}.{} is {kind} in the input bytecode; this access is a PLAIN non-atomic slot access — no Java volatile ordering / no final-field freeze (docs/RUNTIME_SAFETY.md §6).",
        f.name
    ))
}

/// `ensure` calls for the user-class chain from the topmost user ancestor
/// down to `start` — mirrors Java's rule that initializing a class first
/// initializes its superclasses; the walk also covers superinterfaces
/// (JVMS 5.4.3.2 + 5.5: an inherited static resolves to its declaring
/// interface, whose <clinit> must run before the access).
pub(crate) fn ensure_calls(prog: &Program, start: &str) -> Vec<(u32, String)> {
    // BFS over user superclasses AND superinterfaces. JVMS 5.4.3.2 field
    // resolution + 5.5 active use: an inherited static (javac emits the
    // Fieldref with the ACCESSING class as owner) resolves to its
    // declaring interface, whose <clinit> must run before the access.
    // The reversed chain keeps parents first; the runtime ensure is
    // idempotent, so shared-interface diamonds are safe.
    let mut chain: Vec<String> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(start.to_string());
    for _ in 0..256 {
        let Some(cur) = queue.pop_front() else { break };
        if !seen.insert(cur.clone()) {
            continue;
        }
        if prog.is_user(&cur) {
            chain.push(cur.clone());
            let c = prog.class(&cur).unwrap();
            for i in &c.interfaces {
                queue.push_back(i.clone());
            }
            if let Some(s) = &c.super_name {
                queue.push_back(s.clone());
            }
        }
    }
    chain.reverse();
    chain
        .into_iter()
        .map(|name| {
            let c = prog.class(&name).expect("user class");
            let expr = match &c.clinit_fn {
                Some(fname) => format!(
                    "::core::option::Option::Some(crate::classes::{}::{} as ::j2r_runtime::MethodFn)",
                    c.module, fname
                ),
                None => "::core::option::Option::None".to_string(),
            };
            (c.id, expr)
        })
        .collect()
}

/// The allocation sequence for `new {class}`, shared by the default and
/// typed emitters (one source of truth for the JLS §4.12.5 default-value
/// law): the `<clinit>` ensure calls, the `new_obj`/`new_builtin` call
/// bound to `__nobj`, and the field-default spec. The refresh + gc_point
/// stay at the call site (identical for both emitters).
pub(crate) fn new_obj_lines(prog: &Program, class: &str, fault: &str, out: &mut Vec<String>) {
    if prog.is_user(class) {
        let target = prog.class(class).unwrap();
        let total = target.total_slots;
        // JLS §4.12.5: instance fields carry default values (0, false,
        // null) between allocation and the constructor body. The spec is
        // the plan's own `default_spec` (NEW-INSTANCE: one computation,
        // shared with the META tables Constructor.newInstance reads at
        // runtime) and spans the WHOLE user-ancestor chain. The
        // builtin-parent region below the root user class stays Void:
        // those are the builtin's internals, written by its constructors
        // before any user code can read them.
        for (id, expr) in ensure_calls(prog, class) {
            // CLINIT-THROW: the new's clinit throw propagates before the
            // allocation.
            out.push(format!(
                "{RT}::j2r_try!('run, thrown, fault_pc, {fault}, crate::statics::ensure({id}u32, {expr}));"
            ));
        }
        out.push(format!(
            "let __nobj = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::object::new_obj({class:?}, {total}usize));"
        ));
        out.push(format!(
            "{RT}::object::set_defaults(&__nobj, {:?});",
            target.default_spec
        ));
    } else {
        out.push(format!(
            "let __nobj = {RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::object::new_builtin({class:?}));"
        ));
    }
}

/// The JLS 12.4.2 launcher line for the app main template: ensure the
/// main class's `<clinit>` before main() runs. Empty when the class has
/// none. `crate_path` is `classes::` prefixed from the app crate root in
/// both emission modes (single-crate: `{lib_crate}::`; partitioned: the
/// owning partition crate `pN::`).
pub(crate) fn main_clinit_ensure_line(lib_crate: &str, m: &crate::MainInfo) -> String {
    match &m.clinit_fn {
        Some(fname) => format!(
            "{lib_crate}::statics::ensure({}u32, ::core::option::Option::Some({lib_crate}::classes::{}::{} as ::j2r_runtime::MethodFn))",
            m.class_id, m.module, fname
        ),
        None => "::core::result::Result::Ok(())".to_string(),
    }
}

/// The CLINIT-THROW name registration line: internal class names indexed
/// by class id, registered before any code runs so an errant clinit's
/// NCDFE message can carry the binary class name (Temurin's shape). Both
/// emission modes call it right after `init_pool`.
pub(crate) fn clinit_names_line(prog: &Program) -> String {
    let mut names: Vec<(u32, &str)> = prog
        .classes
        .iter()
        .map(|c| (c.id, c.name.as_str()))
        .collect();
    names.sort_by_key(|(id, _)| *id);
    let list: Vec<String> = names.iter().map(|(_, n)| format!("{n:?}")).collect();
    format!(
        "::j2r_runtime::clinit::register_class_names(&[{}]);",
        list.join(", ")
    )
}

pub(crate) fn emit_arith(op: ArithOp, kind: NumKind, fault: &str, out: &mut Vec<String>) {
    use NumKind::*;
    // pops: (reader fn, Value ctor); long shifts pop an int shift amount
    match kind {
        Int | Long => {
            emit_int_or_long_arith(op, kind == Long, fault, out);
        }
        Float | Double => {
            let pop = if kind == Float {
                "::j2r_runtime::pop_float"
            } else {
                "::j2r_runtime::pop_double"
            };
            let ctor = if kind == Float { "Float" } else { "Double" };
            match op {
                ArithOp::Neg => out.push(format!(
                    "let __a = {pop}(&mut stack); stack.push({RT}::Value::{ctor}(-__a));"
                )),
                ArithOp::Add => out.push(format!(
                    "let __b = {pop}(&mut stack); let __a = {pop}(&mut stack); stack.push({RT}::Value::{ctor}(__a + __b));"
                )),
                ArithOp::Sub => out.push(format!(
                    "let __b = {pop}(&mut stack); let __a = {pop}(&mut stack); stack.push({RT}::Value::{ctor}(__a - __b));"
                )),
                ArithOp::Mul => out.push(format!(
                    "let __b = {pop}(&mut stack); let __a = {pop}(&mut stack); stack.push({RT}::Value::{ctor}(__a * __b));"
                )),
                ArithOp::Div => out.push(format!(
                    "let __b = {pop}(&mut stack); let __a = {pop}(&mut stack); stack.push({RT}::Value::{ctor}(__a / __b));"
                )),
                ArithOp::Rem => out.push(format!(
                    "let __b = {pop}(&mut stack); let __a = {pop}(&mut stack); stack.push({RT}::Value::{ctor}(__a % __b));"
                )),
                _ => unreachable!("bit/shift arithmetic on floats"),
            }
        }
        Byte | Char | Short => unreachable!("arith on narrow kind"),
    }
}

pub(crate) fn emit_int_or_long_arith(
    op: ArithOp,
    is_long: bool,
    fault: &str,
    out: &mut Vec<String>,
) {
    let pop_self = if is_long {
        "::j2r_runtime::pop_long"
    } else {
        "::j2r_runtime::pop_int"
    };
    let ctor = if is_long { "Long" } else { "Int" };
    match op {
        ArithOp::Neg => out.push(format!(
            "let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}(__a.wrapping_neg()));"
        )),
        ArithOp::Add => out.push(format!(
            "let __b = {pop_self}(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}(__a.wrapping_add(__b)));"
        )),
        ArithOp::Sub => out.push(format!(
            "let __b = {pop_self}(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}(__a.wrapping_sub(__b)));"
        )),
        ArithOp::Mul => out.push(format!(
            "let __b = {pop_self}(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}(__a.wrapping_mul(__b)));"
        )),
        ArithOp::Div => out.push(format!(
            "let __b = {pop_self}(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}({RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::{}(__a, __b))));",
            if is_long { "long_div" } else { "int_div" }
        )),
        ArithOp::Rem => out.push(format!(
            "let __b = {pop_self}(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}({RT}::j2r_try!('run, thrown, fault_pc, {fault}, {RT}::{}(__a, __b))));",
            if is_long { "long_rem" } else { "int_rem" }
        )),
        ArithOp::And | ArithOp::Or | ArithOp::Xor => {
            let rust_op = match op {
                ArithOp::And => "&",
                ArithOp::Or => "|",
                _ => "^",
            };
            out.push(format!(
                "let __b = {pop_self}(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}(__a {rust_op} __b));"
            ));
        }
        ArithOp::Shl | ArithOp::Shr | ArithOp::UShr => {
            // shift amount is always an int; mask per JVMS
            let mask = if is_long { 63 } else { 31 };
            let shift_fn = match op {
                ArithOp::Shl => "wrapping_shl",
                ArithOp::Shr => "wrapping_shr",
                _ => "wrapping_shr",
            };
            if op == ArithOp::UShr {
                // logical shift on the unsigned reinterpretation
                let uty = if is_long { "u64" } else { "u32" };
                out.push(format!(
                    "let __b = ::j2r_runtime::pop_int(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}(((__a as {uty}).{shift_fn}((__b & {mask}) as u32)) as {}));",
                    if is_long { "i64" } else { "i32" }
                ));
            } else {
                out.push(format!(
                    "let __b = ::j2r_runtime::pop_int(&mut stack); let __a = {pop_self}(&mut stack); stack.push({RT}::Value::{ctor}(__a.{shift_fn}((__b & {mask}) as u32)));"
                ));
            }
        }
    }
}

pub(crate) fn emit_cmp(kind: CmpKind, out: &mut Vec<String>) {
    match kind {
        CmpKind::LCmp => {
            out.push(
                "let __b = ::j2r_runtime::pop_long(&mut stack); let __a = ::j2r_runtime::pop_long(&mut stack);"
                    .into(),
            );
            out.push(
                "stack.push(::j2r_runtime::Value::Int(match __a.cmp(&__b) { ::core::cmp::Ordering::Less => -1i32, ::core::cmp::Ordering::Greater => 1i32, ::core::cmp::Ordering::Equal => 0i32 }));"
                    .into(),
            );
        }
        CmpKind::FCmpL | CmpKind::FCmpG | CmpKind::DCmpL | CmpKind::DCmpG => {
            let pop = if matches!(kind, CmpKind::FCmpL | CmpKind::FCmpG) {
                "::j2r_runtime::pop_float"
            } else {
                "::j2r_runtime::pop_double"
            };
            let nan_val: i32 = if matches!(kind, CmpKind::FCmpG | CmpKind::DCmpG) {
                1
            } else {
                -1
            };
            out.push(format!(
                "let __b = {pop}(&mut stack); let __a = {pop}(&mut stack);"
            ));
            out.push(format!(
                "stack.push(::j2r_runtime::Value::Int(if __a.is_nan() || __b.is_nan() {{ {nan_val}i32 }} else {{ match __a.partial_cmp(&__b) {{ ::core::option::Option::Some(::core::cmp::Ordering::Less) => -1i32, ::core::option::Option::Some(::core::cmp::Ordering::Greater) => 1i32, _ => 0i32 }} }}));"
            ));
        }
    }
}

pub(crate) fn emit_convert(conv: Conv, out: &mut Vec<String>) {
    use NumKind::*;
    let from_pop = match conv.from {
        Int => "::j2r_runtime::pop_int",
        Long => "::j2r_runtime::pop_long",
        Float => "::j2r_runtime::pop_float",
        Double => "::j2r_runtime::pop_double",
        _ => unreachable!("conversion source"),
    };
    // Rust's `as` casts match Java exactly here: float/double -> int/long
    // saturate with NaN -> 0; int -> narrow types wrap; int -> char
    // zero-extends.
    let (ctor, cast) = match (conv.from, conv.to) {
        (Int, Long) => ("Long", " as i64"),
        (Int, Float) => ("Float", " as f32"),
        (Int, Double) => ("Double", " as f64"),
        (Int, Byte) => ("Int", " as i8 as i32"),
        (Int, Char) => ("Int", " as u16 as i32"),
        (Int, Short) => ("Int", " as i16 as i32"),
        (Long, Int) => ("Int", " as i32"),
        (Long, Float) => ("Float", " as f32"),
        (Long, Double) => ("Double", " as f64"),
        (Float, Int) => ("Int", " as i32"),
        (Float, Long) => ("Long", " as i64"),
        (Float, Double) => ("Double", " as f64"),
        (Double, Int) => ("Int", " as i32"),
        (Double, Long) => ("Long", " as i64"),
        (Double, Float) => ("Float", " as f32"),
        _ => unreachable!("conversion pair"),
    };
    out.push(format!(
        "let __a = {from_pop}(&mut stack); stack.push({RT}::Value::{ctor}(__a{cast}));"
    ));
}

// ------------------------------------------------------- helpful-NPE tests
//
// The grammar laws, oracle-pinned by NpeShapeProbe2 (the S1-S8 fixture
// matrix) and the extended captures in scripts/evidence/helpful_npe/
// (standing run 4). The end-to-end differential proof is the fixture;
// these pins hold the EMIT-TIME computation to the captured forms.
