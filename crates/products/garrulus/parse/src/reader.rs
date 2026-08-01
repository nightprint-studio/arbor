//! [`MarkdownReader`] — markdown source into a [`Document`].
//!
//! The block layer hands over `(kind, byte range)` pairs and this module reads
//! the details out of the source slice itself: a heading's level from its
//! hashes, a fence's language from its info string, a list item's task box from
//! its first three bytes. Nothing here inspects a Tree-sitter node, which is
//! what keeps the grammar swappable (see [`grammar`](crate::grammar)).
//!
//! Quotes and list items are handled by removing their per-line prefix and
//! parsing the result as a document of its own, carrying an
//! [`Offsets`] map so spans keep pointing at the original note.

use garrulus_ast::prelude::{
    Block, Document, Inline, ListItem, ReadError, Reader, Span, TaskState,
};

use crate::callout::parse_callout_header;
use crate::frontmatter::{build_frontmatter, split_frontmatter};
use crate::grammar::{self, GKind, GNode};
use crate::scan::scan_inlines;
use crate::unprefix::{Offsets, Unprefixed};

/// How deep quotes and list items may nest before the reader stops recursing
/// and keeps the rest verbatim. Well past anything a human writes; there only
/// so that a pathological file cannot blow the stack.
const MAX_DEPTH: usize = 16;

/// Reads the Obsidian markdown dialect: YAML frontmatter, `[[wikilinks]]`,
/// `#tags`, `==highlights==`, `> [!NOTE]` callouts, task lists, pipe tables.
#[derive(Debug, Default, Clone, Copy)]
pub struct MarkdownReader;

impl Reader for MarkdownReader {
    /// Never returns `Err`. See the crate docs: every byte of markdown is
    /// *something*, and unrecognised input survives as `Block::Html` rather
    /// than failing a note open.
    fn read(&self, source: &str) -> Result<Document, ReadError> {
        Ok(read_document(source))
    }
}

/// Read a note. The infallible form of [`Reader::read`].
pub fn read_document(source: &str) -> Document {
    let (front, body, body_at) = split_frontmatter(source);
    Document {
        frontmatter: build_frontmatter(front),
        blocks: parse_fragment(body, &Offsets::Linear(body_at), 0),
    }
}

/// Parse a self-contained run of markdown — the note body, or the de-prefixed
/// interior of a quote or list item.
fn parse_fragment(text: &str, offs: &Offsets<'_>, depth: usize) -> Vec<Block> {
    if depth > MAX_DEPTH {
        return verbatim(text, 0, text.len(), offs).into_iter().collect();
    }
    let Some(root) = grammar::parse_blocks(text) else {
        return verbatim(text, 0, text.len(), offs).into_iter().collect();
    };
    let mut nodes = root.children;
    grammar::fill_gaps(&mut nodes, text);
    nodes
        .iter()
        .filter_map(|node| block_from(node, text, offs, depth))
        .collect()
}

fn block_from(node: &GNode, text: &str, offs: &Offsets<'_>, depth: usize) -> Option<Block> {
    let (start, end) = (node.span.start, node.span.end);
    let raw = text.get(start..end)?;
    match node.kind {
        // Containers are flattened by the block layer and never reach here.
        GKind::Container => None,
        GKind::Heading => Some(heading(raw, start, offs)),
        GKind::Paragraph => paragraph(raw, start, offs),
        GKind::FencedCode => Some(fenced_code(raw, start, offs)),
        GKind::IndentedCode => Some(indented_code(raw, start, offs)),
        GKind::Quote => Some(quote(raw, start, offs, depth)),
        GKind::List => Some(list(node, text, offs, depth)),
        // A bare item outside a list: keep it, as a list of one.
        GKind::ListItem => Some(Block::List {
            ordered: is_ordered(raw),
            items: vec![list_item(node, text, offs, depth)],
            span: tight_span(text, start, end, offs),
        }),
        GKind::Table => Some(table(raw, start, offs)),
        GKind::Rule => Some(Block::Rule {
            span: tight_span(text, start, end, offs),
        }),
        GKind::Verbatim => verbatim(text, start, end, offs),
    }
}

