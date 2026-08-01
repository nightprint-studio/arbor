//! The inline scanner: a run of prose → [`Inline`]s.
//!
//! ## Why this exists at all
//!
//! Three of the constructs Garrulus cares most about — `[[wikilinks]]`,
//! `#tags` and `==highlight==` — are **not in the markdown grammar**. They are
//! Obsidian extensions, so no off-the-shelf parser will hand them over and
//! something has to walk the text.
//!
//! The thing that makes this honest rather than "regex with extra steps" is
//! *what it is handed*. The block layer has already decided what is prose and
//! what is not, so a fenced code block never reaches this function: a `#tag`
//! inside a fence stays text because the scanner is never pointed at it. Inside
//! prose, the scanner still has one hazard of its own — the inline code span —
//! and it handles it by giving backticks the highest precedence and by skipping
//! over code spans when it looks for a closing delimiter.
//!
//! ## Spans
//!
//! Only the four navigable inlines carry a [`Span`] in the model, and they get
//! **absolute** source offsets: callers pass `base`, the byte offset of `text`
//! inside the note. That is what lets the editor turn a wikilink three levels
//! deep inside a quoted list item into a clickable range.

use garrulus_ast::prelude::{Inline, Span};

/// Scan a run of prose. `base` is the byte offset of `text` within the note, so
/// that every emitted span points at the original source.
pub fn scan_inlines(text: &str, base: usize) -> Vec<Inline> {
    Scanner {
        src: text,
        bytes: text.as_bytes(),
        base,
        pos: 0,
        out: Vec::new(),
        buf: String::new(),
    }
    .run()
}

struct Scanner<'a> {
    src: &'a str,
    bytes: &'a [u8],
    base: usize,
    pos: usize,
    out: Vec<Inline>,
    buf: String,
}

impl<'a> Scanner<'a> {
    fn run(mut self) -> Vec<Inline> {
        while self.pos < self.bytes.len() {
            if !self.construct() {
                self.eat_char();
            }
        }
        self.flush();
        self.out
    }

    /// Try every construct that can start at `pos`, in precedence order.
    fn construct(&mut self) -> bool {
        match self.bytes[self.pos] {
            b'\\' => self.escape(),
            b'`' => self.code_span(),
            b'!' => self.bang(),
            b'[' => self.bracket(),
            b'=' => self.paired(b'=', 2, Inline::Highlight),
            b'~' => self.paired(b'~', 2, Inline::Strike),
            b'*' | b'_' => self.emphasis(),
            b'#' => self.tag(),
            b'<' => self.autolink(),
            b'\n' | b'\r' => self.hard_break(),
            _ => false,
        }
    }

    fn eat_char(&mut self) {
        let len = char_len(self.bytes[self.pos]);
        self.buf.push_str(&self.src[self.pos..self.pos + len]);
        self.pos += len;
    }

    fn flush(&mut self) {
        if !self.buf.is_empty() {
            self.out.push(Inline::Text(std::mem::take(&mut self.buf)));
        }
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span {
            start: self.base + start,
            end: self.base + end,
        }
    }

    // ── constructs ──────────────────────────────────────────────────────────

    fn escape(&mut self) -> bool {
        match self.bytes.get(self.pos + 1) {
            Some(b'\n') => {
                self.flush();
                self.out.push(Inline::Break);
                self.pos += 2;
                true
            }
            Some(c) if c.is_ascii_punctuation() => {
                self.buf.push(*c as char);
                self.pos += 2;
                true
            }
            _ => false,
        }
    }

    fn code_span(&mut self) -> bool {
        let ticks = run_len(self.bytes, self.pos, b'`');
        let start = self.pos + ticks;
        let Some(close) = exact_run(self.bytes, start, b'`', ticks) else {
            return false;
        };
        let mut content = &self.src[start..close];
        // CommonMark: one space is stripped from each end when both are present
        // and the span is not all spaces — that is what makes `` ` `` work.
        if content.len() >= 2
            && content.starts_with(' ')
            && content.ends_with(' ')
            && !content.trim().is_empty()
        {
            content = &content[1..content.len() - 1];
        }
        self.flush();
        self.out.push(Inline::Code(content.to_string()));
        self.pos = close + ticks;
        true
    }

