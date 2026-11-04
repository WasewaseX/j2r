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
        // M4 Wave 2 (T06): java.lang.Thread joins the builtin hierarchy.
        "java/lang/Thread" => "java/lang/Object",
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
        // M4 Wave 3 (T13): monitor/conditioning exceptions. IMSE is thrown
        // by monitorexit and Object.wait/notify on a non-owned monitor;
        // InterruptedException by Thread.sleep/join/wait. Both must resolve
        // through the parent chain for catch-type verification and handler
        // matching.
        "java/lang/IllegalMonitorStateException" => "java/lang/RuntimeException",
        "java/lang/InterruptedException" => "java/lang/Exception",
        "java/lang/IllegalStateException" => "java/lang/RuntimeException",
        "java/lang/UnsupportedOperationException" => "java/lang/RuntimeException",
        "java/lang/StackOverflowError" => "java/lang/VirtualMachineError",
        "java/lang/OutOfMemoryError" => "java/lang/VirtualMachineError",
        "java/lang/VirtualMachineError" => "java/lang/Error",
        "java/lang/AssertionError" => "java/lang/Error",
        // M3 Wave 5: reflection subset (M3_PLAN §8). The reflective failure
        // classes hang off `ReflectiveOperationException` exactly like the
        // JDK so exception-handler matching (`catch (NoSuchFieldException)`
        // and friends) resolves through the builtin parent chain.
        "java/lang/Class" => "java/lang/Object",
        "java/lang/reflect/Field" => "java/lang/Object",
        "java/lang/reflect/Method" => "java/lang/Object",
        "java/lang/ReflectiveOperationException" => "java/lang/Exception",
        "java/lang/NoSuchFieldException" => "java/lang/ReflectiveOperationException",
        "java/lang/NoSuchMethodException" => "java/lang/ReflectiveOperationException",
        "java/lang/reflect/InvocationTargetException" => "java/lang/ReflectiveOperationException",
        // M3 Wave 2: java.util collections (M3_PLAN §5)
        "java/util/ArrayList" => "java/lang/Object",
        "java/util/ArrayList$Itr" => "java/lang/Object",
        "java/util/HashMap" => "java/lang/Object",
        "java/util/HashMap$Node" => "java/lang/Object",
        "java/util/HashMap$KeyIterator" => "java/lang/Object",
        "java/util/HashMap$ValueIterator" => "java/lang/Object",
        "java/util/HashMap$KeySet" => "java/lang/Object",
        "java/util/HashMap$Values" => "java/lang/Object",
        "java/util/HashSet" => "java/lang/Object",
        "java/util/Objects" => "java/lang/Object",
        "java/util/NoSuchElementException" => "java/lang/RuntimeException",
        "java/util/ConcurrentModificationException" => "java/lang/RuntimeException",
        // M3 Wave 3: functional interfaces (M3_PLAN §6.1). Closures carry
        // their interface as the object class; the specialization hierarchy
        // (UnaryOperator <: Function, BinaryOperator <: BiFunction) lives in
        // `builtin_interfaces_of`.
        "java/util/function/Function" => "java/lang/Object",
        "java/util/function/BiFunction" => "java/lang/Object",
        "java/util/function/Supplier" => "java/lang/Object",
        "java/util/function/Consumer" => "java/lang/Object",
        "java/util/function/BiConsumer" => "java/lang/Object",
        "java/util/function/Predicate" => "java/lang/Object",
        "java/util/function/BiPredicate" => "java/lang/Object",
        "java/util/function/UnaryOperator" => "java/lang/Object",
        "java/util/function/BinaryOperator" => "java/lang/Object",
        "java/util/Comparator" => "java/lang/Object",
        // Real-program surface (jdkutil.rs): utility classes are never
        // instantiated, the SubList view is.
        "java/util/Arrays" => "java/lang/Object",
        "java/util/Collections" => "java/lang/Object",
        "java/util/ArrayList$SubList" => "java/lang/Object",
        // Collections.unmodifiableList view (corpus wave 1: minimal-json's
        // unmodifiable JsonObject copy). One builtin object, one slot: the
        // backing list. Reads delegate, writes throw — JDK view semantics.
        "java/util/Collections$UnmodifiableList" => "java/lang/Object",
        // Real-world corpus wave 1 (minimal-json): java.io writer/reader
        // hierarchy. Found by corpus/MinimalJsonCorpus.java — real libraries
        // extend JDK IO types, so the hierarchy must be known to the loader
        // even before the behavior natives land.
        "java/io/Writer" => "java/lang/Object",
        "java/io/StringWriter" => "java/io/Writer",
        "java/io/Reader" => "java/lang/Object",
        "java/io/StringReader" => "java/io/Reader",
        // Real-world corpus wave 1, follow-up: the java.io exception
        // family. Every java.io native declares `throws IOException`, so
        // `catch (IOException)` handlers in compiled libraries must
        // resolve against the builtin chain exactly like the writer
        // hierarchy does. Parent links are the real JDK hierarchy.
        // Serialization surface (corpus wave 1): minimal-json implements
        // Serializable and declares a readObject hook. Hierarchy facts only —
        // the serialization MACHINERY does not exist and dispatching into it
        // fails loud with a named reason (natives.rs marker row).
        "java/io/InputStream" => "java/lang/Object",
        "java/io/ObjectInputStream" => "java/io/InputStream",
        "java/io/Serializable" => "java/lang/Object",
        "java/lang/ClassNotFoundException" => "java/lang/ReflectiveOperationException",
        "java/io/IOException" => "java/lang/Exception",
        "java/io/UnsupportedEncodingException" => "java/io/IOException",
        "java/io/EOFException" => "java/io/IOException",
        "java/io/FileNotFoundException" => "java/io/IOException",
        "java/io/InterruptedIOException" => "java/io/IOException",
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
        // M4 Wave 2 (T06): Thread implements Runnable.
        "java/lang/Thread" => &["java/lang/Runnable"],
        // M3 Wave 2: collection hierarchy (subtype walks traverse interfaces,
        // so List/Set/Collection/Iterable chain entries let
        // `ArrayList <: List <: Collection <: Iterable` resolve)
        "java/util/ArrayList" => &["java/util/List"],
        "java/util/ArrayList$SubList" => &["java/util/List"],
        "java/util/Collections$UnmodifiableList" => &["java/util/List"],
        "java/util/List" => &["java/util/Collection"],
        "java/util/Collection" => &["java/lang/Iterable"],
        "java/util/HashSet" => &["java/util/Set"],
        "java/util/Set" => &["java/util/Collection"],
        "java/util/HashMap" => &["java/util/Map"],
        "java/util/HashMap$KeySet" => &["java/util/Set"],
        "java/util/HashMap$Values" => &["java/util/Collection"],
        "java/util/ArrayList$Itr"
        | "java/util/HashMap$KeyIterator"
        | "java/util/HashMap$ValueIterator" => &["java/util/Iterator"],
        // M3 Wave 3: functional-interface specialization hierarchy.
        "java/util/function/UnaryOperator" => &["java/util/function/Function"],
        "java/util/function/BinaryOperator" => &["java/util/function/BiFunction"],
        // Real-world corpus wave 1: java.io interfaces.
        "java/io/Writer" => &[
            "java/io/Appendable",
            "java/io/Closeable",
            "java/io/Flushable",
        ],
        "java/io/Reader" => &["java/io/Readable", "java/io/Closeable"],
        "java/io/Serializable" => &[],
        _ => &[],
    }
}

