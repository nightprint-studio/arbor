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
        // The wrapped expression is the statement's first (and only) named child.
        let Some(expr) = n.named_child(0) else { continue };
        let kind = expr.kind();
        // Leave malformed input to the syntax check; only flag a cleanly-parsed non-statement.
        if kind == "ERROR" || expr.is_error() || expr.has_error() {
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
                severity: "error".to_string(),
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
}
