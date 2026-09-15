//! "Missing return statement" diagnostics — a non-`void` method whose body can fall off the end
//! without returning.
//!
//! This is the *reachability* half of return checking (JLS §14.17 / §8.4.7): does every path out of
//! the body end in a `return` or `throw`? The *type* half (the returned value matches the declared
//! type) needs the resolver and is a later phase.
//!
//! The reachability rules are JLS §14.22's "can complete normally", followed statement by statement
//! (loops with their `break`s and constant conditions, `switch` blocks, `try`/`catch`/`finally`,
//! labels). **Conservative by construction**: a shape the analysis does not model answers "cannot
//! complete normally", which can only silence a report, never invent one.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Flag every non-`void` method whose body can complete without a `return`/`throw`.
pub fn missing_return(root: Node, source: &str) -> Vec<Diagnostic> {
    missing_return_nodes(&crate::engine::check::collect_nodes(root), source)
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
        if block_can_complete_normally(body, bytes) {
            let ty = ret.utf8_text(bytes).unwrap_or("").trim();
            // On the closing brace, where javac and IntelliJ put it: that is where control falls out.
            let end = body.end_byte();
            out.push(crate::engine::check_id::CheckId::MissingReturn.span(
                end.saturating_sub(1),
                end,
                format!("Missing return statement (method must return `{ty}`)"),
            ));
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
    return_statement_errors_nodes(&crate::engine::check::collect_nodes(root), source)
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

/// A block lambda body that returns a value on one path and can complete normally on another.
///
/// Such a body is neither value-compatible (it can fall off its end) nor void-compatible (it returns
/// a value) — JLS §15.27.2 — so no functional interface can type it, whatever the target. javac says
/// so as an incompatible return type; the report sits on the closing brace, where control falls out.
pub fn lambda_body_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() != "lambda_expression" {
            continue;
        }
        let Some(body) = n.child_by_field_name("body").filter(|b| b.kind() == "block" && !b.has_error()) else {
            continue;
        };
        let mut returns = Vec::new();
        collect_returns(body, &mut returns);
        if returns.iter().any(|r| has_return_value(*r)) && block_can_complete_normally(body, bytes) {
            let end = body.end_byte();
            out.push(crate::engine::check_id::CheckId::IncompatibleType.span(
                end.saturating_sub(1),
                end,
                "This lambda returns a value, but its body can also complete without returning one",
            ));
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
                severity: crate::engine::check_id::CheckId::ReturnValueFromVoid.severity().to_string(),
                code: crate::engine::check_id::CheckId::ReturnValueFromVoid.code().to_string(),
                start: r.start_byte(),
                end: r.end_byte(),
            });
        } else if !is_void && !has_value {
            out.push(Diagnostic {
                message: format!("Missing return value (method must return `{ret_ty}`)"),
                severity: crate::engine::check_id::CheckId::MissingReturn.severity().to_string(),
                code: crate::engine::check_id::CheckId::MissingReturn.code().to_string(),
                start: r.start_byte(),
                end: r.end_byte(),
            });
        }
    }
}

