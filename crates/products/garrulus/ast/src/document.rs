//! [`Document`] — one parsed note.
//!
//! Frontmatter plus a flat sequence of blocks. Flat is the right shape even
//! though headings imply a hierarchy: a note is edited as a linear document, and
//! the tree the outline wants is a *view* built by walking the headings, not the
//! storage. Storing it nested would make "insert a paragraph here" a tree surgery
//! and every heading level change a restructuring.

use serde::{Deserialize, Serialize};

use crate::block::Block;
use crate::frontmatter::Frontmatter;
use crate::inline::{self, Inline};

/// A note, in the format-agnostic model.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    /// Typed metadata. Round-trips byte-stable until edited — see
    /// [`crate::frontmatter`].
    pub frontmatter: Frontmatter,
    /// Body, in document order.
    pub blocks: Vec<Block>,
}

impl Document {
    /// A document from its two halves.
    pub fn new(frontmatter: Frontmatter, blocks: Vec<Block>) -> Self {
        Self { frontmatter, blocks }
    }

    /// An empty note.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Whether the note has neither metadata nor body.
    pub fn is_empty(&self) -> bool {
        self.frontmatter.is_empty() && self.blocks.is_empty()
    }

    /// The text of the first top-level heading, if the note opens with one.
    ///
    /// One half of the title rule (`frontmatter.title`, else H1, else filename).
    /// The other two halves need the vault's config and the note's path, so the
    /// whole rule lives in `garrulus-vault` and this is the piece that is honestly
    /// a property of the document.
    pub fn first_heading_text(&self) -> Option<String> {
        self.blocks.iter().find_map(|block| match block {
            Block::Heading { level: 1, inlines, .. } => Some(inline::plain_text(inlines)),
            _ => None,
        })
    }

    /// Every heading in the note, as `(level, text)` in document order.
    ///
    /// Top-level only: a heading nested inside a quote or a callout is part of
    /// that block's content, not part of the note's outline.
    pub fn outline(&self) -> Vec<(u8, String)> {
        self.blocks
            .iter()
            .filter_map(|block| match block {
                Block::Heading { level, inlines, .. } => {
                    Some((*level, inline::plain_text(inlines)))
                }
                _ => None,
            })
            .collect()
    }

    /// Append a paragraph of literal text. The one construction helper worth
    /// having here: quick capture and the daily-note append both do exactly this,
    /// and neither should have to spell out three nested `Vec`s to do it.
    pub fn push_text(&mut self, text: impl Into<String>) {
        self.blocks.push(Block::Paragraph {
            inlines: vec![Inline::Text(text.into())],
            span: crate::span::Span::EMPTY,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontmatter::FrontValue;
    use crate::span::Span;

    fn heading(level: u8, text: &str) -> Block {
        Block::Heading { level, inlines: vec![Inline::text(text)], span: Span::EMPTY }
    }

    #[test]
    fn first_heading_text_reads_the_h1_only() {
        let doc = Document::new(
            Frontmatter::empty(),
            vec![heading(2, "Sottotitolo"), heading(1, "Titolo vero")],
        );
        assert_eq!(doc.first_heading_text().as_deref(), Some("Titolo vero"));
    }

    #[test]
    fn outline_keeps_levels_and_order() {
        let doc = Document::new(
            Frontmatter::empty(),
            vec![heading(1, "Bug"), heading(2, "Passi"), heading(2, "Atteso")],
        );
        assert_eq!(
            doc.outline(),
            vec![
                (1, "Bug".to_string()),
                (2, "Passi".to_string()),
                (2, "Atteso".to_string())
            ]
        );
    }

    #[test]
    fn a_document_with_only_frontmatter_is_not_empty() {
        let doc = Document::new(
            Frontmatter::from_entries(vec![("tipo".into(), FrontValue::from("bug"))]),
            vec![],
        );
        assert!(!doc.is_empty());
        assert!(Document::empty().is_empty());
    }

    #[test]
    fn push_text_appends_a_paragraph() {
        let mut doc = Document::empty();
        doc.push_text("riga catturata al volo");
        assert_eq!(doc.blocks.len(), 1);
        assert!(matches!(doc.blocks[0], Block::Paragraph { .. }));
    }
}
