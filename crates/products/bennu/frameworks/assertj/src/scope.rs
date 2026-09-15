//! Which declaration a name refers to — as far as can be said with certainty.
//!
//! Two questions need it. *Is `softly` a soft-assertions object?* and *is `names` a `List`?* — the
//! second decides whether `assertThat(names.isEmpty()).isTrue()` can become `assertThat(names)
//! .isEmpty()`, which only compiles when `names` really is something AssertJ has an `isEmpty` for.
//!
//! This is not a resolver and does not try to be one. It finds the declarations of a name inside the
//! enclosing method, then the fields of the enclosing class, and answers only when exactly one is in
//! play. A lambda parameter, a pattern binding, a local class, an inherited field — anything that
//! makes the answer depend on scope rules — ends in `None`, and the caller says nothing.

use bennu_java::prelude::node_text;
use tree_sitter::Node;

use crate::syntax::{ancestors, children, descendants, encloses};
use crate::unit::Unit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeclKind {
    Local,
    Field,
    /// A method, constructor or lambda parameter, or an enhanced-for variable.
    Param,
    /// A try-with-resources resource.
    Resource,
    /// A name bound with no type written: a lambda parameter, a pattern, a catch parameter.
    Untyped,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Decl<'t> {
    pub kind: DeclKind,
    pub name: Node<'t>,
    /// The written type, when there is one (`var` included — the caller decides what that means).
    pub ty: Option<Node<'t>>,
    /// The initialiser, when there is one.
    pub value: Option<Node<'t>>,
    /// Array dimensions written on the NAME (`int ids[]`), which the type node does not carry.
    pub dims: usize,
    /// The node carrying the declaration's modifiers — where its annotations are.
    pub owner: Node<'t>,
    /// The subtree the name is visible in; `None` where it is not worked out.
    pub scope: Option<Node<'t>>,
}

/// Every declaration of `name` under `root`.
pub(crate) fn declarations<'t>(root: Node<'t>, name: &str, source: &str) -> Vec<Decl<'t>> {
    let mut out = Vec::new();
    for node in descendants(root) {
        declared_by(node, source, &mut out);
    }
    out.retain(|d| node_text(&d.name, source) == name);
    out
}

/// The one declaration `name` can refer to at `at`: a typed local, parameter or resource of the
/// enclosing method, or — when the method declares nothing of that name — a field of its class.
pub(crate) fn visible_declaration<'t>(unit: &Unit<'_>, name: &str, at: Node<'t>) -> Option<Decl<'t>> {
    let method = ancestors(at)
        .find(|n| matches!(n.kind(), "method_declaration" | "constructor_declaration"))?;
    let inside = declarations(method, name, unit.source);
    match inside.as_slice() {
        [] => {}
        [only] => {
            let usable = matches!(only.kind, DeclKind::Local | DeclKind::Param | DeclKind::Resource)
                && only.ty.is_some()
                && in_scope(only, at);
            return usable.then_some(*only);
        }
        _ => return None,
    }
    let body = method.parent().filter(|b| b.kind() == "class_body")?;
    let fields: Vec<Decl<'t>> = children(body)
        .into_iter()
        .filter(|c| c.kind() == "field_declaration")
        .flat_map(|f| declarations(f, name, unit.source))
        .collect();
    match fields.as_slice() {
        [only] if only.ty.is_some() => Some(*only),
        _ => None,
    }
}

fn in_scope(decl: &Decl<'_>, at: Node<'_>) -> bool {
    decl.scope.is_some_and(|s| encloses(s, at))
        && (decl.kind != DeclKind::Local || decl.name.end_byte() <= at.start_byte())
}

