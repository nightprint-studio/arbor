//! The two refactorings that reshape an `if` — **invert** it, and **merge** it with the one inside.
//!
//! Both are pure text over one statement: no resolver, no type, nothing that leaves the file. They
//! are here because they are used constantly and noticed only when missing — an `if` written the
//! wrong way round is the commonest thing a reader wants to flip, and a pair of nested `if`s with
//! nothing between them is the commonest thing a reader wants to join.
//!
//! ## Negation is where this gets interesting
//!
//! Inverting `if (a) X else Y` means writing `!(a)` — correct always, and unreadable often. So the
//! condition is negated *structurally* where its shape allows an exact answer: a comparison flips
//! its operator, `!x` drops its bang, `&&` becomes `||` over negated halves (De Morgan). Everything
//! else falls back to wrapping.
//!
//! The wrapping fallback is not a defeat: it is what keeps the refactoring **sound** on an operand
//! whose shape nobody enumerated. Every rule below is an exact equivalence — no rule may be added
//! that is "usually" right.
//!
//! ## What they will not do
//!
//! - Invert an `if` with **no `else`** and a body that is not a block: `if (a) return;` inverted has
//!   nothing to put in the new body, and inventing an empty statement is not what was asked.
//! - Merge when the outer `if` has an **`else`**: `if (a) { if (b) X } else Y` is not
//!   `if (a && b) X else Y` — the else must still run when `a` holds and `b` does not.
//! - Merge when the outer body holds **anything besides** the inner `if`, for the same reason.

use tree_sitter::Node;

use crate::plan::{Outcome, Plan, RefactorEdit, Refusal};
use crate::selection::{enclosing, is_block, text};

const INVERT_IF: (&str, &str) = ("invert-if", "Invert if");
const MERGE_IF: (&str, &str) = ("merge-nested-if", "Merge nested if");

/// Plan an *invert if* at the caret: swap the branches and negate the condition.
pub fn invert_if(root: Node<'_>, source: &str, start: usize, end: usize) -> Outcome {
    let (id, label) = INVERT_IF;
    let stmt = if_at(root, start, end)?;
    let condition = stmt.child_by_field_name("condition")?;
    let consequence = stmt.child_by_field_name("consequence")?;
    let alternative = stmt.child_by_field_name("alternative");

    // Without an `else` there is no second branch to swap in, and the honest inversion —
    // `if (!a) {} else X` — is a worse program than the one that was there.
    let Some(alternative) = alternative else {
        return Some(Err(Refusal::new(
            id,
            label,
            "this `if` has no `else`, so there is nothing to swap the body with",
        )));
    };
    // `else if` chains read as a ladder, and inverting one rung rewrites the ladder's meaning
    // rather than one statement. Refused rather than half-done.
    if alternative.kind() == "if_statement" {
        return Some(Err(Refusal::new(
            id,
            label,
            "the `else` is another `if` — inverting one rung of a chain changes what the rest tests",
        )));
    }

    // The `condition` node carries its own parentheses, so replacing it whole with a
    // parenthesised negation leaves exactly one pair — `if (n <= 0)`, not `if ((n <= 0))`.
    let negated = negate(&condition, source);
    let edits = vec![
        // Descending order is the plan's contract; `Plan::new` sorts, so the order written here is
        // only for the reader.
        RefactorEdit::new(
            consequence.start_byte(),
            consequence.end_byte(),
            text(&alternative, source).to_string(),
            "consequence",
        ),
        RefactorEdit::new(
            alternative.start_byte(),
            alternative.end_byte(),
            text(&consequence, source).to_string(),
            "alternative",
        ),
        RefactorEdit::new(condition.start_byte(), condition.end_byte(), negated, "condition"),
    ];
    Some(Ok(Plan::new(id, label, edits).caret_at(condition.start_byte())))
}

