//! Decoding literals to their *values*.
//!
//! Ranges are enough for a rewriter, but not for `picus-analyze`: to say "these
//! two INSERTs write the same key" it has to compare `'SOGLIA_SCONTO'` in an
//! Oracle file with `'SOGLIA_SCONTO'` in a PostgreSQL one, and the two may be
//! spelled with different quoting. So string literals arrive here **decoded** —
//! doubling undone, q-quote and dollar-quote delimiters removed — while the
//! range still points at the original bytes.

use serde::{Deserialize, Serialize};

/// The value of a literal, when the crate can determine one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum LiteralValue {
    /// Decoded text, without its quotes.
    String(String),
    /// Kept as written. `1.50` and `1.5` are the same number and different
    /// source, and which of those matters depends on the caller.
    Number(String),
    Bool(bool),
    Null,
}

/// Decode a literal node. `kind` is the Tree-sitter node kind, `text` the exact
/// source bytes of the node.
///
/// Returns `None` for anything that is not a literal, and for the two encodings
/// this crate refuses to guess at: `U&'…'` (needs the UESCAPE rules) and bit /
/// hex strings (whose value is bytes, not text).
pub fn decode(kind: &str, text: &str) -> Option<LiteralValue> {
    match kind {
        "string_literal" => Some(LiteralValue::String(single_quoted(text, 0))),
        // `N'…'` is a national-character string: same shape, one prefix char.
        "national_string" => Some(LiteralValue::String(single_quoted(text, 1))),
        "escape_string" => Some(LiteralValue::String(escape_string(text))),
        "q_string" => q_string(text).map(LiteralValue::String),
        "dollar_quoted_string" => dollar_quoted(text).map(LiteralValue::String),
        "number_literal" => Some(LiteralValue::Number(text.to_string())),
        "boolean_literal" => Some(LiteralValue::Bool(text.eq_ignore_ascii_case("true"))),
        "null_literal" => Some(LiteralValue::Null),
        _ => None,
    }
}

/// `'…'` with `''` doubling, after `prefix_len` leading characters.
fn single_quoted(text: &str, prefix_len: usize) -> String {
    let inner = text
        .get(prefix_len..)
        .and_then(|t| t.strip_prefix('\''))
        .and_then(|t| t.strip_suffix('\''))
        .unwrap_or("");
    inner.replace("''", "'")
}

/// PostgreSQL `E'…'`: backslash escapes on top of `''` doubling.
fn escape_string(text: &str) -> String {
    let inner = text
        .get(1..)
        .and_then(|t| t.strip_prefix('\''))
        .and_then(|t| t.strip_suffix('\''))
        .unwrap_or("");
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('b') => out.push('\u{8}'),
                Some('f') => out.push('\u{c}'),
                // Numeric and unicode escapes are left as written: getting them
                // subtly wrong is worse than not claiming to have decoded them.
                Some(other) => out.push(other),
                None => {}
            },
            '\'' => {
                // `''` inside an E-string is still one quote.
                if chars.as_str().starts_with('\'') {
                    chars.next();
                }
                out.push('\'');
            }
            other => out.push(other),
        }
    }
    out
}

/// Oracle `q'X…Y'`. The scanner has already matched the delimiters, so here it
/// is only a matter of trimming `q'` + one char from the front and two from the
/// back.
fn q_string(text: &str) -> Option<String> {
    let inner = text.get(1..)?.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut it = inner.chars();
    it.next()?; // the opening delimiter
    let body = it.as_str();
    let closing = body.char_indices().next_back()?; // the closing delimiter
    Some(body.get(..closing.0)?.to_string())
}

/// Where the body of a `$tag$…$tag$` literal starts and ends, as byte offsets
/// into `text`.
///
/// Public because the walker needs the **offsets**, not the contents: a `$$ … $$`
/// holding a function body is re-parsed as SQL, and every position that comes
/// back has to be reported against the file on disk. Given the string, the caller
/// would have to work the delimiter length out a second time, and a second
/// implementation of "how long is the opening tag" is a second chance to be off
/// by one in the middle of somebody's source file.
///
/// The opener and the closer are the same length by construction, which is what
/// makes this a pair of offsets rather than a search.
pub fn dollar_body_span(text: &str) -> Option<(usize, usize)> {
    let after_first = text.strip_prefix('$')?;
    let tag_end = after_first.find('$')?;
    let opener_len = tag_end + 2; // `$` + tag + `$`
    let end = text.len().checked_sub(opener_len)?;
    if end < opener_len {
        return None;
    }
    Some((opener_len, end))
}

/// PostgreSQL `$tag$…$tag$`.
fn dollar_quoted(text: &str) -> Option<String> {
    let (start, end) = dollar_body_span(text)?;
    Some(text.get(start..end)?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(kind: &str, text: &str) -> String {
        match decode(kind, text) {
            Some(LiteralValue::String(v)) => v,
            other => panic!("expected a string, got {other:?}"),
        }
    }

    #[test]
    fn doubling_is_undone() {
        assert_eq!(s("string_literal", "'l''ora'"), "l'ora");
        assert_eq!(s("string_literal", "''"), "");
    }

    #[test]
    fn a_comment_marker_inside_a_string_is_just_text() {
        assert_eq!(s("string_literal", "'-- not a comment'"), "-- not a comment");
    }

    #[test]
    fn q_quoting_of_every_delimiter_family() {
        assert_eq!(s("q_string", "q'[it's here]'"), "it's here");
        assert_eq!(s("q_string", "q'{a}'"), "a");
        assert_eq!(s("q_string", "q'(a)'"), "a");
        assert_eq!(s("q_string", "q'<a>'"), "a");
        assert_eq!(s("q_string", "q'!a!'"), "a");
        assert_eq!(s("q_string", "Q'#x#'"), "x");
        assert_eq!(s("q_string", "q'##'"), "");
    }

    #[test]
    fn dollar_quoting_keeps_the_body_verbatim() {
        assert_eq!(s("dollar_quoted_string", "$$a; b$$"), "a; b");
        assert_eq!(s("dollar_quoted_string", "$fn$ BEGIN END; $fn$"), " BEGIN END; ");
        // A `$$` inside a tagged body is body text.
        assert_eq!(s("dollar_quoted_string", "$t$a$$b$t$"), "a$$b");
    }

    #[test]
    fn escape_strings_decode_the_escapes_they_claim_to() {
        assert_eq!(s("escape_string", "E'a\\nb'"), "a\nb");
        assert_eq!(s("escape_string", "E'a\\'b'"), "a'b");
    }

    #[test]
    fn numbers_and_keywords() {
        assert_eq!(decode("number_literal", "1.5e3"), Some(LiteralValue::Number("1.5e3".into())));
        assert_eq!(decode("boolean_literal", "TRUE"), Some(LiteralValue::Bool(true)));
        assert_eq!(decode("boolean_literal", "false"), Some(LiteralValue::Bool(false)));
        assert_eq!(decode("null_literal", "NULL"), Some(LiteralValue::Null));
        assert_eq!(decode("object_name", "SYSDATE"), None);
    }
}
