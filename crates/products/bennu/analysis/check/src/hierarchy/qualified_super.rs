//! Qualified `super` (pure-AST, JLS §8.8.7.1, §15.12.1): `outer.super(…)` and `Iface.super.m()`.
//!
//!   * **`outer.super(…)`** is the way to give an *inner* superclass its enclosing instance. When the
//!     superclass is declared in the file and is not an inner class (top-level, `static`, or a member
//!     of an interface) the qualifier has nothing to qualify (`illegal.qual.not.icls`).
//!   * **`Iface.super.m()`** reaches the default method of a *direct* superinterface. `Iface` must be
//!     listed in the enclosing class's `implements` clause (`not.encl.class` otherwise), and no other
//!     direct superinterface may extend `Iface` and redeclare `m`, which would make `Iface`'s version
//!     overridden (`illegal.default.super.call`).
//!
//! Everything is read from this file: the types involved must be declared here under a simple name
//! nothing else in it shares, and any shape that leaves room for doubt (an anonymous class around the
//! call, a generic `implements` entry, an enclosing type of the same name) is skipped.

use std::collections::HashMap;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::nodes::child_of_kind;

const TYPE_KINDS: [&str; 4] = ["class_declaration", "interface_declaration", "enum_declaration", "record_declaration"];

/// The file's types by simple name; `None` when the name is declared more than once.
type Declared<'t> = HashMap<&'t str, Option<Node<'t>>>;

fn declared_types<'t>(nodes: &[Node<'t>], bytes: &'t [u8]) -> Declared<'t> {
    let mut map: Declared = HashMap::new();
    for &n in nodes.iter().filter(|n| TYPE_KINDS.contains(&n.kind())) {
        let Some(name) = n.child_by_field_name("name").and_then(|id| id.utf8_text(bytes).ok()) else { continue };
        map.entry(name).and_modify(|slot| *slot = None).or_insert(Some(n));
    }
    map
}

/// All qualified-`super` errors over the shared pre-collected node list.
pub fn qualified_super_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let declared = declared_types(nodes, bytes);
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "explicit_constructor_invocation" => check_qualified_constructor_call(n, bytes, &declared, &mut out),
            "method_invocation" => check_interface_super_call(n, bytes, &declared, &mut out),
            _ => {}
        }
    }
    out
}

// ── outer.super(…) ───────────────────────────────────────────────────────────

fn check_qualified_constructor_call(call: Node, bytes: &[u8], declared: &Declared, out: &mut Vec<Diagnostic>) {
    if call.child_by_field_name("object").is_none() {
        return;
    }
    let Some(class) = enclosing_class(call) else { return };
    let Some(superclass) = class.child_by_field_name("superclass").and_then(|s| s.named_child(0)) else { return };
    if superclass.kind() != "type_identifier" {
        return;
    }
    let Ok(name) = superclass.utf8_text(bytes) else { return };
    let Some(Some(target)) = declared.get(name) else { return };
    if target.kind() == "class_declaration" && !is_inner_class(*target) {
        out.push(CheckId::IllegalDeclaration.at(
            call,
            format!("`{name}` is not an inner class, so its constructor takes no enclosing-instance qualifier"),
        ));
    }
}

/// Whether a class is an inner (non-static member) class — the only kind that has an enclosing instance
/// to qualify. A local class is judged inner: it may capture one.
fn is_inner_class(class: Node) -> bool {
    let Some(parent) = class.parent() else { return false };
    match parent.kind() {
        "program" => false,
        "class_body" => {
            let outer_is_class = parent.parent().is_some_and(|p| matches!(p.kind(), "class_declaration" | "enum_declaration" | "record_declaration"));
            outer_is_class && !has_modifier(class, "static")
        }
        // A member of an interface or annotation is implicitly static.
        "interface_body" | "annotation_type_body" => false,
        _ => true,
    }
}

fn has_modifier(decl: Node, keyword: &str) -> bool {
    child_of_kind(decl, "modifiers").is_some_and(|mods| {
        let mut c = mods.walk();
        let found = mods.children(&mut c).any(|m| m.kind() == keyword);
        found
    })
}

// ── Iface.super.m() ──────────────────────────────────────────────────────────

fn check_interface_super_call(call: Node, bytes: &[u8], declared: &Declared, out: &mut Vec<Diagnostic>) {
    // `X.super.m(…)`: an `object` that is a bare name, and a `super` token among the call's children.
    let Some(object) = call.child_by_field_name("object") else { return };
    if object.kind() != "identifier" {
        return;
    }
    let has_super_token = {
        let mut c = call.walk();
        let found = call.children(&mut c).any(|ch| ch.kind() == "super");
        found
    };
    if !has_super_token {
        return;
    }
    let Ok(qualifier) = object.utf8_text(bytes) else { return };
    let Some(Some(iface)) = declared.get(qualifier) else { return };
    if iface.kind() != "interface_declaration" || encloses(call, qualifier, bytes) {
        return;
    }
    let Some(class) = enclosing_class(call) else { return };
    let Some(direct) = direct_interfaces(class, bytes) else { return };
    let Some(method) = call.child_by_field_name("name").and_then(|n| n.utf8_text(bytes).ok()) else { return };

    if !direct.contains(&qualifier) {
        out.push(CheckId::UnresolvedSuperMethod.at(
            call,
            format!("`{qualifier}` is not a direct superinterface of the enclosing class, so `{qualifier}.super` is not allowed here"),
        ));
        return;
    }
    let overriding = direct.iter().filter(|other| **other != qualifier).find(|other| {
        let Some(Some(other_iface)) = declared.get(*other) else { return false };
        extends_and_declares(*other_iface, qualifier, method, bytes)
    });
    if let Some(other) = overriding {
        out.push(CheckId::UnresolvedSuperMethod.at(
            call,
            format!("`{qualifier}.super.{method}()` is not allowed: `{other}` overrides `{method}` in a more specific interface"),
        ));
    }
}