// ── blocks ──────────────────────────────────────────────────────────────────

fn heading(raw: &str, start: usize, offs: &Offsets<'_>) -> Block {
    let indent = leading_blank(raw);
    let hashes = raw[indent..].bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&hashes) {
        let (mut from, mut to) = trimmed_bounds(raw, indent + hashes, raw.len());
        // A closing run of hashes is decoration, not content: `## Titolo ##`.
        let closing = raw[from..to].bytes().rev().take_while(|b| *b == b'#').count();
        if closing > 0 && raw[from..to - closing].ends_with([' ', '\t']) {
            to -= closing;
            (from, to) = trimmed_bounds(raw, from, to);
        }
        return Block::Heading {
            level: hashes as u8,
            inlines: scan_inlines(&raw[from..to], offs.at(start + from)),
            span: abs(start + indent, start + trimmed_bounds(raw, 0, raw.len()).1, offs),
        };
    }
    match setext_underline(raw) {
        Some((level, underline_at)) => {
            let (from, to) = trimmed_bounds(raw, 0, underline_at);
            Block::Heading {
                level,
                inlines: scan_inlines(&raw[from..to], offs.at(start + from)),
                span: abs(start + from, start + trimmed_bounds(raw, 0, raw.len()).1, offs),
            }
        }
        // The grammar called it a heading and it looks like neither form; a
        // paragraph loses no text and no reader will notice.
        None => paragraph(raw, start, offs).unwrap_or(Block::Rule {
            span: abs(start, start + raw.len(), offs),
        }),
    }
}

/// `(level, byte offset of the underline)` for a setext heading.
fn setext_underline(raw: &str) -> Option<(u8, usize)> {
    let (at, line) = lines_with_offsets(raw)
        .into_iter()
        .rev()
        .find(|(_, line)| !line.trim().is_empty())?;
    let trimmed = line.trim();
    let first = trimmed.chars().next()?;
    if (first == '=' || first == '-') && trimmed.chars().all(|c| c == first) {
        Some((if first == '=' { 1 } else { 2 }, at))
    } else {
        None
    }
}

fn paragraph(raw: &str, start: usize, offs: &Offsets<'_>) -> Option<Block> {
    let (from, to) = trimmed_bounds(raw, 0, raw.len());
    if from == to {
        return None;
    }
    Some(Block::Paragraph {
        inlines: scan_inlines(&raw[from..to], offs.at(start + from)),
        span: abs(start + from, start + to, offs),
    })
}

fn fenced_code(raw: &str, start: usize, offs: &Offsets<'_>) -> Block {
    let open_end = raw.find('\n').map_or(raw.len(), |i| i + 1);
    let indent = leading_blank(raw).min(open_end);
    let opener = &raw[indent..open_end];
    let fence = opener.as_bytes().first().copied().unwrap_or(b'`');
    let width = opener.bytes().take_while(|b| *b == fence).count();
    let info = opener[width..].trim();
    // Obsidian writes `lang` but also `rust,ignore` and `js title="x"`; only the
    // first token is the language, the rest belongs to the renderer.
    let lang = info
        .split([' ', '\t', ','])
        .next()
        .filter(|l| !l.is_empty())
        .map(str::to_string);

    let mut text = String::new();
    for (at, line) in lines_with_offsets(raw) {
        if at < open_end {
            continue;
        }
        if closes_fence(line, fence, width) {
            break;
        }
        text.push_str(strip_indent(line, indent));
        text.push('\n');
    }
    drop_final_newline(&mut text);
    Block::Code {
        lang,
        text,
        span: abs(start + indent, start + trimmed_bounds(raw, 0, raw.len()).1, offs),
    }
}

