//! The fulcrum i18n markup, parsed **with positions and without failing**.
//!
//! A translated string is not plain text. It carries four constructs:
//!
//! | Written | Means |
//! |---|---|
//! | `{amount}` | a placeholder, interpolated at render time |
//! | `$red.bold{…}` | a style span, chainable, optionally `$mod:red{…}` |
//! | `@potion{…}` | a glossary reference, optionally `@rpg:hp{…}` |
//! | `~sleep(0.8)`, `~slow{…}` | a control — pacing or effect, args and body both optional |
//!
//! `\` escapes any of `$ @ ~ { } \`. A stray `}` at the top level is literal text, not a
//! terminator — treating it as one silently truncated the rest of the string.
//!
//! ## Two deliberate differences from the engine's own parser
//!
//! The engine (`fulcrum-i18n`) parses the same grammar, and this is **not** a copy of it. Two
//! things have to be different, and both come from the same fact: the engine parses a file that is
//! finished, and an editor parses one that is being typed.
//!
//! 1. **It never fails.** The engine returns `Err(UnclosedBrace)` and the caller logs it. Here an
//!    unclosed brace is a *problem with a span* alongside everything that did parse, because half
//!    a string mid-keystroke still has to highlight, still has to preview, and a parser that
//!    answers nothing until the string is valid answers nothing for most of the time you are
//!    looking at it.
//! 2. **Every part carries its byte range.** The engine needs the tree; an editor needs to know
//!    *where* — to underline the one unknown style name in `$red.bolde{…}` rather than the whole
//!    span, to complete a glossary key at the caret, to colour a placeholder differently from the
//!    text around it. Offsets are **bytes**, so they can be handed straight to the editor, and
//!    they are relative to the string that was parsed.
//!
//! Everything else — what the constructs are, how they nest, what escapes — is the engine's, and
//! has to stay the engine's. The tests below pin the shapes its own test suite pins.

use serde::Serialize;

/// One piece of a parsed string, with where it was written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub kind: SegmentKind,
    /// Byte offset of the segment's first character, relative to the parsed string.
    pub start: usize,
    /// Byte offset just past its last character.
    pub end: usize,
}

/// A name written inside a construct, with its own span — which is what makes "this style does not
/// exist" reportable on the style rather than on the whole span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Name {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SegmentKind {
    /// Literal text, with escapes already resolved.
    Text { text: String },
    /// `{name}` — interpolated by the caller at render time.
    Placeholder { name: Name },
    /// `$[ns:]a.b.c{…}` — styles applied left to right, each overriding only the fields it sets.
    Style {
        namespace: Option<Name>,
        /// At least one. A chain is `$red.bold{…}`.
        styles: Vec<Name>,
        content: Vec<Segment>,
    },
    /// `@[ns:]key[.sub]{…}` — a glossary entry; the content is what is shown.
    Glossary {
        namespace: Option<Name>,
        key: Name,
        content: Vec<Segment>,
    },
    /// `~name(args…){…}` — pacing or effect. i18n knows the *form* only: what `slow` or `sleep`
    /// mean is the consumer's, exactly as with styles.
    Control {
        name: Name,
        /// Positional, raw, trimmed. Never interpreted here — `0.8` and `amp=2` both pass through.
        args: Vec<String>,
        content: Vec<Segment>,
    },
}

/// Something wrong with the markup, on the span that caused it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MarkupProblem {
    pub message: String,
    pub start: usize,
    pub end: usize,
}

/// A parsed string: what it says, and what is wrong with it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Parsed {
    pub segments: Vec<Segment>,
    pub problems: Vec<MarkupProblem>,
}

/// Parse `text`. Never fails — see the module doc.
pub fn parse_markup(text: &str) -> Parsed {
    let mut p = Parser { src: text.as_bytes(), at: 0, problems: Vec::new() };
    let segments = p.segments(false);
    Parsed { segments, problems: p.problems }
}

/// The plain text of a parsed string: the text, plus every construct's content, in order.
///
/// A placeholder contributes its **name**, which is what the engine's own flatten does — the point
/// being that a hover reading `Dealt {amount} damage` shows the shape of the sentence rather than a
/// hole in it.
pub fn flatten(segments: &[Segment]) -> String {
    let mut out = String::new();
    push_flat(segments, &mut out);
    out
}

