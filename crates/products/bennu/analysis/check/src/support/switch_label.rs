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

use bennu_java::prelude::{ClassMembers, TypeRef};
use tree_sitter::Node;

use crate::support::assignable::{names_a_declared_object, spells_its_type};

/// The reference selectors a `switch` took before patterns (JLS §14.11: `String` and the boxes of
/// `char`, `byte`, `short`, `int`). An enum is the other one, told by its flags.
const LEGACY_REFERENCE_SELECTORS: [&str; 5] =
    ["java/lang/String", "java/lang/Integer", "java/lang/Short", "java/lang/Byte", "java/lang/Character"];

/// The boxes Java 21 still refuses as selectors and a later release reads through primitive patterns
/// — left unjudged rather than guessed at.
const OTHER_BOXES: [&str; 4] = ["java/lang/Long", "java/lang/Float", "java/lang/Double", "java/lang/Boolean"];

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

/// Whether a selector of type `sel` is one only a **pattern** switch accepts — `Object`, an interface,
/// any class that is not `String`, a box or an enum. Such a switch has to be exhaustive even as a
/// statement, cannot take a constant label, and does not exist before Java 21.
///
/// `cond` is the selector as written. An `Object` is believed only when the code spells it
/// ([`spells_its_type`], [`names_a_declared_object`]): inference reads an erased type variable as
/// `Object`, and a switch over a `T extends Shape` is not a switch over `Object`.
pub(crate) fn is_pattern_selector(sel: &TypeRef, members: &ClassMembers, cond: Node, bytes: &[u8]) -> bool {
    let name = sel.binary_name.as_str();
    if sel.dims > 0
        || !name.contains('/')
        || members.flags.is_enum
        || members.flags.has_hidden_members
        || LEGACY_REFERENCE_SELECTORS.contains(&name)
        || OTHER_BOXES.contains(&name)
    {
        return false;
    }
    if name != "java/lang/Object" {
        return true;
    }
    let inner = if cond.kind() == "parenthesized_expression" { cond.named_child(0) } else { Some(cond) };
    inner.is_some_and(|n| spells_its_type(n) || names_a_declared_object(n, bytes))
}

/// The expressions a switch EXPRESSION's value comes from: each arrow arm's expression, and every
/// `yield` of a block arm or a colon group — not the yields of a switch expression nested inside,
/// nor anything in a lambda or a class body, which are other targets.
pub(crate) fn switch_results<'t>(switch: Node<'t>) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    let Some(body) = switch.child_by_field_name("body") else { return out };
    let mut bc = body.walk();
    for arm in body.named_children(&mut bc) {
        match arm.kind() {
            "switch_rule" => {
                let mut ac = arm.walk();
                for part in arm.named_children(&mut ac) {
                    match part.kind() {
                        "expression_statement" => out.extend(part.named_child(0)),
                        "block" => collect_yields(part, &mut out),
                        _ => {}
                    }
                }
            }
            "switch_block_statement_group" => collect_yields(arm, &mut out),
            _ => {}
        }
    }
    out
}

fn collect_yields<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) {
    let mut c = node.walk();
    for child in node.named_children(&mut c) {
        match child.kind() {
            "yield_statement" => out.extend(child.named_child(0)),
            "lambda_expression" | "class_body" => {}
            "switch_expression" if !crate::switching::switches::is_statement_position(child) => {}
            _ => collect_yields(child, out),
        }
    }
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
