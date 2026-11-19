//! `verify`::sim — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use crate::ir::{ArithOp, CmpKind, Conv, ElemKind, Instr, Ldc2, NumKind};
use crate::{ConcatPlan, MethodPlan};
use j2r_runtime::{parse_method_desc, TypeDesc};
use std::collections::{BTreeMap, HashMap, VecDeque};

pub fn verify_method_shapes(
    m: &MethodPlan,
    concats: &BTreeMap<u16, ConcatPlan>,
    lambdas: &BTreeMap<u16, crate::lambda::LambdaPlan>,
) -> std::result::Result<MethodShapes, VerifyError> {
    if m.is_native {
        // T12 12c (docs/M5_JNI_NATIVES.md §4): a native method has no
        // bytecode to verify — its body is the emitted call-through to
        // the binding registry, correct by construction. There is no
        // abstract/native distinction here: abstract methods never
        // reach a MethodPlan.
        return Ok(MethodShapes::default());
    }
    if m.instrs.is_empty() {
        return Err(err(0, "method body has no instructions"));
    }
    // instruction boundary table
    let mut at: HashMap<u32, usize> = HashMap::with_capacity(m.instrs.len());
    for (i, (pc, _)) in m.instrs.iter().enumerate() {
        at.insert(*pc, i);
    }
    let boundary = |pc: u32, what: &str| -> std::result::Result<usize, VerifyError> {
        at.get(&pc)
            .copied()
            .ok_or_else(|| err(pc, format!("{what} {pc} is not an instruction boundary")))
    };

    // worklist fixpoint
    let mut states: Vec<Option<State>> = vec![None; m.instrs.len()];
    let mut queue: VecDeque<usize> = VecDeque::new();
    states[0] = Some(entry_state(m)?);
    queue.push_back(0);

    while let Some(idx) = queue.pop_front() {
        let Some(state) = states[idx].clone() else {
            continue;
        };
        let (pc, instr) = &m.instrs[idx];
        let pc = *pc;
        let mut sim = Sim {
            m,
            pc,
            stack: state.stack,
            locals: state.locals,
            concats,
            lambdas,
            max_stack: m.max_stack,
        };
        let successors = sim.execute(instr)?;
        // exception edges: every covered instruction can jump to any handler
        for (start, end, handler, _) in &m.exception_table {
            if *start <= pc && pc < *end {
                let h = at.get(handler).copied().ok_or_else(|| {
                    err(
                        *handler,
                        format!("exception handler_pc {handler} is not an instruction boundary"),
                    )
                })?;
                let hstate = State {
                    stack: vec![VTy::Ref],
                    locals: sim.locals.clone(),
                };
                merge_state(&mut states, &mut queue, h, *handler, &hstate, pc)?;
            }
        }
        for (target, st) in successors {
            let t = boundary(target, "branch target")?;
            merge_state(&mut states, &mut queue, t, target, &st, pc)?;
        }
    }

    // Export the verified input state of every reached instruction.
    let mut per_pc = std::collections::BTreeMap::new();
    for (idx, st) in states.iter().enumerate() {
        if let Some(st) = st {
            per_pc.insert(
                m.instrs[idx].0,
                PcShape {
                    stack: st.stack.clone(),
                    locals: st.locals.clone(),
                },
            );
        }
    }
    Ok(MethodShapes {
        per_pc,
        max_locals: m.max_locals,
        max_stack: m.max_stack,
    })
}

/// Verifies one method body (shape simulation + fixpoint over joins).
pub fn verify_method(
    m: &MethodPlan,
    concats: &BTreeMap<u16, ConcatPlan>,
    lambdas: &BTreeMap<u16, crate::lambda::LambdaPlan>,
) -> std::result::Result<(), VerifyError> {
    verify_method_shapes(m, concats, lambdas).map(|_| ())
}

/// Merges `st` into the recorded state of instruction slot `t` (whose pc is
/// `t_pc`, for diagnostics); queues `t` when its state changed.
pub(crate) fn merge_state(
    states: &mut [Option<State>],
    queue: &mut VecDeque<usize>,
    t: usize,
    t_pc: u32,
    st: &State,
    from_pc: u32,
) -> std::result::Result<(), VerifyError> {
    match &mut states[t] {
        None => {
            states[t] = Some(st.clone());
            queue.push_back(t);
        }
        Some(existing) => {
            let changed = existing
                .merge_into(st)
                .map_err(|msg| err(from_pc, format!("{msg} (edge into pc {t_pc})")))?;
            if changed && !queue.contains(&t) {
                queue.push_back(t);
            }
        }
    }
    Ok(())
}

