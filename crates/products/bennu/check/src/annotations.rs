//! Annotation-target legality — a well-known annotation used where its `@Target` forbids.
//!
//! The general rule needs the annotation's `@Target` meta-annotation (from the resolver / bytecode);
//! this pure-AST pass covers the **built-in JDK annotations whose target is fixed and universal**, so
//! it's false-positive-free without resolving anything:
//!   * `@Override` → methods only;
//!   * `@FunctionalInterface` → interface types only;
//!   * `@SafeVarargs` → methods & constructors only.
//!
//! Any annotation not in the table is skipped (a custom `@Target` could allow anything) — the
//! resolver-backed phase generalises this later.
//!
//! Plus one check that needs no table at all: **the same element given a value twice**
//! (`@Column(name = "a", name = "b")`, javac's `compiler.err.duplicate.annotation.member.value`).
//! Which value wins is not a question Java answers — it refuses to compile — and on a long
//! configuration annotation the two are rarely adjacent enough to see.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Where a declaration sits, for target checking.
#[derive(Clone, Copy, PartialEq)]
enum Target {
    Method,
    Constructor,
    Field,
    Type,
    /// An interface specifically (a sub-case of `Type`).
    Interface,
}

/// The declaration kinds we attach annotations to, mapped to their target category.
fn target_of(kind: &str) -> Option<Target> {
    match kind {
        "method_declaration" => Some(Target::Method),
        "constructor_declaration" => Some(Target::Constructor),
        "field_declaration" => Some(Target::Field),
        "interface_declaration" | "annotation_type_declaration" => Some(Target::Interface),
        "class_declaration" | "enum_declaration" | "record_declaration" => Some(Target::Type),
        _ => None,
    }
}

/// Whether `annotation` (a built-in with a fixed target) is allowed on `target`; `None` for an
/// annotation we don't model.
fn allowed(annotation: &str, target: Target) -> Option<bool> {
    match annotation {
        "Override" => Some(target == Target::Method),
        "SafeVarargs" => Some(matches!(target, Target::Method | Target::Constructor)),
        "FunctionalInterface" => Some(target == Target::Interface),
        _ => None,
    }
}

/// A readable name for the target, for the message.
fn target_word(annotation: &str) -> &'static str {
    match annotation {
        "Override" => "methods",
        "SafeVarargs" => "methods and constructors",
        "FunctionalInterface" => "interfaces",
        _ => "this element",
    }
}

/// All annotation-target errors in `root`.
pub fn annotation_errors(root: Node, source: &str) -> Vec<Diagnostic> {
    annotation_errors_nodes(&crate::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
pub fn annotation_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        let Some(target) = target_of(n.kind()) else { continue };
        for (name, node) in annotations_of(n, bytes) {
            if allowed(&name, target) == Some(false) {
                out.push(Diagnostic {
                    message: format!("`@{name}` is only applicable to {}", target_word(&name)),
                    severity: crate::check_id::CheckId::AnnotationNotApplicable.severity().to_string(),
                    code: crate::check_id::CheckId::AnnotationNotApplicable.code().to_string(),
                    start: node.start_byte(),
                    end: node.end_byte(),
                });
            }
        }
    }
    out
}

