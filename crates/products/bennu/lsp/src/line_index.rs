//! LSP `{line, character}` positions ↔ Bennu's UTF-8 byte offsets.
//!
//! The single most load-bearing translation in the client, and the one that fails
//! quietly: get it wrong and go-to lands one column off on ASCII and several columns off
//! the moment a line contains a `→` or an emoji — a bug that looks like "the server is
//! confused" rather than like an off-by-one here.
//!
//! Two coordinate systems meet:
//!
//! * **LSP** counts a `line` (0-based) and a `character` **within that line**, in units
//!   of the negotiated [`PositionEncoding`]. The protocol's default is **UTF-16 code
//!   units** — a historical debt from VS Code's JavaScript strings — so `character` is
//!   not a character count and not a byte count. LSP 3.17 lets a client ask for
//!   `utf-8`; whether it gets it is the server's call, so both are supported and the
//!   negotiated answer is carried in the encoding field rather than assumed.
//! * **Bennu** uses a flat UTF-8 **byte offset** from the start of the file, which is
//!   what every other span on its wire is (`Diagnostic::start`, `UsageHit::start`,
//!   `RenameEdit::start`).
//!
//! Line terminators: `\n`, `\r\n` and a lone `\r` all end a line (the LSP spec's own
//! list). A `\r\n` is one terminator, not two empty lines — the mistake that shifts
//! every line number in a CRLF file by its line count.
//!
//! Everything clamps rather than failing. A server is free to send a position past the
//! end of a line (`character: 4294967295` is a known idiom for "end of line") or past
//! the end of the file when its copy of the document is one keystroke behind ours;
//! answering with the nearest valid offset degrades to a jump a line off, whereas
//! returning an error degrades to a feature that intermittently does nothing.

use serde::{Deserialize, Serialize};

/// How a server counts the `character` field of a position.
///
/// Serialized in the spec's own spelling so it can be sent in
/// `ClientCapabilities.general.positionEncodings` and read back out of
/// `ServerCapabilities.positionEncoding` without a mapping table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionEncoding {
    /// UTF-16 code units — the protocol default, and what a server that says nothing means.
    #[default]
    #[serde(rename = "utf-16")]
    Utf16,
    /// UTF-8 bytes: what Bennu uses internally, so negotiating this makes the conversion
    /// a pure line-start lookup.
    #[serde(rename = "utf-8")]
    Utf8,
    /// Unicode scalar values. Rare, but in the spec and cheap to support.
    #[serde(rename = "utf-32")]
    Utf32,
}

impl PositionEncoding {
    /// The width of `c` in this encoding's units.
    fn width(self, c: char) -> usize {
        match self {
            PositionEncoding::Utf8 => c.len_utf8(),
            PositionEncoding::Utf16 => c.len_utf16(),
            PositionEncoding::Utf32 => 1,
        }
    }
}

/// A position in a text document, as LSP states it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    /// 0-based line.
    pub line: u32,
    /// 0-based offset within the line, in [`PositionEncoding`] units.
    pub character: u32,
}

impl Position {
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// A half-open range of [`Position`]s.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }
}