pub(crate) struct Sim<'a> {
    m: &'a MethodPlan,
    pc: u32,
    stack: Vec<VTy>,
    locals: Vec<VTy>,
    concats: &'a BTreeMap<u16, ConcatPlan>,
    lambdas: &'a BTreeMap<u16, crate::lambda::LambdaPlan>,
    max_stack: usize,
}

impl<'a> Sim<'a> {
    fn e(&self, msg: impl Into<String>) -> VerifyError {
        err(self.pc, msg)
    }

    fn pop(&mut self) -> std::result::Result<VTy, VerifyError> {
        self.stack
            .pop()
            .ok_or_else(|| self.e("operand stack underflow"))
    }

    /// Pops one value that must be reference-shaped (`Ref`/`Null`/`Top`,
    /// or a typed array, which is itself a reference).
    fn pop_ref(&mut self) -> std::result::Result<VTy, VerifyError> {
        let v = self.pop()?;
        match v {
            VTy::Ref | VTy::Null | VTy::Top | VTy::Array(_) => Ok(VTy::Ref),
            other => Err(self.e(format!(
                "expected a reference on the stack, found {other:?}"
            ))),
        }
    }

    /// Pops one value that must be exactly `want` (`Top` is accepted for
    /// any category-1 `want`; `Null` is accepted for every reference shape
    /// — `Ref` *and* typed arrays, because `null` is assignable to any
    /// array type, corpus F2 — and typed arrays for `Ref`; arrays-of-
    /// references are accepted for `Array(Ref)`, JLS 4.10.3 covariance:
    /// `[[D <: [Object` because `[D <: Object`, so a `double[][]` may be
    /// passed where an `Object[]` parameter is declared — JAMA wave 3,
    /// `Vector.copyInto(Object[])` on `double[][]`).
    fn pop_exact(&mut self, want: VTy) -> std::result::Result<VTy, VerifyError> {
        let v = self.pop()?;
        let ok = v == want
            || (v == VTy::Top && want.is_cat1())
            || (v == VTy::Null && matches!(want, VTy::Ref | VTy::Array(_)))
            || (matches!(v, VTy::Array(_)) && want == VTy::Ref)
            || (matches!(&v, VTy::Array(k) if k.elements_are_refs())
                && want == VTy::Array(ElemKind::Ref));
        if ok {
            Ok(want)
        } else {
            Err(self.e(format!("expected {want:?} on the stack, found {v:?}")))
        }
    }

    /// Pops the array operand of `xXaload`/`xXastore`: any reference works,
    /// but a *typed* array must carry the matching element kind.
    fn pop_array(&mut self, k: ElemKind) -> std::result::Result<(), VerifyError> {
        let v = self.pop()?;
        match v {
            // A primitive load/store demands its exact element kind (a long
            // load off an int[] is malformed). A reference load (aaload) is
            // fine on ANY ref-element array — including nested primitive
            // arrays (`[[I` elements are `int[]` references).
            VTy::Array(j) if j != k && !matches!(k, ElemKind::Ref) => Err(self.e(format!(
                "array element kind mismatch: operation expects {k:?}, array is {j:?}"
            ))),
            VTy::Ref | VTy::Null | VTy::Top | VTy::Array(_) => Ok(()),
            other => Err(self.e(format!(
                "expected an array reference on the stack, found {other:?}"
            ))),
        }
    }

    pub(crate) fn push(&mut self, v: VTy) -> std::result::Result<(), VerifyError> {
        self.stack.push(v);
        self.check_max()
    }

    fn check_max(&self) -> std::result::Result<(), VerifyError> {
        if self.stack.len() > self.max_stack {
            return Err(self.e(format!(
                "operand stack overflow: {} entries exceed max_stack {}",
                self.stack.len(),
                self.max_stack
            )));
        }
        Ok(())
    }

    fn check_local(&self, idx: u16) -> std::result::Result<usize, VerifyError> {
        let idx = idx as usize;
        if idx >= self.locals.len() {
            return Err(self.e(format!(
                "local variable index {idx} out of range (max_locals {})",
                self.locals.len()
            )));
        }
        Ok(idx)
    }

    fn load(&mut self, idx: u16) -> std::result::Result<(), VerifyError> {
        let i = self.check_local(idx)?;
        match self.locals[i].clone() {
            VTy::Unset => Err(self.e(format!("load of never-assigned local {i}"))),
            VTy::Poison => Err(self.e(format!("load of local {i} with conflicting types"))),
            VTy::HighHalf => Err(self.e(format!(
                "load of the reserved high half of category-2 local {}",
                i.saturating_sub(1)
            ))),
            v => self.push(v),
        }
    }

