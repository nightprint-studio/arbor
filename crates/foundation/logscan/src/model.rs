//! What an interpreted line IS: levels, tokens, styles, links, spans, pieces.
//!
//! Two views of the same answer, and the distinction matters:
//!
//! * [`Span`] is the **library** view — byte ranges into [`Line::text`], which is what a
//!   Rust host wants when it means to slice, search or re-annotate.
//! * [`Piece`] is the **wire** view — the text itself, already cut up, produced by
//!   [`Line::pieces`]. A frontend renders a list of pieces without ever doing offset
//!   arithmetic, which is the only way this survives contact with JavaScript: Rust counts
//!   UTF-8 bytes and JS counts UTF-16 code units, so a byte range crossing a seam is a bug
//!   waiting for the first non-ASCII log line.

use serde::Serialize;

/// The severity of a line, once something on it said so.
///
/// Six levels, because that is what the logging frameworks agree on between them —
/// `java.util.logging`'s finer gradations (`FINEST`/`FINER`, `CONFIG`) fold onto the
/// nearest of these rather than being carried as separate cases nobody styles differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl Level {
    /// The lowercase name — the same string the wire carries.
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Trace => "trace",
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
            Level::Fatal => "fatal",
        }
    }

    /// Whether this level is one a reader should not be able to miss (`warn` and worse).
    pub fn is_bad(self) -> bool {
        self >= Level::Warn
    }
}

/// What a piece of a line *is* — the semantic classification a rule made.
///
/// Deliberately small and presentational-agnostic: it says what was recognised, never how
/// to paint it. A host maps these to its own theme, and a token it does not know how to
/// style renders as text, which is why adding one is not a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Token {
    /// Nothing was recognised — plain text, carrying at most a colour the program chose.
    #[default]
    Text,
    /// The level word itself (`ERROR`, `[WARN]`, `SEVERE`).
    Level,
    /// A date and/or a clock time.
    Timestamp,
    /// A bracketed thread name — `[main]`, `[http-nio-8080-exec-3]`.
    Thread,
    /// A dotted qualified name: a logger, a package, a class.
    Package,
    /// A qualified (or plainly suffixed) name that reads as a throwable.
    Exception,
    /// A stack-trace frame's source location — the `(Foo.java:42)` part.
    Frame,
    /// A URL, of any scheme.
    Url,
    /// A filesystem path, with the `:42` line suffix when it carried one.
    Path,
}

impl Token {
    /// Whether this is the "nothing was recognised" case. (Used to keep the wire small —
    /// the overwhelmingly common token is the one worth omitting.)
    pub fn is_text(&self) -> bool {
        matches!(self, Token::Text)
    }

    /// The lowercase name — the same string the wire carries.
    pub fn as_str(self) -> &'static str {
        match self {
            Token::Text => "text",
            Token::Level => "level",
            Token::Timestamp => "timestamp",
            Token::Thread => "thread",
            Token::Package => "package",
            Token::Exception => "exception",
            Token::Frame => "frame",
            Token::Url => "url",
            Token::Path => "path",
        }
    }
}

/// The eight ANSI hues. Bright and normal map to the same case on purpose: eight colours
/// is what a themed console should show, sixteen is two of everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Colour {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

/// What the *program* asked for, via ANSI SGR — as opposed to [`Token`], which is what the
/// rules recognised. Kept apart because they answer to different authorities and a host may
/// well want to honour one and ignore the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Style {
    pub colour: Option<Colour>,
    pub bold: bool,
}

impl Style {
    /// Whether the program asked for nothing at all.
    pub fn is_plain(&self) -> bool {
        self.colour.is_none() && !self.bold
    }
}

/// Where a piece points — what "clicking it" should mean.
///
/// [`Link::Source`] is deliberately **unresolved**: a stack frame names a class, and turning
/// a class into a file is something only the host's index can do. Resolving it here would
/// mean this crate knowing about Java projects, which is exactly what it must not know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Link {
    /// A source location named by a stack frame. `class` is as written in the frame —
    /// binary-ish (`com.acme.Foo$Inner`), module prefix already stripped; see
    /// [`outer_class`](crate::java::outer_class) for the source-file-bearing outer name.
    Source {
        class: String,
        /// The method the frame named. A host opening a view with no usable line numbers —
        /// a stub decompiled from bytecode — lands on this instead.
        #[serde(skip_serializing_if = "Option::is_none")]
        method: Option<String>,
        /// The file name the compiler recorded (`Foo.java`), when the frame had one.
        #[serde(skip_serializing_if = "Option::is_none")]
        file: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
    },
    /// A path the host can open as-is.
    File {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
    },
    /// An external URL.
    Url { url: String },
}

