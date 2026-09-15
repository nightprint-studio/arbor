//! The boolean that throws away what it was computed from.
//!
//! ```java
//! assertThat(names.isEmpty()).isTrue();   // fails with: expected true but was false
//! assertThat(names).isEmpty();            // fails with: expected empty but was ["ada", "grace"]
//! ```
//!
//! Both check the same thing. Only one of them tells you, from the CI log, what went wrong — the other
//! sends you to a debugger to find out what `names` held. The rewrite evaluates exactly the same
//! expressions, once each, so it is safe whatever they do.
//!
//! The ones that depend on a type — `isEmpty`, `hasSize`, `contains` — are offered only when that type
//! is **written** in the file (a local, a parameter, a field) and is one AssertJ has the assertion
//! for. `assertThat(order.isEmpty()).isTrue()` on a class of the project's own has no `isEmpty` to
//! become, and an edit that stops the file compiling is worse than no edit.

use bennu_ext::prelude::{ExtEdit, ExtIntention, ExtProblem};
use bennu_proto::prelude::{severity, Diagnostic};
use tree_sitter::Node;

use crate::chain::{arguments, call_name, chain_of, statement_call};
use crate::ext::{CODE_DEDICATED, INTENTION_DEDICATED};
use crate::resolve::{assertj_entry, Entry};
use crate::scope::visible_declaration;
use crate::shape::{shape_of, Shape};
use crate::syntax::{descendants, has_comment, span, unparen};
use crate::unit::Unit;

struct Dedicated {
    statement: (usize, usize),
    /// The `assertThat` argument — what the squiggle covers.
    squiggle: (usize, usize),
    /// The whole chain — what the rewrite replaces.
    replace: (usize, usize),
    text: String,
    was: String,
    check: &'static str,
    label: String,
}

/// The parts of the rewritten chain: `assertThat(subject).check(argument)`.
struct Parts<'s> {
    subject: &'s str,
    check: &'static str,
    argument: String,
}

pub(crate) fn diagnostics(unit: &Unit<'_>) -> Vec<Diagnostic> {
    find(unit)
        .into_iter()
        .map(|d| {
            let computed = if d.was == "isEqualTo" { "number" } else { "boolean" };
            Diagnostic {
                message: format!(
                    "use `{}` — it shows the value under test when it fails, where `{}` can only \
                     show the {computed} computed from it",
                    d.check, d.was
                ),
                severity: severity::WEAK.to_string(),
                code: CODE_DEDICATED.to_string(),
                start: d.squiggle.0,
                end: d.squiggle.1,
            }
        })
        .collect()
}

/// Offered for a squiggle under the caret, and equally at any caret inside such a statement — the
/// finding is a style one, and a project that hides weak findings should still get the rewrite.
pub(crate) fn intentions(unit: &Unit<'_>, offset: usize, problems: &[ExtProblem]) -> Vec<ExtIntention> {
    find(unit)
        .into_iter()
        .filter(|d| {
            let reported =
                problems.iter().any(|p| p.code == CODE_DEDICATED && (p.start, p.end) == d.squiggle);
            reported || (d.statement.0 <= offset && offset <= d.statement.1)
        })
        .map(|d| ExtIntention {
            id: INTENTION_DEDICATED.to_string(),
            label: d.label,
            edits: vec![ExtEdit::replace(d.replace.0, d.replace.1, d.text)],
        })
        .collect()
}

fn find(unit: &Unit<'_>) -> Vec<Dedicated> {
    unit.nodes()
        .into_iter()
        .filter(|n| n.kind() == "expression_statement")
        .filter_map(|statement| rewrite(unit, statement))
        .collect()
}

