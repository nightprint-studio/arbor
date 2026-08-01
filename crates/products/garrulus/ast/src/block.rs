//! [`Block`] — the block-level structure of a note, plus tasks and callouts.
//!
//! Every variant is a thing a note *has*, not a thing markdown *writes*: a
//! [`Block::Rule`], not "three dashes"; a [`Block::Callout`] with a
//! [`CalloutKind`], not a quote whose first line happens to start with `[!`.
//! [`Block::Html`] is the deliberate exception and the escape hatch — raw markup a
//! reader could not interpret, kept verbatim so writing it back is lossless.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::inline::Inline;
use crate::span::Span;

/// One block-level element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Block {
    /// A section heading. `level` is 1–6; the reader clamps, nothing downstream
    /// should have to defend against a level 0 or 9.
    Heading {
        /// Depth, 1 (most significant) through 6.
        level: u8,
        /// Heading text.
        inlines: Vec<Inline>,
        /// Byte range of the whole heading line.
        span: Span,
    },
    /// A run of prose.
    Paragraph {
        /// Paragraph content.
        inlines: Vec<Inline>,
        /// Byte range of the paragraph.
        span: Span,
    },
    /// A bullet or numbered list. Nesting lives inside [`ListItem::blocks`], so a
    /// sub-list is a `List` block inside its parent item — the same shape the
    /// outline and the task extractor already walk.
    List {
        /// `true` for numbered lists.
        ordered: bool,
        /// The items, in document order.
        items: Vec<ListItem>,
        /// Byte range of the whole list.
        span: Span,
    },
    /// A fenced or indented code block.
    Code {
        /// Info string, lowercased by the reader when present. Drives syntax
        /// highlighting and the "open in Picus / Merula / Bennu" block actions.
        lang: Option<String>,
        /// Body, without the fences and without the trailing fence newline.
        text: String,
        /// Byte range of the whole block, fences included.
        span: Span,
    },
    /// A block quote.
    Quote {
        /// Quoted content.
        blocks: Vec<Block>,
        /// Byte range of the whole quote.
        span: Span,
    },
    /// An Obsidian callout: a quote with a kind, an optional title and a fold
    /// state. Modelled apart from [`Block::Quote`] because it renders, folds and
    /// exports differently, and because a reader that cannot produce callouts
    /// simply never emits this variant.
    Callout {
        /// Which callout this is.
        kind: CalloutKind,
        /// Title on the marker line. `None` means "use the kind's own name".
        title: Option<String>,
        /// `true` when the note asks for it to start collapsed (`[!WARNING]-`).
        folded: bool,
        /// Callout body.
        blocks: Vec<Block>,
        /// Byte range of the whole callout.
        span: Span,
    },
    /// A table. `head` is one row of cells; each cell is a run of inlines.
    Table {
        /// Header cells. Empty when the table has no header row.
        head: Vec<Vec<Inline>>,
        /// Body rows, each a list of cells.
        rows: Vec<Vec<Vec<Inline>>>,
        /// Byte range of the whole table.
        span: Span,
    },
    /// A thematic break.
    Rule {
        /// Byte range of the rule.
        span: Span,
    },
    /// Raw markup kept verbatim. The lossless escape hatch: whatever the reader
    /// could not model, the writer can still put back exactly as it found it.
    Html {
        /// The markup, unmodified.
        text: String,
        /// Byte range of the markup.
        span: Span,
    },
}

impl Block {
    /// The byte range of this block. Every block variant carries one, which is
    /// what lets a refactor splice at block granularity.
    pub fn span(&self) -> Span {
        match self {
            Block::Heading { span, .. }
            | Block::Paragraph { span, .. }
            | Block::List { span, .. }
            | Block::Code { span, .. }
            | Block::Quote { span, .. }
            | Block::Callout { span, .. }
            | Block::Table { span, .. }
            | Block::Rule { span }
            | Block::Html { span, .. } => *span,
        }
    }

    /// The blocks nested directly inside this one, empty for the leaves.
    ///
    /// List items are *not* returned here: an item is not a block, and flattening
    /// it away would lose its task state. Walk [`Block::List`] through
    /// [`crate::walk`], which handles both.
    pub fn children(&self) -> &[Block] {
        match self {
            Block::Quote { blocks, .. } | Block::Callout { blocks, .. } => blocks,
            _ => &[],
        }
    }
}

/// One entry of a [`Block::List`].
///
/// An item is a small container of blocks rather than a run of inlines, because a
/// task can hold a paragraph, a nested list and a code sample, and pretending
/// otherwise is where list handling usually goes wrong.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListItem {
    /// `Some` when the item is a checkbox — that is what makes it a task.
    pub task: Option<TaskState>,
    /// Item content.
    pub blocks: Vec<Block>,
    /// Byte range of the item, marker included.
    pub span: Span,
}

impl ListItem {
    /// A plain, unchecked list item holding `blocks`.
    pub fn new(blocks: Vec<Block>, span: Span) -> Self {
        Self { task: None, blocks, span }
    }

    /// Whether this item is a checkbox at all.
    pub fn is_task(&self) -> bool {
        self.task.is_some()
    }
}

