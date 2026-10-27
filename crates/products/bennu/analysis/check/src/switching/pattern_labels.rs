//! Pattern-`switch` label structure (pure-AST, JLS §14.11.1): what a list of `case` labels says about
//! itself without knowing the selector's type.
//!
//!   * **`case Object o` is unconditional** — every reference value matches it. Next to a `default`
//!     (`unconditional.pattern.and.default`) or a second one (`duplicate.unconditional.pattern`) it is
//!     an error whatever the selector is.
//!   * **a guard that is the constant `false`** can never be taken (`guard.has.constant.expression.false`).
//!   * **dominance** — a pattern after an unguarded pattern of the SAME type (or after `Object`) can
//!     never match (`pattern.dominated`). Subtype dominance (`CharSequence` before `String`) needs the
//!     hierarchy and is not judged here.
//!   * **fall-through into a pattern** — in a colon-form `switch`, a group that ends in a statement
//!     which completes normally falls into the next group; if that one starts with a binding pattern
//!     the bindings would be unassigned (`flows.through.to.pattern`).
//!
//! Types are compared as written (whitespace removed): `List<String>` and `List<Integer>` differ, and a
//! spelling that differs for the same type (`java.lang.Object`) is only recognised for `Object`.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use bennu_java::prelude::{FileSymbols, TypeResolver};

use crate::engine::check_id::CheckId;
use crate::support::resolve::written_type_binary;
use crate::support::switch_label::{label_is_default, labels_of};
use crate::support::walk::{hierarchy_fully_known, reaches};

/// One `case` label that is a type pattern.
struct TypePattern<'t> {
    label: Node<'t>,
    ty: String,
    /// The written type, for resolving it.
    ty_node: Node<'t>,
    /// Whether it matches without a guard (`when true` is no guard).
    unguarded: bool,
}

/// All label-structure errors over the shared pre-collected node list.
pub fn pattern_label_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes.iter().filter(|n| n.kind() == "switch_expression" && !n.has_error()) {
        let Some(body) = n.child_by_field_name("body") else { continue };
        check_guards(body, bytes, &mut out);
        check_patterns(n, body, bytes, &mut out);
        check_fall_through(body, bytes, &mut out);
    }
    out
}

/// The guard of a label, as written without parentheses, if it has one.
fn guard_text<'s>(label: Node, bytes: &'s [u8]) -> Option<&'s str> {
    let mut c = label.walk();
    let guard = label.named_children(&mut c).find(|ch| ch.kind() == "guard")?;
    let expr = guard.named_child(0)?;
    Some(expr.utf8_text(bytes).ok()?.trim().trim_matches(|ch| ch == '(' || ch == ')').trim())
}

fn check_guards(body: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    for label in labels_of(body) {
        if guard_text(label, bytes) == Some("false") {
            out.push(CheckId::IncompatibleCaseLabel.at(label, "The guard of this `case` is the constant `false`, so it can never match"));
        }
    }
}

/// The type patterns among the labels of a switch, in source order.
fn type_patterns<'t>(body: Node<'t>, bytes: &[u8]) -> Vec<TypePattern<'t>> {
    let mut found = Vec::new();
    for label in labels_of(body) {
        let mut c = label.walk();
        let pattern = label.named_children(&mut c).find(|ch| ch.kind() == "pattern");
        let Some(pattern) = pattern else { continue };
        let mut pc = pattern.walk();
        let Some(type_pattern) = pattern.named_children(&mut pc).find(|ch| ch.kind() == "type_pattern") else { continue };
        let Some(ty_node) = type_pattern.named_child(0) else { continue };
        let Some(ty) = ty_node.utf8_text(bytes).ok() else { continue };
        let unguarded = guard_text(label, bytes).is_none_or(|g| g == "true");
        found.push(TypePattern { label, ty: ty.split_whitespace().collect(), ty_node, unguarded });
    }
    found
}

fn is_object(ty: &str) -> bool {
    ty == "Object" || ty == "java.lang.Object"
}

fn check_patterns(switch: Node, body: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let patterns = type_patterns(body, bytes);
    if patterns.is_empty() || crate::source::generics_syntax::file_declares_type_named_object(switch, bytes) {
        return;
    }
    let has_default = labels_of(body).into_iter().any(|l| label_is_default(l, bytes));
    let mut unconditional: Vec<&TypePattern> = Vec::new();
    for p in &patterns {
        if is_object(&p.ty) && p.unguarded {
            unconditional.push(p);
        }
    }
    if let Some(first) = unconditional.first() {
        if has_default {
            out.push(CheckId::IncompatibleCaseLabel.at(
                first.label,
                "An unconditional pattern (`Object`) makes the `default` of this `switch` unreachable",
            ));
        }
    }
    for second in unconditional.iter().skip(1) {
        out.push(CheckId::IncompatibleCaseLabel.at(second.label, "This `switch` already has an unconditional pattern"));
    }
    // Dominance: an earlier unguarded pattern of the same type, or `Object`, covers a later one. A
    // second `Object` is the duplicate-unconditional case above.
    for (i, later) in patterns.iter().enumerate() {
        let dominated = patterns[..i]
            .iter()
            .any(|earlier| earlier.unguarded && (earlier.ty == later.ty || is_object(&earlier.ty)) && !(is_object(&later.ty) && later.unguarded));
        if dominated {
            out.push(CheckId::IncompatibleCaseLabel.at(
                later.label,
                format!("The pattern `{}` is dominated by an earlier pattern and can never match", later.ty),
            ));
        }
    }
}

