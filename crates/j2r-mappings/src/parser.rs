//! The Tiny v2 parser (M5_PLAN §4 wave 3, T14): strict, line-oriented,
//! loud errors with line numbers. Grammar verified against the real
//! Fabric intermediary 1.21.4 map (93 017 lines, captured 2026-11-18) —
//! header `tiny\t<major>\t<minor>\t<namespaces…>`, indent-nested rows
//! (`c` classes, `f` fields, `m` methods, `p` params), one name column
//! per header namespace, empty column = unnamed there.
//!
//! Nesting law: a row at tab-depth d attaches to the enclosing context
//! at depth d-1 — classes nest under classes (inner classes), members
//! under the enclosing class, params under the enclosing method. A depth
//! jump with no matching context, an unknown kind char, a wrong column
//! count, or a member row without its descriptor is an error naming the
//! line — never a skipped row (a silently dropped mapping is a
//! wrong-name compile later).

use crate::{ClassEntry, MappingSet, MemberEntry, Names, ParamEntry};
use std::fmt;

/// A parse failure: the 1-based line and what the law expects instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tiny v2 parse error at line {}: {}",
            self.line, self.message
        )
    }
}

/// Parses a whole Tiny v2 document. `\r\n` tolerated, blank lines
/// skipped, everything else strict.
pub fn parse(text: &str) -> Result<MappingSet, ParseError> {
    let mut lines = text.lines().enumerate();
    let (hidx, header) = lines.next().ok_or(ParseError {
        line: 1,
        message: "empty document (expected the `tiny<TAB>2` header)".into(),
    })?;
    let mut set = parse_header(hidx + 1, header)?;
    let ns_count = set.namespaces.len();

    // The context stack: a row at depth d attaches to the frame at
    // depth d-1 (Root owns the depth-0 rows).
    let mut stack: Vec<Frame> = vec![Frame::Root];
    for (idx, raw) in lines {
        let lineno = idx + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.trim().is_empty() {
            continue;
        }
        let depth = line.chars().take_while(|&c| c == '\t').count();
        let body = &line[depth..];
        let fields: Vec<&str> = body.split('\t').collect();
        if fields.first().is_none_or(|f| f.is_empty()) {
            return Err(ParseError {
                line: lineno,
                message: format!("row has no kind char (body {body:?})"),
            });
        }
        // Pop contexts the row cannot nest under: anything at or below
        // the row's depth. Root never pops (it owns the depth-0 rows).
        while let Some(top) = stack.last() {
            if matches!(top, Frame::Root) || top.depth() < depth {
                break;
            }
            stack.pop();
        }
        let attach_depth = stack.last().expect("stack keeps Root").depth();
        if attach_depth != depth.saturating_sub(1) {
            return Err(ParseError {
                line: lineno,
                message: format!(
                    "depth {depth} row has no enclosing context at depth {} (nesting law: a row attaches at its parent's depth + 1)",
                    depth.saturating_sub(1)
                ),
            });
        }
        match stack.last().expect("stack keeps Root") {
            Frame::Root => parse_class_row(&mut set, &mut stack, depth, lineno, &fields, false)?,
            Frame::Class { path, .. } => {
                if fields[0] == "c" {
                    let names = name_cols(&fields[1..], ns_count, lineno)?;
                    let entry = class_entry_mut(&mut set.classes, path);
                    entry.inner_classes.push(ClassEntry {
                        names,
                        ..Default::default()
                    });
                    let mut sub = path.clone();
                    sub.push(entry.inner_classes.len() - 1);
                    stack.push(Frame::Class { depth, path: sub });
                } else if fields[0] == "f" || fields[0] == "m" {
                    let member = member_row(lineno, &fields, ns_count)?;
                    let is_method = fields[0] == "m";
                    let entry = class_entry_mut(&mut set.classes, path);
                    if is_method {
                        entry.methods.push(member);
                        let method_idx = entry.methods.len() - 1;
                        stack.push(Frame::Method {
                            depth,
                            owner_class_path: path.clone(),
                            method_idx,
                        });
                    } else {
                        entry.fields.push(member);
                    }
                } else {
                    return Err(ParseError {
                        line: lineno,
                        message: format!(
                            "member rows are `f` (field), `m` (method) or `c` (inner class), got kind {:?}",
                            fields[0]
                        ),
                    });
                }
            }
            Frame::Method {
                owner_class_path,
                method_idx,
                ..
            } => {
                if fields[0] != "p" {
                    return Err(ParseError {
                        line: lineno,
                        message: format!(
                            "rows under a method are parameter rows (`p`), got kind {:?}",
                            fields[0]
                        ),
                    });
                }
                let index = fields.get(1).ok_or(ParseError {
                    line: lineno,
                    message: "param rows carry the slot index after the kind char".into(),
                })?;
                let index: u32 = index.parse().map_err(|_| ParseError {
                    line: lineno,
                    message: format!("param index {index:?} is not a u32"),
                })?;
                let names = name_cols(&fields[2..], ns_count, lineno)?;
                let class = class_entry_mut(&mut set.classes, owner_class_path);
                class.methods[*method_idx]
                    .params
                    .push(ParamEntry { index, names });
            }
        }
    }
    Ok(set.index())
}

