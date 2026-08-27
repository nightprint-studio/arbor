//! Instance state read in the arguments of a `this(…)` or `super(…)` —
//! `compiler.err.cant.ref.before.ctor.called`.
//!
//! Until the delegated constructor returns, the object does not exist: its fields hold their default
//! values and no method of it may run. So `this(count)` reads a zero and `this(computeSize())` calls
//! a method on a half-built object — and Java refuses both rather than let the value be silently
//! wrong.
//!
//! It is easy to write, because the line reads exactly like the working version one scope out:
//! `A(int n) { this.count = n; }` beside `A() { this(count); }`, where the second means the default
//! and not the first's assignment.
//!
//! ## Only what this class itself declares
//!
//! An inherited field would need the resolver to find, and a bare name in a constructor can bind to
//! a parameter, a local, a static, an inherited field or a static import. So the check flags only
//! what it can see in the enclosing class body and prove is an instance member:
//!   * a bare name (or `this.name`) that this class declares as a **non-static field**, and that no
//!     parameter or local of the constructor shadows;
//!   * a bare call to a method this class declares as **non-static**;
//!   * `this` itself, written as an argument.
//!
//! Everything else is left alone. A static field or method is legal there — it is exactly how a
//! constructor is supposed to compute an argument for its own delegation.

use std::collections::HashSet;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check_id::CheckId;
use crate::nodes::has_keyword;
use crate::scopes::resolves_as_local;

/// Every instance reference in a constructor-delegation argument list.
pub fn ctor_before_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if matches!(n.kind(), "class_declaration" | "enum_declaration" | "record_declaration") {
            check_type(n, bytes, &mut out);
        }
    }
    out
}

fn check_type(type_node: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(body) = type_node.child_by_field_name("body") else { return };
    let (fields, methods) = instance_members(body, bytes);
    if fields.is_empty() && methods.is_empty() {
        // `this` as an argument is still illegal, so we do not return — but with nothing declared
        // there is also nothing else to find.
    }

    let mut bc = body.walk();
    for member in body.named_children(&mut bc) {
        if member.kind() != "constructor_declaration" {
            continue;
        }
        let Some(ctor_body) = member.child_by_field_name("body") else { continue };
        let mut cc = ctor_body.walk();
        for stmt in ctor_body.named_children(&mut cc) {
            if stmt.kind() != "explicit_constructor_invocation" {
                continue;
            }
            let Some(args) = stmt.child_by_field_name("arguments") else { continue };
            scan(args, type_node, bytes, &fields, &methods, out);
        }
    }
}