    fn store(&mut self, idx: u16) -> std::result::Result<(), VerifyError> {
        let i = self.check_local(idx)?;
        let v = self.pop()?;
        if v.is_cat2() {
            if i + 1 >= self.locals.len() {
                return Err(self.e(format!(
                    "category-2 store at local {i} has no high-half slot (max_locals {})",
                    self.locals.len()
                )));
            }
            self.locals[i] = v;
            self.locals[i + 1] = VTy::HighHalf;
        } else {
            // a cat-1 store into the high half breaks the cat-2 local below
            if i > 0 && self.locals[i - 1].is_cat2() {
                self.locals[i - 1] = VTy::Poison;
            }
            self.locals[i] = v;
        }
        Ok(())
    }

    // -------------------------------------------------- the dup family,
    // mirroring JVMS §6.5 exactly (stricter than the runtime, which is
    // lenient on unreachable forms)

    fn pop_cat1(&mut self, what: &str) -> std::result::Result<VTy, VerifyError> {
        let v = self.pop()?;
        if v.is_cat2() {
            return Err(self.e(format!("{what} requires category-1 values")));
        }
        Ok(v)
    }

    /// dup_x2 form 1/2 (verbatim from `dup_family`, round-5 reshaping).
    fn dup_x2(&mut self) -> std::result::Result<(), VerifyError> {
        let v1 = self.pop_cat1("dup_x2")?;
        let v2 = self.pop()?;
        if v2.is_cat2() {
            // Form 2: v2(cat2), v1 -> v1, v2, v1
            self.push(v1.clone())?;
            self.push(v2.clone())?;
            self.push(v1.clone())?;
        } else {
            // Form 1: v3, v2, v1 (all cat1) -> v1, v3, v2, v1
            let v3 = self.pop_cat1("dup_x2")?;
            self.push(v1.clone())?;
            self.push(v3.clone())?;
            self.push(v2.clone())?;
            self.push(v1.clone())?;
        }
        Ok(())
    }

    /// dup2 form 1/2 (verbatim from `dup_family`, round-5 reshaping).
    fn dup2(&mut self) -> std::result::Result<(), VerifyError> {
        let v1 = self.pop()?;
        if v1.is_cat2() {
            // Form 2: v1 -> v1, v1
            self.push(v1.clone())?;
            self.push(v1.clone())?;
        } else {
            // Form 1: v2, v1 -> v2, v1, v2, v1
            let v2 = self.pop_cat1("dup2")?;
            self.push(v2.clone())?;
            self.push(v1.clone())?;
            self.push(v2.clone())?;
            self.push(v1.clone())?;
        }
        Ok(())
    }

    /// dup2_x1 form 1/2 (verbatim from `dup_family`, round-5 reshaping).
    fn dup2_x1(&mut self) -> std::result::Result<(), VerifyError> {
        let v1 = self.pop()?;
        if v1.is_cat2() {
            // Form 2: v2(cat1), v1(cat2) -> v1, v2, v1
            let v2 = self.pop_cat1("dup2_x1")?;
            self.push(v1.clone())?;
            self.push(v2.clone())?;
            self.push(v1.clone())?;
        } else {
            // Form 1: v3, v2, v1 (all cat1) -> v2, v1, v3, v2, v1
            let v2 = self.pop_cat1("dup2_x1")?;
            let v3 = self.pop_cat1("dup2_x1")?;
            self.push(v2.clone())?;
            self.push(v1.clone())?;
            self.push(v3.clone())?;
            self.push(v2.clone())?;
            self.push(v1.clone())?;
        }
        Ok(())
    }

