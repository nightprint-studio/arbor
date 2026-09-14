//! A statement as a chain of calls: `assertThat(x).as("d").isEqualTo(1)` is one call to start it
//! and two chained after.
//!
//! In the tree it is the other way round — the outermost node is the LAST call, and the first one is
//! buried at the bottom of the `object` fields — which is the wrong order for every question asked
//! here, so it is turned around once.

use bennu_java::prelude::node_text;
use tree_sitter::Node;

use crate::syntax::children;

pub(crate) struct Chain<'t> {
    /// The call the chain starts with — the one whose receiver is not itself a call.
    pub root: Node<'t>,
    /// The calls chained after it, in the order they are written.
    pub links: Vec<Node<'t>>,
}

/// The chain `expr` is the last call of, or `None` when it is not a call.
pub(crate) fn chain_of(expr: Node<'_>) -> Option<Chain<'_>> {
    if expr.kind() != "method_invocation" {
        return None;
    }
    let mut calls = vec![expr];
    let mut current = expr;
    while let Some(object) = current.child_by_field_name("object") {
        if object.kind() != "method_invocation" {
            break;
        }
        calls.push(object);
        current = object;
    }
    let root = calls.pop()?;
    calls.reverse();
    Some(Chain { root, links: calls })
}

/// The method name of a call.
pub(crate) fn call_name<'s>(call: Node<'_>, source: &'s str) -> &'s str {
    call.child_by_field_name("name").map(|n| node_text(&n, source)).unwrap_or_default()
}

/// A call's arguments, comments left out.
pub(crate) fn arguments(call: Node<'_>) -> Vec<Node<'_>> {
    call.child_by_field_name("arguments").map(children).unwrap_or_default()
}

/// The call an expression statement consists of — `None` for any other statement.
pub(crate) fn statement_call(statement: Node<'_>) -> Option<Node<'_>> {
    if statement.kind() != "expression_statement" {
        return None;
    }
    let expr = children(statement).into_iter().next()?;
    (expr.kind() == "method_invocation").then_some(expr)
}
