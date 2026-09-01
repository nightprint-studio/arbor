//! A tolerant scan of an XML buffer.
//!
//! ## Why not a document parser
//!
//! Because the buffer is being typed into, so it is malformed most of the time. `<dependen` is
//! not well-formed XML and never will be until the user finishes the word — which is exactly the
//! moment they wanted a completion list. A parser that returns nothing there is absent precisely
//! when it was needed, so this scans instead: it finds every tag it can, keeps going past the
//! ones it cannot, and never fails.
//!
//! It also keeps **byte spans on everything**, which a tree does not need but an editor does:
//! the span of the name being typed is what a completion replaces, and the span of an attribute
//! value is what a check underlines.
//!
//! ## What it does not do
//!
//! Build a tree. Nesting is recovered on demand by replaying the tag sequence
//! ([`Scan::path_at`]), which is both cheaper and more honest on a malformed buffer: an
//! unclosed element leaves the stack deeper than it should be, and that is a better answer than
//! refusing to say where the caret is.
//!
//! Entity references are left as written and namespace prefixes are kept on the name. Both are
//! the consumer's business — [`crate::grammar`] compares local names, and which prefix a
//! document binds to which namespace is a question about that document.

/// Everything found in one buffer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scan {
    /// Every tag, in document order.
    pub tags: Vec<Tag>,
    /// The `<!DOCTYPE …>`, when the document declares one.
    pub doctype: Option<Doctype>,
    /// Spans that are not markup — comments, CDATA, processing instructions. A caret inside one
    /// is a caret in prose, and answering there would be answering about text.
    pub inert: Vec<(usize, usize)>,
}

impl Scan {
    /// The document's root element name, as written (prefix included).
    pub fn root(&self) -> Option<&str> {
        self.tags.iter().find(|t| t.kind != TagKind::Close).map(|t| t.name.as_str())
    }

    /// The tag whose text contains `offset`, if the caret is inside one.
    ///
    /// Both boundaries are exclusive of the caret being *outside*: `|<a>` is content and `<a>|`
    /// is content, and getting either wrong is the difference between offering the children of
    /// `a` and offering its attributes. The one exception is an unterminated tag, whose `end` is
    /// wherever the scan happened to stop rather than a real edge — the caret there is still in
    /// the tag, which is the whole point of scanning one.
    pub fn tag_at(&self, offset: usize) -> Option<&Tag> {
        self.tags
            .iter()
            .find(|t| offset > t.start && (offset < t.end || (!t.closed && offset == t.end)))
    }

    /// Whether `offset` falls in a comment, CDATA section or processing instruction.
    pub fn inert_at(&self, offset: usize) -> bool {
        self.inert.iter().any(|(s, e)| offset > *s && offset < *e)
    }

    /// The chain of elements open at `offset`, outermost first.
    ///
    /// Replayed from the tag sequence rather than read off a tree, which is what lets it answer
    /// on a buffer that does not parse. A close tag that matches nothing is ignored rather than
    /// unwinding the stack: while you are typing `</`, the name is not there yet, and popping on
    /// a guess would report the caret one level too shallow.
    pub fn path_at(&self, offset: usize) -> Vec<&str> {
        let mut stack: Vec<&str> = Vec::new();
        for t in &self.tags {
            if t.end > offset {
                break;
            }
            match t.kind {
                TagKind::Open => stack.push(&t.name),
                TagKind::Close => {
                    if let Some(i) = stack.iter().rposition(|n| *n == t.name) {
                        stack.truncate(i);
                    }
                }
                TagKind::SelfClose => {}
            }
        }
        stack
    }

    /// The element the caret is *inside the content of* — the innermost open one.
    pub fn parent_at(&self, offset: usize) -> Option<&str> {
        self.path_at(offset).last().copied()
    }