/// Recursively collect the `return_statement`s directly governed by the current method — descending
/// through control flow but stopping at any construct that introduces its own return target.
pub(crate) fn collect_returns<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) {
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
pub(crate) fn has_return_value(ret: Node) -> bool {
    let mut c = ret.walk();
    for n in ret.named_children(&mut c) {
        if !matches!(n.kind(), "line_comment" | "block_comment") {
            return true;
        }
    }
    false
}

/// JLS §14.22: can `stmt` complete normally? A method body that can is missing its return.
///
/// The rules are javac's, not an approximation of them — which is what lets this report the loops,
/// switches and `try`s the old last-statement heuristic had to wave through. The one place the
/// language itself needs more than the tree is a loop condition that is a *constant expression*
/// (`while (DEBUG)` with a `static final boolean`): that is decided by
/// [`never_constant_true`], which says "not constant" only when it can prove it.
///
/// Anything this does not model answers **false** ("cannot complete normally"). That is the silent
/// side: a `false` can only ever remove a report, because every rule combines a child's answer with
/// `||` towards a report only when another child definitely completes.
pub(crate) fn can_complete_normally(stmt: Node, bytes: &[u8]) -> bool {
    match stmt.kind() {
        "return_statement" | "throw_statement" | "break_statement" | "continue_statement"
        | "yield_statement" => false,
        "block" => block_can_complete_normally(stmt, bytes),
        "local_variable_declaration" | "expression_statement" | "assert_statement" | "enhanced_for_statement"
        | "class_declaration" | "record_declaration" | "interface_declaration" | "enum_declaration"
        | "explicit_constructor_invocation" | ";" => true,
        "if_statement" => match stmt.child_by_field_name("alternative") {
            None => true,
            Some(alt) => {
                stmt.child_by_field_name("consequence").is_some_and(|c| can_complete_normally(c, bytes))
                    || can_complete_normally(alt, bytes)
            }
        },
        "labeled_statement" => {
            let Some(inner) = labeled_body(stmt) else { return false };
            can_complete_normally(inner, bytes) || label_name(stmt, bytes).is_some_and(|l| has_labeled_exit(inner, "break_statement", l, bytes))
        }
        "synchronized_statement" => stmt.child_by_field_name("body").is_some_and(|b| can_complete_normally(b, bytes)),
        "while_statement" => {
            let never_true = stmt.child_by_field_name("condition").is_some_and(|c| never_constant_true(c, bytes));
            never_true || breaks_out_of(stmt)
        }
        "for_statement" => {
            // No condition is `for (;;)`, a constant `true`.
            let never_true = stmt.child_by_field_name("condition").is_some_and(|c| never_constant_true(c, bytes));
            never_true || breaks_out_of(stmt)
        }
        "do_statement" => {
            if breaks_out_of(stmt) {
                return true;
            }
            let never_true = stmt.child_by_field_name("condition").is_some_and(|c| never_constant_true(c, bytes));
            let body_completes = stmt.child_by_field_name("body").is_some_and(|b| can_complete_normally(b, bytes))
                || continues_in(stmt, bytes);
            never_true && body_completes
        }
        "try_statement" | "try_with_resources_statement" => try_can_complete_normally(stmt, bytes),
        "switch_expression" => switch_statement_can_complete_normally(stmt, bytes),
        _ => false,
    }
}

/// A block completes normally iff every statement in it does — an earlier statement that cannot
/// makes the rest unreachable, which is its own error and not a missing return. Empty completes.
pub(crate) fn block_can_complete_normally(block: Node, bytes: &[u8]) -> bool {
    let mut c = block.walk();
    for ch in block.named_children(&mut c) {
        if matches!(ch.kind(), "line_comment" | "block_comment") {
            continue;
        }
        if !can_complete_normally(ch, bytes) {
            return false;
        }
    }
    true
}

/// `try` completes normally iff the `try` block or some `catch` block does — and a `finally`, when
/// present, does too (a `finally` that cannot complete overrides everything before it).
fn try_can_complete_normally(stmt: Node, bytes: &[u8]) -> bool {
    let mut body_or_catch = stmt.child_by_field_name("body").is_some_and(|b| can_complete_normally(b, bytes));
    let mut c = stmt.walk();
    for ch in stmt.named_children(&mut c) {
        match ch.kind() {
            "catch_clause" => {
                body_or_catch |= ch.child_by_field_name("body").is_some_and(|b| can_complete_normally(b, bytes));
            }
            "finally_clause" => {
                let completes = crate::support::nodes::child_of_kind(ch, "block")
                    .is_some_and(|b| can_complete_normally(b, bytes));
                if !completes {
                    return false;
                }
            }
            _ => {}
        }
    }
    body_or_catch
}

/// A `switch` statement (JLS §14.22, both block shapes).
///
/// It completes normally when control can leave the block at its end (the last group completes, a
/// trailing label with no statements, an expression rule, a rule block that completes), when a
/// `break` leaves it, or when no label matches at all — which, for a switch without `default`, is
/// possible unless the switch is required to be exhaustive. Pattern and `null` labels make it an
/// *enhanced* switch that must be, so there the missing `default` proves nothing and the answer is
/// the silent one.
fn switch_statement_can_complete_normally(stmt: Node, bytes: &[u8]) -> bool {
    let Some(block) = stmt.child_by_field_name("body") else { return false };
    if breaks_out_of(stmt) {
        return true;
    }
    let mut has_default = false;
    let mut enhanced = false;
    let mut end_reachable = false;
    let mut c = block.walk();
    let children: Vec<Node> = block.named_children(&mut c).filter(|n| !matches!(n.kind(), "line_comment" | "block_comment")).collect();
    if children.is_empty() {
        return true;
    }
    for (i, item) in children.iter().enumerate() {
        let mut ic = item.walk();
        let parts: Vec<Node> = item.named_children(&mut ic).filter(|n| !matches!(n.kind(), "line_comment" | "block_comment")).collect();
        for label in parts.iter().filter(|p| p.kind() == "switch_label") {
            let (d, e) = classify_label(*label, bytes);
            has_default |= d;
            enhanced |= e;
        }
        match item.kind() {
            "switch_rule" => {
                if let Some(action) = parts.iter().find(|p| p.kind() != "switch_label") {
                    end_reachable |= match action.kind() {
                        "expression_statement" => true,
                        "block" => can_complete_normally(*action, bytes),
                        _ => false,
                    };
                }
            }
            "switch_block_statement_group" if i + 1 == children.len() => {
                // The last group: its statements run into the end of the block.
                let statements: Vec<&Node> = parts.iter().filter(|p| p.kind() != "switch_label").collect();
                end_reachable |= statements.is_empty()
                    || statements.iter().all(|s| can_complete_normally(**s, bytes));
            }
            "switch_block_statement_group" => {}
            _ => return false,
        }
    }
    end_reachable || (!has_default && !enhanced)
}

/// `(is default, makes the switch enhanced)` for one `switch_label`.
pub(crate) fn classify_label(label: Node, bytes: &[u8]) -> (bool, bool) {
    let mut is_default = false;
    let mut enhanced = false;
    let mut c = label.walk();
    for ch in label.children(&mut c) {
        match ch.kind() {
            "default" => is_default = true,
            "pattern" | "type_pattern" | "record_pattern" | "guard" | "null_literal" => enhanced = true,
            _ if ch.utf8_text(bytes) == Ok("null") => enhanced = true,
            _ => {}
        }
    }
    (is_default, enhanced)
}

/// Kinds that start a new `break`/`continue` target, or a new body altogether.
fn is_breakable(kind: &str) -> bool {
    matches!(kind, "while_statement" | "do_statement" | "for_statement" | "enhanced_for_statement" | "switch_expression")
}

fn is_body_boundary(kind: &str) -> bool {
    matches!(kind, "lambda_expression" | "class_body" | "method_declaration" | "constructor_declaration")
}

/// Whether an unlabeled `break` inside `target` leaves `target` itself (its nearest breakable).
fn breaks_out_of(target: Node) -> bool {
    has_unlabeled_exit(target, "break_statement", is_breakable)
}

/// Whether an unlabeled `continue` inside the `do` loop `target` continues `target` itself, or a
/// labeled one names the label written directly on it.
fn continues_in(target: Node, bytes: &[u8]) -> bool {
    let is_loop = |k: &str| matches!(k, "while_statement" | "do_statement" | "for_statement" | "enhanced_for_statement");
    if has_unlabeled_exit(target, "continue_statement", is_loop) {
        return true;
    }
    let labeled = target.parent().filter(|p| p.kind() == "labeled_statement");
    labeled
        .and_then(|l| label_name(l, bytes))
        .is_some_and(|name| has_labeled_exit(target, "continue_statement", name, bytes))
}

fn has_unlabeled_exit(target: Node, exit_kind: &str, is_target_kind: impl Fn(&str) -> bool) -> bool {
    let mut stack: Vec<Node> = Vec::new();
    let mut c = target.walk();
    stack.extend(target.named_children(&mut c));
    while let Some(n) = stack.pop() {
        if n.kind() == exit_kind {
            if crate::support::nodes::child_of_kind(n, "identifier").is_none() {
                return true;
            }
            continue;
        }
        if is_target_kind(n.kind()) || is_body_boundary(n.kind()) {
            continue; // an exit in there belongs to the nested statement
        }
        let mut cc = n.walk();
        stack.extend(n.named_children(&mut cc));
    }
    false
}

fn has_labeled_exit(scope: Node, exit_kind: &str, label: &str, bytes: &[u8]) -> bool {
    let mut stack = vec![scope];
    while let Some(n) = stack.pop() {
        if n.kind() == exit_kind {
            if crate::support::nodes::child_of_kind(n, "identifier").and_then(|i| i.utf8_text(bytes).ok()) == Some(label) {
                return true;
            }
            continue;
        }
        if is_body_boundary(n.kind()) {
            continue;
        }
        let mut cc = n.walk();
        stack.extend(n.named_children(&mut cc));
    }
    false
}

fn label_name<'b>(labeled: Node, bytes: &'b [u8]) -> Option<&'b str> {
    crate::support::nodes::child_of_kind(labeled, "identifier").and_then(|i| i.utf8_text(bytes).ok())
}

