//! `illegal.forward.ref` — an initializer that reads, by simple name, a field declared further down.
//!
//! ```java
//! int forward = later;     // error: `later` is declared below
//! int later = 1;
//! ```
//!
//! JLS 8.3.3: in a field initializer or an initializer block, a simple-name use of a field that is
//! declared textually later in the same class is an error — when it is not the target of an assignment
//! and the two fields are the same kind (both static or both instance). Qualified uses (`this.later`,
//! `Owner.later`) are exempt, and so is anything inside a lambda or an anonymous class, which run later.
//!
//! A name the initializer declares itself (a pattern binding) is never mistaken for the field.

use std::collections::HashMap;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::decls::enum_rules::{declared_names, is_bare_read_or_write};
use crate::engine::check_id::CheckId;
use crate::support::nodes::{has_keyword, text};

/// Every forward reference from a field initializer or an initializer block.
pub fn forward_ref_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() != "class_body" || n.parent().is_some_and(|p| p.kind() == "object_creation_expression") {
            continue;
        }
        check_body(n, bytes, &mut out);
    }
    out
}

fn check_body(body: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mut c = body.walk();
    let members: Vec<Node> = body.named_children(&mut c).collect();
    // name → (position, is_static) of each field's declaration.
    let mut fields: HashMap<String, (usize, bool)> = HashMap::new();
    for (i, m) in members.iter().enumerate() {
        if m.kind() != "field_declaration" {
            continue;
        }
        let is_static = has_keyword(*m, bytes, "static");
        let mut dc = m.walk();
        for d in m.children_by_field_name("declarator", &mut dc) {
            if let Some(name) = d.child_by_field_name("name").and_then(|n| text(n, bytes)) {
                fields.entry(name).or_insert((i, is_static));
            }
        }
    }
    for (i, m) in members.iter().enumerate() {
        let (scope, is_static) = match m.kind() {
            "field_declaration" => (*m, has_keyword(*m, bytes, "static")),
            "static_initializer" => (*m, true),
            "block" => (*m, false),
            _ => continue,
        };
        let own = declared_names_in_initializers(scope, bytes);
        let mut stack = vec![scope];
        while let Some(n) = stack.pop() {
            if matches!(n.kind(), "lambda_expression" | "class_body" | "method_declaration") {
                continue; // runs later, or is a body of its own
            }
            if n.kind() == "identifier" && is_bare_read_or_write(n) && !is_assignment_target(n) {
                if let Some(name) = text(n, bytes) {
                    if let Some(&(at, field_static)) = fields.get(&name) {
                        let in_own_declarator = m.kind() == "field_declaration" && at == i;
                        if at > i && field_static == is_static && !own.contains(&name) && !in_own_declarator {
                            out.push(CheckId::ForwardReference.at(
                                n,
                                format!("Illegal forward reference: `{name}` is declared below this initializer"),
                            ));
                        }
                    }
                }
            }
            let mut c = n.walk();
            stack.extend(n.children(&mut c));
        }
    }
}

/// Names the initializer declares for itself, other than the field it initializes: a pattern binding
/// or a switch-expression variable. The declared field's own name (`x` in `int x = …`) is excluded —
/// it is the very name a forward reference would be confused with.
fn declared_names_in_initializers(scope: Node, bytes: &[u8]) -> std::collections::HashSet<String> {
    let mut names = declared_names(scope, bytes);
    if scope.kind() == "field_declaration" {
        let mut dc = scope.walk();
        for d in scope.children_by_field_name("declarator", &mut dc) {
            if let Some(n) = d.child_by_field_name("name").and_then(|x| text(x, bytes)) {
                names.remove(&n);
            }
        }
    }
    names
}

/// The left side of `x = …` (a plain assignment is the one write JLS 8.3.3 allows).
fn is_assignment_target(id: Node) -> bool {
    id.parent().is_some_and(|p| {
        p.kind() == "assignment_expression"
            && p.child_by_field_name("left").is_some_and(|l| l.id() == id.id())
            && p.child(1).is_some_and(|op| op.kind() == "=")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(src: &str) -> usize {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        forward_ref_errors_nodes(&nodes, src).len()
    }

    #[test]
    fn reading_a_field_declared_below_is_reported() {
        assert_eq!(count("class A { int a = b; int b = 1; }"), 1);
        assert_eq!(count("class A { static int a = b + 1; static int b = 1; }"), 1);
        assert_eq!(count("class A { { x = y; } int y; int x; }"), 1);
    }

    #[test]
    fn declared_above_qualified_assigned_and_deferred_uses_are_fine() {
        assert_eq!(count("class A { int b = 1; int a = b; }"), 0);
        assert_eq!(count("class A { int a = this.b; int b = 1; }"), 0);
        assert_eq!(count("class A { int a = A.b; static int b = 1; }"), 0);
        assert_eq!(count("class A { { b = 2; } int b; }"), 0);
        assert_eq!(count("class A { Runnable r = () -> b++; int b; }"), 0);
        assert_eq!(count("class A { Object o = new Object() { int x = b; }; int b; }"), 0);
        // A static initializer reading an instance field is another error, not this one.
        assert_eq!(count("class A { static int a = b; int b; }"), 0);
    }
}
