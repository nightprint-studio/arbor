//! The handful of tree walks every check here needs.

use tree_sitter::Node;

/// Comments are named nodes in this grammar, and one sitting in an argument list would otherwise
/// count as an argument.
pub(crate) fn is_comment(node: Node<'_>) -> bool {
    matches!(node.kind(), "line_comment" | "block_comment")
}

/// The named children of a node, comments left out.
pub(crate) fn children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    let found: Vec<Node<'t>> = node.named_children(&mut cursor).filter(|c| !is_comment(*c)).collect();
    found
}

/// Whether a node has a comment among its direct children. A rewrite rebuilds the text of the nodes
/// it keeps, so a comment between them would be silently deleted — the caller declines instead.
pub(crate) fn has_comment(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    let found = node.named_children(&mut cursor).any(is_comment);
    found
}

/// Every named node under `root` (itself included), in source order.
pub(crate) fn descendants<'t>(root: Node<'t>) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        out.push(node);
        let mut cursor = node.walk();
        let kids: Vec<Node<'t>> = node.named_children(&mut cursor).collect();
        stack.extend(kids.into_iter().rev());
    }
    out
}

/// The parents of a node, nearest first.
pub(crate) fn ancestors<'t>(node: Node<'t>) -> impl Iterator<Item = Node<'t>> {
    std::iter::successors(node.parent(), |n| n.parent())
}

/// The expression inside any number of parentheses.
pub(crate) fn unparen<'t>(node: Node<'t>) -> Node<'t> {
    let mut current = node;
    while current.kind() == "parenthesized_expression" {
        match children(current).first() {
            Some(inner) => current = *inner,
            None => break,
        }
    }
    current
}

pub(crate) fn encloses(outer: Node<'_>, inner: Node<'_>) -> bool {
    outer.start_byte() <= inner.start_byte() && inner.end_byte() <= outer.end_byte()
}

pub(crate) fn span(node: Node<'_>) -> (usize, usize) {
    (node.start_byte(), node.end_byte())
}

/// The whitespace a line starts with — the indentation a statement inserted after the one at `at`
/// should copy.
pub(crate) fn line_indent(source: &str, at: usize) -> &str {
    let line_start = source[..at].rfind('\n').map_or(0, |i| i + 1);
    let line = &source[line_start..];
    let width = line.len() - line.trim_start_matches([' ', '\t']).len();
    &line[..width]
}
