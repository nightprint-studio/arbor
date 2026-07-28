//! "Can this text be written in this encoding?" — and if not, *which*
//! character stops it.
//!
//! `encoding_rs`' one-shot `encode` is lossy on purpose: an unmappable
//! character becomes an HTML numeric reference, so a save that should have
//! failed instead writes plausible-looking garbage into a script that will be
//! run against a production database. Everything here refuses that trade.

use encoding_rs::Encoding;

/// The first character of a string that the destination encoding cannot
/// represent, located well enough to put a caret on it in the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnrepresentableChar {
    pub ch: char,
    /// Canonical name of the encoding that rejected it (`"windows-1252"`).
    pub encoding: &'static str,
    /// 0-based index in `chars()` — not bytes, so it addresses the same unit
    /// the character is.
    pub char_index: usize,
    /// 1-based line, counting `\n`.
    pub line: usize,
    /// 1-based column in characters.
    pub column: usize,
}

impl std::fmt::Display for UnrepresentableChar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "character '{}' (U+{:04X}) at line {}, column {} cannot be represented in {}",
            self.ch, self.ch as u32, self.line, self.column, self.encoding
        )
    }
}

impl std::error::Error for UnrepresentableChar {}

/// `Ok(())` when every character of `content` survives a trip through
/// `encoding`, `Err` naming the first one that does not.
///
/// Unicode encodings always succeed: a Rust `str` is by construction valid
/// Unicode with no lone surrogates.
pub fn check_representable(
    content: &str,
    encoding: &'static Encoding,
) -> Result<(), UnrepresentableChar> {
    if is_unicode(encoding) {
        return Ok(());
    }
    // Fast path: one bulk encode answers "is there a problem at all" without
    // allocating per character. Only when the answer is yes do we pay for the
    // scan that locates it — the failing path is the rare one.
    let (_, _, had_errors) = encoding.encode(content);
    if !had_errors {
        return Ok(());
    }
    Err(locate_offender(content, encoding))
}

/// Encode for disk, failing instead of substituting.
///
/// Also the only correct way to write UTF-16 in this crate: `encoding_rs` has
/// no UTF-16 encoder and quietly redirects `Encoding::encode` to UTF-8 for
/// those labels, so UTF-16 is encoded by hand here.
pub fn encode_strict(
    content: &str,
    encoding: &'static Encoding,
) -> Result<Vec<u8>, UnrepresentableChar> {
    check_representable(content, encoding)?;

    if encoding == encoding_rs::UTF_16LE || encoding == encoding_rs::UTF_16BE {
        let little = encoding == encoding_rs::UTF_16LE;
        let mut out = Vec::with_capacity(content.len() * 2);
        for unit in content.encode_utf16() {
            let bytes = if little { unit.to_le_bytes() } else { unit.to_be_bytes() };
            out.extend_from_slice(&bytes);
        }
        return Ok(out);
    }

    let (cow, _, _) = encoding.encode(content);
    Ok(cow.into_owned())
}

/// Strict counterpart of `encode_for_disk_with_bom`: same label-resolution and
/// BOM behaviour, but it refuses to corrupt and it can actually write UTF-16.
pub fn encode_for_disk_strict(
    content:     &str,
    encoding:    Option<&str>,
    prepend_bom: bool,
) -> Result<Vec<u8>, UnrepresentableChar> {
    let enc = encoding
        .and_then(|label| Encoding::for_label(label.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);

    let body = encode_strict(content, enc)?;
    let Some(bom) = super::bom_for(enc).filter(|_| prepend_bom) else {
        return Ok(body);
    };

    let mut out = Vec::with_capacity(bom.len() + body.len());
    out.extend_from_slice(bom);
    out.extend_from_slice(&body);
    Ok(out)
}

/// UTF-8 / UTF-16 can encode any `str`, so representability never fails.
fn is_unicode(encoding: &'static Encoding) -> bool {
    encoding == encoding_rs::UTF_8
        || encoding == encoding_rs::UTF_16LE
        || encoding == encoding_rs::UTF_16BE
}

/// Walk the string once to find the first character the encoding rejects,
/// tracking line/column as we go. Only ever called on the failing path.
fn locate_offender(content: &str, encoding: &'static Encoding) -> UnrepresentableChar {
    let mut line = 1usize;
    let mut column = 1usize;
    let mut buf = [0u8; 4];

    for (char_index, ch) in content.chars().enumerate() {
        let (_, _, had_errors) = encoding.encode(ch.encode_utf8(&mut buf));
        if had_errors {
            return UnrepresentableChar {
                ch,
                encoding: encoding.name(),
                char_index,
                line,
                column,
            };
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    // Unreachable in practice: the bulk encode already reported an error, so
    // some character must fail individually. Report the end of the text rather
    // than panicking on an `encoding_rs` behaviour change.
    UnrepresentableChar {
        ch: '\u{FFFD}',
        encoding: encoding.name(),
        char_index: content.chars().count(),
        line,
        column,
    }
}