    fn bang(&mut self) -> bool {
        let rest = &self.src[self.pos..];
        if rest.starts_with("![[") {
            self.wiki_link(true)
        } else if rest.starts_with("![") {
            self.image()
        } else {
            false
        }
    }

    fn bracket(&mut self) -> bool {
        if self.src[self.pos..].starts_with("[[") {
            self.wiki_link(false)
        } else {
            self.link()
        }
    }

    /// `[[target#heading|alias]]`, or `![[…]]` when embedded.
    fn wiki_link(&mut self, embed: bool) -> bool {
        let open = self.pos;
        let inner_at = open + if embed { 3 } else { 2 };
        let Some(close) = find(self.bytes, inner_at, b"]]") else {
            return false;
        };
        let inner = &self.src[inner_at..close];
        // Order matters: `[[a#b|c]]` — the alias pipe wins over the `#`, so a
        // title containing `#` after a pipe is not mistaken for a heading.
        let (locator, alias) = match inner.find('|') {
            Some(i) => (&inner[..i], Some(inner[i + 1..].trim().to_string())),
            None => (inner, None),
        };
        let (target, heading) = match locator.find('#') {
            Some(i) => (&locator[..i], Some(locator[i + 1..].trim().to_string())),
            None => (locator, None),
        };
        self.flush();
        self.out.push(Inline::WikiLink {
            target: target.trim().to_string(),
            heading: heading.filter(|h| !h.is_empty()),
            alias: alias.filter(|a| !a.is_empty()),
            embed,
            span: self.span(open, close + 2),
        });
        self.pos = close + 2;
        true
    }

    fn image(&mut self) -> bool {
        let open = self.pos;
        let Some(label_end) = find(self.bytes, open + 2, b"]") else {
            return false;
        };
        let Some((src, end)) = self.destination(label_end + 1) else {
            return false;
        };
        let alt = self.src[open + 2..label_end].to_string();
        self.flush();
        self.out.push(Inline::Image {
            src,
            alt,
            span: self.span(open, end),
        });
        self.pos = end;
        true
    }

    fn link(&mut self) -> bool {
        let open = self.pos;
        let Some(label_end) = matching_bracket(self.bytes, open) else {
            return false;
        };
        let Some((href, end)) = self.destination(label_end + 1) else {
            return false;
        };
        let label = scan_inlines(&self.src[open + 1..label_end], self.base + open + 1);
        self.flush();
        self.out.push(Inline::Link {
            href,
            label,
            span: self.span(open, end),
        });
        self.pos = end;
        true
    }

