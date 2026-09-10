//! The four questions every framework rule asks of a Java declaration.
//!
//! *What is it annotated with? What modifiers does it carry? What is its name? What text does this
//! node cover?* Spring asks them about a method, Jackson about a field, the scheduling reader about
//! both — and each of them had written its own copy, which is three chances for "what counts as a
//! modifier" to be decided differently.
//!
//! The one that actually differs between copies, and the reason this is worth sharing: **modifiers
//! are read as tokens, not as text.** `@Cacheable(value = "final")` contains the word `final`, and a
//! `contains("final")` on the declaration's source says the method cannot be proxied. Reading the
//! modifier *nodes* and skipping the annotation ones is the only way that stays right, and it is
//! not the implementation anybody writes first.

use tree_sitter::Node;

/// The source text a node covers.
pub fn node_text<'a>(node: &Node<'_>, source: &'a str) -> &'a str {
    source.get(node.start_byte()..node.end_byte()).unwrap_or_default()
}

/// The first named child of `kind`.
pub fn named_child_of<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let found = node.named_children(&mut cursor).find(|c| c.kind() == kind);
    found
}

/// The annotations written on a declaration, as `(name as written, the annotation node)`.
///
/// The name is kept **as written** — `@jakarta.validation.constraints.NotNull` qualifies, and a
/// caller that wants the simple name takes the last segment with [`simple_name`]. Keeping the
/// qualified form here means a rule can tell `@lombok.Getter` from somebody's own `@Getter`.
pub fn annotations_of<'t>(decl: Node<'t>, source: &str) -> Vec<(String, Node<'t>)> {
    let Some(modifiers) = named_child_of(decl, "modifiers") else { return Vec::new() };
    let mut out = Vec::new();
    let mut cursor = modifiers.walk();
    for child in modifiers.named_children(&mut cursor) {
        if !matches!(child.kind(), "annotation" | "marker_annotation") {
            continue;
        }
        if let Some(name) = child.child_by_field_name("name") {
            out.push((node_text(&name, source).to_string(), child));
        }
    }
    out
}

/// The annotation of that simple name, if the declaration carries it.
pub fn annotation_named<'t>(
    decl: Node<'t>,
    source: &str,
    name: &str,
) -> Option<Node<'t>> {
    annotations_of(decl, source)
        .into_iter()
        .find(|(written, _)| simple_name(written) == name)
        .map(|(_, node)| node)
}

/// The keyword modifiers of a declaration (`public`, `final`, `static`, `transient`).
///
/// Read from the modifier **tokens**, never from the declaration's text — see the module docs for
/// the string that makes the text version wrong.
pub fn modifier_words(decl: Node<'_>, source: &str) -> Vec<String> {
    let Some(modifiers) = named_child_of(decl, "modifiers") else { return Vec::new() };
    let mut out = Vec::new();
    let mut cursor = modifiers.walk();
    for child in modifiers.children(&mut cursor) {
        if matches!(child.kind(), "annotation" | "marker_annotation") {
            continue;
        }
        out.push(node_text(&child, source).to_string());
    }
    out
}

/// Whether a declaration carries a keyword modifier.
pub fn has_modifier(decl: Node<'_>, source: &str, word: &str) -> bool {
    modifier_words(decl, source).iter().any(|w| w == word)
}

/// The last segment of a possibly-qualified name.
pub fn simple_name(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

/// The text of a plain string literal, without its quotes.
///
/// `None` for anything that is not one — a text block, a concatenation, a constant reference —
/// because nothing certain can be said about a value this cannot read, and every rule built on
/// these asks about a value it was *given*.
pub fn string_literal(node: Node<'_>, source: &str) -> Option<String> {
    if node.kind() != "string_literal" {
        return None;
    }
    let raw = node_text(&node, source);
    if raw.starts_with("\"\"\"") || !raw.starts_with('"') || raw.len() < 2 || !raw.ends_with('"') {
        return None;
    }
    Some(raw[1..raw.len() - 1].to_string())
}

/// The value of an annotation's named element, when it is a plain string literal.
pub fn annotation_string(annotation: Node<'_>, source: &str, element: &str) -> Option<String> {
    let args = annotation.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        if child.kind() != "element_value_pair" {
            continue;
        }
        let key = child.child_by_field_name("key")?;
        if node_text(&key, source) != element {
            continue;
        }
        return string_literal(child.child_by_field_name("value")?, source);
    }
    None
}

