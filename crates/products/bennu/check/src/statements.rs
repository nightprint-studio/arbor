//! "Not a statement" diagnostics — an expression used as a statement that Java forbids.
//!
//! JLS §14.8: an *expression statement* may only be an **assignment**, a **pre/post
//! increment/decrement**, a **method invocation**, or a **class instance creation** (`new`).
//! Anything else — a bare field access (`stepper.add;`), a lone identifier (`x;`), an arithmetic or
//! comparison expression (`a + b;`, `a == b;`) — is a compile error ("not a statement"), even though
//! it parses cleanly. tree-sitter accepts it syntactically, so this catches the classic bug of
//! forgetting the call parentheses: `list.clear;` instead of `list.clear();`.
//!
//! Pure AST: we only look at the KIND of the expression under each `expression_statement`, so there
//! are no false positives (an ERROR node is left to the syntax check).
//!
//! ## The one place an `expression_statement` isn't a statement
//!
//! An arrow switch arm (`case OWNER -> 4;`) is parsed by tree-sitter-java as a `switch_rule`
//! whose body is an `expression_statement` — but in a switch **expression** that body is the
//! arm's *value*, and any expression is legal there. `return switch (p) { case OWNER -> 4; }`
//! is ordinary Java, and reporting "`4` is not a statement" on it is exactly the false
//! positive this module claims not to have.
//!
//! So a `switch_rule` body is skipped. The cost is real and worth naming: in a switch used as
//! a *statement*, an arm body genuinely must be a statement expression, and `case A -> x.field;`
//! there is now missed. Telling the two apart needs the switch's surrounding context rather
//! than the node itself, and while that is knowable, a check that is silent about a rare
//! mistake beats one that shouts about a common correct construct (docs §7).
//!
//! ## `switch (x) { … };` — a trailing semicolon, not a broken block
//!
//! The grammar has one rule for both switch forms and puts `switch_expression` inside
//! `expression`, so a switch **statement** written with a trailing `;` parses as an
//! `expression_statement` wrapping the whole switch. Read literally that is "a switch is not a
//! statement expression" — and the squiggle lands on the entire block, which is how a stray `;`
//! managed to paint a 200-line switch red. Java reads it as a switch statement followed by an
//! empty statement: legal, merely redundant. So the switch is left alone and the `;` gets the
//! same *warning* a stray `;` gets anywhere else.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// The expression kinds JLS §14.8 allows as a statement. Anything else under an
/// `expression_statement` is "not a statement".
fn is_statement_expression(kind: &str) -> bool {
    matches!(
        kind,
        "method_invocation"            // foo() / obj.foo()
            | "object_creation_expression" // new Foo()
            | "assignment_expression"      // x = …, x += …
            | "update_expression"          // i++, --i
    )
}

/// The `;` closing an `expression_statement` — the anonymous last child. `None` when the parse is
/// missing it (malformed input, left to the syntax check).
fn trailing_semicolon<'t>(stmt: Node<'t>) -> Option<Node<'t>> {
    let last = stmt.child(stmt.child_count().checked_sub(1)?)?;
    (last.kind() == ";" && !last.is_missing()).then_some(last)
}

