//! `typed`::arith — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use crate::emit::returns_void;
use crate::ir::{ArithOp, Conv, NumKind};
use crate::readiness::Kind;

/// The int/long arithmetic arm (verbatim from `emit_arith_typed`,
/// round-5 reshaping): reads-first then the per-op result push; div/rem
/// trap through the try-wrapped helpers, shifts mask to 31/63.
pub(crate) fn emit_arith_intlong_typed(
    ctx: &mut Ctx<'_>,
    pc: u32,
    fault: &str,
    op: ArithOp,
    kind: NumKind,
    vs: &[usize],
) {
    use NumKind as NK;
    let k = vs.len();
    let prim = if kind == NK::Long { Prim::L } else { Prim::I };
    let (result_slot, a_slot, b_slot) = match op {
        ArithOp::Neg => (vs[k - 1], vs[k - 1], vs[k - 1]),
        _ => (vs[k - 2], vs[k - 2], vs[k - 1]),
    };
    // Reads first (the same-kind refill overwrites the operand's
    // variable), then write.
    let a = ctx.read_prim(pc, a_slot, prim);
    let b = if op == ArithOp::Neg {
        String::new()
    } else if op == ArithOp::Shl || op == ArithOp::Shr || op == ArithOp::UShr {
        // The shift amount is always an int, even for long shifts.
        ctx.read_prim(pc, b_slot, Prim::I)
    } else {
        ctx.read_prim(pc, b_slot, prim)
    };
    let new_kind = match kind {
        NK::Long => Kind::Long,
        _ => Kind::Int,
    };
    let ty = prim.rust_ty();
    // Typed operand bindings: the composed reads may be bare
    // literals or leaf reads — without the annotation, Rust
    // inference hits ambiguous `{integer}`/`{float}` types, and
    // the shared `__a`/`__b` names of a LATER op in the same arm
    // would shadow an earlier entry's captured string.
    match op {
        ArithOp::Neg => {
            ctx.out.push(format!("let __a: {ty} = {a};"));
            let target = ctx.refill_target(result_slot, pc, Some(new_kind));
            ctx.push_result(
                result_slot,
                target,
                "__a.wrapping_neg()".to_string(),
                Src::Prim(prim),
                false,
                new_kind,
            );
        }
        ArithOp::Add | ArithOp::Sub | ArithOp::Mul | ArithOp::And | ArithOp::Or | ArithOp::Xor => {
            let rust_op = match op {
                ArithOp::Add => "wrapping_add",
                ArithOp::Sub => "wrapping_sub",
                ArithOp::Mul => "wrapping_mul",
                ArithOp::And => "&",
                ArithOp::Or => "|",
                _ => "^",
            };
            ctx.out
                .push(format!("let __b: {ty} = {b}; let __a: {ty} = {a};"));
            let expr = if rust_op.starts_with("wrapping") {
                format!("__a.{rust_op}(__b)")
            } else {
                format!("__a {rust_op} __b")
            };
            let target = ctx.refill_target(result_slot, pc, Some(new_kind));
            ctx.push_result(result_slot, target, expr, Src::Prim(prim), false, new_kind);
        }
        ArithOp::Div | ArithOp::Rem => {
            let helper = match (op, kind) {
                (ArithOp::Div, NK::Long) => "long_div",
                (ArithOp::Div, _) => "int_div",
                (ArithOp::Rem, NK::Long) => "long_rem",
                _ => "int_rem",
            };
            ctx.out
                .push(format!("let __b: {ty} = {b}; let __a: {ty} = {a};"));
            let div = try_wrap(fault, format!("{RT}::{helper}(__a, __b)"));
            let target = ctx.refill_target(result_slot, pc, Some(new_kind));
            // The trapping divide materializes into the def-time
            // temp (a method-unique `__t{n}`) — a pending result
            // can live across a LATER divide's old `__r` binding.
            ctx.push_result(result_slot, target, div, Src::Prim(prim), false, new_kind);
        }
        ArithOp::Shl | ArithOp::Shr | ArithOp::UShr => {
            let mask = if kind == NK::Long { 63 } else { 31 };
            let shift_fn = if op == ArithOp::Shl {
                "wrapping_shl"
            } else {
                "wrapping_shr"
            };
            // The shift amount is an INT even for long shifts —
            // the two bindings carry DIFFERENT types here.
            ctx.out
                .push(format!("let __b: i32 = {b}; let __a: {} = {a};", ty));
            let expr = if op == ArithOp::UShr {
                // logical shift on the unsigned reinterpretation
                let uty = if kind == NK::Long { "u64" } else { "u32" };
                format!(
                    " ((__a as {uty}).{shift_fn}((__b & {mask}) as u32)) as {}",
                    if kind == NK::Long { "i64" } else { "i32" }
                )
                .trim_start()
                .to_string()
            } else {
                format!("__a.{shift_fn}((__b & {mask}) as u32)")
            };
            let target = ctx.refill_target(result_slot, pc, Some(new_kind));
            // Shifts never trap, but the composed string binds the
            // shared __a/__b names — it materializes into its own
            // temp like every other composed op, or a LATER op's
            // rebinding of those names would re-evaluate this
            // entry's captured string against wrong values.
            ctx.push_result(result_slot, target, expr, Src::Prim(prim), false, new_kind);
        }
    }
}