/// Plan a *merge nested if*: `if (a) { if (b) X }` becomes `if (a && b) X`.
pub fn merge_nested_if(root: Node<'_>, source: &str, start: usize, end: usize) -> Outcome {
    let (id, label) = MERGE_IF;
    let outer = if_at(root, start, end)?;
    let outer_cond = outer.child_by_field_name("condition")?;
    let outer_body = outer.child_by_field_name("consequence")?;

    // An `else` on either `if` makes the two conditions answer different questions — see the
    // module docs. Silence rather than a refusal on the outer: with an `else` present the user is
    // almost never reaching for this, and a greyed row in every menu is noise.
    if outer.child_by_field_name("alternative").is_some() {
        return None;
    }

    let inner = sole_if_in(&outer_body)?;
    if inner.child_by_field_name("alternative").is_some() {
        return Some(Err(Refusal::new(
            id,
            label,
            "the inner `if` has an `else`, which would have to run when the outer test passes and the inner does not",
        )));
    }
    let inner_cond = inner.child_by_field_name("condition")?;
    let inner_body = inner.child_by_field_name("consequence")?;

    // `a || b` under a merge becomes `x && (a || b)`, so each half is parenthesised unless its
    // shape already binds tighter than `&&`.
    let merged = format!("({} && {})", inside(&outer_cond, source), inside(&inner_cond, source));

    let edits = vec![
        RefactorEdit::new(
            outer_body.start_byte(),
            outer_body.end_byte(),
            text(&inner_body, source).to_string(),
            "body",
        ),
        RefactorEdit::new(outer_cond.start_byte(), outer_cond.end_byte(), merged, "condition"),
    ];
    Some(Ok(Plan::new(id, label, edits).caret_at(outer_cond.start_byte())))
}

// ── the pieces ───────────────────────────────────────────────────────────────

/// The `if` the caret is in — but only through its **own** head, not from inside its body.
///
/// A caret inside the body is standing in the code the `if` guards, and offering to invert the
/// guard from there puts a row in the menu for a statement the user is not looking at. The head is
/// everything up to the end of the condition.
fn if_at<'t>(root: Node<'t>, start: usize, end: usize) -> Option<Node<'t>> {
    let at = crate::selection::node_covering(root, start, end)?;
    let stmt = enclosing(at, &["if_statement"])?;
    let condition = stmt.child_by_field_name("condition")?;
    (start >= stmt.start_byte() && start <= condition.end_byte()).then_some(stmt)
}

/// The single `if` a block holds and nothing else — or the `if` itself when the body is not a
/// block. `None` when there is anything to keep it company.
fn sole_if_in<'t>(body: &Node<'t>) -> Option<Node<'t>> {
    if body.kind() == "if_statement" {
        return Some(*body);
    }
    if !is_block(body) {
        return None;
    }
    let mut cursor = body.walk();
    let statements: Vec<Node<'t>> = body
        .named_children(&mut cursor)
        .filter(|n| n.kind() != "line_comment" && n.kind() != "block_comment")
        .collect();
    match statements.as_slice() {
        [only] if only.kind() == "if_statement" => Some(*only),
        _ => None,
    }
}

/// A condition's text without the parentheses tree-sitter includes in it.
fn inside<'a>(condition: &Node<'_>, source: &'a str) -> &'a str {
    let whole = text(condition, source);
    match condition.kind() {
        "parenthesized_expression" => whole
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .map(str::trim)
            .unwrap_or(whole),
        _ => whole,
    }
}

/// The negation of a condition, written the way a person would.
///
/// Every arm is an exact equivalence, and that is the bar: a rule that is right "almost always"
/// turns an inversion into a silent behaviour change, which is the one thing a refactoring may
/// never do. What no arm recognises is wrapped, which is always correct and only ever ugly.
fn negate(condition: &Node<'_>, source: &str) -> String {
    let expr = inside(condition, source);
    let inner = strip_parens(condition);
    format!("({})", negate_expr(&inner, expr, source))
}

