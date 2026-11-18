//! The M5 wave-3 mappings model: a parsed Tiny v2 file — namespaces,
//! class rows with their field/method/param members — with both-direction
//! lookups by (namespace, name) and by (owner, descriptor, name).
//!
//! A `MappingSet` is the data the wave-3 remap stage (T15) reads; the
//! parser is the only way to build one, and every malformed shape is a
//! loud error carrying a line number (never a skipped row — a silently
//! dropped mapping is a wrong-name compile later).
//!
//! The format law (verified against the real Fabric intermediary 1.21.4
//! map, 93 017 lines, captured 2026-11-18): a tab-separated,
//! indent-nested line format. Indent 0: class rows (`c`). Indent 1:
//! members — `f` field, `m` method, `c` inner class — each carrying the
//! descriptor first, then one name column per namespace. Indent 2 under
//! a method: `p` parameter rows (index first). Column count is exactly
//! the header's namespace count for name lists; an empty column means
//! "no name in this namespace" (kept as `None`, never silently dropped).

use std::collections::HashMap;

pub mod parser;

pub use parser::{parse, ParseError};

/// One row's names: `names[i]` is the entry's name in `namespaces[i]`;
/// `None` = the namespace does not name this entry.
pub type Names = Vec<Option<String>>;

/// A field or method row: descriptor (in the file's source-namespace
/// grammar, verbatim) + the per-namespace names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberEntry {
    pub desc: String,
    pub names: Names,
    /// Parameter rows under a method (`p` lines); empty for fields and
    /// for methods the file carries no params for.
    pub params: Vec<ParamEntry>,
}

/// A parameter row under a method: the slot index (javac's parameter
/// order) + the per-namespace names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamEntry {
    pub index: u32,
    pub names: Names,
}

/// A class row: per-namespace names + the members nested under it.
/// Inner-class rows (`c` at indent 1) nest here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClassEntry {
    pub names: Names,
    pub fields: Vec<MemberEntry>,
    pub methods: Vec<MemberEntry>,
    pub inner_classes: Vec<ClassEntry>,
}

/// A parsed Tiny v2 file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MappingSet {
    /// The header's namespace list, in file order. Column `i` of every
    /// row is namespace `namespaces[i]`.
    pub namespaces: Vec<String>,
    /// The class rows in file order.
    pub classes: Vec<ClassEntry>,
    /// namespace index → name → the tree path to the class row
    /// (`classes[i]` → `inner_classes[j]` → …). Inner classes carry
    /// their own binary names, so the walk is the full tree.
    by_ns: Vec<HashMap<String, Vec<usize>>>,
}

/// Resolves a tree path built by `index`.
fn class_at<'a>(classes: &'a [ClassEntry], path: &[usize]) -> &'a ClassEntry {
    let (head, rest) = path.split_first().expect("index paths are never empty");
    let entry = &classes[*head];
    if rest.is_empty() {
        entry
    } else {
        class_at(&entry.inner_classes, rest)
    }
}

impl MappingSet {
    /// Builds the lookup indexes. Called by the parser; public so a
    /// hand-assembled set (tests, future remap tooling) can index too.
    pub fn index(mut self) -> Self {
        let mut by_ns: Vec<HashMap<String, Vec<usize>>> =
            vec![HashMap::new(); self.namespaces.len()];
        fn walk(
            classes: &[ClassEntry],
            path: &mut Vec<usize>,
            by_ns: &mut [HashMap<String, Vec<usize>>],
        ) {
            for (ci, class) in classes.iter().enumerate() {
                path.push(ci);
                for (ni, name) in class.names.iter().enumerate() {
                    if let Some(name) = name {
                        by_ns[ni].insert(name.clone(), path.clone());
                    }
                }
                walk(&class.inner_classes, path, by_ns);
                path.pop();
            }
        }
        walk(&self.classes, &mut Vec::new(), &mut by_ns);
        self.by_ns = by_ns;
        self
    }

    /// The class entry whose name in `ns` is `name`.
    pub fn class(&self, ns: usize, name: &str) -> Option<&ClassEntry> {
        let path = self.by_ns.get(ns)?.get(name)?;
        Some(class_at(&self.classes, path))
    }

    /// The class's name in `to_ns`, given its name in `from_ns`.
    /// Namespace indexes are validated by construction; a wrong-namespace
    /// name simply misses.
    pub fn map_class(&self, from_ns: usize, to_ns: usize, name: &str) -> Option<&str> {
        self.class(from_ns, name)?.names.get(to_ns)?.as_deref()
    }

    /// The field's name in `to_ns`: owner + descriptor + name, all in
    /// `from_ns` grammar. The descriptor distinguishes the overload
    /// space for methods; fields are unique per (owner, name, desc).
    pub fn map_field(
        &self,
        from_ns: usize,
        to_ns: usize,
        owner: &str,
        desc: &str,
        name: &str,
    ) -> Option<&str> {
        let class = self.class(from_ns, owner)?;
        let member = class
            .fields
            .iter()
            .find(|m| m.names[from_ns].as_deref() == Some(name) && m.desc == desc)?;
        member.names[to_ns].as_deref()
    }

    /// The method's name in `to_ns`: owner + descriptor + name in
    /// `from_ns` grammar. Overloads disambiguate by descriptor.
    pub fn map_method(
        &self,
        from_ns: usize,
        to_ns: usize,
        owner: &str,
        desc: &str,
        name: &str,
    ) -> Option<&str> {
        let class = self.class(from_ns, owner)?;
        let member = class
            .methods
            .iter()
            .find(|m| m.names[from_ns].as_deref() == Some(name) && m.desc == desc)?;
        member.names[to_ns].as_deref()
    }

    /// The method's parameter names in `to_ns`, in slot order (only the
    /// rows the file carries).
    pub fn method_params(
        &self,
        from_ns: usize,
        to_ns: usize,
        owner: &str,
        desc: &str,
        name: &str,
    ) -> Vec<&str> {
        let Some(class) = self.class(from_ns, owner) else {
            return Vec::new();
        };
        let Some(member) = class
            .methods
            .iter()
            .find(|m| m.names[from_ns].as_deref() == Some(name) && m.desc == desc)
        else {
            return Vec::new();
        };
        let mut params: Vec<&str> = Vec::new();
        for p in &member.params {
            if let Some(name) = p.names.get(to_ns).and_then(|n| n.as_deref()) {
                params.push(name);
            }
        }
        params
    }
}
