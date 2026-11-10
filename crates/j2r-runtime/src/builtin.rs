//! Builtin JDK class knowledge: hierarchy (for subtype checks and dispatch
//! walks) and field-slot layout (so user classes extending builtin classes
//! get consistent flattened slot indices).

/// Direct superclass of a builtin class, or `None` when the class is unknown
/// to the runtime or is `java/lang/Object` (the root).
pub fn builtin_parent_of(class: &str) -> Option<&'static str> {
    let p = match class {
        "java/lang/String" => "java/lang/Object",
        // Corpus gap F4/F5 support (2026-11-05): java.lang.Comparable is a
        // known interface — `new Comparable[n]` must validate (ANewArray
        // walks builtin_parent_of) and the boxed numbers / String implement
        // it, so aastore into Comparable[] and arraycopy element checks
        // resolve. Registered with parent Object like the other pure
        // interfaces; compareTo itself stays loud-unsupported until a
        // corpus path calls it through the interface.
        "java/lang/Comparable" => "java/lang/Object",
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
        // Formatter exception family (FmtProbe wave): the three exact
        // classes the probe catches; parents flattened to IAE (the JDK's
        // UnknownFormatException layer adds nothing observable here).
        "java/util/UnknownFormatConversionException" => "java/lang/IllegalArgumentException",
        "java/util/IllegalFormatConversionException" => "java/lang/IllegalArgumentException",
        "java/util/MissingFormatArgumentException" => "java/lang/IllegalArgumentException",
        // FmtResidualProbe wave: the two remaining IllegalFormatException
        // subclasses the JDK Formatter throws at PARSE time (flag/width
        // legality), plus catch-type resolution for the generated catch
        // blocks. Parents flattened to IAE like the three above.
        "java/util/MissingFormatWidthException" => "java/lang/IllegalArgumentException",
        "java/util/FormatFlagsConversionMismatchException" => "java/lang/IllegalArgumentException",
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
        // Corpus wave 2 (JZlib): javac emits `throw new InternalError()`
        // as the unreachable-default arm of exhaustive string/enum
        // switches (GZIPHeader.getName) — instantiable like any builtin
        // Throwable.
        "java/lang/InternalError" => "java/lang/VirtualMachineError",
        "java/lang/AssertionError" => "java/lang/Error",
        // T12 (M5_JNI_NATIVES.md §3.6/§6): the linkage-error family the
        // native-binding surface throws. UnsatisfiedLinkError's
        // uncaught-header parity (the 12c e2e) needs the parent chain
        // for the builtin toString dispatch; NoSuchMethodError rides
        // the same family (the T11 pending shape).
        "java/lang/UnsatisfiedLinkError" => "java/lang/LinkageError",
        "java/lang/NoSuchMethodError" => "java/lang/IncompatibleClassChangeError",
        "java/lang/IncompatibleClassChangeError" => "java/lang/LinkageError",
        "java/lang/LinkageError" => "java/lang/Error",
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
        // stringsim corpus wave (2026-11-07): entrySet views + the
        // unmodifiableMap wrapper + Sift4's index-oriented LinkedList.
        "java/util/HashMap$EntrySet" => "java/lang/Object",
        // stringsim corpus wave: the narrow regex surface (Pattern \s+ /
        // Matcher.replaceAll), slots owned by j2r-runtime/src/regex.rs.
        "java/util/regex/Pattern" => "java/lang/Object",
        "java/util/regex/Matcher" => "java/lang/Object",
        "java/util/HashMap$EntryIterator" => "java/lang/Object",
        "java/util/Collections$UnmodifiableMap" => "java/lang/Object",
        "java/util/LinkedList" => "java/lang/Object",
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
        // Real-world corpus wave 3 (JAMA): the write-side text surface plus
        // the registered-loud classes. PrintWriter is REAL (io subset in
        // natives.rs); DecimalFormat/DecimalFormatSymbols/StreamTokenizer/
        // Vector resolve and instantiate but every behavior native fails
        // loud (natives.rs wave-3 marker rows) — machinery does not exist
        // and says so by name, exactly like the serialization surface.
        "java/io/PrintWriter" => "java/io/Writer",
        "java/text/Format" => "java/lang/Object",
        "java/text/NumberFormat" => "java/text/Format",
        "java/text/DecimalFormat" => "java/text/NumberFormat",
        "java/text/DecimalFormatSymbols" => "java/lang/Object",
        "java/io/StreamTokenizer" => "java/lang/Object",
        "java/util/Vector" => "java/lang/Object",
        "java/util/Locale" => "java/lang/Object",
        // Real-world corpus wave 2 (JZlib): the byte-stream hierarchy.
        // JZlib's stream classes (ZOutputStream, ZInputStream,
        // DeflaterOutputStream, GZIP*, InflaterInputStream) extend the
        // Filter* pair directly, so the whole chain must be known to the
        // loader. The Filter* pair owns the protected field the user code
        // touches by name (see builtin_field_slot).
        "java/io/OutputStream" => "java/lang/Object",
        "java/io/FilterOutputStream" => "java/io/OutputStream",
        "java/io/FilterInputStream" => "java/io/InputStream",
        // Corpus wave 2 (JZlib Inflate.readString/readBytes): the byte
        // buffer with JDK field layout — slot 0 = buf (byte[] ref), slot 1
        // = count (Int). Instantiable through the <init> natives.
        "java/io/ByteArrayOutputStream" => "java/io/OutputStream",
        "java/io/ByteArrayInputStream" => "java/io/InputStream",
        // BufferedInputStream (stream wave 3): inherits FilterInputStream's
        // inner-stream slot, then buf (byte[] ref), pos, count, markpos,
        // marklimit. close() nulls in/buf — the null slots ARE the closed
        // state ("Stream closed" checks read them).
        "java/io/BufferedInputStream" => "java/io/FilterInputStream",
        // PushbackInputStream (stream wave 4): inherits the inner slot, then
        // the pushback buffer (byte[] ref) + the read cursor pos.
        "java/io/PushbackInputStream" => "java/io/FilterInputStream",
        // Real-world corpus wave 2 (JZlib): javac compiles every `enum`
        // to `extends java/lang/Enum` (WrapperType). Two own slots: name
        // (Str) then ordinal (Int) — see the Enum natives rows.
        "java/lang/Enum" => "java/lang/Object",
        "java/lang/ClassNotFoundException" => "java/lang/ReflectiveOperationException",
        // Corpus wave 2 (JZlib): Deflate.clone/GZIPHeader.clone declare
        // `throws CloneNotSupportedException` — catch handlers must resolve
        // the builtin chain exactly like IOException.
        "java/lang/CloneNotSupportedException" => "java/lang/Exception",
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
            &["java/lang/CharSequence", "java/lang/Comparable"]
        }
        // JDK: every boxed numeric + Boolean/Character implements Comparable;
        // Enum implements Comparable<E> (javac's hand-rolled WrapperType
        // relies on it in JZlib).
        "java/lang/Integer"
        | "java/lang/Long"
        | "java/lang/Float"
        | "java/lang/Double"
        | "java/lang/Byte"
        | "java/lang/Short"
        | "java/lang/Character"
        | "java/lang/Boolean"
        | "java/lang/Enum" => &["java/lang/Comparable"],
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
        // JDK: HashMap.Node implements Map.Entry — the enhanced-for's
        // JDK: HashMap.Node implements Map.Entry — the enhanced-for's
        // checkcast Map$Entry on entrySet iteration depends on this.
        "java/util/HashMap$Node" => &["java/util/Map$Entry"],
        "java/util/HashMap$Values" => &["java/util/Collection"],
        // stringsim corpus wave: the entry view is a Set; the wrapper is a
        // Map; Sift4's list is a List (interfaces the compiled code sees).
        "java/util/HashMap$EntrySet" => &["java/util/Set"],
        "java/util/HashMap$EntryIterator" => &["java/util/Iterator"],
        "java/util/Collections$UnmodifiableMap" => &["java/util/Map"],
        "java/util/LinkedList" => &["java/util/List"],
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
        // Corpus wave 2 (JZlib): byte-stream interfaces. OutputStream is
        // Closeable + Flushable, InputStream is Closeable; the Filter*
        // pair inherits the same through the abstract bases.
        "java/io/OutputStream" | "java/io/FilterOutputStream" => {
            &["java/io/Closeable", "java/io/Flushable"]
        }
        "java/io/InputStream" | "java/io/FilterInputStream" => &["java/io/Closeable"],
        // Corpus wave 3 (JAMA): PrintWriter implements the Writer trio
        // directly in the JDK; Vector is a List (List→Collection→Iterable
        // already chained above).
        "java/io/PrintWriter" => &[
            "java/io/Appendable",
            "java/io/Closeable",
            "java/io/Flushable",
        ],
        "java/util/Vector" => &["java/util/List"],
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
        // M4 Wave 2 (T06): runnable, name, daemon, started, tid;
        // T14 (Wave 4) added slot 5: pendingInterrupt.
        "java/lang/Thread" => 6,
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
        // stringsim corpus wave: EntryIterator shares the KeyIterator layout;
        // EntrySet/UnmodifiableMap are slot-0-delegate views; LinkedList
        // copies the ArrayList storage model (size, modCount, Object[]).
        "java/util/HashMap$EntryIterator" => 4,
        // stringsim corpus wave: Pattern (the pattern text), Matcher
        // (pattern text + input text).
        "java/util/regex/Pattern" => 1,
        "java/util/regex/Matcher" => 2,
        "java/util/HashMap$EntrySet" => 1,
        "java/util/Collections$UnmodifiableMap" => 1,
        "java/util/LinkedList" => 3,
        // views: map ref
        "java/util/HashMap$KeySet" | "java/util/HashMap$Values" => 1,
        // HashSet: backing map ref
        "java/util/HashSet" => 1,
        "java/util/Objects" => 0,
        // Real-world corpus wave 1: java.io layouts.
        // Writer: slot 0 = the `lock` object ref (WritingBuffer passes it via
        // super(lock)); StringWriter/Reader/StringReader inherit it.
        "java/io/Writer" | "java/io/Reader" => 1,
        // StringWriter: own slot = the accumulated buffer ref (1 own slot
        // after Writer's inherited lock). StringReader (runtime-io wave
        // 2026-11-07, ByteArrayInputStream cursor model): text, pos,
        // mark, closed — 4 own slots; 5 flattened with the lock.
        "java/io/StringWriter" => 1,
        "java/io/StringReader" => 4,
        // Real-world corpus wave 2 (JZlib): byte-stream layouts. The abstract
        // bases own nothing; the Filter* pair owns exactly one slot each —
        // the protected field user subclasses touch by name (see
        // builtin_field_slot).
        "java/io/OutputStream" | "java/io/InputStream" => 0,
        "java/io/FilterOutputStream" | "java/io/FilterInputStream" => 1,
        // ByteArrayOutputStream: slot 0 = buf (byte[] ref), slot 1 = count.
        "java/io/ByteArrayOutputStream" => 2,
        // ByteArrayInputStream: slot 0 = buf (byte[] ref), slot 1 = pos,
        // slot 2 = mark, slot 3 = count (skip/charset wave).
        "java/io/ByteArrayInputStream" => 4,
        // BufferedInputStream (stream wave 3): own slots after the inherited
        // FilterInputStream slot — buf (byte[] ref), pos, count, markpos,
        // marklimit (total 6 slots flattened).
        "java/io/BufferedInputStream" => 5,
        // PushbackInputStream (stream wave 4): own slots after the inherited
        // FilterInputStream slot — buf (byte[] ref), pos (total 3 flattened).
        "java/io/PushbackInputStream" => 2,
        // PrintWriter (corpus wave 3): absolute slots after the inherited
        // Writer lock — 1 = sink ref, 2 = autoFlush (Int). Behavior subset in
        // natives.rs (native stdout/stderr + StringWriter sinks).
        "java/io/PrintWriter" => 2,
        // Registered-loud wave-3 classes: minimal layouts (the constructor
        // natives fail loudly before any slot could matter). StreamTokenizer
        // owns ttype (I) / sval (Str) / nval (D) — the getfield sites in
        // JAMA's uncalled Matrix.read resolve through builtin_field_slot.
        "java/io/StreamTokenizer" => 3,
        "java/util/Vector" => 0,
        "java/text/NumberFormat" => 0,
        "java/text/DecimalFormat" => 0,
        "java/text/DecimalFormatSymbols" => 0,
        "java/util/Locale" => 0,
        // Enum: slot 0 = name (Str), slot 1 = ordinal (Int). The generated
        // subclass (javac's WrapperType) starts at base 2.
        "java/lang/Enum" => 2,
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

