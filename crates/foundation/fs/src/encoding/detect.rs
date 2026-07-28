//! Encoding detection that reports **how** it decided, not just what.
//!
//! A silent guess is the whole problem: a windows-1252 script rewritten as
//! UTF-8 by an outside editor looks fine to a naïve detector, and the wrong
//! bytes reach the database. So detection here returns a [`Detection`] whose
//! [`EncodingSource`] says which rung of the chain produced the answer, and
//! the ambiguous rung is resolved by an explicit [`EncodingContext`] the
//! caller built — never by an implicit assumption buried in this file.

use encoding_rs::Encoding;

use super::context::EncodingContext;

/// The single-byte encoding assumed when the bytes are not UTF-8 and the
/// caller did not say otherwise. Legacy SQL/Java/`.properties` repositories on
/// Windows European systems are CP1252, and CP1252 maps every byte to a
/// distinct codepoint, so choosing it is at worst mislabelled — never lossy.
pub const DEFAULT_LEGACY_ENCODING: &Encoding = encoding_rs::WINDOWS_1252;

/// How a file's encoding was decided. Mirrors the frontend `EncodingSource`
/// union one-for-one; [`EncodingSource::as_str`] is the wire word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EncodingSource {
    /// A byte-order mark declared it. The only self-describing case.
    Bom,
    /// Valid UTF-8 carrying at least one multibyte sequence — positive
    /// evidence, because no legacy single-byte file would be valid UTF-8
    /// by accident at any length.
    Utf8,
    /// The bytes were ambiguous (pure ASCII) and the encoding came from the
    /// folder's dominant vote.
    Inherited,
    /// A guess: either "not UTF-8, so the legacy single-byte encoding", or
    /// "ambiguous with nothing to inherit from".
    Heuristic,
    /// The user (or a config file) pinned it, overriding detection.
    Forced,
}

impl EncodingSource {
    /// The wire word shared with the frontend union.
    pub fn as_str(self) -> &'static str {
        match self {
            EncodingSource::Bom       => "bom",
            EncodingSource::Utf8      => "utf8",
            EncodingSource::Inherited => "inherited",
            EncodingSource::Heuristic => "heuristic",
            EncodingSource::Forced    => "forced",
        }
    }

    /// `true` when the answer is a guess rather than evidence — the UI can
    /// mark these files so the user knows the label is not proven.
    pub fn is_guess(self) -> bool {
        matches!(self, EncodingSource::Inherited | EncodingSource::Heuristic)
    }
}

impl std::fmt::Display for EncodingSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// Serialised through `as_str` rather than a derive so the wire words and the
// Rust-side accessor can never drift apart.
impl serde::Serialize for EncodingSource {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

/// What the bytes **alone** can prove, before any context is applied.
///
/// This is the pure half of the chain: same bytes, same verdict, always.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingEvidence {
    /// A BOM declared the encoding outright.
    Bom(&'static Encoding),
    /// Valid UTF-8 with at least one multibyte sequence.
    Utf8Multibyte,
    /// Every byte is < 0x80. Genuinely undecidable: ASCII is valid UTF-8 *and*
    /// valid windows-1252 *and* valid ISO-8859-x, byte for byte.
    Ascii,
    /// Not valid UTF-8, so some single-byte legacy encoding. *Which* one the
    /// bytes cannot say — that is the context's call.
    SingleByte,
}

impl EncodingEvidence {
    /// `true` for the pure-ASCII case, the only one the bytes cannot settle.
    pub fn is_ambiguous(self) -> bool {
        matches!(self, EncodingEvidence::Ascii)
    }
}

/// Classify a buffer using only the bytes. See [`EncodingEvidence`].
///
/// Give it the whole file, or a prefix cut on a character boundary: a prefix
/// that slices a multibyte sequence in half reads as `SingleByte`.
pub fn evidence(bytes: &[u8]) -> EncodingEvidence {
    if let Some((enc, _)) = Encoding::for_bom(bytes) {
        return EncodingEvidence::Bom(enc);
    }
    // Order matters: ASCII is a *subset* of valid UTF-8, so the multibyte
    // check is what separates evidence from coincidence.
    if std::str::from_utf8(bytes).is_ok() {
        if bytes.iter().any(|b| *b >= 0x80) {
            return EncodingEvidence::Utf8Multibyte;
        }
        return EncodingEvidence::Ascii;
    }
    EncodingEvidence::SingleByte
}

/// An encoding decision plus its provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detection {
    pub encoding: &'static Encoding,
    pub source:   EncodingSource,
    /// The file physically starts with a BOM. Needed to round-trip: the BOM is
    /// stripped from the decoded text, so a save has to put it back.
    pub had_bom:  bool,
}

impl Detection {
    /// The canonical encoding label (`"UTF-8"`, `"windows-1252"`, …) — what
    /// goes on the wire and into the UI pill.
    pub fn label(&self) -> &'static str {
        self.encoding.name()
    }

    /// A decision the user or a config pinned, bypassing the chain.
    pub fn forced(encoding: &'static Encoding, had_bom: bool) -> Self {
        Detection { encoding, source: EncodingSource::Forced, had_bom }
    }
}

/// Run the full chain, resolving the ambiguous rung through `ctx`.
///
/// 1. BOM → [`EncodingSource::Bom`].
/// 2. UTF-8 with a multibyte sequence → [`EncodingSource::Utf8`].
/// 3. Pure ASCII → the context's dominant encoding
///    ([`EncodingSource::Inherited`]), or its legacy encoding when the folder
///    voted for nothing at all ([`EncodingSource::Heuristic`]).
/// 4. Otherwise → the context's legacy encoding ([`EncodingSource::Heuristic`]).
pub fn detect_in_context(bytes: &[u8], ctx: &EncodingContext) -> Detection {
    match evidence(bytes) {
        EncodingEvidence::Bom(enc) => Detection {
            encoding: enc,
            source:   EncodingSource::Bom,
            had_bom:  true,
        },
        EncodingEvidence::Utf8Multibyte => Detection {
            encoding: encoding_rs::UTF_8,
            source:   EncodingSource::Utf8,
            had_bom:  false,
        },
        EncodingEvidence::Ascii => match ctx.dominant() {
            Some(enc) => Detection {
                encoding: enc,
                source:   EncodingSource::Inherited,
                had_bom:  false,
            },
            None => Detection {
                encoding: ctx.legacy(),
                source:   EncodingSource::Heuristic,
                had_bom:  false,
            },
        },
        EncodingEvidence::SingleByte => Detection {
            encoding: ctx.legacy(),
            source:   EncodingSource::Heuristic,
            had_bom:  false,
        },
    }
}

/// Detect + decode in one pass, **removing** the BOM from the returned text.
///
/// Deliberately different from the frozen [`super::decode_bytes_full`], which
/// leaves the BOM in the string as a leading U+FEFF: a U+FEFF that survives
/// into an editor buffer eventually gets written into the middle of a file, or
/// counted as a character by a linter. Round-tripping is still lossless
/// because [`Detection::had_bom`] remembers it was there.
pub fn decode_in_context(bytes: &[u8], ctx: &EncodingContext) -> (String, Detection) {
    let detection = detect_in_context(bytes, ctx);
    let (cow, _) = detection.encoding.decode_with_bom_removal(bytes);
    (cow.into_owned(), detection)
}
