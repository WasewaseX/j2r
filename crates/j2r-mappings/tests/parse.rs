//! The T14 law tests: every rule the Tiny v2 parser enforces, pinned
//! against a real-shaped synthetic fixture (the row shapes mirror the
//! real Fabric intermediary 1.21.4 capture: header, class rows, field
//! rows with descriptors, method overloads sharing a name, parameter
//! rows, inner classes, empty columns) plus the malformed shapes that
//! must stay loud.

use j2r_mappings::parse;

const FIXTURE: &str = include_str!("fixtures/synthetic_intermediary.tiny");

/// The header law: namespaces come from the header, in file order.
#[test]
fn header_namespaces_carry_the_file_order() {
    let set = parse(FIXTURE).expect("fixture parses");
    assert_eq!(set.namespaces, vec!["official", "intermediary"]);
}

/// The class-row law: both directions of the same row resolve to the
/// same class.
#[test]
fn class_rows_map_both_directions() {
    let set = parse(FIXTURE).expect("fixture parses");
    assert_eq!(set.map_class(0, 1, "a"), Some("net/minecraft/class_7833"));
    assert_eq!(set.map_class(1, 0, "net/minecraft/class_7833"), Some("a"));
    assert_eq!(set.map_class(0, 1, "b"), Some("net/minecraft/class_5973"));
    assert!(
        set.map_class(1, 0, "a").is_none(),
        "intermediary name is not an official name"
    );
}

/// The field-row law: the descriptor is part of the identity and is
/// preserved verbatim.
#[test]
fn field_rows_disambiguate_by_descriptor() {
    let set = parse(FIXTURE).expect("fixture parses");
    assert_eq!(set.map_field(0, 1, "a", "La;", "a"), Some("field_40713"));
    assert_eq!(set.map_field(0, 1, "a", "La;", "b"), Some("field_40714"));
    assert!(
        set.map_field(0, 1, "a", "Ljava/lang/String;", "a")
            .is_none(),
        "wrong descriptor misses"
    );
    assert!(
        set.map_field(0, 1, "b", "La;", "a").is_none(),
        "wrong owner misses"
    );
}

/// The method-row law: overloads share a name, the descriptor
/// disambiguates.
#[test]
fn method_overloads_disambiguate_by_descriptor() {
    let set = parse(FIXTURE).expect("fixture parses");
    let a_f = "(F)Lorg/joml/Quaternionf;";
    let vf_f = "(Lorg/joml/Vector3f;F)Lorg/joml/Quaternionf;";
    assert_eq!(set.map_method(0, 1, "a", a_f, "a"), Some("method_46349"));
    assert_eq!(set.map_method(0, 1, "a", vf_f, "a"), Some("method_46350"));
    assert_eq!(set.map_method(0, 1, "a", a_f, "b"), Some("method_46351"));
    assert!(set.map_method(0, 1, "a", "(I)V", "a").is_none());
}

/// The parameter-row law: params nest under the preceding method row and
/// carry the slot index.
#[test]
fn param_rows_nest_under_their_method() {
    let set = parse(FIXTURE).expect("fixture parses");
    let params = set.method_params(0, 1, "b", "(Lb;I)V", "a");
    assert_eq!(params, vec!["p_i_50528_1_", "p_i_50528_2_"]);
    // A method with no param rows returns an empty list, never a panic.
    assert!(set
        .method_params(0, 1, "a", "(F)Lorg/joml/Quaternionf;", "a")
        .is_empty());
}

/// The inner-class law: an indent-1 `c` row nests under the enclosing
/// class and is indexable by its own name.
#[test]
fn inner_classes_nest_and_index() {
    let set = parse(FIXTURE).expect("fixture parses");
    assert_eq!(
        set.map_class(0, 1, "$inner$a"),
        Some("net/minecraft/client/class_310$class_1")
    );
    let outer = set
        .class(0, "net/minecraft/client/Minecraft")
        .expect("outer present");
    assert_eq!(outer.inner_classes.len(), 1);
    // The inner class's method resolves through the nested entry.
    assert_eq!(
        set.map_method(0, 1, "$inner$a", "()V", "a"),
        Some("method_1505")
    );
}

/// The empty-column law: an unnamed namespace stays `None` (present and
/// searchable as absent in THAT namespace, present in the other) — never
/// dropped.
#[test]
fn empty_columns_are_none_not_dropped() {
    let set = parse(FIXTURE).expect("fixture parses");
    let only = set
        .class(0, "only")
        .expect("the one-named class is present");
    assert_eq!(only.names[0].as_deref(), Some("only"));
    assert_eq!(only.names[1], None);
    assert!(set.map_class(0, 1, "only").is_none());
    // The reverse row: named only in the intermediary namespace.
    assert!(set.class(0, "unmapped_in_official_class_8888").is_none());
    assert!(set.class(1, "unmapped_in_official_class_8888").is_some());
    assert!(set
        .map_class(1, 0, "unmapped_in_official_class_8888")
        .is_none());
}

