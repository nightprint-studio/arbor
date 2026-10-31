//! A build script, **scanned** — never evaluated.
//!
//! `build.gradle` is Groovy and `build.gradle.kts` is Kotlin: both are programs, and reading what a
//! build *means* would take a Gradle run. What every real build file is made of, though, is a small
//! declarative subset — `plugins { … }`, `dependencies { … }`, `include 'a', 'b'`,
//! `sourceCompatibility = '17'` — and that subset has the same shape in both languages: statements
//! (a line, or several while a bracket is open) and `name { … }` blocks. This module reads that
//! shape and nothing else.
//!
//! ## Offsets survive the masking
//!
//! A scan must not see a dependency inside a comment, nor a brace inside a string, and every answer
//! needs a byte offset the editor can jump to. So the text is *masked* instead of rewritten:
//! comments become spaces (newlines kept), and for structure the contents of string literals
//! become spaces too. Both masks are exactly as long as the source, which is what keeps one offset
//! meaning one place in all three.

use std::ops::Range;

/// A script, masked once and then asked questions of.
#[derive(Debug, Clone)]
pub struct Script {
    /// The source with comments blanked; string literals intact.
    code: String,
    /// `code` with the *contents* of string literals blanked too — brace and paren matching reads
    /// this one, so a `}` inside a string never closes a block.
    shape: String,
    line_starts: Vec<usize>,
}

/// A `head { … }` block at one nesting level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The statement head before the brace, trimmed: `dependencies`, `java`,
    /// `tasks.register("docs")`, `subprojects`.
    pub head: String,
    /// Offset of the first character of the head.
    pub head_start: usize,
    /// Offset of the `{`.
    pub open: usize,
    /// Offset of the matching `}` — the end of the script when it was never closed, which is the
    /// ordinary state of a file being typed.
    pub close: usize,
}

impl Block {
    /// The text between the braces.
    pub fn body(&self) -> Range<usize> {
        (self.open + 1)..self.close.max(self.open + 1)
    }
}

/// A string literal found in a piece of script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Literal {
    /// What is between the quotes, escapes left as written.
    pub text: String,
    /// Offset of the opening quote, within the text the literal was found in.
    pub start: usize,
    /// Offset just past the closing quote.
    pub end: usize,
    /// `"…"` can interpolate (`$x`, `${x}`); `'…'` never does in either language.
    pub double_quoted: bool,
}

impl Script {
    pub fn new(text: &str) -> Self {
        let (code, shape) = mask(text);
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { code, shape, line_starts }
    }

    pub fn len(&self) -> usize {
        self.code.len()
    }

    pub fn is_empty(&self) -> bool {
        self.code.is_empty()
    }

    /// The whole script.
    pub fn all(&self) -> Range<usize> {
        0..self.code.len()
    }

    /// The masked-comments text of `range`. Empty for a range that is not a valid slice.
    pub fn text(&self, range: Range<usize>) -> &str {
        self.code.get(range).unwrap_or("")
    }

    /// 1-based line of a byte offset.
    pub fn line_of(&self, offset: usize) -> u32 {
        match self.line_starts.binary_search(&offset) {
            Ok(i) => i as u32 + 1,
            Err(i) => i as u32,
        }
    }

    /// The blocks that sit directly in `range`, in order. Blocks inside those are not returned —
    /// ask again with the body of the one you want.
    ///
    /// A `{` inside parentheses (`foo(a, { … })`) is an argument, not a block, and is skipped over.
    pub fn blocks(&self, range: Range<usize>) -> Vec<Block> {
        let sh = self.shape.as_bytes();
        let end = range.end.min(sh.len());
        let mut out = Vec::new();
        let mut parens = 0i32;
        let mut statement_start = range.start;
        let mut i = range.start;
        while i < end {
            match sh[i] {
                b'(' | b'[' => parens += 1,
                b')' | b']' => parens = (parens - 1).max(0),
                b'\n' | b';' if parens == 0 => statement_start = i + 1,
                b'{' => {
                    let close = self.matching_brace(i, end);
                    if parens == 0 {
                        let head_text = self.text(statement_start..i);
                        let trimmed = head_text.trim();
                        if !trimmed.is_empty() {
                            let lead = head_text.len() - head_text.trim_start().len();
                            out.push(Block {
                                head: trimmed.to_string(),
                                head_start: statement_start + lead,
                                open: i,
                                close,
                            });
                        }
                    }
                    i = close;
                    statement_start = close + 1;
                }
                _ => {}
            }
            i += 1;
        }
        out
    }