/// The simple names in the class's `implements` clause; `None` when any entry is not a plain name
/// (a generic or qualified one cannot be matched against this file's types).
fn direct_interfaces<'s>(class: Node, bytes: &'s [u8]) -> Option<Vec<&'s str>> {
    let Some(clause) = class.child_by_field_name("interfaces") else { return Some(Vec::new()) };
    let list = child_of_kind(clause, "type_list")?;
    let mut c = list.walk();
    let names = list
        .named_children(&mut c)
        .map(|t| if t.kind() == "type_identifier" { t.utf8_text(bytes).ok() } else { None })
        .collect();
    names
}

/// Whether `iface` extends `parent` directly and declares a method called `method`.
fn extends_and_declares(iface: Node, parent: &str, method: &str, bytes: &[u8]) -> bool {
    if iface.kind() != "interface_declaration" {
        return false;
    }
    let extends_parent = child_of_kind(iface, "extends_interfaces")
        .and_then(|e| child_of_kind(e, "type_list"))
        .is_some_and(|list| {
            let mut c = list.walk();
            let found = list.named_children(&mut c).any(|t| t.utf8_text(bytes) == Ok(parent));
            found
        });
    let Some(body) = iface.child_by_field_name("body") else { return false };
    let mut c = body.walk();
    let declares = body.named_children(&mut c).any(|m| {
        m.kind() == "method_declaration" && m.child_by_field_name("name").and_then(|n| n.utf8_text(bytes).ok()) == Some(method)
    });
    extends_parent && declares
}

// ── tree helpers ─────────────────────────────────────────────────────────────

/// The nearest enclosing `class_declaration`, or `None` when an anonymous class body comes first.
fn enclosing_class(node: Node) -> Option<Node> {
    let mut cur = node.parent();
    while let Some(p) = cur {
        match p.kind() {
            "class_declaration" => return Some(p),
            "class_body" if p.parent().is_some_and(|g| g.kind() == "object_creation_expression") => return None,
            "interface_declaration" | "enum_declaration" | "record_declaration" => return None,
            _ => cur = p.parent(),
        }
    }
    None
}

/// Whether a type called `name` encloses `node` (then `name.super` may name that enclosing type instead).
fn encloses(node: Node, name: &str, bytes: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        if TYPE_KINDS.contains(&p.kind())
            && p.child_by_field_name("name").and_then(|id| id.utf8_text(bytes).ok()) == Some(name)
        {
            return true;
        }
        cur = p.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        qualified_super_errors_nodes(&nodes, src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn a_qualifier_on_a_non_inner_superclass_is_flagged() {
        let bad = "class B { } class C extends B { C(B outer) { outer.super(); } }";
        assert_eq!(messages(bad).len(), 1, "{:?}", messages(bad));
        let static_nested = "class O { static class B { } class C extends B { C(O o) { o.super(); } } }";
        assert_eq!(messages(static_nested).len(), 1);
        let inner = "class O { class B { } } class C extends O.B { C(O o) { o.super(); } }";
        assert!(messages(inner).is_empty());
        let inner_simple = "class O { class B { } class D extends B { D(O o) { o.super(); } } }";
        assert!(messages(inner_simple).is_empty());
    }

    const IFACES: &str = "interface Root { default void hello() { } } interface Mid extends Root { } interface Over extends Root { default void hello() { } }";

    #[test]
    fn an_interface_that_is_not_a_direct_superinterface_is_flagged() {
        let bad = format!("{IFACES} class C implements Mid {{ public void hello() {{ Root.super.hello(); }} }}");
        assert_eq!(messages(&bad).len(), 1, "{:?}", messages(&bad));
        let direct = format!("{IFACES} class C implements Root {{ public void hello() {{ Root.super.hello(); }} }}");
        assert!(messages(&direct).is_empty());
    }

    #[test]
    fn a_default_overridden_in_another_direct_superinterface_is_flagged() {
        let bad = format!("{IFACES} class C implements Root, Over {{ public void hello() {{ Root.super.hello(); }} }}");
        assert_eq!(messages(&bad).len(), 1, "{:?}", messages(&bad));
        let fine = format!("{IFACES} class C implements Root, Mid {{ public void hello() {{ Root.super.hello(); }} }}");
        assert!(messages(&fine).is_empty());
    }

    #[test]
    fn an_enclosing_class_qualifier_and_an_anonymous_class_are_left_alone() {
        assert!(messages("class A { void m() { } class In { void f() { A.super.toString(); } } }").is_empty());
        let anonymous = format!("{IFACES} class C {{ void f() {{ new Mid() {{ public void hello() {{ Root.super.hello(); }} }}; }} }}");
        assert!(messages(&anonymous).is_empty());
    }
}
