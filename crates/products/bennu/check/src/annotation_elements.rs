//! A value given for an annotation element that does not exist — `@Column(nulable = true)`.
//!
//! javac reports it as `cannot find symbol` on the element name; the point here is that it is a
//! *typo in configuration*, and configuration is where a typo survives longest. Nothing reads
//! `nulable`, so the column stays `NOT NULL` and the failure arrives from the database, far from the
//! line that caused it. On a legacy Spring/JPA codebase this is the annotation mistake that costs
//! the most time.
//!
//! Conservative in the two places it could be wrong:
//!   * the annotation type must **resolve**, and its members must be fully known — an unresolved
//!     annotation says nothing about which elements it declares;
//!   * an element inherited from a meta-annotation is not a thing in Java (annotation types cannot
//!     extend), so the declared methods ARE the whole vocabulary — but a `value` written without a
//!     name is still checked against a declared `value()`.

use std::collections::HashSet;

use bennu_java::prelude::{FileSymbols, MemberKind, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check_id::CheckId;
use crate::resolve::type_binary;

/// Every unknown annotation element, and every annotation repeated where Java forbids it.
pub fn annotation_element_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "annotation" => {
                check_annotation(n, bytes, symbols, resolver, &mut out);
                check_values(n, bytes, &mut out);
            }
            "modifiers" => check_repeats(n, bytes, symbols, resolver, &mut out),
            _ => {}
        }
    }
    out
}

fn check_annotation(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    let Some(name_node) = n.child_by_field_name("name") else { return };
    let Ok(written) = name_node.utf8_text(bytes) else { return };
    let Some(binary) = type_binary(written, symbols, resolver) else { return };
    let Some(members) = resolver.members_of(&binary) else { return };
    // Only judge something we know IS an annotation type: a name that resolved to an ordinary class
    // would have a completely different member set, and every element would look wrong.
    if !members.flags.is_annotation {
        return;
    }
    let declared: Vec<&str> = members
        .methods
        .iter()
        .filter(|m| m.kind == MemberKind::Method)
        .map(|m| m.name.as_str())
        .collect();
    // An annotation type with no elements at all is more likely one we decoded poorly than one
    // someone is passing arguments to.
    if declared.is_empty() {
        return;
    }

    let Some(args) = n.child_by_field_name("arguments") else { return };
    let mut cw = args.walk();
    for pair in args.named_children(&mut cw) {
        if pair.kind() != "element_value_pair" {
            continue;
        }
        let Some(key_node) = pair.child_by_field_name("key") else { continue };
        let Ok(key) = key_node.utf8_text(bytes) else { continue };
        if !declared.contains(&key) {
            out.push(CheckId::UnknownAnnotationElement.at(
                key_node,
                format!("`{}` declares no element `{key}`", simple(&binary)),
            ));
            continue;
        }
        // The element exists — is the value a shape its declared type can hold?
        let Some(element) = members
            .methods
            .iter()
            .find(|m| m.kind == MemberKind::Method && m.name == key)
        else {
            continue;
        };
        if let Some(value) = pair.child_by_field_name("value") {
            check_value_type(value, &element.return_type.binary_name, key, bytes, out);
        }
    }
}