/// `assertThat(EXPR).CHECK(…)` — exactly that, nothing chained before or after the check.
fn rewrite(unit: &Unit<'_>, statement: Node<'_>) -> Option<Dedicated> {
    let expr = statement_call(statement)?;
    let chain = chain_of(expr)?;
    let [link] = chain.links.as_slice() else { return None };
    let (root, link) = (chain.root, *link);
    if call_name(root, unit.source) != "assertThat" || assertj_entry(unit, root) != Some(Entry::Static) {
        return None;
    }
    let root_arguments = root.child_by_field_name("arguments")?;
    let link_arguments = link.child_by_field_name("arguments")?;
    if root.child_by_field_name("type_arguments").is_some()
        || link.child_by_field_name("type_arguments").is_some()
        || has_comment(root_arguments)
        || has_comment(link_arguments)
    {
        return None;
    }
    let written_arguments = arguments(root);
    let [written] = written_arguments.as_slice() else { return None };
    let actual = unparen(*written);
    let nested = descendants(actual)
        .iter()
        .any(|n| n.kind() == "method_invocation" && call_name(*n, unit.source).starts_with("assertThat"));
    if nested {
        return None;
    }
    let was = call_name(link, unit.source);
    let check_arguments = arguments(link);
    let parts = match (was, check_arguments.as_slice()) {
        ("isTrue", []) => boolean(unit, actual, true)?,
        ("isFalse", []) => boolean(unit, actual, false)?,
        ("isEqualTo", [expected]) => size(unit, actual, *expected)?,
        _ => return None,
    };
    let callee = unit.source[root.start_byte()..root_arguments.start_byte()].trim_end();
    let ellipsis = if parts.argument.is_empty() { "" } else { "…" };
    Some(Dedicated {
        statement: span(statement),
        squiggle: span(*written),
        replace: span(expr),
        text: format!("{callee}({}).{}({})", parts.subject, parts.check, parts.argument),
        was: was.to_string(),
        check: parts.check,
        label: format!("Replace with assertThat(…).{}({ellipsis})", parts.check),
    })
}

fn boolean<'s>(unit: &Unit<'s>, actual: Node<'_>, holds: bool) -> Option<Parts<'s>> {
    match actual.kind() {
        "method_invocation" => method_check(unit, actual, holds),
        "binary_expression" => null_check(unit, actual, holds),
        // Only the positive form: `isNotInstanceOf` fails on a null actual, where `!(x instanceof T)`
        // holds — the two are not the same assertion.
        "instanceof_expression" if holds => instance_check(unit, actual),
        _ => None,
    }
}

/// `x == null`, `null != x`, either polarity.
fn null_check<'s>(unit: &Unit<'s>, comparison: Node<'_>, holds: bool) -> Option<Parts<'s>> {
    let left = unparen(comparison.child_by_field_name("left")?);
    let right = unparen(comparison.child_by_field_name("right")?);
    let equal = match unit.text(comparison.child_by_field_name("operator")?) {
        "==" => true,
        "!=" => false,
        _ => return None,
    };
    let subject = match (left.kind(), right.kind()) {
        ("null_literal", "null_literal") => return None,
        (_, "null_literal") => left,
        ("null_literal", _) => right,
        _ => return None,
    };
    if !is_plain(subject) {
        return None;
    }
    let check = if equal == holds { "isNull" } else { "isNotNull" };
    Some(Parts { subject: unit.text(subject), check, argument: String::new() })
}

/// `x instanceof T` — not a pattern, whose binding the rewrite would delete.
fn instance_check<'s>(unit: &Unit<'s>, test: Node<'_>) -> Option<Parts<'s>> {
    if test.child_by_field_name("name").is_some() || test.child_by_field_name("pattern").is_some() {
        return None;
    }
    let mut cursor = test.walk();
    let unusual = test.children(&mut cursor).any(|c| c.kind() == "final" || c.kind().contains("pattern"));
    if unusual {
        return None;
    }
    let subject = unparen(test.child_by_field_name("left")?);
    let ty = test.child_by_field_name("right")?;
    if !is_plain(subject) || !matches!(ty.kind(), "type_identifier" | "scoped_type_identifier") {
        return None;
    }
    Some(Parts { subject: unit.text(subject), check: "isInstanceOf", argument: format!("{}.class", unit.text(ty)) })
}

fn method_check<'s>(unit: &Unit<'s>, call: Node<'_>, holds: bool) -> Option<Parts<'s>> {
    let receiver = call.child_by_field_name("object")?;
    if call.child_by_field_name("type_arguments").is_some() {
        return None;
    }
    let method = call_name(call, unit.source);
    let args = arguments(call);
    if method == "equals" {
        let [other] = args.as_slice() else { return None };
        // `equals` on an array is identity, AssertJ's `isEqualTo` on one compares the contents.
        if !is_plain(receiver) || shape_at(unit, receiver) == Some(Shape::Array) {
            return None;
        }
        let check = if holds { "isEqualTo" } else { "isNotEqualTo" };
        return Some(Parts { subject: unit.text(receiver), check, argument: unit.text(*other).to_string() });
    }
    let shape = shape_at(unit, receiver)?;
    let check = typed_check(unit, method, &args, &shape, holds, call)?;
    let argument = args.first().map(|a| unit.text(*a).to_string()).unwrap_or_default();
    Some(Parts { subject: unit.text(receiver), check, argument })
}