    /// Every opening tag paired with the names of the tags **directly** inside it, as written.
    ///
    /// One traversal rather than a [`Scan::path_at`] per tag: `path_at` replays the whole tag list
    /// from the start, so asking it once per tag is quadratic on a document where the answer is a
    /// single walk. Names keep their prefix — whether a child is prefixed is the one thing a check
    /// about namespaces needs, and [`local_name`] is one call away for the checks that do not.
    ///
    /// An element whose close tag never arrives is still reported, keeping whatever it had
    /// collected. That is the right way round: a file being typed is missing its close tags most
    /// of the time, and an answer that waited for a well-formed document would be absent exactly
    /// while the mistake is being written.
    pub fn direct_children(&self) -> Vec<(&Tag, Vec<&str>)> {
        let mut open: Vec<(&Tag, Vec<&str>)> = Vec::new();
        let mut done: Vec<(&Tag, Vec<&str>)> = Vec::new();
        for t in &self.tags {
            match t.kind {
                TagKind::Open => {
                    if let Some((_, kids)) = open.last_mut() {
                        kids.push(&t.name);
                    }
                    open.push((t, Vec::new()));
                }
                TagKind::SelfClose => {
                    if let Some((_, kids)) = open.last_mut() {
                        kids.push(&t.name);
                    }
                    // Reported on too: `<dependency/>` is missing everything, and skipping it
                    // would make the emptiest possible form the only one nobody checks.
                    done.push((t, Vec::new()));
                }
                TagKind::Close => {
                    // Unwound the way `path_at` does — to the matching name, ignoring a close tag
                    // that matches nothing, so a half-typed `</` does not pop a level.
                    if let Some(i) = open.iter().rposition(|(t2, _)| t2.name == t.name) {
                        done.extend(open.drain(i..));
                    }
                }
            }
        }
        done.extend(open);
        done
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    /// `<name …>`
    Open,
    /// `</name>`
    Close,
    /// `<name …/>`
    SelfClose,
}

/// One tag, with the spans an editor needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// As written, prefix included (`xs:element`).
    pub name: String,
    pub kind: TagKind,
    /// Byte offset of the `<`.
    pub start: usize,
    /// Byte offset just past the `>`, or the end of the buffer for a tag still being typed.
    pub end: usize,
    /// Span of the name itself — what a completion replaces.
    pub name_start: usize,
    pub name_end: usize,
    pub attrs: Vec<Attr>,
    /// Whether the `>` was actually found. An unterminated tag is the normal state mid-edit and
    /// must still answer; it is only worth knowing so a check does not report what follows it.
    pub closed: bool,
}

impl Tag {
    /// The name without its namespace prefix.
    pub fn local(&self) -> &str {
        local_name(&self.name)
    }

    pub fn attr(&self, name: &str) -> Option<&Attr> {
        self.attrs.iter().find(|a| a.name == name)
    }

    /// The attribute whose value span contains `offset`.
    pub fn attr_value_at(&self, offset: usize) -> Option<&Attr> {
        self.attrs.iter().find(|a| a.quoted && offset >= a.value_start && offset <= a.value_end)
    }

    /// The attribute whose name span contains `offset`.
    pub fn attr_name_at(&self, offset: usize) -> Option<&Attr> {
        self.attrs.iter().find(|a| offset >= a.name_start && offset <= a.name_end)
    }
}

/// One attribute of a tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    pub name: String,
    pub name_start: usize,
    pub name_end: usize,
    /// The value with its quotes stripped, empty when there is none yet.
    pub value: String,
    /// Span of the value's **contents** — quotes excluded, so a highlight lands on the value.
    pub value_start: usize,
    pub value_end: usize,
    /// Whether a quoted value was actually written. `name=` alone is a caret position, not a
    /// value, and the two need telling apart.
    pub quoted: bool,
}

impl Attr {
    pub fn local(&self) -> &str {
        local_name(&self.name)
    }
}

/// `xs:element` → `element`.
pub fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// The `<!DOCTYPE root PUBLIC "…" "…">` of a document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Doctype {
    pub root: String,
    /// The formal public identifier, empty when the declaration is `SYSTEM`.
    pub public_id: String,
    /// The system identifier — a URL or a path, and the thing worth matching a file against.
    pub system_id: String,
    /// Byte offset of the `<!DOCTYPE`.
    pub offset: usize,
    /// Byte offset just past its `>`. Kept because a caret lands *somewhere* on the declaration
    /// and "go to the schema this file uses" should work from anywhere on it, not only from the
    /// exact URL.
    pub end: usize,
}

