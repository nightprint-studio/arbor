//! JUnit's assertions, rewritten as AssertJ's.
//!
//! Mechanical, and exactly the kind of mechanical that goes wrong by hand: JUnit 4 takes the message
//! **first** and JUnit 5 takes it **last**, both take *expected* before *actual*, and a swapped pair
//! still compiles and still passes — it only fails with the two values named the wrong way round.
//!
//! Offered one call at a time, and for the whole file in one gesture. Declined wherever the target is
//! not the same assertion:
//!
//! - a **delta** overload (`assertEquals(1.0, ratio, 0.01)`) — `isEqualTo` has no tolerance;
//! - a **`Supplier`** message (`() -> "…"`) — `as(…)` takes a description, not a supplier;
//! - a two-argument JUnit 4 `assertTrue(label, flag)` whose first argument is not a literal — the
//!   message and the condition cannot be told apart;
//! - a file where a bare `assertThat` already means something else (Hamcrest, a helper) — the import
//!   the rewrite needs would take that name away from it.

use std::cmp::Reverse;

use bennu_ext::prelude::{ExtEdit, ExtIntention};
use bennu_facts::prelude::static_call_resolves_to;
use bennu_intentions::prelude::insert_static_import_edit;
use tree_sitter::Node;

use crate::chain::{call_name, statement_call};
use crate::ext::{INTENTION_FROM_JUNIT, INTENTION_FROM_JUNIT_FILE};
use crate::resolve::{receiver, Receiver, ASSERT_OWNERS};
use crate::scope::visible_declaration;
use crate::syntax::{children, has_comment, span, unparen};
use crate::unit::Unit;

const JUNIT4: &[&str] = &["org.junit.Assert"];
const JUNIT5: &[&str] = &["org.junit.jupiter.api.Assertions"];
const ASSERTIONS: &str = "org.assertj.core.api.Assertions";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dialect {
    /// `org.junit.Assert` — message first.
    Four,
    /// `org.junit.jupiter.api.Assertions` — message last.
    Five,
}

/// One JUnit assertion and the AssertJ check it becomes.
struct Rule {
    junit: &'static str,
    check: &'static str,
    /// 2 = `(expected, actual)`, 1 = `(subject)`.
    operands: usize,
}

const RULES: &[Rule] = &[
    Rule { junit: "assertEquals", check: "isEqualTo", operands: 2 },
    Rule { junit: "assertNotEquals", check: "isNotEqualTo", operands: 2 },
    Rule { junit: "assertSame", check: "isSameAs", operands: 2 },
    Rule { junit: "assertNotSame", check: "isNotSameAs", operands: 2 },
    Rule { junit: "assertArrayEquals", check: "containsExactly", operands: 2 },
    Rule { junit: "assertTrue", check: "isTrue", operands: 1 },
    Rule { junit: "assertFalse", check: "isFalse", operands: 1 },
    Rule { junit: "assertNull", check: "isNull", operands: 1 },
    Rule { junit: "assertNotNull", check: "isNotNull", operands: 1 },
];

struct Conversion {
    statement: (usize, usize),
    call: (usize, usize),
    text: String,
}

impl Conversion {
    fn edit(&self) -> ExtEdit {
        ExtEdit::replace(self.call.0, self.call.1, self.text.clone())
    }
}

pub(crate) fn intentions(unit: &Unit<'_>, offset: usize) -> Vec<ExtIntention> {
    if extends_an_assertion_base(unit) {
        return Vec::new();
    }
    let found = conversions(unit);
    if found.is_empty() {
        return Vec::new();
    }
    let Some(import) = assert_that_import(unit) else { return Vec::new() };
    let mut out = Vec::new();
    let here = found
        .iter()
        .filter(|c| c.statement.0 <= offset && offset <= c.statement.1)
        .min_by_key(|c| c.statement.1 - c.statement.0);
    if let Some(conversion) = here {
        out.push(ExtIntention {
            id: INTENTION_FROM_JUNIT.to_string(),
            label: "Replace with AssertJ".to_string(),
            edits: with_import(vec![conversion.edit()], &import),
        });
    }
    let whole_file = outermost(found);
    if whole_file.len() >= 2 {
        out.push(ExtIntention {
            id: INTENTION_FROM_JUNIT_FILE.to_string(),
            label: "Replace every JUnit assertion in this file with AssertJ".to_string(),
            edits: with_import(whole_file.iter().map(Conversion::edit).collect(), &import),
        });
    }
    out
}