/// The statement a `labeled_statement` labels — its last named child (the first is the label).
fn labeled_body(labeled: Node) -> Option<Node> {
    let mut c = labeled.walk();
    labeled.named_children(&mut c).filter(|n| !matches!(n.kind(), "line_comment" | "block_comment")).skip(1).last()
}

/// Whether a loop condition is provably NOT the constant `true` (JLS §15.29).
///
/// A constant expression is built from literals and constant variables only, so one operand that is
/// certainly not constant settles it: a call, an assignment, `instanceof`, an object creation — or a
/// name bound to a parameter or a non-`final` local of the enclosing body. Any other name may be a
/// `static final` constant somewhere, so on its own it proves nothing.
pub(crate) fn never_constant_true(cond: Node, bytes: &[u8]) -> bool {
    let expr = unwrap_parens(cond);
    if expr.kind() == "false" {
        return true;
    }
    let mut stack = vec![expr];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "method_invocation" | "object_creation_expression" | "array_creation_expression" | "array_access"
            | "assignment_expression" | "update_expression" | "instanceof_expression" | "lambda_expression"
            | "method_reference" | "this" | "super" => return true,
            "identifier" if names_a_variable(n, bytes) => return true,
            // `a.b` may be a qualified constant: its parts are not variable reads of their own.
            "field_access" => continue,
            _ => {}
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    false
}

