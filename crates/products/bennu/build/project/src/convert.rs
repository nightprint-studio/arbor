//! Encoding **conversion**: rewrite a file's bytes from one encoding to another.
//!
//! This is the write-side sibling of [`crate::encoding`], and deliberately separate from it. The
//! decode/encode pair there serves the editor's round trip (read a file, show it, save it back in
//! the *same* encoding) and its fallback — "a character the encoding cannot hold turns the whole
//! save into UTF-8" — is exactly wrong for a conversion, where the person chose the target and
//! must be told, before anything is written, which characters do not fit.
//!
//! Everything here is pure: bytes in, bytes out, no filesystem, no config. The backend drives it
//! file by file (plan → apply) and owns the I/O, the backups and the Jobs panel.
//!
//! ## Rules the conversion keeps
//!
//! * **Line endings are never touched.** Text is decoded and re-encoded as-is; `\r\n` stays
//!   `\r\n`, a mixed file stays mixed.
//! * **The BOM is explicit.** `encoding_rs` strips and sniffs BOMs on its own, which is the wrong
//!   default here (it would silently drop one, or switch the source codec). BOM handling is done
//!   by hand and governed by [`BomPolicy`].
//! * **A malformed source is refused**, not converted. Decoding lossily would bake `U+FFFD` into
//!   the output — a conversion that destroys the bytes it was asked to preserve.
//! * **ISO-8859-1 is real Latin-1.** The WHATWG label maps it to windows-1252 (which puts `€` at
//!   `0x80`); a file declared Latin-1 must not gain Cp1252 characters, so it is handled here as
//!   the identity map on U+0000–U+00FF.
//! * **Binary files are refused**, by a NUL sniff (waived for UTF-16, which is full of NULs).

use encoding_rs::Encoding;

/// How many unrepresentable characters are quoted back for one file. The total is always exact.
pub const MAX_SAMPLES: usize = 8;

/// How much of a file is sniffed for NUL bytes.
const SNIFF_BYTES: usize = 8192;

/// How far into a file an encoding declaration (XML prolog, JSP directive) is looked for.
const DECLARATION_WINDOW: usize = 2048;

/// A text encoding this module can read and write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Utf8,
    Utf16Le,
    Utf16Be,
    /// ISO-8859-1 proper: U+0000–U+00FF ↔ `0x00`–`0xFF`.
    Latin1,
    /// Any other single/multi-byte legacy encoding `encoding_rs` can both decode and encode.
    Legacy(&'static Encoding),
}

impl Codec {
    /// Resolve a label — `UTF-8`, `Cp1252`, `windows-1252`, `latin1`, `UTF-16LE`, `ISO-8859-15`, … —
    /// or `None` for one this module cannot write (unknown, `replacement`, stateful ISO-2022-JP).
    pub fn parse(label: &str) -> Option<Codec> {
        let norm: String = label
            .trim()
            .chars()
            .filter(|c| !matches!(c, '-' | '_' | ' '))
            .collect::<String>()
            .to_ascii_lowercase();
        match norm.as_str() {
            "utf8" | "unicode11utf8" => return Some(Codec::Utf8),
            "utf16le" | "utf16" => return Some(Codec::Utf16Le),
            "utf16be" => return Some(Codec::Utf16Be),
            "iso88591" | "latin1" | "l1" | "isolatin1" | "8859_1" | "88591" => return Some(Codec::Latin1),
            // Java's names for the Windows code pages and a few others `encoding_rs` does not know.
            "cp1252" => return Some(Codec::Legacy(encoding_rs::WINDOWS_1252)),
            "xmacroman" | "macroman" => return Some(Codec::Legacy(encoding_rs::MACINTOSH)),
            _ => {}
        }
        let enc = Encoding::for_label(label.trim().as_bytes())?;
        let name = enc.name();
        // UTF-16 is handled above; the rest of this set cannot be written (their encoder is UTF-8).
        if enc.output_encoding() != enc || name == "ISO-2022-JP" || name == "x-user-defined" {
            return None;
        }
        if enc == encoding_rs::UTF_8 {
            return Some(Codec::Utf8);
        }
        Some(Codec::Legacy(enc))
    }