/// Scan a buffer. Never fails.
pub fn scan(source: &str) -> Scan {
    let bytes = source.as_bytes();
    let mut out = Scan::default();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &source[i..];
        // The three inert forms. Each is skipped whole, because each may legally contain
        // anything at all — including text that looks exactly like a tag.
        if let Some((open, close)) = inert_delimiters(rest) {
            let end = rest[open.len()..]
                .find(close)
                .map(|n| i + open.len() + n + close.len())
                .unwrap_or(bytes.len());
            out.inert.push((i, end));
            i = end;
            continue;
        }
        if rest.starts_with("<!DOCTYPE") {
            let end = doctype_end(source, i);
            out.doctype = Some(doctype(&source[i..end], i, end));
            i = end;
            continue;
        }
        if rest.starts_with("<!") {
            // An internal-subset declaration or something we do not model. Skip to its `>`.
            i = rest.find('>').map(|n| i + n + 1).unwrap_or(bytes.len());
            continue;
        }
        match tag_at(source, i) {
            Some(tag) => {
                i = tag.end.max(i + 1);
                out.tags.push(tag);
            }
            // A bare `<` in text. Not markup, not an error worth reporting — XML says it should
            // be escaped, but saying so on every keystroke of a half-typed tag is noise.
            None => i += 1,
        }
    }
    out
}

/// The opening and closing delimiters of the inert construct starting here, if any.
fn inert_delimiters(rest: &str) -> Option<(&'static str, &'static str)> {
    for (open, close) in [("<!--", "-->"), ("<![CDATA[", "]]>"), ("<?", "?>")] {
        if rest.starts_with(open) {
            return Some((open, close));
        }
    }
    None
}

/// Read one tag starting at `from`. `None` when what follows the `<` cannot begin a name.
fn tag_at(source: &str, from: usize) -> Option<Tag> {
    let after = &source[from + 1..];
    let (kind_offset, closing) = match after.starts_with('/') {
        true => (from + 2, true),
        false => (from + 1, false),
    };
    let name_text = &source[kind_offset..];
    let name_len = name_text.find(|c: char| !is_name_char(c)).unwrap_or(name_text.len());
    if name_len == 0 {
        return None;
    }
    let name = name_text[..name_len].to_string();
    let name_start = kind_offset;
    let name_end = kind_offset + name_len;

    let (attrs, end, closed, self_closing) = tag_body(source, name_end);
    Some(Tag {
        name,
        kind: match (closing, self_closing) {
            (true, _) => TagKind::Close,
            (_, true) => TagKind::SelfClose,
            _ => TagKind::Open,
        },
        start: from,
        end,
        name_start,
        name_end,
        attrs,
        closed,
    })
}

/// Read attributes from `at` up to the tag's `>`, wherever it turns out to be.
///
/// The termination rule is the interesting part: a tag that is never closed runs to the end of
/// the buffer *unless* another `<` appears first. Stopping at the next `<` is what keeps one
/// unterminated tag from swallowing the rest of the file, which is the difference between
/// "completion is slightly wrong on this line" and "completion is dead below here".
fn tag_body(source: &str, at: usize) -> (Vec<Attr>, usize, bool, bool) {
    let bytes = source.as_bytes();
    let mut attrs = Vec::new();
    let mut i = at;
    let mut self_closing = false;

    while i < bytes.len() {
        match bytes[i] {
            b'>' => return (attrs, i + 1, true, self_closing),
            b'/' if source[i..].starts_with("/>") => {
                self_closing = true;
                return (attrs, i + 2, true, true);
            }
            b'<' => return (attrs, i, false, false),
            c if c.is_ascii_whitespace() => i += 1,
            _ => match attr_at(source, i) {
                Some(attr) => {
                    i = attr_end(&attr).max(i + 1);
                    attrs.push(attr);
                }
                // Punctuation where an attribute name should be — a stray `=`, a leftover
                // quote. Step over it rather than giving up on the tag: the `>` is usually one
                // character further on, and abandoning here would report the tag unterminated
                // and everything after it as content.
                None => i += 1,
            },
        }
    }
    (attrs, bytes.len(), false, self_closing)
}

fn attr_end(attr: &Attr) -> usize {
    // Past the closing quote when there is one, past the name otherwise.
    if attr.quoted {
        attr.value_end + 1
    } else {
        attr.name_end
    }
}