fn closes_fence(line: &str, fence: u8, width: usize) -> bool {
    if width < 3 {
        return false;
    }
    let body = line.trim_start_matches([' ', '\t']);
    let run = body.bytes().take_while(|b| *b == fence).count();
    run >= width && body[run..].trim().is_empty()
}

fn indented_code(raw: &str, start: usize, offs: &Offsets<'_>) -> Block {
    let mut text = String::new();
    for (_, line) in lines_with_offsets(raw) {
        text.push_str(strip_indent(line, 4));
        text.push('\n');
    }
    drop_final_newline(&mut text);
    let (from, to) = trimmed_bounds(raw, 0, raw.len());
    Block::Code {
        lang: None,
        text,
        span: abs(start + from, start + to, offs),
    }
}

fn quote(raw: &str, start: usize, offs: &Offsets<'_>, depth: usize) -> Block {
    let span = tight(raw, start, offs);
    let inner = Unprefixed::build(raw, start, offs, |_, line| quote_marker(line));
    let mapped = Offsets::Mapped {
        map: &inner,
        shift: 0,
    };
    let first_end = inner.text.find('\n').map_or(inner.text.len(), |i| i + 1);
    match parse_callout_header(&inner.text[..first_end]) {
        Some(header) => Block::Callout {
            kind: header.kind,
            title: header.title,
            folded: header.folded,
            blocks: parse_fragment(
                &inner.text[first_end..],
                &mapped.sub(first_end),
                depth + 1,
            ),
            span,
        },
        None => Block::Quote {
            blocks: parse_fragment(&inner.text, &mapped, depth + 1),
            span,
        },
    }
}

/// Bytes to drop for `>`, `> `, or an indented variant of either.
fn quote_marker(line: &str) -> usize {
    let indent = leading_blank(line);
    match line[indent..].strip_prefix('>') {
        Some(after) => indent + 1 + usize::from(after.starts_with(' ')),
        None => 0,
    }
}

fn list(node: &GNode, text: &str, offs: &Offsets<'_>, depth: usize) -> Block {
    let items: Vec<&GNode> = node
        .children
        .iter()
        .filter(|child| child.kind == GKind::ListItem)
        .collect();
    let ordered = items
        .first()
        .and_then(|item| text.get(item.span.start..item.span.end))
        .map_or(false, is_ordered);
    Block::List {
        ordered,
        items: items
            .into_iter()
            .map(|item| list_item(item, text, offs, depth))
            .collect(),
        span: tight_span(text, node.span.start, node.span.end, offs),
    }
}

fn list_item(node: &GNode, text: &str, offs: &Offsets<'_>, depth: usize) -> ListItem {
    let raw = text.get(node.span.start..node.span.end).unwrap_or("");
    // The first line's marker sets the column every continuation line hangs
    // from; a deeper line keeps its extra indent, which is how a nested list
    // stays nested.
    let mut column = 0usize;
    let inner = Unprefixed::build(raw, node.span.start, offs, |line_no, line| {
        if line_no == 0 {
            column = marker_width(line);
            column
        } else {
            leading_blank(line).min(column)
        }
    });
    let (task, content_at) = task_marker(&inner.text);
    let mapped = Offsets::Mapped {
        map: &inner,
        shift: content_at,
    };
    ListItem {
        task,
        blocks: parse_fragment(&inner.text[content_at..], &mapped, depth + 1),
        span: tight_span(text, node.span.start, node.span.end, offs),
    }
}

fn table(raw: &str, start: usize, offs: &Offsets<'_>) -> Block {
    let mut head: Vec<Vec<Inline>> = Vec::new();
    let mut rows: Vec<Vec<Vec<Inline>>> = Vec::new();
    let mut seen_head = false;
    for (at, line) in lines_with_offsets(raw) {
        if line.trim().is_empty() || is_delimiter_row(line) {
            continue;
        }
        let cells: Vec<Vec<Inline>> = split_row(line)
            .into_iter()
            .map(|(from, to)| scan_inlines(&line[from..to], offs.at(start + at + from)))
            .collect();
        if seen_head {
            rows.push(cells);
        } else {
            head = cells;
            seen_head = true;
        }
    }
    Block::Table {
        head,
        rows,
        span: tight(raw, start, offs),
    }
}