/// CRLF and blank lines are tolerated; content is unchanged.
#[test]
fn crlf_and_blank_lines_are_tolerated() {
    let crlf = FIXTURE.replace('\n', "\r\n");
    let a = parse(FIXTURE).expect("fixture parses");
    let b = parse(&crlf).expect("crlf parses");
    assert_eq!(a.classes.len(), b.classes.len());
    let blanked = "tiny\t2\t0\tX\tY\n\nc\tx\ty\n\n\n".to_string();
    let set = parse(&blanked).expect("blank lines skip");
    assert_eq!(set.classes.len(), 1);
}

/// Malformed: wrong header magic, wrong major version, too few
/// namespaces — all loud, all naming the line.
#[test]
fn malformed_headers_are_loud() {
    let cases: Vec<(&str, &str)> = vec![
        ("tinyx\t2\t0\tA\tB\n", "tiny"),
        ("tiny\t1\t0\tA\tB\n", "v2"),
        ("tiny\t2\t0\tA\n", "two"),
        ("", "empty"),
    ];
    for (text, expect) in cases {
        let err = parse(text).expect_err(expect);
        assert_eq!(err.line, 1, "{expect}: header errors name line 1");
    }
}

/// Malformed rows: unknown kind char, wrong column count, missing
/// descriptor, empty descriptor, depth jump without context — all loud
/// with the right line number.
#[test]
fn malformed_rows_are_loud_with_line_numbers() {
    // line 3 (after header + one class): unknown kind
    let err = parse("tiny\t2\t0\tA\tB\nc\tcls\tclazz\n\tx\tf1\tf2\n").expect_err("unknown kind");
    assert_eq!(err.line, 3);
    // wrong column count on a class row
    let err = parse("tiny\t2\t0\tA\tB\nc\tcls\n").expect_err("column count");
    assert_eq!(err.line, 2);
    // missing descriptor on a member row
    let err = parse("tiny\t2\t0\tA\tB\nc\tcls\tclazz\n\tf\n").expect_err("no descriptor");
    assert_eq!(err.line, 3);
    // empty descriptor on a member row
    let err = parse("tiny\t2\t0\tA\tB\nc\tcls\tclazz\n\tf\t\tf1\tf2\n").expect_err("empty desc");
    assert_eq!(err.line, 3);
    // depth-2 row with no enclosing member (param under a class)
    let err = parse("tiny\t2\t0\tA\tB\nc\tcls\tclazz\n\t\tp\t0\tp1\tp2\n").expect_err("depth jump");
    assert_eq!(err.line, 3);
    // param index not a u32
    let err = parse("tiny\t2\t0\tA\tB\nc\tcls\tclazz\n\tm\t(I)V\tm1\tm2\n\t\tp\tx\tp1\tp2\n")
        .expect_err("bad index");
    assert_eq!(err.line, 4);
    // method-kind row under a method
    let err = parse("tiny\t2\t0\tA\tB\nc\tcls\tclazz\n\tm\t(I)V\tm1\tm2\n\t\tm\t()V\tm3\tm4\n")
        .expect_err("method under method");
    assert_eq!(err.line, 4);
}

/// The error display carries both the line and the message.
#[test]
fn parse_error_display_names_the_line() {
    let err = parse("nope\n").expect_err("bad magic");
    let msg = err.to_string();
    assert!(msg.contains("line 1"), "{msg}");
    assert!(msg.contains("tiny"), "{msg}");
}

/// The real intermediary capture's shape law: the header names the
/// namespaces, class rows dominate, and a two-namespace file round-trips
/// the fixture's counts exactly.
#[test]
fn fixture_row_census_matches_the_file() {
    let set = parse(FIXTURE).expect("fixture parses");
    // top-level classes: a, b, Minecraft, only, (unmapped lands under ns1)
    assert_eq!(set.classes.len(), 5);
    let a = set.class(0, "a").expect("class a");
    assert_eq!(a.fields.len(), 2);
    assert_eq!(a.methods.len(), 3);
    assert_eq!(a.inner_classes.len(), 0);
    let mc = set
        .class(0, "net/minecraft/client/Minecraft")
        .expect("Minecraft");
    assert_eq!(mc.inner_classes.len(), 1);
}

/// Namespace index bounds: a lookup with a bad index misses (never
/// panics) — the remapper passes column indexes it computed itself.
#[test]
fn out_of_bounds_namespace_indexes_miss_not_panic() {
    let set = parse(FIXTURE).expect("fixture parses");
    assert!(set.map_class(0, 7, "a").is_none());
    assert!(set.map_class(9, 0, "a").is_none());
    assert!(set.class(9, "a").is_none());
}
