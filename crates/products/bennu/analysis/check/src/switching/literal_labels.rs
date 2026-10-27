//! `case` labels that can never be constants (pure-AST).
//!
//! A `switch` takes integral, `char`, `String`, enum (and, from 21, pattern) labels, and every
//! non-pattern label must be a *constant expression*. Two shapes are wrong whatever the selector is:
//!
//!   * **a floating-point or boolean literal** — never a legal label: on an `int` selector it is a lossy
//!     conversion, and on a `double` or `boolean` selector the selector itself is the error;
//!   * **the name of a local variable or parameter that is not `final`** — a constant variable has to
//!     be `final`, so `case limit:` after `int limit = 3;` is `const.expr.req` (`string.const.req` for
//!     a `String` selector). Only a name the method declares exactly once, without `final`, and that is
//!     not also the name of an enum constant in the file (an enum `switch` reads a bare label as one)
//!     is judged; a constant-style name (no lowercase letter) is left alone for the same reason.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::nodes::has_keyword;
use crate::support::scopes::{enclosing_executable_scope, params_declare};

const NEVER_A_LABEL: [&str; 4] = ["decimal_floating_point_literal", "hex_floating_point_literal", "true", "false"];

/// All floating-point / boolean / non-constant-name `case` labels over the shared pre-collected node list.
pub fn literal_label_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes.iter().filter(|n| n.kind() == "switch_label") {
        let mut c = n.walk();
        for label in n.named_children(&mut c) {
            if is_never_a_label(label) {
                out.push(CheckId::IncompatibleCaseLabel.at(
                    label,
                    "A `case` label cannot be a floating-point or boolean constant — a `switch` takes integral, `char`, `String` or enum labels",
                ));
            } else if label.kind() == "identifier" && names_a_non_final_variable(label, nodes, bytes) {
                let name = label.utf8_text(bytes).unwrap_or_default();
                out.push(CheckId::IncompatibleCaseLabel.at(
                    label,
                    format!("`case {name}` is not a constant — a variable used as a `case` label has to be `final` and initialised with a constant"),
                ));
            }
        }
    }
    out
}

/// A bare literal, or one behind a sign (`-1.5`).
fn is_never_a_label(label: Node) -> bool {
    if NEVER_A_LABEL.contains(&label.kind()) {
        return true;
    }
    label.kind() == "unary_expression"
        && label.child_by_field_name("operand").is_some_and(|operand| NEVER_A_LABEL.contains(&operand.kind()))
}

/// Whether the label is the one declaration the enclosing method has of that name, and it is not `final`.
fn names_a_non_final_variable(label: Node, nodes: &[Node], bytes: &[u8]) -> bool {
    let Ok(name) = label.utf8_text(bytes) else { return false };
    if !name.chars().any(|ch| ch.is_lowercase()) || is_an_enum_constant(name, nodes, bytes) {
        return false;
    }
    let Some(method) = enclosing_executable_scope(label) else { return false };
    let mut declarations = parameter_declarations(method, name, bytes);
    collect_local_declarations(method, name, bytes, &mut declarations);
    match declarations.as_slice() {
        [only] => is_in_scope_at(*only, label) && !has_keyword(*only, bytes, "final"),
        _ => false,
    }
}

/// Whether a declaration is visible at `label`: a parameter always is; a local only when it comes
/// first and the block (or loop) it was declared in encloses the label. A same-named field would be the
/// meaning of the label otherwise.
fn is_in_scope_at(declaration: Node, label: Node) -> bool {
    if declaration.kind() == "formal_parameter" {
        return true;
    }
    if declaration.start_byte() >= label.start_byte() {
        return false;
    }
    let Some(scope) = declaration.parent() else { return false };
    let mut cur = label.parent();
    while let Some(p) = cur {
        if p.id() == scope.id() {
            return true;
        }
        cur = p.parent();
    }
    false
}

fn is_an_enum_constant(name: &str, nodes: &[Node], bytes: &[u8]) -> bool {
    nodes.iter().any(|n| {
        n.kind() == "enum_constant" && n.child_by_field_name("name").and_then(|id| id.utf8_text(bytes).ok()) == Some(name)
    })
}

/// The `formal_parameter`s of `method` called `name`.
fn parameter_declarations<'t>(method: Node<'t>, name: &str, bytes: &[u8]) -> Vec<Node<'t>> {
    let Some(params) = method.child_by_field_name("parameters") else { return Vec::new() };
    if !params_declare(method, name, bytes) {
        return Vec::new();
    }
    let mut c = params.walk();
    let found = params
        .named_children(&mut c)
        .filter(|p| p.kind() == "formal_parameter" && p.child_by_field_name("name").and_then(|id| id.utf8_text(bytes).ok()) == Some(name))
        .collect();
    found
}

/// Every `local_variable_declaration` under `root` that declares `name`.
fn collect_local_declarations<'t>(root: Node<'t>, name: &str, bytes: &[u8], out: &mut Vec<Node<'t>>) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "local_variable_declaration" {
            let mut c = n.walk();
            let declares = n.named_children(&mut c).any(|d| {
                d.kind() == "variable_declarator"
                    && d.child_by_field_name("name").and_then(|id| id.utf8_text(bytes).ok()) == Some(name)
            });
            if declares {
                out.push(n);
            }
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(src: &str) -> usize {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        literal_label_errors_nodes(&nodes, src).len()
    }

    #[test]
    fn floating_point_and_boolean_labels_are_flagged() {
        assert_eq!(count("class C { void m(int v) { switch (v) { case 1.5: break; } } }"), 1);
        assert_eq!(count("class C { void m(int v) { switch (v) { case -2.5f: break; } } }"), 1);
        assert_eq!(count("class C { void m(boolean v) { switch (v) { case true: break; } } }"), 1);
    }

    #[test]
    fn integral_string_char_and_enum_labels_are_not() {
        assert_eq!(count("class C { void m(int v) { switch (v) { case 1: case 'a': case -3: break; } } }"), 0);
        assert_eq!(count("class C { void m(String v) { switch (v) { case \"a\": break; } } }"), 0);
        assert_eq!(count("class C { void m(E v) { switch (v) { case RED: break; } } }"), 0);
    }

    #[test]
    fn a_non_final_variable_as_a_label_is_flagged() {
        assert_eq!(count("class C { void m(int v, int other) { switch (v) { case other: break; } } }"), 1);
        assert_eq!(count("class C { void m(int v) { int limit = 3; switch (v) { case limit: break; } } }"), 1);
    }

    #[test]
    fn a_final_variable_a_constant_style_name_or_an_enum_constant_is_not() {
        assert_eq!(count("class C { void m(int v) { final int limit = 3; switch (v) { case limit: break; } } }"), 0);
        assert_eq!(count("class C { static final int LIMIT = 3; void m(int v) { switch (v) { case LIMIT: break; } } }"), 0);
        assert_eq!(count("enum E { open } class C { void m(E v) { switch (v) { case open: break; } } }"), 0);
        // A field is not judged: it may be `static final` with a constant initialiser.
        assert_eq!(count("class C { static final int limit = 3; void m(int v) { switch (v) { case limit: break; } } }"), 0);
    }
}
