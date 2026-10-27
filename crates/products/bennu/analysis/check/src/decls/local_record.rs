//! A local record reaching the method around it (pure-AST, JLS §14.3, §8.10).
//!
//! A record is implicitly `static`, and that holds for one declared inside a method too: it has no
//! enclosing instance and cannot capture the method's locals. Reading either is
//! `compiler.err.non-static.cant.be.ref`.
//!
//! ```java
//! void m() {
//!     int base = 1;
//!     record Local(int x) { int plus() { return x + base; } }   // `base` is a local of m()
//! }
//! ```
//!
//! Conservative, since a name read inside a record could be many things: it is judged only when it is a
//! bare value reference ([`is_value_position`]), the record declares nothing by that name anywhere (a
//! component, a field, a parameter, a local, a pattern variable), and it then names either
//!   * a local or parameter of the enclosing method declared **before** the record, or
//!   * a non-`static` field of the enclosing class that no local of the method hides.
//!
//! A name that is neither (a static field, an outer class's member, a type) stays silent.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::nodes::has_keyword;
use crate::support::scopes::{declares_name_in_scope, enclosing_executable_scope, is_value_position, params_declare};

/// Every use of an enclosing local or instance field inside a local record.
pub fn local_record_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "record_declaration" && n.parent().is_some_and(|p| p.kind() == "block") {
            check_local_record(n, bytes, &mut out);
        }
    }
    out
}

fn check_local_record(record: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(body) = record.child_by_field_name("body") else { return };
    let Some(method) = enclosing_executable_scope(record) else { return };
    let class = enclosing_class(method);

    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        if n.kind() == "identifier" && is_value_position(n) {
            if let Ok(name) = n.utf8_text(bytes) {
                if !declared_inside(record, body, name, bytes) {
                    if local_declared_before(record, method, name, bytes) {
                        out.push(CheckId::StaticContextAccess.at(
                            n,
                            format!("A local record is implicitly static and cannot use the local variable `{name}` of the enclosing method"),
                        ));
                    } else if class.is_some_and(|c| has_instance_field(c, name, bytes))
                        && !declares_name_in_scope(method, name, bytes)
                    {
                        out.push(CheckId::StaticContextAccess.at(
                            n,
                            format!("A local record is implicitly static and cannot use the instance field `{name}` of the enclosing class"),
                        ));
                    }
                }
            }
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
}

/// The class around `node`, when the nearest type declaration is a plain class.
fn enclosing_class(node: Node) -> Option<Node> {
    let mut cur = node.parent();
    while let Some(p) = cur {
        match p.kind() {
            "class_declaration" => return Some(p),
            "interface_declaration" | "enum_declaration" | "record_declaration" | "annotation_type_declaration" => return None,
            _ => cur = p.parent(),
        }
    }
    None
}

/// Whether the record binds `name` itself anywhere — a component, a field, a parameter or local of one
/// of its methods, a pattern variable. Over-collecting only silences the check.
fn declared_inside(record: Node, body: Node, name: &str, bytes: &[u8]) -> bool {
    params_declare(record, name, bytes) || declares_name_in_scope(record, name, bytes) || declares_declarator(body, name, bytes)
}

/// Whether any `variable_declarator` under `root` (a field, a local) is named `name`.
fn declares_declarator(root: Node, name: &str, bytes: &[u8]) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "variable_declarator"
            && n.child_by_field_name("name").and_then(|id| id.utf8_text(bytes).ok()) == Some(name)
        {
            return true;
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    false
}

/// Whether `method` declares `name` as a parameter, or a block around the record declares it as a local
/// in a statement that precedes the record — the only way the record could see it.
pub(crate) fn local_declared_before(record: Node, method: Node, name: &str, bytes: &[u8]) -> bool {
    if params_declare(method, name, bytes) {
        return true;
    }
    let mut cur = record.parent();
    while let Some(p) = cur {
        if p.id() == method.id() {
            break;
        }
        if p.kind() == "block" {
            let mut c = p.walk();
            let declared = p
                .named_children(&mut c)
                .take_while(|stmt| stmt.start_byte() < record.start_byte())
                .any(|stmt| stmt.kind() == "local_variable_declaration" && declares_declarator(stmt, name, bytes));
            if declared {
                return true;
            }
        }
        cur = p.parent();
    }
    false
}

/// Whether the class body declares a non-`static` field called `name`.
fn has_instance_field(class: Node, name: &str, bytes: &[u8]) -> bool {
    let Some(body) = class.child_by_field_name("body") else { return false };
    let mut c = body.walk();
    let found = body
        .named_children(&mut c)
        .filter(|m| m.kind() == "field_declaration" && !has_keyword(*m, bytes, "static"))
        .any(|field| declares_declarator(field, name, bytes));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        local_record_errors_nodes(&nodes, src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn a_local_of_the_enclosing_method_is_flagged() {
        let m = messages("class C { void m() { int base = 1; record R(int x) { int plus() { return x + base; } } } }");
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("local variable `base`"), "{m:?}");
        let param = messages("class C { void m(int base) { record R(int x) { int plus() { return x + base; } } } }");
        assert_eq!(param.len(), 1, "{param:?}");
    }

    #[test]
    fn an_instance_field_is_flagged_and_a_static_one_is_not() {
        let m = messages("class C { int f; void m() { record R(int x) { int plus() { return x + f; } } } }");
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("instance field `f`"), "{m:?}");
        assert!(messages("class C { static int f; void m() { record R(int x) { int plus() { return x + f; } } } }").is_empty());
    }

    #[test]
    fn names_the_record_declares_or_that_come_later_are_not_captures() {
        // The component, a local of its own method, and a local declared AFTER the record.
        assert!(messages("class C { void m() { int x = 1; record R(int x) { int p() { return x; } } } }").is_empty());
        assert!(messages("class C { void m() { record R(int a) { int p() { int t = 1; return a + t; } } int t = 2; } }").is_empty());
        // A member (non-local) record sees neither locals nor, here, anything to judge.
        assert!(messages("class C { int f; record R(int x) { int p() { return x; } } }").is_empty());
    }
}