    /// The canonical label this module reports and the frontend picker uses.
    pub fn name(self) -> &'static str {
        match self {
            Codec::Utf8 => "UTF-8",
            Codec::Utf16Le => "UTF-16LE",
            Codec::Utf16Be => "UTF-16BE",
            Codec::Latin1 => "ISO-8859-1",
            Codec::Legacy(e) => e.name(),
        }
    }

    /// The spelling a Java build file uses (`project.build.sourceEncoding`, `-encoding`).
    pub fn java_name(self) -> &'static str {
        match self {
            Codec::Legacy(e) if e == encoding_rs::MACINTOSH => "x-MacRoman",
            other => other.name(),
        }
    }

    /// The spelling an in-file declaration uses (`<?xml encoding="…"?>`, `pageEncoding`).
    pub fn declaration_name(self) -> &'static str {
        match self {
            Codec::Utf16Le | Codec::Utf16Be => "UTF-16",
            other => other.name(),
        }
    }

    /// Whether the encoding has a byte-order mark to speak of.
    pub fn supports_bom(self) -> bool {
        matches!(self, Codec::Utf8 | Codec::Utf16Le | Codec::Utf16Be)
    }

    fn bom_bytes(self) -> &'static [u8] {
        match self {
            Codec::Utf8 => &[0xEF, 0xBB, 0xBF],
            Codec::Utf16Le => &[0xFF, 0xFE],
            Codec::Utf16Be => &[0xFE, 0xFF],
            _ => &[],
        }
    }

    fn is_utf16(self) -> bool {
        matches!(self, Codec::Utf16Le | Codec::Utf16Be)
    }
}

/// What to do with the byte-order mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BomPolicy {
    /// The output has a BOM exactly when the input had one (and the target can carry one).
    Keep,
    /// Always write one (UTF-8 / UTF-16 targets; ignored for encodings that have none).
    Add,
    /// Never write one.
    Remove,
}

impl BomPolicy {
    pub fn parse(s: &str) -> Option<BomPolicy> {
        match s {
            "keep" => Some(BomPolicy::Keep),
            "add" => Some(BomPolicy::Add),
            "remove" => Some(BomPolicy::Remove),
            _ => None,
        }
    }
}

/// What to do with a character the target encoding cannot hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    /// Refuse the file: nothing is written for it.
    Abort,
    /// Write `?` in its place.
    Replace,
    /// Write an escape the file's language reads back as the same character (see [`EscapeStyle`]).
    Escape,
}

impl Fallback {
    pub fn parse(s: &str) -> Option<Fallback> {
        match s {
            "abort" => Some(Fallback::Abort),
            "replace" => Some(Fallback::Replace),
            "escape" => Some(Fallback::Escape),
            _ => None,
        }
    }
}

/// The escape syntax [`Fallback::Escape`] uses, which depends on what the file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeStyle {
    /// `€` — Java sources, `.properties`, Groovy, Kotlin, Scala.
    Java,
    /// `&#x20AC;` — XML, HTML, JSP and friends.
    Xml,
    /// The file has no escape syntax: the escape falls back to `?`.
    None,
}

impl EscapeStyle {
    /// The style for a file, from its extension.
    pub fn for_extension(ext: &str) -> EscapeStyle {
        match ext.to_ascii_lowercase().as_str() {
            "java" | "properties" | "groovy" | "kt" | "kts" | "scala" | "js" | "json" => EscapeStyle::Java,
            "xml" | "html" | "htm" | "xhtml" | "jsp" | "jspx" | "jspf" | "tld" | "xsl" | "xslt" | "svg"
            | "fxml" | "xsd" | "wsdl" | "tag" | "tagx" => EscapeStyle::Xml,
            _ => EscapeStyle::None,
        }
    }
}

/// Everything one conversion is told.
#[derive(Debug, Clone, Copy)]
pub struct ConvertOptions {
    pub from: Codec,
    pub to: Codec,
    pub bom: BomPolicy,
    pub fallback: Fallback,
    pub escape: EscapeStyle,
    /// Rewrite the encoding the file declares about itself (XML prolog, JSP directive) to `to`.
    pub rewrite_declaration: bool,
}

