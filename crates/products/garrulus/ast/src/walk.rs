//! One traversal, reused by everything that reads a note.
//!
//! The outline, the task list, the tag extraction, the link extraction, the
//! word index, the "does this note mention that one" check — six consumers that
//! would otherwise each grow their own recursive match and each forget a
//! container the day a new [`Block`] variant appears. Adding a variant here is a
//! single edit; adding it in six hand-rolled walks is a bug that shows up as
//! "tags inside callouts are invisible to search".
//!
//! All visits are **pre-order**: the container is offered before its children, so
//! a consumer that wants to skip a subtree can note its span and ignore what
//! follows.

use crate::block::Block;
use crate::document::Document;
use crate::inline::Inline;

/// Visit every block in `blocks`, including those nested inside quotes, callouts
/// and list items.
pub fn visit_blocks<F: FnMut(&Block)>(blocks: &[Block], f: &mut F) {
    for block in blocks {
        f(block);
        match block {
            Block::Quote { blocks, .. } | Block::Callout { blocks, .. } => {
                visit_blocks(blocks, f)
            }
            Block::List { items, .. } => {
                for item in items {
                    visit_blocks(&item.blocks, f);
                }
            }
            _ => {}
        }
    }
}

/// Visit every inline in `inlines`, including those nested inside emphasis and
/// link labels.
pub fn visit_inlines<F: FnMut(&Inline)>(inlines: &[Inline], f: &mut F) {
    for inline in inlines {
        f(inline);
        let children = inline.children();
        if !children.is_empty() {
            visit_inlines(children, f);
        }
    }
}

/// Visit the inlines a single block owns directly — heading text, paragraph
/// content, table cells — without descending into nested blocks.
///
/// The piece [`visit_document`] composes with [`visit_blocks`]: separating them
/// is what lets a consumer that only cares about, say, headings avoid walking
/// every paragraph's inline tree.
pub fn visit_block_inlines<F: FnMut(&Inline)>(block: &Block, f: &mut F) {
    match block {
        Block::Heading { inlines, .. } | Block::Paragraph { inlines, .. } => {
            visit_inlines(inlines, f)
        }
        Block::Table { head, rows, .. } => {
            for cell in head {
                visit_inlines(cell, f);
            }
            for row in rows {
                for cell in row {
                    visit_inlines(cell, f);
                }
            }
        }
        _ => {}
    }
}

/// Walk a whole document once, offering every block and every inline.
///
/// The entry point for the extraction pass that runs on save: one traversal
/// produces the outline, the tasks, the tags and the links together.
pub fn visit_document<B, I>(doc: &Document, on_block: &mut B, on_inline: &mut I)
where
    B: FnMut(&Block),
    I: FnMut(&Inline),
{
    visit_blocks(&doc.blocks, &mut |block: &Block| {
        on_block(block);
        visit_block_inlines(block, on_inline);
    });
}

/// [`visit_blocks`], with mutable access — the traversal an AST refactor needs
/// (retarget every link to a renamed note, promote a section, flip a task).
pub fn visit_blocks_mut<F: FnMut(&mut Block)>(blocks: &mut [Block], f: &mut F) {
    for block in blocks {
        f(block);
        match block {
            Block::Quote { blocks, .. } | Block::Callout { blocks, .. } => {
                visit_blocks_mut(blocks, f)
            }
            Block::List { items, .. } => {
                for item in items {
                    visit_blocks_mut(&mut item.blocks, f);
                }
            }
            _ => {}
        }
    }
}

