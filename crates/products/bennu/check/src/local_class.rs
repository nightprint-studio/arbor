//! A capturing local class instantiated from a `static` member of itself —
//! `compiler.err.local.cant.be.inst.static`.
//!
//! A local class that reads a variable of the method around it carries a hidden reference to that
//! variable. A `static` member of the same class has no such reference — there is no enclosing
//! instance to take it from — so `new Local()` written there cannot supply what the constructor
//! needs, and Java refuses it:
//!
//! ```java
//! static void foo(Object there) {
//!     class Local {
//!         { there.hashCode(); }          // captures `there`
//!         static { new Local(); }        // ← no `there` to get it from
//!     }
//! }
//! ```
//!
//! **Java 16 and up only.** A local class could not have `static` members before that, so the shape
//! cannot occur in older code at all — which is also why the check costs nothing there: no `static`
//! member inside a local class means no walk.
//!
//! Capture is decided textually and conservatively: the class body mentions a bare name that the
//! enclosing method declares and the class itself does not. Over-detecting capture would report a
//! legal instantiation, so a name the class re-declares (a field, a parameter of its own methods)
//! counts as its own and not as captured.

use std::collections::HashSet;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check_id::CheckId;
use crate::nodes::{child_field_name, has_keyword, text};
use crate::scopes::resolves_as_local;

/// Every instantiation of a capturing local class from one of its own `static` members.
pub fn local_class_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        // A LOCAL class is one declared inside a block — a method body, a lambda body, an
        // initializer. A member type's `static` members have no enclosing instance either, but they
        // capture nothing, so there is nothing for them to fail to supply.
        if !matches!(n.kind(), "class_declaration" | "record_declaration") {
            continue;
        }
        if n.parent().is_none_or(|p| p.kind() != "block") {
            continue;
        }
        check_local(n, bytes, &mut out);
    }
    out
}

fn check_local(local: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(body) = local.child_by_field_name("body") else { return };
    let Some(name) = local.child_by_field_name("name").and_then(|n| text(n, bytes)) else {
        return;
    };

    // Cheapest gate first, and the one that makes this free on pre-16 code: no `static` member, no
    // question. Only then is capture worth working out.
    let statics: Vec<Node> = static_members(body, bytes);
    if statics.is_empty() {
        return;
    }
    if !captures_enclosing(local, body, bytes) {
        return;
    }

    for member in statics {
        let mut stack = vec![member];
        while let Some(n) = stack.pop() {
            if n.kind() == "object_creation_expression" {
                let created = n.child_by_field_name("type").and_then(|t| text(t, bytes));
                if created.as_deref() == Some(name.as_str()) {
                    out.push(CheckId::LocalClassFromStatic.at(
                        n,
                        format!(
                            "`{name}` captures a variable of the enclosing method, so it cannot be \
                             created from a `static` member — there is no enclosing instance here"
                        ),
                    ));
                }
            }
            let mut c = n.walk();
            stack.extend(n.named_children(&mut c));
        }
    }
}

/// The `static` members of a local class body — a `static` initializer, or any `static` declaration.
fn static_members<'t>(body: Node<'t>, bytes: &[u8]) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    let mut c = body.walk();
    for m in body.named_children(&mut c) {
        if m.kind() == "static_initializer" || has_keyword(m, bytes, "static") {
            out.push(m);
        }
    }
    out
}