/// Whether a task is done.
///
/// Two states on purpose. Obsidian's custom checkbox characters (`[/]`, `[-]`,
/// `[?]`) are a theme convention, not a data model, and admitting them here would
/// spread an open-ended enum through the whole task surface. If they ever become
/// first-class they arrive as a note-type field, which is typed and filterable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    /// `- [ ]`
    Todo,
    /// `- [x]`
    Done,
}

impl TaskState {
    /// Whether the task is complete.
    pub fn is_done(self) -> bool {
        matches!(self, TaskState::Done)
    }
}

/// Which callout a [`Block::Callout`] is.
///
/// The spelling is Obsidian's, because vault compatibility is a hard constraint:
/// parsing is case-insensitive, writing is uppercase, and anything unrecognised
/// survives as [`CalloutKind::Other`] rather than being flattened to a note — a
/// vault full of `[!ABSTRACT]` must not come back changed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CalloutKind {
    /// `[!NOTE]`
    Note,
    /// `[!TIP]`
    Tip,
    /// `[!INFO]`
    Info,
    /// `[!WARNING]`
    Warning,
    /// `[!DANGER]`
    Danger,
    /// `[!QUESTION]`
    Question,
    /// `[!EXAMPLE]`
    Example,
    /// `[!QUOTE]`
    Quote,
    /// Any other kind, stored already uppercased so that parse-then-write is
    /// byte-identical for callouts Garrulus does not know about.
    Other(String),
}

impl CalloutKind {
    /// The Obsidian spelling, uppercase — exactly what goes back on disk.
    pub fn as_str(&self) -> &str {
        match self {
            CalloutKind::Note => "NOTE",
            CalloutKind::Tip => "TIP",
            CalloutKind::Info => "INFO",
            CalloutKind::Warning => "WARNING",
            CalloutKind::Danger => "DANGER",
            CalloutKind::Question => "QUESTION",
            CalloutKind::Example => "EXAMPLE",
            CalloutKind::Quote => "QUOTE",
            CalloutKind::Other(raw) => raw,
        }
    }

    /// Whether this is a kind Garrulus renders with a dedicated icon and accent.
    pub fn is_known(&self) -> bool {
        !matches!(self, CalloutKind::Other(_))
    }

    /// Every kind with first-class rendering, in the order the UI offers them.
    pub const KNOWN: [CalloutKind; 8] = [
        CalloutKind::Note,
        CalloutKind::Tip,
        CalloutKind::Info,
        CalloutKind::Warning,
        CalloutKind::Danger,
        CalloutKind::Question,
        CalloutKind::Example,
        CalloutKind::Quote,
    ];
}

impl fmt::Display for CalloutKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CalloutKind {
    /// Parsing never fails: an unknown kind is data, not an error.
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let upper = s.trim().to_uppercase();
        Ok(match upper.as_str() {
            "NOTE" => CalloutKind::Note,
            "TIP" => CalloutKind::Tip,
            "INFO" => CalloutKind::Info,
            "WARNING" => CalloutKind::Warning,
            "DANGER" => CalloutKind::Danger,
            "QUESTION" => CalloutKind::Question,
            "EXAMPLE" => CalloutKind::Example,
            "QUOTE" => CalloutKind::Quote,
            // Normalised on the way in so `Display` is the exact inverse.
            _ => CalloutKind::Other(upper),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> CalloutKind {
        s.parse().expect("CalloutKind parsing is infallible")
    }

    #[test]
    fn known_kinds_round_trip_through_their_obsidian_spelling() {
        for kind in CalloutKind::KNOWN {
            let written = kind.to_string();
            assert_eq!(parse(&written), kind, "round trip failed for {written}");
        }
    }

    #[test]
    fn parsing_is_case_insensitive_and_writing_is_uppercase() {
        assert_eq!(parse("warning"), CalloutKind::Warning);
        assert_eq!(parse("  Tip "), CalloutKind::Tip);
        assert_eq!(parse("InFo").to_string(), "INFO");
    }

    #[test]
    fn an_unknown_kind_survives_normalised_and_round_trips() {
        let kind = parse("abstract");
        assert_eq!(kind, CalloutKind::Other("ABSTRACT".into()));
        assert!(!kind.is_known());
        // The invariant that matters: a vault full of unknown callouts comes
        // back unchanged.
        assert_eq!(parse(&kind.to_string()), kind);
    }

    #[test]
    fn children_returns_nested_blocks_only_for_containers() {
        let inner = Block::Rule { span: Span::EMPTY };
        let quote = Block::Quote { blocks: vec![inner.clone()], span: Span::new(0, 8) };
        assert_eq!(quote.children(), &[inner]);
        assert_eq!(quote.span(), Span::new(0, 8));
        assert!(Block::Rule { span: Span::EMPTY }.children().is_empty());
    }

    #[test]
    fn a_list_item_is_a_task_only_when_it_has_a_checkbox() {
        let plain = ListItem::new(vec![], Span::EMPTY);
        assert!(!plain.is_task());
        let done = ListItem { task: Some(TaskState::Done), ..plain };
        assert!(done.is_task());
        assert!(done.task.expect("just set").is_done());
    }
}
