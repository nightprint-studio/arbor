//! Soft assertions nobody reports.
//!
//! A `SoftAssertions` object does not throw: it *collects*, and throws everything at once when
//! `assertAll()` is called. Forget that call and every failure is collected into an object that goes
//! out of scope — the test passes, every time, and the assertions in it look exactly like working
//! ones.
//!
//! Only the shape that cannot be reported anywhere else is judged: a **local** `new SoftAssertions()`
//! (or `BDDSoftAssertions`) that the method only ever calls `assertThat…` / `then…` on. The moment the
//! variable goes anywhere — an argument, a return value, an assignment, a method reference — somebody
//! else may call `assertAll`, and nothing is said. The auto-closing kinds and the JUnit 5 extension
//! report on their own and are never candidates.

use bennu_ext::prelude::{ExtEdit, ExtIntention, ExtProblem};
use bennu_proto::prelude::{severity, Diagnostic};
use tree_sitter::Node;

use crate::chain::call_name;
use crate::ext::{CODE_SOFT_NEVER_ASSERTED, INTENTION_ADD_ASSERT_ALL};
use crate::resolve::{creates, names_type, API_PACKAGE};
use crate::syntax::{ancestors, children, descendants, line_indent, unparen};
use crate::unit::Unit;

/// The soft-assertions types that need an explicit `assertAll()`.
const COLLECTING: &[&str] = &["SoftAssertions", "BDDSoftAssertions"];

struct Unasserted<'t> {
    name: Node<'t>,
    declaration: Node<'t>,
    body: Node<'t>,
}

pub(crate) fn diagnostics(unit: &Unit<'_>) -> Vec<Diagnostic> {
    unasserted(unit)
        .into_iter()
        .map(|u| {
            let name = unit.text(u.name);
            Diagnostic {
                message: format!(
                    "`{name}` collects failures that nothing reports — without `{name}.assertAll()` \
                     this test passes whatever its assertions find"
                ),
                severity: severity::WARNING.to_string(),
                code: CODE_SOFT_NEVER_ASSERTED.to_string(),
                start: u.name.start_byte(),
                end: u.name.end_byte(),
            }
        })
        .collect()
}

/// The `assertAll()` a reported variable is missing — offered only where appending it to the method
/// is certain to compile and to run.
pub(crate) fn intentions(unit: &Unit<'_>, problems: &[ExtProblem]) -> Vec<ExtIntention> {
    let reported: Vec<&ExtProblem> =
        problems.iter().filter(|p| p.code == CODE_SOFT_NEVER_ASSERTED).collect();
    if reported.is_empty() {
        return Vec::new();
    }
    unasserted(unit)
        .iter()
        .filter(|u| reported.iter().any(|p| p.start == u.name.start_byte() && p.end == u.name.end_byte()))
        .filter_map(|u| add_assert_all(unit, u))
        .collect()
}

fn unasserted<'t>(unit: &'t Unit<'_>) -> Vec<Unasserted<'t>> {
    unit.nodes()
        .into_iter()
        .filter(|n| n.kind() == "local_variable_declaration")
        .flat_map(|declaration| collectors_in(unit, declaration))
        .collect()
}

fn collectors_in<'t>(unit: &Unit<'_>, declaration: Node<'t>) -> Vec<Unasserted<'t>> {
    let Some(ty) = declaration.child_by_field_name("type") else { return Vec::new() };
    let written = unit.compact(ty);
    let inferred = written == "var" || written == "val";
    if !inferred && !COLLECTING.iter().any(|t| names_type(&written, API_PACKAGE, t, &unit.facts)) {
        return Vec::new();
    }
    let Some(body) = enclosing_body(declaration) else { return Vec::new() };
    children(declaration)
        .into_iter()
        .filter(|d| d.kind() == "variable_declarator")
        .filter_map(|d| {
            let name = d.child_by_field_name("name")?;
            let value = d.child_by_field_name("value")?;
            let candidate = d.child_by_field_name("dimensions").is_none()
                && creates_fresh(unit, value)
                && only_collected(unit, body, name);
            candidate.then_some(Unasserted { name, declaration, body })
        })
        .collect()
}

