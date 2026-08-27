//! `break` / `continue` legality — pure-AST, and decidable without resolving anything, because the
//! question is entirely about where the statement sits in the tree.
//!
//! Three findings, all of them javac errors rather than style:
//!   1. **`break` outside any loop or `switch`** (`compiler.err.break.outside.switch.loop`) — nothing
//!      to break out of.
//!   2. **`continue` outside any loop** (`compiler.err.cont.outside.loop`) — a `switch` does not
//!      count, which is the case worth catching: `continue` inside a `switch` inside no loop reads
//!      like it continues the switch, and does not compile.
//!   3. **A label nothing declares** (`compiler.err.undef.label`) — `break outer;` with no enclosing
//!      `outer:`. Survives a refactor that deletes or renames the labelled statement, and the file
//!      still looks right at the `break`.
//!
//! The walk stops at a **body boundary** — a lambda body, an anonymous or local class — because a
//! loop outside one does not enclose a `break` inside it, and neither do its labels. Missing that is
//! how this check would produce a false negative on the one shape where the mistake is easy to make.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check_id::CheckId;

/// Every `break` / `continue` legality error in the pre-collected node slice.
pub fn branch_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "break_statement" => check_branch(n, bytes, true, &mut out),
            "continue_statement" => check_branch(n, bytes, false, &mut out),
            _ => {}
        }
    }
    out
}

/// A `break` is satisfied by a loop **or** a `switch`; a `continue` only by a loop. A labelled one
/// asks a different question — not "is there an enclosing loop" but "is there a statement carrying
/// that label" — so the two are answered separately.
fn check_branch(stmt: Node, bytes: &[u8], is_break: bool, out: &mut Vec<Diagnostic>) {
    match label_of(stmt, bytes) {
        Some(label) => {
            if !label_in_scope(stmt, &label, bytes) {
                out.push(CheckId::UnknownLabel.at(
                    stmt,
                    format!("no enclosing statement is labelled `{label}`"),
                ));
            }
        }
        None => {
            if !enclosed_by_target(stmt, is_break) {
                let (kw, wanted) = if is_break {
                    ("break", "a loop or a `switch`")
                } else {
                    ("continue", "a loop")
                };
                out.push(
                    CheckId::BranchOutsideLoop
                        .at(stmt, format!("`{kw}` is not inside {wanted}")),
                );
            }
        }
    }
}

/// The label a `break`/`continue` names, when it names one.
///
/// Read as the statement's own identifier child rather than by trimming the text: `break;` and
/// `break outer;` differ by one optional node, and a text scan would have to re-tokenize.
fn label_of(stmt: Node, bytes: &[u8]) -> Option<String> {
    let mut cw = stmt.walk();
    let id = stmt.named_children(&mut cw).find(|c| c.kind() == "identifier")?;
    id.utf8_text(bytes).ok().map(str::to_owned)
}

/// Whether an enclosing loop (or, for `break`, `switch`) contains this statement.
fn enclosed_by_target(stmt: Node, is_break: bool) -> bool {
    let mut cur = stmt.parent();
    while let Some(n) = cur {
        if is_body_boundary(&n) {
            return false;
        }
        match n.kind() {
            "for_statement" | "enhanced_for_statement" | "while_statement" | "do_statement" => {
                return true;
            }
            "switch_expression" if is_break => return true,
            _ => {}
        }
        cur = n.parent();
    }
    false
}

/// Whether some enclosing statement carries `label`.
///
/// Only labels on the path from the statement to the enclosing body count — a label on a sibling
/// statement is not in scope, and accepting one would turn this check into a name search that
/// approves the very refactor mistake it exists to catch.
fn label_in_scope(stmt: Node, label: &str, bytes: &[u8]) -> bool {
    let mut cur = stmt.parent();
    while let Some(n) = cur {
        if is_body_boundary(&n) {
            return false;
        }
        if n.kind() == "labeled_statement" {
            let mut cw = n.walk();
            if n.named_children(&mut cw)
                .find(|c| c.kind() == "identifier")
                .and_then(|id| id.utf8_text(bytes).ok())
                .is_some_and(|name| name == label)
            {
                return true;
            }
        }
        cur = n.parent();
    }
    false
}