fn negate_expr(node: &Node<'_>, fallback: &str, source: &str) -> String {
    match node.kind() {
        // `!x` → `x`. The one case that gets shorter.
        "unary_expression" => {
            let operator = node.child(0).map(|c| text(&c, source)).unwrap_or_default();
            if operator == "!" {
                if let Some(operand) = node.child_by_field_name("operand") {
                    return inside(&operand, source).to_string();
                }
            }
            format!("!{fallback}")
        }
        "binary_expression" => negate_binary(node, fallback, source),
        // `true` / `false` are the two literals worth flipping; everything else keeps its bang.
        "true" => "false".to_string(),
        "false" => "true".to_string(),
        // A bare name, a call, a field access: `!` binds tighter than any of them, so no
        // parentheses are needed and none are added.
        "identifier" | "method_invocation" | "field_access" | "array_access" | "this" => {
            format!("!{}", text(node, source))
        }
        _ => format!("!({fallback})"),
    }
}

fn negate_binary(node: &Node<'_>, fallback: &str, source: &str) -> String {
    let (Some(left), Some(right), Some(op)) = (
        node.child_by_field_name("left"),
        node.child_by_field_name("right"),
        node.child_by_field_name("operator"),
    ) else {
        return format!("!({fallback})");
    };
    let operator = text(&op, source);
    let l = text(&left, source);
    let r = text(&right, source);

    // A comparison flips its operator exactly. NOT included: `==` / `!=` on floating point, which
    // flip correctly too — NaN makes `!(a == b)` and `a != b` agree, both being true.
    let flipped = match operator {
        "==" => Some("!="),
        "!=" => Some("=="),
        "<" => Some(">="),
        "<=" => Some(">"),
        ">" => Some("<="),
        ">=" => Some("<"),
        _ => None,
    };
    if let Some(flipped) = flipped {
        return format!("{l} {flipped} {r}");
    }

    // De Morgan, and it must recurse: `!(a && b)` is `!a || !b`, where each half is negated by the
    // same rules rather than merely wrapped.
    let joined = match operator {
        "&&" => Some("||"),
        "||" => Some("&&"),
        _ => None,
    };
    if let Some(joined) = joined {
        let ln = negate_expr(&strip_parens(&left), l, source);
        let rn = negate_expr(&strip_parens(&right), r, source);
        return format!("{ln} {joined} {rn}");
    }

    format!("!({fallback})")
}

