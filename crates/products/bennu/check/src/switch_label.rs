//! Shared `switch_label` predicates — the three switch checks all need the same answers about a
//! label, and each had grown its own copy.
//!
//! Per tree-sitter-java a `switch_label`'s named children are one of: an **expression** (the constant
//! forms — `case A`, `case 1`, `case "x"`, `case Status.A`), a **pattern** (`type_pattern` /
//! `record_pattern`), or a **guard** (the `when` clause, which the grammar makes a *sibling* of the
//! pattern rather than a child of it). The `default` keyword is an anonymous child, so it is found
//! by scanning the unnamed children.
//!
//! The guard's sibling position is the reason [`label_is_pattern`] exists and why it is checked
//! before reading any constant: in `case Foo f when flag ->`, the guard expression `flag` is a bare
//! `identifier` sitting exactly where a case constant sits. A check that read it as a constant would
//! report a perfectly legal arm.

use tree_sitter::Node;

/// Whether a `switch_label` is a **pattern** label rather than a constant one.
///
/// The `guard` is matched too, so a grammar that ever emitted one without a sibling pattern still
/// can't leak a guard expression into a constant set.
pub fn label_is_pattern(label: Node) -> bool {
    let mut c = label.walk();
    for ch in label.named_children(&mut c) {
        if matches!(ch.kind(), "pattern" | "type_pattern" | "record_pattern" | "guard") {
            return true;
        }
    }
    false
}

/// Whether a `switch_label` is the `default` clause. The keyword is an anonymous (unnamed) child of
/// the label, so the scan includes unnamed children.
pub fn label_is_default(label: Node, bytes: &[u8]) -> bool {
    let mut c = label.walk();
    for ch in label.children(&mut c) {
        if !ch.is_named() && ch.utf8_text(bytes) == Ok("default") {
            return true;
        }
    }
    false
}

/// The direct arms of a `switch`'s body (`switch_rule` for `case X ->`, `switch_block_statement_group`
/// for `case X:`), and within each arm its `switch_label`s — as a flat list of labels.
///
/// Descends exactly one level into the body, so a nested `switch` (which lives inside an arm's
/// *statement*, not as a direct body child) never contributes its labels here; it is visited as its
/// own `switch_expression` node.
pub fn labels_of<'t>(body: Node<'t>) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    let mut bc = body.walk();
    for arm in body.named_children(&mut bc) {
        if !matches!(arm.kind(), "switch_rule" | "switch_block_statement_group") {
            continue;
        }
        let mut ac = arm.walk();
        for label in arm.named_children(&mut ac) {
            if label.kind() == "switch_label" {
                out.push(label);
            }
        }
    }
    out
}
