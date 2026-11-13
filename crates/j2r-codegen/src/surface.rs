//! EXTERNAL-HARNESS H-1/H-2 (jbox2d port process findings, 2026-11-13):
//! `j2r cfg-report --surface` — the sorted, deduplicated JDK member set
//! the input needs, diffed against the NATIVES table. The owner's
//! bug.md §B4-B12 note: each compile reports only the FIRST unresolved
//! reference, so mapping a missing surface took one ~10 s compile per
//! gap (ten cycles for jbox2d). This walk answers the whole question in
//! one page: every JDK member the inputs reference, marked OK (the
//! compiler's own resolution law accepts it) or MISSING (the exact
//! error the compiler would raise).
//!
//! `--diagnostics` prints only the MISSING entries per class and exits
//! non-zero (H-2's collect-all-diagnostics mode — walk everything, then
//! fail; never stop at the first).
//!
//! The classification predicates are the compiler's OWN functions
//! (`resolve_call`, `validate_virtual`, `resolve_field`,
//! `builtin_is_instantiable`, the ANewArray element law) — the report
//! can never disagree with a later compile, because it asks the same
//! code the same question.

use crate::ir;
use crate::Program;
use j2r_runtime::builtin;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Call,
    Field,
    Instantiate,
    ClassRef,
}

impl Kind {
    fn label(&self) -> &'static str {
        match self {
            Kind::Call => "call",
            Kind::Field => "field",
            Kind::Instantiate => "new",
            Kind::ClassRef => "class",
        }
    }
}

/// One JDK member reference: `owner.name:desc` (Instantiate sites carry
/// `name="<init>"`, `desc=""`; ClassRef sites carry `name=""`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Site {
    pub owner: String,
    pub name: String,
    pub desc: String,
    pub kind: Kind,
}

impl Site {
    fn render(&self) -> String {
        match self.kind {
            Kind::Instantiate => format!("{}::<new>", self.owner),
            Kind::ClassRef => format!("{}::<class>", self.owner),
            _ => format!(
                "{}.{}{} {}",
                self.owner,
                self.name,
                self.desc,
                self.kind.label()
            ),
        }
    }
}

/// The sorted, deduplicated set of NON-USER references in the program
/// (user-class sites are the program's own business and always
/// resolve). Includes class-reference sites (catch types, instanceof/
/// checkcast/ANewArray element types, ldc Class constants).
pub fn collect(prog: &Program) -> Vec<Site> {
    let mut set: BTreeSet<Site> = BTreeSet::new();
    let mut push = |owner: &str, name: &str, desc: &str, kind: Kind| {
        if !prog.is_user(owner) {
            set.insert(Site {
                owner: owner.to_string(),
                name: name.to_string(),
                desc: desc.to_string(),
                kind,
            });
        }
    };
    for c in &prog.classes {
        for m in &c.methods {
            for (_pc, instr) in &m.instrs {
                match instr {
                    ir::Instr::InvokeStatic { class, name, desc }
                    | ir::Instr::InvokeSpecial { class, name, desc }
                    | ir::Instr::InvokeVirtual { class, name, desc }
                    | ir::Instr::InvokeInterface { class, name, desc } => {
                        push(class, name, desc, Kind::Call);
                    }
                    ir::Instr::GetStatic { class, field, .. }
                    | ir::Instr::PutStatic { class, field, .. }
                    | ir::Instr::GetField { class, field, .. }
                    | ir::Instr::PutField { class, field, .. } => {
                        push(class, field, "", Kind::Field);
                    }
                    ir::Instr::New { class } => push(class, "<init>", "", Kind::Instantiate),
                    ir::Instr::ANewArray { elem }
                    | ir::Instr::CheckCast { class: elem, .. }
                    | ir::Instr::InstanceOf { class: elem, .. } => {
                        push(elem, "", "", Kind::ClassRef);
                    }
                    ir::Instr::Ldc(ir::Ldc::Class(name)) => {
                        push(name, "", "", Kind::ClassRef);
                    }
                    _ => {}
                }
            }
            // catch types ride the exception table, not the instr stream
            for (_a, _b, _h, catch) in &m.exception_table {
                if let Some(ct) = catch {
                    push(ct, "", "", Kind::ClassRef);
                }
            }
        }
    }
    set.into_iter().collect()
}

/// `None` when the compiler's own law accepts the site; `Some(reason)`
/// when it would fail. The predicates mirror validate_instr exactly.
pub fn classify(prog: &Program, site: &Site) -> Option<String> {
    match site.kind {
        Kind::Call => crate::resolve_call(prog, &site.owner, &site.name, &site.desc)
            .err()
            .map(|e| e.to_string()),
        Kind::Field => {
            // the validate_instr GetStatic special cases
            if site.owner == "java/lang/System" && (site.name == "out" || site.name == "err") {
                return None;
            }
            if site.owner == "java/util/Locale" && site.name == "US" {
                return None;
            }
            if site.name == "TYPE" && crate::primitive_type_field(&site.owner).is_some() {
                return None;
            }
            crate::resolve_field(prog, &site.owner, &site.name, true)
                .err()
                .map(|e| e.to_string())
        }
        Kind::Instantiate => {
            if prog.is_user(&site.owner) {
                return None;
            }
            if crate::builtin_is_instantiable(&site.owner) {
                None
            } else {
                Some(format!(
                    "instantiation of JDK class {} is not supported",
                    site.owner
                ))
            }
        }
        Kind::ClassRef => {
            let e = &site.owner;
            if prog.is_user(e)
                || e == "java/lang/Object"
                || builtin::builtin_parent_of(e).is_some()
                || builtin::builtin_is_interface(e)
                || e.starts_with('[')
            {
                None
            } else {
                Some(format!("unknown class reference {e}"))
            }
        }
    }
}

/// The one-page report. `diagnostics_only` prints only the MISSING rows
/// grouped by owning class (H-2); the full mode prints every site
/// sorted, OK first-class citizens included (H-1). Returns the number
/// of MISSING sites.
pub fn render(prog: &Program, diagnostics_only: bool) -> (String, usize) {
    let sites = collect(prog);
    let mut ok = 0usize;
    let mut missing: Vec<(String, &Site)> = Vec::new();
    for s in &sites {
        match classify(prog, s) {
            None => ok += 1,
            Some(reason) => missing.push((reason, s)),
        }
    }
    let mut out = String::new();
    if diagnostics_only {
        out.push_str(&format!(
            "j2r surface diagnostics: {} jdk sites, {} missing\n",
            sites.len(),
            missing.len()
        ));
        for (reason, s) in &missing {
            out.push_str(&format!("  MISSING {} — {reason}\n", s.render()));
        }
    } else {
        out.push_str(&format!(
            "j2r surface report: {} jdk sites ({} ok, {} missing)\n",
            sites.len(),
            ok,
            missing.len()
        ));
        for s in &sites {
            let mark = if missing.iter().any(|(_, m)| *m == s) {
                "MISSING"
            } else {
                "ok     "
            };
            out.push_str(&format!("  {mark} {}\n", s.render()));
        }
    }
    (out, missing.len())
}
