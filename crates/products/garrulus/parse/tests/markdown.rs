//! End-to-end behaviour of the markdown `Reader` / `Writer` pair.
//!
//! Unlike the unit tests inside the crate, these run the real grammar: they are
//! the ones that fail if the Tree-sitter markdown pin ever changes shape.
//!
//! The collectors below are deliberately hand-rolled rather than borrowed from
//! `garrulus_ast::walk` — a test that shares a traversal with the code under
//! test stops being able to catch that traversal being wrong.

use garrulus_parse::prelude::*;

use garrulus_ast::prelude::{Block, CalloutKind, Document, Inline, TaskState};

fn read(source: &str) -> Document {
    MarkdownReader.read(source).expect("markdown never fails to read")
}

fn write(doc: &Document) -> String {
    MarkdownWriter.write(doc).expect("markdown never fails to write")
}

fn collect_inlines(blocks: &[Block], out: &mut Vec<Inline>) {
    for block in blocks {
        match block {
            Block::Heading { inlines, .. } | Block::Paragraph { inlines, .. } => {
                flatten(inlines, out)
            }
            Block::List { items, .. } => {
                for item in items {
                    collect_inlines(&item.blocks, out);
                }
            }
            Block::Quote { blocks, .. } | Block::Callout { blocks, .. } => {
                collect_inlines(blocks, out)
            }
            Block::Table { head, rows, .. } => {
                for cell in head {
                    flatten(cell, out);
                }
                for row in rows {
                    for cell in row {
                        flatten(cell, out);
                    }
                }
            }
            Block::Code { .. } | Block::Rule { .. } | Block::Html { .. } => {}
        }
    }
}

fn flatten(inlines: &[Inline], out: &mut Vec<Inline>) {
    for inline in inlines {
        out.push(inline.clone());
        match inline {
            Inline::Emph(inner)
            | Inline::Strong(inner)
            | Inline::Strike(inner)
            | Inline::Highlight(inner)
            | Inline::Link { label: inner, .. } => flatten(inner, out),
            _ => {}
        }
    }
}

fn inlines_of(doc: &Document) -> Vec<Inline> {
    let mut out = Vec::new();
    collect_inlines(&doc.blocks, &mut out);
    out
}

// ── the fixture the whole design exists for ─────────────────────────────────

const FENCED: &str = "\
---
title: Fixture
tags:
  - prova
---

Prosa con [[Nota vera]] e #tag-vero.

```md
Questo e' dentro un fence: [[Nota finta]] e #tag-finto.
```

    E anche qui, indentato: [[Altra finta]] e #anche-finto.

Chiusura.
";

