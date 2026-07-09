//! "Missing return statement" diagnostics — a non-`void` method whose body can fall off the end
//! without returning.
//!
//! This is the *reachability* half of return checking (JLS §14.17 / §8.4.7): does every path out of
//! the body end in a `return` or `throw`? The *type* half (the returned value matches the declared
//! type) needs the resolver and is a later phase.
//!
//! **Conservative by construction**: `definitely_returns` answers "does this statement guarantee a
//! return/throw?" and errs toward **true** for anything it doesn't fully model (loops, `switch`,
//! `try`, labeled/synchronized) — so we never false-flag valid code. We only flag when the last
//! statement clearly *cannot* guarantee a return (a plain statement, or an `if` with no `else`).

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Flag every non-`void` method whose body can complete without a `return`/`throw`.
pub fn missing_return(root: Node, source: &str) -> Vec<Diagnostic> {
    missing_return_nodes(&crate::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
pub fn missing_return_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() != "method_declaration" {
            continue;
        }
        let Some(ret) = n.child_by_field_name("type") else { continue };
        if ret.kind() == "void_type" {
            continue; // void needs no return
        }
        // No body → abstract / interface / native method: nothing to check.
        let Some(body) = n.child_by_field_name("body") else { continue };
        if body.kind() != "block" {
            continue;
        }
        // A body still being typed (a syntax error inside) would mis-analyze — the syntax check
        // already flags it, so don't pile a spurious "missing return" on top.
        if body.has_error() {
            continue;
        }
        if !block_definitely_returns(body) {
            let ty = ret.utf8_text(bytes).unwrap_or("").trim();
            out.push(Diagnostic {
                message: format!("Missing return statement (method must return `{ty}`)"),
                severity: crate::check_id::CheckId::MissingReturn.severity().to_string(),
                code: crate::check_id::CheckId::MissingReturn.code().to_string(),
                start: ret.start_byte(),
                end: ret.end_byte(),
            });
        }
    }
    out
}

/// Flag the two *statement-shape* return errors, which need no resolver:
///   * `return <value>;` inside a `void` method or a constructor (JLS §14.17: a value-returning
///     return in a body that returns nothing);
///   * a bare `return;` inside a non-`void` method (a missing return value).
///
/// Returns are attributed to the nearest enclosing method/constructor — the walk stops at
/// `lambda_expression` and nested type/method declarations, so a `return` inside a lambda or an
/// anonymous class is judged against *its* target, never the outer method.
pub fn return_statement_errors(root: Node, source: &str) -> Vec<Diagnostic> {
    return_statement_errors_nodes(&crate::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
pub fn return_statement_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "method_declaration" => {
                let is_void = n
                    .child_by_field_name("type")
                    .is_some_and(|t| t.kind() == "void_type");
                let ty = n
                    .child_by_field_name("type")
                    .and_then(|t| t.utf8_text(bytes).ok())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if let Some(body) = n.child_by_field_name("body") {
                    check_returns(body, is_void, false, &ty, &mut out);
                }
            }
            "constructor_declaration" => {
                if let Some(body) = n.child_by_field_name("body") {
                    // A constructor returns nothing → a value-returning `return` is illegal, and a
                    // bare `return;` is always fine.
                    check_returns(body, true, true, "", &mut out);
                }
            }
            _ => {}
        }
    }
    out
}

/// Scan `body` for the returns that belong to this method/constructor (not to a nested lambda /
/// declaration) and flag the void/value mismatch.
fn check_returns(body: Node, is_void: bool, is_ctor: bool, ret_ty: &str, out: &mut Vec<Diagnostic>) {
    let mut returns = Vec::new();
    collect_returns(body, &mut returns);
    for r in returns {
        let has_value = has_return_value(r);
        if is_void && has_value {
            let where_ = if is_ctor { "a constructor" } else { "a `void` method" };
            out.push(Diagnostic {
                message: format!("Cannot return a value from {where_}"),
                severity: crate::check_id::CheckId::ReturnValueFromVoid.severity().to_string(),
                code: crate::check_id::CheckId::ReturnValueFromVoid.code().to_string(),
                start: r.start_byte(),
                end: r.end_byte(),
            });
        } else if !is_void && !has_value {
            out.push(Diagnostic {
                message: format!("Missing return value (method must return `{ret_ty}`)"),
                severity: crate::check_id::CheckId::MissingReturn.severity().to_string(),
                code: crate::check_id::CheckId::MissingReturn.code().to_string(),
                start: r.start_byte(),
                end: r.end_byte(),
            });
        }
    }
}

/// Recursively collect the `return_statement`s directly governed by the current method — descending
/// through control flow but stopping at any construct that introduces its own return target.
fn collect_returns<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) {
    let mut c = node.walk();
    for ch in node.named_children(&mut c) {
        match ch.kind() {
            "return_statement" => out.push(ch),
            // Own return target → don't attribute their returns to us.
            "lambda_expression"
            | "method_declaration"
            | "constructor_declaration"
            | "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration" => {}
            _ => collect_returns(ch, out),
        }
    }
}

/// Whether a `return_statement` carries a value (`return x;`) vs. a bare `return;`. A comment is not
/// a value.
fn has_return_value(ret: Node) -> bool {
    let mut c = ret.walk();
    for n in ret.named_children(&mut c) {
        if !matches!(n.kind(), "line_comment" | "block_comment") {
            return true;
        }
    }
    false
}

