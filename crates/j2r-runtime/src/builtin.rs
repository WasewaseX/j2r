//! Builtin JDK class knowledge: hierarchy (for subtype checks and dispatch
//! walks) and field-slot layout (so user classes extending builtin classes
//! get consistent flattened slot indices).

/// Direct superclass of a builtin class, or `None` when the class is unknown
/// to the runtime or is `java/lang/Object` (the root).
pub fn builtin_parent_of(class: &str) -> Option<&'static str> {
    let p = match class {
        "java/lang/String" => "java/lang/Object",
        "java/lang/StringBuilder" => "java/lang/Object",
        "java/lang/StringBuffer" => "java/lang/Object",
        "java/lang/System" => "java/lang/Object",
        "java/lang/Math" => "java/lang/Object",
        "java/io/PrintStream" => "java/lang/Object",
        "java/lang/Number" => "java/lang/Object",
        "java/lang/Integer" => "java/lang/Number",
        "java/lang/Long" => "java/lang/Number",
        "java/lang/Float" => "java/lang/Number",
        "java/lang/Double" => "java/lang/Number",
        "java/lang/Byte" => "java/lang/Number",
        "java/lang/Short" => "java/lang/Number",
        "java/lang/Boolean" => "java/lang/Object",
        "java/lang/Character" => "java/lang/Object",
        "java/lang/Throwable" => "java/lang/Object",
        "java/lang/Exception" => "java/lang/Throwable",
        "java/lang/Error" => "java/lang/Throwable",
        "java/lang/RuntimeException" => "java/lang/Exception",
        "java/lang/ArithmeticException" => "java/lang/RuntimeException",
        "java/lang/NullPointerException" => "java/lang/RuntimeException",
        "java/lang/IndexOutOfBoundsException" => "java/lang/RuntimeException",
        "java/lang/ArrayIndexOutOfBoundsException" => "java/lang/IndexOutOfBoundsException",
        "java/lang/StringIndexOutOfBoundsException" => "java/lang/IndexOutOfBoundsException",
        "java/lang/NegativeArraySizeException" => "java/lang/RuntimeException",
        "java/lang/ClassCastException" => "java/lang/RuntimeException",
        "java/lang/ArrayStoreException" => "java/lang/RuntimeException",
        "java/lang/IllegalArgumentException" => "java/lang/RuntimeException",
        "java/lang/NumberFormatException" => "java/lang/IllegalArgumentException",
        "java/lang/IllegalStateException" => "java/lang/RuntimeException",
        "java/lang/UnsupportedOperationException" => "java/lang/RuntimeException",
        "java/lang/StackOverflowError" => "java/lang/VirtualMachineError",
        "java/lang/OutOfMemoryError" => "java/lang/VirtualMachineError",
        "java/lang/VirtualMachineError" => "java/lang/Error",
        "java/lang/AssertionError" => "java/lang/Error",
        _ => return None,
    };
    Some(p)
}

/// Interfaces a builtin class implements (beyond the single-parent chain).
pub fn builtin_interfaces_of(class: &str) -> &'static [&'static str] {
    match class {
        "java/lang/String" | "java/lang/StringBuilder" | "java/lang/StringBuffer" => {
            &["java/lang/CharSequence"]
        }
        _ => &[],
    }
}

/// Number of field slots a class owns itself (not counting inherited ones).
/// Slot 0 of `Throwable` is `detailMessage`; the boxed classes and
/// `StringBuilder` keep their payload in slot 0; `PrintStream` keeps its
/// sink id in slot 0.
fn builtin_own_slots(class: &str) -> Option<usize> {
    let n = match class {
        "java/lang/Object" | "java/lang/System" | "java/lang/Math" | "java/lang/Number" => 0,
        "java/lang/Throwable"
        | "java/lang/String"
        | "java/lang/StringBuilder"
        | "java/lang/StringBuffer"
        | "java/lang/Integer"
        | "java/lang/Long"
        | "java/lang/Float"
        | "java/lang/Double"
        | "java/lang/Byte"
        | "java/lang/Short"
        | "java/lang/Boolean"
        | "java/lang/Character"
        | "java/io/PrintStream" => 1,
        _ => {
            // exception families own no extra fields
            if builtin_parent_of(class).is_some() {
                0
            } else {
                return None;
            }
        }
    };
    Some(n)
}

/// Total flattened slot count of a builtin class (including inherited
/// slots). `None` when the class is unknown.
pub fn builtin_total_slots(class: &str) -> Option<usize> {
    let mut cur = class;
    let mut total = 0usize;
    for _ in 0..32 {
        total += builtin_own_slots(cur)?;
        match builtin_parent_of(cur) {
            Some(p) => cur = p,
            None => return Some(total),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exception_slot_counts() {
        assert_eq!(builtin_total_slots("java/lang/Object"), Some(0));
        assert_eq!(builtin_total_slots("java/lang/Throwable"), Some(1));
        assert_eq!(builtin_total_slots("java/lang/Exception"), Some(1));
        assert_eq!(
            builtin_total_slots("java/lang/ArrayIndexOutOfBoundsException"),
            Some(1)
        );
        assert_eq!(builtin_total_slots("java/lang/StringBuilder"), Some(1));
        assert_eq!(builtin_total_slots("java/lang/Integer"), Some(1));
        assert_eq!(builtin_total_slots("java/lang/Unknown"), None);
    }
}