fn attr_at(source: &str, from: usize) -> Option<Attr> {
    let text = &source[from..];
    let name_len = text.find(|c: char| !is_name_char(c)).unwrap_or(text.len());
    if name_len == 0 {
        return None;
    }
    let name = text[..name_len].to_string();
    let name_start = from;
    let name_end = from + name_len;

    // `name = "value"` with any amount of space around the `=`; anything else means this
    // attribute has no value yet, which is a caret position rather than an error.
    let after_name = &source[name_end..];
    let spaces = after_name.len() - after_name.trim_start().len();
    let Some(after_eq) = after_name.trim_start().strip_prefix('=') else {
        return Some(Attr {
            name,
            name_start,
            name_end,
            value: String::new(),
            value_start: name_end,
            value_end: name_end,
            quoted: false,
        });
    };
    let eq_at = name_end + spaces + 1;
    let trimmed = after_eq.trim_start();
    let value_at = eq_at + (after_eq.len() - trimmed.len());
    let Some(quote) = trimmed.chars().next().filter(|c| *c == '"' || *c == '\'') else {
        return Some(Attr {
            name,
            name_start,
            name_end,
            value: String::new(),
            value_start: value_at,
            value_end: value_at,
            quoted: false,
        });
    };
    let body_at = value_at + quote.len_utf8();
    // An unterminated quote ends at the line's end rather than eating the rest of the file —
    // the same reasoning as the unterminated tag above.
    let body = &source[body_at..];
    let close = body
        .find(quote)
        .or_else(|| body.find('\n'))
        .unwrap_or(body.len());
    Some(Attr {
        name,
        name_start,
        name_end,
        value: body[..close].to_string(),
        value_start: body_at,
        value_end: body_at + close,
        quoted: true,
    })
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':')
}

/// Where the `<!DOCTYPE …>` ends, counting the internal subset's brackets.
fn doctype_end(source: &str, from: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = from;
    let mut in_subset = false;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'[' => in_subset = true,
                b']' => in_subset = false,
                b'>' if !in_subset => return i + 1,
                _ => {}
            },
        }
        i += 1;
    }
    bytes.len()
}

fn doctype(text: &str, offset: usize, end: usize) -> Doctype {
    let body = text.trim_start_matches("<!DOCTYPE").trim_start();
    let root_len = body.find(|c: char| !is_name_char(c)).unwrap_or(body.len());
    let root = body[..root_len].to_string();
    let rest = &body[root_len..];

    let quoted: Vec<String> = quoted_strings(rest);
    // `PUBLIC "id" "url"` has two, `SYSTEM "url"` has one. Reading it by keyword rather than by
    // count, because a `PUBLIC` with the system id omitted is legal and would otherwise be read
    // as a `SYSTEM`.
    let (public_id, system_id) = if rest.trim_start().starts_with("PUBLIC") {
        (quoted.first().cloned().unwrap_or_default(), quoted.get(1).cloned().unwrap_or_default())
    } else {
        (String::new(), quoted.first().cloned().unwrap_or_default())
    };
    Doctype { root, public_id, system_id, offset, end }
}