/// The `(simple_name, node)` of every annotation in a declaration's `modifiers`.
fn annotations_of<'a>(node: Node<'a>, bytes: &[u8]) -> Vec<(String, Node<'a>)> {
    let mut out = Vec::new();
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() != "modifiers" {
            continue;
        }
        let mut mc = ch.walk();
        for m in ch.children(&mut mc) {
            if matches!(m.kind(), "marker_annotation" | "annotation") {
                if let Some(name) = m.child_by_field_name("name") {
                    if let Ok(t) = name.utf8_text(bytes) {
                        // Simple name (last segment of a possibly-qualified annotation name).
                        let simple = t.rsplit('.').next().unwrap_or(t).to_string();
                        out.push((simple, m));
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        p.parse(src, None).unwrap()
    }

    fn errs(src: &str) -> Vec<String> {
        let tree = parse(src);
        annotation_errors(tree.root_node(), src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn override_on_method_is_ok() {
        assert!(errs("class C { @Override public String toString() { return \"\"; } }").is_empty());
    }

    #[test]
    fn override_on_field_is_flagged() {
        let e = errs("class C { @Override int x; }");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("@Override"));
        assert!(e[0].contains("methods"));
    }

    #[test]
    fn override_on_class_is_flagged() {
        let e = errs("@Override class C {}");
        assert_eq!(e.len(), 1);
    }

    #[test]
    fn functional_interface_on_interface_is_ok() {
        assert!(errs("@FunctionalInterface interface I { void run(); }").is_empty());
    }

    #[test]
    fn functional_interface_on_class_is_flagged() {
        let e = errs("@FunctionalInterface class C {}");
        assert_eq!(e.len(), 1);
        assert!(e[0].contains("interfaces"));
    }

    #[test]
    fn safevarargs_on_field_is_flagged_on_method_ok() {
        assert_eq!(errs("class C { @SafeVarargs int x; }").len(), 1);
        assert!(errs("class C { @SafeVarargs final void m(int... xs) {} }").is_empty());
    }

    #[test]
    fn unknown_annotation_is_never_flagged() {
        // A custom annotation could `@Target` anything → we don't model it, so never flag.
        assert!(errs("class C { @Inject int x; @Autowired void m() {} }").is_empty());
    }

    #[test]
    fn qualified_override_name_is_recognised() {
        let e = errs("class C { @java.lang.Override int x; }");
        assert_eq!(e.len(), 1, "qualified @java.lang.Override still resolves to Override: {e:?}");
    }
}


// ── the same element valued twice ────────────────────────────────────────────

/// Every `@Foo(a = 1, a = 2)` in the pre-collected node slice.
///
/// Pairs only. A single-element `@Foo("x")` has no name to repeat, and a nested annotation's own
/// arguments are a separate list reached as its own `annotation` node — so no recursion here.
pub fn duplicate_annotation_values_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() != "annotation_argument_list" {
            continue;
        }
        let mut seen: Vec<&str> = Vec::new();
        let mut cw = n.walk();
        for pair in n.named_children(&mut cw) {
            if pair.kind() != "element_value_pair" {
                continue;
            }
            let Some(key_node) = pair.child_by_field_name("key") else { continue };
            let Ok(key) = key_node.utf8_text(bytes) else { continue };
            if seen.contains(&key) {
                out.push(crate::check_id::CheckId::DuplicateAnnotationValue.at(
                    key_node,
                    format!("`{key}` is given a value twice in this annotation"),
                ));
            } else {
                seen.push(key);
            }
        }
    }
    out
}

#[cfg(test)]
mod duplicate_value_tests {
    use crate::check::collect_nodes;

    fn codes(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).expect("parse");
        let nodes = collect_nodes(tree.root_node());
        super::duplicate_annotation_values_nodes(&nodes, src).into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn the_same_element_valued_twice_is_flagged() {
        let src = r#"class A { @Column(name = "a", nullable = true, name = "b") String f; }"#;
        assert_eq!(codes(src), ["duplicate-annotation-value"]);
    }

    #[test]
    fn distinct_elements_are_fine() {
        let src = r#"class A { @Column(name = "a", nullable = true) String f; }"#;
        assert!(codes(src).is_empty());
    }

    /// The same element name on two DIFFERENT annotations is two annotations, not a duplicate.
    #[test]
    fn the_same_name_on_two_annotations_is_fine() {
        let src = r#"class A { @Column(name = "a") @JoinColumn(name = "b") String f; }"#;
        assert!(codes(src).is_empty());
    }

    #[test]
    fn a_nested_annotation_keeps_its_own_argument_list() {
        let src = r#"class A { @Table(indexes = @Index(name = "i"), name = "t") String f; }"#;
        assert!(codes(src).is_empty());
    }
}