/// `new SoftAssertions()` with no arguments and no body.
fn creates_fresh(unit: &Unit<'_>, value: Node<'_>) -> bool {
    creates(unit, value, COLLECTING)
        && unparen(value).child_by_field_name("arguments").is_some_and(|a| children(a).is_empty())
}

fn enclosing_body<'t>(node: Node<'t>) -> Option<Node<'t>> {
    ancestors(node)
        .find(|a| matches!(a.kind(), "method_declaration" | "constructor_declaration" | "lambda_expression"))?
        .child_by_field_name("body")
}

/// Whether every use of the variable in `body` is a call to one of its `assertThat…` / `then…`
/// methods — and there is at least one.
///
/// Any other use at all silences: an `assertAll()` obviously, but also an argument, a return, an
/// assignment, a `wasSuccess()` read by hand. The search covers the whole method rather than the
/// variable's exact scope, which can only find *more* uses — and more uses only ever silence.
fn only_collected<'t>(unit: &Unit<'_>, body: Node<'t>, name: Node<'t>) -> bool {
    let written = unit.text(name);
    let mut collected = 0usize;
    for id in descendants(body) {
        if id.kind() != "identifier" || id == name || unit.text(id) != written {
            continue;
        }
        let Some(parent) = id.parent() else { return false };
        if names_member(parent, id) {
            continue;
        }
        let receives =
            parent.kind() == "method_invocation" && parent.child_by_field_name("object") == Some(id);
        let method = call_name(parent, unit.source);
        if !receives || !(method.starts_with("assertThat") || method.starts_with("then")) {
            return false;
        }
        collected += 1;
    }
    collected > 0
}

/// Whether `id` is the member name in `parent` (`x.softly`, `x.softly()`) rather than a use of a
/// variable called that.
fn names_member<'t>(parent: Node<'t>, id: Node<'t>) -> bool {
    matches!(parent.kind(), "method_invocation" | "field_access")
        && (parent.child_by_field_name("name") == Some(id) || parent.child_by_field_name("field") == Some(id))
}

/// Append `name.assertAll();` to the method.
///
/// The end of the method is only the right place when control certainly gets there, so: no `return`
/// anywhere in it, the variable declared directly in the method's own block (not in a nested one it
/// would be out of scope past), the last statement one that completes normally, and no lambda or
/// anonymous class capturing the variable — whose assertions might run after the method returns.
fn add_assert_all(unit: &Unit<'_>, u: &Unasserted<'_>) -> Option<ExtIntention> {
    let owner = u.body.parent()?;
    if !matches!(owner.kind(), "method_declaration" | "constructor_declaration")
        || u.body.has_error()
        || u.declaration.parent()? != u.body
    {
        return None;
    }
    let name = unit.text(u.name);
    let inside = descendants(u.body);
    if inside.iter().any(|n| n.kind() == "return_statement") {
        return None;
    }
    let captured = inside.iter().any(|n| {
        matches!(n.kind(), "lambda_expression" | "class_body")
            && descendants(*n).iter().any(|i| i.kind() == "identifier" && unit.text(*i) == name)
    });
    if captured {
        return None;
    }
    let last = children(u.body).into_iter().last()?;
    if !matches!(last.kind(), "expression_statement" | "local_variable_declaration") {
        return None;
    }
    Some(ExtIntention {
        id: INTENTION_ADD_ASSERT_ALL.to_string(),
        label: format!("Add {name}.assertAll() at the end of the test"),
        edits: vec![assert_all_edit(unit, u.body, last, name)?],
    })
}

/// On its own line before the closing brace when the brace has one, after the last statement when it
/// does not (`{ …; softly.assertThat(x).isTrue(); }`).
fn assert_all_edit(unit: &Unit<'_>, body: Node<'_>, last: Node<'_>, name: &str) -> Option<ExtEdit> {
    let source = unit.source;
    let close = body.end_byte().checked_sub(1)?;
    if source.as_bytes().get(close) != Some(&b'}') {
        return None;
    }
    let line_start = source[..close].rfind('\n').map_or(0, |i| i + 1);
    let brace_alone = line_start > last.end_byte() && source[line_start..close].trim().is_empty();
    Some(if brace_alone {
        let indent = line_indent(source, last.start_byte());
        ExtEdit::insert(line_start, format!("{indent}{name}.assertAll();\n"))
    } else {
        ExtEdit::insert(last.end_byte(), format!(" {name}.assertAll();"))
    })
}

