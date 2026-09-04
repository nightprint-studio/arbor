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
            // A marker (`@Column`) carries no values, so it has nothing to check about the ones it
            // gives — but it is exactly the shape that gives NONE, and an element with no `default`
            // still has to be supplied. It reaches the same reading, which finds every required
            // element missing.
            "marker_annotation" => check_annotation(n, bytes, symbols, resolver, &mut out),
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
    let elements: Vec<&bennu_java::prelude::Member> = members
        .methods
        .iter()
        .filter(|m| m.kind == MemberKind::Method)
        .collect();
    // An annotation type with no elements at all is more likely one we decoded poorly than one
    // someone is passing arguments to.
    if elements.is_empty() {
        return;
    }
    let declares = |name: &str| elements.iter().any(|m| m.name == name);

    // What the use site actually gave a value for — the other half of the vocabulary question.
    let mut supplied: HashSet<&str> = HashSet::new();

    if let Some(args) = n.child_by_field_name("arguments") {
        let mut cw = args.walk();
        for pair in args.named_children(&mut cw) {
            if pair.kind() != "element_value_pair" {
                // `@Foo("x")` — the single-element shorthand, which IS `value = "x"`. Legal only
                // when the type declares `value()`; javac otherwise reports it as a `value()` it
                // cannot find, which is the same sentence as any other unknown element.
                supplied.insert("value");
                if !declares("value") {
                    out.push(CheckId::UnknownAnnotationElement.at(
                        pair,
                        format!(
                            "`{}` declares no element `value`, so this value needs a name",
                            simple(&binary)
                        ),
                    ));
                }
                continue;
            }
            let Some(key_node) = pair.child_by_field_name("key") else { continue };
            let Ok(key) = key_node.utf8_text(bytes) else { continue };
            supplied.insert(key);
            if !declares(key) {
                out.push(CheckId::UnknownAnnotationElement.at(
                    key_node,
                    format!("`{}` declares no element `{key}`", simple(&binary)),
                ));
                continue;
            }
            // The element exists — is the value a shape its declared type can hold?
            let Some(element) = elements.iter().find(|m| m.name == key) else { continue };
            if let Some(value) = pair.child_by_field_name("value") {
                let declared = &element.return_type.binary_name;
                check_value_type(value, declared, key, bytes, out);
                check_constant_names(value, declared, bytes, symbols, resolver, out);
            }
        }
    }

    // An element with no `default` MUST be given a value. `is_default` carries the `default` clause
    // for a project element (the source `annotation_type_element_declaration`) and the
    // `AnnotationDefault` attribute for a library one, so both sides answer the same question — and
    // a resolver that could not answer it at all would have failed one of the gates above.
    let missing: Vec<&str> = elements
        .iter()
        .filter(|m| !m.is_default && !supplied.contains(m.name.as_str()))
        .map(|m| m.name.as_str())
        .collect();
    if !missing.is_empty() {
        out.push(CheckId::MissingAnnotationElement.at(
            name_node,
            format!(
                "`{}` requires a value for {}",
                simple(&binary),
                list(&missing)
            ),
        ));
    }
}

