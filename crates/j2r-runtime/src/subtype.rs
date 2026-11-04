//! Java subtype checks (`instanceof`, `checkcast`, exception-handler
//! matching) over the combined class universe: builtin JDK classes known to
//! the runtime plus the classes being compiled (provided via
//! [`crate::UserParents`]).

use crate::builtin;
use crate::UserParents;

/// Single-parent lookup used by virtual dispatch: compiled classes first,
/// then builtin classes.
pub fn parent_of(class: &str, user: Option<crate::UserParents>) -> Option<&'static str> {
    if let Some(u) = user {
        if let Some(parents) = u(class) {
            return Some(parents[0]);
        }
    }
    builtin::builtin_parent_of(class)
}

fn array_elem(class: &str) -> &str {
    // "[LFoo;" -> "Foo", "[I" -> "I", "[[J" -> "[J"
    if let Some(rest) = class.strip_prefix("[L") {
        rest.strip_suffix(';').unwrap_or(rest)
    } else {
        &class[1..]
    }
}

/// Java subtype relation. Handles builtin classes, compiled classes (via the
/// generated parents table), arrays (exact, covariance over reference element
/// types, and the universal array super-types) and `java/lang/Object`.
pub fn is_subtype(child: &str, parent: &str, user: Option<UserParents>) -> bool {
    if child == parent {
        return true;
    }
    if parent == "java/lang/Object" {
        return true;
    }
    if child.starts_with('[') {
        if parent == "java/lang/Cloneable" || parent == "java/io/Serializable" {
            return true;
        }
        if parent.starts_with('[') {
            let ce = array_elem(child);
            let pe = array_elem(parent);
            if ce.starts_with('[') {
                // element is itself an array (e.g. `int[][] <: Object[]`)
                return is_subtype(ce, pe, user);
            }
            if child.starts_with("[L") && parent.starts_with("[L") {
                // reference arrays are covariant in their element type
                return is_ref_subtype(ce, pe, user);
            }
            // primitive-element arrays are only assignable to identical types
            return false;
        }
        return false;
    }
    is_ref_subtype(child, parent, user)
}

fn is_ref_subtype(child: &str, parent: &str, user: Option<UserParents>) -> bool {
    let mut frontier: Vec<&str> = vec![child];
    let mut visited: Vec<&str> = vec![child];
    let mut steps = 0usize;
    while let Some(c) = frontier.pop() {
        let mut parents: Vec<&'static str> = Vec::with_capacity(2);
        if let Some(u) = user {
            if let Some(ps) = u(c) {
                parents.extend_from_slice(ps);
            }
        }
        if let Some(p) = builtin::builtin_parent_of(c) {
            parents.push(p);
        }
        for extra in builtin::builtin_interfaces_of(c) {
            parents.push(extra);
        }
        for p in parents {
            if p == parent {
                return true;
            }
            if !visited.contains(&p) {
                visited.push(p);
                frontier.push(p);
            }
        }
        steps += 1;
        if steps > 256 {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parents_of(class: &str) -> Option<&'static [&'static str]> {
        match class {
            "MyEx" => Some(&["java/lang/Exception"]),
            "Shape" => Some(&["java/lang/Object", "Iface"]),
            "Iface" => Some(&["java/lang/Object"]),
            _ => None,
        }
    }

    #[test]
    fn builtin_chain() {
        let _rt = crate::test_support::runtime();
        assert!(is_subtype(
            "java/lang/ArrayIndexOutOfBoundsException",
            "java/lang/Exception",
            None
        ));
        assert!(is_subtype(
            "java/lang/ArithmeticException",
            "java/lang/Throwable",
            None
        ));
        assert!(!is_subtype("java/lang/String", "java/lang/Throwable", None));
    }

    #[test]
    fn user_classes_and_interfaces() {
        let _rt = crate::test_support::runtime();
        assert!(is_subtype("MyEx", "java/lang/Throwable", Some(parents_of)));
        assert!(is_subtype("Shape", "Iface", Some(parents_of)));
        assert!(!is_subtype("Iface", "Shape", Some(parents_of)));
    }

    #[test]
    fn builtin_interfaces() {
        let _rt = crate::test_support::runtime();
        assert!(is_subtype(
            "java/lang/String",
            "java/lang/CharSequence",
            None
        ));
    }

    #[test]
    fn arrays() {
        let _rt = crate::test_support::runtime();
        assert!(is_subtype("[I", "[I", None));
        assert!(is_subtype("[I", "java/lang/Object", None));
        assert!(is_subtype("[I", "java/lang/Cloneable", None));
        assert!(is_subtype(
            "[Ljava/lang/String;",
            "[Ljava/lang/Object;",
            None
        ));
        assert!(is_subtype(
            "[Ljava/lang/Exception;",
            "[Ljava/lang/Throwable;",
            Some(parents_of)
        ));
        assert!(!is_subtype(
            "[Ljava/lang/Object;",
            "[Ljava/lang/String;",
            None
        ));
        assert!(!is_subtype("[I", "[J", None));
        assert!(!is_subtype("[I", "[Ljava/lang/Object;", None));
    }
}