fn verbatim(text: &str, start: usize, end: usize, offs: &Offsets<'_>) -> Option<Block> {
    let raw = text.get(start..end)?;
    let (from, to) = trimmed_bounds(raw, 0, raw.len());
    if from == to {
        return None;
    }
    Some(Block::Html {
        text: raw[from..to].to_string(),
        span: abs(start + from, start + to, offs),
    })
}

// ── source-slice helpers, all pure ──────────────────────────────────────────

fn abs(from: usize, to: usize, offs: &Offsets<'_>) -> Span {
    Span {
        start: offs.at(from),
        end: offs.at(to),
    }
}

/// The span of `raw` with surrounding whitespace excluded.
fn tight(raw: &str, start: usize, offs: &Offsets<'_>) -> Span {
    let (from, to) = trimmed_bounds(raw, 0, raw.len());
    abs(start + from, start + to, offs)
}

fn tight_span(text: &str, start: usize, end: usize, offs: &Offsets<'_>) -> Span {
    match text.get(start..end) {
        Some(raw) => tight(raw, start, offs),
        None => abs(start, end, offs),
    }
}

/// `raw[from..to]` with leading and trailing whitespace excluded, as bounds
/// **into `raw`**.
fn trimmed_bounds(raw: &str, from: usize, to: usize) -> (usize, usize) {
    let slice = &raw[from..to];
    let start = from + (slice.len() - slice.trim_start().len());
    let end = to - (slice.len() - slice.trim_end().len());
    (start.min(end), end)
}

