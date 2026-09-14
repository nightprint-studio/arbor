//! Whether an argument is a plain value — certainly, not probably.
//!
//! Mockito does not look at the arguments of a stubbed call. It counts the matchers **registered**
//! while they were evaluated, and compares the count with the number of arguments. So what matters is
//! not how an argument looks but whether evaluating it can register a matcher, and plenty of things
//! that look plain can:
//!
//! - `orderWith(id)` — a project helper returning `argThat(…)`;
//! - `captor.capture()` — an instance call that is a matcher;
//! - `v`, when `v` is a parameter and the caller passed `any()`, or a local assigned from a call.
//!
//! A value is judged plain only when none of that is possible: no method call anywhere in it, and no
//! name that could be carrying the result of one.

use bennu_java::prelude::{annotations_of, node_text, simple_name};
use tree_sitter::Node;

use crate::file::{assigned_identifier, contains_invocation, descendants, JavaFile};

/// A test method is called by the engine, never with a matcher — so its parameters are values.
const TEST_METHODS: &[&str] =
    &["Test", "ParameterizedTest", "RepeatedTest", "TestFactory", "TestTemplate"];

pub(crate) fn is_plain_value(file: &JavaFile<'_>, argument: Node<'_>) -> bool {
    !contains_invocation(argument) && !reads_uncertain_name(file.source, argument)
}

/// Whether `argument` reads a name that might hold a registered matcher: a parameter of a helper
/// method, a lambda parameter, or a local assigned from a call.
fn reads_uncertain_name<'t>(source: &str, argument: Node<'t>) -> bool {
    let names: Vec<&str> = descendants(argument)
        .into_iter()
        .filter(|n| n.kind() == "identifier")
        .map(|n| node_text(&n, source))
        .collect();
    if names.is_empty() {
        return false;
    }
    // Outside a method body there is nothing to reason about with confidence.
    let Some(scope) = enclosing_executable(argument) else { return true };
    let is_test = is_test_method(scope, source);
    let named = |node: Option<Node<'t>>| node.is_some_and(|n| names.contains(&node_text(&n, source)));
    descendants(scope).into_iter().any(|n| match n.kind() {
        "formal_parameters" => !is_test && n.parent() == Some(scope) && binds_any(n, &names, source),
        "lambda_expression" => {
            n.child_by_field_name("parameters").is_some_and(|p| binds_any(p, &names, source))
        }
        "variable_declarator" => {
            named(n.child_by_field_name("name"))
                && n.child_by_field_name("value").is_some_and(contains_invocation)
        }
        "assignment_expression" => {
            named(n.child_by_field_name("left").and_then(assigned_identifier))
                && n.child_by_field_name("right").is_some_and(contains_invocation)
        }
        _ => false,
    })
}

fn binds_any(node: Node<'_>, names: &[&str], source: &str) -> bool {
    descendants(node)
        .iter()
        .any(|n| n.kind() == "identifier" && names.contains(&node_text(n, source)))
}

fn is_test_method(scope: Node<'_>, source: &str) -> bool {
    scope.kind() == "method_declaration"
        && annotations_of(scope, source)
            .iter()
            .any(|(name, _)| TEST_METHODS.contains(&simple_name(name)))
}

/// The method, constructor or initializer `node` is evaluated in — when it is a member of a named
/// type. In an anonymous or local class the enclosing method's locals are captured from further out,
/// which this does not follow; `None` there makes the caller unsure.
fn enclosing_executable(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node.parent();
    while let Some(n) = current {
        match n.kind() {
            "method_declaration" | "constructor_declaration" | "compact_constructor_declaration"
            | "static_initializer" => return is_member_of_named_type(n).then_some(n),
            "block" if n.parent().is_some_and(|p| p.kind() == "class_body") => {
                return is_member_of_named_type(n).then_some(n)
            }
            "class_body" | "interface_body" | "enum_body" | "program" => return None,
            _ => current = n.parent(),
        }
    }
    None
}

fn is_member_of_named_type(member: Node<'_>) -> bool {
    let Some(owner) = member.parent().and_then(|body| body.parent()) else { return false };
    matches!(owner.kind(), "class_declaration" | "record_declaration")
        && owner.parent().is_some_and(|p| matches!(p.kind(), "program" | "class_body"))
}
