//! Classpath resolution and transitive closure loading (M3_PLAN §4).
//!
//! Model = the JVM application class loader, simplified to the
//! ahead-of-time whole-program shape M1 already uses:
//! * `-cp` entries (dirs, `.jar`, `.zip`) are searched **in order**;
//!   first match wins (JVM classloading convention);
//! * starting from the explicit inputs and/or the main class, every
//!   class named by a `CONSTANT_Class` entry of any loaded class is
//!   loaded too — the generated crate stays self-contained (gate-5
//!   shape unchanged);
//! * JDK namespaces (`java/`, `javax/`, `jdk/`, `sun/`, `com/sun/`) are
//!   never loaded from the classpath — they resolve through the runtime
//!   natives table or fail loudly (M1 discipline).
//!
//! Robustness: archive corruption surfaces as a compile error with the
//! entry name and cause (the zero-dep reader's `Err` discipline,
//! M3_PLAN §4.3); a missing referenced class is a compile error naming
//! the referer — the whole-program model requires a complete closure.

#![forbid(unsafe_code)]

use j2r_classfile::zip::Jar;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// One resolved class source: an explicit file, a file inside a
/// classpath directory, or an archive entry.
#[derive(Debug, Clone)]
pub enum ClassSource {
    /// Explicitly listed `.class` path (M1 behavior, wins over -cp).
    Explicit(PathBuf),
    /// `a/b/C.class` found in the directory entry `dir`.
    Dir { dir: PathBuf, rel: PathBuf },
    /// Archive entry `a/b/C.class` in `jar_index`-th classpath archive.
    Archive { archive: usize, entry: String },
}

impl std::fmt::Display for ClassSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClassSource::Explicit(p) => write!(f, "input file {}", p.display()),
            ClassSource::Dir { dir, rel } => write!(f, "{} in {}", rel.display(), dir.display()),
            ClassSource::Archive { archive, entry } => {
                write!(f, "entry {entry} of archive #{}", archive + 1)
            }
        }
    }
}

/// A parsed classpath: ordered dir entries + fully parsed archives.
pub struct Classpath {
    dirs: Vec<PathBuf>,
    /// Parsed once at construction; entries looked up by exact name.
    archives: Vec<(PathBuf, Jar<'static>)>,
}

/// Resolver failure — always a compile error for the caller.
#[derive(Debug)]
pub struct ClasspathError {
    pub message: String,
}

impl std::fmt::Display for ClasspathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Classpath {
    /// Open every entry up front (archives are parsed once; dirs are
    /// searched per lookup). Fails loudly on missing/unreadable entries
    /// and corrupt archives — the compile cannot proceed with a partial
    /// classpath.
    pub fn new(entries: &[PathBuf]) -> Result<Classpath, ClasspathError> {
        let mut dirs = Vec::new();
        let mut archives = Vec::new();
        for e in entries {
            let meta = std::fs::metadata(e).map_err(|io| ClasspathError {
                message: format!("classpath entry {} is not readable: {io}", e.display()),
            })?;
            if meta.is_dir() {
                dirs.push(e.clone());
                continue;
            }
            let lower = e.to_string_lossy().to_lowercase();
            if !(lower.ends_with(".jar") || lower.ends_with(".zip")) {
                return Err(ClasspathError {
                    message: format!(
                        "classpath entry {} is neither a directory nor a .jar/.zip file",
                        e.display()
                    ),
                });
            }
            let bytes = std::fs::read(e).map_err(|io| ClasspathError {
                message: format!("classpath archive {} is not readable: {io}", e.display()),
            })?;
            // Leak the archive bytes so `Jar<'a>` borrows forever —
            // acceptable at compile time (one archive set per compile);
            // documented in M3_PLAN §4.3.
            let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
            let jar = Jar::parse(leaked).map_err(|ze| ClasspathError {
                message: format!("classpath archive {} failed to parse: {ze}", e.display()),
            })?;
            archives.push((e.clone(), jar));
        }
        Ok(Classpath { dirs, archives })
    }

    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty() && self.archives.is_empty()
    }

    pub fn archive_count(&self) -> usize {
        self.archives.len()
    }

    /// First-match-wins lookup of `a/b/C.class` across dirs, then
    /// archives (both in classpath order).
    pub fn find(&self, internal_name: &str) -> Option<ClassSource> {
        let rel = PathBuf::from(format!("{internal_name}.class"));
        for dir in &self.dirs {
            let path = dir.join(&rel);
            if path.is_file() {
                return Some(ClassSource::Dir {
                    dir: dir.clone(),
                    rel,
                });
            }
        }
        let entry_name = format!("{internal_name}.class");
        for (i, (_, jar)) in self.archives.iter().enumerate() {
            if jar.entry(&entry_name).is_some() {
                return Some(ClassSource::Archive {
                    archive: i,
                    entry: entry_name,
                });
            }
        }
        None
    }

