//! # garrulus-parse
//!
//! The markdown end of the `Reader` / `Writer` seam defined by
//! [`garrulus_ast`]: [`MarkdownReader`] turns Obsidian-dialect markdown into a
//! [`Document`], [`MarkdownWriter`] turns one back into text.
//!
//! [`Document`]: garrulus_ast::prelude::Document
//! [`MarkdownReader`]: reader::MarkdownReader
//! [`MarkdownWriter`]: writer::MarkdownWriter
//!
//! ## How the work is split, and why
//!
//! Markdown is regular enough that a regex gets you 90% of the way, which is
//! exactly why almost every note app mangles a `[[link]]` sitting inside a code
//! fence. The split here is chosen so that the *hard* part — "is this position
//! inside a fence, a quote, a list item, or prose?" — is answered by a real
//! parser, and everything else is pure, cheap and unit-testable:
//!
//! | Layer | Module | Nature |
//! |---|---|---|
//! | Frontmatter fence + minimal YAML | [`frontmatter`] | pure |
//! | Block structure (where things start and end) | [`grammar`] | **Tree-sitter** |
//! | Block details (fence info string, list marker, table cells, heading level) | [`reader`] | pure |
//! | Inline content (emphasis, links, wikilinks, tags, highlights) | [`scan`] | pure |
//! | Callout headers | [`callout`] | pure |
//! | Rendering | [`writer`] | pure |
//!
//! Tree-sitter is therefore asked one narrow question — *give me the kind and
//! the byte range of every block* — and nothing else. That deliberately keeps
//! this crate's exposure to the grammar's internal node names down to a
//! dozen strings, all of them in [`grammar`], because the grammar crate is the
//! one dependency here whose pin cannot be verified without a network.
//!
//! Interiors of block quotes and list items are handled by removing the
//! per-line prefix (`> `, `- `, indentation) and parsing the result again,
//! carrying a line map so every [`Span`] still points at the **original**
//! source bytes. That is what makes a link inside a nested quote clickable in
//! the editor.
//!
//! [`Span`]: garrulus_ast::prelude::Span
//!
//! ## Never fails
//!
//! Neither the reader nor the writer ever returns `Err`. The traits are
//! fallible because other formats will need to be (an org-mode or Notion
//! importer can meet genuinely unreadable input), but markdown has no such
//! thing: every byte is *something*, and a notes app that refuses to open a
//! file is worse than one that shows a paragraph where it hoped for a table.
//! Anything the block layer does not recognise survives verbatim as
//! [`Block::Html`], so no byte is ever dropped.
//!
//! [`Block::Html`]: garrulus_ast::prelude::Block::Html
//!
//! ## Public API: use the [`prelude`]

pub mod callout;
pub mod frontmatter;
pub mod grammar;
pub mod prelude;
pub mod reader;
pub mod scan;
pub mod writer;

mod unprefix;