/// `["a"]` → ``` `a` ```; `["a", "b"]` → ``` `a` and `b` ```; more → a comma list ending in "and".
fn list(names: &[&str]) -> String {
    match names {
        [one] => format!("`{one}`"),
        [head @ .., last] => format!(
            "{} and `{last}`",
            head.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")
        ),
        [] => String::new(),
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

    /// An element with NO `default` — one a use site has to supply.
    fn required(name: &str) -> Member {
        Member { is_default: false, ..element(name) }
    }

    /// A required element of a given type.
    fn required_of(name: &str, ty: &str) -> Member {
        Member { return_type: TypeRef::simple(ty), ..required(name) }
    }

    /// An element with a `default` clause, which is what nearly every configuration annotation
    /// declares — `@Column` has a default for all eight of its own. `is_default` carries that.
    fn element(name: &str) -> Member {
        Member {
            name: name.to_string(),
            kind: MemberKind::Method,
            return_type: TypeRef::simple("java/lang/String"),
            params: Vec::new(),
            is_static: false,
            is_abstract: true,
            is_default: true,
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
        // An annotation with a REQUIRED element (no `default`) beside an optional one — the shape
        // every `@Column`-style configuration annotation has at least one of.
        members.insert(
            "com/acme/Named".into(),
            ann(vec![required("id"), element_of("size", "int")]),
        );
        // A single-element annotation, so the `@Only("x")` shorthand is legal on it.
        members.insert("com/acme/Only".into(), ann(vec![required("value")]));
        // Two required elements, to pin how several missing ones are worded.
        members.insert("com/acme/Pair".into(), ann(vec![required("left"), required("right")]));
        // An element whose declared type is an ENUM — a name written for it is an enum constant,
        // which is not a constant expression and must never be judged as one.
        members.insert("com/acme/Level".into(), ann(vec![required_of("at", "com/acme/Where")]));
        members.insert(
            "com/acme/Where".into(),
            ClassMembers {
                flags: ClassFlags { is_enum: true, ..ClassFlags::default() },
                ..ann(Vec::new())
            },
        );
        // The class the constant-name tests resolve against: one mutable field, one `static final`
        // `String` (a constant variable), and one `final` of a class type (`final`, and still not
        // a constant — the clause `final` alone does not decide).
        members.insert(
            "com/acme/Holder".into(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some(TypeRef::simple("java/lang/Object")),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: vec![
                    Member::field("MUTABLE", TypeRef::simple("java/lang/String")).stat(),
                    Member::field("CONST", TypeRef::simple("java/lang/String")).stat().final_(),
                    Member::field("OBJS", TypeRef::simple("com/acme/MyObj[]")).stat().final_(),
                ],
                flags: ClassFlags::default(),
            },
        );
        // A plain class that happens to be written where an annotation goes — never judged.
        members.insert(
            "com/acme/NotAnAnnotation".into(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some(TypeRef::simple("java/lang/Object")),
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
            ("Named", "com/acme/Named"),
            ("Only", "com/acme/Only"),
            ("Pair", "com/acme/Pair"),
            ("Level", "com/acme/Level"),
            ("Where", "com/acme/Where"),
            ("Holder", "com/acme/Holder"),
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

    fn diags(src: &str) -> Vec<Diagnostic> {
        let tree = bennu_java::prelude::parse_java(src).expect("parse");
        let root = tree.root_node();
        let nodes = crate::check::collect_nodes(root);
        let symbols = bennu_java::prelude::extract_symbols_from_root(&root, src);
        annotation_element_errors_in(&nodes, src, &symbols, &resolver())
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

    // ── an element that has to be supplied ───────────────────────────────────

    /// The other half of the vocabulary question: an element with no `default` is not optional, and
    /// leaving it out is a compile error javac reports at the `@`.
    #[test]
    fn a_required_element_left_out_is_flagged() {
        assert_eq!(codes(r#"class A { @Named(size = 2) String f; }"#), ["missing-annotation-element"]);
    }

    /// A marker is the shape that supplies NOTHING, so every required element is missing — and it
    /// reaches the check through its own node kind, which `annotation` does not cover.
    #[test]
    fn a_marker_of_an_annotation_with_a_required_element_is_flagged() {
        assert_eq!(codes(r#"class A { @Named String f; }"#), ["missing-annotation-element"]);
    }

    /// The everyday shape, and the one a wrong `is_default` would flood: an annotation whose
    /// elements all have defaults, used with none of them.
    #[test]
    fn a_marker_of_an_all_default_annotation_is_fine() {
        assert!(codes(r#"class A { @Column String f; }"#).is_empty());
        assert!(codes(r#"class A { @Column(name = "c") String f; }"#).is_empty());
    }

    #[test]
    fn a_required_element_that_is_supplied_is_fine() {
        assert!(codes(r#"class A { @Named(id = "x") String f; }"#).is_empty());
    }

    /// Several missing elements are ONE diagnostic listing them — javac emits one per element, and
    /// three errors on one `@` reads as three problems when it is one.
    #[test]
    fn two_missing_elements_are_listed_together() {
        let ds = diags(r#"class A { @Pair String f; }"#);
        assert_eq!(ds.len(), 1);
        assert!(ds[0].message.contains("`left` and `right`"), "{}", ds[0].message);
    }

    // ── the single-value shorthand ───────────────────────────────────────────

    /// `@Only("x")` IS `value = "x"`, and legal exactly when the type declares `value()`.
    #[test]
    fn the_shorthand_is_fine_on_a_type_that_declares_value() {
        assert!(codes(r#"class A { @Only("x") String f; }"#).is_empty());
    }

    /// javac reports this as a `value()` it cannot find — the same sentence as any other unknown
    /// element, plus the required element the shorthand did not supply.
    #[test]
    fn the_shorthand_on_a_type_without_value_is_flagged() {
        let mut got = codes(r#"class A { @Named("x") String f; }"#);
        got.sort();
        assert_eq!(got, ["missing-annotation-element", "unknown-annotation-element"]);
    }

    // ── a name that is not a constant ────────────────────────────────────────

    /// The report this exists for: a variable passed where a constant is required. `MUTABLE` is not
    /// `final`, so it is never a constant, whatever it holds.
    #[test]
    fn a_non_final_field_as_a_value_is_flagged() {
        assert_eq!(
            codes(r#"class A { @Named(id = Holder.MUTABLE) String f; }"#),
            ["non-constant-annotation-value"]
        );
    }

    /// `final` and of a `String` type — a constant variable, and the spelling everyone writes.
    #[test]
    fn a_static_final_string_field_as_a_value_is_fine() {
        assert!(codes(r#"class A { @Named(id = Holder.CONST) String f; }"#).is_empty());
    }

    /// `final` is necessary and not sufficient (JLS §4.12.4): a constant variable is also of a
    /// primitive or `String` type, so a `final` array of a class type is not one.
    #[test]
    fn a_final_field_of_a_class_type_as_a_value_is_flagged() {
        let ds = diags(r#"class A { @Named(id = Holder.OBJS) String f; }"#);
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].code, "non-constant-annotation-value");
        assert!(ds[0].message.contains("MyObj[]"), "{}", ds[0].message);
    }

    /// An ENUM-typed element takes an enum constant, which is a `static final` field of a class
    /// type — exactly the shape the rule above rejects. Judging it by that rule would report the
    /// only correct spelling, so an element of any other family is never judged.
    #[test]
    fn a_name_written_for_an_enum_element_is_never_judged() {
        assert!(codes(r#"class A { @Level(at = Holder.OBJS) String f; }"#).is_empty());
    }

    /// A name inside a method body may be a local shadowing the field, and then the field's own
    /// modifiers say nothing about what the name means.
    #[test]
    fn a_name_inside_a_body_is_left_alone() {
        let src = r#"class A { void m() { @Named(id = MUTABLE) int i = 0; } }"#;
        assert!(!codes(src).iter().any(|c| c == "non-constant-annotation-value"));
    }

    /// Reading out of an array is never constant, even when the array itself is `static final`.
    #[test]
    fn an_array_access_as_a_value_is_flagged() {
        assert!(codes(r#"class A { @Named(id = Holder.CONST[0]) String f; }"#)
            .contains(&"non-constant-annotation-value".to_string()));
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
        // Reading an element out of an array is never constant, even when the array itself is a
        // `static final` — javac calls this one `expression.not.allowable.as.annotation.value`.
        "array_access" => "an array access",
        "lambda_expression" => "a lambda",
        "method_reference" => "a method reference",
        "assignment_expression" => "an assignment",
        "ternary_expression" => "a conditional",
        "update_expression" => "an increment",
        // A bare name is NOT judged here. Whether it may be one depends on the element's declared
        // type — an enum element takes an enum constant, a `Class` element a class literal — so it
        // is decided in `check_annotation`, which is the only place that holds the declaration.
        _ => return,
    };
    out.push(CheckId::NonConstantAnnotationValue.at(
        value,
        format!("an annotation value must be a constant, and {what} is not one"),
    ));
}

/// A name written for an element whose declared type demands a **constant expression**, checked
/// against what that name actually is.
///
/// Only elements of a primitive or `String` type get here. The other legal element types take
/// something a constant expression never is — an enum element takes an enum constant, a `Class`
/// element a class literal, an annotation element an annotation — so a name written for one of
/// those is not this check's business, and judging it by these rules would report the correct
/// spelling.
///
/// Descends an array initialiser, because `{A, B}` given to a `String[]` element asks the question
/// once per entry.
fn check_constant_names(
    value: Node,
    declared: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    let (base, _) = bennu_java::prelude::split_array_dims(declared);
    if !(crate::nodes::is_primitive(base) || base == "java/lang/String") {
        return;
    }
    if value.kind() == "element_value_array_initializer" {
        let mut c = value.walk();
        for el in value.named_children(&mut c) {
            check_constant_names(el, declared, bytes, symbols, resolver, out);
        }
        return;
    }
    if let Some(why) = not_a_constant_variable(value, bytes, symbols, resolver) {
        out.push(CheckId::NonConstantAnnotationValue.at(
            value,
            format!("an annotation value must be a constant, and {why}"),
        ));
    }
}

/// Why the name `value` reads is **provably** not a constant variable — or `None` when it may be
/// one, or is not a name we can resolve at all.
///
/// Java's rule (JLS §4.12.4): a constant variable is `final`, of a primitive or `String` type, and
/// initialised with a constant expression. Two of the three clauses are decided here, and both are
/// decided from the index rather than guessed:
///
///   * **not `final`** — never a constant, whatever it holds;
///   * **`final`, but not of a primitive or `String` type** — `static final MyObj[] OBJ = …` is as
///     `final` as anything and still not a constant variable, so `final` alone proves nothing.
///
/// The third clause is deliberately not attempted. `static final String N = f();` and
/// `static final int LEN = "abc".length();` are both rejected by javac, but telling them from
/// `static final String N = "n"` needs the initializer and constant folding — and guessing there
/// would flag the legal spelling, which is the overwhelmingly common one.
///
/// Two things narrow it further, both to avoid saying something wrong:
///   * a bare name written anywhere inside a `block`, a lambda or a parameter list is skipped — a
///     local or a parameter can shadow the field, and then the field's own modifiers say nothing
///     about what the name means;
///   * a name that resolves to nothing (a static import, a field of an ENCLOSING class rather than
///     a supertype, an unindexed type) yields `None` and no diagnostic.
fn not_a_constant_variable(
    value: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<String> {
    let (owner, name) = match value.kind() {
        "identifier" => {
            if shadowable_position(value) {
                return None;
            }
            let name = value.utf8_text(bytes).ok()?;
            let crate::type_scope::TypeScope::Inside(owner) =
                crate::resolve::enclosing_scope(value, bytes, symbols)
            else {
                return None;
            };
            (owner, name)
        }
        // `Other.K` — a qualified read, so no local can shadow it. The receiver has to name a TYPE:
        // an instance receiver could not be constant in the first place, and whatever produced it
        // would already have been reported by the shape scan.
        "field_access" => {
            let object = value.child_by_field_name("object")?;
            let field = value.child_by_field_name("field")?;
            let owner = crate::resolve::type_binary_at(
                object.utf8_text(bytes).ok()?,
                value,
                bytes,
                symbols,
                resolver,
            )?;
            (owner, field.utf8_text(bytes).ok()?)
        }
        _ => return None,
    };
    let field = find_field(resolver, &owner, name)?;
    if !field.is_final {
        return Some(format!("`{name}` is not `final`"));
    }
    let (base, _) = bennu_java::prelude::split_array_dims(&field.return_type.binary_name);
    if crate::nodes::is_primitive(base) || base == "java/lang/String" {
        return None; // final and of the right family — the initializer is the part we do not judge
    }
    Some(format!(
        "`{name}` is declared `{}`, and only a `final` primitive or `String` is one",
        pretty(&field.return_type.binary_name)
    ))
}

/// Whether `node` sits somewhere a local or a parameter could shadow a field of the same name.
fn shadowable_position(node: Node) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if matches!(n.kind(), "block" | "formal_parameters" | "lambda_expression") {
            return true;
        }
        cur = n.parent();
    }
    false
}

/// The field named `name` on `owner` or any KNOWN supertype — `None` when nothing declares it (the
/// hierarchy may simply be incomplete, which is why the caller treats `None` as "say nothing").
fn find_field(
    resolver: &dyn TypeResolver,
    owner: &str,
    name: &str,
) -> Option<bennu_java::prelude::Member> {
    let mut found = None;
    crate::walk::for_each_supertype(resolver, owner, &mut |_, cm| {
        if found.is_none() {
            found = cm
                .fields
                .iter()
                .find(|f| f.name == name && f.kind == MemberKind::Field)
                .cloned();
        }
    });
    found
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
