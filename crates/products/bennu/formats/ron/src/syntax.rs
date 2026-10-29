//! Is this RON — and where does it stop being.
//!
//! Two readers, and the order is the point. The parser (`arbor-studio-ron`'s) is the authority on
//! validity but it reports an unclosed bracket where it *runs out of input*: at the end of the
//! file, a screen away from the `(` that caused it. So brackets are checked first, by a lexer that
//! knows strings, chars and comments, and reported **at the opener**; only a document whose
//! brackets balance reaches the parser, whose verdict then is about something else (a missing
//! comma, a stray character).

use arbor_studio_ron::prelude::parse;

use crate::{RonDiagnostic, RonSeverity};

/// Every syntax problem in `src`: at most one, since both readers stop at the first.
pub fn syntax_diagnostics(src: &str) -> Vec<RonDiagnostic> {
    if let Some(d) = bracket_diagnostic(src) {
        return vec![d];
    }
    match parse(src) {
        Ok(_) => Vec::new(),
        Err(e) => {
            let start = offset_of(src, e.line, e.col);
            vec![RonDiagnostic {
                start,
                end: token_end(src, start),
                severity: RonSeverity::Error,
                code: "ron-syntax",
                message: e.msg,
            }]
        }
    }
}

/// An unbalanced or mismatched bracket, reported where the *cause* is: the opener that never
/// closed, or the closer that closes the wrong thing.
pub fn bracket_diagnostic(src: &str) -> Option<RonDiagnostic> {
    let b = src.as_bytes();
    let mut stack: Vec<(u8, usize)> = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => i = skip_block_comment(b, i),
            b'"' => i = skip_string(b, i),
            b'r' if is_raw_string_at(b, i) => i = skip_raw_string(b, i),
            b'\'' => i = skip_char(b, i),
            c @ (b'(' | b'[' | b'{') => {
                stack.push((c, i));
                i += 1;
            }
            c @ (b')' | b']' | b'}') => {
                match stack.pop() {
                    Some((open, _)) if closer_of(open) == c => {}
                    Some((open, at)) => {
                        return Some(error(
                            i,
                            i + 1,
                            format!(
                                "`{}` closes the `{}` opened at line {}, which needs `{}`",
                                c as char,
                                open as char,
                                line_of(src, at),
                                closer_of(open) as char
                            ),
                        ));
                    }
                    None => {
                        return Some(error(i, i + 1, format!("unmatched `{}`", c as char)));
                    }
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    stack.last().map(|&(open, at)| {
        error(at, at + 1, format!("unclosed `{}` — expected `{}`", open as char, closer_of(open) as char))
    })
}

fn error(start: usize, end: usize, message: String) -> RonDiagnostic {
    RonDiagnostic { start, end, severity: RonSeverity::Error, code: "ron-brackets", message }
}

fn closer_of(open: u8) -> u8 {
    match open {
        b'(' => b')',
        b'[' => b']',
        _ => b'}',
    }
}

fn line_of(src: &str, at: usize) -> usize {
    src.as_bytes()[..at.min(src.len())].iter().filter(|c| **c == b'\n').count() + 1
}

fn skip_block_comment(b: &[u8], mut i: usize) -> usize {
    let mut depth = 0usize;
    while i < b.len() {
        if b[i..].starts_with(b"/*") {
            depth += 1;
            i += 2;
        } else if b[i..].starts_with(b"*/") {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    i
}

fn skip_string(b: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

fn is_raw_string_at(b: &[u8], i: usize) -> bool {
    // Not the tail of an identifier (`color"` is not a thing, but `bar#` is not a raw string start).
    if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') {
        return false;
    }
    let mut j = i + 1;
    while b.get(j) == Some(&b'#') {
        j += 1;
    }
    b.get(j) == Some(&b'"')
}

fn skip_raw_string(b: &[u8], i: usize) -> usize {
    let mut j = i + 1;
    let mut hashes = 0usize;
    while b.get(j) == Some(&b'#') {
        hashes += 1;
        j += 1;
    }
    j += 1; // opening quote
    while j < b.len() {
        if b[j] == b'"' && b[j + 1..].iter().take(hashes).filter(|c| **c == b'#').count() == hashes {
            return j + 1 + hashes;
        }
        j += 1;
    }
    b.len()
}

/// A char literal (`'a'`, `'\n'`, `'\''`). A `'` that does not close within a few bytes is not one,
/// and is stepped over alone rather than swallowing the rest of the file.
fn skip_char(b: &[u8], i: usize) -> usize {
    let body = if b.get(i + 1) == Some(&b'\\') { i + 3 } else { i + 2 };
    for end in body..(body + 8).min(b.len()) {
        if b[end] == b'\'' {
            return end + 1;
        }
        if b[end] == b'\n' {
            break;
        }
    }
    i + 1
}

/// The byte offset of a 1-based `line` / char-counted `col`, as the parser reports them.
fn offset_of(src: &str, line: usize, col: usize) -> usize {
    let mut start = 0usize;
    for _ in 1..line {
        match src[start..].find('\n') {
            Some(i) => start += i + 1,
            None => return src.len(),
        }
    }
    let line_text = &src[start..];
    let in_line = line_text
        .char_indices()
        .nth(col.saturating_sub(1))
        .map_or(line_text.find('\n').unwrap_or(line_text.len()), |(i, _)| i);
    start + in_line
}

/// The extent of the token at `start`, so the squiggle covers something. At the end of the text
/// (`unexpected end of input`) there is no token: the last character stands in.
fn token_end(src: &str, start: usize) -> usize {
    let rest = &src[start.min(src.len())..];
    match rest.char_indices().find(|(_, c)| c.is_whitespace() || matches!(c, ',' | ')' | ']' | '}')) {
        Some((0, c)) => start + c.len_utf8(),
        Some((i, _)) => start + i,
        None if rest.is_empty() => src.len(),
        None => src.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_balanced_document_has_no_bracket_problem() {
        assert!(bracket_diagnostic("(a: [1, 2], b: {\"k\": (3)})").is_none());
    }

    #[test]
    fn an_unclosed_bracket_is_reported_at_the_opener_not_at_the_end() {
        let src = "(\n  a: [1, 2,\n  b: 3,\n)";
        let d = bracket_diagnostic(src).expect("a mismatch");
        // `)` closes the `[` opened on line 2.
        assert!(d.message.contains("line 2"), "{}", d.message);
        assert_eq!(&src[d.start..d.end], ")");
        let open = "(a: 1,\n b: 2";
        let d = bracket_diagnostic(open).unwrap();
        assert_eq!((d.start, d.end), (0, 1));
        assert!(d.message.starts_with("unclosed `(`"));
    }

    #[test]
    fn a_stray_closer_is_reported() {
        let d = bracket_diagnostic("(a: 1))").unwrap();
        assert_eq!(d.message, "unmatched `)`");
        assert_eq!(d.start, 6);
    }

    #[test]
    fn brackets_inside_strings_chars_and_comments_do_not_count() {
        let src = "(a: \"((\", b: ')', c: r#\"]\"#, // (\n d: /* { */ 1)";
        assert!(bracket_diagnostic(src).is_none(), "{:?}", bracket_diagnostic(src));
    }

    #[test]
    fn a_parser_error_is_pointed_at_its_token() {
        let src = "(a: 1 b: 2)";
        let ds = syntax_diagnostics(src);
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].code, "ron-syntax");
        assert!(ds[0].start <= src.find("b:").unwrap() + 1);
        assert!(ds[0].end > ds[0].start);
    }

    #[test]
    fn valid_ron_has_no_diagnostics() {
        assert!(syntax_diagnostics("#![enable(implicit_some)]\n(a: Some(1), b: [Red, Green])").is_empty());
    }

    #[test]
    fn offsets_count_bytes_not_chars() {
        let src = "(a: \"è\", b: @)";
        let d = &syntax_diagnostics(src)[0];
        assert_eq!(&src[d.start..d.end], "@");
    }
}