/// The float/double arithmetic arm (verbatim from `emit_arith_typed`,
/// round-5 reshaping): neg is a bare negate; the rest are the plain FP
/// operators (JLS: no traps on FP div/rem).
pub(crate) fn emit_arith_float_typed(
    ctx: &mut Ctx<'_>,
    pc: u32,
    op: ArithOp,
    kind: NumKind,
    vs: &[usize],
) {
    use NumKind as NK;
    let k = vs.len();
    let prim = if kind == NK::Float { Prim::F } else { Prim::D };
    let (result_slot, a_slot, b_slot) = match op {
        ArithOp::Neg => (vs[k - 1], vs[k - 1], vs[k - 1]),
        _ => (vs[k - 2], vs[k - 2], vs[k - 1]),
    };
    let a = ctx.read_prim(pc, a_slot, prim);
    let b = ctx.read_prim(pc, b_slot, prim);
    let new_kind = if kind == NK::Float {
        Kind::Float
    } else {
        Kind::Double
    };
    let ty = prim.rust_ty();
    let rust_op = match op {
        ArithOp::Neg => {
            ctx.out.push(format!("let __a: {ty} = {a};"));
            let target = ctx.refill_target(result_slot, pc, Some(new_kind));
            ctx.push_result(
                result_slot,
                target,
                "-__a".to_string(),
                Src::Prim(prim),
                false,
                new_kind,
            );
            return;
        }
        ArithOp::Add => "+",
        ArithOp::Sub => "-",
        ArithOp::Mul => "*",
        ArithOp::Div => "/",
        _ => "%",
    };
    ctx.out
        .push(format!("let __b: {ty} = {b}; let __a: {ty} = {a};"));
    let target = ctx.refill_target(result_slot, pc, Some(new_kind));
    // FP arithmetic never traps (JLS: no exceptions on div/rem),
    // but the composed string binds shared names — the result
    // materializes into its own temp like every other op.
    ctx.push_result(
        result_slot,
        target,
        format!("__a {rust_op} __b"),
        Src::Prim(prim),
        false,
        new_kind,
    );
}

pub(crate) fn emit_arith_typed(
    ctx: &mut Ctx<'_>,
    pc: u32,
    fault: &str,
    op: ArithOp,
    kind: NumKind,
    vs: &[usize],
) {
    use NumKind as NK;
    // The unification layer's purity law: int/long division and remainder
    // trap (ArithmeticException — the helper is j2r_try-wrapped), every
    // other arithmetic form is side-effect-free and keeps its expression
    // tree symbolic.
    // Purity note: int/long division and remainder are the only trapping
    // arithmetic (their helper is try-wrapped); every other form is
    // side-effect-free. Every composed result materializes into its own
    // temp regardless — the composed string binds the shared __a/__b
    // names, so a pending entry must never carry it past this op.
    match kind {
        NK::Int | NK::Long => emit_arith_intlong_typed(ctx, pc, fault, op, kind, vs),
        NK::Float | NK::Double => emit_arith_float_typed(ctx, pc, op, kind, vs),
        _ => unreachable!("arith on narrow kind"),
    }
}

