//! The call shapes Mockito's API is made of.
//!
//! Every one of them is recognised the same way: by its name, by its place in the chain, and by the
//! innermost call of the chain resolving to Mockito through the imports. The chain matters as much
//! as the name — `doReturn(1).when(repo)` and `Mockito.when(repo.find())` are both called `when`,
//! and they are two different methods of two different types.

use tree_sitter::Node;

use crate::file::{arguments, call_name, static_qualifier, unwrap_expression, JavaFile};
use crate::owners::{BDD, DO_METHODS, STUBBING};

/// For `when(repo.find(…))` / `given(repo.find(…))` resolved as Mockito's: the call being stubbed.
///
/// `None` when the one argument is not a method call — `when(true)` is a different mistake, with a
/// different exception, thrown on the spot.
pub(crate) fn stubbed_invocation<'t>(file: &JavaFile<'_>, call: Node<'t>) -> Option<Node<'t>> {
    let name = call_name(call, file.source);
    let owners = match name {
        "when" => STUBBING,
        "given" => BDD,
        _ => return None,
    };
    if !file.is_static_call(call, name, owners) {
        return None;
    }
    let args = arguments(call);
    let [argument] = args.as_slice() else { return None };
    let target = unwrap_expression(*argument);
    (target.kind() == "method_invocation").then_some(target)
}

/// Whether `node` is a stubber: `doReturn(x)` resolved as Mockito's, or any of the `do…` calls
/// chained onto one (`doReturn(1).doThrow(e)`).
pub(crate) fn is_stubber(file: &JavaFile<'_>, node: Node<'_>) -> bool {
    if node.kind() != "method_invocation" {
        return false;
    }
    let name = call_name(node, file.source);
    if !DO_METHODS.contains(&name) {
        return false;
    }
    match static_qualifier(node, file.source) {
        Some(qualifier) => file.resolves(name, qualifier, STUBBING),
        None => node.child_by_field_name("object").is_some_and(|o| is_stubber(file, o)),
    }
}

/// `verify(mock)` or `verify(mock, mode)`, resolved as Mockito's.
pub(crate) fn is_verify(file: &JavaFile<'_>, call: Node<'_>) -> bool {
    matches!(arguments(call).len(), 1 | 2) && file.is_static_call(call, "verify", STUBBING)
}

/// BDD's `then(mock)`, resolved as BDDMockito's.
pub(crate) fn is_bdd_then(file: &JavaFile<'_>, call: Node<'_>) -> bool {
    arguments(call).len() == 1 && file.is_static_call(call, "then", BDD)
}
