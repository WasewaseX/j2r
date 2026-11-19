use super::*;

/// The generated crate's symbol tables, passed as a plain value so native
/// implementations can dispatch back into compiled code (e.g. `toString`
/// during string concatenation).
#[derive(Copy, Clone, Default)]
pub struct Dispatch {
    pub lookup: Option<UserLookup>,
    pub parents: Option<UserParents>,
    /// Reflection-subset class facts (M3 Wave 5).
    pub meta: Option<UserMeta>,
    /// Static-field pool access for `Field.get/set` on static fields:
    /// read/write pool slot by absolute index.
    pub static_get: Option<fn(u32) -> Value>,
    pub static_set: Option<fn(u32, &Value)>,
}
impl Dispatch {
    pub fn new(lookup: Option<UserLookup>, parents: Option<UserParents>) -> Self {
        Dispatch {
            lookup,
            parents,
            meta: None,
            static_get: None,
            static_set: None,
        }
    }

    pub fn find_method(&self, class: &str, name: &str, desc: &str) -> Option<MethodFn> {
        self.lookup.and_then(|u| u(class, name, desc))
    }

    pub fn parent_of(&self, class: &str) -> Option<&'static str> {
        subtype::parent_of(class, self.parents)
    }

    /// The FULL parents array of a compiled class (superclass first, then
    /// interfaces) — the interface-default resolution walks the interface
    /// edges, which `parent_of` (parents[0] only) never visits. `None`
    /// when the class is not compiled (builtin callers use
    /// `subtype::builtin_parent_of`).
    pub fn parents_of_full(&self, class: &str) -> Option<&'static [&'static str]> {
        let u = self.parents?;
        u(class)
    }

    pub fn is_subtype(&self, child: &str, parent: &str) -> bool {
        is_subtype(child, parent, self.parents)
    }

    /// Compiled-class facts, `None` for builtin/unknown classes.
    pub fn class_info(&self, class: &str) -> Option<&'static UserClassInfo> {
        self.meta.and_then(|m| m(class))
    }

    /// Is `class` a compiled interface? Falls back to the builtin table.
    pub fn is_interface(&self, class: &str) -> bool {
        if let Some(info) = self.class_info(class) {
            return info.is_interface;
        }
        crate::builtin::builtin_is_interface(class)
    }
}
/// Signature of a native (JDK) method implementation.
pub type NativeFn = fn(Option<&JRef>, &[Value], Dispatch) -> Result<Value, Throw>;
