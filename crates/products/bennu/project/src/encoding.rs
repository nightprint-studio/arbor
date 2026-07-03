//! Encoding detection (docs §5 #21, §10).
//!
//! The reference project is **`Cp1252`**, declared in the pom — so reading in the
//! declared encoding is critical, and UTF-8 is only the *fallback* (docs §0). The
//! resolution order:
//!
//! 1. an explicit per-path **override** (IntelliJ-style footer "reload in X"),
//! 2. the pom `project.build.sourceEncoding`,
//! 3. the configured default (`"UTF-8"`).
//!
//! Decoding runs through `encoding_rs` (the WHATWG encoding set), so the declared label —
//! `UTF-8`, `Cp1252`, `ISO-8859-1`, `ISO-8859-15`, … — is honoured natively; an
//! unrecognised label falls back to UTF-8 with the declared label preserved for the FE.
//! The round-trip *save* (`encode`) stays hand-rolled for Cp1252 / UTF-8: its
//! unmappable-char fallback (whole-file UTF-8, so nothing is corrupted) differs from
//! `encoding_rs`' encoder (which emits numeric character references), so we don't route
//! writes through it.

use std::path::Path;

use crate::pom::Pom;

/// Resolve the encoding *label* Bennu will decode a project's files in: the pom's
/// `project.build.sourceEncoding`, else `default_label`.
pub fn project_encoding_label(pom: &Pom, default_label: &str) -> String {
    pom.property("project.build.sourceEncoding")
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default_label.to_string())
}

/// Resolve the project source encoding label straight from `root/pom.xml`
/// (`project.build.sourceEncoding`), else `default_label`. A convenience for callers that
/// only hold the project root (the index build / bulk scans), not a parsed pom. A missing
/// or unreadable pom yields `default_label`.
pub fn source_encoding_label(root: &Path, default_label: &str) -> String {
    match std::fs::read_to_string(root.join("pom.xml")) {
        Ok(xml) => project_encoding_label(&crate::pom::parse(&xml), default_label),
        Err(_) => default_label.to_string(),
    }
}

/// Look up an `encoding_rs::Encoding` for a WHATWG `label` (case/space/`-`/`_`-insensitive:
/// `UTF-8`, `Cp1252`, `windows-1252`, `ISO-8859-1`, `ISO-8859-15`, …), defaulting to UTF-8
/// for an empty or unrecognised label.
fn encoding_for(label: &str) -> &'static encoding_rs::Encoding {
    encoding_rs::Encoding::for_label(label.trim().as_bytes()).unwrap_or(encoding_rs::UTF_8)
}

/// Decode `bytes` using `label` (through `encoding_rs`). Returns the decoded text plus the
/// label that actually applied. `UTF-8` strips a BOM; `Cp1252` / `ISO-8859-1` / … decode
/// natively; an unrecognised label falls back to UTF-8 while preserving the declared label
/// (so the FE still shows e.g. `ISO-8859-1`). Lossy: invalid bytes become U+FFFD, so a
/// mislabelled file never hard-fails the open.
pub fn decode(bytes: &[u8], label: &str) -> (String, String) {
    let (text, _, _had_errors) = encoding_for(label).decode(bytes);
    (text.into_owned(), label.to_string())
}

/// Outcome of an INDEXING decode ([`decode_for_index`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDecode {
    /// The decoded text (always usable — the declared encoding when it fit, else a recovery
    /// decode).
    pub text: String,
    /// The encoding label that produced `text`.
    pub encoding: String,
    /// True when the file's bytes were NOT valid in the project's *declared* encoding — a
    /// non-compliant file (recovered here so its classes are still indexed, but flagged for
    /// the "non-compliant files" report).
    pub non_compliant: bool,
}

/// Decode source `bytes` for INDEXING, trying the project's declared (Maven) encoding first
/// and recovering with `encoding_rs` when the bytes don't fit it.
///
/// 1. Decode with `declared_label`. If it produced no replacement characters, the file is
///    compliant — return its text.
/// 2. Otherwise the file isn't valid in its declared encoding: recover so the class is
///    still indexed and flag it non-compliant. Prefer UTF-8 when it decodes cleanly (a
///    UTF-8 file mislabelled Cp1252 is the common case), else Windows-1252 (which maps every
///    byte, so it never fails). ASCII structure survives every path — the point is to never
///    silently drop a class.
pub fn decode_for_index(bytes: &[u8], declared_label: &str) -> IndexDecode {
    let (text, _, had_errors) = encoding_for(declared_label).decode(bytes);
    if !had_errors {
        return IndexDecode { text: text.into_owned(), encoding: declared_label.to_string(), non_compliant: false };
    }
    let (utf8, _, utf8_err) = encoding_rs::UTF_8.decode(bytes);
    if !utf8_err {
        return IndexDecode { text: utf8.into_owned(), encoding: "UTF-8".to_string(), non_compliant: true };
    }
    let (w1252, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
    IndexDecode { text: w1252.into_owned(), encoding: "windows-1252".to_string(), non_compliant: true }
}

/// Encode `text` using `label`, the round-trip inverse of [`decode`]. Returns the
/// encoded bytes plus the label that actually applied. Native paths: `Cp1252` /
/// `Windows-1252` (each char mapped back to its single byte) and `UTF-8` (the default).
/// If a char can't be represented in the requested encoding (a non-Cp1252 char under a
/// Cp1252 label), the whole write falls back to UTF-8 — the returned label reflects the
/// encoding actually used, so the caller reports the truth. Any label other than the
/// Cp1252 aliases encodes as UTF-8 while preserving the declared label.
pub fn encode(text: &str, label: &str) -> (Vec<u8>, String) {
    let norm = label.to_ascii_lowercase().replace(['-', '_'], "");
    match norm.as_str() {
        "cp1252" | "windows1252" | "1252" => match encode_cp1252(text) {
            Some(bytes) => (bytes, label.to_string()),
            // A char outside Cp1252 → don't corrupt it; fall back to UTF-8, report UTF-8.
            None => (text.as_bytes().to_vec(), "UTF-8".to_string()),
        },
        // UTF-8 (default) + anything unrecognised → UTF-8, true label kept.
        _ => (text.as_bytes().to_vec(), label.to_string()),
    }
}

/// Encode `text` as Windows-1252, or `None` if any char has no Cp1252 byte. Pure
/// table lookup — the inverse of the Cp1252 decode (no dependency, hard rule 7).
fn encode_cp1252(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        out.push(cp1252_byte(ch)?);
    }
    Some(out)
}