enum Frame {
    Root,
    /// A class context (top-level or inner): rows at depth+1 attach.
    /// `path` walks the tree (classes[i] → inner_classes[j] → …).
    Class {
        depth: usize,
        path: Vec<usize>,
    },
    /// A method context: param rows at depth+1 attach.
    Method {
        depth: usize,
        owner_class_path: Vec<usize>,
        method_idx: usize,
    },
}

impl Frame {
    fn depth(&self) -> usize {
        match self {
            Frame::Root => 0,
            Frame::Class { depth, .. } | Frame::Method { depth, .. } => *depth,
        }
    }
}

fn parse_class_row(
    set: &mut MappingSet,
    stack: &mut Vec<Frame>,
    depth: usize,
    lineno: usize,
    fields: &[&str],
    _nested: bool,
) -> Result<(), ParseError> {
    if fields[0] != "c" {
        return Err(ParseError {
            line: lineno,
            message: format!(
                "top-level rows are class rows (`c`), got kind {:?}",
                fields[0]
            ),
        });
    }
    let names = name_cols(&fields[1..], set.namespaces.len(), lineno)?;
    set.classes.push(ClassEntry {
        names,
        ..Default::default()
    });
    stack.push(Frame::Class {
        depth,
        path: vec![set.classes.len() - 1],
    });
    Ok(())
}

fn member_row(lineno: usize, fields: &[&str], ns_count: usize) -> Result<MemberEntry, ParseError> {
    let desc = fields.get(1).copied().ok_or(ParseError {
        line: lineno,
        message: "member rows carry a descriptor after the kind char".into(),
    })?;
    if desc.is_empty() {
        return Err(ParseError {
            line: lineno,
            message: "member descriptor is empty".into(),
        });
    }
    let names = name_cols(&fields[2..], ns_count, lineno)?;
    Ok(MemberEntry {
        desc: desc.to_string(),
        names,
        params: Vec::new(),
    })
}

fn class_entry_mut<'a>(classes: &'a mut [ClassEntry], path: &[usize]) -> &'a mut ClassEntry {
    let (head, rest) = path.split_first().expect("class path is never empty");
    let entry = &mut classes[*head];
    if rest.is_empty() {
        entry
    } else {
        class_entry_mut(&mut entry.inner_classes, rest)
    }
}

fn name_cols(fields: &[&str], ns_count: usize, line: usize) -> Result<Names, ParseError> {
    if fields.len() != ns_count {
        return Err(ParseError {
            line,
            message: format!(
                "name column count {} != header namespace count {ns_count} (empty namespaces stay as empty columns)",
                fields.len()
            ),
        });
    }
    Ok(fields
        .iter()
        .map(|f| {
            if f.is_empty() {
                None
            } else {
                Some(f.to_string())
            }
        })
        .collect())
}

fn parse_header(lineno: usize, header: &str) -> Result<MappingSet, ParseError> {
    let header = header.strip_suffix('\r').unwrap_or(header);
    let fields: Vec<&str> = header.split('\t').collect();
    if fields.first() != Some(&"tiny") {
        return Err(ParseError {
            line: lineno,
            message: format!("document must start with the `tiny` header, got {header:?}"),
        });
    }
    let major = fields.get(1).copied().ok_or(ParseError {
        line: lineno,
        message: "header lacks the format major version".into(),
    })?;
    if major != "2" {
        return Err(ParseError {
            line: lineno,
            message: format!("unsupported tiny format {major:?} (this parser pins v2)"),
        });
    }
    fields.get(2).ok_or(ParseError {
        line: lineno,
        message: "header lacks the format minor version".into(),
    })?;
    let namespaces: Vec<String> = fields[3..].iter().map(|f| f.to_string()).collect();
    if namespaces.len() < 2 {
        return Err(ParseError {
            line: lineno,
            message: format!(
                "a usable mapping file names at least two namespaces, header carries {}",
                namespaces.len()
            ),
        });
    }
    if namespaces.iter().any(|n| n.is_empty()) {
        return Err(ParseError {
            line: lineno,
            message: "namespace names must be non-empty".into(),
        });
    }
    Ok(MappingSet {
        namespaces,
        classes: Vec::new(),
        by_ns: Vec::new(),
    })
}