    /// Read the raw bytes of a resolved source.
    pub fn read(&self, src: &ClassSource) -> Result<Vec<u8>, ClasspathError> {
        match src {
            ClassSource::Explicit(p) => std::fs::read(p).map_err(|io| ClasspathError {
                message: format!("cannot read {}: {io}", p.display()),
            }),
            ClassSource::Dir { dir, rel } => {
                let full = dir.join(rel);
                std::fs::read(&full).map_err(|io| ClasspathError {
                    message: format!("cannot read {}: {io}", full.display()),
                })
            }
            ClassSource::Archive { archive, entry } => {
                let (_, jar) = &self.archives[*archive];
                jar.read(entry).map_err(|ze| ClasspathError {
                    message: format!("archive entry {entry}: {ze}"),
                })
            }
        }
    }
}

/// Decode a `CONSTANT_Class` pool name into the referenced class:
/// * a bare internal name (`a/b/C`, `Main`) is returned as-is;
/// * an array descriptor (`[La/b/C;`, `[[Ljava/lang/String;`) yields the
///   eventual component class;
/// * an all-primitive array (`[I`, `[[J`) names no class → `None`.
pub fn component_class_of(name: &str) -> Option<String> {
    let mut brackets = 0usize;
    let mut cur = name;
    while let Some(rest) = cur.strip_prefix('[') {
        cur = rest;
        brackets += 1;
    }
    if brackets == 0 {
        // Plain class reference (primitives never appear bare in a
        // CONSTANT_Class entry).
        return Some(name.to_string());
    }
    // Array descriptor: the component is either `L…;` or a primitive.
    cur.strip_prefix('L')
        .and_then(|s| s.strip_suffix(';'))
        .map(|s| s.to_string())
}

/// The JDK namespaces never loaded from a classpath (M3_PLAN §4.2).
pub fn is_jdk_namespace(internal_name: &str) -> bool {
    internal_name.starts_with("java/")
        || internal_name.starts_with("javax/")
        || internal_name.starts_with("jdk/")
        || internal_name.starts_with("sun/")
        || internal_name.starts_with("com/sun/")
}

/// Walk the class-references of one parsed class and return the names a
/// closure loader may need to fetch: deduplicated, arrays stripped,
/// primitives and JDK namespaces dropped, and the class itself dropped.
pub fn referenced_user_classes(cf: &j2r_classfile::ClassFile) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let refs = cf.referenced_classes().unwrap_or_default();
    for r in refs {
        let Some(name) = component_class_of(&r) else {
            continue;
        };
        if is_jdk_namespace(&name) {
            continue;
        }
        if name == cf.this_class {
            continue;
        }
        out.insert(name);
    }
    out
}

/// Summary of one closure-loading run (for evidence and messages).
#[derive(Debug, Default)]
pub struct ClosureStats {
    /// Classes loaded from classpath archives/dirs (not explicit inputs).
    pub from_classpath: BTreeMap<String, ClassSource>,
    /// Total classes in the final closure (explicit + classpath).
    pub total: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_class_decodes_arrays() {
        assert_eq!(component_class_of("a/b/C"), Some("a/b/C".to_string()));
        assert_eq!(component_class_of("[La/b/C;"), Some("a/b/C".to_string()));
        assert_eq!(
            component_class_of("[[Ljava/lang/String;"),
            Some("java/lang/String".to_string())
        );
        assert_eq!(component_class_of("[I"), None);
        assert_eq!(component_class_of("[[J"), None);
        // A bare `L…;` never appears in a CONSTANT_Class entry — it is
        // returned verbatim like any other plain name.
        assert_eq!(
            component_class_of("Ljava/lang/String;"),
            Some("Ljava/lang/String;".to_string())
        );
        // Default-package class: no slash, still a plain name.
        assert_eq!(component_class_of("Main"), Some("Main".to_string()));
    }

    #[test]
    fn jdk_namespace_detection() {
        assert!(is_jdk_namespace("java/util/ArrayList"));
        assert!(is_jdk_namespace("javax/crypto/Cipher"));
        assert!(is_jdk_namespace("jdk/internal/misc/Unsafe"));
        assert!(is_jdk_namespace("sun/misc/Unsafe"));
        assert!(is_jdk_namespace("com/sun/jna/Native"));
        assert!(!is_jdk_namespace("com/example/App"));
        assert!(!is_jdk_namespace("javafx/scene/Node")); // "java" prefix but not java/
    }
}