fn quoted_strings(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(['"', '\'']) {
        let quote = rest.as_bytes()[i] as char;
        let body = &rest[i + quote.len_utf8()..];
        let Some(close) = body.find(quote) else { break };
        out.push(body[..close].to_string());
        rest = &body[close + quote.len_utf8()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_carry_the_spans_a_completion_would_replace() {
        let src = "<project>\n  <artifactId>demo</artifactId>\n</project>\n";
        let s = scan(src);
        assert_eq!(s.tags.len(), 4);
        assert_eq!(s.root(), Some("project"));
        let a = &s.tags[1];
        assert_eq!(a.name, "artifactId");
        assert_eq!(&src[a.name_start..a.name_end], "artifactId");
        assert_eq!(a.kind, TagKind::Open);
        assert_eq!(s.tags[2].kind, TagKind::Close);
    }

    #[test]
    fn attributes_keep_their_value_span_without_the_quotes() {
        let src = r#"<action name="save" class='com.acme.Save' />"#;
        let t = &scan(src).tags[0];
        assert_eq!(t.kind, TagKind::SelfClose);
        assert_eq!(t.attrs.len(), 2);
        assert_eq!(t.attr("class").unwrap().value, "com.acme.Save");
        let a = t.attr("name").unwrap();
        assert_eq!(&src[a.value_start..a.value_end], "save");
        assert_eq!(t.attr_value_at(a.value_start + 1).unwrap().name, "name");
        assert!(t.attr_value_at(a.name_start).is_none(), "the name is not the value");
    }

    /// The mid-edit states. Each of these is a moment the user wanted help, so none of them may
    /// come back empty.
    #[test]
    fn a_half_typed_tag_is_still_a_tag() {
        let t = &scan("<project>\n  <artifac").tags[1];
        assert_eq!(t.name, "artifac");
        assert!(!t.closed);

        // An attribute with no value yet.
        let t = &scan(r#"<action name=>"#).tags[0];
        assert!(!t.attrs[0].quoted, "`name=` is a caret position, not a value");

        // An attribute name with nothing after it.
        let t = &scan(r#"<action nam"#).tags[0];
        assert_eq!(t.attrs[0].name, "nam");

        // An unterminated quote.
        let t = &scan("<action name=\"sav\n<other/>").tags[0];
        assert_eq!(t.attrs[0].value, "sav", "stops at the line, not at the end of the file");
    }

    /// The rule that keeps one unterminated tag from killing the rest of the file.
    #[test]
    fn an_unclosed_tag_stops_at_the_next_one() {
        let s = scan("<a\n<b/>\n<c/>");
        assert_eq!(s.tags.len(), 3);
        assert_eq!(s.tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert!(!s.tags[0].closed);
    }

    #[test]
    fn comments_cdata_and_instructions_are_inert() {
        let src = "<?xml version=\"1.0\"?>\n<!-- <ghost/> -->\n<a><![CDATA[<also-ghost/>]]></a>";
        let s = scan(src);
        assert_eq!(s.tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["a", "a"]);
        assert!(s.inert_at(src.find("ghost").unwrap()));
        assert!(!s.inert_at(src.find("<a>").unwrap() + 1));
    }

    #[test]
    fn the_doctype_is_read_by_keyword_rather_than_by_counting_strings() {
        let s = scan(
            "<!DOCTYPE struts PUBLIC \"-//Apache Software Foundation//DTD Struts Configuration 2.5//EN\" \
             \"http://struts.apache.org/dtds/struts-2.5.dtd\">\n<struts/>",
        );
        let d = s.doctype.unwrap();
        assert_eq!(d.root, "struts");
        assert!(d.public_id.starts_with("-//Apache"));
        assert_eq!(d.system_id, "http://struts.apache.org/dtds/struts-2.5.dtd");

        let s = scan("<!DOCTYPE web-app SYSTEM \"web-app_2_3.dtd\">\n<web-app/>");
        let d = s.doctype.unwrap();
        assert_eq!(d.public_id, "");
        assert_eq!(d.system_id, "web-app_2_3.dtd");
    }

    #[test]
    fn an_internal_subset_does_not_end_the_doctype_early() {
        let s = scan("<!DOCTYPE a [\n  <!ENTITY x \"y\">\n]>\n<a/>");
        assert_eq!(s.doctype.unwrap().root, "a");
        assert_eq!(s.tags.len(), 1, "the entity declaration is not a tag");
    }

    #[test]
    fn nesting_is_replayed_from_the_tag_sequence() {
        let src = "<project>\n  <build>\n    <plugins>\n      \n    </plugins>\n  </build>\n</project>";
        let s = scan(src);
        let at = src.find("      \n").unwrap() + 6;
        assert_eq!(s.path_at(at), ["project", "build", "plugins"]);
        assert_eq!(s.parent_at(at), Some("plugins"));
        assert_eq!(s.path_at(src.len()), Vec::<&str>::new(), "everything closed again");
    }

    /// While you type `</`, the name is not there yet — popping on a guess would report the
    /// caret one level too shallow, which is where a completion list would come from.
    #[test]
    fn a_close_tag_matching_nothing_does_not_unwind_the_stack() {
        let src = "<a><b></c>";
        let s = scan(src);
        assert_eq!(s.path_at(src.len()), ["a", "b"]);
        // And a well-matched close unwinds everything under it, so a missing `</b>` recovers.
        let src = "<a><b></a><";
        assert_eq!(scan(src).path_at(src.len()), Vec::<&str>::new());
    }

    #[test]
    fn a_bare_angle_bracket_in_text_is_not_markup() {
        let s = scan("<a>1 < 2</a>");
        assert_eq!(s.tags.len(), 2);
    }

    #[test]
    fn prefixes_are_kept_on_the_name_and_stripped_on_demand() {
        let t = &scan("<xs:element xsi:type=\"x\"/>").tags[0];
        assert_eq!(t.name, "xs:element");
        assert_eq!(t.local(), "element");
        assert_eq!(t.attrs[0].local(), "type");
    }
}