/// Named instance fields a builtin class owns — corpus wave 2 (JZlib):
/// user stream subclasses touch the protected Filter* fields (`out` /
/// `in`) directly by name, so the codegen must lay them out like user
/// fields. The returned index is WITHIN the class's own-slot range; the
/// codegen adds the class's inherited base (total slots of everything
/// strictly below it in the receiver's chain).
pub fn builtin_field_slot(class: &str, field: &str) -> Option<usize> {
    match (class, field) {
        ("java/io/FilterOutputStream", "out") => Some(0),
        ("java/io/FilterInputStream", "in") => Some(0),
        // Corpus wave 3 (JAMA): StreamTokenizer's three public fields read
        // directly by the uncalled Matrix.read loop.
        ("java/io/StreamTokenizer", "ttype") => Some(0),
        ("java/io/StreamTokenizer", "sval") => Some(1),
        ("java/io/StreamTokenizer", "nval") => Some(2),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linkage_error_family_parent_chain() {
        // T12 (M5_JNI_NATIVES.md §3.6): the native-binding surface's
        // error family must resolve through the builtin chain for the
        // builtin toString dispatch (the uncaught-header parity) and
        // for catch-type matching.
        assert_eq!(
            builtin_parent_of("java/lang/UnsatisfiedLinkError"),
            Some("java/lang/LinkageError")
        );
        assert_eq!(
            builtin_parent_of("java/lang/LinkageError"),
            Some("java/lang/Error")
        );
        assert_eq!(
            builtin_parent_of("java/lang/NoSuchMethodError"),
            Some("java/lang/IncompatibleClassChangeError")
        );
        assert_eq!(
            builtin_parent_of("java/lang/IncompatibleClassChangeError"),
            Some("java/lang/LinkageError")
        );
        // ...and the chain reaches Throwable for the toString dispatch.
        assert_eq!(
            builtin_parent_of("java/lang/Error"),
            Some("java/lang/Throwable")
        );
        assert_eq!(
            builtin_total_slots("java/lang/UnsatisfiedLinkError"),
            Some(3)
        );
    }

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
        // Writer + Reader own the lock slot; StringWriter owns one payload
        // slot on top of it. StringReader (runtime-io wave 2026-11-07)
        // owns text/pos/mark/closed — 4 own + lock = 5 flattened.
        assert_eq!(builtin_total_slots("java/io/Writer"), Some(1));
        assert_eq!(builtin_total_slots("java/io/StringWriter"), Some(2));
        assert_eq!(builtin_total_slots("java/io/StringReader"), Some(5));
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
