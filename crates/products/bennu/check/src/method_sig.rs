//! The erased shape of a declared method, read off the tree.
//!
//! Every check that asks "does this method override that one" needs the same answer, and each one
//! used to carry its own copy. The copies drifted: one resolved a parameter type with
//! [`type_binary`](crate::resolve::type_binary) and the other with
//! [`type_binary_at`](crate::resolve::type_binary_at), so a nested parameter type resolved against
//! its owner in one check and against the whole file in the other — the same signature matching in
//! one place and not in the next. One copy, and it is the owner-aware one.
//!
//! The other half of that question — the supertypes of the class declaring the method — lives in
//! [`crate::supertypes`], which had five copies of its own.

use bennu_java::prelude::{FileSymbols, TypeResolver};
use tree_sitter::Node;

use crate::resolve::type_binary_at;

/// The erased binary names of a method's parameter types, in order.
///
/// `None` — skip this method — when a parameter type does not resolve, or when the method is
/// varargs. Erased-array matching against a `spread_parameter` is finicky enough that guessing it
/// wrong would report an override that isn't one, and a missed varargs override costs nothing.
pub(crate) fn method_param_binaries(
    md: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<Vec<String>> {
    let params_node = md.child_by_field_name("parameters")?;
    let mut out = Vec::new();
    let mut c = params_node.walk();
    for p in params_node.named_children(&mut c) {
        match p.kind() {
            "formal_parameter" => {
                let ty = p.child_by_field_name("type")?;
                let written = ty.utf8_text(bytes).ok()?;
                out.push(type_binary_at(written, ty, bytes, symbols, resolver)?);
            }
            "spread_parameter" => return None,
            _ => {}
        }
    }
    Some(out)
}