pub(crate) fn emit_convert_typed(ctx: &mut Ctx<'_>, pc: u32, conv: Conv, vs: &[usize]) {
    use NumKind as NK;
    let k = vs.len();
    // `to_prim` is deliberately unused: the CAST uses the descriptor kinds
    // (the match below) and the storage kind is the from-kind for narrow
    // targets, decided by `refill_target` — the pair match pins legality.
    let (from_prim, _to_prim) = match (conv.from, conv.to) {
        (NK::Int, NK::Long) => (Prim::I, Prim::L),
        (NK::Int, NK::Float) => (Prim::I, Prim::F),
        (NK::Int, NK::Double) => (Prim::I, Prim::D),
        (NK::Int, NK::Byte) | (NK::Int, NK::Char) | (NK::Int, NK::Short) => (Prim::I, Prim::I),
        (NK::Long, NK::Int) | (NK::Long, NK::Float) | (NK::Long, NK::Double) => (Prim::L, Prim::L),
        (NK::Float, NK::Int) | (NK::Float, NK::Long) => (Prim::F, Prim::F),
        (NK::Float, NK::Double) => (Prim::F, Prim::D),
        (NK::Double, NK::Int) | (NK::Double, NK::Long) => (Prim::D, Prim::D),
        (NK::Double, NK::Float) => (Prim::D, Prim::F),
        _ => unreachable!("conversion pair"),
    };
    // Rust's `as` casts match Java exactly here (emit_convert's law):
    // float/double → int/long saturate with NaN → 0; int → narrow wraps;
    // int → char zero-extends. The CAST uses the descriptor kinds; the
    // variable kind is the from-kind for narrow targets.
    let cast: &str = match (conv.from, conv.to) {
        (NK::Int, NK::Long) => " as i64",
        (NK::Int, NK::Float) => " as f32",
        (NK::Int, NK::Double) => " as f64",
        (NK::Int, NK::Byte) => " as i8 as i32",
        (NK::Int, NK::Char) => " as u16 as i32",
        (NK::Int, NK::Short) => " as i16 as i32",
        (NK::Long, NK::Int) => " as i32",
        (NK::Long, NK::Float) => " as f32",
        (NK::Long, NK::Double) => " as f64",
        (NK::Float, NK::Int) => " as i32",
        (NK::Float, NK::Long) => " as i64",
        (NK::Float, NK::Double) => " as f64",
        (NK::Double, NK::Int) => " as i32",
        (NK::Double, NK::Long) => " as i64",
        (NK::Double, NK::Float) => " as f32",
        _ => unreachable!("conversion pair"),
    };
    let (result_slot, a_slot) = (vs[k - 1], vs[k - 1]);
    let a = ctx.read_prim(pc, a_slot, from_prim);
    ctx.out
        .push(format!("let __a: {} = {a};", from_prim.rust_ty()));
    let new_kind = match conv.to {
        NK::Long => Kind::Long,
        NK::Float => Kind::Float,
        NK::Double => Kind::Double,
        _ => Kind::Int,
    };
    // The result variable's type: the TARGET kind for widening to
    // long/float/double, the from-kind (int) for narrow targets.
    let target = ctx.refill_target(result_slot, pc, Some(new_kind));
    if let Some((name, site)) = target {
        let expr = format!("__a{cast}");
        // One unified path for both storage kinds: the def's kind (the
        // DESCRIPTOR target) decides the read law and the Cell ctor at
        // materialization — a Cell-target write bypass here would strand
        // the operand's pending entry (LU.det's getfield-pivsign → i2d →
        // dstore read a stale int through a double-typed local).
        let src = match conv.to {
            NK::Long => Src::Prim(Prim::L),
            NK::Float => Src::Prim(Prim::F),
            NK::Double => Src::Prim(Prim::D),
            _ => Src::Prim(Prim::I),
        };
        ctx.push_result(result_slot, Some((name, site)), expr, src, false, new_kind);
    }
}

pub(crate) fn emit_cmp_typed(ctx: &mut Ctx<'_>, pc: u32, kind: crate::ir::CmpKind, vs: &[usize]) {
    use crate::ir::CmpKind as CK;
    let k = vs.len();
    let (a_slot, b_slot) = (vs[k - 2], vs[k - 1]);
    let (prim, nan_val): (Prim, i32) = match kind {
        CK::LCmp => (Prim::L, 0),
        CK::FCmpL => (Prim::F, -1),
        CK::FCmpG => (Prim::F, 1),
        CK::DCmpL => (Prim::D, -1),
        CK::DCmpG => (Prim::D, 1),
    };
    let b = ctx.read_prim(pc, b_slot, prim);
    let a = ctx.read_prim(pc, a_slot, prim);
    ctx.out.push(format!(
        "let __b: {} = {b}; let __a: {} = {a};",
        prim.rust_ty(),
        prim.rust_ty()
    ));
    let expr = if kind == CK::LCmp {
        "match __a.cmp(&__b) { ::core::cmp::Ordering::Less => -1i32, ::core::cmp::Ordering::Greater => 1i32, ::core::cmp::Ordering::Equal => 0i32 }".to_string()
    } else {
        format!(
            "if __a.is_nan() || __b.is_nan() {{ {nan_val}i32 }} else {{ match __a.partial_cmp(&__b) {{ ::core::option::Option::Some(::core::cmp::Ordering::Less) => -1i32, ::core::option::Option::Some(::core::cmp::Ordering::Greater) => 1i32, _ => 0i32 }} }}"
        )
    };
    let target = ctx.refill_target(a_slot, pc, Some(Kind::Int));
    ctx.push_result(a_slot, target, expr, Src::Prim(Prim::I), false, Kind::Int);
}

