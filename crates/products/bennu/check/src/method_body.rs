//! Missing-method-body diagnostics (pure-AST).
//!
//! A method that MUST have a body but doesn't. Two structural cases the compiler always rejects,
//! detectable false-positive-free from the syntax tree:
//!   * a **concrete class/enum method** with no body (ends in `;` not `{…}`) that is neither
//!     `abstract` nor `native` — those two are the only legal ways to omit a body in a class;
//!   * a **`private` interface method** with no body — private interface methods are always
//!     concrete (Java 9+), so a bodyless one is illegal.
//!
//! The converse ("`abstract` method with a body", "`default` in a class") lives in
//! [`crate::declarations`]; this module is only the missing-body direction.
//!
//! A `method_declaration` has a body iff it owns a `block` child (its `body` field). No block = the
//! declaration ended with `;`.
//!
//! DELIBERATELY NOT flagged (would be a false positive):
//!   * a bodyless `abstract`/`native` class method — both are legal;
//!   * a bodyless NON-private interface method — that's an ordinary abstract interface method;
//!   * annotation-type elements — those parse as `annotation_type_element_declaration`, a different
//!     node kind, so they never reach this check;
//!   * anything under an `ERROR` subtree — a genuinely malformed method already surfaces via
//!     [`crate::syntax`], so we only act on a well-formed `method_declaration` that simply lacks a
//!     `block` (`node.has_error()` → skip, don't double-report).

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Type declarations whose body can hold methods. Used to find a method's enclosing type.
const TYPE_DECLS: [&str; 5] = [
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
];

/// All missing-method-body errors over the shared pre-collected node list (one traversal across all
/// pure-AST checks).
pub fn method_body_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "method_declaration" {
            check_method(n, bytes, &mut out);
        }
    }
    out.sort_by_key(|d| d.start);
    out
}

fn check_method(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    // Only well-formed methods: a malformed one already surfaces as an ERROR via the syntax pass, so
    // acting inside an ERROR subtree would double-report. Skip if the node carries any error.
    if n.has_error() {
        return;
    }
    // Has a body iff it owns a `block` child (its `body` field). No block = the decl ended with `;`.
    let has_body = n.child_by_field_name("body").map(|b| b.kind() == "block").unwrap_or(false);
    if has_body {
        return;
    }

    let Some(ty) = enclosing_type(n) else { return };
    let mods = modifier_keywords(n, bytes);
    let has = |m: &str| mods.contains(&m);

    match ty.kind() {
        // Case 1 — a concrete class/enum method must have a body unless it's `abstract` or `native`.
        "class_declaration" | "enum_declaration" => {
            if !has("abstract") && !has("native") {
                out.push(err(name_span(n), "Missing method body, or declare `abstract`"));
            }
        }
        // Case 2 — a `private` interface method must be concrete (a non-private bodyless interface
        // method is a normal abstract method → left alone; `default`/`static` always have a body).
        "interface_declaration" => {
            if has("private") {
                out.push(err(name_span(n), "A private interface method must have a body"));
            }
        }
        _ => {}
    }
}

/// The keyword modifiers (anonymous tokens) on a declaration — `["private", "static"]`. Annotations
/// (named nodes inside `modifiers`) are excluded. Mirrors `declarations::modifier_keywords`.
fn modifier_keywords<'a>(node: Node, bytes: &'a [u8]) -> Vec<&'a str> {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "modifiers" {
            let mut out = Vec::new();
            let mut mc = ch.walk();
            for m in ch.children(&mut mc) {
                if !m.is_named() {
                    if let Ok(t) = m.utf8_text(bytes) {
                        out.push(t);
                    }
                }
            }
            return out;
        }
    }
    Vec::new()
}

/// The nearest enclosing type declaration of `node`, if any (mirrors `declarations::enclosing_type`).
fn enclosing_type(node: Node) -> Option<Node> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if TYPE_DECLS.contains(&n.kind()) {
            return Some(n);
        }
        cur = n.parent();
    }
    None
}

/// The span to anchor the error on: the method's NAME token (tighter than the whole decl).
fn name_span(node: Node) -> Node {
    node.child_by_field_name("name").unwrap_or(node)
}

fn err(node: Node, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        message: message.into(),
        severity: "error".to_string(),
        start: node.start_byte(),
        end: node.end_byte(),
    }
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

    fn msgs(src: &str) -> Vec<String> {
        let tree = parse(src);
        let nodes = crate::check::collect_nodes(tree.root_node());
        method_body_errors_nodes(&nodes, src).into_iter().map(|d| d.message).collect()
    }

    // ---- Case 1: concrete class/enum method missing its body ----

    #[test]
    fn concrete_class_method_without_body_is_flagged() {
        let m = msgs("class C { void m(); }");
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("Missing method body"), "{m:?}");
    }

    #[test]
    fn concrete_enum_method_without_body_is_flagged() {
        let m = msgs("enum E { A; void m(); }");
        assert!(m.iter().any(|s| s.contains("Missing method body")), "{m:?}");
    }

    #[test]
    fn abstract_class_method_without_body_is_not_flagged() {
        // Legal bodyless method (abstract) — the placement legality is `declarations`' job, not ours.
        assert!(msgs("class C { abstract void m(); }").is_empty(), "{:?}", msgs("class C { abstract void m(); }"));
    }

    #[test]
    fn abstract_method_in_abstract_class_is_not_flagged() {
        assert!(msgs("abstract class C { abstract void m(); }").is_empty());
    }

    #[test]
    fn native_class_method_without_body_is_not_flagged() {
        assert!(msgs("class C { native void m(); }").is_empty());
    }

    #[test]
    fn concrete_class_method_with_body_is_not_flagged() {
        assert!(msgs("class C { void m(){} }").is_empty());
    }

    // ---- Case 2: private interface method missing its body ----

    #[test]
    fn private_interface_method_without_body_is_flagged() {
        let m = msgs("interface I { private void h(); }");
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("private interface method must have a body"), "{m:?}");
    }

    #[test]
    fn normal_abstract_interface_method_is_not_flagged() {
        // A plain non-private bodyless interface method is a legal abstract method → leave it alone.
        assert!(msgs("interface I { void ok(); }").is_empty());
    }

    #[test]
    fn default_and_static_interface_methods_are_not_flagged() {
        assert!(msgs("interface I { default void d(){} static void s(){} }").is_empty());
    }

    // ---- General negatives ----

    #[test]
    fn ordinary_class_is_clean() {
        let src = "public class C { private int x; public int get() { return x; } }";
        assert!(msgs(src).is_empty(), "{:?}", msgs(src));
    }
}