/// The **whole** text of an annotation's named element, literal or not — for a caller that only
/// needs to recognise a constant it knows (`Propagation.NEVER`, `JsonCreator.Mode.DELEGATING`).
pub fn annotation_value_text<'a>(
    annotation: Node<'_>,
    source: &'a str,
    element: &str,
) -> Option<&'a str> {
    let args = annotation.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        if child.kind() != "element_value_pair" {
            continue;
        }
        let key = child.child_by_field_name("key")?;
        if node_text(&key, source) != element {
            continue;
        }
        return Some(node_text(&child.child_by_field_name("value")?, source));
    }
    None
}

/// Every class-like declaration in a subtree, nested ones included.
pub fn type_declarations<'t>(root: Node<'t>) -> Vec<Node<'t>> {
    fn walk<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if matches!(
                child.kind(),
                "class_declaration" | "record_declaration" | "interface_declaration"
                    | "enum_declaration"
            ) {
                out.push(child);
            }
            walk(child, out);
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::parse_java;

    const SRC: &str = r#"class A {
    @Cacheable(value = "final")
    public final void go() { }

    @Deprecated
    private static int n;
}"#;

    fn method() -> (tree_sitter::Tree, &'static str) {
        (parse_java(SRC).unwrap(), SRC)
    }

    #[test]
    fn annotations_keep_the_name_as_written() {
        let (tree, src) = method();
        let class = type_declarations(tree.root_node())[0];
        let body = class.child_by_field_name("body").unwrap();
        let mut cursor = body.walk();
        let m = body.named_children(&mut cursor).find(|c| c.kind() == "method_declaration").unwrap();
        let names: Vec<String> = annotations_of(m, src).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["Cacheable"]);
        assert!(annotation_named(m, src, "Cacheable").is_some());
    }

    /// The reason this is shared: an annotation argument holding the word `final` must not read as
    /// a modifier, and a `contains` on the declaration's text says it does.
    #[test]
    fn a_modifier_word_inside_an_annotation_argument_is_not_a_modifier() {
        let (tree, src) = method();
        let class = type_declarations(tree.root_node())[0];
        let body = class.child_by_field_name("body").unwrap();
        let mut cursor = body.walk();
        let m = body.named_children(&mut cursor).find(|c| c.kind() == "method_declaration").unwrap();
        let words = modifier_words(m, src);
        assert!(words.contains(&"public".to_string()));
        assert!(words.contains(&"final".to_string()), "the real one is still there");
        assert_eq!(words.iter().filter(|w| *w == "final").count(), 1, "and only once");
        assert!(has_modifier(m, src, "public"));
        assert!(!has_modifier(m, src, "static"));
    }

    #[test]
    fn an_annotations_string_element_is_read_and_a_non_literal_is_not() {
        let src = r#"class A { @Scheduled(cron = "0 0 2 * * ?") void a() { } @Scheduled(cron = C) void b() { } }"#;
        let tree = parse_java(src).unwrap();
        let class = type_declarations(tree.root_node())[0];
        let body = class.child_by_field_name("body").unwrap();
        let mut cursor = body.walk();
        let methods: Vec<_> =
            body.named_children(&mut cursor).filter(|c| c.kind() == "method_declaration").collect();
        let first = annotation_named(methods[0], src, "Scheduled").unwrap();
        assert_eq!(annotation_string(first, src, "cron").as_deref(), Some("0 0 2 * * ?"));
        let second = annotation_named(methods[1], src, "Scheduled").unwrap();
        assert_eq!(annotation_string(second, src, "cron"), None);
        assert_eq!(annotation_value_text(second, src, "cron"), Some("C"));
    }

    #[test]
    fn nested_types_are_found_too() {
        let src = "class Outer { class Inner { } enum E { } }";
        let tree = parse_java(src).unwrap();
        assert_eq!(type_declarations(tree.root_node()).len(), 3);
    }

    #[test]
    fn a_text_block_is_not_a_plain_literal() {
        let src = "class A { String s = \"\"\"\nhello\n\"\"\"; }";
        let tree = parse_java(src).unwrap();
        // Whatever the grammar calls it, it must not come back as a readable literal.
        let mut found = None;
        fn walk<'t>(n: tree_sitter::Node<'t>, out: &mut Option<tree_sitter::Node<'t>>) {
            let mut c = n.walk();
            for ch in n.named_children(&mut c) {
                if ch.kind() == "string_literal" {
                    *out = Some(ch);
                }
                walk(ch, out);
            }
        }
        walk(tree.root_node(), &mut found);
        if let Some(node) = found {
            assert_eq!(string_literal(node, src), None);
        }
    }
}