    /// dup2_x2 forms 1-4 (verbatim from `dup_family`, round-5 reshaping).
    /// Forms follow the runtime's stack.rs table (and the JVMS):
    /// each of the top two two-slot units (U = the duplicated
    /// unit on top, X = the unit the copy is inserted below) is
    /// either one category-2 value or two category-1 values.
    ///   F1 U=c1,c1  X=c1,c1: v4,v3,v2,v1 -> v2,v1,v4,v3,v2,v1
    ///   F2 U=cat2   X=c1,c1: v3,v2,v1 -> v1,v3,v2,v1
    ///     (JAMA wave 3: `LUrowi[j] = LUcolj[i] -= s;` — javac's
    ///     compound double assignment ends cat2-on-top)
    ///   F3 U=c1,c1  X=cat2:  v3,v2,v1 -> v2,v1,v3,v2,v1
    ///   F4 U=cat2   X=cat2:  v2,v1 -> v1,v2,v1
    fn dup2_x2(&mut self) -> std::result::Result<(), VerifyError> {
        let v1 = self.pop()?;
        let v2 = self.pop()?;
        match (v1.is_cat2(), v2.is_cat2()) {
            (true, true) => {
                // Form 4
                self.push(v1.clone())?;
                self.push(v2.clone())?;
                self.push(v1.clone())?;
            }
            (true, false) => {
                // Form 2
                let v3 = self.pop_cat1("dup2_x2")?;
                self.push(v1.clone())?;
                self.push(v3.clone())?;
                self.push(v2.clone())?;
                self.push(v1.clone())?;
            }
            (false, true) => {
                return Err(self.e(
                    "dup2_x2: no JVMS form has a category-1 value on top of a category-2 value",
                ));
            }
            (false, false) => {
                let v3 = self.pop()?;
                if v3.is_cat2() {
                    // Form 3
                    self.push(v2.clone())?;
                    self.push(v1.clone())?;
                    self.push(v3.clone())?;
                    self.push(v2.clone())?;
                    self.push(v1.clone())?;
                } else {
                    // Form 1
                    let v4 = self.pop_cat1("dup2_x2")?;
                    self.push(v2.clone())?;
                    self.push(v1.clone())?;
                    self.push(v4.clone())?;
                    self.push(v3.clone())?;
                    self.push(v2.clone())?;
                    self.push(v1.clone())?;
                }
            }
        }
        Ok(())
    }

    fn dup_family(&mut self, instr: &Instr) -> std::result::Result<(), VerifyError> {
        match instr {
            Instr::Pop => {
                self.pop_cat1("pop")?;
            }
            Instr::Pop2 => {
                let v = self.pop()?;
                if !v.is_cat2() {
                    self.pop_cat1("pop2")?;
                }
            }
            Instr::Dup => {
                let v = self.pop_cat1("dup")?;
                self.push(v.clone())?;
                self.push(v.clone())?;
            }
            Instr::DupX1 => {
                let v1 = self.pop_cat1("dup_x1")?;
                let v2 = self.pop_cat1("dup_x1")?;
                self.push(v1.clone())?;
                self.push(v2.clone())?;
                self.push(v1.clone())?;
            }
            Instr::DupX2 => self.dup_x2()?,
            Instr::Dup2 => self.dup2()?,
            Instr::Dup2X1 => self.dup2_x1()?,
            Instr::Dup2X2 => self.dup2_x2()?,
            Instr::Swap => {
                let v1 = self.pop_cat1("swap")?;
                let v2 = self.pop_cat1("swap")?;
                self.push(v1.clone())?;
                self.push(v2.clone())?;
            }
            _ => unreachable!("not a dup-family instruction"),
        }
        Ok(())
    }

    /// Branch successors helper: fallthrough (optional) plus the target.
    fn branch_out(
        &mut self,
        target: u32,
        fallthrough: bool,
    ) -> std::result::Result<Vec<(u32, State)>, VerifyError> {
        let mut out = Vec::with_capacity(2);
        if fallthrough {
            let np = next_pc(self.m, self.pc);
            if np == u32::MAX {
                return Err(self.e("control flow falls off the end of the method body"));
            }
            out.push((np, self.snapshot()));
        }
        out.push((target, self.snapshot()));
        Ok(out)
    }

    /// Executes one instruction against the abstract state. Returns the
    /// control successors as `(target_pc, entry_state)` pairs (empty for
    /// `return`/`athrow`). The dispatch match partitions the `Instr`
    /// enum exhaustively into the family helpers (round-5 reshaping);
    /// each helper re-matches only its own variants, so a variant landing
    /// in the wrong helper is the loud `unreachable!`, never a silent
    /// no-op. The family bodies are verbatim from the pre-split match.
    fn execute(&mut self, instr: &Instr) -> std::result::Result<Vec<(u32, State)>, VerifyError> {
        use Instr::*;
        match instr {
            IfZero(_, _)
            | IfICmp(_, _)
            | IfRefCmp { .. }
            | IfNull(_)
            | IfNonNull(_)
            | Goto(_)
            | TableSwitch { .. }
            | LookupSwitch { .. }
            | Return(_)
            | AThrow => {
                return self.execute_control(instr);
            }
            Nop | AConstNull | IConst(_) | LConst(_) | FConst(_) | DConst(_) | Ldc(_)
            | Ldc2W(_) => self.execute_const_ops(instr)?,
            Load(_) | Store(_) | IInc { .. } | ALoad(_) | AStore(_) | ArrayLength => {
                self.execute_local_array_ops(instr)?;
            }
            Pop | Pop2 | Dup | DupX1 | DupX2 | Dup2 | Dup2X1 | Dup2X2 | Swap => {
                self.dup_family(instr)?;
            }
            Arith { .. } | Convert(_) | Cmp(_) => self.execute_arith_ops(instr)?,
            GetStatic { .. } | PutStatic { .. } | GetField { .. } | PutField { .. } => {
                self.execute_field_ops(instr)?;
            }
            InvokeVirtual { .. }
            | InvokeSpecial { .. }
            | InvokeInterface { .. }
            | InvokeStatic { .. }
            | InvokeDynamic { .. } => self.execute_invoke_ops(instr)?,
            New { .. }
            | NewArray(_)
            | NewIntArrayFilled { .. }
            | MapPutPrim { .. }
            | MapGetPrim { .. }
            | ANewArray { .. }
            | MultiNewArray { .. } => {
                self.execute_alloc_ops(instr)?;
            }
            MonitorEnter | MonitorExit | CheckCast { .. } | InstanceOf { .. } => {
                self.execute_misc_ops(instr)?;
            }
        }
        // fallthrough successor
        let np = next_pc(self.m, self.pc);
        if np == u32::MAX {
            return Err(self.e("control flow falls off the end of the method body"));
        }
        Ok(vec![(np, self.snapshot())])
    }

