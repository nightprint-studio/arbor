//! Control-flow *smells* in `switch` and `try`/`finally` (pure-AST, both **warnings** — legal Java
//! the compiler accepts but that almost always hides a bug):
//!
//!   * **switch fall-through** — an old-style (colon) `case`/`default` group that runs off its end
//!     into the next label because it lacks a `break`/`return`/`throw`/`continue`/`yield`. Arrow
//!     (`case X ->`) groups never fall through, so they're never considered.
//!   * **abnormal completion in `finally`** — a `return`/`break`/`continue` that completes the
//!     `finally` block abnormally, silently discarding any pending exception or return from the
//!     `try`/`catch`.
//!
//! PARAMOUNT rule: never a false positive. Both checks are deliberately *narrow* — they fire only on
//! shapes that are unambiguously the smell, and SKIP anything they can't fully prove. See the
//! per-check comments for exactly what is (and isn't) flagged.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

const FALLTHROUGH_MSG: &str = "Fall-through: this `case` continues into the next without `break`";
const FINALLY_MSG: &str =
    "`return`/`break`/`continue` in `finally` discards any pending exception or result";

/// Parse-free entry: flag switch fall-through and abnormal-completion-in-`finally` over a shared
/// pre-collected node slice (one traversal across all pure-AST checks).
pub fn switch_flow_warnings_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            // A `switch` (statement OR expression) shares the `switch_expression` kind in
            // tree-sitter-java; the colon-vs-arrow distinction lives in the body's group kinds.
            "switch_expression" if !n.has_error() => check_switch_fallthrough(n, source, &mut out),
            "finally_clause" if !n.has_error() => check_finally(n, &mut out),
            _ => {}
        }
    }
    out
}

// ── 1. switch fall-through ───────────────────────────────────────────────────
//
// Only colon-style groups (`switch_block_statement_group`) can fall through; arrow rules
// (`switch_rule`) each own a single statement/block and break implicitly, so a body made of
// `switch_rule`s is skipped wholesale.
//
// A group is flagged only when ALL hold:
//   (a) it is NOT the last group in the switch      — the last group has nowhere to fall to;
//   (b) it has ≥1 real statement                    — an empty group stacked above another label
//                                                      (`case A:` then `case B:`) is intentional;
//   (c) its LAST statement is a "plain" statement    — an expression statement, a local var decl:
//                                                      control provably reaches the group's end and
//                                                      slides into the next label.
// Anything whose last statement is a control structure we can't fully analyse (an `if` without a
// proven-terminating shape, a loop, a nested switch, a labelled/synchronized/try block) is SKIPPED —
// we'd rather miss a real fall-through than risk flagging one that actually terminates.

fn check_switch_fallthrough(switch: Node, _source: &str, out: &mut Vec<Diagnostic>) {
    let Some(body) = switch.child_by_field_name("body") else { return };

    // Collect only the colon-style groups, in order. If the body is arrow-style there are none.
    let mut groups: Vec<Node> = Vec::new();
    let mut c = body.walk();
    for ch in body.named_children(&mut c) {
        if ch.kind() == "switch_block_statement_group" {
            groups.push(ch);
        }
    }
    if groups.len() < 2 {
        return; // a single (hence last) group can't fall through
    }

    // Every group except the last is a fall-through candidate. Rule (a) is exactly "skip the last".
    for i in 0..groups.len() - 1 {
        let group = groups[i];
        let stmts = group_statements(group);
        // (b) empty group = intentional label stacking (`case A:` above `case B:`).
        let Some(&last) = stmts.last() else { continue };
        // (c) fires only when the last statement clearly slides off the end.
        if falls_off_end(last) {
            out.push(warn(FALLTHROUGH_MSG, group));
        }
    }
}

/// The executable statements of a colon-style group, in order — skipping the `switch_label`s and
/// comments that carry no control flow.
fn group_statements(group: Node) -> Vec<Node> {
    let mut v = Vec::new();
    let mut c = group.walk();
    for ch in group.named_children(&mut c) {
        if matches!(ch.kind(), "switch_label" | "line_comment" | "block_comment") {
            continue;
        }
        v.push(ch);
    }
    v
}

/// Whether `stmt` — the LAST statement of a group — provably lets control reach the group's end
/// (and thus fall into the next label). TRUE only for the "plain" statements that never divert
/// control: an expression statement, a local variable declaration, an empty statement, an assert.
///
/// Everything else answers FALSE and is skipped:
///   * terminators (`break`/`return`/`throw`/`continue`/`yield`) — the group can't reach its end;
///   * any control structure (`if`, loops, `switch`, `try`, `block`, labelled/synchronized) — we
///     don't fully model whether it terminates, so we stay silent rather than risk a false positive.
fn falls_off_end(stmt: Node) -> bool {
    matches!(
        stmt.kind(),
        "expression_statement" | "local_variable_declaration" | "empty_statement" | "assert_statement"
    )
}