fn with_import(mut edits: Vec<ExtEdit>, import: &Option<ExtEdit>) -> Vec<ExtEdit> {
    if let Some(import) = import {
        edits.insert(0, import.clone());
    }
    edits
}

/// A conversion inside another one's arguments is dropped: the outer edit replaces its text
/// wholesale, and two edits over one span is not a set the host can apply.
fn outermost(mut found: Vec<Conversion>) -> Vec<Conversion> {
    found.sort_by_key(|c| (c.call.0, Reverse(c.call.1)));
    let mut kept: Vec<Conversion> = Vec::new();
    for conversion in found {
        if kept.last().is_some_and(|k| conversion.call.0 < k.call.1) {
            continue;
        }
        kept.push(conversion);
    }
    kept
}

fn conversions(unit: &Unit<'_>) -> Vec<Conversion> {
    unit.nodes()
        .into_iter()
        .filter(|n| n.kind() == "expression_statement")
        .filter_map(|statement| convert(unit, statement))
        .collect()
}

fn convert(unit: &Unit<'_>, statement: Node<'_>) -> Option<Conversion> {
    let call = statement_call(statement)?;
    let name = call_name(call, unit.source);
    let rule = RULES.iter().find(|r| r.junit == name)?;
    let dialect = dialect_of(unit, call, name)?;
    let list = call.child_by_field_name("arguments")?;
    if has_comment(list) || call.child_by_field_name("type_arguments").is_some() {
        return None;
    }
    let args = children(list);
    if args.iter().any(|a| matches!(a.kind(), "lambda_expression" | "method_reference")) {
        return None;
    }
    let (message, operands) = split(unit, dialect, rule.operands, &args)?;
    let text = render(unit, rule, dialect, message, &operands, call)?;
    Some(Conversion { statement: span(statement), call: span(call), text })
}

fn dialect_of(unit: &Unit<'_>, call: Node<'_>, name: &str) -> Option<Dialect> {
    let qualifier = match receiver(call, unit.source) {
        Receiver::Bare => None,
        Receiver::Qualifier(q) => Some(q),
        Receiver::Other => return None,
    };
    let q = qualifier.as_deref();
    if static_call_resolves_to(name, q, &unit.facts, JUNIT4) {
        Some(Dialect::Four)
    } else if static_call_resolves_to(name, q, &unit.facts, JUNIT5) {
        Some(Dialect::Five)
    } else {
        None
    }
}

/// The message, if any, and the operands in JUnit's order.
fn split<'t>(
    unit: &Unit<'_>,
    dialect: Dialect,
    operands: usize,
    args: &[Node<'t>],
) -> Option<(Option<Node<'t>>, Vec<Node<'t>>)> {
    if args.len() == operands {
        return Some((None, args.to_vec()));
    }
    if args.len() != operands + 1 {
        return None;
    }
    let (message, rest) = match dialect {
        Dialect::Four => (args[0], &args[1..]),
        Dialect::Five => (args[operands], &args[..operands]),
    };
    // JUnit 4's one-operand assertions put the message first as well, so `assertTrue(a, b)` is
    // `(message, condition)` — but only a literal proves which is which.
    let trusted = if dialect == Dialect::Four && operands == 1 {
        message.kind() == "string_literal" && !unit.text(message).contains('%')
    } else {
        is_message(unit, message)
    };
    trusted.then(|| (Some(message), rest.to_vec()))
}