    /// Control-family arms (verbatim from `execute`, round-5 reshaping):
    /// every variant returns its explicit successor list.
    #[allow(clippy::type_complexity)]
    fn execute_control(
        &mut self,
        instr: &Instr,
    ) -> std::result::Result<Vec<(u32, State)>, VerifyError> {
        use Instr::*;
        match instr {
            IfZero(_, t) => {
                self.pop_exact(VTy::Int)?;
                self.branch_out(*t, true)
            }
            IfICmp(_, t) => {
                self.pop_exact(VTy::Int)?;
                self.pop_exact(VTy::Int)?;
                self.branch_out(*t, true)
            }
            IfRefCmp { target: t, .. } => {
                self.pop_ref()?;
                self.pop_ref()?;
                self.branch_out(*t, true)
            }
            IfNull(t) | IfNonNull(t) => {
                self.pop_ref()?;
                self.branch_out(*t, true)
            }
            Goto(t) => self.branch_out(*t, false),
            TableSwitch {
                default, targets, ..
            } => {
                self.pop_exact(VTy::Int)?;
                let snap = self.snapshot();
                let mut out = Vec::with_capacity(targets.len() + 1);
                out.push((*default, snap.clone()));
                for t in targets {
                    out.push((*t, snap.clone()));
                }
                Ok(out)
            }
            LookupSwitch { default, pairs } => {
                self.pop_exact(VTy::Int)?;
                let snap = self.snapshot();
                let mut out = Vec::with_capacity(pairs.len() + 1);
                out.push((*default, snap.clone()));
                for (_, t) in pairs {
                    out.push((*t, snap.clone()));
                }
                Ok(out)
            }
            Return(t) => {
                let (_params, ret) = parse_method_desc(&self.m.desc)
                    .ok_or_else(|| self.e(format!("bad descriptor {:?}", self.m.desc)))?;
                match (t, &ret) {
                    (TypeDesc::Void, TypeDesc::Void) => {}
                    (TypeDesc::Void, _) | (_, TypeDesc::Void) => {
                        return Err(self.e(format!(
                            "{} in a method declared -> {}",
                            if *t == TypeDesc::Void {
                                "void return"
                            } else {
                                "value return"
                            },
                            self.m.desc
                        )))
                    }
                    (TypeDesc::Long, TypeDesc::Long)
                    | (TypeDesc::Float, TypeDesc::Float)
                    | (TypeDesc::Double, TypeDesc::Double) => {
                        self.pop_exact(td_vty(t))?;
                    }
                    (TypeDesc::Object(_), TypeDesc::Object(_))
                    | (TypeDesc::Object(_), TypeDesc::Array(_))
                    | (TypeDesc::Array(_), _) => {
                        self.pop_ref()?;
                    }
                    (TypeDesc::Int, _)
                    | (TypeDesc::Boolean, _)
                    | (TypeDesc::Byte, _)
                    | (TypeDesc::Short, _)
                    | (TypeDesc::Char, _) => {
                        self.pop_exact(VTy::Int)?;
                    }
                    _ => {
                        return Err(self.e(format!(
                            "return kind {t:?} does not match descriptor {}",
                            self.m.desc
                        )))
                    }
                }
                Ok(Vec::new())
            }
            AThrow => {
                self.pop_ref()?;
                Ok(Vec::new())
            }
            _ => unreachable!("not a control-family instruction"),
        }
    }