/// Flag every `expression_statement` whose expression isn't a legal statement expression.
pub fn invalid_statements(root: Node, source: &str) -> Vec<Diagnostic> {
    invalid_statements_nodes(&crate::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
pub fn invalid_statements_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() != "expression_statement" {
            continue;
        }
        // An arrow switch arm's body: the "statement" is the arm's VALUE. See the module doc.
        if n.parent().map(|p| p.kind()) == Some("switch_rule") {
            continue;
        }
        // The wrapped expression is the statement's first (and only) named child.
        let Some(expr) = n.named_child(0) else { continue };
        let kind = expr.kind();
        // Leave malformed input to the syntax check; only flag a cleanly-parsed non-statement.
        if kind == "ERROR" || expr.is_error() || expr.has_error() {
            continue;
        }
        // A switch statement that ends with `;`. See the module doc: the switch is fine, the
        // semicolon is the redundant part — so that is what the diagnostic points at.
        if kind == "switch_expression" {
            if let Some(semi) = trailing_semicolon(n) {
                out.push(crate::check_id::CheckId::EmptyStatement.at(
                    semi,
                    "Unnecessary semicolon after a `switch` statement",
                ));
            }
            continue;
        }
        if !is_statement_expression(kind) {
            let text = expr.utf8_text(bytes).unwrap_or("");
            let hint = if kind == "field_access" || kind == "identifier" {
                " (did you mean to call it — add `()`?)"
            } else {
                ""
            };
            out.push(Diagnostic {
                message: format!("`{}` is not a statement{hint}", text.trim()),
                severity: crate::check_id::CheckId::NotAStatement.severity().to_string(),
                code: crate::check_id::CheckId::NotAStatement.code().to_string(),
                start: expr.start_byte(),
                end: expr.end_byte(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        p.parse(src, None).unwrap()
    }

    fn bad(body: &str) -> Vec<Diagnostic> {
        let src = format!("class C {{ void m() {{ {body} }} }}");
        let tree = parse(&src);
        invalid_statements(tree.root_node(), &src)
    }

    #[test]
    fn field_access_as_statement_is_flagged() {
        // The user's example: `stepper.add;` — a field access, not a call.
        let d = bad("stepper.add;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].severity, "error");
        assert!(d[0].message.contains("not a statement"));
        assert!(d[0].message.contains("()"), "should hint the missing call: {}", d[0].message);
    }

    #[test]
    fn method_call_and_new_and_assignment_and_update_are_ok() {
        assert!(bad("foo();").is_empty());
        assert!(bad("obj.foo();").is_empty());
        assert!(bad("new Foo();").is_empty());
        assert!(bad("x = 5;").is_empty());
        assert!(bad("x += 5;").is_empty());
        assert!(bad("i++;").is_empty());
        assert!(bad("--i;").is_empty());
    }

    #[test]
    fn lone_identifier_is_flagged() {
        let d = bad("x;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("not a statement"));
    }

    #[test]
    fn arithmetic_and_comparison_are_flagged() {
        assert_eq!(bad("a + b;").len(), 1);
        assert_eq!(bad("a == b;").len(), 1);
    }

    #[test]
    fn nested_field_access_is_flagged_once() {
        let d = bad("a.b.c;");
        assert_eq!(d.len(), 1, "one diagnostic for the whole statement: {d:?}");
    }

    #[test]
    fn chained_call_then_field_is_flagged() {
        // `list.iterator().hasNext;` — the outer expression is a field access on a call result.
        let d = bad("list.iterator().hasNext;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("()"));
    }

    #[test]
    fn statements_inside_control_flow_are_checked() {
        let d = bad("if (cond) { obj.value; }");
        assert_eq!(d.len(), 1, "a bad statement in a nested block is still caught: {d:?}");
    }

    // ── switch expressions ───────────────────────────────────────────────────────

    /// The reported bug: a `return switch (…) { case X -> 4; }` had every arm flagged, because
    /// tree-sitter wraps an arrow arm's value in an `expression_statement`.
    #[test]
    fn switch_expression_arms_are_values_not_statements() {
        let src = "class C {\n  Integer level(P p) {\n    return switch (p) {\n\
                   case OWNER -> 4;\n case FULL -> 3;\n case NONE -> 0;\n    };\n  }\n}";
        let tree = parse(src);
        let d = invalid_statements(tree.root_node(), src);
        assert!(d.is_empty(), "an arrow arm's value is legal Java: {d:?}");
    }

    /// Not just literals: any expression is a legal arm value.
    #[test]
    fn switch_expression_arm_expressions_are_not_flagged() {
        let src = "class C {\n  String f(P p) {\n    return switch (p) {\n\
                   case A -> a + b;\n case B -> obj.field;\n case C -> \"x\";\n    };\n  }\n}";
        let tree = parse(src);
        let d = invalid_statements(tree.root_node(), src);
        assert!(d.is_empty(), "{d:?}");
    }

    /// The skip is scoped to the arm body itself — a bad statement inside an arm's BLOCK is
    /// still a statement position, and still caught.
    #[test]
    fn a_block_arm_still_checks_its_statements() {
        let src = "class C {\n  void f(P p) {\n    switch (p) {\n\
                   case A -> { obj.value; }\n    }\n  }\n}";
        let tree = parse(src);
        let d = invalid_statements(tree.root_node(), src);
        assert_eq!(d.len(), 1, "a block arm's body is ordinary statement territory: {d:?}");
    }

    /// The reported bug: a trailing `;` on a switch STATEMENT reported the whole block as "not a
    /// statement". It must be a warning, and it must point at the semicolon.
    #[test]
    fn trailing_semicolon_on_a_switch_statement_warns_on_the_semicolon() {
        let src = "class C {\n  void f(int i) {\n    switch (i) {\n\
                   case 1: break;\n    };\n  }\n}";
        let tree = parse(src);
        let d = invalid_statements(tree.root_node(), src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].severity, "warning");
        assert_eq!(&src[d[0].start..d[0].end], ";", "the squiggle covers the `;`, not the switch");
        assert!(d[0].message.contains("semicolon"), "{}", d[0].message);
    }

    /// …and a switch statement WITHOUT the stray `;` says nothing at all.
    #[test]
    fn a_plain_switch_statement_is_silent() {
        let src = "class C {\n  void f(int i) {\n    switch (i) {\n\
                   case 1: break;\n    }\n  }\n}";
        let tree = parse(src);
        assert!(invalid_statements(tree.root_node(), src).is_empty());
    }

    /// A switch used as a real expression is not an `expression_statement` at all — untouched.
    #[test]
    fn a_switch_expression_assigned_to_a_variable_is_silent() {
        let src = "class C {\n  int f(int i) {\n    int v = switch (i) {\n\
                   case 1 -> 1;\n default -> 0;\n    };\n    return v;\n  }\n}";
        let tree = parse(src);
        assert!(invalid_statements(tree.root_node(), src).is_empty());
    }

    /// And the classic colon-form switch is untouched by the skip.
    #[test]
    fn colon_form_switch_statements_are_still_checked() {
        let src = "class C {\n  void f(int i) {\n    switch (i) {\n\
                   case 1: obj.value; break;\n    }\n  }\n}";
        let tree = parse(src);
        let d = invalid_statements(tree.root_node(), src);
        assert_eq!(d.len(), 1, "{d:?}");
    }
}
