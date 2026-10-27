//! Unreachable-code diagnostics (pure-AST): a statement that can never execute because the
//! preceding statement in the same block always transfers control away — a `return` / `throw` /
//! `break` / `continue`. In Java this is a **compile error**, not a warning.
//!
//! Conservative (never a false positive): only a statement that is the **direct** next sibling of an
//! unconditional terminator in the *same* block is flagged. A `return` nested in an `if` (or any
//! conditional) doesn't mark the following code dead, because the terminator isn't a direct sibling.
//! An empty statement (`;`) is skipped (flagging a stray semicolon would be noise). `while (false)`
//! and infinite-loop reachability are intentionally left out (harder to prove without false flags).

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Parse `source` and flag the first unreachable statement after each unconditional terminator.
pub fn unreachable_code(source: &str) -> Vec<Diagnostic> {
    match bennu_java::prelude::parse_java(source) {
        Some(tree) => unreachable_code_in(tree.root_node(), source),
        None => Vec::new(),
    }
}

/// Tree-driven core (shared with the `check_file` single-parse path).
pub fn unreachable_code_in(root: Node, source: &str) -> Vec<Diagnostic> {
    unreachable_code_nodes(&crate::engine::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
pub fn unreachable_code_nodes(nodes: &[Node], _source: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "block" || n.kind() == "constructor_body" {
            check_block(n, &mut out);
        }
    }
    out.sort_by_key(|d| d.start);
    out
}

/// Flag the first statement that directly follows an unconditional terminator in `block`.
fn check_block(block: Node, out: &mut Vec<Diagnostic>) {
    let mut c = block.walk();
    let mut terminated = false;
    for stmt in block.named_children(&mut c) {
        if !is_statement(stmt.kind()) {
            continue; // comments / non-statement nodes don't affect flow
        }
        if terminated {
            out.push(Diagnostic {
                message: "Unreachable statement".to_string(),
                severity: crate::engine::check_id::CheckId::UnreachableStatement.severity().to_string(),
                code: crate::engine::check_id::CheckId::UnreachableStatement.code().to_string(),
                start: stmt.start_byte(),
                end: stmt.end_byte(),
            });
            return; // one diagnostic per block is enough (the rest cascade from the same cause)
        }
        if is_terminator(stmt.kind()) || never_completes(stmt) {
            terminated = true;
        }
    }
}

/// A loop that cannot finish: its condition is the literal `true` (or a `for` has none) and nothing
/// inside it contains a `break` at all. `while (true) { work(); }` followed by a statement is the
/// JLS 14.22 error; a `break` anywhere in the body — even one aimed at an inner loop or a `switch` —
/// makes this stay silent, because telling which statement it leaves is where a false flag hides.
fn never_completes(stmt: Node) -> bool {
    let condition_is_true = match stmt.kind() {
        "while_statement" | "do_statement" => stmt
            .child_by_field_name("condition")
            .is_some_and(|c| c.kind() == "parenthesized_expression" && only_child_is_true(c)),
        "for_statement" => match stmt.child_by_field_name("condition") {
            None => true,
            Some(c) => c.kind() == "true",
        },
        _ => false,
    };
    condition_is_true && !contains_break(stmt)
}

fn only_child_is_true(parenthesized: Node) -> bool {
    parenthesized.named_child_count() == 1 && parenthesized.named_child(0).is_some_and(|c| c.kind() == "true")
}

fn contains_break(node: Node) -> bool {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "break_statement" {
            return true;
        }
        let mut c = n.walk();
        stack.extend(n.children(&mut c));
    }
    false
}

/// A statement that unconditionally transfers control away from the current block.
fn is_terminator(kind: &str) -> bool {
    matches!(
        kind,
        "return_statement" | "throw_statement" | "break_statement" | "continue_statement"
    )
}

/// Whether a node kind is an executable statement (so a `;` / comment between real statements
/// doesn't count as the "unreachable" one, and non-statements don't reset the terminated flag).
fn is_statement(kind: &str) -> bool {
    matches!(
        kind,
        "expression_statement"
            | "local_variable_declaration"
            | "if_statement"
            | "for_statement"
            | "enhanced_for_statement"
            | "while_statement"
            | "do_statement"
            | "switch_expression"
            | "return_statement"
            | "throw_statement"
            | "break_statement"
            | "continue_statement"
            | "yield_statement"
            | "try_statement"
            | "synchronized_statement"
            | "block"
            | "labeled_statement"
            | "assert_statement"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unreach(src: &str) -> Vec<String> {
        unreachable_code(src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn statement_after_return_is_unreachable() {
        let d = unreach("class C { int m() { return 1; int x = 2; } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Unreachable"), "{d:?}");
    }

    #[test]
    fn statement_after_throw_is_unreachable() {
        assert_eq!(unreach("class C { void m() { throw new RuntimeException(); foo(); } }").len(), 1);
    }

    #[test]
    fn statement_after_break_is_unreachable() {
        let src = "class C { void m() { for (;;) { break; int x = 1; } } }";
        assert_eq!(unreach(src).len(), 1, "{:?}", unreach(src));
    }

    #[test]
    fn a_statement_after_an_endless_loop_is_unreachable() {
        assert_eq!(unreach("class C { void m() { while (true) { } int x = 1; } }").len(), 1);
        assert_eq!(unreach("class C { void m() { for (;;) { work(); } int x = 1; } }").len(), 1);
        assert_eq!(unreach("class C { void m() { do { } while (true); int x = 1; } }").len(), 1);
    }

    #[test]
    fn a_loop_with_a_break_or_a_real_condition_can_finish() {
        assert!(unreach("class C { void m() { while (true) { if (f()) break; } int x = 1; } }").is_empty());
        assert!(unreach("class C { void m(boolean b) { while (b) { } int x = 1; } }").is_empty());
        assert!(unreach("class C { void m() { for (int i = 0; ; i++) { if (i > 3) break; } int x = 1; } }").is_empty());
        // A break aimed anywhere — here at a nested loop — is enough to stay silent.
        assert!(unreach("class C { void m() { while (true) { for (;;) { break; } } int x = 1; } }").is_empty());
    }

    #[test]
    fn return_as_last_statement_is_ok() {
        assert!(unreach("class C { int m() { int x = 1; return x; } }").is_empty());
    }

    #[test]
    fn return_inside_if_does_not_kill_following_code() {
        // The `return` is nested in the `if`, not a direct sibling → the call after is reachable.
        let src = "class C { void m(boolean b) { if (b) { return; } doWork(); } }";
        assert!(unreach(src).is_empty(), "{:?}", unreach(src));
    }

    #[test]
    fn empty_statement_after_return_is_not_flagged() {
        // A stray `;` after a return isn't worth a diagnostic; a real statement after it would be.
        assert!(unreach("class C { void m() { return; ; } }").is_empty());
    }

    #[test]
    fn only_first_unreachable_is_reported() {
        let d = unreach("class C { int m() { return 1; int a = 2; int b = 3; } }");
        assert_eq!(d.len(), 1, "one diagnostic per block ({d:?})");
    }
}