// ── 2. abnormal completion in `finally` ──────────────────────────────────────
//
// A `return`/`break`/`continue` directly in the finally's control flow completes the block
// abnormally: any exception thrown (or value returned) by the `try`/`catch` is silently dropped.
//
// Soundness — we descend from the finally's `block` and flag only jumps that unquestionably target
// OUT of the finally:
//   * we STOP descending at every construct that owns its own jump target — a nested
//     `lambda_expression`, a local/anonymous `class_body`, or (for `break`/`continue`) a nested
//     loop / `switch` — so a `return` inside a lambda, or a `break` that targets an inner `for`, is
//     never attributed to the finally;
//   * a `return` is flagged wherever it's reached (methods/ctors are boundaries we already stopped
//     at, so any `return` still visited belongs to the enclosing method — i.e. the finally);
//   * `break`/`continue` are flagged only when NOT enclosed by a nested loop/switch within the
//     finally (otherwise they target that inner construct, which is fine).
// A labelled `break`/`continue` could target an OUTER loop even through a nested loop; we can't
// resolve the label cheaply, so we conservatively treat any label as inner-targeting and SKIP it.

fn check_finally(finally_clause: Node, out: &mut Vec<Diagnostic>) {
    // `finally_clause` = `finally` keyword + a `block`.
    let Some(block) = finally_clause.child_by_field_name("body").or_else(|| first_block(finally_clause))
    else {
        return;
    };
    let mut hits = Vec::new();
    scan_finally(block, false, &mut hits);
    for h in hits {
        out.push(warn(FINALLY_MSG, h));
    }
}

/// The first `block` child of a node (tree-sitter-java's `finally_clause` exposes its block as an
/// un-named-field child in some grammar versions — fall back to a scan).
fn first_block(n: Node) -> Option<Node> {
    let mut c = n.walk();
    for ch in n.named_children(&mut c) {
        if ch.kind() == "block" {
            return Some(ch);
        }
    }
    None
}

/// Walk the finally's body collecting the abnormal-completion jumps. `in_inner_loop` tracks whether
/// we're inside a loop/switch *nested within the finally* — the target of a bare `break`/`continue`.
fn scan_finally<'t>(node: Node<'t>, in_inner_loop: bool, hits: &mut Vec<Node<'t>>) {
    let mut c = node.walk();
    for ch in node.named_children(&mut c) {
        match ch.kind() {
            // A `return` inside the finally always leaves the finally (methods/ctors are boundaries
            // we stop at below, so any `return` reached here is the enclosing method's).
            "return_statement" => hits.push(ch),

            // `break`/`continue` only complete the FINALLY abnormally when they don't target a loop
            // or switch nested inside it. A labelled jump might reach an outer loop through an inner
            // one — we can't resolve the label here, so we SKIP any labelled jump (stay sound).
            "break_statement" | "continue_statement" => {
                if !in_inner_loop && !has_label(ch) {
                    hits.push(ch);
                }
                // don't descend — a jump has no sub-statements of interest
            }

            // Own jump target / own `return` target → never attribute their jumps to the finally.
            "lambda_expression"
            | "method_declaration"
            | "constructor_declaration"
            | "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
            | "class_body" => {}

            // A nested loop / switch is the target of a bare `break`/`continue` inside it: descend
            // with `in_inner_loop = true` so those jumps are NOT attributed to the finally. (A
            // `return` deeper still is flagged — the flag is passed through unchanged for it below.)
            "for_statement" | "enhanced_for_statement" | "while_statement" | "do_statement"
            | "switch_expression" => {
                scan_finally(ch, true, hits);
            }

            // Ordinary nesting (blocks, `if`, `try`, labelled, synchronized, …): keep the current
            // loop context and keep looking.
            _ => scan_finally(ch, in_inner_loop, hits),
        }
    }
}

/// Whether a `break`/`continue` carries a label (`break outer;`) — its first named child is an
/// `identifier`. A labelled jump may escape a nested loop to an outer one, which we can't resolve
/// cheaply, so the caller SKIPs it to stay sound.
fn has_label(jump: Node) -> bool {
    let mut c = jump.walk();
    for ch in jump.named_children(&mut c) {
        if ch.kind() == "identifier" {
            return true;
        }
    }
    false
}

// ── shared ───────────────────────────────────────────────────────────────────