fn simple(binary: &str) -> &str {
    binary.rsplit(['/', '$']).next().unwrap_or(binary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, TypeRef, Visibility};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MapResolver {
        members: HashMap<String, ClassMembers>,
        simple: HashMap<String, String>,
        /// The annotations on each annotation TYPE — what tells `@Repeatable` from not.
        own: HashMap<String, Vec<bennu_java::prelude::Annotation>>,
    }

    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.members.get(binary).cloned().map(Arc::new)
        }
        fn class_annotations(&self, binary: &str) -> Vec<bennu_java::prelude::Annotation> {
            self.own.get(binary).cloned().unwrap_or_default()
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn element_of(name: &str, ty: &str) -> Member {
        Member { return_type: TypeRef::simple(ty), ..element(name) }
    }

    fn element(name: &str) -> Member {
        Member {
            name: name.to_string(),
            kind: MemberKind::Method,
            return_type: TypeRef::simple("java/lang/String"),
            params: Vec::new(),
            is_static: false,
            is_abstract: true,
            is_default: false,
            is_final: false,
            visibility: Visibility::Public,
            raw_signature: String::new(),
            throws: Vec::new(),
            annotations: Vec::new(),
        }
    }

    fn ann(methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: None,
            interfaces: Vec::new(),
            methods,
            fields: Vec::new(),
            flags: ClassFlags { is_annotation: true, is_interface: true, ..ClassFlags::default() },
        }
    }

    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert(
            "javax/persistence/Column".into(),
            ann(vec![
                element("name"),
                element_of("nullable", "boolean"),
                element_of("length", "int"),
                element_of("tags", "java/lang/String[]"),
                // `Class<?>[]` — the array marker survives the generic argument list now (it used
                // to be parsed away, leaving a name indistinguishable from a plain `Class`).
                element_of("kind", "java/lang/Class[]"),
                // And a genuinely single-valued `Class<?>`, to pin the other side of it.
                element_of("one_kind", "java/lang/Class"),
            ]),
        );
        members.insert("com/acme/Marker".into(), ann(Vec::new()));
        members.insert("com/acme/Tag".into(), ann(vec![element("v")]));
        members.insert("com/acme/Quiet".into(), ann(vec![element("v")]));
        // A plain class that happens to be written where an annotation goes — never judged.
        members.insert(
            "com/acme/NotAnAnnotation".into(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some("java/lang/Object".into()),
                interfaces: Vec::new(),
                methods: vec![element("whatever")],
                fields: Vec::new(),
                flags: ClassFlags::default(),
            },
        );
        let simple = [
            ("Column", "javax/persistence/Column"),
            ("Marker", "com/acme/Marker"),
            ("Tag", "com/acme/Tag"),
            ("Quiet", "com/acme/Quiet"),
            ("NotAnAnnotation", "com/acme/NotAnAnnotation"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let ann = |n: &str| bennu_java::prelude::Annotation {
            name: n.to_string(),
            qualified: format!("java.lang.annotation.{n}"),
            start: 0,
            end: 0,
            strings: Vec::new(),
            args: Vec::new(),
            positional: Vec::new(),
        };
        // `Column` is a plain annotation; `Tag` is `@Repeatable`; `Quiet` is one whose own
        // annotations we could not read — the project-declared case.
        let own = [
            ("javax/persistence/Column".to_string(), vec![ann("Retention"), ann("Target")]),
            ("com/acme/Tag".to_string(), vec![ann("Retention"), ann("Repeatable")]),
        ]
        .into_iter()
        .collect();
        MapResolver { members, simple, own }
    }

    fn codes(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).expect("parse");
        let root = tree.root_node();
        let nodes = crate::check::collect_nodes(root);
        let symbols = bennu_java::prelude::extract_symbols_from_root(&root, src);
        annotation_element_errors_in(&nodes, src, &symbols, &resolver())
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    /// The typo this check exists for: nothing reads `nulable`, so the column stays NOT NULL.
    #[test]
    fn a_misspelled_element_is_flagged() {
        let src = r#"class A { @Column(name = "c", nulable = true) String f; }"#;
        assert_eq!(codes(src), ["unknown-annotation-element"]);
    }

    #[test]
    fn declared_elements_are_fine() {
        let src = r#"class A { @Column(name = "c", nullable = true, length = 20) String f; }"#;
        assert!(codes(src).is_empty());
    }

    #[test]
    fn an_unresolvable_annotation_is_left_alone() {
        let src = r#"class A { @Whatever(nulable = true) String f; }"#;
        assert!(codes(src).is_empty());
    }

    /// A name that resolves to an ordinary class has a completely different member set; judging it
    /// would report every element as unknown.
    #[test]
    fn a_type_that_is_not_an_annotation_is_left_alone() {
        let src = r#"class A { @NotAnAnnotation(nulable = true) String f; }"#;
        assert!(codes(src).is_empty());
    }

    /// An annotation we decoded with no elements at all is more likely a decoding gap than a real
    /// marker someone is passing arguments to.
    #[test]
    fn an_annotation_with_no_known_elements_is_left_alone() {
        let src = r#"class A { @Marker(anything = 1) String f; }"#;
        assert!(codes(src).is_empty());
    }

    #[test]
    fn the_same_annotation_twice_is_flagged_when_it_is_not_repeatable() {
        let src = r#"class A { @Column(name = "a") @Column(name = "b") String f; }"#;
        assert_eq!(codes(src), ["not-repeatable-annotation"]);
    }

    #[test]
    fn a_repeatable_annotation_may_appear_twice() {
        let src = r#"class A { @Tag(v = "a") @Tag(v = "b") String f; }"#;
        assert!(codes(src).is_empty());
    }

    #[test]
    fn one_occurrence_is_always_fine() {
        let src = r#"class A { @Column(name = "a") String f; }"#;
        assert!(codes(src).is_empty());
    }

    /// The gate that matters: empty means "none OR not read", and a project-declared annotation type
    /// is currently the second. Concluding "not repeatable" from that absence would report every
    /// `@MyMarker` a project writes twice in its own code.
    #[test]
    fn an_annotation_whose_own_annotations_are_unknown_is_never_judged() {
        let src = r#"class A { @Quiet(v = "a") @Quiet(v = "b") String f; }"#;
        assert!(codes(src).is_empty());
    }

    #[test]
    fn two_different_annotations_are_fine() {
        let src = r#"class A { @Column(name = "a") @Tag(v = "b") String f; }"#;
        assert!(codes(src).is_empty());
    }

    /// The one people write: configuration that looks like configuration and is a compile error.
    #[test]
    fn a_method_call_as_an_annotation_value_is_flagged() {
        let src = r#"class A { @Column(name = helper.get()) String f; }"#;
        assert_eq!(codes(src), ["non-constant-annotation-value"]);
    }

    #[test]
    fn a_new_or_a_lambda_as_a_value_is_flagged() {
        assert_eq!(codes(r#"class A { @Column(name = new String()) String f; }"#), ["non-constant-annotation-value"]);
        assert_eq!(codes(r#"class A { @Column(name = () -> 1) String f; }"#), ["non-constant-annotation-value"]);
    }

    /// Inside an array initialiser too — on an element that really is an array, so the only thing
    /// wrong with it is the call.
    #[test]
    fn a_call_inside_an_array_value_is_flagged() {
        let src = r#"class A { @Column(tags = {"a", helper.get()}) String f; }"#;
        assert_eq!(codes(src), ["non-constant-annotation-value"]);
    }

    #[test]
    fn literals_class_literals_and_constants_are_all_fine() {
        assert!(codes(r#"class A { @Column(name = "a") String f; }"#).is_empty());
        assert!(codes(r#"class A { @Column(name = Helper.CONST) String f; }"#).is_empty());
        assert!(codes(r#"class A { @Column(name = String.class) String f; }"#).is_empty());
        assert!(codes(r#"class A { @Column(name = 1 + 2) String f; }"#).is_empty());
    }

    /// A nested annotation is a legal value, and its own arguments are checked on its own node —
    /// descending here would report them twice.
    #[test]
    fn a_nested_annotation_value_is_not_reported_twice() {
        let src = r#"class A { @Column(name = @Tag(v = helper.get())) String f; }"#;
        assert_eq!(codes(src), ["non-constant-annotation-value"]);
    }

    /// javac's `annotation.value.not.allowable.type`: a list where the element holds one value.
    #[test]
    fn a_list_given_to_a_single_valued_element_is_flagged() {
        assert_eq!(codes(r#"class A { @Column(length = {1, 2}) String f; }"#), ["annotation-value-type"]);
    }

    /// A list given to a `Class<?>[]` element is what that element is FOR. The array marker used to
    /// be parsed away with the generic argument list, so the element read as a plain `Class` and
    /// every one of commons-lang's eleven was reported.
    #[test]
    fn a_list_given_to_a_generic_array_element_is_fine() {
        assert!(codes(r#"class A { @Column(kind = {String.class, Integer.class}) String f; }"#).is_empty());
    }

    /// The other side of the same fix: a `Class<?>` element really does hold one value, and now
    /// that array-ness is trustworthy in both directions, a list given to it is reported — which is
    /// javac's `annotation.value.not.allowable.type`, and used to be suppressed along with the rest.
    #[test]
    fn a_list_given_to_a_single_valued_generic_element_is_flagged() {
        assert_eq!(
            codes(r#"class A { @Column(one_kind = {String.class, Integer.class}) String f; }"#),
            ["annotation-value-type"]
        );
    }

    /// The reverse is Java's single-element shorthand and is legal.
    #[test]
    fn one_value_given_to_an_array_element_is_the_shorthand() {
        assert!(codes(r#"class A { @Column(tags = "one") String f; }"#).is_empty());
        assert!(codes(r#"class A { @Column(tags = {"a", "b"}) String f; }"#).is_empty());
    }

    #[test]
    fn a_literal_of_the_wrong_kind_is_flagged() {
        assert_eq!(codes(r#"class A { @Column(length = "no") String f; }"#), ["annotation-value-type"]);
        assert_eq!(codes(r#"class A { @Column(name = 1) String f; }"#), ["annotation-value-type"]);
        assert_eq!(codes(r#"class A { @Column(nullable = 3) String f; }"#), ["annotation-value-type"]);
    }

    #[test]
    fn a_literal_of_the_right_kind_is_fine() {
        assert!(codes(r#"class A { @Column(length = 20) String f; }"#).is_empty());
        assert!(codes(r#"class A { @Column(name = "a") String f; }"#).is_empty());
        assert!(codes(r#"class A { @Column(nullable = true) String f; }"#).is_empty());
    }

    /// `char` widens to every integral type — `@Ann(i = 'x')` compiles, and calling it a mismatch
    /// would be exactly the false positive this check must not produce.
    #[test]
    fn a_char_given_to_a_numeric_element_is_legal() {
        assert!(codes(r#"class A { @Column(length = 'x') String f; }"#).is_empty());
    }

    /// A bare name may be a `static final` of any type — not judged.
    #[test]
    fn a_constant_reference_is_never_judged() {
        assert!(codes(r#"class A { @Column(length = Helper.MAX) String f; }"#).is_empty());
    }

    #[test]
    fn a_marker_annotation_with_no_arguments_is_left_alone() {
        assert!(codes("class A { @Column String f; }").is_empty());
    }
}


// ── the same annotation written twice ────────────────────────────────────────

/// The same annotation twice on one declaration, when its type is not `@Repeatable`.
///
/// Java added repeating annotations in 8 and made them opt-in: the annotation type must itself carry
/// `@Repeatable(Container.class)`. Without it the second one is a compile error rather than a second
/// value — worth saying at the site, because on a declaration long enough for anyone to write the
/// second by accident the two are rarely adjacent.
///
/// Keyed by the RESOLVED binary name, not by what was written, so `@Column` and
/// `@javax.persistence.Column` on one declaration are seen as the pair they are.
///
/// Silent unless the type resolves, is known to be an annotation, AND its own annotations came back
/// non-empty. That last gate is the one that matters: [`TypeResolver::class_annotations`] returns
/// empty both for "no annotations" and for "could not read them", and a project-declared annotation
/// type is currently the second — so without it every `@MyMarker` written twice in a project's own
/// code would be reported on the strength of an absence that means nothing.
fn check_repeats(
    modifiers: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    let mut seen: HashSet<String> = HashSet::new();
    let mut cw = modifiers.walk();
    for a in modifiers.named_children(&mut cw) {
        if !matches!(a.kind(), "annotation" | "marker_annotation") {
            continue;
        }
        let Some(name_node) = a.child_by_field_name("name") else { continue };
        let Ok(written) = name_node.utf8_text(bytes) else { continue };
        let Some(binary) = type_binary(written, symbols, resolver) else { continue };
        let Some(members) = resolver.members_of(&binary) else { continue };
        if !members.flags.is_annotation {
            continue;
        }
        let own = resolver.class_annotations(&binary);
        if own.is_empty() {
            // Cannot tell whether it is `@Repeatable`. Record it so a THIRD occurrence is not
            // reported either, and move on.
            seen.insert(binary);
            continue;
        }
        let repeatable = own.iter().any(|x| x.name == "Repeatable");
        if !seen.insert(binary.clone()) && !repeatable {
            out.push(CheckId::NotRepeatableAnnotation.at(
                name_node,
                format!("`{}` is not `@Repeatable`, so it may appear only once here", simple(&binary)),
            ));
        }
    }
}


// ── a value that is not a constant ───────────────────────────────────────────

/// An annotation element given something that is not a constant expression —
/// `compiler.err.expression.not.allowable.as.annotation.value`.
///
/// An annotation is recorded in the class file at compile time, so its values must be knowable then:
/// a literal, a `static final` constant, a class literal, an enum constant, a nested annotation, or
/// an array of those. `@Value(config.get("k"))` reads like configuration and is a compile error.
///
/// Only shapes that can NEVER be constant are flagged, and they are recognised by kind rather than
/// by evaluating anything: a call, a `new`, a lambda, a method reference, an assignment, a ternary,
/// an increment. A bare name is left alone — it may well be a `static final`, and deciding that
/// needs the resolver. So this is the half that is provable from the tree, and the type-mismatch
/// half (`annotation.value.not.allowable.type`: a `String` where the element declares `int`) is not
/// here — it needs the element's declared type compared with the value's, which is the type checker.
fn check_values(annotation: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(args) = annotation.child_by_field_name("arguments") else { return };
    let mut cw = args.walk();
    for arg in args.named_children(&mut cw) {
        match arg.kind() {
            "element_value_pair" => {
                if let Some(v) = arg.child_by_field_name("value") {
                    scan_value(v, bytes, out);
                }
            }
            // `@Foo("x")` — the single-element form, whose value is the argument itself.
            _ => scan_value(arg, bytes, out),
        }
    }
}

/// Flag `value`, and the elements of an array initialiser written for it.
fn scan_value(value: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    if value.kind() == "element_value_array_initializer" {
        let mut c = value.walk();
        for el in value.named_children(&mut c) {
            scan_value(el, bytes, out);
        }
        return;
    }
    // A nested annotation is a legal value and carries its own arguments, which the outer walk
    // reaches on its own `annotation` node — descending here would report them twice.
    if matches!(value.kind(), "annotation" | "marker_annotation") {
        return;
    }
    let what = match value.kind() {
        "method_invocation" => "a method call",
        "object_creation_expression" => "a `new`",
        "array_creation_expression" => "a `new` array",
        "lambda_expression" => "a lambda",
        "method_reference" => "a method reference",
        "assignment_expression" => "an assignment",
        "ternary_expression" => "a conditional",
        "update_expression" => "an increment",
        _ => return,
    };
    let _ = bytes;
    out.push(CheckId::NonConstantAnnotationValue.at(
        value,
        format!("an annotation value must be a constant, and {what} is not one"),
    ));
}


// ── a value the declared type cannot hold ────────────────────────────────────

/// An annotation element given a value whose type its declaration cannot accept.
///
/// Two shapes, both decided from the value's SYNTAX against the element's declared type — no
/// inference, so nothing here needs the type checker:
///
///   * **an array where the element is not one** — `@Ann(i = {1, 2})` with `int i()`. This is
///     javac's `annotation.value.not.allowable.type` proper. The reverse is legal and is NOT
///     flagged: `@Column(name = "a")` for a `String[]` element is Java's single-element shorthand.
///   * **a literal of the wrong kind** — a string where a number is declared, a number where a
///     `String` is, a boolean where either is. javac reports these as plain incompatible types.
///
/// Only LITERALS are judged. A bare name may be a `static final` constant of any type, and deciding
/// that needs the resolver plus constant folding — so it is left alone, along with everything else.
/// This is the half of the question the tree can answer; the other half is the type checker's.
fn check_value_type(
    value: Node,
    declared: &str,
    key: &str,
    bytes: &[u8],
    out: &mut Vec<Diagnostic>,
) {
    // Array-ness is read off the declared binary name, and it is trustworthy in both directions:
    // a library element's type comes from a bytecode descriptor, a project element's from
    // `resolve_written_type`, and both spell an array `elem[]`.
    //
    // They did not always agree. The written-type parse used to stop at the closing `>` of a
    // generic argument list, so `Class<?>[]` was recorded as `Class` with the array marker silently
    // gone — commons-lang has eleven such elements, and every one was reported for the list it is
    // supposed to hold. The dimensions are peeled BEFORE the arguments now (see
    // `bennu_java::typename::split_array_dims`), so this needs no gate.
    let is_array = declared.contains('[');
    if value.kind() == "element_value_array_initializer" {
        if !is_array {
            out.push(CheckId::AnnotationValueType.at(
                value,
                format!("`{key}` is declared `{}`, which holds one value, not a list", pretty(declared)),
            ));
        }
        return;
    }
    if is_array {
        // The single-element shorthand — `@Ann(arr = "one")` for a `String[]`. Legal, and the
        // element type would have to be compared against the value, which is the same question one
        // level down; not worth a second, weaker copy of it here.
        return;
    }
    let Some(got) = literal_kind(value) else { return };
    let want = declared_kind(declared);
    let Some(want) = want else { return };
    if got != want {
        let _ = bytes;
        out.push(CheckId::AnnotationValueType.at(
            value,
            format!("`{key}` is declared `{}`, and this is {got}", pretty(declared)),
        ));
    }
}

/// What a literal IS, in the only three families an annotation element can declare.
///
/// `char` is deliberately read as a number, because it widens to every integral type — `@Ann(i =
/// 'x')` compiles, and calling it a mismatch would be exactly the false positive this check must
/// not produce.
fn literal_kind(value: Node) -> Option<&'static str> {
    Some(match value.kind() {
        "string_literal" | "text_block" => "a string",
        "decimal_integer_literal"
        | "hex_integer_literal"
        | "octal_integer_literal"
        | "binary_integer_literal"
        | "decimal_floating_point_literal"
        | "hex_floating_point_literal"
        | "character_literal" => "a number",
        "true" | "false" => "a boolean",
        _ => return None,
    })
}

/// What family a declared element type belongs to. `None` for anything else — an enum, an
/// annotation, a `Class`, a type we could not read — where a literal says nothing conclusive.
fn declared_kind(binary: &str) -> Option<&'static str> {
    Some(match binary {
        "java/lang/String" => "a string",
        "int" | "long" | "short" | "byte" | "char" | "float" | "double" => "a number",
        "boolean" => "a boolean",
        _ => return None,
    })
}

/// A declared type as a Java reader would write it.
fn pretty(binary: &str) -> String {
    let base = binary.trim_end_matches("[]");
    let name = base.rsplit(['/', '$']).next().unwrap_or(base);
    if binary.contains('[') {
        format!("{name}[]")
    } else {
        name.to_string()
    }
}