/// Map one Unicode scalar back to its Cp1252 byte (the inverse of the Cp1252 decode).
/// `None` for a scalar that Cp1252 can't represent.
fn cp1252_byte(ch: char) -> Option<u8> {
    match ch {
        '\u{20AC}' => Some(0x80),
        '\u{201A}' => Some(0x82),
        '\u{0192}' => Some(0x83),
        '\u{201E}' => Some(0x84),
        '\u{2026}' => Some(0x85),
        '\u{2020}' => Some(0x86),
        '\u{2021}' => Some(0x87),
        '\u{02C6}' => Some(0x88),
        '\u{2030}' => Some(0x89),
        '\u{0160}' => Some(0x8A),
        '\u{2039}' => Some(0x8B),
        '\u{0152}' => Some(0x8C),
        '\u{017D}' => Some(0x8E),
        '\u{2018}' => Some(0x91),
        '\u{2019}' => Some(0x92),
        '\u{201C}' => Some(0x93),
        '\u{201D}' => Some(0x94),
        '\u{2022}' => Some(0x95),
        '\u{2013}' => Some(0x96),
        '\u{2014}' => Some(0x97),
        '\u{02DC}' => Some(0x98),
        '\u{2122}' => Some(0x99),
        '\u{0161}' => Some(0x9A),
        '\u{203A}' => Some(0x9B),
        '\u{0153}' => Some(0x9C),
        '\u{017E}' => Some(0x9E),
        '\u{0178}' => Some(0x9F),
        // ASCII (0x00–0x7F) and the Latin-1 high range (0xA0–0xFF) map identity, minus
        // the five undefined Cp1252 slots (0x81/0x8D/0x8F/0x90/0x9D) which decode as the
        // raw code point and so must round-trip back to that same byte.
        c if (c as u32) <= 0x7F => Some(c as u8),
        c @ ('\u{81}' | '\u{8D}' | '\u{8F}' | '\u{90}' | '\u{9D}') => Some(c as u8),
        c if (0xA0..=0xFF).contains(&(c as u32)) => Some(c as u8),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pom;

    #[test]
    fn label_comes_from_pom_then_default() {
        let with = pom::parse(
            "<project><properties><project.build.sourceEncoding>Cp1252\
             </project.build.sourceEncoding></properties></project>",
        );
        assert_eq!(project_encoding_label(&with, "UTF-8"), "Cp1252");

        let without = pom::parse("<project></project>");
        assert_eq!(project_encoding_label(&without, "UTF-8"), "UTF-8");
    }

    #[test]
    fn decodes_cp1252_high_bytes() {
        // 0x80 = € , 0xE0 = à (Latin-1), 0x92 = ’ (right single quote)
        let (text, label) = decode(&[0x80, 0xE0, 0x92], "Cp1252");
        assert_eq!(text, "€à’");
        assert_eq!(label, "Cp1252");
    }

    #[test]
    fn decodes_utf8_and_strips_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("città".as_bytes());
        let (text, label) = decode(&bytes, "UTF-8");
        assert_eq!(text, "città");
        assert_eq!(label, "UTF-8");
    }

    #[test]
    fn index_decode_recovers_and_flags_non_compliant() {
        // Declared Cp1252 with valid Cp1252 bytes (0x80 = €) → compliant, decoded natively.
        let ok = decode_for_index(&[0x80], "Cp1252");
        assert!(!ok.non_compliant);
        assert_eq!(ok.text, "€");
        assert_eq!(ok.encoding, "Cp1252");

        // Declared UTF-8 but a lone 0xE0 (invalid UTF-8, a Latin-1 'à') → the declared
        // encoding doesn't fit, so it's flagged non-compliant and recovered (not dropped).
        let bad = decode_for_index(&[b'x', 0xE0], "UTF-8");
        assert!(bad.non_compliant);
        assert!(bad.text.starts_with('x'));
    }

    #[test]
    fn encode_is_inverse_of_decode_cp1252() {
        // The exact bytes from `decodes_cp1252_high_bytes`, re-encoded, round-trip.
        let (text, _) = decode(&[0x80, 0xE0, 0x92], "Cp1252");
        let (bytes, label) = encode(&text, "Cp1252");
        assert_eq!(bytes, vec![0x80, 0xE0, 0x92]);
        assert_eq!(label, "Cp1252");
    }

    #[test]
    fn encode_falls_back_to_utf8_when_cp1252_cant_represent() {
        // '€' is Cp1252 but '☃' (U+2603) is not → the write falls back to UTF-8.
        let (bytes, label) = encode("€☃", "Cp1252");
        assert_eq!(bytes, "€☃".as_bytes());
        assert_eq!(label, "UTF-8");
    }

    #[test]
    fn encode_utf8_ascii_roundtrips() {
        let (bytes, label) = encode("class Foo {}", "UTF-8");
        assert_eq!(bytes, b"class Foo {}");
        assert_eq!(label, "UTF-8");
    }
}