    /// `(dest)` or `(dest "title")` starting at `at`; the title is dropped
    /// because the model has nowhere to put it.
    fn destination(&self, at: usize) -> Option<(String, usize)> {
        if self.bytes.get(at) != Some(&b'(') {
            return None;
        }
        let mut depth = 1usize;
        let mut i = at + 1;
        while i < self.bytes.len() {
            match self.bytes[i] {
                b'\\' => i += 1,
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        let raw = self.src[at + 1..i].trim();
                        let dest = raw.split_once(char::is_whitespace).map_or(raw, |(d, _)| d);
                        let dest = dest.trim_start_matches('<').trim_end_matches('>');
                        return Some((dest.to_string(), i + 1));
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// `==highlight==` and `~~strike~~`: a fixed-width symmetric delimiter.
    fn paired(&mut self, delim: u8, width: usize, wrap: fn(Vec<Inline>) -> Inline) -> bool {
        if run_len(self.bytes, self.pos, delim) < width {
            return false;
        }
        let start = self.pos + width;
        if self.bytes.get(start).map_or(true, is_space) {
            return false;
        }
        let Some(close) = self.find_close(start, delim, width) else {
            return false;
        };
        let inner = scan_inlines(&self.src[start..close], self.base + start);
        self.flush();
        self.out.push(wrap(inner));
        self.pos = close + width;
        true
    }

    /// `*`/`_` emphasis. Deliberately **not** full CommonMark flanking rules:
    /// the two that matter in a notes vault are "an opener is not followed by a
    /// space" and "`_` only fires at a word boundary", the second of which is
    /// what keeps `snake_case_names` intact.
    fn emphasis(&mut self) -> bool {
        let delim = self.bytes[self.pos];
        let run = run_len(self.bytes, self.pos, delim);
        let width = run.min(3);
        let start = self.pos + width;
        if self.bytes.get(start).map_or(true, is_space) {
            return false;
        }
        if delim == b'_' && !self.at_word_edge(self.pos) {
            return false;
        }
        let Some(close) = self.find_close(start, delim, width) else {
            return false;
        };
        if delim == b'_' && !self.at_word_edge(close + width) {
            return false;
        }
        let inner = scan_inlines(&self.src[start..close], self.base + start);
        self.flush();
        self.out.push(match width {
            1 => Inline::Emph(inner),
            2 => Inline::Strong(inner),
            _ => Inline::Strong(vec![Inline::Emph(inner)]),
        });
        self.pos = close + width;
        true
    }

    /// `#tag`, `#nested/tag`.
    ///
    /// Two rules earn their keep: the `#` must open a token (otherwise
    /// `C#`, `id#3` and every URL fragment become tags), and the name may not be
    /// all digits (otherwise `#1` in "issue #1" becomes one).
    fn tag(&mut self) -> bool {
        if !self.at_token_start(self.pos) {
            return false;
        }
        let mut end = self.pos + 1;
        let mut has_non_digit = false;
        for c in self.src[self.pos + 1..].chars() {
            if c.is_alphanumeric() || c == '_' || c == '-' || c == '/' {
                has_non_digit |= !c.is_numeric();
                end += c.len_utf8();
            } else {
                break;
            }
        }
        if end == self.pos + 1 || !has_non_digit {
            return false;
        }
        self.flush();
        self.out.push(Inline::Tag {
            name: self.src[self.pos + 1..end].to_string(),
            span: self.span(self.pos, end),
        });
        self.pos = end;
        true
    }

    /// `<https://…>` / `<mailto:…>`. Anything else starting with `<` is left as
    /// text so that raw HTML written inline survives the round trip.
    fn autolink(&mut self) -> bool {
        let Some(close) = find(self.bytes, self.pos + 1, b">") else {
            return false;
        };
        let inner = &self.src[self.pos + 1..close];
        if inner.is_empty()
            || inner.chars().any(char::is_whitespace)
            || !(inner.contains("://") || inner.starts_with("mailto:"))
        {
            return false;
        }
        self.flush();
        self.out.push(Inline::Link {
            href: inner.to_string(),
            label: vec![Inline::Text(inner.to_string())],
            span: self.span(self.pos, close + 1),
        });
        self.pos = close + 1;
        true
    }

    /// A line ending in two or more spaces is a hard break; a plain newline is
    /// a soft one and stays in the text, so the source keeps its own wrapping.
    fn hard_break(&mut self) -> bool {
        let newline = if self.bytes[self.pos] == b'\r' {
            if self.bytes.get(self.pos + 1) != Some(&b'\n') {
                return false;
            }
            2
        } else {
            1
        };
        let kept = self.buf.trim_end_matches(' ').len();
        if self.buf.len() - kept < 2 {
            return false;
        }
        self.buf.truncate(kept);
        self.flush();
        self.out.push(Inline::Break);
        self.pos += newline;
        true
    }

    // ── helpers ─────────────────────────────────────────────────────────────

    /// Find a closing run of at least `width` `delim`s, **skipping code spans**
    /// so that `==a `b==c` d==` closes where a reader expects it to.
    fn find_close(&self, from: usize, delim: u8, width: usize) -> Option<usize> {
        let mut i = from;
        while i < self.bytes.len() {
            match self.bytes[i] {
                b'\\' => i += 2,
                b'`' => {
                    let ticks = run_len(self.bytes, i, b'`');
                    i = match exact_run(self.bytes, i + ticks, b'`', ticks) {
                        Some(end) => end + ticks,
                        None => i + ticks,
                    };
                }
                c if c == delim => {
                    let run = run_len(self.bytes, i, delim);
                    if run >= width && i > from && !is_space(&self.bytes[i - 1]) {
                        return Some(i);
                    }
                    i += run;
                }
                _ => i += 1,
            }
        }
        None
    }

    /// True when `at` opens a token: start of the run, or preceded by
    /// whitespace or an opening bracket.
    fn at_token_start(&self, at: usize) -> bool {
        match self.src[..at].chars().next_back() {
            None => true,
            Some(c) => c.is_whitespace() || matches!(c, '(' | '[' | '{'),
        }
    }

    /// True when `at` sits on the boundary of a word — used only by `_`.
    fn at_word_edge(&self, at: usize) -> bool {
        let before = self.src[..at.min(self.src.len())]
            .chars()
            .next_back()
            .map_or(false, |c| c.is_alphanumeric() || c == '_');
        let after = self.src[at.min(self.src.len())..]
            .chars()
            .next()
            .map_or(false, |c| c.is_alphanumeric() || c == '_');
        !(before && after)
    }
}

fn is_space(b: &u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// Length in bytes of the UTF-8 character whose first byte is `b`.
fn char_len(b: u8) -> usize {
    match b {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        // A continuation byte here means the caller is mid-character, which the
        // scanner never is; advancing by one keeps it from looping.
        _ => 1,
    }
}

fn run_len(bytes: &[u8], at: usize, b: u8) -> usize {
    bytes[at..].iter().take_while(|c| **c == b).count()
}

/// The next run of *exactly* `width` `b`s at or after `from`.
fn exact_run(bytes: &[u8], from: usize, b: u8, width: usize) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b {
            let run = run_len(bytes, i, b);
            if run == width {
                return Some(i);
            }
            i += run;
        } else {
            i += 1;
        }
    }
    None
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from > bytes.len() {
        return None;
    }
    bytes[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| from + i)
}

/// The `]` matching the `[` at `open`, honouring nesting and escapes.
fn matching_bracket(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(text: &str) -> Vec<Inline> {
        scan_inlines(text, 0)
    }

    fn text(s: &str) -> Inline {
        Inline::Text(s.to_string())
    }

    #[test]
    fn plain_prose_is_one_text_node() {
        assert_eq!(scan("solo testo, niente di che"), vec![text("solo testo, niente di che")]);
    }

    #[test]
    fn reads_a_wikilink_with_heading_and_alias() {
        let got = scan("vedi [[Nota A#Sezione|qui]] ok");
        assert_eq!(
            got,
            vec![
                text("vedi "),
                Inline::WikiLink {
                    target: "Nota A".into(),
                    heading: Some("Sezione".into()),
                    alias: Some("qui".into()),
                    embed: false,
                    span: Span { start: 5, end: 27 },
                },
                text(" ok"),
            ]
        );
    }

    #[test]
    fn an_alias_may_contain_a_hash() {
        let got = scan("[[Nota|C# e altro]]");
        assert_eq!(
            got,
            vec![Inline::WikiLink {
                target: "Nota".into(),
                heading: None,
                alias: Some("C# e altro".into()),
                embed: false,
                span: Span { start: 0, end: 19 },
            }]
        );
    }

    #[test]
    fn an_embed_keeps_the_bang_inside_its_span() {
        let got = scan("![[disegno.png]]");
        assert_eq!(
            got,
            vec![Inline::WikiLink {
                target: "disegno.png".into(),
                heading: None,
                alias: None,
                embed: true,
                span: Span { start: 0, end: 16 },
            }]
        );
    }

    #[test]
    fn an_unclosed_wikilink_stays_text() {
        assert_eq!(scan("[[mai chiuso"), vec![text("[[mai chiuso")]);
    }

    #[test]
    fn a_code_span_shields_a_wikilink_and_a_tag() {
        // The single most important behaviour in this module.
        let got = scan("usa `[[Nota]] e #tag` invece");
        assert_eq!(
            got,
            vec![
                text("usa "),
                Inline::Code("[[Nota]] e #tag".into()),
                text(" invece"),
            ]
        );
    }

    #[test]
    fn a_code_span_is_skipped_when_looking_for_a_closing_delimiter() {
        let got = scan("==a `b==c` d==");
        assert_eq!(
            got,
            vec![Inline::Highlight(vec![
                text("a "),
                Inline::Code("b==c".into()),
                text(" d"),
            ])]
        );
    }

    #[test]
    fn tags_open_a_token_and_are_never_all_digits() {
        assert_eq!(
            scan("#lavoro/urgente"),
            vec![Inline::Tag {
                name: "lavoro/urgente".into(),
                span: Span { start: 0, end: 15 },
            }]
        );
        // `C#` is not a tag opener, and `#1` is an issue number.
        assert_eq!(scan("scrivo in C# ok"), vec![text("scrivo in C# ok")]);
        assert_eq!(scan("issue #1 aperta"), vec![text("issue #1 aperta")]);
        assert_eq!(scan("#"), vec![text("#")]);
    }

    #[test]
    fn a_tag_span_is_in_bytes_after_multibyte_prose() {
        // "perché " is 8 bytes, not 7 — a char-based span would point one byte
        // short and the editor would highlight the wrong range.
        let got = scan("perché #città");
        assert_eq!(
            got,
            vec![
                text("perché "),
                Inline::Tag {
                    name: "città".into(),
                    span: Span { start: 8, end: 15 },
                }
            ]
        );
    }

    #[test]
    fn base_is_added_to_every_span() {
        let got = scan_inlines("x [[A]]", 100);
        let Inline::WikiLink { span, .. } = &got[1] else {
            panic!("expected a wikilink, got {:?}", got[1]);
        };
        assert_eq!(*span, Span { start: 102, end: 107 });
    }

    #[test]
    fn reads_emphasis_strong_strike_and_highlight() {
        assert_eq!(scan("*a*"), vec![Inline::Emph(vec![text("a")])]);
        assert_eq!(scan("**a**"), vec![Inline::Strong(vec![text("a")])]);
        assert_eq!(
            scan("***a***"),
            vec![Inline::Strong(vec![Inline::Emph(vec![text("a")])])]
        );
        assert_eq!(scan("~~a~~"), vec![Inline::Strike(vec![text("a")])]);
        assert_eq!(scan("==a=="), vec![Inline::Highlight(vec![text("a")])]);
    }

    #[test]
    fn underscores_inside_a_word_are_not_emphasis() {
        assert_eq!(scan("snake_case_name"), vec![text("snake_case_name")]);
        assert_eq!(scan("_vero_"), vec![Inline::Emph(vec![text("vero")])]);
    }

    #[test]
    fn a_star_followed_by_a_space_is_a_literal_star() {
        assert_eq!(scan("2 * 3 * 4"), vec![text("2 * 3 * 4")]);
    }

    #[test]
    fn reads_links_and_images() {
        assert_eq!(
            scan("[etichetta](./a.md)"),
            vec![Inline::Link {
                href: "./a.md".into(),
                label: vec![text("etichetta")],
                span: Span { start: 0, end: 19 },
            }]
        );
        assert_eq!(
            scan("![alt](img.png)"),
            vec![Inline::Image {
                src: "img.png".into(),
                alt: "alt".into(),
                span: Span { start: 0, end: 15 },
            }]
        );
        // A bracket with no destination is prose, not a broken link.
        assert_eq!(scan("[nota a margine]"), vec![text("[nota a margine]")]);
    }

    #[test]
    fn a_link_title_is_dropped_but_the_span_covers_it() {
        let got = scan("[a](http://x \"t\")");
        assert_eq!(
            got,
            vec![Inline::Link {
                href: "http://x".into(),
                label: vec![text("a")],
                span: Span { start: 0, end: 17 },
            }]
        );
    }

    #[test]
    fn reads_an_autolink_but_leaves_inline_html_alone() {
        assert_eq!(
            scan("<https://x.org>"),
            vec![Inline::Link {
                href: "https://x.org".into(),
                label: vec![text("https://x.org")],
                span: Span { start: 0, end: 15 },
            }]
        );
        assert_eq!(scan("<span>ciao</span>"), vec![text("<span>ciao</span>")]);
    }

    #[test]
    fn two_trailing_spaces_are_a_hard_break_and_a_lone_newline_is_not() {
        assert_eq!(
            scan("a  \nb"),
            vec![text("a"), Inline::Break, text("b")]
        );
        assert_eq!(scan("a\nb"), vec![text("a\nb")]);
        assert_eq!(scan("a\\\nb"), vec![text("a"), Inline::Break, text("b")]);
    }

    #[test]
    fn a_backslash_escapes_a_construct() {
        assert_eq!(scan("\\#nontag"), vec![text("#nontag")]);
        assert_eq!(scan("\\*non enfasi\\*"), vec![text("*non enfasi*")]);
    }

    #[test]
    fn nested_constructs_are_scanned_recursively() {
        assert_eq!(
            scan("**forte con [[Nota]]**"),
            vec![Inline::Strong(vec![
                text("forte con "),
                Inline::WikiLink {
                    target: "Nota".into(),
                    heading: None,
                    alias: None,
                    embed: false,
                    span: Span { start: 12, end: 20 },
                },
            ])]
        );
    }
}
