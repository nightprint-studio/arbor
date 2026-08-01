//! **The only file in this crate that touches Tree-sitter.**
//!
//! ## The contract with the rest of the crate
//!
//! Tree-sitter is asked exactly one question: *where does each block start and
//! end, and what kind is it?* It answers with [`GNode`], a tree of
//! `(kind, byte range)` and nothing else. Every other detail — the fence's info
//! string, a heading's level, a list marker, a table's cells, the text inside a
//! quote — is derived from the source slice by pure code in
//! [`reader`](crate::reader).
//!
//! That is not fastidiousness. The grammar crate is the one dependency here
//! whose resolution cannot be checked without a network, and reading node
//! *kinds* (a dozen strings, stable across the whole `tree-sitter-markdown`
//! family) is a far smaller bet than reading node *structure* (field names,
//! child ordering, the block/inline tree split), which changes between
//! releases. If the grammar has to be swapped — or vendored as `parser.c` and
//! compiled with `cc` the way `picus-parse` does it — [`parse_blocks`] is the
//! only function that changes.
//!
//! ## Nothing is ever dropped
//!
//! [`fill_gaps`] closes the loop: any byte range the walk did not claim comes
//! back as [`GKind::Verbatim`], which the reader turns into a `Block::Html`
//! carrying the text as-is. A node kind this file has never heard of therefore
//! costs structure, never content.

use garrulus_ast::prelude::Span;
use tree_sitter::{Language, Node, Parser};

/// The block kinds the reader knows how to build from a source slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GKind {
    /// `document` / `section` and any unrecognised wrapper: transparent, its
    /// recognised descendants are spliced into the parent.
    Container,
    Heading,
    Paragraph,
    Quote,
    FencedCode,
    IndentedCode,
    List,
    ListItem,
    Table,
    Rule,
    /// Kept byte-for-byte: HTML blocks, link reference definitions, and
    /// anything the walk failed to claim.
    Verbatim,
}

/// A block and its byte range, in the coordinates of the text that was parsed.
#[derive(Debug, Clone)]
pub(crate) struct GNode {
    pub kind: GKind,
    pub span: Span,
    pub children: Vec<GNode>,
}

/// The compiled markdown block grammar, for a caller that wants to drive
/// Tree-sitter directly (a highlighter, a query).
///
/// If the build fails on this line, the grammar crate exposes its language
/// under a different name (older releases use `fn language() -> Language`) —
/// that is the single edit, and nothing else in the crate depends on it.
pub fn block_language() -> Language {
    tree_sitter_md::LANGUAGE.into()
}

/// Parse `text` into a tree of blocks. Spans are relative to `text`.
///
/// Returns `None` only if the grammar cannot be loaded, which cannot happen
/// with a matching `tree-sitter` version — but "cannot happen" is no reason for
/// a parser to panic, so the caller degrades to verbatim text instead.
pub(crate) fn parse_blocks(text: &str) -> Option<GNode> {
    let mut parser = Parser::new();
    parser.set_language(&block_language()).ok()?;
    let tree = parser.parse(text.as_bytes(), None)?;
    Some(convert(tree.root_node()))
}

/// Insert a [`GKind::Verbatim`] node for every stretch of `text` that carries
/// content and that no node claimed, then sort the result into source order.
///
/// This is the crate's guarantee that a note never loses bytes to a grammar
/// change: `nodes` is what the reader renders, so anything missing from it
/// would silently vanish from the file on the next save.
pub(crate) fn fill_gaps(nodes: &mut Vec<GNode>, text: &str) {
    nodes.sort_by_key(|n| n.span.start);
    let mut gaps = Vec::new();
    let mut cursor = 0usize;
    for node in nodes.iter() {
        push_gap(&mut gaps, text, cursor, node.span.start);
        cursor = cursor.max(node.span.end);
    }
    push_gap(&mut gaps, text, cursor, text.len());
    if gaps.is_empty() {
        return;
    }
    nodes.extend(gaps);
    nodes.sort_by_key(|n| n.span.start);
}

fn push_gap(gaps: &mut Vec<GNode>, text: &str, start: usize, end: usize) {
    if start >= end || end > text.len() {
        return;
    }
    if text[start..end].trim().is_empty() {
        return;
    }
    gaps.push(GNode {
        kind: GKind::Verbatim,
        span: Span { start, end },
        children: Vec::new(),
    });
}

/// The node kinds this crate recognises. Everything else is a transparent
/// wrapper (`None`) whose recognised descendants are lifted into the parent.
fn kind_of(kind: &str) -> Option<GKind> {
    Some(match kind {
        "document" | "section" => GKind::Container,
        "atx_heading" | "setext_heading" => GKind::Heading,
        "paragraph" => GKind::Paragraph,
        "block_quote" => GKind::Quote,
        "fenced_code_block" => GKind::FencedCode,
        "indented_code_block" => GKind::IndentedCode,
        "list" => GKind::List,
        "list_item" => GKind::ListItem,
        "pipe_table" => GKind::Table,
        "thematic_break" => GKind::Rule,
        "html_block" | "link_reference_definition" => GKind::Verbatim,
        _ => return None,
    })
}

fn convert(node: Node<'_>) -> GNode {
    let kind = kind_of(node.kind()).unwrap_or(GKind::Container);
    GNode {
        kind,
        span: Span {
            start: node.start_byte(),
            end: node.end_byte(),
        },
        // Only these two need their children: a list needs its items, and a
        // container is nothing *but* its children. A quote's interior is
        // re-parsed from de-quoted text, and everything else is read straight
        // out of the source slice — so their subtrees are never inspected and
        // this crate stays blind to the grammar's inner node names.
        children: match kind {
            GKind::Container | GKind::List => collect(node),
            _ => Vec::new(),
        },
    }
}

fn collect(node: Node<'_>) -> Vec<GNode> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match kind_of(child.kind()) {
            Some(GKind::Container) | None => out.extend(collect(child)),
            Some(_) => out.push(convert(child)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(kind: GKind, start: usize, end: usize) -> GNode {
        GNode {
            kind,
            span: Span { start, end },
            children: Vec::new(),
        }
    }

    #[test]
    fn a_gap_between_two_blocks_becomes_verbatim() {
        let text = "aaa\nSCONOSCIUTO\nbbb";
        let mut nodes = vec![
            node(GKind::Paragraph, 0, 4),
            node(GKind::Paragraph, 16, 19),
        ];
        fill_gaps(&mut nodes, text);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[1].kind, GKind::Verbatim);
        assert_eq!(&text[nodes[1].span.start..nodes[1].span.end], "SCONOSCIUTO\n");
    }

    #[test]
    fn blank_gaps_are_not_material() {
        let text = "aaa\n\n\nbbb";
        let mut nodes = vec![node(GKind::Paragraph, 0, 3), node(GKind::Paragraph, 6, 9)];
        fill_gaps(&mut nodes, text);
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn text_nobody_claimed_at_all_still_survives() {
        let text = "tutto orfano";
        let mut nodes = Vec::new();
        fill_gaps(&mut nodes, text);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].span, Span { start: 0, end: 12 });
    }

    #[test]
    fn overlapping_nodes_do_not_produce_a_backwards_gap() {
        let text = "0123456789";
        let mut nodes = vec![node(GKind::Paragraph, 0, 8), node(GKind::Paragraph, 4, 6)];
        fill_gaps(&mut nodes, text);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[2].kind, GKind::Verbatim);
        assert_eq!(nodes[2].span, Span { start: 8, end: 10 });
    }
}
