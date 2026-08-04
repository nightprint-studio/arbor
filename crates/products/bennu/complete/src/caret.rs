//! Line and token mechanics around a byte offset.
//!
//! Small functions, but every one of them is a place a provider can be subtly wrong in a way
//! that only shows up on somebody else's file: an offset landing mid-character, a line ending
//! in `\r\n`, a token scan that walks off the front of the buffer. Written once, here.
//!
//! **Offsets are UTF-8 byte offsets**, the same convention as the rest of the bennu contract.

/// The offset, clamped into the buffer and rejected when it is not a character boundary.
///
/// The first line of every provider. A caret offset arrives from the editor and has usually
/// been mapped from UTF-16, so "inside a character" is a real state rather than a paranoid
/// one — and slicing there panics.
pub fn safe_offset(source: &str, offset: usize) -> Option<usize> {
    let offset = offset.min(source.len());
    source.is_char_boundary(offset).then_some(offset)
}

/// Byte offset of the start of the line containing `offset`.
pub fn line_start(source: &str, offset: usize) -> usize {
    let offset = offset.min(source.len());
    source[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Byte offset of the end of the line containing `offset`, **excluding** the line terminator
/// (and excluding the `\r` of a `\r\n` pair, so a caller slicing to it never sees one).
pub fn line_end(source: &str, offset: usize) -> usize {
    let offset = offset.min(source.len());
    let end = source[offset..].find('\n').map(|i| offset + i).unwrap_or(source.len());
    if end > offset && source.as_bytes()[end - 1] == b'\r' {
        end - 1
    } else {
        end
    }
}

/// 1-based line number of `offset` — the form gutter marks and diagnostics report in.
pub fn line_number(source: &str, offset: usize) -> u32 {
    let offset = offset.min(source.len());
    source[..offset].bytes().filter(|&b| b == b'\n').count() as u32 + 1
}

/// The text from the start of the line up to the caret. What the caret has typed on this line.
pub fn line_prefix(source: &str, offset: usize) -> &str {
    let offset = offset.min(source.len());
    if !source.is_char_boundary(offset) {
        return "";
    }
    &source[line_start(source, offset)..offset]
}

/// The whole line containing `offset`, without its terminator.
pub fn line_at(source: &str, offset: usize) -> &str {
    &source[line_start(source, offset)..line_end(source, offset)]
}

/// How many bytes of leading whitespace the line at `offset` has.
///
/// Indentation is structure in yaml and is what generated code has to match everywhere else,
/// so it is asked for often enough to be worth not re-deriving.
pub fn indent_of(source: &str, offset: usize) -> usize {
    let line = line_at(source, offset);
    line.len() - line.trim_start().len()
}

/// The token immediately before the caret: where it starts, and its text.
///
/// Walks backwards while `part` accepts the character, so the provider decides what a token is
/// — `is_alphanumeric() || c == '.'` for a property key, `c != '"'` for an attribute value.
/// The start offset is the half a provider forgets to return and the popup needs: it is the
/// span the accepted candidate replaces.
///
/// Returns `(offset, "")` when the caret is not on a character boundary or nothing qualifies.
pub fn token_before(source: &str, offset: usize, part: impl Fn(char) -> bool) -> (usize, &str) {
    let Some(offset) = safe_offset(source, offset) else { return (offset.min(source.len()), "") };
    let mut start = offset;
    for (i, c) in source[..offset].char_indices().rev() {
        if !part(c) {
            break;
        }
        start = i;
    }
    (start, &source[start..offset])
}

/// The token immediately **after** the caret: where it ends, and its text.
///
/// The mirror of [`token_before`], and it exists for one rule. Ghost text is inserted *at* the
/// caret, so anything of the same token already written to the right of it gets duplicated
/// rather than completed: `</jav|a.version>` has exactly one continuation, and it is already on
/// screen. A provider passes the same `part` predicate it used for the prefix and hands the
/// answer to [`crate::prefix::ghost`].
///
/// Returns `(offset, "")` when the caret is not on a character boundary or nothing qualifies.
pub fn token_after(source: &str, offset: usize, part: impl Fn(char) -> bool) -> (usize, &str) {
    let Some(offset) = safe_offset(source, offset) else { return (offset.min(source.len()), "") };
    let mut end = offset;
    for (i, c) in source[offset..].char_indices() {
        if !part(c) {
            break;
        }
        end = offset + i + c.len_utf8();
    }
    (end, &source[offset..end])
}

/// Whether the caret sits inside `[start, end]` — inclusive at both ends.
///
/// Inclusive on purpose: a caret at the very end of a key is still *on* that key as far as the
/// user is concerned, and an exclusive test is why hover sometimes does nothing at the last
/// character of the word you are pointing at.
pub fn within(offset: usize, start: usize, end: usize) -> bool {
    offset >= start && offset <= end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_offset_inside_a_character_is_refused_rather_than_sliced() {
        let s = "à = 1"; // 'à' is two bytes
        assert_eq!(safe_offset(s, 1), None);
        assert_eq!(safe_offset(s, 2), Some(2));
        assert_eq!(safe_offset(s, 999), Some(s.len()), "past the end clamps to the end");
        assert_eq!(line_prefix(s, 1), "", "and the slicing helpers stay quiet, not panic");
    }

    #[test]
    fn a_line_ends_before_its_terminator_whichever_one_it_is() {
        let unix = "a: 1\nb: 2\n";
        assert_eq!(line_at(unix, 6), "b: 2");
        let dos = "a: 1\r\nb: 2\r\n";
        assert_eq!(line_at(dos, 7), "b: 2", "the \\r is not part of the line");
        assert_eq!(line_at(dos, 0), "a: 1");
    }

    #[test]
    fn lines_are_numbered_from_one() {
        let s = "a\nb\nc";
        assert_eq!(line_number(s, 0), 1);
        assert_eq!(line_number(s, 2), 2);
        assert_eq!(line_number(s, s.len()), 3);
    }

    #[test]
    fn the_token_before_the_caret_reports_the_span_it_would_replace() {
        let s = "  spring.datasource.ur";
        let (start, tok) = token_before(s, s.len(), |c| c.is_alphanumeric() || c == '.' || c == '-');
        assert_eq!(tok, "spring.datasource.ur");
        assert_eq!(start, 2, "the indentation is not part of the token");

        // Nothing qualifying at the caret is an empty token at the caret, not a walk-off.
        let (start, tok) = token_before(s, 1, |c| c.is_alphanumeric());
        assert_eq!((start, tok), (1, ""));
        assert_eq!(token_before("", 0, |_| true), (0, ""));
    }

    #[test]
    fn the_token_after_the_caret_is_what_ghost_text_would_duplicate() {
        let s = "</java.version>";
        // Caret after `</jav`.
        let (end, tok) = token_after(s, 5, |c| c.is_alphanumeric() || c == '.');
        assert_eq!(tok, "a.version");
        assert_eq!(end, 14, "stops at the `>`, which cannot be part of the name");

        // Nothing qualifying at the caret is an empty token at the caret.
        assert_eq!(token_after(s, 14, |c| c.is_alphanumeric()), (14, ""));
        assert_eq!(token_after(s, s.len(), |_| true), (s.len(), ""));
        assert_eq!(token_after("", 0, |_| true), (0, ""));
    }

    #[test]
    fn the_token_after_the_caret_never_splits_a_character() {
        let s = "caffè";
        let (end, tok) = token_after(s, 2, |c| c.is_alphabetic());
        assert_eq!((end, tok), (s.len(), "ffè"));
    }

    #[test]
    fn indentation_is_measured_on_the_line_the_caret_is_on() {
        let s = "a:\n    b: 1\n";
        assert_eq!(indent_of(s, 0), 0);
        assert_eq!(indent_of(s, 8), 4);
    }

    #[test]
    fn a_caret_at_the_end_of_a_word_is_still_on_it() {
        assert!(within(4, 0, 4), "inclusive — pointing at the last character must answer");
        assert!(!within(5, 0, 4));
    }
}
