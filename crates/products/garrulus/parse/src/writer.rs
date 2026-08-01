//! [`MarkdownWriter`] — a [`Document`] back into markdown.
//!
//! ## The two rules that matter
//!
//! **Frontmatter is reproduced from its source text when it has one.** The
//! model keeps the raw block alongside the parsed pairs precisely so that a
//! note whose metadata nobody touched comes back byte-identical: no re-quoting,
//! no key reordering, no `2026-07-31` turning into `2026-07-31 00:00:00`. Pairs
//! are only rendered when there is no source to reproduce — a note the app
//! itself created.
//!
//! **Text is escaped only where it would otherwise re-parse.** Escaping every
//! `*` and `_` is correct and unreadable, and these files are read and edited
//! by hand in an editor that shows the source. So [`escape_text`] escapes a
//! character when — and only when — leaving it alone would turn prose back into
//! a construct on the next read.

use garrulus_ast::prelude::{
    Block, CalloutKind, Document, FrontValue, Frontmatter, Inline, ListItem, TaskState, WriteError,
    Writer,
};

use crate::callout::format_callout_header;

/// Renders the Obsidian markdown dialect. Round-trips a document read by
/// [`MarkdownReader`](crate::reader::MarkdownReader).
#[derive(Debug, Default, Clone, Copy)]
pub struct MarkdownWriter;

impl Writer for MarkdownWriter {
    /// Never returns `Err`: every document in the model has a markdown
    /// spelling, and `Block::Html` covers whatever did not.
    fn write(&self, doc: &Document) -> Result<String, WriteError> {
        Ok(write_document(doc))
    }
}