pub(crate) fn unwrap_parens(mut n: Node) -> Node {
    while n.kind() == "parenthesized_expression" {
        match n.named_child(0) {
            Some(inner) => n = inner,
            None => break,
        }
    }
    n
}

/// Whether the identifier `id` reads a parameter, or a non-`final` local declared before it in an
/// enclosing block — neither of which can ever be a constant variable.
fn names_a_variable(id: Node, bytes: &[u8]) -> bool {
    let Ok(name) = id.utf8_text(bytes) else { return false };
    let mut cur = id.parent();
    while let Some(n) = cur {
        match n.kind() {
            "block" => {
                let mut c = n.walk();
                for stmt in n.named_children(&mut c) {
                    if stmt.start_byte() >= id.start_byte() {
                        break;
                    }
                    if stmt.kind() == "local_variable_declaration" && declares(stmt, name, bytes) {
                        return !crate::support::nodes::has_keyword(stmt, bytes, "final");
                    }
                }
            }
            "method_declaration" | "constructor_declaration" | "lambda_expression" => {
                let params = n.child_by_field_name("parameters");
                if params.is_some_and(|p| declares_param(p, name, bytes)) {
                    return true;
                }
                if n.kind() != "lambda_expression" {
                    return false;
                }
            }
            "class_body" => return false,
            _ => {}
        }
        cur = n.parent();
    }
    false
}