    /// The `{`'s partner, or `limit` when it is never closed.
    fn matching_brace(&self, open: usize, limit: usize) -> usize {
        let sh = self.shape.as_bytes();
        let mut depth = 0i32;
        let mut i = open;
        while i < limit.min(sh.len()) {
            match sh[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return i;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        limit
    }

    /// The statements that sit directly in `range`, as byte ranges.
    ///
    /// A statement ends at a newline or `;` with every bracket closed, except that a line ending in
    /// `,` or `+`, or followed by a line that starts with `.`, continues. A trailing closure
    /// (`implementation("x") { exclude … }`) belongs to its statement.
    pub fn statements(&self, range: Range<usize>) -> Vec<Range<usize>> {
        let sh = self.shape.as_bytes();
        let end = range.end.min(sh.len());
        let mut out = Vec::new();
        let mut depth = 0i32;
        let mut start = range.start;
        let mut i = range.start;
        while i < end {
            match sh[i] {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = (depth - 1).max(0),
                b'\n' | b';' if depth == 0 => {
                    if !(sh[i] == b'\n' && self.continues(start, i, end)) {
                        if !self.text(start..i).trim().is_empty() {
                            out.push(start..i);
                        }
                        start = i + 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        if start < end && !self.text(start..end).trim().is_empty() {
            out.push(start..end);
        }
        out
    }

    /// Whether the statement that began at `start` goes on past the newline at `newline`.
    fn continues(&self, start: usize, newline: usize, limit: usize) -> bool {
        let line = self.text(start..newline).trim_end();
        if line.ends_with(',') || line.ends_with('+') {
            return true;
        }
        // A method chain: the next non-blank thing is `.foo(…)`.
        let rest = self.text((newline + 1).min(limit)..limit);
        rest.trim_start().starts_with('.')
    }

    /// Where the statement that begins at `from` ends — same rule as [`statements`](Self::statements).
    pub fn statement_end(&self, from: usize) -> usize {
        let sh = self.shape.as_bytes();
        let mut depth = 0i32;
        let mut i = from;
        while i < sh.len() {
            match sh[i] {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = (depth - 1).max(0),
                b'\n' | b';' if depth == 0 => {
                    if !(sh[i] == b'\n' && self.continues(from, i, sh.len())) {
                        return i;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        sh.len()
    }

    /// Offsets of every occurrence of `word` as a whole identifier that is not a member access
    /// (`foo.include` is not `include`) and is followed by whitespace, `(`, `.` or `{`.
    pub fn find_word(&self, word: &str) -> Vec<usize> {
        let sh = self.shape.as_bytes();
        let w = word.as_bytes();
        let mut out = Vec::new();
        if w.is_empty() || sh.len() < w.len() {
            return out;
        }
        let mut i = 0;
        while i + w.len() <= sh.len() {
            if &sh[i..i + w.len()] == w {
                let before_ok = i == 0 || !(is_ident(sh[i - 1]) || sh[i - 1] == b'.');
                let after = sh.get(i + w.len()).copied().unwrap_or(b'\n');
                let after_ok = !is_ident(after);
                if before_ok && after_ok {
                    out.push(i);
                }
            }
            i += 1;
        }
        out
    }
}

pub(crate) fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Blank `v[from..to]`, keeping newlines so lines stay where they were.
fn blank(v: &mut [u8], from: usize, to: usize) {
    let to = to.min(v.len());
    let mut k = from;
    while k < to {
        if v[k] != b'\n' {
            v[k] = b' ';
        }
        k += 1;
    }
}

fn find(hay: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if from >= hay.len() || pat.is_empty() {
        return None;
    }
    hay[from..].windows(pat.len()).position(|w| w == pat).map(|p| p + from)
}

/// `(code, shape)` — see the module doc. Both are exactly as long as `text`.
fn mask(text: &str) -> (String, String) {
    let b = text.as_bytes();
    let n = b.len();
    let mut code = b.to_vec();
    let mut shape = b.to_vec();
    let mut i = 0;
    while i < n {
        let c = b[i];
        if c == b'/' && i + 1 < n && b[i + 1] == b'/' {
            let end = b[i..].iter().position(|&x| x == b'\n').map(|p| i + p).unwrap_or(n);
            blank(&mut code, i, end);
            blank(&mut shape, i, end);
            i = end;
            continue;
        }
        if c == b'/' && i + 1 < n && b[i + 1] == b'*' {
            let end = find(b, i + 2, b"*/").map(|p| p + 2).unwrap_or(n);
            blank(&mut code, i, end);
            blank(&mut shape, i, end);
            i = end;
            continue;
        }
        if c == b'"' || c == b'\'' {
            if i + 2 < n && b[i + 1] == c && b[i + 2] == c {
                let inner_end = find(b, i + 3, &[c, c, c]).unwrap_or(n);
                blank(&mut shape, i + 3, inner_end);
                i = (inner_end + 3).min(n);
                continue;
            }
            let mut j = i + 1;
            while j < n && b[j] != c && b[j] != b'\n' {
                if b[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            let inner_end = j.min(n);
            blank(&mut shape, i + 1, inner_end);
            i = (inner_end + 1).min(n);
            continue;
        }
        i += 1;
    }
    (into_string(code), into_string(shape))
}

fn into_string(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// Every string literal in `s`, in order. Comments are not skipped here — pass text that came out of
/// [`Script::text`], where they are already blank.
pub fn literals(s: &str) -> Vec<Literal> {
    let b = s.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let c = b[i];
        if c != b'"' && c != b'\'' {
            i += 1;
            continue;
        }
        if i + 2 < n && b[i + 1] == c && b[i + 2] == c {
            let inner_end = find(b, i + 3, &[c, c, c]).unwrap_or(n);
            out.push(Literal {
                text: String::from_utf8_lossy(&b[i + 3..inner_end]).into_owned(),
                start: i,
                end: (inner_end + 3).min(n),
                double_quoted: c == b'"',
            });
            i = (inner_end + 3).min(n);
            continue;
        }
        let mut j = i + 1;
        while j < n && b[j] != c && b[j] != b'\n' {
            if b[j] == b'\\' {
                j += 1;
            }
            j += 1;
        }
        let inner_end = j.min(n);
        out.push(Literal {
            text: String::from_utf8_lossy(&b[i + 1..inner_end]).into_owned(),
            start: i,
            end: (inner_end + 1).min(n),
            double_quoted: c == b'"',
        });
        i = (inner_end + 1).min(n);
    }
    out
}

/// `s` split at commas that are not inside brackets or strings.
pub fn split_top_level(s: &str) -> Vec<String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(q) => {
                if c == b'\\' {
                    i += 1;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = (depth - 1).max(0),
                b',' if depth == 0 => {
                    out.push(s[start..i].to_string());
                    start = i + 1;
                }
                _ => {}
            },
        }
        i += 1;
    }
    out.push(s[start..].to_string());
    out
}

/// The text inside the first balanced `( … )` of `s`, when `s` (ignoring leading whitespace) begins
/// with one. `None` for text that does not start with a paren or never closes it.
pub fn paren_inner(s: &str) -> Option<&str> {
    let t = s.trim_start();
    if !t.starts_with('(') {
        return None;
    }
    let b = t.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(q) => {
                if c == b'\\' {
                    i += 1;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&t[1..i]);
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_hides_what_it_contains_and_keeps_the_offsets() {
        let s = Script::new("a // implementation 'x:y:1'\nb");
        assert_eq!(s.len(), "a // implementation 'x:y:1'\nb".len());
        assert!(!s.text(s.all()).contains("implementation"));
        assert_eq!(s.line_of(s.len() - 1), 2);
    }

    #[test]
    fn a_brace_inside_a_string_does_not_close_a_block() {
        let s = Script::new("dependencies {\n  implementation 'a:b:1' // }\n  api \"}\"\n}\nafter {}");
        let blocks = s.blocks(s.all());
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].head, "dependencies");
        assert_eq!(blocks[1].head, "after");
        assert!(s.text(blocks[0].body()).contains("api"));
    }

    #[test]
    fn nested_blocks_are_not_returned_at_the_outer_level() {
        let s = Script::new("subprojects {\n  dependencies { implementation 'a:b:1' }\n}");
        let top = s.blocks(s.all());
        assert_eq!(top.len(), 1);
        let inner = s.blocks(top[0].body());
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].head, "dependencies");
    }

    #[test]
    fn a_statement_continues_through_a_trailing_comma_and_an_open_paren() {
        let s = Script::new("include 'a',\n  'b'\ninclude(\n 'c'\n)\n");
        let st = s.statements(s.all());
        assert_eq!(st.len(), 2);
        assert!(s.text(st[0].clone()).contains("'b'"));
    }

    #[test]
    fn literals_read_all_quote_styles() {
        let lits = literals("a 'x:y' \"p:q:${v}\" '''tri'''");
        let texts: Vec<&str> = lits.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["x:y", "p:q:${v}", "tri"]);
        assert!(!lits[0].double_quoted && lits[1].double_quoted);
    }

    #[test]
    fn splitting_respects_brackets_and_quotes() {
        let parts = split_top_level("a(1, 2), 'x,y', [3, 4]");
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1].trim(), "'x,y'");
    }

    #[test]
    fn paren_inner_takes_the_balanced_group() {
        assert_eq!(paren_inner(" (a(b), c) { x }"), Some("a(b), c"));
        assert_eq!(paren_inner("a"), None);
    }
}