/// Whether crossing this node leaves the statement's own body.
///
/// A loop around a lambda does not enclose a `break` written inside it, and a method of a local or
/// anonymous class is a body of its own. `method_declaration` is the ordinary case; the others are
/// how a `break` ends up textually inside a loop while being legally outside it.
fn is_body_boundary(n: &Node) -> bool {
    matches!(
        n.kind(),
        "lambda_expression"
            | "method_declaration"
            | "constructor_declaration"
            | "compact_constructor_declaration"
            | "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::collect_nodes;

    fn codes(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).expect("parse");
        let nodes = collect_nodes(tree.root_node());
        branch_errors_nodes(&nodes, src).into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn a_break_with_no_loop_or_switch_is_flagged() {
        assert_eq!(codes("class A { void m() { break; } }"), ["branch-outside-loop"]);
    }

    #[test]
    fn a_continue_with_no_loop_is_flagged() {
        assert_eq!(codes("class A { void m() { continue; } }"), ["branch-outside-loop"]);
    }

    /// The case the check exists for: a `switch` satisfies a `break` but never a `continue`.
    #[test]
    fn a_continue_inside_a_switch_outside_a_loop_is_flagged() {
        let src = "class A { void m(int i) { switch (i) { case 1: continue; } } }";
        assert_eq!(codes(src), ["branch-outside-loop"]);
    }

    #[test]
    fn a_break_inside_a_switch_is_fine() {
        let src = "class A { void m(int i) { switch (i) { case 1: break; } } }";
        assert!(codes(src).is_empty());
    }

    #[test]
    fn a_break_and_a_continue_inside_every_loop_form_are_fine() {
        for loop_src in [
            "for (;;) { break; continue; }",
            "for (int i : new int[0]) { break; continue; }",
            "while (true) { break; continue; }",
            "do { break; continue; } while (true);",
        ] {
            let src = format!("class A {{ void m() {{ {loop_src} }} }}");
            assert!(codes(&src).is_empty(), "{loop_src}");
        }
    }

    #[test]
    fn a_label_nothing_declares_is_flagged() {
        let src = "class A { void m() { outer: for (;;) { break nope; } } }";
        assert_eq!(codes(src), ["unknown-label"]);
    }

    #[test]
    fn a_label_an_enclosing_statement_declares_is_fine() {
        let src = "class A { void m() { outer: for (;;) { for (;;) { continue outer; } } } }";
        assert!(codes(src).is_empty());
    }

    /// A loop outside a lambda does not enclose a `break` written inside it — javac agrees, and a
    /// walk that did not stop at the body would call this legal.
    #[test]
    fn a_break_inside_a_lambda_does_not_see_the_enclosing_loop() {
        let src = "class A { void m() { for (;;) { Runnable r = () -> { break; }; } } }";
        assert_eq!(codes(src), ["branch-outside-loop"]);
    }

    /// Same for a method of an anonymous class, and for the label scope.
    #[test]
    fn an_anonymous_class_method_sees_neither_the_loop_nor_its_labels() {
        let src = "class A { void m() { outer: for (;;) { Runnable r = new Runnable() { \
                   public void run() { break outer; } }; } } }";
        assert_eq!(codes(src), ["unknown-label"]);
    }

    /// A label on a sibling statement is not in scope where the `break` is written.
    #[test]
    fn a_label_on_a_sibling_statement_is_not_in_scope() {
        let src = "class A { void m() { outer: for (;;) {} for (;;) { break outer; } } }";
        assert_eq!(codes(src), ["unknown-label"]);
    }
}