/// Dominance by **subtype** (resolver-backed): `case CharSequence c` before `case String s` leaves the
/// second unreachable, because every `String` is a `CharSequence`. Judged for plain class or interface
/// names that resolve, with both hierarchies fully known; a type spelled the same as an earlier one is
/// [`pattern_label_errors_nodes`]'s.
pub fn pattern_dominance_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let known = |binary: &str| resolver.members_of(binary).is_some() && hierarchy_fully_known(resolver, binary);
    let mut out = Vec::new();
    for &n in nodes.iter().filter(|n| n.kind() == "switch_expression" && !n.has_error()) {
        let Some(body) = n.child_by_field_name("body") else { continue };
        let patterns = type_patterns(body, bytes);
        for (i, later) in patterns.iter().enumerate() {
            if later.ty_node.kind() != "type_identifier" {
                continue;
            }
            let Some(later_binary) = written_type_binary(later.ty_node, bytes, symbols, resolver) else { continue };
            let dominated_by = patterns[..i].iter().find(|earlier| {
                if !earlier.unguarded || earlier.ty == later.ty || is_object(&earlier.ty) || earlier.ty_node.kind() != "type_identifier" {
                    return false;
                }
                let Some(earlier_binary) = written_type_binary(earlier.ty_node, bytes, symbols, resolver) else { return false };
                earlier_binary != later_binary
                    && known(&earlier_binary)
                    && known(&later_binary)
                    && reaches(resolver, &later_binary, &earlier_binary)
            });
            if let Some(earlier) = dominated_by {
                out.push(CheckId::IncompatibleCaseLabel.at(
                    later.label,
                    format!("The pattern `{}` is dominated by the earlier `{}` and can never match", later.ty, earlier.ty),
                ));
            }
        }
    }
    out
}

/// A colon-form group that ends in a statement completing normally, followed by a group whose first
/// label binds a pattern variable.
///
/// Only an expression statement or a local declaration is taken to complete normally — a branch, a
/// `return`, a `throw` or a block is not judged.
fn check_fall_through(body: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mut c = body.walk();
    let groups: Vec<Node> =
        body.named_children(&mut c).filter(|g| g.kind() == "switch_block_statement_group").collect();
    for pair in groups.windows(2) {
        let (before, after) = (pair[0], pair[1]);
        if !ends_normally(before) {
            continue;
        }
        let first_label = {
            let mut lc = after.walk();
            let found = after.named_children(&mut lc).find(|ch| ch.kind() == "switch_label");
            found
        };
        let Some(label) = first_label else { continue };
        let has_statements = {
            let mut sc = after.walk();
            let found = after.named_children(&mut sc).any(|ch| ch.kind() != "switch_label");
            found
        };
        if has_statements && binds_a_variable(label) && !label_is_default(label, bytes) {
            out.push(CheckId::IncompatibleCaseLabel.at(
                label,
                "Execution falls through into a pattern that binds a variable — end the previous group with `break`",
            ));
        }
    }
}

fn ends_normally(group: Node) -> bool {
    let mut c = group.walk();
    let last = group.named_children(&mut c).filter(|ch| ch.kind() != "switch_label").last();
    last.is_some_and(|stmt| matches!(stmt.kind(), "expression_statement" | "local_variable_declaration"))
}

fn binds_a_variable(label: Node) -> bool {
    let mut c = label.walk();
    let found = label.named_children(&mut c).any(|ch| ch.kind() == "pattern");
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        pattern_label_errors_nodes(&nodes, src).into_iter().map(|d| d.message).collect()
    }

    fn switch(arms: &str) -> String {
        format!("class C {{ void m(Object o, boolean f) {{ switch (o) {{ {arms} }} }} }}")
    }

    #[test]
    fn unconditional_pattern_with_default_or_twice() {
        assert_eq!(messages(&switch("case Object a -> {} default -> {}")).len(), 1);
        assert_eq!(messages(&switch("case Object a -> {} case Object b -> {}")).len(), 1);
        assert!(messages(&switch("case String s -> {} default -> {}")).is_empty());
        assert!(messages(&switch("case Object a when f -> {} default -> {}")).is_empty());
    }

    #[test]
    fn a_constant_false_guard_is_flagged() {
        assert_eq!(messages(&switch("case String s when false -> {} default -> {}")).len(), 1);
        assert!(messages(&switch("case String s when f -> {} default -> {}")).is_empty());
    }

    #[test]
    fn same_type_after_an_unguarded_pattern_is_dominated() {
        assert_eq!(messages(&switch("case Integer n -> {} case Integer m when f -> {} default -> {}")).len(), 1);
        assert!(messages(&switch("case Integer n when f -> {} case Integer m -> {} default -> {}")).is_empty());
        assert!(messages(&switch("case Integer n -> {} case String s -> {} default -> {}")).is_empty());
    }

    #[test]
    fn falling_into_a_binding_pattern_is_flagged() {
        let bad = switch("case String s: System.gc(); case Integer i: System.gc(); break; default: break;");
        assert_eq!(messages(&bad).len(), 1, "{:?}", messages(&bad));
        let ok = switch("case String s: System.gc(); break; case Integer i: System.gc(); break; default: break;");
        assert!(messages(&ok).is_empty());
    }
}
