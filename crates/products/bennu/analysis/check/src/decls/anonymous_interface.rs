//! An anonymous class that implements an interface cannot be given constructor arguments (pure-AST,
//! JLS §15.9.5.1): `new Callback(1) { … }` is an error when `Callback` is an interface, because an
//! interface has no constructor to pass `1` to — the anonymous class extends `Object`.
//!
//! Judged for an interface declared in the file under a simple name nothing else in it shares.

use std::collections::HashMap;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;

/// All `new Interface(args) { … }` over the shared pre-collected node list.
pub fn anonymous_interface_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    // Simple name → whether the one type of that name is an interface; ambiguous names are dropped.
    let mut kinds: HashMap<&str, Option<bool>> = HashMap::new();
    for &n in nodes.iter().filter(|n| {
        matches!(n.kind(), "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration")
    }) {
        let Some(name) = n.child_by_field_name("name").and_then(|id| id.utf8_text(bytes).ok()) else { continue };
        let is_interface = n.kind() == "interface_declaration";
        kinds.entry(name).and_modify(|slot| *slot = None).or_insert(Some(is_interface));
    }

    let mut out = Vec::new();
    for &n in nodes.iter().filter(|n| n.kind() == "object_creation_expression") {
        let has_body = {
            let mut c = n.walk();
            let found = n.children(&mut c).any(|ch| ch.kind() == "class_body");
            found
        };
        let has_arguments = n.child_by_field_name("arguments").is_some_and(|a| a.named_child_count() > 0);
        let Some(ty) = n.child_by_field_name("type").filter(|t| t.kind() == "type_identifier") else { continue };
        let Ok(name) = ty.utf8_text(bytes) else { continue };
        if has_body && has_arguments && n.child_by_field_name("object").is_none() && kinds.get(name) == Some(&Some(true)) {
            out.push(CheckId::IllegalDeclaration.at(
                ty,
                format!("`{name}` is an interface, so an anonymous class implementing it takes no constructor arguments"),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(src: &str) -> usize {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        anonymous_interface_errors_nodes(&nodes, src).len()
    }

    #[test]
    fn arguments_to_an_anonymous_interface_are_flagged() {
        assert_eq!(count("interface I { void f(); } class C { void m() { new I(1) { public void f() { } }; } }"), 1);
    }

    #[test]
    fn a_class_or_an_empty_argument_list_is_not() {
        assert_eq!(count("class B { B(int x) { } } class C { void m() { new B(1) { }; } }"), 0);
        assert_eq!(count("interface I { void f(); } class C { void m() { new I() { public void f() { } }; } }"), 0);
        assert_eq!(count("interface I { } class C { void m() { Object o = new I(); } }"), 0);
    }
}