/// One character the target encoding cannot represent, and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unrepresentable {
    /// 1-based line.
    pub line: u32,
    /// 1-based column, in characters.
    pub column: u32,
    pub ch: char,
}

/// The result of converting one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Converted {
    /// The file's new bytes.
    pub bytes: Vec<u8>,
    /// The first [`MAX_SAMPLES`] characters that did not fit (and were replaced / escaped).
    pub samples: Vec<Unrepresentable>,
    /// How many did not fit, in all.
    pub unrepresentable_total: usize,
    pub bom_before: bool,
    pub bom_after: bool,
    /// The encoding the file declared about itself, when it did and it was not already `to`.
    pub declaration_before: Option<String>,
    /// Whether that declaration was rewritten.
    pub declaration_rewritten: bool,
}

/// Why a file was not converted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertError {
    /// NUL bytes in something that is not UTF-16: not text.
    Binary,
    /// The bytes are not valid in the source encoding. `line` is 1-based, UTF-8 sources only.
    InvalidSource { line: Option<u32> },
    /// [`Fallback::Abort`] and the target cannot hold `total` characters of the file.
    Unrepresentable { total: usize, samples: Vec<Unrepresentable> },
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConvertError::Binary => write!(f, "binary file"),
            ConvertError::InvalidSource { line: Some(l) } => {
                write!(f, "not valid in the source encoding (first bad byte on line {l})")
            }
            ConvertError::InvalidSource { line: None } => write!(f, "not valid in the source encoding"),
            ConvertError::Unrepresentable { total, .. } => {
                write!(f, "{total} character{} the target encoding cannot hold", if *total == 1 { "" } else { "s" })
            }
        }
    }
}

/// A BOM at the start of `bytes`, and its length.
pub fn detect_bom(bytes: &[u8]) -> Option<(Codec, usize)> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Some((Codec::Utf8, 3))
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        Some((Codec::Utf16Le, 2))
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        Some((Codec::Utf16Be, 2))
    } else {
        None
    }
}

/// What [`detect`] concluded and how sure it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detected {
    pub codec: Codec,
    pub basis: DetectBasis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectBasis {
    /// A byte-order mark said so.
    Bom,
    /// Pure ASCII: every encoding here reads it the same way. Reported as UTF-8.
    Ascii,
    /// Non-ASCII bytes that form valid UTF-8 — a legacy encoding all but never does by accident.
    ValidUtf8,
    /// Non-ASCII bytes that are not valid UTF-8: a legacy single-byte encoding, guessed as Cp1252.
    LegacyGuess,
}

/// Best guess at the encoding of `bytes`. A hint for the preview, never an authority: the file's
/// own resolved encoding (override → pom → default) is what the editor uses.
pub fn detect(bytes: &[u8]) -> Detected {
    if let Some((codec, _)) = detect_bom(bytes) {
        return Detected { codec, basis: DetectBasis::Bom };
    }
    if bytes.is_ascii() {
        return Detected { codec: Codec::Utf8, basis: DetectBasis::Ascii };
    }
    if std::str::from_utf8(bytes).is_ok() {
        return Detected { codec: Codec::Utf8, basis: DetectBasis::ValidUtf8 };
    }
    Detected { codec: Codec::Legacy(encoding_rs::WINDOWS_1252), basis: DetectBasis::LegacyGuess }
}

/// Whether `bytes` look like a binary file when read as `from`.
pub fn looks_binary(bytes: &[u8], from: Codec) -> bool {
    !from.is_utf16() && bytes[..bytes.len().min(SNIFF_BYTES)].contains(&0)
}

/// Decode `bytes` in `codec` with no BOM handling and no replacement. `Err` carries the byte offset
/// of the first malformed sequence when it is known (UTF-8 only).
fn decode_strict(bytes: &[u8], codec: Codec) -> Result<String, Option<usize>> {
    match codec {
        Codec::Latin1 => Ok(bytes.iter().map(|&b| b as char).collect()),
        Codec::Utf8 => std::str::from_utf8(bytes).map(str::to_owned).map_err(|e| Some(e.valid_up_to())),
        Codec::Utf16Le => decode_with(encoding_rs::UTF_16LE, bytes),
        Codec::Utf16Be => decode_with(encoding_rs::UTF_16BE, bytes),
        Codec::Legacy(enc) => decode_with(enc, bytes),
    }
}