/// Walk the argument list for references to instance state.
fn scan(
    args: Node,
    type_node: Node,
    bytes: &[u8],
    fields: &HashSet<String>,
    methods: &HashSet<String>,
    out: &mut Vec<Diagnostic>,
) {
    let mut stack = vec![args];
    while let Some(n) = stack.pop() {
        // A lambda body runs after the constructor has finished — `this(() -> count)` is legal.
        if n.kind() == "lambda_expression" || n.kind() == "class_body" {
            continue;
        }
        match n.kind() {
            "this" => {
                out.push(CheckId::ReferenceBeforeConstructor.at(
                    n,
                    "`this` cannot be used before the other constructor has been called".to_string(),
                ));
                continue;
            }
            "field_access" => {
                // `Outer.this` — a QUALIFIED this, and it is not the object being constructed. It
                // names the lexically enclosing instance, which already exists, and JLS §8.8.7.1
                // allows it in an explicit constructor invocation precisely so an inner class can
                // hand its outer instance to `super(…)` — which is what guava's
                // `super(ForwardingMap.this)` does, twenty-five times over.
                //
                // The whole node is skipped rather than just not flagged: the `this` inside it would
                // otherwise be reached on the way down and reported on its own.
                if n.child_by_field_name("field").is_some_and(|f| f.kind() == "this") {
                    continue;
                }
                // `this.count` — the same read, written explicitly.
                let is_this = n
                    .child_by_field_name("object")
                    .is_some_and(|o| o.kind() == "this");
                if is_this {
                    if let Some(f) = n.child_by_field_name("field") {
                        if let Ok(name) = f.utf8_text(bytes) {
                            if fields.contains(name) {
                                out.push(flag_field(f, name));
                                continue;
                            }
                        }
                    }
                }
            }
            "method_invocation" => {
                // Only a BARE call — `helper()`, i.e. implicit `this.helper()`. A call on some other
                // receiver is that object's business.
                if n.child_by_field_name("object").is_none() {
                    if let Some(nm) = n.child_by_field_name("name") {
                        if let Ok(name) = nm.utf8_text(bytes) {
                            if methods.contains(name) {
                                out.push(CheckId::ReferenceBeforeConstructor.at(
                                    nm,
                                    format!(
                                        "`{name}()` runs on an object that does not exist yet — the \
                                         other constructor has not been called"
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
            "identifier" => {
                let Ok(name) = n.utf8_text(bytes) else { continue };
                // The `name` of a call, or the part after a `.`, is not a bare read.
                let bare = n.parent().is_none_or(|p| {
                    !matches!(p.kind(), "field_access" | "method_invocation" | "scoped_identifier")
                });
                if bare && fields.contains(name) && !resolves_as_local(n, type_node, bytes) {
                    out.push(flag_field(n, name));
                }
            }
            _ => {}
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
}

fn flag_field(node: Node, name: &str) -> Diagnostic {
    CheckId::ReferenceBeforeConstructor.at(
        node,
        format!(
            "`{name}` is still its default here — the other constructor has not been called yet"
        ),
    )
}

/// The non-static fields and methods this class body declares, by name.
fn instance_members(body: Node, bytes: &[u8]) -> (HashSet<String>, HashSet<String>) {
    let (mut fields, mut methods) = (HashSet::new(), HashSet::new());
    let mut c = body.walk();
    for member in body.named_children(&mut c) {
        if has_keyword(member, bytes, "static") {
            continue;
        }
        match member.kind() {
            "field_declaration" => {
                let mut fc = member.walk();
                for d in member.named_children(&mut fc) {
                    if d.kind() == "variable_declarator" {
                        if let Some(nm) = d.child_by_field_name("name") {
                            if let Ok(t) = nm.utf8_text(bytes) {
                                fields.insert(t.to_string());
                            }
                        }
                    }
                }
            }
            "method_declaration" => {
                if let Some(nm) = member.child_by_field_name("name") {
                    if let Ok(t) = nm.utf8_text(bytes) {
                        methods.insert(t.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    (fields, methods)
}

#[cfg(test)]
mod tests {
    use crate::check::collect_nodes;

    fn codes(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).expect("parse");
        let nodes = collect_nodes(tree.root_node());
        super::ctor_before_errors_nodes(&nodes, src).into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn an_instance_field_in_a_this_call_is_flagged() {
        let src = "class A { int count; A() { this(count); } A(int n) {} }";
        assert_eq!(codes(src), ["reference-before-constructor"]);
    }

    #[test]
    fn the_same_field_written_with_this_is_flagged() {
        let src = "class A { int count; A() { this(this.count); } A(int n) {} }";
        assert_eq!(codes(src), ["reference-before-constructor"]);
    }

    #[test]
    fn an_instance_method_call_is_flagged() {
        let src = "class A { int size() { return 1; } A() { this(size()); } A(int n) {} }";
        assert_eq!(codes(src), ["reference-before-constructor"]);
    }

    #[test]
    fn this_itself_as_an_argument_is_flagged() {
        let src = "class A { A() { this(this); } A(Object o) {} }";
        assert_eq!(codes(src), ["reference-before-constructor"]);
    }

    #[test]
    fn it_applies_to_a_super_call_too() {
        let src = "class A extends B { int count; A() { super(count); } }";
        assert_eq!(codes(src), ["reference-before-constructor"]);
    }

    /// A static member is exactly how a constructor is meant to compute its delegation argument.
    #[test]
    fn a_static_field_or_method_is_legal_there() {
        assert!(codes("class A { static int D = 1; A() { this(D); } A(int n) {} }").is_empty());
        assert!(codes("class A { static int d() { return 1; } A() { this(d()); } A(int n) {} }").is_empty());
    }

    /// A parameter shadowing the field name is the parameter, not the field.
    #[test]
    fn a_parameter_of_the_same_name_shadows_the_field() {
        let src = "class A { int count; A(int count) { this(count); } A(int a, int b) {} }";
        assert!(codes(src).is_empty());
    }

    /// A lambda body runs long after the constructor returns.
    #[test]
    fn a_read_deferred_into_a_lambda_is_legal() {
        let src = "class A { int count; A() { this(() -> count); } A(Object o) {} }";
        assert!(codes(src).is_empty());
    }

    /// A call on another object is that object's business, not this half-built one's.
    #[test]
    fn a_call_on_another_receiver_is_left_alone() {
        let src = "class A { int size() { return 1; } A(B b) { this(b.size()); } A(int n) {} }";
        assert!(codes(src).is_empty());
    }

    /// What guava writes, and Java allows: an inner class handing its enclosing instance to
    /// `super(…)`. The enclosing instance already exists — it is the object under construction that
    /// does not.
    #[test]
    fn a_qualified_this_names_the_enclosing_instance_and_is_legal() {
        let src = "class Outer { class Inner extends B { Inner() { super(Outer.this); } } }";
        assert!(codes(src).is_empty());
    }

    /// The unqualified one in the same position is still the object being built.
    #[test]
    fn a_bare_this_beside_it_is_still_flagged() {
        let src = "class Outer { class Inner extends B { Inner() { super(this); } } }";
        assert_eq!(codes(src), ["reference-before-constructor"]);
    }

    /// A field this class does not declare could be inherited, static, or anything — not judged.
    #[test]
    fn a_name_this_class_does_not_declare_is_left_alone() {
        assert!(codes("class A { A() { this(whatever); } A(int n) {} }").is_empty());
    }

    /// Only the delegation's arguments. The body after it is a fully built object.
    #[test]
    fn the_constructor_body_after_the_delegation_is_fine() {
        let src = "class A { int count; A() { this(1); count = count + 1; } A(int n) {} }";
        assert!(codes(src).is_empty());
    }
}
