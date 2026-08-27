//! The shape of a declared method, read off the tree: its erased parameter types, its modifiers,
//! and the supertypes of the class declaring it.
//!
//! Every check that asks "does this method override that one" needs the same four answers, and each
//! one used to carry its own copy. The copies drifted: one resolved a parameter type with
//! [`type_binary`](crate::resolve::type_binary) and the other with
//! [`type_binary_at`](crate::resolve::type_binary_at), so a nested parameter type resolved against
//! its owner in one check and against the whole file in the other — the same signature matching in
//! one place and not in the next. One copy, and it is the owner-aware one.

use bennu_java::prelude::{FileSymbols, TypeResolver};
use tree_sitter::Node;

use crate::nodes::text;
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

/// The `extends` type text of a class, if it declares one.
pub(crate) fn superclass_text(n: Node, bytes: &[u8]) -> Option<String> {
    let sc = n.child_by_field_name("superclass")?;
    let mut c = sc.walk();
    for ch in sc.named_children(&mut c) {
        if matches!(ch.kind(), "type_identifier" | "scoped_type_identifier" | "generic_type") {
            return text(ch, bytes);
        }
    }
    None
}

/// The `implements` (or interface `extends`) type texts of a type declaration.
pub(crate) fn implements_texts(n: Node, bytes: &[u8]) -> Vec<String> {
    let Some(w) = n.child_by_field_name("interfaces") else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![w];
    while let Some(node) = stack.pop() {
        if matches!(node.kind(), "type_identifier" | "scoped_type_identifier" | "generic_type") {
            if let Some(t) = text(node, bytes) {
                out.push(t);
            }
            continue;
        }
        let mut c = node.walk();
        for ch in node.named_children(&mut c) {
            stack.push(ch);
        }
    }
    out
}

