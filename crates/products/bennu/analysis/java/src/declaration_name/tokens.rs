//! Just enough of a Java lexer to read a declaration backwards from the caret.
//!
//! Not the tree-sitter parse, deliberately: the buffer is mid-edit by definition here —
//! `private final Foo ` is not a declaration yet — and a parse of it is error recovery's guess about
//! something broken. Tokens do not depend on the statement being finished. What the lexer must get
//! right is only what hides code: comments and literals, including the one the caret is inside.

/// What a token is, as far as reading a declaration needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// An identifier or a keyword — the grammar tells them apart, the lexer does not.
    Word,
    Number,
    /// A string, character or text-block literal, quotes included.
    Literal,
    /// One byte of punctuation. `->`, `...` and `>>` arrive as their single bytes, which is what
    /// lets a `>` that closes two type arguments be counted as two.
    Punct(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Token {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
}

/// Every token of `source`, or `None` when `caret` is inside a comment or a literal — where a
/// declaration cannot be being written, whatever the text there looks like.
pub(super) fn tokens(source: &str, caret: usize) -> Option<Vec<Token>> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let byte = bytes[i];
        if byte.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if byte == b'/' && bytes.get(i + 1) == Some(&b'/') {
            let end = bytes[i..].iter().position(|&b| b == b'\n').map_or(bytes.len(), |p| i + p);
            if hides(start, end, true, caret) {
                return None;
            }
            i = end;
            continue;
        }
        if byte == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let close = bytes[i + 2..].windows(2).position(|w| w == b"*/").map(|p| i + 2 + p + 2);
            let (end, closed) = close.map_or((bytes.len(), false), |end| (end, true));
            if hides(start, end, !closed, caret) {
                return None;
            }
            i = end;
            continue;
        }
        let kind = if byte == b'"' || byte == b'\'' {
            let (end, closed) = literal_end(bytes, i);
            if hides(start, end, !closed, caret) {
                return None;
            }
            i = end;
            Kind::Literal
        } else if is_identifier_start(byte) {
            while i < bytes.len() && is_identifier_byte(bytes[i]) {
                i += 1;
            }
            Kind::Word
        } else if byte.is_ascii_digit() {
            while i < bytes.len() && (is_identifier_byte(bytes[i]) || bytes[i] == b'.') {
                i += 1;
            }
            Kind::Number
        } else {
            i += 1;
            Kind::Punct(byte)
        };
        out.push(Token { kind, start, end: i });
    }
    Some(out)
}

/// Whether a comment or literal spanning `start..end` covers `caret`. One that runs to the end of its
/// line or of the file without being closed covers the caret sitting right at that end, too: `// Foo |`
/// is still a comment, `"Foo" |` is no longer a string.
fn hides(start: usize, end: usize, open_at_end: bool, caret: usize) -> bool {
    start < caret && (caret < end || (open_at_end && caret == end))
}

/// Where the literal opened at `start` ends, and whether it was closed. A string or character left
/// open ends at its line, as the compiler reads it; a text block runs until its closing `"""`.
fn literal_end(bytes: &[u8], start: usize) -> (usize, bool) {
    let quote = bytes[start];
    if quote == b'"' && bytes[start..].starts_with(b"\"\"\"") {
        let mut i = start + 3;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'"' if bytes[i..].starts_with(b"\"\"\"") => return (i + 3, true),
                _ => i += 1,
            }
        }
        return (bytes.len(), false);
    }
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'\n' => return (i, false),
            b if b == quote => return (i + 1, true),
            _ => i += 1,
        }
    }
    (bytes.len(), false)
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' || byte >= 0x80
}

/// A byte that can be part of a Java identifier. Every non-ASCII byte counts: Java allows letters
/// from any script, and none of the punctuation this reads is outside ASCII.
pub(super) fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte >= 0x80
}

/// The source text of `token`.
pub(super) fn text<'s>(source: &'s str, token: &Token) -> &'s str {
    &source[token.start..token.end]
}

/// Whether the token at `index` exists and is the punctuation `byte`.
pub(super) fn is(tokens: &[Token], index: usize, byte: u8) -> bool {
    tokens.get(index).is_some_and(|t| t.kind == Kind::Punct(byte))
}

/// Whether a word reads as a class name: it starts with a capital letter.
pub(super) fn starts_uppercase(word: &str) -> bool {
    word.chars().next().is_some_and(char::is_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<Kind> {
        tokens(source, 0).unwrap().iter().map(|t| t.kind).collect()
    }

    #[test]
    fn comments_produce_no_tokens_and_literals_one() {
        assert_eq!(
            kinds("a /* b */ // c\n\"d // e\" 'f'"),
            [Kind::Word, Kind::Literal, Kind::Literal],
        );
    }

    #[test]
    fn a_caret_inside_a_comment_or_literal_is_refused() {
        let line = "// Order ";
        assert!(tokens(line, line.len()).is_none());
        let block = "/* Order  */";
        assert!(tokens(block, 9).is_none());
        let open_string = "s = \"Order \nx";
        assert!(tokens(open_string, 11).is_none());
        let text_block = "s = \"\"\"\nOrder ";
        assert!(tokens(text_block, text_block.len()).is_none());
    }

    #[test]
    fn a_caret_just_past_a_closed_comment_or_literal_is_code() {
        let block = "/* x */ ";
        assert!(tokens(block, block.len()).is_some());
        let string = "\"x\" ";
        assert!(tokens(string, 3).is_some());
        // A line comment ends at its newline: the next line is code.
        let line = "// x\nOrder ";
        assert!(tokens(line, line.len()).is_some());
    }

    #[test]
    fn an_escaped_quote_does_not_close_the_literal() {
        assert_eq!(kinds(r#""a \" b" c"#), [Kind::Literal, Kind::Word]);
    }
}