fn leading_blank(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

fn strip_indent(line: &str, max: usize) -> &str {
    let drop = line
        .bytes()
        .take(max)
        .take_while(|b| *b == b' ' || *b == b'\t')
        .count();
    &line[drop..]
}

fn drop_final_newline(text: &mut String) {
    if text.ends_with('\n') {
        text.pop();
        if text.ends_with('\r') {
            text.pop();
        }
    }
}

/// Lines of `text` **without** their newline, each with its byte offset.
///
/// Materialised rather than lazy on purpose: the offset is accumulated as the
/// iterator advances, so a lazy version would hand back nonsense the moment a
/// caller walked it backwards — and one caller does.
fn lines_with_offsets(text: &str) -> Vec<(usize, &str)> {
    let mut at = 0usize;
    text.split_inclusive('\n')
        .map(|line| {
            let start = at;
            at += line.len();
            (start, line.trim_end_matches(['\n', '\r']))
        })
        .collect()
}

/// Bytes occupied by a list item's marker and the space after it.
fn marker_width(line: &str) -> usize {
    let indent = leading_blank(line);
    let rest = &line[indent..];
    let marker = if rest.starts_with(['-', '*', '+']) {
        1
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || !rest[digits..].starts_with(['.', ')']) {
            return 0;
        }
        digits + 1
    };
    let after = &rest[marker..];
    indent + marker + (after.len() - after.trim_start_matches([' ', '\t']).len())
}

fn is_ordered(raw: &str) -> bool {
    let trimmed = raw.trim_start_matches([' ', '\t']);
    trimmed.starts_with(|c: char| c.is_ascii_digit())
}

/// `[ ]` / `[x]` at the head of an item's content.
fn task_marker(text: &str) -> (Option<TaskState>, usize) {
    let state = match text.get(..3) {
        Some("[ ]") => TaskState::Todo,
        Some("[x]") | Some("[X]") => TaskState::Done,
        _ => return (None, 0),
    };
    let skip = 3 + usize::from(text[3..].starts_with(' '));
    (Some(state), skip)
}

fn is_delimiter_row(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed.contains('-')
        && trimmed
            .chars()
            .all(|c| matches!(c, '-' | ':' | '|' | ' ' | '\t'))
}

/// Cell bounds within a pipe-table row, honouring `\|` and inline code.
fn split_row(line: &str) -> Vec<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut cells = Vec::new();
    let mut cell_start = 0usize;
    let mut in_code = false;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'`' => in_code = !in_code,
            b'|' if !in_code => {
                cells.push((cell_start, i));
                cell_start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    cells.push((cell_start, bytes.len()));
    // The leading and trailing pipes of `| a | b |` produce empty edge cells.
    if cells
        .first()
        .is_some_and(|(from, to)| line[*from..*to].trim().is_empty())
    {
        cells.remove(0);
    }
    if cells.len() > 1
        && cells
            .last()
            .is_some_and(|(from, to)| line[*from..*to].trim().is_empty())
    {
        cells.pop();
    }
    cells
        .into_iter()
        .map(|(from, to)| trimmed_bounds(line, from, to))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_carry_their_byte_offsets_and_lose_their_newline() {
        assert_eq!(
            lines_with_offsets("a\nbb\r\nc"),
            vec![(0, "a"), (2, "bb"), (6, "c")]
        );
    }

    #[test]
    fn a_quote_marker_is_one_optional_space_wide() {
        assert_eq!(quote_marker("> testo"), 2);
        assert_eq!(quote_marker(">testo"), 1);
        assert_eq!(quote_marker("  > testo"), 4);
        assert_eq!(quote_marker("testo"), 0);
    }

    #[test]
    fn marker_width_covers_both_list_flavours() {
        assert_eq!(marker_width("- voce"), 2);
        assert_eq!(marker_width("  * voce"), 4);
        assert_eq!(marker_width("12. voce"), 4);
        assert_eq!(marker_width("1) voce"), 3);
        assert_eq!(marker_width("non una voce"), 0);
    }

    #[test]
    fn task_markers_are_read_and_skipped() {
        assert_eq!(task_marker("[ ] fare"), (Some(TaskState::Todo), 4));
        assert_eq!(task_marker("[x] fatto"), (Some(TaskState::Done), 4));
        assert_eq!(task_marker("[X]fatto"), (Some(TaskState::Done), 3));
        assert_eq!(task_marker("testo"), (None, 0));
    }

    #[test]
    fn table_rows_split_on_unescaped_pipes_outside_code() {
        let line = "| a | b `x|y` | c \\| d |";
        let cells: Vec<&str> = split_row(line)
            .into_iter()
            .map(|(from, to)| &line[from..to])
            .collect();
        assert_eq!(cells, vec!["a", "b `x|y`", "c \\| d"]);
    }

    #[test]
    fn a_row_without_edge_pipes_still_splits() {
        let line = "a | b";
        let cells: Vec<&str> = split_row(line)
            .into_iter()
            .map(|(from, to)| &line[from..to])
            .collect();
        assert_eq!(cells, vec!["a", "b"]);
    }

    #[test]
    fn delimiter_rows_are_recognised_and_headings_are_not() {
        assert!(is_delimiter_row("| --- | :-: |"));
        assert!(!is_delimiter_row("| a | b |"));
        assert!(!is_delimiter_row(""));
    }

    #[test]
    fn setext_underlines_are_detected_by_their_own_line() {
        assert_eq!(setext_underline("Titolo\n===\n"), Some((1, 7)));
        assert_eq!(setext_underline("Titolo\n---\n"), Some((2, 7)));
        assert_eq!(setext_underline("Titolo\n"), None);
    }

    #[test]
    fn a_closing_fence_needs_at_least_the_opening_width() {
        assert!(closes_fence("```", b'`', 3));
        assert!(closes_fence("  ````  ", b'`', 3));
        assert!(!closes_fence("``", b'`', 3));
        assert!(!closes_fence("``` rust", b'`', 3));
    }
}
