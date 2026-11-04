//! A forgiving Java tokenizer for the formatter.
//!
//! It does not need to understand Java, only to cut it into pieces that can be moved around without
//! changing meaning: words, numbers, literals, comments and punctuation, each remembering how much
//! whitespace (and how many line breaks) preceded it. It never fails — an unterminated string or
//! comment simply ends at the line or the file — because the formatter must work on half-written code.
//!
//! `>` is always a token of its own so that `List<List<String>>` closes cleanly; the shift operators
//! are recognised later, from adjacency, where the spacing rules need them.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Word,
    Number,
    /// `"…"` and `'…'`.
    Str,
    /// `"""…"""`, newlines included.
    TextBlock,
    LineComment,
    BlockComment,
    Punct,
}

#[derive(Clone, Debug)]
pub(crate) struct Tok {
    pub kind: Kind,
    pub text: String,
    /// Line breaks in the whitespace before this token (0 = same line as the previous token).
    pub nl_before: usize,
    /// Any whitespace at all before it (a line break counts).
    pub ws_before: bool,
    /// The whitespace text itself — what "leave the spacing alone" has to reproduce, and the source of
    /// a line's original indentation (the part after the last newline).
    pub gap: String,
}

impl Tok {
    pub fn is(&self, text: &str) -> bool {
        self.text == text && !matches!(self.kind, Kind::Str | Kind::TextBlock | Kind::LineComment | Kind::BlockComment)
    }

    pub fn is_comment(&self) -> bool {
        matches!(self.kind, Kind::LineComment | Kind::BlockComment)
    }

    pub fn is_word(&self) -> bool {
        self.kind == Kind::Word
    }
}

/// Multi-character operators, longest first. `>>`, `>>>` and their assignments are deliberately
/// absent (see the module doc); `>=` is present because `List<String>=` is not code.
const OPERATORS: [&str; 21] = [
    "<<=", "...", "->", "::", "++", "--", "&&", "||", "==", "!=", "<=", ">=", "+=", "-=", "*=", "/=",
    "%=", "&=", "|=", "^=", "<<",
];

pub(crate) fn lex(src: &str) -> Vec<Tok> {
    let b = src.as_bytes();
    let mut out: Vec<Tok> = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        // Whitespace run.
        let ws_start = i;
        let mut nl = 0usize;
        while i < b.len() && (b[i] == b' ' || b[i] == b'\t' || b[i] == b'\n' || b[i] == b'\r' || b[i] == 0x0c) {
            if b[i] == b'\n' {
                nl += 1;
            }
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        let ws = i > ws_start;
        let start = i;
        let c = b[i];
        let kind;
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            // A `\r` before the newline is whitespace, not comment text.
            while i > start && b[i - 1] == b'\r' {
                i -= 1;
            }
            kind = Kind::LineComment;
        } else if c == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            i += 2;
            while i < b.len() && !(b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(b.len());
            kind = Kind::BlockComment;
        } else if src[i..].starts_with("\"\"\"") {
            i += 3;
            while i < b.len() && !src[i..].starts_with("\"\"\"") {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 3).min(b.len());
            kind = Kind::TextBlock;
        } else if c == b'"' || c == b'\'' {
            i += 1;
            while i < b.len() && b[i] != c && b[i] != b'\n' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            if i < b.len() && b[i] == c {
                i += 1;
            }
            i = i.min(b.len());
            kind = Kind::Str;
        } else if c.is_ascii_digit() || (c == b'.' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            i += 1;
            while i < b.len() {
                let d = b[i];
                if d.is_ascii_alphanumeric() || d == b'_' {
                    i += 1;
                } else if d == b'.' && i + 1 < b.len() && b[i + 1].is_ascii_digit() {
                    i += 1;
                } else if (d == b'+' || d == b'-') && matches!(b[i - 1], b'e' | b'E' | b'p' | b'P') && !src[start..i].starts_with("0x") {
                    i += 1;
                } else {
                    break;
                }
            }
            kind = Kind::Number;
        } else if is_word_start(src, i) {
            i = advance_word(src, i);
            kind = Kind::Word;
        } else {
            // Punctuation: a known operator, else one character (which may be multi-byte).
            let rest = &src[i..];
            let op = OPERATORS.iter().find(|o| rest.starts_with(**o));
            i += match op {
                Some(o) => o.len(),
                None => rest.chars().next().map(char::len_utf8).unwrap_or(1),
            };
            kind = Kind::Punct;
        }
        out.push(Tok { kind, text: src[start..i].to_string(), nl_before: nl, ws_before: ws, gap: src[ws_start..start].to_string() });
    }
    out
}

fn is_word_start(src: &str, i: usize) -> bool {
    src[i..].chars().next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
}

fn advance_word(src: &str, mut i: usize) -> usize {
    while let Some(c) = src[i..].chars().next() {
        if c.is_alphanumeric() || c == '_' || c == '$' {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    i
}

/// What must be identical before and after formatting: every token's text, with comments compared
/// without their whitespace (comment alignment is allowed to move spaces around inside them).
/// Imports are excluded by the caller; everything else has to survive byte for byte.
pub(crate) fn signature(toks: &[Tok]) -> Vec<String> {
    toks.iter()
        .map(|t| match t.kind {
            Kind::BlockComment => t.text.split_whitespace().collect::<Vec<_>>().join(" ").replace("* ", "*"),
            Kind::LineComment => t.text.trim_start_matches('/').split_whitespace().collect::<Vec<_>>().join(" "),
            _ => t.text.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(src: &str) -> Vec<String> {
        lex(src).into_iter().map(|t| t.text).collect()
    }

    #[test]
    fn words_numbers_and_operators() {
        assert_eq!(texts("a = b->c + 1.5e+3f;"), ["a", "=", "b", "->", "c", "+", "1.5e+3f", ";"]);
    }

    #[test]
    fn a_closing_generic_is_two_tokens() {
        assert_eq!(texts("List<List<String>> x"), ["List", "<", "List", "<", "String", ">", ">", "x"]);
    }

    #[test]
    fn strings_comments_and_text_blocks_are_single_tokens() {
        let toks = lex("s = \"a // b\"; // tail\n/* c */ t = \"\"\"\n x \"\"\";");
        let kinds: Vec<Kind> = toks.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&Kind::LineComment) && kinds.contains(&Kind::BlockComment) && kinds.contains(&Kind::TextBlock));
        assert_eq!(toks.iter().filter(|t| t.kind == Kind::Str).count(), 1);
    }

    #[test]
    fn line_breaks_are_counted_before_the_token_that_follows_them() {
        let toks = lex("a\n\n b");
        assert_eq!(toks[1].nl_before, 2);
        assert!(toks[1].ws_before);
    }

    #[test]
    fn an_unterminated_string_ends_at_the_line() {
        assert_eq!(texts("x = \"abc\ny"), ["x", "=", "\"abc", "y"]);
    }
}