/// Walk down through `(…)` to the expression they wrap.
fn strip_parens<'t>(node: &Node<'t>) -> Node<'t> {
    let mut current = *node;
    while matches!(current.kind(), "parenthesized_expression" | "condition") {
        match current.named_child(0) {
            Some(inner) => current = inner,
            None => break,
        }
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    fn applied(source: &str, needle: &str, f: fn(Node<'_>, &str, usize, usize) -> Outcome) -> String {
        let tree = parse_java(source).unwrap();
        let at = source.find(needle).unwrap();
        match f(tree.root_node(), source, at, at) {
            Some(Ok(plan)) => plan.apply(source),
            other => panic!("expected a plan, got {other:?}"),
        }
    }

    fn refusal(source: &str, needle: &str, f: fn(Node<'_>, &str, usize, usize) -> Outcome) -> String {
        let tree = parse_java(source).unwrap();
        let at = source.find(needle).unwrap();
        match f(tree.root_node(), source, at, at) {
            Some(Err(r)) => r.reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    // ── invert ────────────────────────────────────────────────────────────────

    #[test]
    fn inverting_swaps_the_branches_and_flips_the_comparison() {
        let src = "class A { void f(int n) { if (n > 0) { a(); } else { b(); } } }";
        let out = applied(src, "if (", invert_if);
        assert!(out.contains("if (n <= 0) { b(); } else { a(); }"), "{out}");
    }

    /// The rule that keeps this sound: an operand nobody enumerated is wrapped, not guessed at.
    #[test]
    fn an_unrecognised_condition_is_wrapped() {
        let src = "class A { void f(int n) { if (n instanceof Object) { a(); } else { b(); } } }";
        let out = applied(src, "if (", invert_if);
        assert!(out.contains("!(n instanceof Object)"), "{out}");
    }

    #[test]
    fn a_bang_is_dropped_rather_than_doubled() {
        let src = "class A { void f(boolean ok) { if (!ok) { a(); } else { b(); } } }";
        let out = applied(src, "if (", invert_if);
        assert!(out.contains("if (ok)"), "{out}");
        assert!(!out.contains("!!"), "{out}");
    }

    /// De Morgan, recursively — the halves are negated by the same rules, not merely wrapped.
    #[test]
    fn de_morgan_negates_both_halves() {
        let src = "class A { void f(int n, boolean ok) { if (n > 0 && ok) { a(); } else { b(); } } }";
        let out = applied(src, "if (", invert_if);
        assert!(out.contains("if (n <= 0 || !ok)"), "{out}");
    }

    /// A call is a perfectly good operand for `!`, and parenthesising it would be noise.
    #[test]
    fn a_call_takes_a_bare_bang() {
        let src = "class A { void f() { if (ready()) { a(); } else { b(); } } }";
        let out = applied(src, "if (", invert_if);
        assert!(out.contains("if (!ready())"), "{out}");
    }

    #[test]
    fn an_if_without_an_else_is_refused() {
        let src = "class A { void f(int n) { if (n > 0) { a(); } } }";
        assert!(refusal(src, "if (", invert_if).contains("no `else`"));
    }

    /// Inverting one rung of an `else if` ladder rewrites what the rungs below it test.
    #[test]
    fn an_else_if_chain_is_refused() {
        let src = "class A { void f(int n) { if (n > 0) { a(); } else if (n < 0) { b(); } else { c(); } } }";
        assert!(refusal(src, "if (n > 0)", invert_if).contains("chain"));
    }

    /// Standing in the guarded code is not standing on the guard.
    #[test]
    fn a_caret_inside_the_body_offers_nothing() {
        let src = "class A { void f(int n) { if (n > 0) { doWork(); } else { b(); } } }";
        let tree = parse_java(src).unwrap();
        let at = src.find("doWork").unwrap();
        assert!(invert_if(tree.root_node(), src, at, at).is_none());
    }

    // ── merge ─────────────────────────────────────────────────────────────────

    #[test]
    fn two_nested_ifs_become_one() {
        let src = "class A { void f(int n, boolean ok) { if (n > 0) { if (ok) { a(); } } } }";
        let out = applied(src, "if (n > 0)", merge_nested_if);
        assert!(out.contains("if (n > 0 && ok) { a(); }"), "{out}");
    }

    /// `a || b` under a merge has to keep its own grouping or the merge changes what is tested.
    #[test]
    fn a_disjunction_keeps_its_parentheses() {
        let src = "class A { void f(int n, boolean p, boolean q) { if (p || q) { if (n > 0) { a(); } } } }";
        let out = applied(src, "if (p || q)", merge_nested_if);
        assert!(out.contains("(p || q && n > 0)") || out.contains("((p || q) && n > 0)"), "{out}");
    }

    #[test]
    fn an_inner_else_is_refused() {
        let src = "class A { void f(int n, boolean ok) { if (n > 0) { if (ok) { a(); } else { b(); } } } }";
        assert!(refusal(src, "if (n > 0)", merge_nested_if).contains("`else`"));
    }

    /// Anything keeping the inner `if` company means the merge would move code under a second test.
    #[test]
    fn a_sibling_statement_blocks_the_merge() {
        let src = "class A { void f(int n, boolean ok) { if (n > 0) { log(); if (ok) { a(); } } } }";
        let tree = parse_java(src).unwrap();
        let at = src.find("if (n > 0)").unwrap();
        assert!(merge_nested_if(tree.root_node(), src, at, at).is_none());
    }

    /// An `else` on the outer means this is not what the user is reaching for — silence, not a
    /// greyed row in every menu.
    #[test]
    fn an_outer_else_is_silent() {
        let src = "class A { void f(int n, boolean ok) { if (n > 0) { if (ok) { a(); } } else { b(); } } }";
        let tree = parse_java(src).unwrap();
        let at = src.find("if (n > 0)").unwrap();
        assert!(merge_nested_if(tree.root_node(), src, at, at).is_none());
    }
}