fn push_flat(segments: &[Segment], out: &mut String) {
    for s in segments {
        match &s.kind {
            SegmentKind::Text { text } => out.push_str(text),
            SegmentKind::Placeholder { name } => out.push_str(&name.text),
            SegmentKind::Style { content, .. }
            | SegmentKind::Glossary { content, .. }
            | SegmentKind::Control { content, .. } => push_flat(content, out),
        }
    }
}

/// Every placeholder name in the string, in order, deduplicated.
///
/// What the preview's parameter table is built from, and what tells you that a translation of
/// `status:running_named` in one language uses `{name}` and in another has forgotten it.
pub fn placeholders(segments: &[Segment]) -> Vec<String> {
    let mut out = Vec::new();
    collect_placeholders(segments, &mut out);
    out
}

fn collect_placeholders(segments: &[Segment], out: &mut Vec<String>) {
    for s in segments {
        match &s.kind {
            SegmentKind::Placeholder { name } => {
                if !out.contains(&name.text) {
                    out.push(name.text.clone());
                }
            }
            SegmentKind::Style { content, .. }
            | SegmentKind::Glossary { content, .. }
            | SegmentKind::Control { content, .. } => collect_placeholders(content, out),
            SegmentKind::Text { .. } => {}
        }
    }
}

/// Every style name referenced anywhere in the string, with its span — what the "no such style"
/// check walks.
pub fn style_refs(segments: &[Segment]) -> Vec<Name> {
    let mut refs = Refs::default();
    collect(segments, &mut refs);
    refs.styles
}

/// Every glossary key referenced anywhere, with its span.
pub fn glossary_refs(segments: &[Segment]) -> Vec<Name> {
    let mut refs = Refs::default();
    collect(segments, &mut refs);
    refs.glossary
}

/// Every control name used anywhere, with its span.
///
/// Unlike the two above there is nothing to check these against — see
/// [`LabelCatalog::controls`](crate::catalog::LabelCatalog::controls) for what they are for.
pub fn control_refs(segments: &[Segment]) -> Vec<Name> {
    let mut refs = Refs::default();
    collect(segments, &mut refs);
    refs.controls
}

/// The named things a tree references. One walker fills all three, so a construct that starts
/// nesting differently cannot be handled one way here and another way there.
#[derive(Default)]
struct Refs {
    styles: Vec<Name>,
    glossary: Vec<Name>,
    controls: Vec<Name>,
}

fn collect(segments: &[Segment], out: &mut Refs) {
    for s in segments {
        match &s.kind {
            SegmentKind::Style { styles, content, .. } => {
                out.styles.extend(styles.iter().cloned());
                collect(content, out);
            }
            SegmentKind::Glossary { key, content, .. } => {
                out.glossary.push(key.clone());
                collect(content, out);
            }
            SegmentKind::Control { name, content, .. } => {
                out.controls.push(name.clone());
                collect(content, out);
            }
            _ => {}
        }
    }
}

// ── the scanner ───────────────────────────────────────────────────────────────

