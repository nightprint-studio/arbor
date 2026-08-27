//! Small readings of the tree that every check needs and none of them owns: what a node's text is,
//! which field it fills in its parent, what modifiers a declaration carries, whether a binary name is
//! a primitive.
//!
//! Each of these was written between two and five times across the crate. None of the copies was
//! wrong — they are four-line functions — but a copy is a place a fix does not reach, and this crate
//! has already paid for that twice: once where a parameter type resolved against the file in one
//! check and against its owner in the next, and once where a name shadowed by an `instanceof` pattern
//! was invisible to one check and not the other. These are here so the count stops at one.
//!
//! Nothing here resolves anything. A helper that needs a `TypeResolver` belongs in
//! [`crate::method_sig`] (signatures) or [`crate::resolve`] (type names) instead.

use tree_sitter::Node;

/// A node's source text.
pub(crate) fn text(node: Node, bytes: &[u8]) -> Option<String> {
    node.utf8_text(bytes).ok().map(str::to_string)
}

/// The first direct named child of `n` with the given kind.
pub(crate) fn child_of_kind<'t>(n: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut c = n.walk();
    for ch in n.named_children(&mut c) {
        if ch.kind() == kind {
            return Some(ch);
        }
    }
    None
}

/// The field name that immediate child `child` occupies in `parent` (`name`, `value`, `object`,
/// `field`, …), or `None` if it fills no named field.
///
/// The slot is the reliable discriminator between a binding and a reference: the same `identifier`
/// node means one thing in a declarator's `name` and another in its `value`.
pub(crate) fn child_field_name(parent: Node, child: Node) -> Option<String> {
    let mut c = parent.walk();
    if c.goto_first_child() {
        loop {
            if c.node().id() == child.id() {
                return c.field_name().map(str::to_string);
            }
            if !c.goto_next_sibling() {
                break;
            }
        }
    }
    None
}

/// Whether a declaration carries `keyword` among its modifiers.
///
/// Reads the `modifiers` node's text and splits it on whitespace, which is why an annotation in
/// front does not confuse it: `@Override public` splits into words and none of them is `private`.
pub(crate) fn has_keyword(node: Node, bytes: &[u8], keyword: &str) -> bool {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "modifiers" {
            if let Ok(t) = ch.utf8_text(bytes) {
                return t.split_whitespace().any(|w| w == keyword);
            }
        }
    }
    false
}

/// The keyword modifiers on a declaration — `["public", "abstract"]`. Annotations, which are named
/// nodes inside `modifiers`, are excluded.
pub(crate) fn modifier_keywords<'a>(node: Node, bytes: &'a [u8]) -> Vec<&'a str> {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "modifiers" {
            let mut out = Vec::new();
            let mut mc = ch.walk();
            for m in ch.children(&mut mc) {
                if !m.is_named() {
                    if let Ok(t) = m.utf8_text(bytes) {
                        out.push(t);
                    }
                }
            }
            return out;
        }
    }
    Vec::new()
}

/// The last segment of a binary name — `java/util/Map$Entry` → `Entry`.
///
/// Splits on both separators because a nested type has two spellings in circulation (`Outer/Inner`
/// from source, `Outer$Inner` from bytecode) and a message should read the same either way.
pub(crate) fn simple_name(binary: &str) -> &str {
    binary.rsplit(['/', '$']).next().unwrap_or(binary)
}

/// Whether a binary name is one of Java's primitives (or `void`).
pub(crate) fn is_primitive(binary: &str) -> bool {
    matches!(
        binary,
        "int" | "long" | "short" | "byte" | "char" | "boolean" | "float" | "double" | "void"
    )
}

/// Whether a binary name looks like an unresolved type VARIABLE (`T`, `K`, `E`) rather than a type.
///
/// A single uppercase letter, which is the convention every generic declaration follows and the only
/// signal available once a name has failed to resolve. Deliberately narrow: a real one-letter class
/// would be misread, and the cost of that is a skipped check rather than a wrong one.
pub(crate) fn is_type_var(binary: &str) -> bool {
    binary.len() == 1 && binary.chars().all(|c| c.is_ascii_uppercase())
}