/// One annotated range of a line — the library view. Byte offsets into [`Line::text`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub token: Token,
    pub style: Style,
    pub link: Option<Link>,
}

/// One interpreted line: the text with the escapes gone, what level it turned out to be,
/// and the annotated ranges over it.
///
/// The spans are sorted, non-overlapping and **sparse** — only what was recognised or
/// coloured is in there. Everything between them is ordinary text, which [`Line::pieces`]
/// fills back in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The line with every ANSI escape removed.
    pub text: String,
    /// The level, when the line said so or inherited one (see
    /// [`LogReader`](crate::reader::LogReader)).
    pub level: Option<Level>,
    pub spans: Vec<Span>,
}

/// One piece of a line, ready to render — the wire view. Produced by [`Line::pieces`].
///
/// Everything that is absent is omitted from the JSON, so an unremarkable line costs
/// `{"text":"…"}` per piece and an unremarkable *log* costs one piece.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Piece<'a> {
    pub text: &'a str,
    #[serde(skip_serializing_if = "Token::is_text")]
    pub token: Token,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colour: Option<Colour>,
    #[serde(skip_serializing_if = "is_not_bold")]
    pub bold: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<&'a Link>,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's `skip_serializing_if` hands us a &bool
fn is_not_bold(b: &bool) -> bool {
    !*b
}

impl<'a> Piece<'a> {
    /// A piece nothing was recognised in and nothing was asked for.
    pub fn plain(text: &'a str) -> Self {
        Piece { text, token: Token::Text, colour: None, bold: false, link: None }
    }
}

impl Line {
    /// A line with no annotations at all — what a host gets for text nothing matched.
    pub fn plain(text: impl Into<String>) -> Self {
        Line { text: text.into(), level: None, spans: Vec::new() }
    }

    /// The whole line cut into consecutive pieces, gaps included, in order.
    ///
    /// Concatenating their `text` reproduces [`Line::text`] exactly — a property the tests
    /// assert, because a renderer that drops a character is a renderer that silently lies
    /// about what a program printed.
    pub fn pieces(&self) -> Vec<Piece<'_>> {
        let mut out = Vec::with_capacity(self.spans.len() * 2 + 1);
        let mut at = 0usize;
        for s in &self.spans {
            if s.start > at {
                out.push(Piece::plain(&self.text[at..s.start]));
            }
            out.push(Piece {
                text: &self.text[s.start..s.end],
                token: s.token,
                colour: s.style.colour,
                bold: s.style.bold,
                link: s.link.as_ref(),
            });
            at = s.end;
        }
        if at < self.text.len() {
            out.push(Piece::plain(&self.text[at..]));
        }
        out
    }

    /// Every link on the line, in order — for a host that wants the targets without
    /// rendering anything (a "jump to the first frame" action, say).
    pub fn links(&self) -> impl Iterator<Item = &Link> {
        self.spans.iter().filter_map(|s| s.link.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: usize, end: usize, token: Token) -> Span {
        Span { start, end, token, style: Style::default(), link: None }
    }

    #[test]
    fn pieces_cover_the_line_exactly() {
        let line = Line {
            text: "hello com.acme.Foo bye".to_string(),
            level: None,
            spans: vec![span(6, 18, Token::Package)],
        };
        let pieces = line.pieces();
        assert_eq!(pieces.len(), 3);
        assert_eq!(pieces[1].text, "com.acme.Foo");
        assert_eq!(pieces[1].token, Token::Package);
        let rebuilt: String = pieces.iter().map(|p| p.text).collect();
        assert_eq!(rebuilt, line.text);
    }

    #[test]
    fn a_line_that_is_all_span_has_no_gaps() {
        let line =
            Line { text: "ERROR".to_string(), level: None, spans: vec![span(0, 5, Token::Level)] };
        assert_eq!(line.pieces().len(), 1);
    }

    #[test]
    fn an_unremarkable_piece_serialises_to_just_its_text() {
        let line = Line::plain("nothing to see");
        let json = serde_json::to_string(&line.pieces()).unwrap();
        assert_eq!(json, r#"[{"text":"nothing to see"}]"#);
    }

    #[test]
    fn a_link_carries_its_kind() {
        let link = Link::File { path: "/tmp/x.txt".into(), line: Some(4) };
        let json = serde_json::to_string(&link).unwrap();
        assert_eq!(json, r#"{"kind":"file","path":"/tmp/x.txt","line":4}"#);
    }
}