/// Whether a builtin class is an interface — `Class.isInterface` and
/// `Class.getSuperclass` need the distinction (an interface's superclass is
/// `null`, even though the subtype walk reaches `Object` through it).
/// Compiled classes carry the flag in their generated `UserClassInfo`
/// instead (M3_PLAN §8.1).
pub fn builtin_is_interface(class: &str) -> bool {
    matches!(
        class,
        "java/lang/Runnable"
            | "java/lang/CharSequence"
            | "java/lang/Iterable"
            | "java/util/Collection"
            | "java/util/List"
            | "java/util/Set"
            | "java/util/Map"
            | "java/util/Iterator"
            | "java/util/function/Function"
            | "java/util/function/BiFunction"
            | "java/util/function/Supplier"
            | "java/util/function/Consumer"
            | "java/util/function/BiConsumer"
            | "java/util/function/Predicate"
            | "java/util/function/BiPredicate"
            | "java/util/function/UnaryOperator"
            | "java/util/function/BinaryOperator"
            | "java/util/Comparator"
            | "java/io/Appendable"
            | "java/io/Closeable"
            | "java/io/Flushable"
            | "java/io/Readable"
            | "java/io/Serializable"
    )
}

/// Number of field slots a class owns itself (not counting inherited ones).
/// `Throwable`: slot 0 `detailMessage`, slot 1 `cause`, slot 2 the
/// suppressed list (audit A2); the boxed classes and `StringBuilder` keep
/// their payload in slot 0; `PrintStream` keeps its sink id in slot 0.
fn builtin_own_slots(class: &str) -> Option<usize> {
    let n = match class {
        "java/lang/Object" | "java/lang/System" | "java/lang/Math" | "java/lang/Number" => 0,
        "java/lang/Throwable" => 3,
        // M4 Wave 2 (T06): runnable, name, daemon, started, tid.
        "java/lang/Thread" => 5,
        "java/lang/String"
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
        | "java/io/PrintStream"
        // M3 Wave 5: reflective objects. `Class` slot 0 = internal class
        // name payload; Field: owner/name/desc/is_static/instance_slot/
        // static_slot; Method: owner/name/desc/is_static.
        | "java/lang/Class" => 1,
        "java/lang/reflect/Field" => 6,
        "java/lang/reflect/Method" => 4,
        // M3 Wave 2 layouts (documented in collections.rs):
        // ArrayList: size, modCount, elements ref
        "java/util/ArrayList" => 3,
        // ArrayList$Itr: list ref, cursor, expectedModCount, lastRet
        "java/util/ArrayList$Itr" => 4,
        // ArrayList$SubList (jdkutil.rs): parent ref, offset, size, expectedMod
        "java/util/ArrayList$SubList" => 4,
        // Collections$UnmodifiableList (jdkutil.rs): backing list ref
        "java/util/Collections$UnmodifiableList" => 1,
        // HashMap: size, modCount, threshold, table ref
        "java/util/HashMap" => 4,
        // HashMap$Node: hash, key, value, next
        "java/util/HashMap$Node" => 4,
        // hash iterators: map ref, table idx, pending node, expectedModCount
        "java/util/HashMap$KeyIterator" | "java/util/HashMap$ValueIterator" => 4,
        // views: map ref
        "java/util/HashMap$KeySet" | "java/util/HashMap$Values" => 1,
        // HashSet: backing map ref
        "java/util/HashSet" => 1,
        "java/util/Objects" => 0,
        // Real-world corpus wave 1: java.io layouts.
        // Writer: slot 0 = the `lock` object ref (WritingBuffer passes it via
        // super(lock)); StringWriter/Reader/StringReader inherit it.
        "java/io/Writer" | "java/io/Reader" => 1,
        // StringWriter: own slot = the accumulated buffer ref; StringReader:
        // own slot = payload (string + cursor) held by the runtime.
        "java/io/StringWriter" | "java/io/StringReader" => 1,
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
        let _rt = crate::test_support::runtime();
        assert_eq!(builtin_total_slots("java/lang/Object"), Some(0));
        // Audit A2 layout: detailMessage + cause + suppressed list.
        assert_eq!(builtin_total_slots("java/lang/Throwable"), Some(3));
        assert_eq!(builtin_total_slots("java/lang/Exception"), Some(3));
        assert_eq!(
            builtin_total_slots("java/lang/ArrayIndexOutOfBoundsException"),
            Some(3)
        );
        assert_eq!(builtin_total_slots("java/lang/StringBuilder"), Some(1));
        assert_eq!(builtin_total_slots("java/lang/Integer"), Some(1));
        assert_eq!(builtin_total_slots("java/lang/Unknown"), None);
    }

    #[test]
    fn corpus_wave1_io_hierarchy() {
        let _rt = crate::test_support::runtime();
        // Real-world corpus wave 1 (minimal-json): java.io family must be
        // known to the loader (hierarchy, interfaces, slot layout).
        assert_eq!(
            builtin_parent_of("java/io/Writer"),
            Some("java/lang/Object")
        );
        assert_eq!(
            builtin_parent_of("java/io/StringWriter"),
            Some("java/io/Writer")
        );
        assert_eq!(
            builtin_parent_of("java/io/StringReader"),
            Some("java/io/Reader")
        );
        // Writer + Reader own the lock slot; StringWriter/StringReader own
        // one payload slot on top of the inherited lock.
        assert_eq!(builtin_total_slots("java/io/Writer"), Some(1));
        assert_eq!(builtin_total_slots("java/io/StringWriter"), Some(2));
        assert_eq!(builtin_total_slots("java/io/StringReader"), Some(2));
        assert!(builtin_is_interface("java/io/Appendable"));
        assert!(builtin_is_interface("java/io/Closeable"));
        assert!(builtin_is_interface("java/io/Flushable"));
        assert!(builtin_is_interface("java/io/Readable"));
        assert_eq!(
            builtin_interfaces_of("java/io/Writer"),
            &[
                "java/io/Appendable",
                "java/io/Closeable",
                "java/io/Flushable"
            ]
        );
        assert_eq!(
            builtin_interfaces_of("java/io/Reader"),
            &["java/io/Readable", "java/io/Closeable"]
        );
    }
}