fn decode_with(enc: &'static Encoding, bytes: &[u8]) -> Result<String, Option<usize>> {
    enc.decode_without_bom_handling_and_without_replacement(bytes)
        .map(|c| c.into_owned())
        .ok_or(None)
}

/// 1-based line of byte offset `at`.
fn line_of(bytes: &[u8], at: usize) -> u32 {
    1 + bytes[..at.min(bytes.len())].iter().filter(|&&b| b == b'\n').count() as u32
}

/// Convert `bytes` from `opts.from` to `opts.to`. See the module docs for what is promised.
pub fn convert(bytes: &[u8], opts: &ConvertOptions) -> Result<Converted, ConvertError> {
    if looks_binary(bytes, opts.from) {
        return Err(ConvertError::Binary);
    }
    // The BOM belongs to the source codec, or it is not a BOM (an FF FE at the head of a Cp1252 file
    // is "ÿþ" and stays so).
    let own_bom = opts.from.bom_bytes();
    let bom_before = !own_bom.is_empty() && bytes.starts_with(own_bom);
    let body = if bom_before { &bytes[own_bom.len()..] } else { bytes };

    let mut text = decode_strict(body, opts.from).map_err(|offset| ConvertError::InvalidSource {
        line: offset.map(|o| line_of(body, o)),
    })?;

    let (declaration_before, declaration_rewritten) = if opts.rewrite_declaration {
        match rewrite_declaration(&text, opts.to.declaration_name()) {
            Some((found, new_text)) => {
                text = new_text;
                (Some(found), true)
            }
            None => (None, false),
        }
    } else {
        (find_declaration(&text).map(|d| d.value).filter(|v| !same_codec(v, opts.to)), false)
    };

    let (mut encoded, samples, total) = encode_text(&text, opts.to, opts.escape, opts.fallback);
    if opts.fallback == Fallback::Abort && total > 0 {
        return Err(ConvertError::Unrepresentable { total, samples });
    }

    let bom_after = opts.to.supports_bom()
        && match opts.bom {
            BomPolicy::Keep => bom_before,
            BomPolicy::Add => true,
            BomPolicy::Remove => false,
        };
    if bom_after {
        let mut with = opts.to.bom_bytes().to_vec();
        with.append(&mut encoded);
        encoded = with;
    }
    Ok(Converted {
        bytes: encoded,
        samples,
        unrepresentable_total: total,
        bom_before,
        bom_after,
        declaration_before,
        declaration_rewritten,
    })
}

fn same_codec(label: &str, codec: Codec) -> bool {
    Codec::parse(label) == Some(codec)
        // `UTF-16` in a declaration stands for either byte order.
        || (codec.is_utf16() && Codec::parse(label).is_some_and(Codec::is_utf16))
}

/// Encode `text`, applying the fallback to characters `to` cannot hold. Returns the bytes, the first
/// few offenders and the exact count. With [`Fallback::Abort`] the bytes are meaningless when the
/// count is not zero — the caller refuses the file.
fn encode_text(
    text: &str,
    to: Codec,
    escape: EscapeStyle,
    fallback: Fallback,
) -> (Vec<u8>, Vec<Unrepresentable>, usize) {
    match to {
        Codec::Utf8 => (text.as_bytes().to_vec(), Vec::new(), 0),
        Codec::Utf16Le | Codec::Utf16Be => {
            let mut out = Vec::with_capacity(text.len() * 2);
            for unit in text.encode_utf16() {
                let pair = if to == Codec::Utf16Le { unit.to_le_bytes() } else { unit.to_be_bytes() };
                out.extend_from_slice(&pair);
            }
            (out, Vec::new(), 0)
        }
        Codec::Latin1 | Codec::Legacy(_) => encode_single_pass(text, to, escape, fallback),
    }
}