/// The checks that exist only on some assertion objects.
fn typed_check(
    unit: &Unit<'_>,
    method: &str,
    args: &[Node<'_>],
    shape: &Shape,
    holds: bool,
    at: Node<'_>,
) -> Option<&'static str> {
    let pick = |yes: &'static str, no: &'static str| Some(if holds { yes } else { no });
    match (method, args, shape) {
        ("isEmpty", [], Shape::Optional) => pick("isEmpty", "isPresent"),
        ("isEmpty", [], Shape::Str | Shape::TextLike | Shape::Collection { .. } | Shape::Map) => {
            pick("isEmpty", "isNotEmpty")
        }
        ("isPresent", [], Shape::Optional) => pick("isPresent", "isEmpty"),
        ("contains", [e], Shape::Str) if e.kind() != "null_literal" => pick("contains", "doesNotContain"),
        // `contains(ELEMENT...)` is typed where `Collection.contains(Object)` is not: offered only when
        // the element visibly has the collection's element type.
        ("contains", [e], Shape::Collection { element: Some(element) }) if fits(unit, *e, element, at) => {
            pick("contains", "doesNotContain")
        }
        ("startsWith", [p], Shape::Str) if holds && p.kind() != "null_literal" => Some("startsWith"),
        ("endsWith", [p], Shape::Str) if holds && p.kind() != "null_literal" => Some("endsWith"),
        _ => None,
    }
}

/// `assertThat(c.size()).isEqualTo(n)`, `s.length()`, `arr.length` → `hasSize(n)`.
fn size<'s>(unit: &Unit<'s>, actual: Node<'_>, expected: Node<'_>) -> Option<Parts<'s>> {
    if !is_int(unit, expected) {
        return None;
    }
    let (receiver, measured) = match actual.kind() {
        "method_invocation"
            if arguments(actual).is_empty() && actual.child_by_field_name("type_arguments").is_none() =>
        {
            (actual.child_by_field_name("object")?, call_name(actual, unit.source))
        }
        "field_access" => (actual.child_by_field_name("object")?, unit.text(actual.child_by_field_name("field")?)),
        _ => return None,
    };
    let shape = shape_at(unit, receiver)?;
    let measures = matches!(
        (actual.kind(), measured, &shape),
        ("method_invocation", "size", Shape::Collection { .. } | Shape::Map)
            | ("method_invocation", "length", Shape::Str | Shape::TextLike)
            | ("field_access", "length", Shape::Array)
    );
    measures.then(|| Parts { subject: unit.text(receiver), check: "hasSize", argument: unit.text(expected).to_string() })
}

/// The declared shape of a receiver written as a plain name.
fn shape_at(unit: &Unit<'_>, receiver: Node<'_>) -> Option<Shape> {
    if receiver.kind() != "identifier" {
        return None;
    }
    let decl = visible_declaration(unit, unit.text(receiver), receiver)?;
    Some(shape_of(unit, &decl))
}

/// `hasSize(int)` takes an `int`; `isEqualTo(2L)` compiled against `Object` and would not.
fn is_int(unit: &Unit<'_>, expected: Node<'_>) -> bool {
    match expected.kind() {
        "decimal_integer_literal" => !unit.text(expected).ends_with(['l', 'L']),
        "identifier" => visible_declaration(unit, unit.text(expected), expected)
            .is_some_and(|d| d.dims == 0 && d.ty.is_some_and(|t| unit.text(t) == "int")),
        _ => false,
    }
}

/// Whether `element` certainly has the type `expected`.
fn fits(unit: &Unit<'_>, element: Node<'_>, expected: &str, at: Node<'_>) -> bool {
    match element.kind() {
        "string_literal" => expected == "String",
        "character_literal" => expected == "Character",
        "true" | "false" => expected == "Boolean",
        "decimal_integer_literal" => expected == "Integer" && !unit.text(element).ends_with(['l', 'L']),
        "identifier" => visible_declaration(unit, unit.text(element), at)
            .is_some_and(|d| d.dims == 0 && d.ty.is_some_and(|t| unit.compact(t) == expected)),
        _ => false,
    }
}