#[cfg(test)]
mod tests {
    use crate::ext::{CODE_SOFT_NEVER_ASSERTED as CODE, INTENTION_ADD_ASSERT_ALL as FIX};
    use crate::testing::{apply, offered, squiggled};

    fn test_class(body: &str) -> String {
        format!(
            "package com.acme;\n\nimport org.assertj.core.api.SoftAssertions;\n\nclass OrderTest {{\n    void totals() {{\n{body}\n    }}\n}}\n"
        )
    }

    const COLLECTED: &str = "        SoftAssertions softly = new SoftAssertions();\n        softly.assertThat(order.total()).isEqualTo(3);\n        softly.assertThat(order.lines()).hasSize(2);";

    #[test]
    fn a_soft_assertions_object_nothing_reports_is_flagged() {
        assert_eq!(squiggled(&test_class(COLLECTED), CODE), ["softly"]);
        let inferred = COLLECTED.replace("SoftAssertions softly", "var softly");
        assert_eq!(squiggled(&test_class(&inferred), CODE), ["softly"]);
    }

    #[test]
    fn the_fix_appends_assert_all_as_the_last_statement() {
        let src = test_class(COLLECTED);
        let fix = offered(&src, FIX, src.find("softly").unwrap()).expect("the fix");
        assert_eq!(apply(&src, &fix.edits), test_class(&format!("{COLLECTED}\n        softly.assertAll();")));
    }

    #[test]
    fn a_reported_object_is_silent() {
        let asserted = format!("{COLLECTED}\n        softly.assertAll();");
        assert!(squiggled(&test_class(&asserted), CODE).is_empty());
    }

    /// Once the variable goes anywhere, somebody else may report it.
    #[test]
    fn an_object_that_escapes_the_method_is_not_judged() {
        for escape in ["verify(softly);", "return softly;", "this.last = softly;", "SoftAssertions alias = softly;"] {
            let body = format!("{COLLECTED}\n        {escape}");
            assert!(squiggled(&test_class(&body), CODE).is_empty(), "{escape}");
        }
    }

    #[test]
    fn the_kinds_that_report_on_their_own_are_not_candidates() {
        let closing = "        try (AutoCloseableSoftAssertions softly = new AutoCloseableSoftAssertions()) {\n            softly.assertThat(order.total()).isEqualTo(3);\n        }";
        assert!(squiggled(&test_class(closing), CODE).is_empty());
        let lambda = "        SoftAssertions.assertSoftly(softly -> softly.assertThat(order.total()).isEqualTo(3));";
        assert!(squiggled(&test_class(lambda), CODE).is_empty());
    }

    #[test]
    fn a_project_class_called_soft_assertions_is_not_assertjs() {
        let own = test_class(COLLECTED).replace("org.assertj.core.api.SoftAssertions", "com.acme.SoftAssertions");
        assert!(squiggled(&own, CODE).is_empty());
    }

    /// The end of the method is not reached after a `return`, and a lambda's assertions may run after
    /// it — both keep the warning and withhold the edit.
    #[test]
    fn the_fix_is_withheld_where_the_end_of_the_method_is_not_the_end_of_the_test() {
        let captured = COLLECTED.replace(
            "softly.assertThat(order.lines()).hasSize(2);",
            "order.lines().forEach(line -> softly.assertThat(line).isNotNull());",
        );
        let src = test_class(&captured);
        assert_eq!(squiggled(&src, CODE), ["softly"]);
        assert!(offered(&src, FIX, src.find("softly").unwrap()).is_none());

        let early = format!("        if (order == null) {{\n            return;\n        }}\n{COLLECTED}");
        let src = test_class(&early);
        assert_eq!(squiggled(&src, CODE), ["softly"]);
        assert!(offered(&src, FIX, src.find("softly").unwrap()).is_none());
    }
}