    /// Const-family arms (verbatim from `execute`, round-5 reshaping).
    fn execute_const_ops(&mut self, instr: &Instr) -> std::result::Result<(), VerifyError> {
        use Instr::*;
        match instr {
            Nop => {}
            AConstNull => self.push(VTy::Null)?,
            IConst(_) => self.push(VTy::Int)?,
            LConst(_) => self.push(VTy::Long)?,
            FConst(_) => self.push(VTy::Float)?,
            DConst(_) => self.push(VTy::Double)?,
            Ldc(c) => match c {
                crate::ir::Ldc::Int(_) => self.push(VTy::Int)?,
                crate::ir::Ldc::Float(_) => self.push(VTy::Float)?,
                crate::ir::Ldc::String(_) | crate::ir::Ldc::Class(_) => self.push(VTy::Ref)?,
            },
            Ldc2W(c) => match c {
                Ldc2::Long(_) => self.push(VTy::Long)?,
                Ldc2::Double(_) => self.push(VTy::Double)?,
            },
            _ => unreachable!("not a const-family instruction"),
        }
        Ok(())
    }

    /// Local/array access arms (verbatim from `execute`, round-5
    /// reshaping).
    fn execute_local_array_ops(&mut self, instr: &Instr) -> std::result::Result<(), VerifyError> {
        use Instr::*;
        match instr {
            Load(idx) => self.load(*idx)?,
            Store(idx) => self.store(*idx)?,
            IInc { idx, .. } => {
                let i = self.check_local(*idx)?;
                match self.locals[i].clone() {
                    VTy::Int | VTy::Top => {}
                    VTy::Unset => return Err(self.e(format!("iinc of never-assigned local {i}"))),
                    other => {
                        return Err(self.e(format!(
                            "iinc requires an int local, local {i} is {other:?}"
                        )))
                    }
                }
            }
            ALoad(k) => {
                let k = k.clone();
                self.pop_exact(VTy::Int)?;
                let v = self.pop()?;
                match &v {
                    VTy::Array(j) => {
                        // A primitive opcode demands its exact kind (a long
                        // load off an int[] is malformed). `aaload` takes
                        // the RECEIVER's static element shape when the
                        // array is typed — `[[I` yields `[I`, exactly like
                        // the JVM verifier (corpus wave 2, JZlib
                        // InfBlocks.proc feeding InfCodes.init's `[I`).
                        if !matches!(k, ElemKind::Ref) && *j != k {
                            return Err(self.e(format!(
                                "array element kind mismatch: operation expects {k:?}, array is {j:?}"
                            )));
                        }
                        self.push(elem_vty(j.clone()))?;
                    }
                    VTy::Ref | VTy::Null | VTy::Top => self.push(elem_vty(k))?,
                    other => {
                        return Err(self.e(format!(
                            "expected an array reference on the stack, found {other:?}"
                        )))
                    }
                }
            }
            AStore(k) => {
                self.pop_exact(elem_vty(k.clone()))?;
                self.pop_exact(VTy::Int)?;
                self.pop_array(k.clone())?;
            }
            ArrayLength => {
                self.pop_ref()?;
                self.push(VTy::Int)?;
            }
            _ => unreachable!("not a local/array-family instruction"),
        }
        Ok(())
    }