/// A receiver the rewrite can move without changing what it means: a name, a field, a call chain.
fn is_plain(node: Node<'_>) -> bool {
    match node.kind() {
        "identifier" | "this" => true,
        "field_access" => node.child_by_field_name("object").is_some_and(is_plain),
        "method_invocation" => node.child_by_field_name("object").is_none_or(is_plain),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::ext::{CODE_DEDICATED as CODE, INTENTION_DEDICATED as REWRITE};
    use crate::testing::{apply, diagnostics, offered, squiggled};

    fn test_class(statement: &str) -> String {
        format!(
            "package com.acme;

import java.util.List;
import java.util.Map;
import java.util.Optional;

import static org.assertj.core.api.Assertions.assertThat;

class OrderTest {{
    private List<String> names;

    void check(String code, Order order, Order other, Optional<Order> found, int[] ids, Map<String, Integer> totals) {{
        {statement}
    }}
}}
"
        )
    }

    fn assert_rewrites(from: &str, to: &str) {
        let source = test_class(from);
        assert_eq!(squiggled(&source, CODE).len(), 1, "no finding for {from}");
        let offer = offered(&source, REWRITE, source.find(from).unwrap())
            .unwrap_or_else(|| panic!("no rewrite offered for {from}"));
        assert_eq!(apply(&source, &offer.edits), test_class(to));
    }

    fn assert_untouched(from: &str) {
        let source = test_class(from);
        assert!(squiggled(&source, CODE).is_empty(), "{from}");
        assert!(offered(&source, REWRITE, source.find(from).unwrap()).is_none(), "{from}");
    }

    #[test]
    fn equality_nullness_and_type_say_what_they_mean() {
        assert_rewrites("assertThat(order.equals(other)).isTrue();", "assertThat(order).isEqualTo(other);");
        assert_rewrites("assertThat(order.equals(other)).isFalse();", "assertThat(order).isNotEqualTo(other);");
        assert_rewrites("assertThat(order == null).isTrue();", "assertThat(order).isNull();");
        assert_rewrites("assertThat(null != order).isTrue();", "assertThat(order).isNotNull();");
        assert_rewrites("assertThat(order == null).isFalse();", "assertThat(order).isNotNull();");
        assert_rewrites("assertThat(order instanceof Order).isTrue();", "assertThat(order).isInstanceOf(Order.class);");
    }

    #[test]
    fn a_declared_collection_string_or_optional_gets_its_own_assertion() {
        assert_rewrites("assertThat(names.isEmpty()).isTrue();", "assertThat(names).isEmpty();");
        assert_rewrites("assertThat(totals.isEmpty()).isFalse();", "assertThat(totals).isNotEmpty();");
        assert_rewrites("assertThat(names.size()).isEqualTo(2);", "assertThat(names).hasSize(2);");
        assert_rewrites("assertThat(code.length()).isEqualTo(3);", "assertThat(code).hasSize(3);");
        assert_rewrites("assertThat(ids.length).isEqualTo(3);", "assertThat(ids).hasSize(3);");
        assert_rewrites("assertThat(names.contains(\"ada\")).isTrue();", "assertThat(names).contains(\"ada\");");
        assert_rewrites("assertThat(names.contains(\"ada\")).isFalse();", "assertThat(names).doesNotContain(\"ada\");");
        assert_rewrites("assertThat(code.startsWith(\"A\")).isTrue();", "assertThat(code).startsWith(\"A\");");
        assert_rewrites("assertThat(found.isPresent()).isTrue();", "assertThat(found).isPresent();");
        assert_rewrites("assertThat(found.isPresent()).isFalse();", "assertThat(found).isEmpty();");
    }

    #[test]
    fn the_finding_is_a_style_one() {
        let source = test_class("assertThat(names.isEmpty()).isTrue();");
        let found: Vec<_> = diagnostics(&source).into_iter().filter(|d| d.code == CODE).collect();
        assert_eq!(found[0].severity, "weak");
        assert_eq!(&source[found[0].start..found[0].end], "names.isEmpty()");
    }

    /// Every one of these either would not compile after the rewrite or would mean something else.
    #[test]
    fn anything_uncertain_is_left_as_written() {
        assert_untouched("assertThat(order.isEmpty()).isTrue();");
        assert_untouched("assertThat(names.size()).isEqualTo(2L);");
        assert_untouched("assertThat(names.contains(order)).isTrue();");
        assert_untouched("assertThat(names.isEmpty()).as(\"names\").isTrue();");
        assert_untouched("assertThat(order instanceof Order o).isTrue();");
        assert_untouched("assertThat(order instanceof Order).isFalse();");
        assert_untouched("assertThat(code.startsWith(\"A\")).isFalse();");
        assert_untouched("assertThat(names.isEmpty()).isEqualTo(true);");
    }

    /// A project's own `List` has no `isEmpty` assertion behind it.
    #[test]
    fn a_type_that_is_not_the_jdks_is_not_trusted() {
        let source = test_class("assertThat(names.isEmpty()).isTrue();")
            .replace("import java.util.List;", "import com.acme.model.List;");
        assert!(squiggled(&source, CODE).is_empty());
    }
}
