//! A field initialized from itself — `private int total = total;`.
//!
//! javac calls it `compiler.err.illegal.self.ref`, and it is an error rather than a warning because
//! the read can never see anything but the field's default. It survives review easily: the name on
//! both sides is the name you meant, and a constructor parameter or a local of the same name is the
//! thing that was supposed to be there.
//!
//! Fields only. The same shape in a local (`int x = x;`) is javac's
//! `compiler.err.var.might.not.have.been.initialized`, which the definite-assignment check owns —
//! two checks reporting one line would be worse than either.
//!
//! Three things are deliberately NOT flagged, each because it is legal Java:
//!   * a qualified read — `int f = this.f;` and `static int s = A.s;` compile, and mean "the default";
//!   * a read deferred into a lambda or an anonymous/local class body — `Supplier<T> f = () -> f.get()`
//!     runs after the field exists, and is a normal idiom;
//!   * a different field that merely shares a prefix, or the name used as a member selector
//!     (`other.total`), which names someone else's field entirely.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check_id::CheckId;

/// Every self-referencing field initializer in the pre-collected node slice.
pub fn self_ref_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() != "field_declaration" {
            continue;
        }
        let mut cw = n.walk();
        for declarator in n.named_children(&mut cw) {
            if declarator.kind() != "variable_declarator" {
                continue;
            }
            let (Some(name_node), Some(value)) = (
                declarator.child_by_field_name("name"),
                declarator.child_by_field_name("value"),
            ) else {
                continue;
            };
            let Ok(name) = name_node.utf8_text(bytes) else { continue };
            if let Some(hit) = bare_read_of(value, name, bytes) {
                out.push(CheckId::SelfReferencingInitializer.at(
                    hit,
                    format!("`{name}` is read by its own initializer, so it can only be the default"),
                ));
            }
        }
    }
    out
}

/// The first bare read of `name` inside `value`, if there is one.
///
/// An explicit stack, not recursion: an initializer is an expression, and an expression can nest as
/// deeply as it is long — see `bennu-java`'s `deep_expression` tests for what that costs.
fn bare_read_of<'t>(value: Node<'t>, name: &str, bytes: &[u8]) -> Option<Node<'t>> {
    let mut stack = vec![value];
    while let Some(n) = stack.pop() {
        // A body that runs later cannot be reading the field too early.
        if matches!(n.kind(), "lambda_expression" | "class_body") {
            continue;
        }
        if n.kind() == "identifier" && n.utf8_text(bytes).is_ok_and(|t| t == name) && is_bare(n) {
            return Some(n);
        }
        let mut cw = n.walk();
        stack.extend(n.named_children(&mut cw));
    }
    None
}

/// Whether this identifier stands on its own rather than being the part after a `.`.
///
/// `other.total` and `this.total` both put the name in a field access's `field` slot; only the
/// first is someone else's field, but neither is the unqualified read that is illegal — so one
/// test excludes both. A method call's name sits in `name`, and is not a read of the field at all.
fn is_bare(id: Node) -> bool {
    let Some(parent) = id.parent() else { return true };
    match parent.kind() {
        "field_access" | "scoped_identifier" | "method_invocation" | "scoped_type_identifier" => {
            parent.child_by_field_name("object").map(|o| o.id()) == Some(id.id())
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::collect_nodes;

    fn codes(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).expect("parse");
        let nodes = collect_nodes(tree.root_node());
        self_ref_errors_nodes(&nodes, src).into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn a_field_read_by_its_own_initializer_is_flagged() {
        assert_eq!(codes("class A { int total = total; }"), ["self-referencing-initializer"]);
    }

    #[test]
    fn the_read_is_found_inside_a_larger_expression() {
        assert_eq!(codes("class A { static int s = s + 1; }"), ["self-referencing-initializer"]);
        assert_eq!(codes("class A { int[] a = new int[a.length]; }"), ["self-referencing-initializer"]);
    }

    /// Legal Java, and the reason this check reads the tree instead of the text.
    #[test]
    fn a_qualified_read_of_the_same_field_is_legal() {
        assert!(codes("class A { int f = this.f; }").is_empty());
        assert!(codes("class A { static int s = A.s; }").is_empty());
    }

    #[test]
    fn a_read_deferred_into_a_lambda_is_legal() {
        let src = "class A { java.util.function.Supplier<Integer> f = () -> f.get() + 1; }";
        assert!(codes(src).is_empty());
    }

    #[test]
    fn a_read_deferred_into_an_anonymous_class_is_legal() {
        let src = "class A { Runnable r = new Runnable() { public void run() { r.toString(); } }; }";
        assert!(codes(src).is_empty());
    }

    #[test]
    fn another_fields_member_of_the_same_name_is_not_this_field() {
        assert!(codes("class A { int total = other.total; B other; }").is_empty());
    }

    /// A local with the same shape belongs to the definite-assignment check, not this one.
    #[test]
    fn a_local_of_the_same_shape_is_left_to_definite_assignment() {
        assert!(codes("class A { void m() { int x = x; } }").is_empty());
    }

    #[test]
    fn an_ordinary_initializer_is_left_alone() {
        assert!(codes("class A { int total = 0; int other = total; }").is_empty());
    }
}