pub(crate) fn emit_getstatic_typed(
    ctx: &mut Ctx<'_>,
    pc: u32,
    fault: &str,
    class: &str,
    field: &str,
) {
    fn push_value(ctx: &mut Ctx<'_>, pc: u32, expr: String) {
        // Every call site pushes a REF (System.out/err, Locale.US, TYPE,
        // any ref-typed static): a ref epoch means the position is a Cell
        // and the cell path stores directly. A Typed target here would
        // take the def-time temp — unreachable for refs by the verifier,
        // but the unified push law handles it uniformly.
        let slot = ctx.shape(pc).stack.len();
        let target = ctx.fresh_target(slot, pc);
        ctx.push_result(slot, target, expr, Src::Value, false, Kind::Ref);
    }
    if class == "java/lang/System" && field == "out" {
        push_value(ctx, pc, format!("{RT}::natives::system_out()"));
    } else if class == "java/lang/System" && field == "err" {
        push_value(ctx, pc, format!("{RT}::natives::system_err()"));
    } else if class == "java/util/Locale" && field == "US" {
        push_value(ctx, pc, format!("{RT}::natives::builtin_locale_us()"));
    } else if field == "TYPE" {
        if let Some(desc) = crate::primitive_type_field(class) {
            push_value(ctx, pc, format!("{RT}::reflect::class_value({desc:?})"));
        } else {
            let f = crate::resolve_field_pub(ctx.prog, class, field, true)
                .expect("validated static field");
            let slot = f.static_slot.expect("static field");
            push_value(ctx, pc, format!("crate::statics::get({slot}u32)"));
        }
    } else {
        let f =
            crate::resolve_field_pub(ctx.prog, class, field, true).expect("validated static field");
        if let Some(marker) = crate::emit::jmm_boundary_marker(f, class) {
            ctx.out.push(marker);
        }
        let slot = f.static_slot.expect("static field");
        let ensures = crate::emit::ensure_calls(ctx.prog, class);
        // `ensure` may dispatch into user code (<clinit>): refresh first.
        if !ensures.is_empty() {
            ctx.refresh(pc);
        }
        for (id, expr) in ensures {
            // CLINIT-THROW: same lowering as the default body.
            ctx.out.push(format!(
                "{};",
                try_wrap(fault, format!("crate::statics::ensure({id}u32, {expr})"))
            ));
        }
        let kind = desc_kind_of(&f.desc);
        let push_slot = ctx.shape(pc).stack.len();
        let target = ctx.fresh_target(push_slot, pc);
        if let Some((_, Site::Typed(tp))) = &target {
            assert_eq!(Some(*tp), Prim::of(kind), "j2r internal: static kind");
        }
        ctx.push_result(
            push_slot,
            target,
            format!("crate::statics::get({slot}u32)"),
            Src::Value,
            false,
            kind,
        );
    }
}

/// The invoke result's refill write: lands on the slot the receiver (or,
/// for statics, the deepest argument) occupied — the kind-change law of
/// the analysis applies unchanged. Void returns write nothing. A typed
/// target keeps the result in the def-time temp (the expect hoists
/// here); a cell target stores as before.
/// The static-call result law: with arguments, the result replaces the
/// deepest argument's slot; a no-arg call pushes onto a FRESH slot (no
/// popped slot to refill — System.nanoTime, Math.random, ...). The call
/// always executes here.
pub(crate) fn write_invoke_result_static(
    ctx: &mut Ctx<'_>,
    pc: u32,
    vs: &[usize],
    k: usize,
    n: usize,
    desc: &str,
    call: String,
) {
    if n >= 1 {
        write_invoke_result(ctx, pc, vs[k - n], desc, call);
        return;
    }
    // Execute the call into `__v` first (no pending def ever reads __v —
    // every symbolic def owns a method-unique temp), then record the
    // symbolic result for the fresh slot from that binding.
    ctx.out.push(format!("let __v = {call};"));
    if !returns_void(desc) {
        let slot = ctx.shape(pc).stack.len();
        let target = ctx.fresh_target(slot, pc);
        ctx.push_result(
            slot,
            target,
            "__v".to_string(),
            Src::Value,
            false,
            desc_kind_of(&return_desc(desc)),
        );
    }
}

pub(crate) fn write_invoke_result(
    ctx: &mut Ctx<'_>,
    pc: u32,
    slot: usize,
    desc: &str,
    call: String,
) {
    if returns_void(desc) {
        // Void: the call executes into the shadowable `__v` binding —
        // nothing pending ever reads it (every symbolic def owns a
        // method-unique `__t{n}` temp).
        ctx.out.push(format!("let __v = {call};"));
        return;
    }
    let k = desc_kind_of(&return_desc(desc));
    let target = ctx.refill_target(slot, pc, Some(k));
    ctx.push_result(slot, target, call, Src::Value, false, k);
}

/// The return-type descriptor of a method descriptor (`(II)D` → `D`).
pub(crate) fn return_desc(desc: &str) -> String {
    let close = desc.rfind(')').expect("valid descriptor");
    desc[close + 1..].to_string()
}