    /// Arithmetic/conversion/comparison arms (verbatim from `execute`,
    /// round-5 reshaping).
    fn execute_arith_ops(&mut self, instr: &Instr) -> std::result::Result<(), VerifyError> {
        use Instr::*;
        use NumKind as NK;
        match instr {
            Arith { op, kind } => {
                use ArithOp::*;
                match kind {
                    NK::Int => match op {
                        Neg => {
                            self.pop_exact(VTy::Int)?;
                            self.push(VTy::Int)?;
                        }
                        Shl | Shr | UShr => {
                            self.pop_exact(VTy::Int)?; // shift amount
                            self.pop_exact(VTy::Int)?;
                            self.push(VTy::Int)?;
                        }
                        _ => {
                            self.pop_exact(VTy::Int)?;
                            self.pop_exact(VTy::Int)?;
                            self.push(VTy::Int)?;
                        }
                    },
                    NK::Long => match op {
                        Neg => {
                            self.pop_exact(VTy::Long)?;
                            self.push(VTy::Long)?;
                        }
                        Shl | Shr | UShr => {
                            self.pop_exact(VTy::Int)?; // shift amount is int
                            self.pop_exact(VTy::Long)?;
                            self.push(VTy::Long)?;
                        }
                        _ => {
                            self.pop_exact(VTy::Long)?;
                            self.pop_exact(VTy::Long)?;
                            self.push(VTy::Long)?;
                        }
                    },
                    NK::Float => match op {
                        Neg => {
                            self.pop_exact(VTy::Float)?;
                            self.push(VTy::Float)?;
                        }
                        _ => {
                            self.pop_exact(VTy::Float)?;
                            self.pop_exact(VTy::Float)?;
                            self.push(VTy::Float)?;
                        }
                    },
                    NK::Double => match op {
                        Neg => {
                            self.pop_exact(VTy::Double)?;
                            self.push(VTy::Double)?;
                        }
                        _ => {
                            self.pop_exact(VTy::Double)?;
                            self.pop_exact(VTy::Double)?;
                            self.push(VTy::Double)?;
                        }
                    },
                    // the decoder never produces these for Arith (i2b/i2c/i2s
                    // are Convert); defensive rejection
                    NK::Byte | NK::Char | NK::Short => {
                        return Err(self.e("arithmetic on byte/char/short kind"))
                    }
                }
            }
            Convert(Conv { from, to }) => {
                let f = match from {
                    NK::Int | NK::Byte | NK::Char | NK::Short => VTy::Int,
                    NK::Long => VTy::Long,
                    NK::Float => VTy::Float,
                    NK::Double => VTy::Double,
                };
                self.pop_exact(f)?;
                let t = match to {
                    NK::Int | NK::Byte | NK::Char | NK::Short => VTy::Int,
                    NK::Long => VTy::Long,
                    NK::Float => VTy::Float,
                    NK::Double => VTy::Double,
                };
                self.push(t)?;
            }
            Cmp(k) => match k {
                CmpKind::LCmp => {
                    self.pop_exact(VTy::Long)?;
                    self.pop_exact(VTy::Long)?;
                    self.push(VTy::Int)?;
                }
                CmpKind::FCmpL | CmpKind::FCmpG => {
                    self.pop_exact(VTy::Float)?;
                    self.pop_exact(VTy::Float)?;
                    self.push(VTy::Int)?;
                }
                CmpKind::DCmpL | CmpKind::DCmpG => {
                    self.pop_exact(VTy::Double)?;
                    self.pop_exact(VTy::Double)?;
                    self.push(VTy::Int)?;
                }
            },
            _ => unreachable!("not an arith-family instruction"),
        }
        Ok(())
    }

    /// Field access arms (verbatim from `execute`, round-5 reshaping).
    fn execute_field_ops(&mut self, instr: &Instr) -> std::result::Result<(), VerifyError> {
        use Instr::*;
        match instr {
            GetStatic { desc, .. } => {
                let t = field_vty(desc).map_err(|m| self.e(m))?;
                self.push(t)?;
            }
            PutStatic { desc, .. } => {
                let t = field_vty(desc).map_err(|m| self.e(m))?;
                self.pop_exact(t)?;
            }
            GetField { desc, .. } => {
                let t = field_vty(desc).map_err(|m| self.e(m))?;
                self.pop_ref()?;
                self.push(t)?;
            }
            PutField { desc, .. } => {
                let t = field_vty(desc).map_err(|m| self.e(m))?;
                self.pop_exact(t)?;
                self.pop_ref()?;
            }
            _ => unreachable!("not a field-family instruction"),
        }
        Ok(())
    }

    /// Invoke arms (verbatim from `execute`, round-5 reshaping).
    fn execute_invoke_ops(&mut self, instr: &Instr) -> std::result::Result<(), VerifyError> {
        use Instr::*;
        match instr {
            InvokeVirtual { desc, .. }
            | InvokeSpecial { desc, .. }
            | InvokeInterface { desc, .. } => {
                let (params, ret) = parse_method_desc(desc)
                    .ok_or_else(|| self.e(format!("bad method descriptor {desc:?}")))?;
                for p in params.iter().rev() {
                    self.pop_exact(td_vty(p))?;
                }
                self.pop_ref()?;
                push_ret(self, &ret)?;
            }
            InvokeStatic { desc, .. } => {
                let (params, ret) = parse_method_desc(desc)
                    .ok_or_else(|| self.e(format!("bad method descriptor {desc:?}")))?;
                for p in params.iter().rev() {
                    self.pop_exact(td_vty(p))?;
                }
                push_ret(self, &ret)?;
            }
            InvokeDynamic { index } => {
                if let Some(plan) = self.concats.get(index) {
                    for k in plan.kinds.chars().rev() {
                        self.pop_exact(kind_vty(k))?;
                    }
                    self.push(VTy::Ref)?;
                } else if let Some(lp) = self.lambdas.get(index) {
                    for k in lp.capture_kinds.chars().rev() {
                        self.pop_exact(kind_vty(k))?;
                    }
                    self.push(VTy::Ref)?;
                } else {
                    return Err(self.e(format!("invokedynamic #{index} has no resolved plan")));
                }
            }
            _ => unreachable!("not an invoke-family instruction"),
        }
        Ok(())
    }