/// Whether the class body reads a name some enclosing SCOPE binds and the class itself does not.
///
/// The binding is almost never in the immediately enclosing block — it is a parameter of the method
/// around it, one level further up — so this asks the shared scope walk rather than one node.
/// Bounded by the enclosing TYPE, past which only fields apply and a field is not a capture.
///
/// Textual and deliberately narrow: capture is what makes the instantiation illegal, so
/// over-detecting it would report code that compiles. A name the class re-declares is its own.
fn captures_enclosing(local: Node, body: Node, bytes: &[u8]) -> bool {
    let Some(top) = enclosing_type(local) else { return false };
    let mut own: HashSet<String> = HashSet::new();
    collect_own_names(body, bytes, &mut own);

    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        if n.kind() == "identifier" {
            // A READ of the name: either it stands alone, or it is the RECEIVER of a member access.
            // `there` in `there.hashCode()` is a read of `there` — excluding everything under a
            // `method_invocation`, as a first cut did, threw away exactly the capture being looked
            // for. What is not a read is the `name` of the call and the `field` after the dot.
            let read = match n.parent() {
                None => true,
                Some(p) if matches!(p.kind(), "field_access" | "method_invocation") => {
                    child_field_name(p, n).as_deref() == Some("object")
                }
                Some(p) if p.kind() == "scoped_identifier" => false,
                Some(_) => true,
            };
            if read {
                if let Ok(name) = n.utf8_text(bytes) {
                    if !own.contains(name) && resolves_as_local(n, top, bytes) {
                        return true;
                    }
                }
            }
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    false
}

/// The type declaration around `n` — where the scope walk stops.
fn enclosing_type(n: Node) -> Option<Node> {
    let mut cur = n.parent();
    while let Some(p) = cur {
        if matches!(
            p.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            return Some(p);
        }
        cur = p.parent();
    }
    None
}

/// Every name the class body itself binds — its fields, and the parameters and locals of its
/// members. Anything here is the class's own, not something captured.
fn collect_own_names(body: Node, bytes: &[u8], out: &mut HashSet<String>) {
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        if matches!(n.kind(), "variable_declarator" | "formal_parameter" | "spread_parameter") {
            if let Some(nm) = n.child_by_field_name("name") {
                if let Ok(t) = nm.utf8_text(bytes) {
                    out.insert(t.to_string());
                }
            }
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
}

#[cfg(test)]
mod tests {
    use crate::check::collect_nodes;

    fn codes(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).expect("parse");
        let nodes = collect_nodes(tree.root_node());
        super::local_class_errors_nodes(&nodes, src).into_iter().map(|d| d.code).collect()
    }

    /// The JDK's own shape, from `LocalFreeVarStaticInstantiate`.
    #[test]
    fn a_capturing_local_class_created_from_its_static_block_is_flagged() {
        let src = "class A { static void foo(Object there) { class Local { { there.hashCode(); } \
                   static { new Local(); } } } }";
        assert_eq!(codes(src), ["local-class-from-static"]);
    }

    #[test]
    fn the_same_from_a_static_field_initializer_is_flagged() {
        let src = "class A { static void foo(Object there) { class Local { { there.hashCode(); } \
                   static Object o = new Local(); } } }";
        assert_eq!(codes(src), ["local-class-from-static"]);
    }

    /// No capture, no hidden reference to supply — legal.
    #[test]
    fn a_local_class_that_captures_nothing_is_fine() {
        let src = "class A { static void foo(Object there) { class Local { int n; \
                   static { new Local(); } } } }";
        assert!(codes(src).is_empty());
    }

    /// From an INSTANCE member there is an enclosing instance to take the capture from.
    #[test]
    fn creating_it_from_an_instance_member_is_fine() {
        let src = "class A { static void foo(Object there) { class Local { { there.hashCode(); } \
                   void go() { new Local(); } } } }";
        assert!(codes(src).is_empty());
    }

    /// A name the class re-declares is its own, not something captured from the method.
    #[test]
    fn a_name_the_class_redeclares_is_not_a_capture() {
        let src = "class A { static void foo(Object there) { class Local { Object there; \
                   { there.hashCode(); } static { new Local(); } } } }";
        assert!(codes(src).is_empty());
    }

    /// A member type is not a local class: its `static` members capture nothing.
    #[test]
    fn a_member_class_is_not_a_local_class() {
        let src = "class A { Object there; class Inner { { there.hashCode(); } \
                   static { new Inner(); } } }";
        assert!(codes(src).is_empty());
    }

    /// Creating some OTHER type from the static member says nothing about this one.
    #[test]
    fn creating_a_different_type_is_left_alone() {
        let src = "class A { static void foo(Object there) { class Local { { there.hashCode(); } \
                   static { new Object(); } } } }";
        assert!(codes(src).is_empty());
    }
}