#[test]
fn a_link_and_a_tag_inside_a_code_fence_are_not_picked_up() {
    let doc = read(FENCED);
    let inlines = inlines_of(&doc);

    let targets: Vec<&str> = inlines
        .iter()
        .filter_map(|i| match i {
            Inline::WikiLink { target, .. } => Some(target.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(targets, vec!["Nota vera"], "only the prose link is a link");

    let tags: Vec<&str> = inlines
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(tags, vec!["tag-vero"], "only the prose tag is a tag");
}

#[test]
fn the_fenced_block_keeps_its_language_and_its_text() {
    let doc = read(FENCED);
    let fenced = doc
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Code {
                lang: Some(lang),
                text,
                ..
            } => Some((lang.clone(), text.clone())),
            _ => None,
        })
        .expect("a fenced block");
    assert_eq!(fenced.0, "md");
    assert!(fenced.1.contains("[[Nota finta]]"), "got {:?}", fenced.1);
    assert!(!fenced.1.contains("```"));
}

#[test]
fn frontmatter_survives_the_round_trip_byte_for_byte() {
    let doc = read(FENCED);
    assert_eq!(
        doc.frontmatter.source(),
        Some("title: Fixture\ntags:\n  - prova\n")
    );
    let back = write(&doc);
    assert!(
        back.starts_with("---\ntitle: Fixture\ntags:\n  - prova\n---\n"),
        "got {back}"
    );
}

// ── spans ───────────────────────────────────────────────────────────────────

#[test]
fn a_wikilink_span_points_at_the_original_bytes() {
    let source = "Prosa con [[Nota vera]] dentro.\n";
    let doc = read(source);
    let Some(Inline::WikiLink { span, .. }) = inlines_of(&doc)
        .into_iter()
        .find(|i| matches!(i, Inline::WikiLink { .. }))
    else {
        panic!("expected a wikilink");
    };
    assert_eq!(&source[span.start..span.end], "[[Nota vera]]");
}

#[test]
fn a_span_inside_a_quoted_list_item_still_points_at_the_note() {
    // Two prefixes deep: this is the case a naive "strip and re-parse" gets
    // wrong by exactly the number of stripped bytes per line.
    let source = "> - voce con [[Bersaglio]]\n>   e una continuazione\n";
    let doc = read(source);
    let Some(Inline::WikiLink { span, .. }) = inlines_of(&doc)
        .into_iter()
        .find(|i| matches!(i, Inline::WikiLink { .. }))
    else {
        panic!("expected a wikilink");
    };
    assert_eq!(&source[span.start..span.end], "[[Bersaglio]]");
}

#[test]
fn a_span_after_multibyte_prose_is_a_byte_offset() {
    let source = "Perché città però [[Bersaglio]] qui.\n";
    let doc = read(source);
    let Some(Inline::WikiLink { span, .. }) = inlines_of(&doc)
        .into_iter()
        .find(|i| matches!(i, Inline::WikiLink { .. }))
    else {
        panic!("expected a wikilink");
    };
    assert_eq!(&source[span.start..span.end], "[[Bersaglio]]");
}

// ── structure ───────────────────────────────────────────────────────────────

#[test]
fn a_quote_with_a_callout_header_becomes_a_callout() {
    let doc = read("> [!WARNING]- Attenzione\n> Corpo del callout.\n");
    match doc.blocks.first() {
        Some(Block::Callout {
            kind,
            title,
            folded,
            blocks,
            ..
        }) => {
            assert_eq!(*kind, CalloutKind::Warning);
            assert_eq!(title.as_deref(), Some("Attenzione"));
            assert!(*folded);
            assert_eq!(blocks.len(), 1);
        }
        other => panic!("expected a callout, got {other:?}"),
    }
}

#[test]
fn a_quote_without_a_header_stays_a_quote() {
    let doc = read("> Solo una citazione.\n");
    assert!(matches!(doc.blocks.first(), Some(Block::Quote { .. })));
}

#[test]
fn task_states_are_read_from_the_checkbox() {
    let doc = read("- [ ] da fare\n- [x] fatto\n- semplice\n");
    let Some(Block::List { items, ordered, .. }) = doc.blocks.first() else {
        panic!("expected a list, got {:?}", doc.blocks.first());
    };
    assert!(!ordered);
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].task, Some(TaskState::Todo));
    assert_eq!(items[1].task, Some(TaskState::Done));
    assert_eq!(items[2].task, None);
}

#[test]
fn an_ordered_list_is_marked_ordered() {
    let doc = read("1. uno\n2. due\n");
    let Some(Block::List { ordered, .. }) = doc.blocks.first() else {
        panic!("expected a list");
    };
    assert!(ordered);
}

#[test]
fn a_pipe_table_yields_a_head_and_its_rows() {
    let doc = read("| a | b |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n");
    let Some(Block::Table { head, rows, .. }) = doc.blocks.first() else {
        panic!("expected a table, got {:?}", doc.blocks.first());
    };
    assert_eq!(head.len(), 2);
    assert_eq!(rows.len(), 2);
}

#[test]
fn heading_levels_come_from_the_hashes() {
    let doc = read("# Uno\n\n### Tre\n");
    let levels: Vec<u8> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading { level, .. } => Some(*level),
            _ => None,
        })
        .collect();
    assert_eq!(levels, vec![1, 3]);
}

// ── round trip ──────────────────────────────────────────────────────────────

#[test]
fn a_typical_note_survives_a_read_write_read_cycle() {
    let source = "\
---
title: Nota
---

# Titolo

Prosa con [[Collegamento|alias]], #tag e ==evidenza==.

- [ ] da fare
- [x] fatto

> [!NOTE] Nota a margine
> Corpo.

```rust
let a = 1;
```

| a | b |
| --- | --- |
| 1 | 2 |
";
    let once = write(&read(source));
    let twice = write(&read(&once));
    // The first pass may normalise spacing; the second must not change a thing,
    // which is the property that makes saving a note idempotent.
    assert_eq!(once, twice, "writer is not idempotent\n--- once ---\n{once}");

    let doc = read(&once);
    let inlines = inlines_of(&doc);
    assert!(inlines
        .iter()
        .any(|i| matches!(i, Inline::WikiLink { alias, .. } if alias.as_deref() == Some("alias"))));
    assert!(inlines
        .iter()
        .any(|i| matches!(i, Inline::Tag { name, .. } if name == "tag")));
    assert!(inlines.iter().any(|i| matches!(i, Inline::Highlight(_))));
}