struct Parser<'a> {
    src: &'a [u8],
    at: usize,
    problems: Vec<MarkupProblem>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.src.get(self.at).copied()
    }

    fn problem(&mut self, message: &str, start: usize, end: usize) {
        self.problems.push(MarkupProblem {
            message: message.to_string(),
            start,
            end: end.max(start + 1).min(self.src.len().max(start + 1)),
        });
    }

    /// `nested` distinguishes the level: inside a construct a `}` closes it, at the top level a
    /// stray `}` is literal — see the module doc for why that matters.
    fn segments(&mut self, nested: bool) -> Vec<Segment> {
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            let start = self.at;
            match c {
                b'$' => {
                    self.at += 1;
                    out.push(self.style_block(start));
                }
                b'@' => {
                    self.at += 1;
                    out.push(self.glossary_block(start));
                }
                b'~' => {
                    self.at += 1;
                    out.push(self.control_block(start));
                }
                b'{' => {
                    self.at += 1;
                    let name_start = self.at;
                    while self.peek().is_some_and(|b| b != b'}') {
                        self.at += 1;
                    }
                    let name_end = self.at;
                    if self.peek() == Some(b'}') {
                        self.at += 1;
                    } else {
                        self.problem("unclosed `{` — a placeholder needs its `}`", start, name_end);
                    }
                    out.push(Segment {
                        kind: SegmentKind::Placeholder {
                            name: self.name(name_start, name_end),
                        },
                        start,
                        end: self.at,
                    });
                }
                b'}' if nested => break,
                b'}' => {
                    self.at += 1;
                    out.push(Segment {
                        kind: SegmentKind::Text { text: "}".to_string() },
                        start,
                        end: self.at,
                    });
                }
                _ => {
                    let text = self.text();
                    if !text.is_empty() {
                        out.push(Segment {
                            kind: SegmentKind::Text { text },
                            start,
                            end: self.at,
                        });
                    } else {
                        // Nothing consumed: bail rather than spin. Unreachable while `text()`
                        // handles every byte that is not a trigger, and cheap insurance.
                        break;
                    }
                }
            }
        }
        out
    }

    /// Literal text up to the next unescaped trigger. `\` escapes the byte after it; a trailing
    /// `\` is kept literal, as the engine keeps it.
    fn text(&mut self) -> String {
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            match c {
                b'\\' => {
                    self.at += 1;
                    match self.peek() {
                        Some(escaped) => {
                            out.push(escaped);
                            self.at += 1;
                        }
                        None => out.push(b'\\'),
                    }
                }
                b'$' | b'@' | b'~' | b'{' | b'}' => break,
                _ => {
                    out.push(c);
                    self.at += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    /// An identifier: alphanumeric plus `_`. Multibyte letters count — a style named `città` is
    /// legal in the engine, whose identifier test is `is_alphanumeric` on a `char`.
    fn ident(&mut self) -> (usize, usize) {
        let start = self.at;
        while self.at < self.src.len() {
            let rest = &self.src[self.at..];
            let Some(ch) = std::str::from_utf8(rest).ok().and_then(|s| s.chars().next()) else {
                break;
            };
            if ch.is_alphanumeric() || ch == '_' {
                self.at += ch.len_utf8();
            } else {
                break;
            }
        }
        (start, self.at)
    }

    fn name(&self, start: usize, end: usize) -> Name {
        Name {
            text: String::from_utf8_lossy(&self.src[start..end]).into_owned(),
            start,
            end,
        }
    }

    /// `$[ns:]style[.style…]{content}` — the `$` is already consumed.
    fn style_block(&mut self, start: usize) -> Segment {
        let (mut s, mut e) = self.ident();
        let mut namespace = None;
        if self.peek() == Some(b':') {
            self.at += 1;
            namespace = Some(self.name(s, e));
            let (ns, ne) = self.ident();
            s = ns;
            e = ne;
        }
        let mut styles = Vec::new();
        if s == e {
            self.problem("a `$` needs a style name", start, self.at);
        } else {
            styles.push(self.name(s, e));
        }
        while self.peek() == Some(b'.') {
            let dot = self.at;
            self.at += 1;
            let (cs, ce) = self.ident();
            if cs == ce {
                self.problem("a `.` in a style chain needs a name after it", dot, self.at);
                break;
            }
            styles.push(self.name(cs, ce));
        }
        let content = self.body(start, "style");
        Segment { kind: SegmentKind::Style { namespace, styles, content }, start, end: self.at }
    }

    /// `@[ns:]key[.sub]{content}` — the `@` is already consumed.
    fn glossary_block(&mut self, start: usize) -> Segment {
        let (mut s, mut e) = self.ident();
        let mut namespace = None;
        if self.peek() == Some(b':') {
            self.at += 1;
            namespace = Some(self.name(s, e));
            let (ns, ne) = self.ident();
            s = ns;
            e = ne;
        }
        // A dotted key (`@status.protect`) is one key, not a namespace.
        if self.peek() == Some(b'.') {
            self.at += 1;
            let (_, sub_end) = self.ident();
            e = sub_end;
        }
        if s == e {
            self.problem("an `@` needs a glossary key", start, self.at);
        }
        let key = self.name(s, e);
        let content = self.body(start, "glossary reference");
        Segment { kind: SegmentKind::Glossary { namespace, key, content }, start, end: self.at }
    }

    /// `~name[(args)][{content}]` — the `~` is already consumed. Both parts are optional and
    /// independent: `~beat` is a punctual event, `~cps(20){…}` a parametric span.
    fn control_block(&mut self, start: usize) -> Segment {
        let (s, e) = self.ident();
        if s == e {
            self.problem("a `~` needs a control name", start, self.at);
        }
        let name = self.name(s, e);
        let mut args = Vec::new();
        if self.peek() == Some(b'(') {
            let open = self.at;
            self.at += 1;
            let arg_start = self.at;
            while self.peek().is_some_and(|b| b != b')') {
                self.at += 1;
            }
            let raw = String::from_utf8_lossy(&self.src[arg_start..self.at]).into_owned();
            if self.peek() == Some(b')') {
                self.at += 1;
            } else {
                self.problem("unclosed `(` in a control's arguments", open, self.at);
            }
            args = raw
                .split(',')
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect();
        }
        let content = match self.peek() {
            Some(b'{') => self.body(start, "control"),
            // No braces: a punctual control. NOT a problem — `~beat` is legal.
            _ => Vec::new(),
        };
        Segment { kind: SegmentKind::Control { name, args, content }, start, end: self.at }
    }

    /// The `{ … }` body of a construct. A missing `{` and a missing `}` are both reported, and both
    /// yield whatever content was there — which is the point of not failing.
    fn body(&mut self, start: usize, what: &str) -> Vec<Segment> {
        if self.peek() != Some(b'{') {
            self.problem(&format!("a {what} needs a `{{…}}` body"), start, self.at);
            return Vec::new();
        }
        self.at += 1;
        let content = self.segments(true);
        if self.peek() == Some(b'}') {
            self.at += 1;
        } else {
            self.problem(&format!("unclosed `{{` on this {what}"), start, self.at);
        }
        content
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(s: &Segment) -> &str {
        match &s.kind {
            SegmentKind::Text { text } => text,
            _ => panic!("not text: {s:?}"),
        }
    }

    #[test]
    fn plain_text_is_one_segment() {
        let p = parse_markup("Ciao mondo");
        assert_eq!(p.segments.len(), 1);
        assert_eq!(text_of(&p.segments[0]), "Ciao mondo");
        assert!(p.problems.is_empty());
    }

    #[test]
    fn a_placeholder_carries_the_span_of_its_name() {
        let src = "Lv {level}";
        let p = parse_markup(src);
        let SegmentKind::Placeholder { name } = &p.segments[1].kind else { panic!() };
        assert_eq!(name.text, "level");
        assert_eq!(&src[name.start..name.end], "level");
    }

    #[test]
    fn a_style_chain_keeps_every_name_with_its_own_span() {
        let src = "$red.bold{Attenzione}";
        let p = parse_markup(src);
        let SegmentKind::Style { styles, content, namespace } = &p.segments[0].kind else { panic!() };
        assert!(namespace.is_none());
        assert_eq!(styles.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["red", "bold"]);
        // The span is what makes "no such style" reportable on `bold` and not on the whole span.
        assert_eq!(&src[styles[1].start..styles[1].end], "bold");
        assert_eq!(text_of(&content[0]), "Attenzione");
    }

    #[test]
    fn a_namespaced_style_separates_the_module_from_the_name() {
        let p = parse_markup("$mymod:danger{Run}");
        let SegmentKind::Style { namespace, styles, .. } = &p.segments[0].kind else { panic!() };
        assert_eq!(namespace.as_ref().map(|n| n.text.as_str()), Some("mymod"));
        assert_eq!(styles[0].text, "danger");
    }

    #[test]
    fn a_glossary_key_may_be_dotted() {
        let src = "@status.protect{Protetto}";
        let p = parse_markup(src);
        let SegmentKind::Glossary { key, .. } = &p.segments[0].kind else { panic!() };
        assert_eq!(key.text, "status.protect");
        assert_eq!(&src[key.start..key.end], "status.protect");
    }

    #[test]
    fn constructs_nest() {
        let p = parse_markup("$red{@knight{qualcosa}}");
        let SegmentKind::Style { content, .. } = &p.segments[0].kind else { panic!() };
        let SegmentKind::Glossary { key, content: inner, .. } = &content[0].kind else { panic!() };
        assert_eq!(key.text, "knight");
        assert_eq!(text_of(&inner[0]), "qualcosa");
    }

    #[test]
    fn a_control_needs_neither_arguments_nor_a_body() {
        let p = parse_markup("Aspetta ~beat e poi");
        let SegmentKind::Control { name, args, content } = &p.segments[1].kind else { panic!() };
        assert_eq!(name.text, "beat");
        assert!(args.is_empty());
        assert!(content.is_empty());
        assert!(p.problems.is_empty(), "a punctual control is not a problem");
    }

    #[test]
    fn a_control_takes_positional_arguments_verbatim() {
        let p = parse_markup("~emote(angry, ramza)");
        let SegmentKind::Control { args, .. } = &p.segments[0].kind else { panic!() };
        assert_eq!(args, &["angry", "ramza"]);
    }

    #[test]
    fn a_decimal_argument_survives() {
        let p = parse_markup("~sleep(0.8)");
        let SegmentKind::Control { args, .. } = &p.segments[0].kind else { panic!() };
        assert_eq!(args, &["0.8"]);
    }

    #[test]
    fn escapes_become_literal_text() {
        let p = parse_markup(r"Prezzo: 100\$ poi \@ poi \{ \}");
        assert_eq!(text_of(&p.segments[0]), "Prezzo: 100$ poi @ poi { }");
        assert!(p.problems.is_empty());
    }

    #[test]
    fn an_escaped_sigil_does_not_stop_a_real_one() {
        let p = parse_markup(r"\$bold poi $bold{X}");
        assert_eq!(text_of(&p.segments[0]), "$bold poi ");
        let SegmentKind::Style { styles, .. } = &p.segments[1].kind else { panic!() };
        assert_eq!(styles[0].text, "bold");
    }

    /// A stray `}` at the top level truncated everything after it in an earlier engine version.
    #[test]
    fn a_stray_closing_brace_is_literal_and_loses_nothing() {
        let p = parse_markup("a}b");
        assert_eq!(
            p.segments.iter().map(text_of).collect::<Vec<_>>(),
            ["a", "}", "b"],
        );
    }

    // ── the tolerance, which is this parser's reason for existing ──────────────

    #[test]
    fn an_unclosed_style_reports_and_still_yields_its_content() {
        let p = parse_markup("$oops{aperto");
        assert_eq!(p.problems.len(), 1);
        let SegmentKind::Style { content, .. } = &p.segments[0].kind else { panic!() };
        assert_eq!(text_of(&content[0]), "aperto", "what was typed still parses");
    }

    #[test]
    fn a_style_with_no_name_is_a_problem_on_the_sigil() {
        let p = parse_markup("${test}");
        assert!(!p.problems.is_empty());
        assert_eq!(p.problems[0].start, 0);
    }

    #[test]
    fn a_dangling_chain_dot_is_reported_once() {
        let p = parse_markup("$red.{X}");
        assert_eq!(p.problems.len(), 1);
        let SegmentKind::Style { styles, .. } = &p.segments[0].kind else { panic!() };
        assert_eq!(styles.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["red"]);
    }

    #[test]
    fn an_unclosed_placeholder_is_reported() {
        let p = parse_markup("Lv {level");
        assert_eq!(p.problems.len(), 1);
        let SegmentKind::Placeholder { name } = &p.segments[1].kind else { panic!() };
        assert_eq!(name.text, "level");
    }

    #[test]
    fn a_control_with_unclosed_parens_is_reported() {
        let p = parse_markup("~sleep(0.8");
        assert_eq!(p.problems.len(), 1);
    }

    #[test]
    fn a_style_without_a_body_is_reported_and_costs_nothing_else() {
        let p = parse_markup("$bold e poi");
        assert_eq!(p.problems.len(), 1);
        assert!(p.problems[0].message.contains("body"));
    }

    // ── the derived views ─────────────────────────────────────────────────────

    #[test]
    fn flatten_reads_as_the_sentence() {
        let p = parse_markup("Dealt $red.bold{{amount}} to @rpg:hp{HP} ~beat now");
        assert_eq!(flatten(&p.segments), "Dealt amount to HP  now");
    }

    #[test]
    fn placeholders_are_listed_once_in_order() {
        let p = parse_markup("{a} then $bold{{b}} then {a}");
        assert_eq!(placeholders(&p.segments), ["a", "b"]);
    }

    #[test]
    fn style_and_glossary_references_are_collected_from_every_depth() {
        let p = parse_markup("$outer{@key{$inner{x}}}");
        assert_eq!(
            style_refs(&p.segments).iter().map(|n| n.text.clone()).collect::<Vec<_>>(),
            ["outer", "inner"],
        );
        assert_eq!(
            glossary_refs(&p.segments).iter().map(|n| n.text.clone()).collect::<Vec<_>>(),
            ["key"],
        );
    }

    /// Offsets are bytes and must stay usable on a string that is not ASCII — the whole point of
    /// carrying them is handing them to an editor.
    #[test]
    fn spans_are_byte_offsets_and_survive_multibyte_text() {
        let src = "Città $bold{perduta}";
        let p = parse_markup(src);
        let SegmentKind::Style { styles, .. } = &p.segments[1].kind else { panic!() };
        assert_eq!(&src[styles[0].start..styles[0].end], "bold");
    }
}