/// Does executing `stmt` guarantee a `return` / `throw` on every path? Conservative: constructs we
/// don't fully model answer **true** (assume they return) so we never flag valid code.
fn definitely_returns(stmt: Node) -> bool {
    match stmt.kind() {
        "return_statement" | "throw_statement" => true,
        "block" => block_definitely_returns(stmt),
        "if_statement" => {
            // Guarantees a return only when BOTH branches do (an `else` must exist).
            match (stmt.child_by_field_name("consequence"), stmt.child_by_field_name("alternative")) {
                (Some(cons), Some(alt)) => definitely_returns(cons) && definitely_returns(alt),
                _ => false, // no `else` → can fall through
            }
        }
        // Not fully modelled → assume it may guarantee a return (infinite loop, exhaustive switch,
        // try/finally, …). Conservative: this can MISS a real missing-return, but never false-flags.
        "for_statement"
        | "enhanced_for_statement"
        | "while_statement"
        | "do_statement"
        | "switch_expression"
        | "switch_statement"
        | "try_statement"
        | "try_with_resources_statement"
        | "labeled_statement"
        | "synchronized_statement"
        | "yield_statement" => true,
        // A plain statement (local var, expression statement, break/continue, …) doesn't return.
        _ => false,
    }
}

/// A block guarantees a return iff its LAST real statement does (comments ignored). An empty block
/// falls through.
fn block_definitely_returns(block: Node) -> bool {
    let mut last: Option<Node> = None;
    let mut c = block.walk();
    for ch in block.named_children(&mut c) {
        if matches!(ch.kind(), "line_comment" | "block_comment") {
            continue;
        }
        last = Some(ch);
    }
    last.map(definitely_returns).unwrap_or(false)
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

    fn check(members: &str) -> Vec<Diagnostic> {
        let src = format!("class C {{ {members} }}");
        let tree = parse(&src);
        missing_return(tree.root_node(), &src)
    }

    #[test]
    fn void_method_needs_no_return() {
        assert!(check("void m() { int x = 1; }").is_empty());
    }

    #[test]
    fn non_void_without_return_is_flagged() {
        let d = check("int m() { int x = 1; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("Missing return"));
        assert!(d[0].message.contains("int"));
    }

    #[test]
    fn trailing_return_is_ok() {
        assert!(check("int m() { return 1; }").is_empty());
    }

    #[test]
    fn throw_counts_as_returning() {
        assert!(check("int m() { throw new RuntimeException(); }").is_empty());
    }

    #[test]
    fn if_else_both_return_is_ok() {
        assert!(check("int m(boolean c) { if (c) return 1; else return 2; }").is_empty());
    }

    #[test]
    fn if_without_else_is_flagged() {
        let d = check("int m(boolean c) { if (c) return 1; }");
        assert_eq!(d.len(), 1, "an if with no else can fall through: {d:?}");
    }

    #[test]
    fn infinite_loop_is_not_flagged() {
        // Conservative: a loop as the last statement is assumed to guarantee the return.
        assert!(check("int m() { for (;;) {} }").is_empty());
        assert!(check("int m() { while (true) { } }").is_empty());
    }

    #[test]
    fn switch_and_try_are_not_flagged() {
        // Conservative: not fully modelled → never false-flagged.
        assert!(check("int m(int x) { switch (x) { default: return 0; } }").is_empty());
        assert!(check("int m() { try { return 1; } finally { } }").is_empty());
    }

    #[test]
    fn abstract_and_interface_methods_are_skipped() {
        assert!(check("abstract int m();").is_empty());
    }

    #[test]
    fn nested_block_trailing_return_is_ok() {
        assert!(check("int m() { { return 1; } }").is_empty());
    }

    #[test]
    fn generic_return_type_without_return_is_flagged() {
        let d = check("java.util.List<String> m() { int x = 1; }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // ── return-statement shape (value vs void) ─────────────────────────────────

    fn ret_errs(members: &str) -> Vec<String> {
        let src = format!("class C {{ {members} }}");
        let tree = parse(&src);
        return_statement_errors(tree.root_node(), &src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn value_from_void_method_is_flagged() {
        let e = ret_errs("void m() { return 1; }");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("void"), "{e:?}");
    }

    #[test]
    fn bare_return_from_void_is_ok() {
        assert!(ret_errs("void m(boolean c) { if (c) return; }").is_empty());
    }

    #[test]
    fn bare_return_from_non_void_is_flagged() {
        let e = ret_errs("int m(boolean c) { if (c) return; return 1; }");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("Missing return value") && e[0].contains("int"), "{e:?}");
    }

    #[test]
    fn value_from_constructor_is_flagged() {
        let e = ret_errs("C() { return 1; }");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("constructor"), "{e:?}");
    }

    #[test]
    fn bare_return_from_constructor_is_ok() {
        assert!(ret_errs("C(boolean c) { if (c) return; }").is_empty());
    }

    #[test]
    fn return_inside_lambda_is_judged_against_the_lambda() {
        // The void method's body has a value-returning `return` — but it's inside a Supplier lambda,
        // so it belongs to the lambda, NOT the void method. Must not be flagged.
        let e = ret_errs(
            "void m() { java.util.function.Supplier<Integer> s = () -> { return 1; }; }",
        );
        assert!(e.is_empty(), "{e:?}");
    }

    #[test]
    fn value_returning_method_with_value_is_ok() {
        assert!(ret_errs("int m() { return 1; }").is_empty());
    }
}