/// Line-start offsets over one document's text, for converting positions both ways.
///
/// Borrows the text rather than owning it: the caller always has the buffer already
/// (it came in with the request, or was just read off disk to resolve a cross-file
/// target), and an index that copied it would double the memory of every
/// find-usages sweep for nothing.
#[derive(Debug, Clone)]
pub struct LineIndex<'a> {
    text: &'a str,
    /// Byte offset of the first character of each line. Always starts with `0`, so
    /// `len()` is the line count and an empty document still has one line.
    line_starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    /// Index `text`.
    pub fn new(text: &'a str) -> Self {
        let bytes = text.as_bytes();
        let mut line_starts = Vec::with_capacity(bytes.len() / 32 + 1);
        line_starts.push(0);
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => {
                    line_starts.push(i + 1);
                    i += 1;
                }
                b'\r' => {
                    // `\r\n` is ONE terminator. Counting it twice inserts a phantom empty
                    // line per line, which shifts every line number in a CRLF file.
                    let skip = if bytes.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
                    line_starts.push(i + skip);
                    i += skip;
                }
                _ => i += 1,
            }
        }
        Self { text, line_starts }
    }

    /// The number of lines. Never zero — an empty document is one empty line.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// The document's length in bytes.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The byte range of `line` **excluding** its terminator, or `None` past the end.
    pub fn line_range(&self, line: usize) -> Option<(usize, usize)> {
        let start = *self.line_starts.get(line)?;
        let end = match self.line_starts.get(line + 1) {
            // The next line starts after this one's terminator; walk back over it.
            Some(&next) => {
                let mut e = next;
                let bytes = self.text.as_bytes();
                if e > start && bytes[e - 1] == b'\n' {
                    e -= 1;
                }
                if e > start && bytes[e - 1] == b'\r' {
                    e -= 1;
                }
                e
            }
            None => self.text.len(),
        };
        Some((start, end))
    }

    /// `line`'s text without its terminator, or `""` past the end.
    pub fn line_text(&self, line: usize) -> &'a str {
        match self.line_range(line) {
            Some((s, e)) => &self.text[s..e],
            None => "",
        }
    }

    /// Convert an LSP position to a byte offset, clamping into the document.
    ///
    /// Clamping is layered, because the two ways a position can be out of range mean
    /// different things: a line past the end means the server's document copy is longer
    /// than ours (→ end of file), whereas a character past the line end is routinely
    /// deliberate (→ end of that line).
    pub fn offset_at(&self, pos: Position, encoding: PositionEncoding) -> usize {
        let line = pos.line as usize;
        let Some((start, end)) = self.line_range(line) else {
            return self.text.len();
        };
        if encoding == PositionEncoding::Utf8 {
            // Byte units already — but still snap to a char boundary, since a server
            // that miscounts must not produce an offset that would panic on slicing.
            let raw = start.saturating_add(pos.character as usize).min(end);
            return self.floor_char_boundary(raw, start);
        }
        let mut units = 0usize;
        let target = pos.character as usize;
        for (rel, c) in self.text[start..end].char_indices() {
            // Snap **down**: the first character whose end passes the target is the one the target
            // falls in, and its start is the answer. This is what makes a position landing inside a
            // surrogate pair (undefined per spec, and sent in practice) resolve to the start of that
            // character rather than past it. Clamping the other way can turn a non-empty range into
            // an empty one, which is silently dropped downstream.
            if units + encoding.width(c) > target {
                return start + rel;
            }
            units += encoding.width(c);
        }
        // Ran out of line: the character was at or past its end.
        end
    }

    /// Convert a byte `offset` to an LSP position, clamping into the document.
    pub fn position_at(&self, offset: usize, encoding: PositionEncoding) -> Position {
        let offset = offset.min(self.text.len());
        let line = self.line_of(offset);
        let (start, end) = self.line_range(line).unwrap_or((offset, offset));
        let column_end = offset.clamp(start, end);
        let character = if encoding == PositionEncoding::Utf8 {
            column_end - start
        } else {
            self.text[start..column_end].chars().map(|c| encoding.width(c)).sum()
        };
        Position { line: line as u32, character: character as u32 }
    }

    /// Convert an LSP range to a byte range, in document order (a server that reports
    /// `end` before `start` — some do, on a synthetic edit — must not produce an
    /// inverted span the caller would slice with).
    pub fn byte_range(&self, range: Range, encoding: PositionEncoding) -> (usize, usize) {
        let a = self.offset_at(range.start, encoding);
        let b = self.offset_at(range.end, encoding);
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }
    /// The offset of the **end of the content** of a 0-based line — before its terminator.
    ///
    /// What a fold starts and ends at: folding from the end of the header line leaves the line that
    /// names the region on screen, which is the whole point of a fold. `None` for a line past the
    /// end of the file, which a server can legitimately name for a range that reaches the last line.
    pub fn line_end_offset(&self, line: u32) -> Option<usize> {
        let (_, end) = self.line_range(line as usize)?;
        Some(end)
    }


    /// The 0-based line containing `offset`.
    pub fn line_of(&self, offset: usize) -> usize {
        // `line_starts` is sorted; `partition_point` gives the count of starts at or
        // before `offset`, so minus one is the line index.
        self.line_starts.partition_point(|&s| s <= offset).saturating_sub(1)
    }

    /// `(1-based line, 1-based column in UTF-16 code units)` for `offset` — the shape
    /// Bennu's wire and CodeMirror both want: `UsageHit`/`DeclarationTarget` carry
    /// 1-based line/col, and the editor resolves a column against a CodeMirror line,
    /// whose coordinates are UTF-16.
    pub fn line_col_utf16(&self, offset: usize) -> (usize, usize) {
        let p = self.position_at(offset, PositionEncoding::Utf16);
        (p.line as usize + 1, p.character as usize + 1)
    }

    /// Round `offset` down to a char boundary, never below `floor`.
    fn floor_char_boundary(&self, offset: usize, floor: usize) -> usize {
        let mut o = offset.min(self.text.len());
        while o > floor && !self.text.is_char_boundary(o) {
            o -= 1;
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UTF16: PositionEncoding = PositionEncoding::Utf16;
    const UTF8: PositionEncoding = PositionEncoding::Utf8;
    const UTF32: PositionEncoding = PositionEncoding::Utf32;

    #[test]
    fn an_empty_document_is_one_line() {
        let idx = LineIndex::new("");
        assert_eq!(idx.line_count(), 1);
        assert_eq!(idx.offset_at(Position::new(0, 0), UTF16), 0);
        assert_eq!(idx.position_at(0, UTF16), Position::new(0, 0));
    }

    #[test]
    fn lines_are_split_on_lf() {
        let idx = LineIndex::new("a\nbb\nccc");
        assert_eq!(idx.line_count(), 3);
        assert_eq!(idx.line_text(0), "a");
        assert_eq!(idx.line_text(1), "bb");
        assert_eq!(idx.line_text(2), "ccc");
        assert_eq!(idx.offset_at(Position::new(1, 1), UTF16), 3);
    }

    #[test]
    fn crlf_is_one_terminator_not_two_lines() {
        // The bug this guards: counting `\r` and `\n` separately inserts a phantom empty
        // line per line, shifting every line number in a CRLF file.
        let idx = LineIndex::new("a\r\nb\r\nc");
        assert_eq!(idx.line_count(), 3, "three lines, not five");
        assert_eq!(idx.line_text(1), "b", "the terminator is not part of the line");
        assert_eq!(idx.offset_at(Position::new(2, 0), UTF16), 6);
        assert_eq!(idx.position_at(6, UTF16), Position::new(2, 0));
    }

    #[test]
    fn a_lone_cr_also_ends_a_line() {
        let idx = LineIndex::new("a\rb");
        assert_eq!(idx.line_count(), 2);
        assert_eq!(idx.line_text(0), "a");
        assert_eq!(idx.line_text(1), "b");
    }

    #[test]
    fn a_trailing_newline_opens_a_final_empty_line() {
        // What an editor shows: a file ending in `\n` has a last, empty line the caret
        // can sit on, and a server will report a position on it.
        let idx = LineIndex::new("a\n");
        assert_eq!(idx.line_count(), 2);
        assert_eq!(idx.line_text(1), "");
        assert_eq!(idx.offset_at(Position::new(1, 0), UTF16), 2);
    }

    #[test]
    fn utf16_columns_count_code_units_not_bytes() {
        // `città` — `à` is 2 bytes, 1 UTF-16 unit. A byte-counting client lands one column
        // past every accent on the line.
        let text = "let x = città;";
        let idx = LineIndex::new(text);
        let semicolon_byte = text.find(';').unwrap();
        // 5 ASCII in "città" would be 5 units; the accent costs 1 unit but 2 bytes.
        assert_eq!(semicolon_byte, 14, "bytes");
        assert_eq!(idx.position_at(semicolon_byte, UTF16).character, 13, "UTF-16 units");
        assert_eq!(idx.position_at(semicolon_byte, UTF8).character, 14, "bytes");
        assert_eq!(idx.position_at(semicolon_byte, UTF32).character, 13, "scalars");
        // …and back.
        assert_eq!(idx.offset_at(Position::new(0, 13), UTF16), semicolon_byte);
    }

    #[test]
    fn an_astral_char_is_two_utf16_units_and_one_scalar() {
        // The case that separates all three encodings: 😀 is 4 bytes, 2 UTF-16 code units
        // (a surrogate pair) and 1 scalar value.
        let text = "a😀b";
        let idx = LineIndex::new(text);
        let b_byte = text.find('b').unwrap();
        assert_eq!(b_byte, 5);
        assert_eq!(idx.position_at(b_byte, UTF8).character, 5);
        assert_eq!(idx.position_at(b_byte, UTF16).character, 3);
        assert_eq!(idx.position_at(b_byte, UTF32).character, 2);
        assert_eq!(idx.offset_at(Position::new(0, 3), UTF16), b_byte);
        assert_eq!(idx.offset_at(Position::new(0, 2), UTF32), b_byte);
    }

    #[test]
    fn a_position_inside_a_surrogate_pair_snaps_to_the_char() {
        // Undefined per spec; snapping to the char start is the only answer that cannot
        // produce a byte offset that panics when sliced.
        let idx = LineIndex::new("a😀b");
        let inside = idx.offset_at(Position::new(0, 2), UTF16);
        assert_eq!(inside, 1, "the start of the emoji, not the middle of it");
        assert!("a😀b".is_char_boundary(inside));
    }

    #[test]
    fn a_character_past_the_line_end_clamps_to_the_line_end() {
        // `character: u32::MAX` is a known idiom for "end of line" and must not run into
        // the next line.
        let idx = LineIndex::new("ab\ncd");
        assert_eq!(idx.offset_at(Position::new(0, u32::MAX), UTF16), 2, "not 3, not 5");
        assert_eq!(idx.offset_at(Position::new(0, 99), UTF8), 2);
    }

    #[test]
    fn a_line_past_the_end_clamps_to_the_document_end() {
        let idx = LineIndex::new("ab\ncd");
        assert_eq!(idx.offset_at(Position::new(9, 0), UTF16), 5);
    }

    #[test]
    fn a_utf8_column_still_snaps_to_a_char_boundary() {
        // A server that negotiated utf-8 and then miscounted must not hand us an offset
        // that would panic on a slice.
        let idx = LineIndex::new("à");
        assert_eq!(idx.offset_at(Position::new(0, 1), UTF8), 0, "mid-char → back to 0");
    }

    #[test]
    fn an_inverted_range_comes_back_in_document_order() {
        let idx = LineIndex::new("abcdef");
        let r = Range::new(Position::new(0, 4), Position::new(0, 1));
        assert_eq!(idx.byte_range(r, UTF16), (1, 4));
    }

    #[test]
    fn line_of_is_exact_at_the_boundaries() {
        let idx = LineIndex::new("aa\nbb\ncc");
        assert_eq!(idx.line_of(0), 0);
        assert_eq!(idx.line_of(2), 0, "on the terminator, still line 0");
        assert_eq!(idx.line_of(3), 1, "the first byte of line 1");
        assert_eq!(idx.line_of(8), 2);
        assert_eq!(idx.line_of(999), 2, "past the end clamps");
    }

    #[test]
    fn line_col_is_one_based_for_the_wire() {
        let idx = LineIndex::new("aa\nbb");
        assert_eq!(idx.line_col_utf16(0), (1, 1), "the very first byte is 1:1");
        assert_eq!(idx.line_col_utf16(4), (2, 2));
    }

    #[test]
    fn every_addressable_offset_round_trips_through_every_encoding() {
        // The property that matters: for any offset an LSP position can name, position_at ∘
        // offset_at is the identity. Runs over a text with CRLF, accents and an astral char.
        //
        // "addressable" excludes offsets INSIDE a line terminator — the second byte of a `\r\n`
        // is a char boundary but no `{line, character}` pair denotes it, so requiring it to
        // round-trip would be asserting something the coordinate system cannot express.
        let text = "fn main() {\r\n    let città = \"😀\";\n}\n";
        let idx = LineIndex::new(text);
        let mut checked = 0;
        for enc in [UTF8, UTF16, UTF32] {
            for (offset, _) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
                let (line_start, line_end) =
                    idx.line_range(idx.line_of(offset)).expect("every offset has a line");
                if offset < line_start || offset > line_end {
                    continue; // inside a terminator
                }
                let pos = idx.position_at(offset, enc);
                assert_eq!(
                    idx.offset_at(pos, enc),
                    offset,
                    "{enc:?} round-trip at byte {offset} (pos {pos:?})"
                );
                checked += 1;
            }
        }
        // Guard against the filter above quietly excluding everything.
        assert!(checked > 90, "expected to cover most of the text, checked {checked}");
    }

    #[test]
    fn the_encoding_serializes_in_the_spec_spelling() {
        // It goes out in `positionEncodings` and comes back in `positionEncoding`; a
        // mismatch here would silently leave every position in the wrong units.
        assert_eq!(serde_json::to_string(&PositionEncoding::Utf16).unwrap(), r#""utf-16""#);
        assert_eq!(serde_json::to_string(&PositionEncoding::Utf8).unwrap(), r#""utf-8""#);
        let back: PositionEncoding = serde_json::from_str(r#""utf-8""#).unwrap();
        assert_eq!(back, PositionEncoding::Utf8);
        assert_eq!(PositionEncoding::default(), PositionEncoding::Utf16, "the protocol default");
    }
}