/// The per-character encode for the ASCII-compatible targets — every one this module offers apart
/// from UTF-16 — which is what lets ASCII bypass the encoder and keeps line/column bookkeeping cheap.
fn encode_single_pass(
    text: &str,
    to: Codec,
    escape: EscapeStyle,
    fallback: Fallback,
) -> (Vec<u8>, Vec<Unrepresentable>, usize) {
    let mut out = Vec::with_capacity(text.len());
    let mut samples = Vec::new();
    let mut total = 0usize;
    let (mut line, mut column) = (1u32, 1u32);
    let mut scratch = [0u8; 4];
    for ch in text.chars() {
        if ch.is_ascii() {
            out.push(ch as u8);
        } else {
            match encode_char(ch, to, &mut scratch) {
                Some(bytes) => out.extend_from_slice(&bytes),
                None => {
                    total += 1;
                    if samples.len() < MAX_SAMPLES {
                        samples.push(Unrepresentable { line, column, ch });
                    }
                    push_fallback(&mut out, ch, fallback, escape);
                }
            }
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (out, samples, total)
}

fn encode_char(ch: char, to: Codec, scratch: &mut [u8; 4]) -> Option<Vec<u8>> {
    match to {
        Codec::Latin1 => (ch as u32 <= 0xFF).then(|| vec![ch as u32 as u8]),
        Codec::Legacy(enc) => {
            let s: &str = ch.encode_utf8(scratch);
            let (bytes, _, unmappable) = enc.encode(s);
            (!unmappable).then(|| bytes.into_owned())
        }
        _ => None,
    }
}

fn push_fallback(out: &mut Vec<u8>, ch: char, fallback: Fallback, escape: EscapeStyle) {
    if fallback != Fallback::Escape {
        out.push(b'?');
        return;
    }
    match escape {
        EscapeStyle::Java => {
            let mut units = [0u16; 2];
            for unit in ch.encode_utf16(&mut units) {
                out.extend_from_slice(format!("\\u{:04X}", unit).as_bytes());
            }
        }
        EscapeStyle::Xml => out.extend_from_slice(format!("&#x{:X};", ch as u32).as_bytes()),
        EscapeStyle::None => out.push(b'?'),
    }
}

// ── in-file encoding declarations ────────────────────────────────────────────────────────────

/// An encoding a file states about itself, and where the name sits in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub value: String,
    pub start: usize,
    pub end: usize,
}

/// The encoding a file declares about itself, in its first couple of kilobytes:
///
/// * the XML prolog — `<?xml version="1.0" encoding="ISO-8859-1"?>`;
/// * a JSP page directive — `pageEncoding="…"`, or the `charset=` of `contentType="…"`.
///
/// A file that says one thing and is stored as another is broken in a way the editor will not show
/// (it decodes by the project's rule, the XML parser by the file's), so a conversion has to know.
pub fn find_declaration(text: &str) -> Option<Declaration> {
    let mut end = text.len().min(DECLARATION_WINDOW);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let head = &text[..end];
    if head.trim_start().starts_with("<?xml") {
        let prolog_end = head.find("?>").unwrap_or(head.len());
        if let Some(found) = value_after(&head[..prolog_end], "encoding=") {
            return Some(offset(found, 0));
        }
    }
    let lower = head.to_ascii_lowercase();
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find("<%@") {
        let start = search_from + rel;
        let stop = lower[start..].find("%>").map_or(lower.len(), |e| start + e);
        let directive = &head[start..stop];
        // Keys are case-insensitive but values keep their spelling: search the lowered copy (ASCII
        // lowercasing keeps every byte offset) and read the value from the original.
        let lowered = directive.to_ascii_lowercase();
        if let Some(found) = value_after(&lowered, "pageencoding=") {
            let value = directive[found.start..found.end].to_string();
            return Some(offset(Declaration { value, ..found }, start));
        }
        if let Some(pos) = lowered.find("charset=") {
            let from = pos + "charset=".len();
            let len = lowered[from..]
                .find(|c: char| c == '"' || c == '\'' || c == ';' || c.is_whitespace() || c == '%')
                .unwrap_or(lowered.len() - from);
            if len > 0 {
                return Some(Declaration {
                    value: directive[from..from + len].to_string(),
                    start: start + from,
                    end: start + from + len,
                });
            }
        }
        search_from = stop.max(start + 3);
    }
    None
}

/// The quoted value after `key` (`encoding="X"`), with its span relative to `s`.
fn value_after(s: &str, key: &str) -> Option<Declaration> {
    let at = s.find(key)? + key.len();
    let quote = s[at..].chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let from = at + 1;
    let len = s[from..].find(quote)?;
    (len > 0).then(|| Declaration { value: s[from..from + len].to_string(), start: from, end: from + len })
}

fn offset(d: Declaration, by: usize) -> Declaration {
    Declaration { value: d.value, start: d.start + by, end: d.end + by }
}

/// `text` with its declared encoding replaced by `name`, plus what it declared before. `None` when
/// the file declares nothing, or already declares exactly `name`.
pub fn rewrite_declaration(text: &str, name: &str) -> Option<(String, String)> {
    let found = find_declaration(text)?;
    if found.value.eq_ignore_ascii_case(name) {
        return None;
    }
    let mut out = String::with_capacity(text.len() + name.len());
    out.push_str(&text[..found.start]);
    out.push_str(name);
    out.push_str(&text[found.end..]);
    Some((found.value, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cp1252() -> Codec {
        Codec::parse("Cp1252").unwrap()
    }

    fn opts(from: Codec, to: Codec) -> ConvertOptions {
        ConvertOptions {
            from,
            to,
            bom: BomPolicy::Keep,
            fallback: Fallback::Abort,
            escape: EscapeStyle::Java,
            rewrite_declaration: false,
        }
    }

    #[test]
    fn labels_resolve_to_one_canonical_codec() {
        assert_eq!(Codec::parse("UTF-8"), Some(Codec::Utf8));
        assert_eq!(Codec::parse("utf8"), Some(Codec::Utf8));
        assert_eq!(Codec::parse("Cp1252"), Codec::parse("windows-1252"));
        assert_eq!(Codec::parse("latin1"), Some(Codec::Latin1));
        assert_eq!(Codec::parse("ISO-8859-1"), Some(Codec::Latin1));
        assert_eq!(Codec::parse("UTF-16BE"), Some(Codec::Utf16Be));
        assert_eq!(Codec::parse("ISO-8859-15").unwrap().name(), "ISO-8859-15");
        assert_eq!(Codec::parse("no-such-charset"), None);
        assert_eq!(Codec::parse("ISO-2022-JP"), None);
        assert_eq!(Codec::parse("replacement"), None);
    }

    #[test]
    fn cp1252_to_utf8_and_back_is_lossless() {
        // à è € ’ in Cp1252.
        let src = [b'a', 0xE0, 0xE8, 0x80, 0x92, b'\r', b'\n'];
        let up = convert(&src, &opts(cp1252(), Codec::Utf8)).unwrap();
        assert_eq!(std::str::from_utf8(&up.bytes).unwrap(), "aàè€’\r\n");
        let down = convert(&up.bytes, &opts(Codec::Utf8, cp1252())).unwrap();
        assert_eq!(down.bytes, src);
    }

    #[test]
    fn line_endings_are_never_normalised() {
        let src = "a\r\nb\nc\rd".as_bytes();
        let out = convert(src, &opts(Codec::Utf8, Codec::Utf16Le)).unwrap();
        let back = convert(&out.bytes, &opts(Codec::Utf16Le, Codec::Utf8)).unwrap();
        assert_eq!(back.bytes, src);
    }

    #[test]
    fn latin1_is_not_windows_1252() {
        // € has no Latin-1 byte, though the WHATWG "iso-8859-1" label would happily give it 0x80.
        let err = convert("€".as_bytes(), &opts(Codec::Utf8, Codec::Latin1)).unwrap_err();
        assert!(matches!(err, ConvertError::Unrepresentable { total: 1, .. }));
        // And a Latin-1 byte in the C1 range is the control character, not a Cp1252 glyph.
        let up = convert(&[0x80], &opts(Codec::Latin1, Codec::Utf8)).unwrap();
        assert_eq!(up.bytes, "\u{80}".as_bytes());
    }

    #[test]
    fn abort_reports_every_unrepresentable_char_with_a_position() {
        let err = convert("ok\nx☃y€z".as_bytes(), &opts(Codec::Utf8, Codec::Latin1)).unwrap_err();
        match err {
            ConvertError::Unrepresentable { total, samples } => {
                assert_eq!(total, 2);
                assert_eq!(samples[0], Unrepresentable { line: 2, column: 2, ch: '☃' });
                assert_eq!(samples[1], Unrepresentable { line: 2, column: 4, ch: '€' });
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn replace_writes_a_question_mark_and_counts() {
        let mut o = opts(Codec::Utf8, cp1252());
        o.fallback = Fallback::Replace;
        let out = convert("a€☃".as_bytes(), &o).unwrap();
        assert_eq!(out.bytes, vec![b'a', 0x80, b'?']);
        assert_eq!(out.unrepresentable_total, 1);
        assert_eq!(out.samples[0].ch, '☃');
    }

    #[test]
    fn escape_follows_the_files_language() {
        let mut o = opts(Codec::Utf8, cp1252());
        o.fallback = Fallback::Escape;
        assert_eq!(convert("☃".as_bytes(), &o).unwrap().bytes, b"\\u2603");
        o.escape = EscapeStyle::Xml;
        assert_eq!(convert("☃".as_bytes(), &o).unwrap().bytes, b"&#x2603;");
        o.escape = EscapeStyle::None;
        assert_eq!(convert("☃".as_bytes(), &o).unwrap().bytes, b"?");
        // A supplementary-plane char is a surrogate pair in a Java escape.
        o.escape = EscapeStyle::Java;
        assert_eq!(convert("😀".as_bytes(), &o).unwrap().bytes, b"\\uD83D\\uDE00");
    }

    #[test]
    fn extension_picks_the_escape_style() {
        assert_eq!(EscapeStyle::for_extension("java"), EscapeStyle::Java);
        assert_eq!(EscapeStyle::for_extension("JSP"), EscapeStyle::Xml);
        assert_eq!(EscapeStyle::for_extension("sql"), EscapeStyle::None);
    }

    #[test]
    fn bom_is_kept_added_or_removed_on_request() {
        let with = [&[0xEF_u8, 0xBB, 0xBF][..], &b"hi"[..]].concat();
        let mut o = opts(Codec::Utf8, Codec::Utf8);
        let kept = convert(&with, &o).unwrap();
        assert!(kept.bom_before && kept.bom_after);
        assert_eq!(kept.bytes, with);

        o.bom = BomPolicy::Remove;
        let removed = convert(&with, &o).unwrap();
        assert!(removed.bom_before && !removed.bom_after);
        assert_eq!(removed.bytes, b"hi");

        o.bom = BomPolicy::Add;
        let added = convert(b"hi", &o).unwrap();
        assert!(!added.bom_before && added.bom_after);
        assert_eq!(added.bytes, with);
    }

    #[test]
    fn a_bom_is_only_written_for_encodings_that_have_one() {
        let mut o = opts(Codec::Utf8, cp1252());
        o.bom = BomPolicy::Add;
        let out = convert(b"hi", &o).unwrap();
        assert!(!out.bom_after);
        assert_eq!(out.bytes, b"hi");
    }

    #[test]
    fn a_utf8_bom_in_a_cp1252_file_is_three_characters_not_a_bom() {
        let out = convert(&[0xEF, 0xBB, 0xBF], &opts(cp1252(), Codec::Utf8)).unwrap();
        assert!(!out.bom_before);
        assert_eq!(std::str::from_utf8(&out.bytes).unwrap(), "ï»¿");
    }

    #[test]
    fn utf16_round_trips_with_its_bom() {
        let mut o = opts(Codec::Utf8, Codec::Utf16Le);
        o.bom = BomPolicy::Add;
        let out = convert("aé€".as_bytes(), &o).unwrap();
        assert_eq!(out.bytes, vec![0xFF, 0xFE, b'a', 0, 0xE9, 0, 0xAC, 0x20]);
        let back = convert(&out.bytes, &opts(Codec::Utf16Le, Codec::Utf8)).unwrap();
        // `Keep`: the source had a BOM, so the UTF-8 output carries one too.
        assert!(back.bom_before && back.bom_after);
        assert_eq!(back.bytes, [&[0xEF_u8, 0xBB, 0xBF][..], "aé€".as_bytes()].concat());
    }

    #[test]
    fn utf16_is_not_mistaken_for_binary() {
        let utf16 = [b'a', 0, b'b', 0];
        assert!(!looks_binary(&utf16, Codec::Utf16Le));
        assert!(looks_binary(&utf16, Codec::Utf8));
        assert_eq!(convert(&utf16, &opts(Codec::Utf8, Codec::Utf16Le)).unwrap_err(), ConvertError::Binary);
    }

    #[test]
    fn a_malformed_source_is_refused_with_its_line() {
        let err = convert(b"ok\nbad \xE8 byte", &opts(Codec::Utf8, cp1252())).unwrap_err();
        assert_eq!(err, ConvertError::InvalidSource { line: Some(2) });
    }

    #[test]
    fn identical_bytes_come_back_identical() {
        let ascii = b"class A {}\r\n";
        assert_eq!(convert(ascii, &opts(cp1252(), Codec::Utf8)).unwrap().bytes, ascii);
    }

    #[test]
    fn detection_reads_bom_ascii_utf8_then_guesses_legacy() {
        assert_eq!(detect(&[0xFF, 0xFE, b'a', 0]).basis, DetectBasis::Bom);
        assert_eq!(detect(b"plain").basis, DetectBasis::Ascii);
        assert_eq!(detect("città".as_bytes()).basis, DetectBasis::ValidUtf8);
        let legacy = detect(&[b'c', b'i', b't', b't', 0xE0]);
        assert_eq!(legacy.basis, DetectBasis::LegacyGuess);
        assert_eq!(legacy.codec.name(), "windows-1252");
    }

    #[test]
    fn xml_prolog_is_found_and_rewritten() {
        let xml = "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<a>è</a>";
        let d = find_declaration(xml).unwrap();
        assert_eq!(d.value, "ISO-8859-1");
        let (was, out) = rewrite_declaration(xml, "UTF-8").unwrap();
        assert_eq!(was, "ISO-8859-1");
        assert_eq!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<a>è</a>");
        assert!(rewrite_declaration(&out, "utf-8").is_none());
    }

    #[test]
    fn jsp_directives_are_found() {
        let jsp = "<%@ page language=\"java\" contentType=\"text/html; charset=ISO-8859-1\" %>\nx";
        assert_eq!(find_declaration(jsp).unwrap().value, "ISO-8859-1");
        let jsp2 = "<%@ page pageEncoding=\"Cp1252\" %>";
        assert_eq!(find_declaration(jsp2).unwrap().value, "Cp1252");
        let (_, out) = rewrite_declaration(jsp, "UTF-8").unwrap();
        assert!(out.contains("charset=UTF-8"));
    }

    #[test]
    fn a_file_without_a_declaration_has_none() {
        assert!(find_declaration("class A {}").is_none());
        assert!(find_declaration("<root/>").is_none());
    }

    #[test]
    fn conversion_rewrites_the_declaration_when_asked() {
        let xml = "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a>\u{E8}</a>";
        let src: Vec<u8> = xml.chars().map(|c| c as u32 as u8).collect();
        let mut o = opts(Codec::Latin1, Codec::Utf8);
        o.rewrite_declaration = true;
        let out = convert(&src, &o).unwrap();
        assert!(out.declaration_rewritten);
        assert_eq!(out.declaration_before.as_deref(), Some("ISO-8859-1"));
        assert!(std::str::from_utf8(&out.bytes).unwrap().contains("encoding=\"UTF-8\""));

        // Not asked: reported as stale, left alone.
        o.rewrite_declaration = false;
        let kept = convert(&src, &o).unwrap();
        assert!(!kept.declaration_rewritten);
        assert_eq!(kept.declaration_before.as_deref(), Some("ISO-8859-1"));
    }

    #[test]
    fn java_names_follow_what_a_build_file_expects() {
        assert_eq!(Codec::parse("macintosh").unwrap().java_name(), "x-MacRoman");
        assert_eq!(cp1252().java_name(), "windows-1252");
        assert_eq!(Codec::Utf16Be.declaration_name(), "UTF-16");
    }
}