fn declared_by<'t>(node: Node<'t>, source: &str, out: &mut Vec<Decl<'t>>) {
    match node.kind() {
        "variable_declarator" => out.extend(declarator(node, source)),
        "formal_parameter" => {
            let scope = node.parent().and_then(|p| p.parent());
            out.extend(typed(node, DeclKind::Param, scope, source));
        }
        "enhanced_for_statement" => out.extend(typed(node, DeclKind::Param, Some(node), source)),
        "resource" => {
            let scope = node.parent().and_then(|p| p.parent());
            out.extend(typed(node, DeclKind::Resource, scope, source));
        }
        "catch_formal_parameter" | "instanceof_expression" => {
            out.extend(node.child_by_field_name("name").map(|n| untyped(n, node)));
        }
        "type_pattern" | "record_pattern_component" => out.extend(
            children(node).into_iter().filter(|c| c.kind() == "identifier").map(|n| untyped(n, node)),
        ),
        "lambda_expression" => {
            let Some(params) = node.child_by_field_name("parameters") else { return };
            match params.kind() {
                "identifier" => out.push(untyped(params, node)),
                "inferred_parameters" => out.extend(
                    children(params).into_iter().filter(|c| c.kind() == "identifier").map(|n| untyped(n, node)),
                ),
                _ => {}
            }
        }
        _ => {}
    }
}

fn declarator<'t>(node: Node<'t>, source: &str) -> Option<Decl<'t>> {
    let name = node.child_by_field_name("name")?;
    let owner = node.parent()?;
    let (kind, scope) = match owner.kind() {
        "local_variable_declaration" => (DeclKind::Local, owner.parent()),
        "field_declaration" | "constant_declaration" => (DeclKind::Field, None),
        _ => return Some(untyped(name, owner)),
    };
    Some(Decl {
        kind,
        name,
        ty: owner.child_by_field_name("type"),
        value: node.child_by_field_name("value"),
        dims: dimensions(node, source),
        owner,
        scope,
    })
}

fn typed<'t>(node: Node<'t>, kind: DeclKind, scope: Option<Node<'t>>, source: &str) -> Option<Decl<'t>> {
    Some(Decl {
        kind,
        name: node.child_by_field_name("name")?,
        ty: node.child_by_field_name("type"),
        value: node.child_by_field_name("value"),
        dims: dimensions(node, source),
        owner: node,
        scope,
    })
}

fn untyped<'t>(name: Node<'t>, owner: Node<'t>) -> Decl<'t> {
    Decl { kind: DeclKind::Untyped, name, ty: None, value: None, dims: 0, owner, scope: None }
}

fn dimensions(node: Node<'_>, source: &str) -> usize {
    node.child_by_field_name("dimensions").map_or(0, |d| node_text(&d, source).matches('[').count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::descendants;

    fn lookup(src: &str, name: &str, marker: &str) -> Option<String> {
        let unit = Unit::parse("/p/T.java", src).unwrap();
        let at = marker_node(&unit, marker);
        visible_declaration(&unit, name, at).and_then(|d| d.ty).map(|t| unit.compact(t))
    }

    /// The innermost node that starts at the marker — the use being asked about.
    fn marker_node<'t>(unit: &'t Unit<'_>, marker: &str) -> Node<'t> {
        let at = unit.source.find(marker).unwrap();
        descendants(unit.root()).into_iter().filter(|n| n.start_byte() == at).last().unwrap()
    }

    #[test]
    fn a_local_a_parameter_and_a_field_are_each_found() {
        let src = "class T { java.util.List<String> names; void m(String code) { int n = 1; use(n, code, names); } }";
        assert_eq!(lookup(src, "n", "n, code").as_deref(), Some("int"));
        assert_eq!(lookup(src, "code", "code, names").as_deref(), Some("String"));
        assert_eq!(lookup(src, "names", "names);").as_deref(), Some("java.util.List<String>"));
    }

    /// A local shadows a field, and a name the method binds without a type says nothing at all.
    #[test]
    fn anything_that_depends_on_scope_rules_is_not_answered() {
        let shadowed = "class T { String x; void m() { int x = 1; use(x); } }";
        assert_eq!(lookup(shadowed, "x", "x);").as_deref(), Some("int"));
        let lambda = "class T { String x; void m() { run(x -> use(x)); } }";
        assert_eq!(lookup(lambda, "x", "x);"), None);
        let later = "class T { void m() { use(x); int x = 1; } }";
        assert_eq!(lookup(later, "x", "x);"), None, "declared after the use");
    }
}
