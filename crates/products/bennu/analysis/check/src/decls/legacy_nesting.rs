//! What a nested or local type could not hold before Java 16, and what no version allows.
//!
//! * **An annotation interface cannot be local** (`annotation.decl.not.allowed.here`) — at any level.
//! * **A local interface** (`intf.not.allowed.here`) is an error below Java 16; from 16 on it is legal.
//! * **An inner class cannot have `static` members** (`icls.cant.have.static.decl`) below Java 16:
//!   no static method, no static initializer, no static type, and no static field unless it is a
//!   constant variable. JEP 395 lifted the rule, so the check is silent from 16, and silent without a
//!   known level rather than guessing which side of it the project is on.
//!
//! "Inner" is a member class without `static`, or a local class — the two shapes that carry an
//! enclosing instance. A member of an interface is implicitly static, so it never counts.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::decls::enum_rules::non_constant_fields;
use crate::engine::check_id::CheckId;
use crate::support::nodes::has_keyword;

/// Every nesting violation in the pre-collected node slice, for a project compiled at `java_major`.
pub fn legacy_nesting_errors_nodes(nodes: &[Node], source: &str, java_major: Option<u32>) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let before_16 = java_major.is_some_and(|m| m < 16);
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "annotation_type_declaration" if is_local(n) => {
                out.push(CheckId::IllegalDeclaration.at(n, "An annotation interface cannot be declared locally"));
            }
            "interface_declaration" if before_16 && is_local(n) => {
                out.push(CheckId::IllegalDeclaration.at(n, "A local interface needs Java 16 or later"));
            }
            "enum_declaration" if before_16 && is_local(n) => {
                out.push(CheckId::IllegalDeclaration.at(n, "A local enum needs Java 16 or later"));
            }
            // A local class cannot be `sealed` or `non-sealed` at any level: it has no name to permit.
            "class_declaration"
                if is_local(n) && (has_keyword(n, bytes, "sealed") || has_keyword(n, bytes, "non-sealed")) =>
            {
                out.push(CheckId::IllegalDeclaration.at(n, "A local class cannot be `sealed` or `non-sealed`"));
            }
            "class_declaration" if before_16 && is_inner(n, bytes) => static_members(n, bytes, &mut out),
            _ => {}
        }
    }
    out
}

/// Declared in a block — a method body, a lambda body, an initializer.
fn is_local(n: Node) -> bool {
    n.parent().is_some_and(|p| p.kind() == "block")
}

/// A class that carries an enclosing instance.
fn is_inner(class: Node, bytes: &[u8]) -> bool {
    if has_keyword(class, bytes, "static") {
        return false;
    }
    let Some(parent) = class.parent() else { return false };
    match parent.kind() {
        "block" => true,
        "class_body" => parent
            .parent()
            .is_some_and(|owner| matches!(owner.kind(), "class_declaration" | "enum_declaration" | "record_declaration")),
        _ => false,
    }
}

fn static_members(class: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(body) = class.child_by_field_name("body") else { return };
    let mut c = body.walk();
    for member in body.named_children(&mut c) {
        let offends = match member.kind() {
            "method_declaration" => has_keyword(member, bytes, "static"),
            "static_initializer" => true,
            "field_declaration" => {
                has_keyword(member, bytes, "static") && !non_constant_fields(member, bytes).is_empty()
            }
            "class_declaration" => has_keyword(member, bytes, "static"),
            // Implicitly static.
            "interface_declaration" | "enum_declaration" | "record_declaration" | "annotation_type_declaration" => true,
            _ => false,
        };
        if offends {
            out.push(CheckId::IllegalDeclaration.at(
                member,
                "An inner class cannot declare static members before Java 16",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str, major: Option<u32>) -> usize {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        legacy_nesting_errors_nodes(&nodes, src, major).len()
    }

    #[test]
    fn an_inner_class_with_a_static_method_is_an_error_below_16_only() {
        let src = "class A { class In { static void m() { } } }";
        assert_eq!(run(src, Some(8)), 1);
        assert_eq!(run(src, Some(16)), 0);
        assert_eq!(run(src, None), 0);
    }

    #[test]
    fn constants_static_nested_classes_and_interface_members_are_fine() {
        assert_eq!(run("class A { class In { static final int K = 1; } }", Some(8)), 0);
        assert_eq!(run("class A { static class Nested { static void m() { } } }", Some(8)), 0);
        assert_eq!(run("interface I { class C { static void m() { } } }", Some(8)), 0);
        assert_eq!(run("class A { class In { static int counter; } }", Some(8)), 1);
        assert_eq!(run("class A { class In { interface Deep { } } }", Some(8)), 1);
    }

    #[test]
    fn a_local_interface_needs_16_and_a_local_annotation_is_never_legal() {
        assert_eq!(run("class A { void m() { interface L { } } }", Some(8)), 1);
        assert_eq!(run("class A { void m() { interface L { } } }", Some(17)), 0);
        assert_eq!(run("class A { void m() { @interface L { } } }", Some(21)), 1);
    }
}