fn declares(decl: Node, name: &str, bytes: &[u8]) -> bool {
    let mut c = decl.walk();
    let found = decl.named_children(&mut c).any(|d| {
        d.kind() == "variable_declarator"
            && d.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()) == Some(name)
    });
    found
}

fn declares_param(params: Node, name: &str, bytes: &[u8]) -> bool {
    if params.kind() == "identifier" {
        return params.utf8_text(bytes) == Ok(name);
    }
    let mut c = params.walk();
    let found = params.named_children(&mut c).any(|p| match p.kind() {
        "identifier" => p.utf8_text(bytes) == Ok(name),
        "formal_parameter" | "spread_parameter" => {
            p.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()) == Some(name)
                || crate::support::nodes::child_of_kind(p, "variable_declarator")
                    .and_then(|d| d.child_by_field_name("name"))
                    .and_then(|x| x.utf8_text(bytes).ok())
                    == Some(name)
        }
        _ => false,
    });
    found
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
    fn loops_that_can_exit_are_flagged() {
        assert_eq!(check("int m(int[] a) { for (int v : a) { return v; } }").len(), 1);
        assert_eq!(check("int m(boolean f) { while (f) { return 1; } }").len(), 1);
        assert_eq!(check("int m() { while (true) { break; } }").len(), 1);
        assert_eq!(check("int m() { outer: for (;;) { for (;;) { break outer; } } }").len(), 1);
    }

    #[test]
    fn a_name_that_may_be_a_constant_is_not_flagged() {
        // `T` may be a `static final boolean` somewhere: `while (T)` may never exit.
        assert!(check("int m() { while (T) { } }").is_empty());
        // A final local with a constant initializer IS a constant variable.
        assert!(check("int m() { final boolean t = true; while (t) { } }").is_empty());
    }

    #[test]
    fn continue_to_an_outer_infinite_loop_is_not_an_exit() {
        assert!(check("int m() { outer: while (true) { for (int i = 0; i < 3; i++) { continue outer; } } }").is_empty());
    }

    #[test]
    fn switch_without_default_is_flagged() {
        assert_eq!(check("int m(int k) { switch (k) { case 1: return 1; case 2: return 2; } }").len(), 1);
        assert_eq!(check("int m(int k) { switch (k) { case 1 -> { return 1; } } }").len(), 1);
        assert!(check("int m(int k) { switch (k) { case 1 -> { return 1; } default -> throw new RuntimeException(); } }").is_empty());
    }

    #[test]
    fn pattern_switch_without_default_is_not_flagged() {
        // Enhanced switches must be exhaustive, so the missing `default` proves nothing.
        assert!(check("int m(Object o) { switch (o) { case String s -> { return 1; } case Object x -> { return 2; } } }").is_empty());
    }

    #[test]
    fn catch_that_falls_through_is_flagged() {
        assert_eq!(check("int m() { try { return 1; } catch (RuntimeException e) { } }").len(), 1);
        assert!(check("int m() { try { } finally { throw new RuntimeException(); } }").is_empty());
    }

    #[test]
    fn missing_return_sits_on_the_closing_brace() {
        let src = "class C { int m() { int x = 1; } }";
        let tree = parse(src);
        let d = missing_return(tree.root_node(), src);
        assert_eq!(&src[d[0].start..d[0].end], "}");
        assert_eq!(d[0].start, src.find("} }").unwrap());
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