    /// Allocation arms (verbatim from `execute`, round-5 reshaping).
    fn execute_alloc_ops(&mut self, instr: &Instr) -> std::result::Result<(), VerifyError> {
        use Instr::*;
        match instr {
            New { .. } => self.push(VTy::Ref)?,
            NewArray(p) => {
                self.pop_exact(VTy::Int)?;
                // primitive arrays keep their precise kind so e.g. a long
                // loaded from a long[] stays category-2-correct
                self.push(VTy::Array(prim_elem_kind(*p)))?;
            }
            // G2.6 arm a: the collapsed fill pops nothing and pushes one
            // int-group array ref — the same verified shape as NewArray.
            NewIntArrayFilled { prim, .. } => self.push(VTy::Array(prim_elem_kind(*prim)))?,
            // G3.3 slice 2: the collapsed map cores. MapPutPrim pops the
            // prim value, the key and the map (the put's return is
            // discarded by the core's Pop) and pushes nothing — the same
            // net shape as the valueOf/put/pop run it replaces.
            // MapGetPrim pops the key and the map and pushes the prim
            // payload (category-2 for D/J) — the same net shape as the
            // get/checkcast/unbox run it replaces.
            MapPutPrim { prim, .. } => {
                self.pop_exact(boxed_prim_vty(*prim))?;
                self.pop_ref()?;
                self.pop_ref()?;
            }
            MapGetPrim { prim, .. } => {
                self.pop_ref()?;
                self.pop_ref()?;
                self.push(boxed_prim_vty(*prim))?;
            }
            ANewArray { elem } => {
                self.pop_exact(VTy::Int)?;
                // The operand is the ELEMENT class, which may itself be an
                // array type: javac emits `anewarray "[I"` for
                // `new int[n][]`, producing a `[[I` whose aaload yields
                // `[I` — so the result's element kind follows the operand
                // (`[<elem>` descriptor), not a generic Ref (corpus wave 2,
                // JZlib InfBlocks).
                if elem.starts_with('[') {
                    self.push(VTy::Array(array_elem_kind(&format!("[{elem}"))))?;
                } else {
                    self.push(VTy::Array(ElemKind::Ref))?;
                }
            }
            MultiNewArray { desc, dims } => {
                if *dims == 0 {
                    return Err(self.e("multianewarray with zero dimensions"));
                }
                if !desc.starts_with('[') {
                    return Err(self.e(format!(
                        "multianewarray descriptor {desc} is not an array class"
                    )));
                }
                let depth = desc.chars().take_while(|&c| c == '[').count();
                if *dims as usize > depth {
                    return Err(self.e(format!(
                        "multianewarray descriptor {desc} has {depth} array levels but {dims} dimension sizes are consumed"
                    )));
                }
                for _ in 0..*dims {
                    self.pop_exact(VTy::Int)?;
                }
                if *dims as usize == depth {
                    // Fully allocated: the element kind comes from the
                    // descriptor — a primitive keeps its precise kind, a
                    // nested primitive array keeps its typed shape, class
                    // and sub-array components are refs.
                    self.push(VTy::Array(array_elem_kind(desc)))?;
                } else {
                    // Partially allocated: elements are null or sub-arrays,
                    // same element kind by the descriptor.
                    self.push(VTy::Array(array_elem_kind(desc)))?;
                }
            }
            _ => unreachable!("not an allocation-family instruction"),
        }
        Ok(())
    }

    /// Monitor/check/instanceof arms (verbatim from `execute`, round-5
    /// reshaping).
    fn execute_misc_ops(&mut self, instr: &Instr) -> std::result::Result<(), VerifyError> {
        use Instr::*;
        match instr {
            MonitorEnter | MonitorExit => {
                self.pop_ref()?;
            }
            CheckCast { class } => {
                self.pop_ref()?;
                // Refine the stack type when the target is an array
                // descriptor: javac emits `checkcast [Ljava/lang/String;`
                // after the generic Arrays.copyOf bridge (and for
                // `(String[]) list.toArray()` shapes), and the refined
                // value feeds array-shaped natives right after.
                match class.strip_prefix('[') {
                    Some(_) => self.push(VTy::Array(array_elem_kind(class)))?,
                    None => self.push(VTy::Ref)?,
                }
            }
            InstanceOf { .. } => {
                self.pop_ref()?;
                self.push(VTy::Int)?;
            }
            _ => unreachable!("not a monitor/check-family instruction"),
        }
        Ok(())
    }

    fn snapshot(&self) -> State {
        State {
            stack: self.stack.clone(),
            locals: self.locals.clone(),
        }
    }
}