/// Render a note. The infallible form of [`Writer::write`].
pub fn write_document(doc: &Document) -> String {
    let mut out = String::new();
    write_frontmatter(&doc.frontmatter, &mut out);
    out.push_str(&blocks(&doc.blocks));
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// `---` fence plus the block. The raw text carries its own trailing newline,
/// which is the convention `split_frontmatter` establishes.
fn write_frontmatter(front: &Frontmatter, out: &mut String) {
    // `source()` rather than the raw text: it returns `None` once any field has
    // been written, which is exactly when echoing the original would be a lie.
    if let Some(raw) = front.source() {
        out.push_str("---\n");
        out.push_str(raw);
        if !raw.is_empty() && !raw.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("---\n\n");
        return;
    }
    if front.is_empty() {
        return;
    }
    out.push_str("---\n");
    for (key, value) in front.iter() {
        write_front_entry(key, value, out);
    }
    out.push_str("---\n\n");
}

fn write_front_entry(key: &str, value: &FrontValue, out: &mut String) {
    match value {
        FrontValue::List(items) => {
            out.push_str(key);
            out.push_str(":\n");
            for item in items {
                out.push_str("  - ");
                out.push_str(&front_scalar(item));
                out.push('\n');
            }
        }
        FrontValue::Map(entries) => {
            out.push_str(key);
            out.push_str(":\n");
            for (sub_key, sub_value) in entries {
                out.push_str("  ");
                out.push_str(sub_key);
                out.push_str(": ");
                out.push_str(&front_scalar(sub_value));
                out.push('\n');
            }
        }
        scalar => {
            out.push_str(key);
            out.push_str(": ");
            out.push_str(&front_scalar(scalar));
            out.push('\n');
        }
    }
}

fn front_scalar(value: &FrontValue) -> String {
    match value {
        FrontValue::Str(text) => {
            // A value that would re-read as something else has to be quoted;
            // everything else stays bare, because bare is what a human writes.
            if text.is_empty() || needs_quotes(text) {
                format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
            } else {
                text.clone()
            }
        }
        FrontValue::Num(n) => format_number(*n),
        FrontValue::Bool(b) => b.to_string(),
        // Nested collections have no single-line spelling here; the block forms
        // above handle the cases that occur.
        FrontValue::List(items) => {
            let rendered: Vec<String> = items.iter().map(front_scalar).collect();
            format!("[{}]", rendered.join(", "))
        }
        FrontValue::Map(_) => String::new(),
    }
}

fn needs_quotes(text: &str) -> bool {
    text.trim() != text
        || text.contains(": ")
        || text.ends_with(':')
        || text.starts_with(['-', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '"', '\''])
        || matches!(text, "true" | "false" | "yes" | "no" | "null" | "~")
        || text.parse::<f64>().is_ok()
}

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

// ── blocks ──────────────────────────────────────────────────────────────────

/// Render a run of blocks, one blank line between them.
fn blocks(items: &[Block]) -> String {
    let rendered: Vec<String> = items.iter().map(block).collect();
    let mut out = rendered.join("\n\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn block(item: &Block) -> String {
    match item {
        Block::Heading { level, inlines, .. } => {
            format!("{} {}", "#".repeat((*level).clamp(1, 6) as usize), inlines_of(inlines))
        }
        Block::Paragraph { inlines, .. } => inlines_of(inlines),
        Block::List { ordered, items, .. } => list(*ordered, items),
        Block::Code { lang, text, .. } => code(lang.as_deref(), text),
        Block::Quote { blocks: inner, .. } => quoted(None, &blocks(inner)),
        Block::Callout {
            kind,
            title,
            folded,
            blocks: inner,
            ..
        } => callout(kind, title.as_deref(), *folded, inner),
        Block::Table { head, rows, .. } => table(head, rows),
        Block::Rule { .. } => "---".to_string(),
        Block::Html { text, .. } => text.clone(),
    }
}

fn code(lang: Option<&str>, text: &str) -> String {
    // The fence has to outrun any backtick run inside, or the block ends early.
    let longest = text
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or(0)
        .max(2);
    let fence = "`".repeat(longest + 1);
    format!("{fence}{}\n{text}\n{fence}", lang.unwrap_or(""))
}

fn list(ordered: bool, items: &[ListItem]) -> String {
    let mut out = String::new();
    for (index, item) in items.iter().enumerate() {
        let marker = if ordered {
            format!("{}. ", index + 1)
        } else {
            "- ".to_string()
        };
        let task = match item.task {
            Some(TaskState::Todo) => "[ ] ",
            Some(TaskState::Done) => "[x] ",
            None => "",
        };
        let body = blocks(&item.blocks);
        let indent = " ".repeat(marker.len());
        for (line_no, line) in body.trim_end().lines().enumerate() {
            if line_no == 0 {
                out.push_str(&marker);
                out.push_str(task);
            } else if !line.is_empty() {
                out.push_str(&indent);
            }
            out.push_str(line);
            out.push('\n');
        }
        // An item with no content at all still has to occupy its line.
        if body.trim().is_empty() {
            out.push_str(&marker);
            out.push_str(task.trim_end());
            out.push('\n');
        }
    }
    out.trim_end().to_string()
}

fn callout(kind: &CalloutKind, title: Option<&str>, folded: bool, inner: &[Block]) -> String {
    let header = format_callout_header(kind, title, folded);
    quoted(Some(&header), &blocks(inner))
}

/// Prefix every line with `> `, optionally after a header line.
fn quoted(header: Option<&str>, body: &str) -> String {
    let mut out = String::new();
    if let Some(header) = header {
        out.push_str("> ");
        out.push_str(header);
        out.push('\n');
    }
    for line in body.trim_end().lines() {
        if line.is_empty() {
            out.push_str(">\n");
        } else {
            out.push_str("> ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out.trim_end().to_string()
}

fn table(head: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>]) -> String {
    let width = head.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if width == 0 {
        return String::new();
    }
    let mut out = row(head, width);
    out.push('\n');
    out.push_str(&format!("|{}", " --- |".repeat(width)));
    for cells in rows {
        out.push('\n');
        out.push_str(&row(cells, width));
    }
    out
}

fn row(cells: &[Vec<Inline>], width: usize) -> String {
    let mut out = String::from("|");
    for index in 0..width {
        out.push(' ');
        if let Some(cell) = cells.get(index) {
            // A literal pipe inside a cell would end it.
            out.push_str(&inlines_of(cell).replace('|', "\\|"));
        }
        out.push_str(" |");
    }
    out
}

// ── inlines ─────────────────────────────────────────────────────────────────

fn inlines_of(items: &[Inline]) -> String {
    let mut out = String::new();
    for item in items {
        inline(item, &mut out);
    }
    out
}

fn inline(item: &Inline, out: &mut String) {
    match item {
        Inline::Text(text) => escape_text(text, out),
        Inline::Emph(inner) => wrap("*", inner, out),
        Inline::Strong(inner) => wrap("**", inner, out),
        Inline::Strike(inner) => wrap("~~", inner, out),
        Inline::Highlight(inner) => wrap("==", inner, out),
        Inline::Code(text) => {
            let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
            let fence = "`".repeat(longest + 1);
            out.push_str(&fence);
            // `` `x` `` needs the padding spaces or the fences merge.
            if text.starts_with('`') || text.ends_with('`') {
                out.push(' ');
                out.push_str(text);
                out.push(' ');
            } else {
                out.push_str(text);
            }
            out.push_str(&fence);
        }
        Inline::WikiLink {
            target,
            heading,
            alias,
            embed,
            ..
        } => {
            if *embed {
                out.push('!');
            }
            out.push_str("[[");
            out.push_str(target);
            if let Some(heading) = heading {
                out.push('#');
                out.push_str(heading);
            }
            if let Some(alias) = alias {
                out.push('|');
                out.push_str(alias);
            }
            out.push_str("]]");
        }
        Inline::Link { href, label, .. } => {
            out.push('[');
            out.push_str(&inlines_of(label));
            out.push_str("](");
            out.push_str(&destination(href));
            out.push(')');
        }
        Inline::Image { src, alt, .. } => {
            out.push_str("![");
            out.push_str(alt);
            out.push_str("](");
            out.push_str(&destination(src));
            out.push(')');
        }
        Inline::Tag { name, .. } => {
            out.push('#');
            out.push_str(name);
        }
        Inline::Break => out.push_str("  \n"),
    }
}

fn wrap(delim: &str, inner: &[Inline], out: &mut String) {
    out.push_str(delim);
    out.push_str(&inlines_of(inner));
    out.push_str(delim);
}

/// A destination with spaces has to be bracketed or the parser stops at the
/// first one and calls the rest a title.
fn destination(href: &str) -> String {
    if href.contains(char::is_whitespace) {
        format!("<{href}>")
    } else {
        href.to_string()
    }
}

/// Escape a character only where leaving it alone would re-parse as a
/// construct on the next read.
///
/// The judgement call here is readability: notes are edited as source, so
/// `2 * 3` must not become `2 \* 3`. A `*` is escaped when it opens emphasis
/// (doubled, or followed by a non-space), never otherwise.
fn escape_text(text: &str, out: &mut String) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if needs_escape(bytes, i) {
            out.push('\\');
        }
        let width = utf8_width(bytes[i]);
        out.push_str(&text[i..i + width]);
        i += width;
    }
}

fn needs_escape(bytes: &[u8], at: usize) -> bool {
    let run = |b: u8| bytes[at..].iter().take_while(|c| **c == b).count();
    let opens = || bytes.get(at + 1).map_or(false, |b| !b.is_ascii_whitespace());
    match bytes[at] {
        b'\\' | b'`' | b'[' => true,
        b'=' | b'~' => run(bytes[at]) >= 2,
        b'*' => run(b'*') >= 2 || opens(),
        // `_` only ever opened emphasis at a word edge, so only escape it there.
        b'_' => (run(b'_') >= 2 || opens()) && !word_byte(bytes, at.wrapping_sub(1)),
        b'#' => !word_byte(bytes, at.wrapping_sub(1)) && tag_byte(bytes, at + 1),
        b'!' => bytes.get(at + 1) == Some(&b'['),
        _ => false,
    }
}

fn word_byte(bytes: &[u8], at: usize) -> bool {
    bytes
        .get(at)
        .map_or(false, |b| b.is_ascii_alphanumeric() || *b == b'_' || *b >= 0x80)
}

fn tag_byte(bytes: &[u8], at: usize) -> bool {
    bytes
        .get(at)
        .map_or(false, |b| b.is_ascii_alphanumeric() || *b >= 0x80)
}

fn utf8_width(b: u8) -> usize {
    match b {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escaped(text: &str) -> String {
        let mut out = String::new();
        escape_text(text, &mut out);
        out
    }

    fn text(s: &str) -> Inline {
        Inline::Text(s.to_string())
    }

    #[test]
    fn prose_that_cannot_re_parse_is_left_alone() {
        assert_eq!(escaped("2 * 3 = 6, snake_case, C# e altro"), "2 * 3 = 6, snake_case, C# e altro");
        assert_eq!(escaped("città perché"), "città perché");
    }

    #[test]
    fn constructs_that_would_re_parse_are_escaped() {
        assert_eq!(escaped("[[Nota]]"), "\\[\\[Nota]]");
        assert_eq!(escaped("#lavoro"), "\\#lavoro");
        assert_eq!(escaped("==evidenza=="), "\\==evidenza\\==");
        // Both stars, not just the first: `\**x\**` would still open emphasis
        // on the surviving one.
        assert_eq!(escaped("**forte**"), "\\*\\*forte\\*\\*");
        assert_eq!(escaped("un `backtick`"), "un \\`backtick\\`");
    }

    #[test]
    fn a_code_fence_outruns_the_backticks_inside_it() {
        assert_eq!(code(Some("rust"), "let a = 1;"), "```rust\nlet a = 1;\n```");
        let nested = code(None, "``` inner ```");
        assert!(nested.starts_with("````\n"), "got {nested}");
    }

    #[test]
    fn an_inline_code_span_is_padded_when_it_touches_a_backtick() {
        let mut out = String::new();
        inline(&Inline::Code("`x`".into()), &mut out);
        assert_eq!(out, "`` `x` ``");
    }

    #[test]
    fn a_quote_keeps_its_blank_lines_as_bare_markers() {
        assert_eq!(quoted(None, "a\n\nb\n"), "> a\n>\n> b");
    }

    #[test]
    fn a_callout_header_leads_the_quote() {
        let out = callout(
            &CalloutKind::Warning,
            Some("Attenzione"),
            true,
            &[Block::Paragraph {
                inlines: vec![text("corpo")],
                span: garrulus_ast::prelude::Span { start: 0, end: 0 },
            }],
        );
        assert_eq!(out, "> [!WARNING]- Attenzione\n> corpo");
    }

    #[test]
    fn list_continuation_lines_hang_from_the_marker() {
        let items = vec![ListItem {
            task: Some(TaskState::Done),
            blocks: vec![
                Block::Paragraph {
                    inlines: vec![text("prima")],
                    span: garrulus_ast::prelude::Span { start: 0, end: 0 },
                },
                Block::Paragraph {
                    inlines: vec![text("seconda")],
                    span: garrulus_ast::prelude::Span { start: 0, end: 0 },
                },
            ],
            span: garrulus_ast::prelude::Span { start: 0, end: 0 },
        }];
        assert_eq!(list(false, &items), "- [x] prima\n\n  seconda");
    }

    #[test]
    fn a_table_pads_short_rows_and_escapes_pipes() {
        let out = table(
            &[vec![text("a")], vec![text("b")]],
            &[vec![vec![text("x|y")]]],
        );
        assert_eq!(out, "| a | b |\n| --- | --- |\n| x\\|y |  |");
    }

    #[test]
    fn a_wikilink_round_trips_its_parts() {
        let mut out = String::new();
        inline(
            &Inline::WikiLink {
                target: "Nota A".into(),
                heading: Some("Sezione".into()),
                alias: Some("qui".into()),
                embed: true,
                span: garrulus_ast::prelude::Span { start: 0, end: 0 },
            },
            &mut out,
        );
        assert_eq!(out, "![[Nota A#Sezione|qui]]");
    }

    #[test]
    fn frontmatter_is_reproduced_from_its_source_text() {
        let front = Frontmatter::from_source("title: Nota\ndue: 2026-07-31\n", Vec::new());
        let mut out = String::new();
        write_frontmatter(&front, &mut out);
        assert_eq!(out, "---\ntitle: Nota\ndue: 2026-07-31\n---\n\n");
    }

    #[test]
    fn frontmatter_is_rendered_from_pairs_only_when_there_is_no_source() {
        let front = Frontmatter::from_entries(vec![
            ("title".into(), FrontValue::Str("Nota".into())),
            (
                "tags".into(),
                FrontValue::List(vec![FrontValue::Str("a".into())]),
            ),
            ("peso".into(), FrontValue::Num(3.0)),
            ("data".into(), FrontValue::Str("2026-07-31".into())),
        ]);
        let mut out = String::new();
        write_frontmatter(&front, &mut out);
        assert_eq!(
            out,
            "---\ntitle: Nota\ntags:\n  - a\npeso: 3\ndata: 2026-07-31\n---\n\n"
        );
    }
}