/// A string literal, or a `+` concatenation with one among its operands — which makes the whole
/// thing a `String`, whatever else is concatenated. A literal holding `%` is declined: older AssertJ
/// formats every description.
fn is_message(unit: &Unit<'_>, node: Node<'_>) -> bool {
    let mut operands = Vec::new();
    concatenated(unit, node, &mut operands);
    let literals: Vec<&Node<'_>> = operands.iter().filter(|n| n.kind() == "string_literal").collect();
    !literals.is_empty() && literals.iter().all(|l| !unit.text(**l).contains('%'))
}

fn concatenated<'t>(unit: &Unit<'_>, node: Node<'t>, out: &mut Vec<Node<'t>>) {
    let node = unparen(node);
    let plus = node.kind() == "binary_expression"
        && node.child_by_field_name("operator").is_some_and(|o| unit.text(o) == "+");
    match (plus, node.child_by_field_name("left"), node.child_by_field_name("right")) {
        (true, Some(left), Some(right)) => {
            concatenated(unit, left, out);
            concatenated(unit, right, out);
        }
        _ => out.push(node),
    }
}

fn render(
    unit: &Unit<'_>,
    rule: &Rule,
    dialect: Dialect,
    message: Option<Node<'_>>,
    operands: &[Node<'_>],
    at: Node<'_>,
) -> Option<String> {
    let described = message.map(|m| format!(".as({})", unit.text(m))).unwrap_or_default();
    match operands {
        [subject] => {
            // `assertThat(null)` is ambiguous between AssertJ's overloads and does not compile.
            if subject.kind() == "null_literal" {
                return None;
            }
            Some(format!("assertThat({}){described}.{}()", unit.text(*subject), rule.check))
        }
        [expected, actual] => {
            if actual.kind() == "null_literal" {
                return None;
            }
            if rule.junit == "assertArrayEquals" && !same_array_type(unit, *expected, *actual, at) {
                return None;
            }
            // JUnit 4's `assertEquals(double, double)` is the deprecated overload that always fails.
            let floating = [expected, actual].iter().any(|n| n.kind().ends_with("floating_point_literal"));
            if dialect == Dialect::Four && floating && rule.operands == 2 && rule.junit.contains("Equals") {
                return None;
            }
            Some(format!("assertThat({}){described}.{}({})", unit.text(*actual), rule.check, unit.text(*expected)))
        }
        _ => None,
    }
}

/// `containsExactly(ELEMENT...)` is typed; `assertArrayEquals(Object[], Object[])` is not. Offered only
/// when both arrays visibly have the same written type.
fn same_array_type(unit: &Unit<'_>, expected: Node<'_>, actual: Node<'_>, at: Node<'_>) -> bool {
    match (array_type(unit, expected, at), array_type(unit, actual, at)) {
        (Some(e), Some(a)) => e == a,
        _ => false,
    }
}

fn array_type(unit: &Unit<'_>, node: Node<'_>, at: Node<'_>) -> Option<String> {
    match node.kind() {
        "identifier" => {
            let decl = visible_declaration(unit, unit.text(node), at)?;
            let typed = format!("{}{}", unit.compact(decl.ty?), "[]".repeat(decl.dims));
            typed.ends_with("[]").then_some(typed)
        }
        "array_creation_expression" => {
            let element = unit.compact(node.child_by_field_name("type")?);
            let dims: usize = children(node)
                .iter()
                .map(|c| match c.kind() {
                    "dimensions" => unit.text(*c).matches('[').count(),
                    "dimensions_expr" => 1,
                    _ => 0,
                })
                .sum();
            (dims > 0).then(|| format!("{element}{}", "[]".repeat(dims)))
        }
        _ => None,
    }
}

/// The import a bare `assertThat` needs: `Some(None)` when it already reaches AssertJ, `Some(edit)`
/// when adding one is safe, `None` when the name already means something else in this file.
fn assert_that_import(unit: &Unit<'_>) -> Option<Option<ExtEdit>> {
    let facts = &unit.facts;
    if static_call_resolves_to("assertThat", None, facts, ASSERT_OWNERS) {
        return Some(None);
    }
    let declared = facts.types.iter().any(|t| t.methods.iter().any(|m| m.name == "assertThat"));
    let imported_elsewhere = facts.imports.iter().any(|i| i.ends_with(".assertThat"));
    // Reached through an on-demand import (JUnit 4's `Assert.*`, Hamcrest's) — a single import would
    // shadow it, and every one of those calls would stop compiling.
    let written_bare = unit.nodes().iter().any(|n| {
        n.kind() == "method_invocation"
            && n.child_by_field_name("object").is_none()
            && call_name(*n, unit.source) == "assertThat"
    });
    if declared || imported_elsewhere || written_bare {
        return None;
    }
    let edit = insert_static_import_edit(unit.source, ASSERTIONS, "assertThat")
        .map(|e| ExtEdit::replace(e.start, e.end, e.replacement));
    Some(edit)
}

/// A class extending JUnit 3's `TestCase` or `Assert` inherits its `assertEquals`, which shadows every
/// static import — the call is not the one resolved here.
fn extends_an_assertion_base(unit: &Unit<'_>) -> bool {
    unit.facts.types.iter().any(|t| {
        let base = t.extends.split('<').next().unwrap_or_default().trim();
        let simple = base.rsplit('.').next().unwrap_or_default();
        simple.ends_with("TestCase") || matches!(simple, "Assert" | "Assertions")
    })
}

#[cfg(test)]
mod tests {
    use crate::ext::{INTENTION_FROM_JUNIT as ONE, INTENTION_FROM_JUNIT_FILE as ALL};
    use crate::testing::{apply, offered};

    const JUNIT4: &str = "import org.junit.Test;\n\nimport static org.junit.Assert.assertEquals;\nimport static org.junit.Assert.assertTrue;";
    const JUNIT5: &str = "import org.junit.jupiter.api.Test;\n\nimport static org.junit.jupiter.api.Assertions.*;";

    fn test_class(imports: &str, body: &str) -> String {
        format!("package com.acme;\n\n{imports}\n\npublic class OrderTest {{\n    @Test\n    public void totals() {{\n{body}\n    }}\n}}\n")
    }

    /// `imports` with AssertJ's `assertThat` appended after its last static import.
    fn with_assertj(imports: &str) -> String {
        format!("{imports}\nimport static org.assertj.core.api.Assertions.assertThat;")
    }

    fn converted(imports: &str, body: &str, needle: &str) -> Option<String> {
        let source = test_class(imports, body);
        let offer = offered(&source, ONE, source.find(needle).unwrap())?;
        Some(apply(&source, &offer.edits))
    }

    #[test]
    fn junit4_takes_the_message_first() {
        let out = converted(JUNIT4, "        assertEquals(\"the total\", 3, order.total());", "assertEquals(");
        assert_eq!(
            out.as_deref(),
            Some(test_class(&with_assertj(JUNIT4), "        assertThat(order.total()).as(\"the total\").isEqualTo(3);").as_str())
        );
        let out = converted(JUNIT4, "        assertTrue(\"paid\", order.isPaid());", "assertTrue(");
        assert_eq!(
            out.as_deref(),
            Some(test_class(&with_assertj(JUNIT4), "        assertThat(order.isPaid()).as(\"paid\").isTrue();").as_str())
        );
    }

    #[test]
    fn junit5_takes_the_message_last() {
        let out = converted(JUNIT5, "        assertEquals(3, order.total(), \"the total\");", "assertEquals(");
        assert_eq!(
            out.as_deref(),
            Some(test_class(&with_assertj(JUNIT5), "        assertThat(order.total()).as(\"the total\").isEqualTo(3);").as_str())
        );
        let out = converted(JUNIT5, "        assertTrue(order.isPaid(), \"order \" + id);", "assertTrue(");
        assert_eq!(
            out.as_deref(),
            Some(test_class(&with_assertj(JUNIT5), "        assertThat(order.isPaid()).as(\"order \" + id).isTrue();").as_str())
        );
    }

    #[test]
    fn arrays_of_the_same_written_type_become_contains_exactly() {
        let body = "        int[] expected = {1, 2};\n        int[] actual = order.ids();\n        assertArrayEquals(expected, actual);";
        let out = converted(JUNIT5, body, "assertArrayEquals(");
        let to = body.replace("assertArrayEquals(expected, actual);", "assertThat(actual).containsExactly(expected);");
        assert_eq!(out.as_deref(), Some(test_class(&with_assertj(JUNIT5), &to).as_str()));
    }

    #[test]
    fn an_assertj_import_already_there_is_not_added_twice() {
        let imports = with_assertj(JUNIT4);
        let out = converted(&imports, "        assertEquals(3, order.total());", "assertEquals(");
        assert_eq!(out.as_deref(), Some(test_class(&imports, "        assertThat(order.total()).isEqualTo(3);").as_str()));
    }

    #[test]
    fn the_whole_file_is_one_gesture_with_one_import() {
        let source = test_class(JUNIT4, "        assertEquals(3, order.total());\n        assertTrue(order.isPaid());");
        let offer = offered(&source, ALL, 0).expect("the whole-file offer");
        assert_eq!(
            apply(&source, &offer.edits),
            test_class(&with_assertj(JUNIT4), "        assertThat(order.total()).isEqualTo(3);\n        assertThat(order.isPaid()).isTrue();")
        );
        let single = test_class(JUNIT4, "        assertEquals(3, order.total());");
        assert!(offered(&single, ALL, 0).is_none(), "one assertion is not a file-wide job");
    }

    /// None of these is the same assertion in AssertJ — or the rewrite would not compile.
    #[test]
    fn a_shape_with_no_exact_counterpart_is_not_offered() {
        let refused = [
            (JUNIT5, "        assertEquals(1.0, order.ratio(), 0.01);", "assertEquals("),
            (JUNIT4, "        assertEquals(0.5, order.ratio(), 0.01);", "assertEquals("),
            (JUNIT5, "        assertEquals(3, order.total(), () -> \"the total\");", "assertEquals("),
            (JUNIT4, "        assertTrue(label, order.isPaid());", "assertTrue("),
            (JUNIT4, "        assertEquals(\"50% off\", 3, order.total());", "assertEquals("),
            (JUNIT5, "        assertArrayEquals(order.expected(), order.ids());", "assertArrayEquals("),
        ];
        for (imports, body, needle) in refused {
            assert_eq!(converted(imports, body, needle), None, "{body}");
        }
    }

    /// Hamcrest's `assertThat` owns the bare name; importing AssertJ's would take it away.
    #[test]
    fn a_file_where_assert_that_is_somebody_elses_gets_no_offer() {
        let imports = format!("{JUNIT4}\nimport static org.hamcrest.MatcherAssert.assertThat;");
        assert_eq!(converted(&imports, "        assertEquals(3, order.total());", "assertEquals("), None);
        let star = "import org.junit.Test;\n\nimport static org.junit.Assert.*;";
        let body = "        assertEquals(3, order.total());\n        assertThat(order.total(), is(3));";
        assert_eq!(converted(star, body, "assertEquals("), None);
    }

    #[test]
    fn a_call_that_is_not_junits_is_not_converted() {
        let own = "import static com.acme.Checks.assertEquals;\nimport static org.junit.Assert.assertTrue;";
        assert_eq!(converted(own, "        assertEquals(3, order.total());", "assertEquals("), None);
    }
}