fn warn(message: &str, node: Node) -> Diagnostic {
    Diagnostic {
        message: message.to_string(),
        severity: "warning".to_string(),
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn run(body: &str) -> Vec<String> {
        let src = format!("class C {{ void m() {{ {body} }} }}");
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(&src, None).unwrap();
        let nodes = crate::check::collect_nodes(tree.root_node());
        switch_flow_warnings_nodes(&nodes, &src).into_iter().map(|d| d.message).collect()
    }

    fn fallthroughs(body: &str) -> usize {
        run(body).iter().filter(|m| m.contains("Fall-through")).count()
    }
    fn finallys(body: &str) -> usize {
        run(body).iter().filter(|m| m.contains("finally")).count()
    }

    // ── fall-through: positives ─────────────────────────────────────────────────

    #[test]
    fn plain_expression_group_falls_through() {
        // `case 0:` does work then slides into `case 1:` with no break.
        let d = fallthroughs("switch (x) { case 0: doA(); case 1: doB(); break; }");
        assert_eq!(d, 1, "{:?}", run("switch (x) { case 0: doA(); case 1: doB(); break; }"));
    }

    #[test]
    fn local_decl_group_falls_through() {
        // A group ending in a local var declaration also reaches its end.
        assert_eq!(fallthroughs("switch (x) { case 0: int y = 1; default: doB(); break; }"), 1);
    }

    // ── fall-through: negatives (critical) ──────────────────────────────────────

    #[test]
    fn group_ending_in_break_is_ok() {
        assert_eq!(fallthroughs("switch (x) { case 0: doA(); break; case 1: doB(); break; }"), 0);
    }

    #[test]
    fn group_ending_in_return_or_throw_is_ok() {
        assert_eq!(fallthroughs("switch (x) { case 0: return; case 1: throw new RuntimeException(); }"), 0);
    }

    #[test]
    fn stacked_empty_labels_are_ok() {
        // `case A: case B: doX(); break;` — the empty `case A` is intentional stacking, not fall-through.
        assert_eq!(fallthroughs("switch (x) { case 0: case 1: doX(); break; default: doY(); }"), 0);
    }

    #[test]
    fn last_group_is_never_flagged() {
        // The final group runs off the switch's end, not into another label.
        assert_eq!(fallthroughs("switch (x) { case 0: doA(); break; default: doB(); }"), 0);
    }

    #[test]
    fn arrow_switch_never_falls_through() {
        // Arrow rules break implicitly — a body of `switch_rule`s yields no colon groups.
        assert_eq!(fallthroughs("switch (x) { case 0 -> doA(); case 1 -> doB(); }"), 0);
    }

    #[test]
    fn group_ending_in_if_is_skipped() {
        // Ends in an `if` we don't fully model → SKIP (sound: we'd rather miss than false-flag).
        assert_eq!(fallthroughs("switch (x) { case 0: if (b) doA(); case 1: doB(); break; }"), 0);
    }

    // ── finally: positives ──────────────────────────────────────────────────────

    #[test]
    fn return_in_finally_is_flagged() {
        assert_eq!(finallys("try { work(); } finally { return; }"), 1);
    }

    #[test]
    fn break_in_finally_of_loop_is_flagged() {
        // The `break` inside the finally targets the OUTER `for` — abnormal completion of the finally.
        let src = "for (;;) { try { work(); } finally { break; } }";
        assert_eq!(finallys(src), 1, "{:?}", run(src));
    }

    #[test]
    fn continue_in_finally_of_loop_is_flagged() {
        let src = "for (;;) { try { work(); } finally { continue; } }";
        assert_eq!(finallys(src), 1, "{:?}", run(src));
    }

    // ── finally: negatives (critical) ───────────────────────────────────────────

    #[test]
    fn normal_finally_is_ok() {
        assert_eq!(finallys("try { work(); } finally { cleanup(); log(); }"), 0);
    }

    #[test]
    fn break_inside_nested_loop_in_finally_is_ok() {
        // The `break` targets the inner `for`, not the finally → legitimate.
        let src = "try { work(); } finally { for (int i = 0; i < 3; i++) { if (done()) break; } }";
        assert_eq!(finallys(src), 0, "{:?}", run(src));
    }

    #[test]
    fn continue_inside_nested_loop_in_finally_is_ok() {
        let src = "try { work(); } finally { for (int i = 0; i < 3; i++) { if (skip()) continue; } }";
        assert_eq!(finallys(src), 0, "{:?}", run(src));
    }

    #[test]
    fn return_inside_lambda_in_finally_is_ok() {
        // The `return` belongs to the lambda, not the finally's method.
        let src = "try { work(); } finally { Runnable r = () -> { return; }; r.run(); }";
        assert_eq!(finallys(src), 0, "{:?}", run(src));
    }

    #[test]
    fn labelled_break_in_finally_is_skipped() {
        // A labelled jump might target an outer loop even past a nested one — can't resolve cheaply,
        // so we stay silent (sound: no false positive).
        let src = "outer: for (;;) { try { work(); } finally { break outer; } }";
        assert_eq!(finallys(src), 0, "{:?}", run(src));
    }
}