/// [`visit_inlines`], with mutable access.
pub fn visit_inlines_mut<F: FnMut(&mut Inline)>(inlines: &mut [Inline], f: &mut F) {
    for inline in inlines {
        f(inline);
        if let Some(children) = inline.children_mut() {
            visit_inlines_mut(children, f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{CalloutKind, ListItem, TaskState};
    use crate::frontmatter::Frontmatter;
    use crate::span::Span;

    fn para(text: &str) -> Block {
        Block::Paragraph { inlines: vec![Inline::text(text)], span: Span::EMPTY }
    }

    /// A note exercising every container: a callout holding a quote, a list whose
    /// item holds a nested list, and a table.
    fn sample() -> Document {
        Document::new(
            Frontmatter::empty(),
            vec![
                Block::Heading {
                    level: 1,
                    inlines: vec![Inline::Strong(vec![Inline::text("Titolo")])],
                    span: Span::EMPTY,
                },
                Block::Callout {
                    kind: CalloutKind::Warning,
                    title: None,
                    folded: false,
                    blocks: vec![Block::Quote { blocks: vec![para("dentro")], span: Span::EMPTY }],
                    span: Span::EMPTY,
                },
                Block::List {
                    ordered: false,
                    items: vec![ListItem {
                        task: Some(TaskState::Todo),
                        blocks: vec![
                            para("da fare"),
                            Block::List {
                                ordered: false,
                                items: vec![ListItem::new(vec![para("annidato")], Span::EMPTY)],
                                span: Span::EMPTY,
                            },
                        ],
                        span: Span::EMPTY,
                    }],
                    span: Span::EMPTY,
                },
                Block::Table {
                    head: vec![vec![Inline::text("chiave")]],
                    rows: vec![vec![vec![Inline::Tag {
                        name: "arbor".into(),
                        span: Span::new(0, 6),
                    }]]],
                    span: Span::EMPTY,
                },
            ],
        )
    }

    #[test]
    fn visit_blocks_reaches_callouts_quotes_and_nested_list_items() {
        let doc = sample();
        let mut seen = 0usize;
        let mut paragraphs = Vec::new();
        visit_blocks(&doc.blocks, &mut |b: &Block| {
            seen += 1;
            if let Block::Paragraph { inlines, .. } = b {
                paragraphs.push(crate::inline::plain_text(inlines));
            }
        });
        // heading, callout, quote, para, list, para, list, para, table
        assert_eq!(seen, 9);
        assert_eq!(paragraphs, ["dentro", "da fare", "annidato"]);
    }

    #[test]
    fn visit_inlines_descends_into_styling_and_link_labels() {
        let inlines = vec![Inline::Link {
            href: "https://example.invalid".into(),
            label: vec![Inline::Emph(vec![Inline::text("etichetta")])],
            span: Span::EMPTY,
        }];
        let mut kinds = Vec::new();
        visit_inlines(&inlines, &mut |i: &Inline| {
            kinds.push(match i {
                Inline::Link { .. } => "link",
                Inline::Emph(_) => "emph",
                Inline::Text(_) => "text",
                _ => "other",
            })
        });
        assert_eq!(kinds, ["link", "emph", "text"]);
    }

    #[test]
    fn visit_document_finds_a_tag_buried_in_a_table_cell() {
        let doc = sample();
        let mut tags = Vec::new();
        visit_document(
            &doc,
            &mut |_b: &Block| {},
            &mut |i: &Inline| {
                if let Inline::Tag { name, .. } = i {
                    tags.push(name.clone());
                }
            },
        );
        assert_eq!(tags, ["arbor"]);
    }

    #[test]
    fn visit_document_offers_every_block_once() {
        let doc = sample();
        let mut blocks = 0usize;
        visit_document(&doc, &mut |_b: &Block| blocks += 1, &mut |_i: &Inline| {});
        assert_eq!(blocks, 9);
    }

    #[test]
    fn mutable_walks_reach_the_same_nodes() {
        let mut doc = sample();
        visit_blocks_mut(&mut doc.blocks, &mut |b: &mut Block| {
            if let Block::Heading { level, .. } = b {
                *level = 2;
            }
        });
        assert!(matches!(doc.blocks[0], Block::Heading { level: 2, .. }));

        let mut inlines = vec![Inline::Strong(vec![Inline::text("vecchio")])];
        visit_inlines_mut(&mut inlines, &mut |i: &mut Inline| {
            if let Inline::Text(t) = i {
                *t = "nuovo".into();
            }
        });
        assert_eq!(crate::inline::plain_text(&inlines), "nuovo");
    }
}
